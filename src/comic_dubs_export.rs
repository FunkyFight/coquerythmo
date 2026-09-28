//! Video rendering of the Comic Dubs timeline.
//!
//! Every frame is evaluated by [`crate::comic_dubs_timeline`], rasterized on
//! the CPU and streamed to FFmpeg as raw RGBA. Identical consecutive frames
//! are rendered once, so still pages cost almost nothing while camera moves,
//! transitions and bubble animations stay smooth.

use crate::comic_dubs::{Bubble, ComicDubsProject, Page, Point};
use crate::comic_dubs_text::{self as text_layout, LineReveal, TextLayout};
use crate::comic_dubs_timeline::{
    self as timeline, BubbleFrame, Frame, LayerFrame, Placement, Timeline,
};
use crate::project::ExportConfiguration;
use image::{imageops, RgbaImage};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Music level under voices when ducking is enabled.
const DUCKED_LEVEL: f32 = 0.35;
const DUCK_RAMP_S: f64 = 0.25;
/// Keeps the ducking expression short enough for any command line.
const MAX_DUCK_TERMS: usize = 80;

/// Merges the closest intervals until at most `limit` remain.
fn coarsen(mut intervals: Vec<(u64, u64)>, limit: usize) -> Vec<(u64, u64)> {
    let mut gap = 400;
    while intervals.len() > limit {
        gap *= 2;
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(intervals.len());
        for interval in intervals {
            match merged.last_mut() {
                Some(last) if interval.0 <= last.1 + gap => last.1 = last.1.max(interval.1),
                _ => merged.push(interval),
            }
        }
        intervals = merged;
    }
    intervals
}

pub fn export_mp4(
    project: &ComicDubsProject,
    output: &Path,
    configuration: &ExportConfiguration,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    export_video(
        project,
        output,
        configuration,
        &progress,
        &cancel,
        None,
        false,
        (0.0, 1.0),
    )
}

pub fn export(
    project: &ComicDubsProject,
    output: &Path,
    configuration: &ExportConfiguration,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    if configuration.comic_dubs_pages_zip {
        export_pages_zip(project, output, configuration, progress, cancel)
    } else {
        export_video(
            project,
            output,
            configuration,
            &progress,
            &cancel,
            None,
            configuration.comic_dubs_alpha,
            (0.0, 1.0),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn export_video(
    project: &ComicDubsProject,
    output: &Path,
    configuration: &ExportConfiguration,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    page_filter: Option<usize>,
    alpha: bool,
    progress_range: (f32, f32),
) -> Result<(), String> {
    let first_page = project
        .pages()
        .get(page_filter.unwrap_or(0))
        .ok_or_else(|| "Aucune page Comic Dubs à exporter".to_string())?;
    let fps = configuration.fps.clamp(1.0, 480.0);
    let (width, height) = crate::configured_export::resolve_video_dimensions(
        configuration,
        first_page.width,
        first_page.height,
    );
    // yuv420p needs even dimensions; ProRes 4444 does not.
    let (width, height) = if alpha {
        (width.max(1), height.max(1))
    } else {
        ((width & !1).max(2), (height & !1).max(2))
    };
    let minimum_ms = (1_000.0 / fps).ceil() as u64;
    let plan = Timeline::build(project, page_filter, minimum_ms);
    if plan.is_empty() || plan.total_ms == 0 {
        return Err("Aucune bulle Comic Dubs à exporter".into());
    }
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Impossible de créer le dossier d'export : {error}"))?;
    }
    let result = render_to_ffmpeg(
        project,
        &plan,
        output,
        fps,
        (width, height),
        alpha,
        progress,
        cancel,
        progress_range,
    );
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

fn export_pages_zip(
    project: &ComicDubsProject,
    output: &Path,
    configuration: &ExportConfiguration,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let pages = project.pages().iter().enumerate().collect::<Vec<_>>();
    if pages.is_empty() {
        return Err("Aucune page Comic Dubs à exporter".into());
    }
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let temp_dir =
        crate::media_binary::installation_temp_dir().join(format!("comic-dubs-pages-{stamp}"));
    std::fs::create_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    let result = (|| {
        let mut videos = Vec::with_capacity(pages.len());
        for (position, (page_index, page)) in pages.iter().enumerate() {
            check_cancel(&cancel)?;
            let extension = if configuration.comic_dubs_alpha {
                "mov"
            } else {
                "mp4"
            };
            let name = format!(
                "{:03}-{}.{}",
                position + 1,
                safe_page_name(&page.file_name),
                extension,
            );
            let path = temp_dir.join(&name);
            let share = 0.9 / pages.len() as f32;
            export_video(
                project,
                &path,
                configuration,
                &progress,
                &cancel,
                Some(*page_index),
                configuration.comic_dubs_alpha,
                (share * position as f32, share),
            )?;
            videos.push((name, path));
        }
        let file = std::fs::File::create(output)
            .map_err(|error| format!("Création du ZIP Comic Dubs : {error}"))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, path) in videos {
            check_cancel(&cancel)?;
            zip.start_file(name, options)
                .map_err(|error| format!("Écriture du ZIP Comic Dubs : {error}"))?;
            let mut video = std::fs::File::open(path).map_err(|error| error.to_string())?;
            std::io::copy(&mut video, &mut zip).map_err(|error| error.to_string())?;
        }
        zip.finish()
            .map_err(|error| format!("Finalisation du ZIP Comic Dubs : {error}"))?;
        progress.store(1.0_f32.to_bits(), Ordering::Relaxed);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(temp_dir);
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

fn safe_page_name(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("page");
    let safe = stem
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if safe.trim_matches('-').is_empty() {
        "page".into()
    } else {
        safe
    }
}

/// One FFmpeg audio input placed on the timeline.
#[derive(Debug, Clone, PartialEq)]
struct AudioCue {
    path: PathBuf,
    start_ms: u64,
    gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct MusicBed {
    path: PathBuf,
    looped: bool,
    volume: f32,
    ducking: Vec<(u64, u64)>,
    fade_out_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct AudioPlan {
    cues: Vec<AudioCue>,
    music: Option<MusicBed>,
    total_ms: u64,
}

impl AudioPlan {
    fn build(project: &ComicDubsProject, plan: &Timeline) -> Self {
        let mut cues = Vec::new();
        for (page_index, cue) in plan.cues() {
            let Some(bubble) = project
                .pages()
                .get(page_index)
                .and_then(|page| page.bubbles.get(cue.bubble_index))
            else {
                continue;
            };
            if let Some(audio) = cue.voice_audio.and_then(|id| project.audio(id)) {
                cues.push(AudioCue {
                    path: audio.playback_path.clone(),
                    start_ms: cue.voice_start_ms,
                    gain: bubble.sound.voice_volume,
                });
            }
            if let Some(audio) = cue.sfx_audio.and_then(|id| project.audio(id)) {
                cues.push(AudioCue {
                    path: audio.playback_path.clone(),
                    start_ms: cue.reveal_ms,
                    gain: bubble.sound.sfx_volume,
                });
            }
        }
        let studio = project.studio();
        let music = studio
            .music_audio_id
            .and_then(|id| project.audio(id))
            .filter(|_| studio.music_volume > 0.0)
            .map(|audio| MusicBed {
                path: audio.playback_path.clone(),
                looped: studio.music_loop,
                volume: studio.music_volume,
                ducking: if studio.music_ducking {
                    coarsen(plan.voice_intervals(), MAX_DUCK_TERMS)
                } else {
                    Vec::new()
                },
                fade_out_ms: studio.music_fade_out_ms.min(plan.total_ms),
            });
        Self {
            cues,
            music,
            total_ms: plan.total_ms,
        }
    }

    fn is_silent(&self) -> bool {
        self.cues.is_empty() && self.music.is_none()
    }

    /// Distinct cue files in first-use order: a bruitage reused on many
    /// bubbles is decoded once and split inside the filter graph.
    fn inputs(&self) -> Vec<&Path> {
        let mut inputs: Vec<&Path> = Vec::new();
        for cue in &self.cues {
            if !inputs.contains(&cue.path.as_path()) {
                inputs.push(&cue.path);
            }
        }
        inputs
    }

    /// Filter graph producing `[a]`; inputs start at FFmpeg index `first_input`.
    fn filter_graph(&self, first_input: usize) -> String {
        let total = self.total_ms as f64 / 1_000.0;
        let mut filter = String::new();
        let mut mixed = Vec::new();
        let inputs = self.inputs();
        let uses = inputs
            .iter()
            .map(|path| self.cues.iter().filter(|cue| cue.path == *path).count())
            .collect::<Vec<_>>();
        for (index, count) in uses.iter().enumerate().filter(|(_, count)| **count > 1) {
            write!(filter, "[{}:a]asplit={count}", first_input + index).unwrap();
            for use_index in 0..*count {
                write!(filter, "[u{index}_{use_index}]").unwrap();
            }
            filter.push(';');
        }
        let mut consumed = vec![0; inputs.len()];
        for (index, cue) in self.cues.iter().enumerate() {
            let input = inputs
                .iter()
                .position(|path| *path == cue.path)
                .unwrap_or(0);
            let source = if uses[input] > 1 {
                let label = format!("[u{input}_{}]", consumed[input]);
                consumed[input] += 1;
                label
            } else {
                format!("[{}:a]", first_input + input)
            };
            write!(
                filter,
                "{source}volume={:.3},adelay={}:all=1[c{index}];",
                cue.gain, cue.start_ms,
            )
            .unwrap();
            mixed.push(format!("[c{index}]"));
        }
        if let Some(music) = &self.music {
            let input = first_input + inputs.len();
            let mut volume = format!("{:.4}", music.volume);
            if !music.ducking.is_empty() {
                let terms = music
                    .ducking
                    .iter()
                    .map(|(start, end)| {
                        let start = *start as f64 / 1_000.0 - DUCK_RAMP_S;
                        let end = *end as f64 / 1_000.0 + DUCK_RAMP_S;
                        format!(
                            "clip((t-{start:.3})/{DUCK_RAMP_S},0,1)*clip(({end:.3}-t)/{DUCK_RAMP_S},0,1)"
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("+");
                volume = format!(
                    "'{:.4}*(1-{:.3}*min(1,{terms}))':eval=frame",
                    music.volume,
                    1.0 - DUCKED_LEVEL,
                );
            }
            write!(
                filter,
                "[{input}:a]atrim=duration={total:.3},asetpts=PTS-STARTPTS,volume={volume}"
            )
            .unwrap();
            if music.fade_out_ms > 0 {
                let fade = music.fade_out_ms as f64 / 1_000.0;
                write!(
                    filter,
                    ",afade=t=out:st={:.3}:d={fade:.3}",
                    (total - fade).max(0.0)
                )
                .unwrap();
            }
            filter.push_str("[music];");
            mixed.push("[music]".into());
        }
        write!(
            filter,
            "{}amix=inputs={}:normalize=0:duration=longest,apad=whole_dur={total:.6}[a]",
            mixed.concat(),
            mixed.len(),
        )
        .unwrap();
        filter
    }
}

#[allow(clippy::too_many_arguments)]
fn render_to_ffmpeg(
    project: &ComicDubsProject,
    plan: &Timeline,
    output: &Path,
    fps: f64,
    (width, height): (u32, u32),
    alpha: bool,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    (progress_start, progress_share): (f32, f32),
) -> Result<(), String> {
    let report = |value: f32| {
        progress.store(
            (progress_start + progress_share * value.clamp(0.0, 1.0)).to_bits(),
            Ordering::Relaxed,
        )
    };
    let total_s = plan.total_ms as f64 / 1_000.0;
    let frame_count = (total_s * fps).ceil().max(1.0) as u64;
    let audio = AudioPlan::build(project, plan);

    let mut command = crate::media_binary::command("ffmpeg");
    command.args([
        "-v",
        "warning",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        &format!("{width}x{height}"),
        "-r",
        &format!("{fps}"),
        "-i",
        "pipe:0",
    ]);
    for path in audio.inputs() {
        command.arg("-i").arg(path);
    }
    if let Some(music) = &audio.music {
        if music.looped {
            command.args(["-stream_loop", "-1"]);
        }
        command.arg("-i").arg(&music.path);
    }
    if audio.is_silent() {
        command.args(["-map", "0:v:0", "-an"]);
    } else {
        command.args([
            "-filter_complex",
            &audio.filter_graph(1),
            "-map",
            "0:v:0",
            "-map",
            "[a]",
        ]);
    }
    command.args(["-t", &format!("{total_s:.6}")]);
    if alpha {
        command.args([
            "-c:v",
            "prores_ks",
            "-profile:v",
            "4",
            "-pix_fmt",
            "yuva444p10le",
        ]);
    } else {
        command.args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
        ]);
    }
    if !audio.is_silent() {
        command.args(["-c:a", "aac", "-b:a", "192k"]);
    }
    command
        .args(["-movflags", "+faststart"])
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    report(0.02);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Démarrage de FFmpeg : {error}"))?;
    let stderr = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "FFmpeg n'accepte pas les images Comic Dubs".to_string())?;
    let mut writer = std::io::BufWriter::with_capacity((width * height * 4) as usize, stdin);
    let mut renderer = Renderer::new(project, width, height, alpha);
    let aspect = width as f32 / height as f32;
    let mut last_key = None;
    let mut write_error = None;
    for index in 0..frame_count {
        if cancel.load(Ordering::Relaxed) {
            drop(writer);
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into());
        }
        let at_ms = (index as f64 * 1_000.0 / fps) as u64;
        let frame = timeline::evaluate(project, plan, at_ms, aspect);
        let key = frame.key();
        if last_key.as_ref() != Some(&key) {
            if let Err(error) = renderer.render(&frame) {
                drop(writer);
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            last_key = Some(key);
        }
        if let Err(error) = writer.write_all(renderer.bytes()) {
            write_error = Some(error);
            break;
        }
        if index % 8 == 0 {
            report(0.03 + 0.92 * (index + 1) as f32 / frame_count as f32);
        }
    }
    if write_error.is_none() {
        if let Err(error) = writer.flush() {
            write_error = Some(error);
        }
    }
    drop(writer);

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Attente de FFmpeg : {error}"))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !status.success() || write_error.is_some() {
        let detail = if stderr.trim().is_empty() {
            write_error
                .map(|error| error.to_string())
                .unwrap_or_default()
        } else {
            stderr.trim().to_string()
        };
        return Err(if detail.is_empty() {
            "Échec de l'export MP4 Comic Dubs".into()
        } else {
            format!("Échec de l'export MP4 Comic Dubs : {detail}")
        });
    }
    report(1.0);
    Ok(())
}

/// Alpha coverage of a rasterized text line.
struct Mask {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
    left_pad: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MaskKey {
    text: String,
    size: u32,
    spacing: u32,
    style: u8,
    outline: u32,
}

/// Premultiplied RGBA in 0..=1.
type Color = [f32; 4];

struct Renderer<'a> {
    project: &'a ComicDubsProject,
    width: u32,
    height: u32,
    alpha: bool,
    canvas: Vec<Color>,
    bytes: Vec<u8>,
    pages: HashMap<usize, Vec<RgbaImage>>,
    layouts: HashMap<(usize, usize, u64), TextLayout>,
    masks: HashMap<MaskKey, Option<Mask>>,
    /// Pixel bounds `(x0, y0, x1, y1)` of the layer being drawn.
    clip: (i64, i64, i64, i64),
}

impl<'a> Renderer<'a> {
    fn new(project: &'a ComicDubsProject, width: u32, height: u32, alpha: bool) -> Self {
        let pixels = (width * height) as usize;
        Self {
            project,
            width,
            height,
            alpha,
            canvas: vec![[0.0; 4]; pixels],
            bytes: vec![0; pixels * 4],
            pages: HashMap::new(),
            layouts: HashMap::new(),
            masks: HashMap::new(),
            clip: (0, 0, i64::from(width), i64::from(height)),
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn render(&mut self, frame: &Frame) -> Result<(), String> {
        let background = if self.alpha {
            [0.0; 4]
        } else {
            let [r, g, b] = self.project.studio().background;
            [
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                1.0,
            ]
        };
        self.canvas.fill(background);
        for layer in &frame.layers {
            self.render_layer(layer, frame.shake)?;
        }
        let flash = frame.flash.clamp(0.0, 1.0);
        let alpha_output = self.alpha;
        let row_len = self.width as usize;
        let canvas = &self.canvas;
        for_each_band(&mut self.bytes, row_len * 4, |first_row, bytes| {
            let pixels = &canvas[first_row * row_len..][..bytes.len() / 4];
            for (pixel, bytes) in pixels.iter().zip(bytes.chunks_exact_mut(4)) {
                let pixel = if flash > 0.0 {
                    over([flash; 4], *pixel)
                } else {
                    *pixel
                };
                let alpha = pixel[3].clamp(0.0, 1.0);
                let unpremultiply = if alpha_output && alpha > 0.0 {
                    1.0 / alpha
                } else {
                    1.0
                };
                for channel in 0..3 {
                    bytes[channel] =
                        ((pixel[channel] * unpremultiply).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                bytes[3] = if alpha_output {
                    (alpha * 255.0 + 0.5) as u8
                } else {
                    255
                };
            }
        });
        Ok(())
    }

    fn render_layer(&mut self, layer: &LayerFrame, shake: (f32, f32)) -> Result<(), String> {
        let project = self.project;
        let Some(page) = project.pages().get(layer.page_index) else {
            return Ok(());
        };
        let view = (0.0, 0.0, self.width as f32, self.height as f32);
        let placement = Placement::compute(view, page.width, page.height, layer, shake);
        // Each layer lives in its own (possibly sliding) video frame.
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let shift_x = (layer.offset.0 * self.width as f32).round() as i64;
        let shift_y = (layer.offset.1 * self.height as f32).round() as i64;
        self.clip = (
            shift_x.clamp(0, width),
            shift_y.clamp(0, height),
            (width + shift_x).clamp(0, width),
            (height + shift_y).clamp(0, height),
        );
        if self.clip.0 >= self.clip.2 || self.clip.1 >= self.clip.3 {
            return Ok(());
        }
        self.draw_page(layer, page, placement)?;
        let dim = layer.brightness.clamp(0.0, 1.0);
        for frame in &layer.bubbles {
            let Some(bubble) = page.bubbles.get(frame.bubble_index) else {
                continue;
            };
            if frame.show_background {
                self.draw_bubble_shape(bubble, frame, placement, layer.opacity, dim);
            }
        }
        for frame in &layer.bubbles {
            let Some(bubble) = page.bubbles.get(frame.bubble_index) else {
                continue;
            };
            if frame.show_text {
                self.draw_bubble_text(layer.page_index, page, bubble, frame, placement, layer, dim);
            }
        }
        Ok(())
    }

    fn page_levels(&mut self, page_index: usize, page: &Page) -> Result<(), String> {
        if !self.pages.contains_key(&page_index) {
            if self.pages.len() >= 3 {
                self.pages.clear();
            }
            let source = image::open(&page.image_path)
                .map_err(|error| format!("Page {} illisible : {error}", page.file_name))?
                .to_rgba8();
            let mut levels = vec![source];
            while levels.last().is_some_and(|level| level.width() >= 128) {
                let last = levels.last().unwrap();
                let next = imageops::resize(
                    last,
                    (last.width() / 2).max(1),
                    (last.height() / 2).max(1),
                    imageops::FilterType::Triangle,
                );
                levels.push(next);
            }
            self.pages.insert(page_index, levels);
        }
        Ok(())
    }

    fn draw_page(
        &mut self,
        layer: &LayerFrame,
        page: &Page,
        placement: Placement,
    ) -> Result<(), String> {
        let (width, height) = (self.width, self.height);
        let opacity = layer.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return Ok(());
        }
        let brightness = layer.brightness.clamp(0.0, 1.0);
        self.page_levels(layer.page_index, page)?;
        let levels = &self.pages[&layer.page_index];
        let canvas = &mut self.canvas;
        let level = levels
            .iter()
            .rev()
            .find(|level| level.width() as f32 >= placement.width * 0.95)
            .unwrap_or(&levels[0]);
        let (source_w, source_h) = (level.width() as f32, level.height() as f32);
        let clip = self.clip;
        let x0 = (placement.x.max(0.0).floor() as u32).max(clip.0 as u32);
        let y0 = (placement.y.max(0.0).floor() as u32).max(clip.1 as u32);
        let x1 = ((placement.x + placement.width)
            .min(width as f32)
            .ceil()
            .max(0.0) as u32)
            .min(clip.2 as u32);
        let y1 = ((placement.y + placement.height)
            .min(height as f32)
            .ceil()
            .max(0.0) as u32)
            .min(clip.3 as u32);
        if x1 <= x0 || y1 <= y0 {
            return Ok(());
        }
        let raw = level.as_raw();
        let stride = level.width() as usize * 4;
        let max_x = level.width() as usize - 1;
        let max_y = level.height() as usize - 1;
        let sample = |x: usize, y: usize| {
            let offset = y * stride + x * 4;
            [
                f32::from(raw[offset]),
                f32::from(raw[offset + 1]),
                f32::from(raw[offset + 2]),
                f32::from(raw[offset + 3]),
            ]
        };
        // Horizontal sampling is the same for every row: compute it once.
        let columns = (x0..x1)
            .map(|x| {
                let px = x as f32 + 0.5;
                if px < placement.x || px > placement.x + placement.width {
                    return None;
                }
                let u = ((px - placement.x) / placement.width * source_w - 0.5)
                    .clamp(0.0, source_w - 1.0);
                let ux = u.floor() as usize;
                Some((ux, (ux + 1).min(max_x), u.fract()))
            })
            .collect::<Vec<_>>();
        let draw_row = |y: u32, row: &mut [[f32; 4]]| {
            let py = y as f32 + 0.5;
            if py < placement.y || py > placement.y + placement.height {
                return;
            }
            let v =
                ((py - placement.y) / placement.height * source_h - 0.5).clamp(0.0, source_h - 1.0);
            let (vy, fy) = (v.floor() as usize, v.fract());
            let vy1 = (vy + 1).min(max_y);
            let pixels = &mut row[x0 as usize..x1 as usize];
            for (pixel, column) in pixels.iter_mut().zip(&columns) {
                let Some((ux, ux1, fx)) = *column else {
                    continue;
                };
                let (a, b, c, d) = (
                    sample(ux, vy),
                    sample(ux1, vy),
                    sample(ux, vy1),
                    sample(ux1, vy1),
                );
                let mut color = [0.0; 4];
                for channel in 0..4 {
                    let top = a[channel] + (b[channel] - a[channel]) * fx;
                    let bottom = c[channel] + (d[channel] - c[channel]) * fx;
                    color[channel] = (top + (bottom - top) * fy) / 255.0;
                }
                let alpha = color[3] * opacity;
                let source = [
                    color[0] * brightness * alpha,
                    color[1] * brightness * alpha,
                    color[2] * brightness * alpha,
                    alpha,
                ];
                *pixel = over(source, *pixel);
            }
        };
        let row_len = width as usize;
        let rows = &mut canvas[y0 as usize * row_len..y1 as usize * row_len];
        for_each_band(rows, row_len, |first_row, band| {
            for (index, row) in band.chunks_exact_mut(row_len).enumerate() {
                draw_row(y0 + (first_row + index) as u32, row);
            }
        });
        Ok(())
    }

    fn draw_bubble_shape(
        &mut self,
        bubble: &Bubble,
        frame: &BubbleFrame,
        placement: Placement,
        layer_opacity: f32,
        dim: f32,
    ) {
        let opacity = (frame.opacity * layer_opacity).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }
        let points = timeline::animated_points(bubble, frame)
            .into_iter()
            .map(|point| placement.map(point))
            .collect::<Vec<_>>();
        let scale = placement.font_scale() * frame.scale;
        if bubble.look.shadow {
            let offset = (6.0 * scale, 9.0 * scale);
            let shadow = points
                .iter()
                .map(|(x, y)| (x + offset.0, y + offset.1))
                .collect::<Vec<_>>();
            let coverage = Coverage::polygons(&[shadow], self.width, self.height);
            self.composite(&coverage, premultiplied([0, 0, 0], 0.35 * opacity, 1.0));
        }
        if bubble.color[3] != 0 {
            let coverage =
                Coverage::polygons(std::slice::from_ref(&points), self.width, self.height);
            let [r, g, b, _] = bubble.color;
            self.composite(&coverage, premultiplied([r, g, b], opacity, dim));
        }
        let (outline, width) = match bubble.look.outline_color {
            Some(color) => (color, bubble.look.outline_width * scale),
            None => ([225, 225, 235, 255], bubble.look.outline_width.min(1.0)),
        };
        if bubble.look.outline_width > 0.0 {
            let pieces = stroke_polygons(&points, width.max(1.0));
            let coverage = Coverage::polygons(&pieces, self.width, self.height);
            self.composite(
                &coverage,
                premultiplied([outline[0], outline[1], outline[2]], opacity, dim),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bubble_text(
        &mut self,
        page_index: usize,
        page: &Page,
        bubble: &Bubble,
        frame: &BubbleFrame,
        placement: Placement,
        layer: &LayerFrame,
        dim: f32,
    ) {
        let opacity = (frame.opacity * frame.text_opacity * layer.opacity).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            return;
        }
        let family = self.project.font_family().map(str::to_owned);
        let layout = self
            .layouts
            .entry((page_index, frame.bubble_index, frame.pose_ms))
            .or_insert_with(|| {
                text_layout::layout(
                    bubble,
                    &bubble.text,
                    bubble.points_at(frame.pose_ms),
                    page.width,
                    page.height,
                    family.as_deref(),
                )
            })
            .clone();
        let pose = bubble.points_at(frame.pose_ms);
        let count = pose.len().max(1) as f32;
        let bubble_center = Point {
            x: pose.iter().map(|point| point.x).sum::<f32>() / count,
            y: pose.iter().map(|point| point.y).sum::<f32>() / count,
        };
        let text_center = layout.center();
        let total_scale = frame.scale * frame.text_scale;
        let transform = |point: Point| {
            let text = Point {
                x: text_center.x
                    + (point.x - text_center.x) * frame.text_scale
                    + frame.text_offset.0,
                y: text_center.y
                    + (point.y - text_center.y) * frame.text_scale
                    + frame.text_offset.1,
            };
            placement.map(Point {
                x: bubble_center.x + (text.x - bubble_center.x) * frame.scale + frame.offset.0,
                y: bubble_center.y + (text.y - bubble_center.y) * frame.scale + frame.offset.1,
            })
        };
        let pixel_scale = placement.font_scale() * total_scale;
        let font_px = layout.font_px * pixel_scale;
        if font_px < 1.0 {
            return;
        }
        let spacing = layout.letter_spacing * pixel_scale;
        let line_height = layout.line_height * pixel_scale;
        let color = effective_text_color(bubble);
        let text_color = premultiplied(color, opacity, dim);
        let outline = bubble.look.text_outline_color.map(|color| {
            (
                premultiplied([color[0], color[1], color[2]], opacity, dim),
                (bubble.look.text_outline_width * pixel_scale).max(0.5),
            )
        });
        let style = u8::from(bubble.bold)
            | (u8::from(bubble.look.italic) << 1)
            | (u8::from(bubble.strikethrough) << 2)
            | (u8::from(bubble.underline) << 3);
        let reveals = text_layout::line_reveals(&layout.lines, &bubble.text, frame.reveal);
        for (index, (line, reveal)) in layout.lines.iter().zip(reveals).enumerate() {
            if reveal == LineReveal::Hidden || line.trim().is_empty() {
                continue;
            }
            let origin = transform(layout.line_origin(index, bubble.text_alignment));
            let clip = match &reveal {
                LineReveal::Partial(prefix) => Some(
                    text_layout::measure(prefix, font_px, spacing, family.as_deref(), bubble.bold)
                        + 1.0,
                ),
                _ => None,
            };
            // Rasterize at a bucketed size and scale while blending, so camera
            // zooms reuse the same glyph masks instead of re-rendering text.
            let bucket = mask_bucket(font_px);
            let scale = font_px / bucket;
            let key = MaskKey {
                text: line.clone(),
                size: (bucket * 4.0).round() as u32,
                spacing: (spacing / scale * 4.0).round() as u32,
                style,
                outline: 0,
            };
            let Some((mask_height, left_pad)) = self
                .mask(&key, family.as_deref(), 0.0)
                .map(|mask| (mask.height as f32 * scale, mask.left_pad * scale))
            else {
                continue;
            };
            let x = origin.0 - left_pad;
            let y = origin.1 + (line_height - mask_height) * 0.5;
            let clip_x = clip.map(|width| origin.0 + width);
            if let Some((outline_color, radius)) = outline {
                let outline_key = MaskKey {
                    outline: (radius / scale * 4.0).round().max(1.0) as u32,
                    ..key.clone()
                };
                if let Some(pad) = self
                    .mask(&outline_key, family.as_deref(), radius / scale)
                    .map(|dilated| (dilated.left_pad * scale - left_pad).max(0.0))
                {
                    self.blend_mask(
                        &outline_key,
                        x - pad,
                        y - pad,
                        scale,
                        outline_color,
                        clip_x.map(|clip| clip + radius),
                    );
                }
            }
            self.blend_mask(&key, x, y, scale, text_color, clip_x);
        }
    }

    fn mask(&mut self, key: &MaskKey, family: Option<&str>, radius: f32) -> Option<&Mask> {
        if !self.masks.contains_key(key) {
            if self.masks.len() > 512 {
                self.masks.clear();
            }
            let mask = if key.outline == 0 {
                let font_px = key.size as f32 / 4.0;
                let left_pad = (font_px * 0.25).ceil();
                crate::vector_text::render_bubble_line_standalone(
                    &key.text,
                    font_px,
                    family,
                    key.spacing as f32 / 4.0,
                    key.style & 1 != 0,
                    key.style & 2 != 0,
                    key.style & 4 != 0,
                    key.style & 8 != 0,
                    left_pad,
                )
                .map(|pixmap| Mask {
                    width: pixmap.width,
                    height: pixmap.height,
                    alpha: pixmap
                        .pixels
                        .chunks_exact(4)
                        .map(|pixel| pixel[3])
                        .collect(),
                    left_pad,
                })
            } else {
                let base = MaskKey {
                    outline: 0,
                    ..key.clone()
                };
                self.mask(&base, family, 0.0)
                    .map(|mask| dilate(mask, radius))
            };
            self.masks.insert(key.clone(), mask);
        }
        self.masks.get(key).and_then(Option::as_ref)
    }

    /// Blends a cached mask scaled by `scale` (≤ 1) at `(x, y)`.
    fn blend_mask(
        &mut self,
        key: &MaskKey,
        x: f32,
        y: f32,
        scale: f32,
        color: Color,
        clip_x: Option<f32>,
    ) {
        let Self {
            masks,
            canvas,
            clip,
            width,
            ..
        } = self;
        let Some(mask) = masks.get(key).and_then(Option::as_ref) else {
            return;
        };
        let (mask_w, mask_h) = (mask.width as i64, mask.height as i64);
        let sample = |sx: i64, sy: i64| {
            if sx < 0 || sy < 0 || sx >= mask_w || sy >= mask_h {
                0.0
            } else {
                f32::from(mask.alpha[(sy * mask_w + sx) as usize])
            }
        };
        let right = clip_x.map_or(clip.2, |limit| (limit.round() as i64).min(clip.2));
        let x0 = (x.floor() as i64).max(clip.0);
        let y0 = (y.floor() as i64).max(clip.1);
        let x1 = ((x + mask_w as f32 * scale).ceil() as i64).min(right);
        let y1 = ((y + mask_h as f32 * scale).ceil() as i64).min(clip.3);
        let exact = (scale - 1.0).abs() < 0.001;
        for target_y in y0..y1 {
            let v = (target_y as f32 + 0.5 - y) / scale - 0.5;
            let (vy, fy) = (v.floor() as i64, v - v.floor());
            for target_x in x0..x1 {
                let u = (target_x as f32 + 0.5 - x) / scale - 0.5;
                let coverage = if exact {
                    sample(u.round() as i64, v.round() as i64)
                } else {
                    let (ux, fx) = (u.floor() as i64, u - u.floor());
                    let top = sample(ux, vy) * (1.0 - fx) + sample(ux + 1, vy) * fx;
                    let bottom = sample(ux, vy + 1) * (1.0 - fx) + sample(ux + 1, vy + 1) * fx;
                    top * (1.0 - fy) + bottom * fy
                } / 255.0;
                if coverage <= 0.0 {
                    continue;
                }
                let pixel = &mut canvas[(target_y * i64::from(*width) + target_x) as usize];
                *pixel = over(color.map(|channel| channel * coverage), *pixel);
            }
        }
    }

    fn composite(&mut self, coverage: &Coverage, color: Color) {
        for row in 0..coverage.height {
            let y = (coverage.y + row) as i64;
            if y < self.clip.1 || y >= self.clip.3 {
                continue;
            }
            let canvas_row = (coverage.y + row) * self.width as usize;
            for column in 0..coverage.width {
                let x = (coverage.x + column) as i64;
                if x < self.clip.0 || x >= self.clip.2 {
                    continue;
                }
                let amount = coverage.data[row * coverage.width + column];
                if amount <= 0.0 {
                    continue;
                }
                let pixel = &mut self.canvas[canvas_row + coverage.x + column];
                *pixel = over(color.map(|channel| channel * amount), *pixel);
            }
        }
    }
}

/// Raster size used for a text drawn at `font_px`: 4 px steps for small
/// text, 8 px above, so scaled masks are never enlarged.
fn mask_bucket(font_px: f32) -> f32 {
    let step = if font_px < 48.0 { 4.0 } else { 8.0 };
    ((font_px / step).ceil() * step).max(step)
}

fn effective_text_color(bubble: &Bubble) -> [u8; 3] {
    bubble.text_color.map_or_else(
        || {
            if luminance(bubble.color) > 0.55 {
                [24, 24, 30]
            } else {
                [244, 244, 248]
            }
        },
        |color| [color[0], color[1], color[2]],
    )
}

fn luminance(color: [u8; 4]) -> f32 {
    color[0] as f32 / 255.0 * 0.2126
        + color[1] as f32 / 255.0 * 0.7152
        + color[2] as f32 / 255.0 * 0.0722
}

fn premultiplied(color: [u8; 3], opacity: f32, brightness: f32) -> Color {
    let alpha = opacity.clamp(0.0, 1.0);
    [
        f32::from(color[0]) / 255.0 * brightness * alpha,
        f32::from(color[1]) / 255.0 * brightness * alpha,
        f32::from(color[2]) / 255.0 * brightness * alpha,
        alpha,
    ]
}

fn over(source: Color, destination: Color) -> Color {
    let keep = 1.0 - source[3];
    [
        source[0] + destination[0] * keep,
        source[1] + destination[1] * keep,
        source[2] + destination[2] * keep,
        source[3] + destination[3] * keep,
    ]
}

/// Splits `data` (rows of `row_len` items) into horizontal bands processed on
/// the available cores; `work` gets the band's first row index.
fn for_each_band<T: Send>(data: &mut [T], row_len: usize, work: impl Fn(usize, &mut [T]) + Sync) {
    if row_len == 0 || data.is_empty() {
        return;
    }
    let rows = data.len() / row_len;
    let workers = std::thread::available_parallelism()
        .map_or(1, |cores| cores.get())
        .clamp(1, 8);
    let band = rows.div_ceil(workers).max(32);
    if workers == 1 || band >= rows {
        work(0, data);
        return;
    }
    std::thread::scope(|scope| {
        for (index, chunk) in data.chunks_mut(band * row_len).enumerate() {
            let work = &work;
            scope.spawn(move || work(index * band, chunk));
        }
    });
}

/// Grows a mask by `radius` pixels (text outline).
fn dilate(mask: &Mask, radius: f32) -> Mask {
    let pad = radius.ceil().max(1.0) as u32;
    let width = mask.width + pad * 2;
    let height = mask.height + pad * 2;
    let mut alpha = vec![0_u8; (width * height) as usize];
    let reach = pad as i64;
    let mut offsets = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            if ((dx * dx + dy * dy) as f32).sqrt() <= radius + 0.5 {
                offsets.push((dx, dy));
            }
        }
    }
    for y in 0..mask.height as i64 {
        for x in 0..mask.width as i64 {
            let value = mask.alpha[(y * mask.width as i64 + x) as usize];
            if value == 0 {
                continue;
            }
            for (dx, dy) in &offsets {
                let target = ((y + dy + reach) * width as i64 + x + dx + reach) as usize;
                alpha[target] = alpha[target].max(value);
            }
        }
    }
    Mask {
        width,
        height,
        alpha,
        left_pad: mask.left_pad + pad as f32,
    }
}

/// Anti-aliased coverage of a set of polygons (union), limited to their bounds.
struct Coverage {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    data: Vec<f32>,
}

impl Coverage {
    const SUBSAMPLES: usize = 4;

    fn polygons(polygons: &[Vec<(f32, f32)>], canvas_w: u32, canvas_h: u32) -> Self {
        let points = polygons.iter().flatten();
        let min_x = points.clone().map(|point| point.0).fold(f32::MAX, f32::min);
        let max_x = points.clone().map(|point| point.0).fold(f32::MIN, f32::max);
        let min_y = points.clone().map(|point| point.1).fold(f32::MAX, f32::min);
        let max_y = points.map(|point| point.1).fold(f32::MIN, f32::max);
        let x = min_x.floor().clamp(0.0, canvas_w as f32) as usize;
        let y = min_y.floor().clamp(0.0, canvas_h as f32) as usize;
        let x_end = max_x.ceil().clamp(0.0, canvas_w as f32) as usize;
        let y_end = max_y.ceil().clamp(0.0, canvas_h as f32) as usize;
        let (width, height) = (x_end.saturating_sub(x), y_end.saturating_sub(y));
        let mut coverage = Self {
            x,
            y,
            width,
            height,
            data: vec![0.0; width * height],
        };
        if width == 0 || height == 0 {
            return coverage;
        }
        let mut scratch = vec![0.0_f32; width];
        let mut intersections = Vec::new();
        let weight = 1.0 / Self::SUBSAMPLES as f32;
        for polygon in polygons.iter().filter(|polygon| polygon.len() >= 3) {
            // Only scan this polygon's own bounds (outlines are made of many
            // small pieces spread over a large box).
            let (mut left, mut right) = (f32::MAX, f32::MIN);
            let (mut top, mut bottom) = (f32::MAX, f32::MIN);
            for point in polygon {
                left = left.min(point.0);
                right = right.max(point.0);
                top = top.min(point.1);
                bottom = bottom.max(point.1);
            }
            let rows = (top - y as f32).floor().clamp(0.0, height as f32) as usize
                ..(bottom - y as f32).ceil().clamp(0.0, height as f32) as usize;
            let first_column = (left - x as f32).floor().clamp(0.0, width as f32) as usize;
            let last_column = (right - x as f32).ceil().clamp(0.0, width as f32) as usize;
            if rows.is_empty() || last_column <= first_column {
                continue;
            }
            let span_width = last_column - first_column;
            let origin = (x + first_column) as f32;
            let scratch = &mut scratch[..span_width];
            for row in rows {
                scratch.fill(0.0);
                let mut touched = false;
                for sub in 0..Self::SUBSAMPLES {
                    let sample_y = (y + row) as f32 + (sub as f32 + 0.5) * weight;
                    intersections.clear();
                    for (a, b) in polygon
                        .iter()
                        .zip(polygon.iter().cycle().skip(1))
                        .take(polygon.len())
                    {
                        if (a.1 > sample_y) != (b.1 > sample_y) {
                            intersections.push(a.0 + (sample_y - a.1) * (b.0 - a.0) / (b.1 - a.1));
                        }
                    }
                    intersections.sort_by(f32::total_cmp);
                    for span in intersections.chunks_exact(2) {
                        let start = (span[0] - origin).clamp(0.0, span_width as f32);
                        let end = (span[1] - origin).clamp(0.0, span_width as f32);
                        if end <= start {
                            continue;
                        }
                        touched = true;
                        let first = start.floor() as usize;
                        let last = (end.ceil() as usize).min(span_width);
                        for (column, value) in scratch.iter_mut().enumerate().take(last).skip(first)
                        {
                            let left = start.max(column as f32);
                            let right = end.min(column as f32 + 1.0);
                            if right > left {
                                *value += (right - left) * weight;
                            }
                        }
                    }
                }
                if touched {
                    let offset = row * width + first_column;
                    let data = &mut coverage.data[offset..offset + span_width];
                    for (value, amount) in data.iter_mut().zip(scratch.iter()) {
                        *value = value.max(amount.min(1.0));
                    }
                }
            }
        }
        coverage
    }
}

/// Outline pieces: one quad per edge plus a round joint per vertex.
fn stroke_polygons(points: &[(f32, f32)], width: f32) -> Vec<Vec<(f32, f32)>> {
    let half = width * 0.5;
    let mut pieces = Vec::with_capacity(points.len() * 2);
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = dx.hypot(dy);
        if length < 0.01 {
            continue;
        }
        let (nx, ny) = (-dy / length * half, dx / length * half);
        pieces.push(vec![
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
        ]);
    }
    if half >= 1.0 {
        for point in points {
            pieces.push(
                (0..10)
                    .map(|step| {
                        let angle = step as f32 / 10.0 * std::f32::consts::TAU;
                        (point.0 + angle.cos() * half, point.1 + angle.sin() * half)
                    })
                    .collect(),
            );
        }
    }
    pieces
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err(crate::video_export::EXPORT_CANCELLED_MESSAGE.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comic_dubs::{
        BubbleLook, BubblePreset, BubbleSound, PageFx, PageMotion, PageTransition, StudioSettings,
    };
    use crate::recording::{RecordedAudio, WaveformData};

    fn triangle() -> Vec<Point> {
        vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.5, y: 0.9 },
        ]
    }

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("coquerythmo-{label}-{stamp}"));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn write_wav(path: &Path, samples: usize) {
        let data_len = (samples * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend((0..samples).flat_map(|index| (((index % 40) as i16 - 20) * 400).to_le_bytes()));
        std::fs::write(path, wav).unwrap();
    }

    fn recorded(samples: u64) -> RecordedAudio {
        RecordedAudio {
            file_name: "voice.wav".into(),
            sample_rate: 8_000,
            channels: 1,
            sample_count: samples,
            checksum: String::new(),
            waveform: WaveformData::default(),
        }
    }

    fn probe(output: &Path, entries: &str, stream: Option<&str>) -> String {
        let mut command = crate::media_binary::command("ffprobe");
        command.args(["-v", "error"]);
        if let Some(stream) = stream {
            command.args(["-select_streams", stream]);
        }
        let output = command
            .args(["-show_entries", entries, "-of", "default=nw=1:nk=1"])
            .arg(output)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn audio_plan_places_voices_sfx_and_ducked_music() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let voice = project.add_audio("v.wav".into(), "v.wav".into(), recorded(8_000));
        let sfx = project.add_audio("s.wav".into(), "s.wav".into(), recorded(800));
        let music = project.add_audio("m.wav".into(), "m.wav".into(), recorded(80_000));
        let bubble = project.add_bubble(page, triangle()).unwrap();
        project.assign_audio(bubble, Some(voice));
        project.set_bubble_sound(
            bubble,
            BubbleSound {
                audio_delay_ms: 300,
                sfx_audio_id: Some(sfx),
                voice_volume: 1.5,
                ..BubbleSound::default()
            },
        );
        project.set_studio(StudioSettings {
            music_audio_id: Some(music),
            ..StudioSettings::default()
        });
        let plan = Timeline::build(&project, None, 40);
        let audio = AudioPlan::build(&project, &plan);
        assert_eq!(audio.cues.len(), 2);
        assert_eq!((audio.cues[0].start_ms, audio.cues[0].gain), (300, 1.5));
        assert_eq!(audio.cues[1].start_ms, 0);
        let music = audio.music.as_ref().unwrap();
        assert_eq!(music.ducking, vec![(300, 1_300)]);
        let graph = audio.filter_graph(1);
        assert!(graph.contains("[1:a]volume=1.500,adelay=300:all=1[c0];"));
        assert!(graph.contains("[3:a]atrim=duration="));
        assert!(graph.contains("clip((t-0.050)/0.25,0,1)"));
        assert!(graph.ends_with("[a]"));
    }

    #[test]
    fn reused_sound_effects_share_one_input_and_ducking_stays_bounded() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let sfx = project.add_audio("s.wav".into(), "s.wav".into(), recorded(800));
        for _ in 0..3 {
            let bubble = project.add_bubble(page, triangle()).unwrap();
            project.set_bubble_sound(
                bubble,
                BubbleSound {
                    sfx_audio_id: Some(sfx),
                    ..BubbleSound::default()
                },
            );
        }
        let plan = Timeline::build(&project, None, 40);
        let audio = AudioPlan::build(&project, &plan);
        assert_eq!(audio.cues.len(), 3);
        assert_eq!(audio.inputs().len(), 1);
        let graph = audio.filter_graph(1);
        assert!(graph.starts_with("[1:a]asplit=3[u0_0][u0_1][u0_2];"));
        assert!(graph.contains("[u0_2]volume=1.000,adelay="));

        let many = (0..500)
            .map(|index| (index * 1_000, index * 1_000 + 300))
            .collect();
        assert!(coarsen(many, MAX_DUCK_TERMS).len() <= MAX_DUCK_TERMS);
    }

    #[test]
    fn page_export_keeps_pages_without_bubbles() {
        let mut project = ComicDubsProject::default();
        project.add_page("empty.png".into(), "empty.png".into(), 100, 100);
        let plan = Timeline::build(&project, Some(0), 40);
        assert_eq!(plan.pages.len(), 1);
        assert!(plan.pages[0].cues.is_empty());
        assert!(plan.total_ms >= 1_000);
    }

    #[test]
    fn alpha_frames_keep_the_letterbox_transparent_and_bubbles_opaque() {
        let directory = temp_dir("comic-alpha");
        let path = directory.join("page.png");
        RgbaImage::from_pixel(4, 2, image::Rgba([20, 30, 40, 255]))
            .save(&path)
            .unwrap();
        let mut project = ComicDubsProject::default();
        let page = project.add_page("page.png".into(), path, 40, 20);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        project.set_bubble_color(bubble, [250, 10, 10, 255]);
        // Before its turn, the empty bubble is shown as a mask.
        project.set_page_fx(
            page,
            PageFx {
                intro_ms: 500,
                ..PageFx::default()
            },
        );
        let plan = Timeline::build(&project, None, 40);
        let frame = timeline::evaluate(&project, &plan, 10, 1.0);
        for alpha in [true, false] {
            let mut renderer = Renderer::new(&project, 40, 40, alpha);
            renderer.render(&frame).unwrap();
            let pixel = |x: u32, y: u32| {
                let offset = ((y * 40 + x) * 4) as usize;
                renderer.bytes()[offset..offset + 4].to_vec()
            };
            assert_eq!(pixel(0, 0)[3], if alpha { 0 } else { 255 });
            assert_eq!(pixel(0, 15)[3], 255);
            // The bubble center is red.
            assert!(pixel(20, 16)[0] > 200 && pixel(20, 16)[1] < 60);
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn coverage_is_antialiased_and_strokes_join() {
        let square = vec![(1.5, 1.5), (6.5, 1.5), (6.5, 6.5), (1.5, 6.5)];
        let coverage = Coverage::polygons(&[square.clone()], 10, 10);
        let at =
            |x: usize, y: usize| coverage.data[(y - coverage.y) * coverage.width + x - coverage.x];
        assert!((at(3, 3) - 1.0).abs() < 0.01);
        assert!((at(1, 3) - 0.5).abs() < 0.01);
        let stroke = stroke_polygons(&square, 2.0);
        assert_eq!(stroke.len(), 8);
        let mask = Mask {
            width: 1,
            height: 1,
            alpha: vec![255],
            left_pad: 0.0,
        };
        let grown = dilate(&mask, 2.0);
        assert_eq!((grown.width, grown.height), (5, 5));
        assert_eq!(grown.alpha[12], 255);
    }

    #[test]
    fn exports_every_bubble_and_later_page_with_audio() {
        if !crate::media_binary::can_run("ffmpeg") {
            return;
        }
        let directory = temp_dir("comic-test");
        let page_path = directory.join("page.png");
        RgbaImage::from_pixel(64, 64, image::Rgba([20, 30, 40, 255]))
            .save(&page_path)
            .unwrap();
        let output = directory.join("comic.mp4");
        let audio_path = directory.join("voice.wav");
        write_wav(&audio_path, 800);
        let mut project = ComicDubsProject::default();
        let first_page = project.add_page("page-1.png".into(), page_path.clone(), 64, 64);
        let first = project.add_bubble(first_page, triangle()).unwrap();
        let second = project.add_bubble(first_page, triangle()).unwrap();
        let later_page = project.add_page("page-2.png".into(), page_path, 64, 64);
        let third = project.add_bubble(later_page, triangle()).unwrap();
        let audio = project.add_audio("voice.wav".into(), audio_path, recorded(800));
        for (bubble, text) in [(first, "Un"), (second, "Deux"), (third, "Trois")] {
            project.set_bubble_text(bubble, text.into());
            project.assign_audio(bubble, Some(audio));
        }
        let expected_duration = Timeline::build(&project, None, 40).total_ms as f64 / 1_000.0;
        export_mp4(
            &project,
            &output,
            &ExportConfiguration::default(),
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(std::fs::metadata(&output).unwrap().len() > 100);
        assert_eq!(probe(&output, "stream=codec_type", Some("a:0")), "audio");
        let duration = |stream: Option<&str>| {
            probe(
                &output,
                if stream.is_some() {
                    "stream=duration"
                } else {
                    "format=duration"
                },
                stream,
            )
            .parse::<f64>()
            .unwrap()
        };
        assert!(duration(Some("a:0")) >= expected_duration - 0.1);
        assert!(duration(None) >= expected_duration - 0.1);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn studio_effects_export_with_music_camera_and_transitions() {
        if !crate::media_binary::can_run("ffmpeg") {
            return;
        }
        let directory = temp_dir("comic-studio");
        let page_path = directory.join("page.png");
        let mut page_image = RgbaImage::new(96, 128);
        for (x, y, pixel) in page_image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x * 2) as u8, (y * 2) as u8, 120, 255]);
        }
        page_image.save(&page_path).unwrap();
        let voice_path = directory.join("voice.wav");
        let music_path = directory.join("music.wav");
        write_wav(&voice_path, 4_000);
        write_wav(&music_path, 2_000);
        let mut project = ComicDubsProject::default();
        let voice = project.add_audio("voice.wav".into(), voice_path, recorded(4_000));
        let music = project.add_audio("music.wav".into(), music_path, recorded(2_000));
        project.set_studio(StudioSettings {
            music_audio_id: Some(music),
            background: [30, 0, 60],
            ..StudioSettings::default()
        });
        for (index, preset) in [BubblePreset::Shout, BubblePreset::Sound]
            .into_iter()
            .enumerate()
        {
            let page = project.add_page(format!("{index}.png"), page_path.clone(), 96, 128);
            project.set_page_fx(
                page,
                PageFx {
                    transition: PageTransition::SlideLeft,
                    transition_ms: 300,
                    motion: PageMotion::ZoomIn,
                    ..PageFx::default()
                },
            );
            let bubble = project.add_bubble(page, triangle()).unwrap();
            project.set_bubble_text(bubble, "BOUM ! Ça marche".into());
            project.apply_bubble_preset(bubble, preset);
            project.assign_audio(bubble, Some(voice));
            project.add_shot(page, None);
            let shot = project.add_shot_around_bubble(bubble, 16.0 / 9.0).unwrap();
            let mut settings = *project.shot(shot).unwrap();
            settings.move_ms = 200;
            project.set_shot(settings);
            project.set_bubble_look(
                bubble,
                BubbleLook {
                    shadow: true,
                    italic: true,
                    ..project.bubble(bubble).unwrap().look
                },
            );
        }
        let output = directory.join("studio.mp4");
        let configuration = ExportConfiguration {
            fps: 12.0,
            ..ExportConfiguration::default()
        };
        export(
            &project,
            &output,
            &configuration,
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let expected = Timeline::build(&project, None, 84).total_ms as f64 / 1_000.0;
        let duration = probe(&output, "format=duration", None)
            .parse::<f64>()
            .unwrap();
        assert!(duration >= expected - 0.15, "{duration} < {expected}");
        assert_eq!(probe(&output, "stream=codec_type", Some("a:0")), "audio");
        let _ = std::fs::remove_dir_all(directory);
    }
}
