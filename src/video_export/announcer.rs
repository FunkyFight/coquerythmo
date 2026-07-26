//! Windows-only synthesis of spoken karaoke character cues.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::project::Project;
use crate::rythmo_layout::track_index_for_y_slot;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct KaraokeAnnouncement {
    pub characters: Vec<String>,
    /// The synthesized speech must have ended at this point.
    pub finish_seconds: f64,
}

/// Computes cues from karaoke-character changes on each track independently.
/// Lines starting at the same frame are announced together.
pub(super) fn karaoke_announcements(
    project: &Project,
    fps: f64,
    pre_roll_seconds: f64,
) -> Vec<KaraokeAnnouncement> {
    if !fps.is_finite() || fps <= 0.0 {
        return Vec::new();
    }
    let mut lines = project
        .lines()
        .filter(|line| line.karaoke && !line.character_name.trim().is_empty())
        .collect::<Vec<_>>();
    lines.sort_by_key(|line| {
        (
            line.start_frame,
            track_index_for_y_slot(line.y_slot),
            line.id,
        )
    });

    let mut previous_by_track = BTreeMap::new();
    let mut announcements = Vec::new();
    let mut group_start = 0;
    while group_start < lines.len() {
        let frame = lines[group_start].start_frame;
        let group_end = lines[group_start..]
            .iter()
            .position(|line| line.start_frame != frame)
            .map(|offset| group_start + offset)
            .unwrap_or(lines.len());
        let mut characters = Vec::new();
        let mut seen = HashSet::new();
        for line in &lines[group_start..group_end] {
            let track = track_index_for_y_slot(line.y_slot);
            let character = line.character_name.trim().to_string();
            let changed = previous_by_track.get(&track) != Some(&character);
            previous_by_track.insert(track, character.clone());
            if changed && seen.insert(character.clone()) {
                characters.push(character);
            }
        }
        if !characters.is_empty() {
            announcements.push(KaraokeAnnouncement {
                characters,
                finish_seconds: (frame as f64 / fps + pre_roll_seconds - 1.0).max(0.0),
            });
        }
        group_start = group_end;
    }
    announcements
}

#[cfg(target_os = "windows")]
pub(crate) fn synthesize(
    project: &Project,
    fps: f64,
    pre_roll_seconds: f64,
    output: &Path,
) -> Result<Option<PathBuf>, String> {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let cues = karaoke_announcements(project, fps, pre_roll_seconds);
    if cues.is_empty() {
        return Ok(None);
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis())
        .unwrap_or(0);
    let mut cue_paths = Vec::with_capacity(cues.len());
    for (index, cue) in cues.iter().enumerate() {
        let path = parent.join(format!(
            ".coquerythmo_announce_{}_{}_{}.wav",
            std::process::id(),
            stamp,
            index
        ));
        let ssml = format!(
            "<speak version=\"1.0\" xml:lang=\"fr-FR\"><prosody rate=\"150%\">{}</prosody></speak>",
            xml_escape(&announcement_phrase(&cue.characters))
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.SetOutputToWaveFile($env:COQUERYTHMO_ANNOUNCER_OUT); $s.SpeakSsml($env:COQUERYTHMO_ANNOUNCER_SSML); $s.Dispose()"])
            .env("COQUERYTHMO_ANNOUNCER_OUT", &path)
            .env("COQUERYTHMO_ANNOUNCER_SSML", ssml)
            .status()
            .map_err(|error| format!("Windows karaoke announcer: {error}"))?;
        if !status.success() {
            return Err("Windows karaoke announcer could not synthesize speech".into());
        }
        let duration = wav_duration_seconds(&path)?;
        cue_paths.push((path, cue_start_seconds(cue.finish_seconds, duration)));
    }
    let mix_path = parent.join(format!(
        ".coquerythmo_announcer_{}_{}.wav",
        std::process::id(),
        stamp
    ));
    let mut command = crate::media_binary::command("ffmpeg");
    for (path, _) in &cue_paths {
        command.arg("-i").arg(path);
    }
    let filter = announcer_mix_filter(
        &cue_paths
            .iter()
            .map(|(_, start)| *start)
            .collect::<Vec<_>>(),
    );
    command
        .args(["-filter_complex", &filter, "-c:a", "pcm_s16le", "-y"])
        .arg(&mix_path);
    let status = command
        .status()
        .map_err(|error| format!("ffmpeg mix karaoke announcements: {error}"))?;
    for (path, _) in cue_paths {
        let _ = std::fs::remove_file(path);
    }
    if status.success() {
        Ok(Some(mix_path))
    } else {
        Err("ffmpeg could not mix karaoke announcements".into())
    }
}

fn announcement_phrase(characters: &[String]) -> String {
    match characters {
        [] => String::new(),
        [character] => character.clone(),
        [first, second] => format!("{first} et {second}"),
        _ => format!(
            "{} et {}",
            characters[..characters.len() - 1].join(", "),
            characters.last().expect("non-empty characters")
        ),
    }
}

fn cue_start_seconds(finish_seconds: f64, duration_seconds: f64) -> f64 {
    (finish_seconds - duration_seconds.max(0.0)).max(0.0)
}

#[cfg(target_os = "windows")]
fn wav_duration_seconds(path: &Path) -> Result<f64, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Read synthesized karaoke announcement: {error}"))?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Windows karaoke announcer returned an invalid WAV file".into());
    }
    let mut offset = 12;
    let mut byte_rate = None;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let data_start = offset + 8;
        let data_end = data_start.saturating_add(size);
        if data_end > bytes.len() {
            break;
        }
        if &bytes[offset..offset + 4] == b"fmt " && size >= 12 {
            byte_rate = Some(u32::from_le_bytes(
                bytes[data_start + 8..data_start + 12].try_into().unwrap(),
            ));
        } else if &bytes[offset..offset + 4] == b"data" {
            let Some(byte_rate) = byte_rate.filter(|rate| *rate > 0) else {
                return Err("Windows karaoke announcement WAV has no byte rate".into());
            };
            return Ok(size as f64 / byte_rate as f64);
        }
        offset = data_end + size % 2;
    }
    Err("Windows karaoke announcement WAV has no audio data".into())
}

fn announcer_mix_filter(starts: &[f64]) -> String {
    let filters = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            format!(
                "[{index}:a]adelay={}:all=1[a{index}]",
                (start * 1000.0).round().max(0.0) as i64
            )
        })
        .collect::<Vec<_>>();
    let inputs = (0..starts.len())
        .map(|index| format!("[a{index}]"))
        .collect::<String>();
    format!(
        "{};{}amix=inputs={}:normalize=0",
        filters.join(";"),
        inputs,
        starts.len()
    )
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn synthesize(_: &Project, _: f64, _: f64, _: &Path) -> Result<Option<PathBuf>, String> {
    Ok(None)
}

#[cfg(target_os = "windows")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::RythmoLine;

    fn line(id: u64, frame: i64, character: &str, karaoke: bool) -> RythmoLine {
        RythmoLine {
            id,
            start_frame: frame,
            duration_frames: 24,
            y_slot: 0.0,
            text: String::new(),
            character_name: character.into(),
            character_color: [0.0; 4],
            kind: crate::rythmo_line::RythmoLineKind::Dialogue,
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke,
            note: String::new(),
            presence: crate::rythmo_line::LinePresence::On,
            text_emotions: Vec::new(),
        }
    }

    fn line_on_track(id: u64, frame: i64, track: f32, character: &str) -> RythmoLine {
        let mut line = line(id, frame, character, true);
        line.y_slot = track;
        line
    }

    #[test]
    fn announces_only_character_changes_on_the_same_track() {
        let mut project = Project::new();
        project.insert_line(line_on_track(1, 48, 0.0, "A"));
        project.insert_line(line_on_track(2, 96, 0.0, "A"));
        project.insert_line(line(3, 120, "Ignored", false));
        project.insert_line(line_on_track(4, 144, 0.0, "B"));
        project.insert_line(line_on_track(5, 192, 0.25, "C"));
        project.insert_line(line_on_track(6, 240, 0.25, "C"));
        project.insert_line(line_on_track(7, 288, 0.25, "D"));
        assert_eq!(
            karaoke_announcements(&project, 24.0, 0.0),
            vec![
                KaraokeAnnouncement {
                    characters: vec!["A".into()],
                    finish_seconds: 1.0,
                },
                KaraokeAnnouncement {
                    characters: vec!["B".into()],
                    finish_seconds: 5.0,
                },
                KaraokeAnnouncement {
                    characters: vec!["C".into()],
                    finish_seconds: 7.0,
                },
                KaraokeAnnouncement {
                    characters: vec!["D".into()],
                    finish_seconds: 11.0,
                }
            ]
        );
    }

    #[test]
    fn combines_simultaneous_track_changes_into_one_phrase() {
        let mut project = Project::new();
        project.insert_line(line_on_track(3, 96, 0.5, "C"));
        project.insert_line(line_on_track(1, 96, 0.0, "A"));
        project.insert_line(line_on_track(2, 96, 0.25, "B"));
        assert_eq!(
            karaoke_announcements(&project, 24.0, 0.0),
            vec![KaraokeAnnouncement {
                characters: vec!["A".into(), "B".into(), "C".into()],
                finish_seconds: 3.0,
            }]
        );
        assert_eq!(
            announcement_phrase(&["A".into(), "B".into(), "C".into()]),
            "A, B et C"
        );
    }

    #[test]
    fn cue_timing_uses_source_video_frames() {
        let mut project = Project::new();
        project.insert_line(line(1, 600, "B", true));

        assert_eq!(
            karaoke_announcements(&project, 25.0, 0.0),
            vec![KaraokeAnnouncement {
                characters: vec!["B".into()],
                finish_seconds: 23.0,
            }]
        );
    }

    #[test]
    fn cue_start_makes_speech_end_one_second_before_the_line() {
        assert_eq!(cue_start_seconds(9.0, 2.5), 6.5);
        assert_eq!(cue_start_seconds(0.5, 2.5), 0.0);
    }

    #[test]
    fn mix_filter_separates_delay_filters_from_the_mixer() {
        assert_eq!(
            announcer_mix_filter(&[1.0, 2.5]),
            "[0:a]adelay=1000:all=1[a0];[1:a]adelay=2500:all=1[a1];[a0][a1]amix=inputs=2:normalize=0"
        );
    }
}
