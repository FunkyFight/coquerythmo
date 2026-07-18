//! Orchestration for the persisted, multilingual "Export…" configuration.

use crate::delivery_export::{
    self, AudioDeliveryFormat, AudioExportOptions, AudioTrackKind, SubtitleFormat,
};
use crate::project::{
    AudioSelection, ExportConfiguration, Project, VideoExportAspect, VideoExportQuality,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

pub struct ConfiguredExportContext<'a> {
    pub project: &'a Project,
    pub source_video: Option<&'a Path>,
    pub output_base: &'a Path,
    pub source_fps: f64,
    pub source_total_frames: i64,
    pub source_size: (u32, u32),
    pub configuration: &'a ExportConfiguration,
    pub render_backend_status: Option<Arc<AtomicU32>>,
    pub progress: Arc<AtomicU32>,
    pub cancel: Arc<AtomicBool>,
}

pub fn run(context: ConfiguredExportContext<'_>) -> Result<Vec<PathBuf>, String> {
    if context.configuration.selected_language_ids.is_empty() {
        return Err("No export language selected".into());
    }
    let parent = context
        .output_base
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Cannot create export directory {}: {error}",
            parent.display()
        )
    })?;
    let stem = context
        .output_base
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("export");

    let selected: Vec<_> = context
        .configuration
        .selected_language_ids
        .iter()
        .filter_map(|id| {
            let language = context.project.language(*id)?;
            let project = context.project.project_for_language(*id)?;
            Some((language, project))
        })
        .collect();
    if selected.is_empty() {
        return Err("Selected export languages no longer exist".into());
    }

    let task_count: usize = selected
        .iter()
        .map(|(language, _)| {
            let tracks = selected_audio_tracks(
                context.configuration,
                language.id,
                context
                    .project
                    .language_instrumental_audio_path(language.id)
                    .is_some(),
            )
            .len();
            let cfg = context.configuration;
            (cfg.video_enabled as usize) * tracks
                + bool_count([
                    cfg.subtitle_formats.json,
                    cfg.subtitle_formats.srt,
                    cfg.subtitle_formats.ass,
                    cfg.subtitle_formats.detx,
                    cfg.cross_reference_formats.csv,
                    cfg.cross_reference_formats.pdf,
                    cfg.presence_grid_pdf,
                ])
                + tracks
                    * bool_count([
                        cfg.audio_formats.mp3,
                        cfg.audio_formats.wav,
                        cfg.audio_formats.bwf_stems,
                    ])
        })
        .sum();
    if task_count == 0 {
        return Err("No compatible export output selected".into());
    }

    let mut completed = 0usize;
    let mut outputs = Vec::with_capacity(task_count);
    let mut used_prefixes = HashSet::new();
    for (language, project) in selected {
        check_cancel(&context.cancel)?;
        let language_slug = safe_filename(if language.code.trim().is_empty() {
            &language.name
        } else {
            &language.code
        });
        let base_prefix = format!("{}_{}", safe_filename(stem), language_slug);
        let mut prefix = base_prefix.clone();
        if !used_prefixes.insert(prefix.to_ascii_lowercase()) {
            prefix = format!("{base_prefix}_{}", language.id);
            used_prefixes.insert(prefix.to_ascii_lowercase());
        }
        let audio_selection = context
            .configuration
            .audio_by_language
            .get(&language.id)
            .copied()
            .unwrap_or_else(AudioSelection::default);
        let instrumental_path = project
            .settings()
            .instrumental_audio_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from);
        let tracks = selected_audio_tracks(
            context.configuration,
            language.id,
            instrumental_path.is_some(),
        );

        if context.configuration.video_enabled {
            let source_video = context
                .source_video
                .ok_or_else(|| "Video export requires a source video".to_string())?;
            let (width, height) = resolve_video_dimensions(
                context.configuration,
                context.source_size.0,
                context.source_size.1,
            );
            for track in &tracks {
                let output = parent.join(format!("{prefix}_{}.mp4", track.label()));
                let instrumental = match track.kind {
                    AudioTrackKind::Original => None,
                    AudioTrackKind::Instrumental => {
                        Some(instrumental_path.as_deref().ok_or_else(|| {
                            format!("No instrumental audio for language {}", language.name)
                        })?)
                    }
                };
                let mapped_progress =
                    mapped_progress(context.progress.clone(), completed, task_count);
                crate::video_export::export_mp4(
                    &project,
                    source_video,
                    &output,
                    context.configuration.fps,
                    context.source_fps,
                    context.configuration.br_scale,
                    context.configuration.karaoke_text_scale,
                    width,
                    height,
                    instrumental,
                    project.settings().source_audio_offset_frames,
                    project.settings().instrumental_audio_offset_frames,
                    track.with_announcer,
                    false,
                    context.configuration.pre_roll_seconds,
                    context.render_backend_status.clone(),
                    context.cancel.clone(),
                    mapped_progress,
                )
                .map_err(|error| {
                    format!("Video / {} / {}: {error}", language.name, track.label())
                })?;
                if context.configuration.countdown_enabled
                    && context.configuration.countdown_start > 0
                {
                    crate::video_export::preroll::prepend_countdown(
                        &output,
                        context.configuration.countdown_start,
                        &context.cancel,
                    )
                    .map_err(|error| format!("Countdown / {}: {error}", language.name))?;
                }
                outputs.push(output);
                finish_task(&context.progress, &mut completed, task_count);
            }
        }

        for (enabled, format) in [
            (
                context.configuration.subtitle_formats.json,
                SubtitleFormat::Json,
            ),
            (
                context.configuration.subtitle_formats.srt,
                SubtitleFormat::Srt,
            ),
            (
                context.configuration.subtitle_formats.ass,
                SubtitleFormat::Ass,
            ),
            (
                context.configuration.subtitle_formats.detx,
                SubtitleFormat::Detx,
            ),
        ] {
            if !enabled {
                continue;
            }
            check_cancel(&context.cancel)?;
            let output = parent.join(format!("{prefix}.{}", format.extension()));
            delivery_export::export_subtitle(
                &project,
                context.source_fps,
                &output,
                &language.name,
                format,
            )
            .map_err(|error| format!("{} / {}: {error}", format.extension(), language.name))?;
            outputs.push(output);
            finish_task(&context.progress, &mut completed, task_count);
        }

        for track in &tracks {
            let input = match track.kind {
                AudioTrackKind::Original => context
                    .source_video
                    .ok_or_else(|| "Original audio export requires a source video".to_string())?,
                AudioTrackKind::Instrumental => instrumental_path.as_deref().ok_or_else(|| {
                    format!("No instrumental audio for language {}", language.name)
                })?,
            };
            for (enabled, format, suffix) in [
                (
                    context.configuration.audio_formats.mp3,
                    AudioDeliveryFormat::Mp3,
                    "mp3",
                ),
                (
                    context.configuration.audio_formats.wav,
                    AudioDeliveryFormat::Wav,
                    "wav",
                ),
                (
                    context.configuration.audio_formats.bwf_stems,
                    AudioDeliveryFormat::Bwf,
                    "bwf",
                ),
            ] {
                if !enabled {
                    continue;
                }
                check_cancel(&context.cancel)?;
                let output = parent.join(format!(
                    "{prefix}_{}_{}.{}",
                    track.label(),
                    suffix,
                    format.extension()
                ));
                let announcer_audio = if track.with_announcer {
                    crate::video_export::announcer::synthesize(
                        &project,
                        context.source_fps,
                        0.0,
                        &output,
                    )?
                } else {
                    None
                };
                let options = AudioExportOptions {
                    format,
                    track: track.kind,
                    language_name: &language.name,
                    stem_name: track.label(),
                    duration_frames: (context.source_total_frames > 0)
                        .then_some(context.source_total_frames),
                    announcer_audio: announcer_audio.as_deref(),
                };
                delivery_export::export_audio(
                    &project,
                    context.source_fps,
                    input,
                    &output,
                    &options,
                    &context.cancel,
                )
                .map_err(|error| {
                    format!(
                        "Audio {} / {} / {}: {error}",
                        format.extension(),
                        language.name,
                        track.label()
                    )
                })?;
                if let Some(announcer) = announcer_audio {
                    let _ = std::fs::remove_file(announcer);
                }
                outputs.push(output);
                finish_task(&context.progress, &mut completed, task_count);
            }
        }

        if context.configuration.cross_reference_formats.csv {
            let output = parent.join(format!("{prefix}_cross_reference.csv"));
            delivery_export::export_cross_reference_csv(
                &project,
                context.source_fps,
                &output,
                &language.name,
            )?;
            outputs.push(output);
            finish_task(&context.progress, &mut completed, task_count);
        }
        if context.configuration.cross_reference_formats.pdf {
            let output = parent.join(format!("{prefix}_cross_reference.pdf"));
            delivery_export::export_cross_reference_pdf(
                &project,
                context.source_fps,
                &output,
                &language.name,
            )?;
            outputs.push(output);
            finish_task(&context.progress, &mut completed, task_count);
        }
        if context.configuration.presence_grid_pdf {
            let output = parent.join(format!("{prefix}_presence_grid.pdf"));
            delivery_export::export_presence_grid_pdf(
                &project,
                context.source_fps,
                &output,
                &language.name,
            )?;
            outputs.push(output);
            finish_task(&context.progress, &mut completed, task_count);
        }

        let _ = audio_selection;
    }

    context.progress.store(1.0_f32.to_bits(), Ordering::Relaxed);
    Ok(outputs)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectedAudioTrack {
    kind: AudioTrackKind,
    with_announcer: bool,
}

impl SelectedAudioTrack {
    fn label(self) -> &'static str {
        match (self.kind, self.with_announcer) {
            (AudioTrackKind::Original, false) => "original",
            (AudioTrackKind::Instrumental, false) => "instrumental",
            (AudioTrackKind::Original, true) => "original_announcer",
            (AudioTrackKind::Instrumental, true) => "instrumental_announcer",
        }
    }
}

fn selected_audio_tracks(
    configuration: &ExportConfiguration,
    language_id: u64,
    has_instrumental: bool,
) -> Vec<SelectedAudioTrack> {
    let selection = configuration
        .audio_by_language
        .get(&language_id)
        .copied()
        .unwrap_or_else(AudioSelection::default);
    let mut tracks = Vec::with_capacity(4);
    if selection.original {
        tracks.push(SelectedAudioTrack {
            kind: AudioTrackKind::Original,
            with_announcer: false,
        });
    }
    if selection.instrumental && has_instrumental {
        tracks.push(SelectedAudioTrack {
            kind: AudioTrackKind::Instrumental,
            with_announcer: false,
        });
    }
    if cfg!(target_os = "windows") && selection.original_with_announcer {
        tracks.push(SelectedAudioTrack {
            kind: AudioTrackKind::Original,
            with_announcer: true,
        });
    }
    if cfg!(target_os = "windows") && selection.instrumental_with_announcer && has_instrumental {
        tracks.push(SelectedAudioTrack {
            kind: AudioTrackKind::Instrumental,
            with_announcer: true,
        });
    }
    tracks
}

pub fn resolve_video_dimensions(
    configuration: &ExportConfiguration,
    source_width: u32,
    source_height: u32,
) -> (u32, u32) {
    if configuration.video_quality == VideoExportQuality::Custom {
        return (
            even(configuration.custom_width),
            even(configuration.custom_height),
        );
    }
    let short_edge = match configuration.video_quality {
        VideoExportQuality::P720 => 720,
        VideoExportQuality::P1080 => 1080,
        VideoExportQuality::P1440 => 1440,
        VideoExportQuality::P8k => 4320,
        VideoExportQuality::Custom => unreachable!(),
    };
    match configuration.video_aspect {
        VideoExportAspect::Landscape16x9 => (even(short_edge * 16 / 9), even(short_edge)),
        VideoExportAspect::Portrait9x16 => (even(short_edge), even(short_edge * 16 / 9)),
        VideoExportAspect::Source => {
            let width = source_width.max(1);
            let height = source_height.max(1);
            if width >= height {
                fit_even_within_limit(
                    short_edge as f64 * width as f64 / height as f64,
                    short_edge as f64,
                )
            } else {
                fit_even_within_limit(
                    short_edge as f64,
                    short_edge as f64 * height as f64 / width as f64,
                )
            }
        }
    }
}

fn fit_even_within_limit(mut width: f64, mut height: f64) -> (u32, u32) {
    let largest = width.max(height);
    if largest > 8192.0 {
        let scale = 8192.0 / largest;
        width *= scale;
        height *= scale;
    }
    (even(width.round() as u32), even(height.round() as u32))
}

fn even(value: u32) -> u32 {
    let value = value.clamp(16, 8192);
    if value.is_multiple_of(2) {
        value
    } else {
        (value + 1).min(8192)
    }
}

fn mapped_progress(
    target: Arc<AtomicU32>,
    completed: usize,
    total: usize,
) -> impl FnMut(f32) + Send + 'static {
    move |value| {
        let mapped = (completed as f32 + value.clamp(0.0, 1.0)) / total.max(1) as f32;
        target.store(mapped.to_bits(), Ordering::Relaxed);
    }
}

fn finish_task(progress: &AtomicU32, completed: &mut usize, total: usize) {
    *completed += 1;
    progress.store(
        (*completed as f32 / total.max(1) as f32).to_bits(),
        Ordering::Relaxed,
    );
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into())
    } else {
        Ok(())
    }
}

fn bool_count<const N: usize>(values: [bool; N]) -> usize {
    values.into_iter().filter(|value| *value).count()
}

fn safe_filename(value: &str) -> String {
    let mut result = String::with_capacity(value.len().min(48));
    let mut previous_separator = false;
    for character in value.trim().chars() {
        if result.chars().count() >= 48 {
            break;
        }
        let invalid = character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            );
        if invalid || character.is_whitespace() {
            if !previous_separator && !result.is_empty() {
                result.push('_');
                previous_separator = true;
            }
        } else {
            result.push(character);
            previous_separator = false;
        }
    }
    let result = result.trim_matches(['.', '_']).to_string();
    if result.is_empty() {
        "export".into()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dimensions_cover_presets() {
        let mut configuration = ExportConfiguration {
            video_quality: VideoExportQuality::P720,
            video_aspect: VideoExportAspect::Landscape16x9,
            ..ExportConfiguration::default()
        };
        assert_eq!(
            resolve_video_dimensions(&configuration, 1920, 1080),
            (1280, 720)
        );
        configuration.video_aspect = VideoExportAspect::Portrait9x16;
        assert_eq!(
            resolve_video_dimensions(&configuration, 1920, 1080),
            (720, 1280)
        );
    }

    #[test]
    fn extreme_source_aspect_is_preserved_when_limited() {
        let configuration = ExportConfiguration {
            video_quality: VideoExportQuality::P8k,
            video_aspect: VideoExportAspect::Source,
            ..ExportConfiguration::default()
        };
        let (width, height) = resolve_video_dimensions(&configuration, 3840, 1080);
        assert_eq!(width, 8192);
        assert!((width as f64 / height as f64 - 3840.0 / 1080.0).abs() < 0.01);
    }

    #[test]
    fn filenames_are_windows_safe() {
        assert_eq!(safe_filename(" Français / Canada:* "), "Français_Canada");
        assert_eq!(safe_filename("***"), "export");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn selected_tracks_keep_each_base_audio_and_announcer_variant() {
        let mut configuration = ExportConfiguration::default();
        configuration.audio_by_language.insert(
            42,
            AudioSelection {
                original: true,
                instrumental: true,
                original_with_announcer: true,
                instrumental_with_announcer: true,
            },
        );
        let labels = selected_audio_tracks(&configuration, 42, true)
            .into_iter()
            .map(SelectedAudioTrack::label)
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "original",
                "instrumental",
                "original_announcer",
                "instrumental_announcer",
            ]
        );
    }
}
