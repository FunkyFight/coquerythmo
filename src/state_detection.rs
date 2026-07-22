//! State-level integration for track detections and text synchronization.

use crate::accessibility::AccessibilityEvent;
use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::command::Command;
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionCueId, DetectionKind,
    LineDetectionData, MediaTick, TextAnchor,
};
use crate::packet::ProjectData;
use crate::state::State;
use crate::workspaces::rythmo::view::Selection;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

static AUDITION_GENERATION: AtomicU64 = AtomicU64::new(0);

impl State {
    fn selected_detection_address(&self) -> Option<DetectionAddress> {
        match self.ui_shell.ui.rythmo_state().selected.as_ref() {
            Some(Selection::Detection(address)) => Some(*address),
            _ => None,
        }
    }

    pub fn has_selected_detection(&self) -> bool {
        self.selected_detection_address().is_some()
    }

    pub fn rythmo_detection_hovered(&self) -> bool {
        self.ui_shell.ui.rythmo_state().detection_hover.is_some()
    }

    pub fn open_detection_palette_from_hover(&mut self) -> bool {
        let opened = self
            .ui_shell
            .ui
            .rythmo_state
            .open_detection_palette_from_hover();
        if opened {
            crate::detection_foreground::activate_palette();
        }
        opened
    }

    pub fn focus_detection_parent_line(&mut self) {
        let Some(address) = self.selected_detection_address() else {
            return;
        };
        self.ui_shell.ui.rythmo_state.selected = if address.track().is_some() {
            None
        } else if self
            .project_session
            .project
            .get_line(address.line_id)
            .is_some()
        {
            Some(Selection::Line(address.line_id))
        } else {
            None
        };
        self.ui_shell.ui.rythmo_state.detection_drag = None;
    }

    pub fn add_detection(
        &mut self,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) {
        if target.validate().is_err() {
            return;
        }
        if kind.is_sync_point() {
            let Some(line) = self.project_session.project.get_line(line_id) else {
                return;
            };
            let Some(boundary) = target.grapheme_index() else {
                return;
            };
            let grapheme_count = UnicodeSegmentation::graphemes(line.text.as_str(), true).count();
            let start = MediaTick::from_frame(line.start_frame);
            let end = MediaTick::from_frame(line.end_frame());
            if grapheme_count == 0
                || boundary as usize >= grapheme_count
                || media_tick <= start
                || media_tick >= end
            {
                return;
            }
            if let Some(data) = self.project_session.project.detections().line(line_id) {
                if data.sync_points().iter().any(|point| {
                    point.grapheme_boundary == boundary && point.line_tick == media_tick
                }) || data.sync_points().iter().any(|point| {
                    (point.grapheme_boundary < boundary && point.line_tick >= media_tick)
                        || (point.grapheme_boundary > boundary && point.line_tick <= media_tick)
                }) {
                    return;
                }
            }
        }
        let detection_id = self
            .project_session
            .project
            .detections()
            .line(line_id)
            .map(LineDetectionData::next_detection_id)
            .unwrap_or(Some(DetectionCueId(1)));
        let Some(detection_id) = detection_id else {
            return;
        };
        let address = DetectionAddress {
            line_id,
            detection_id,
        };
        let cue = DetectionCue {
            id: detection_id,
            kind,
            media_tick,
            duration: MediaTick::ZERO,
            target,
        };
        self.execute_detection_command(Command::Detection {
            change: DetectionChange::Add {
                address,
                cue: cue.clone(),
            },
        });
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
        self.announce_detection_visual(kind, "ajouté");
    }

    pub fn move_detection(&mut self, address: DetectionAddress, mut media_tick: MediaTick) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .command_cue(address)
        else {
            return;
        };
        if cue.kind.is_sync_point() {
            let Some(line) = self.project_session.project.get_line(address.line_id) else {
                return;
            };
            let Some(point) = self
                .project_session
                .project
                .detections()
                .sync_point(address)
            else {
                return;
            };
            let data = self
                .project_session
                .project
                .detections()
                .line(address.line_id)
                .expect("selected sync point owns line data");
            let minimum = data
                .sync_points()
                .iter()
                .filter(|other| other.grapheme_boundary < point.grapheme_boundary)
                .map(|other| other.line_tick)
                .max()
                .unwrap_or(MediaTick::from_frame(line.start_frame))
                .saturating_add(MediaTick(1));
            let maximum = data
                .sync_points()
                .iter()
                .filter(|other| other.grapheme_boundary > point.grapheme_boundary)
                .map(|other| other.line_tick)
                .min()
                .unwrap_or(MediaTick::from_frame(line.end_frame()))
                .saturating_sub(MediaTick(1));
            if minimum > maximum {
                return;
            }
            media_tick = media_tick.clamp(minimum, maximum);
        }
        if cue.media_tick == media_tick {
            return;
        }
        let command = Command::Detection {
            change: DetectionChange::Move {
                address,
                old_tick: cue.media_tick,
                new_tick: media_tick,
            },
        };
        let can_coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection {
                change: DetectionChange::Move {
                    address: previous,
                    ..
                }
            }) if *previous == address
        );
        if can_coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection {
                        change: DetectionChange::Move { new_tick, .. },
                    } = last
                    {
                        *new_tick = media_tick;
                    }
                },
                EditOrigin::Local,
            );
            self.broadcast_detection_sync();
        } else {
            self.execute_detection_command(command);
        }
    }

    pub fn delete_detection(&mut self, address: DetectionAddress) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .command_cue(address)
        else {
            return;
        };
        let kind = cue.kind;
        self.execute_detection_command(Command::Detection {
            change: DetectionChange::Remove { address, cue },
        });
        if self.selected_detection_address() == Some(address) {
            self.ui_shell.ui.rythmo_state.selected = None;
        }
        self.ui_shell.ui.rythmo_state.detection_drag = None;
        self.announce_detection_visual(kind, "supprimé");
    }

    pub fn resize_detection(
        &mut self,
        address: DetectionAddress,
        media_tick: MediaTick,
        duration: MediaTick,
    ) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .command_cue(address)
        else {
            return;
        };
        if cue.kind.is_sync_point() {
            return;
        }
        let duration = MediaTick(duration.raw().max(0));
        if cue.media_tick == media_tick && cue.duration == duration {
            return;
        }
        let command = Command::Detection {
            change: DetectionChange::Resize {
                address,
                old_tick: cue.media_tick,
                new_tick: media_tick,
                old_duration: cue.duration,
                new_duration: duration,
            },
        };
        self.execute_detection_command(command);
    }

    pub fn delete_selected_detection(&mut self) {
        if let Some(address) = self.selected_detection_address() {
            self.delete_detection(address);
        }
    }

    pub fn nudge_selected_detection(&mut self, delta_ticks: i64) {
        let Some(address) = self.selected_detection_address() else {
            return;
        };
        if delta_ticks == 0 {
            self.audition_selected_detection();
            return;
        }
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .command_cue(address)
        else {
            return;
        };
        self.move_detection(
            address,
            MediaTick(cue.media_tick.raw().saturating_add(delta_ticks)),
        );
        self.announce_detection_visual(cue.kind, "déplacé");
    }

    pub fn nudge_selected_sync_anchor(&mut self, delta_graphemes: i32) {
        let Some(address) = self
            .selected_detection_address()
            .filter(|address| address.track().is_none())
        else {
            return;
        };
        let Some(point) = self
            .project_session
            .project
            .detections()
            .sync_point(address)
            .cloned()
        else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let count = UnicodeSegmentation::graphemes(line.text.as_str(), true).count();
        let new_boundary = (point.grapheme_boundary as i64 + delta_graphemes as i64)
            .clamp(0, count.saturating_sub(1) as i64) as u32;
        if new_boundary == point.grapheme_boundary {
            return;
        }
        self.move_sync_anchor(address, new_boundary);
    }

    pub fn toggle_selected_sync_affinity(&mut self) {
        let Some(address) = self
            .selected_detection_address()
            .filter(|address| address.track().is_none())
        else {
            return;
        };
        let Some(point) = self
            .project_session
            .project
            .detections()
            .sync_point(address)
            .cloned()
        else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let punctuation = UnicodeSegmentation::graphemes(line.text.as_str(), true)
            .nth(point.grapheme_boundary as usize)
            .is_some_and(|grapheme| grapheme.chars().all(crate::detection::is_sync_punctuation));
        let currently_left = match point.affinity {
            crate::detection::SyncAffinity::Left => true,
            crate::detection::SyncAffinity::Right => false,
            crate::detection::SyncAffinity::Auto => punctuation,
        };
        let new_affinity = if currently_left {
            crate::detection::SyncAffinity::Right
        } else {
            crate::detection::SyncAffinity::Left
        };
        self.execute_detection_command(Command::Detection {
            change: DetectionChange::SetAffinity {
                address,
                old_affinity: point.affinity,
                new_affinity,
            },
        });
    }

    pub fn move_sync_anchor(&mut self, address: DetectionAddress, new_boundary: u32) {
        let Some(point) = self
            .project_session
            .project
            .detections()
            .sync_point(address)
            .cloned()
        else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let count = UnicodeSegmentation::graphemes(line.text.as_str(), true).count();
        if new_boundary as usize >= count || new_boundary == point.grapheme_boundary {
            return;
        }
        let mut probe = self.project_session.project.detections().clone();
        if !probe.retarget_sync_point(address, new_boundary) {
            return;
        }
        let command = Command::Detection {
            change: DetectionChange::Retarget {
                address,
                old_boundary: point.grapheme_boundary,
                new_boundary,
            },
        };
        let can_coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection {
                change: DetectionChange::Retarget {
                    address: previous,
                    ..
                }
            }) if *previous == address
        );
        if can_coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection {
                        change:
                            DetectionChange::Retarget {
                                new_boundary: last_boundary,
                                ..
                            },
                    } = last
                    {
                        *last_boundary = new_boundary;
                    }
                },
                EditOrigin::Local,
            );
            self.broadcast_detection_sync();
        } else {
            self.execute_detection_command(command);
        }
    }

    pub fn add_sync_point_at_playhead(&mut self) {
        let rythmo = self.ui_shell.ui.rythmo_state();
        if rythmo.editing_character.is_some() || rythmo.editing_note.is_some() {
            return;
        }
        let line_id = rythmo
            .editing_line
            .or_else(|| match rythmo.selected.as_ref() {
                Some(Selection::Line(id)) => Some(*id),
                Some(Selection::Detection(address)) if address.track().is_none() => {
                    Some(address.line_id)
                }
                _ => None,
            });
        let Some(line_id) = line_id else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let graphemes =
            UnicodeSegmentation::graphemes(line.text.as_str(), true).collect::<Vec<_>>();
        if graphemes.is_empty() || line.duration_frames <= 0 {
            return;
        }
        let boundary = if rythmo.editing_line == Some(line_id) {
            let cursor_chars = rythmo.line_input.cursor_pos;
            let byte = line
                .text
                .char_indices()
                .nth(cursor_chars)
                .map(|(byte, _)| byte)
                .unwrap_or(line.text.len());
            UnicodeSegmentation::graphemes(&line.text[..byte], true)
                .count()
                .min(graphemes.len() - 1)
        } else {
            let progress = ((self.current_frame() - line.start_frame) as f64
                / line.duration_frames as f64)
                .clamp(0.0, 1.0);
            (progress * graphemes.len() as f64)
                .floor()
                .min((graphemes.len() - 1) as f64) as usize
        };
        self.add_detection(
            line_id,
            DetectionKind::TextSyncPoint,
            MediaTick::from_frame(self.current_frame()),
            TextAnchor::Grapheme {
                index: boundary as u32,
            },
        );
    }

    /// Ctrl+Space decodes the available two seconds before and after the
    /// selected source sign, mixes a short beep exactly at the sign and plays
    /// the bounded preview through the default output device.
    pub fn audition_selected_detection(&mut self) {
        let Some(address) = self.selected_detection_address() else {
            return;
        };
        let fps = self.fps().max(1.0);
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .command_cue(address)
        else {
            return;
        };
        if cue.kind.is_sync_point() {
            return;
        }
        let Some((window_start, window_end)) = self
            .project_session
            .project
            .detections()
            .audition_window(address, fps)
        else {
            return;
        };

        let total_frames = self
            .playback
            .video_player
            .as_ref()
            .map(|player| player.total_frames())
            .unwrap_or(0);
        let start_frame = window_start.as_frame_position().floor().max(0.0) as i64;
        let mut end_frame = window_end
            .as_frame_position()
            .ceil()
            .max(start_frame as f64) as i64;
        if total_frames > 0 {
            end_frame = end_frame.min(total_frames);
        }
        if end_frame <= start_frame {
            return;
        }

        self.seek_absolute(start_frame);
        self.finish_seek();

        let source_path = if self.active_audio_is_instrumental() {
            self.project_session
                .project
                .settings()
                .instrumental_audio_path
                .as_ref()
                .map(PathBuf::from)
                .or_else(|| self.playback.source_video_path.clone())
        } else {
            self.playback.source_video_path.clone()
        };
        let Some(source_path) = source_path else {
            return;
        };

        let audio_offset = self.active_audio_offset_frames();
        let start_seconds = start_frame as f64 / fps;
        let end_seconds = end_frame as f64 / fps;
        let cue_seconds = cue.media_tick.as_frame_position() / fps;
        let audio_start_seconds = ((start_frame - audio_offset) as f64 / fps).max(0.0);
        let leading_silence_seconds =
            ((audio_offset - start_frame).max(0) as f64 / fps).min(end_seconds - start_seconds);
        let duration_seconds = (end_seconds - start_seconds).max(0.01);
        let beep_offset_seconds = (cue_seconds - start_seconds).clamp(0.0, duration_seconds);
        let volume = self.ui_shell.ui.volume().clamp(0.0, 1.0);
        let generation = AUDITION_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        std::thread::spawn(move || {
            if let Err(error) = play_detection_preview(
                generation,
                source_path,
                audio_start_seconds,
                duration_seconds,
                leading_silence_seconds,
                beep_offset_seconds,
                volume,
            ) {
                log::warn!("Detection audition failed: {error}");
            }
        });
    }

    fn execute_detection_command(&mut self, command: Command) {
        EditExecutor::execute(&mut self.project_session, command, EditOrigin::Local);
        self.broadcast_detection_sync();
    }

    fn broadcast_detection_sync(&mut self) {
        if !self.collaboration.network.is_in_room() {
            return;
        }
        let data = ProjectData::from_project(&self.project_session.project);
        self.collaboration
            .network
            .send_raw("sync", serde_json::json!({ "project": data }));
    }

    /// AccessKit receives only the visual object and operation for edits.
    /// Opening a fiche is announced separately with its complete semantic text.
    fn announce_detection_visual(&self, kind: DetectionKind, verb: &str) {
        let object = if kind.is_sync_point() {
            "Point de synchronisation"
        } else {
            "Symbole de détection"
        };
        self.narration.announce_event(AccessibilityEvent::Success {
            message: format!("{object} {verb}"),
        });
    }
}

fn play_detection_preview(
    generation: u64,
    source_path: PathBuf,
    audio_start_seconds: f64,
    duration_seconds: f64,
    leading_silence_seconds: f64,
    beep_offset_seconds: f64,
    volume: f32,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "aucune sortie audio disponible".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|error| format!("configuration audio indisponible: {error}"))?;
    let config = supported.config();
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;

    let decode_duration = (duration_seconds - leading_silence_seconds).max(0.0);
    let mut decoded = Vec::new();
    if decode_duration > 0.001 {
        let output = crate::media_binary::command("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-ss")
            .arg(format!("{audio_start_seconds:.6}"))
            .arg("-i")
            .arg(&source_path)
            .arg("-t")
            .arg(format!("{decode_duration:.6}"))
            .arg("-vn")
            .arg("-f")
            .arg("f32le")
            .arg("-acodec")
            .arg("pcm_f32le")
            .arg("-ac")
            .arg(channels.to_string())
            .arg("-ar")
            .arg(sample_rate.to_string())
            .arg("pipe:1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("ffmpeg ne démarre pas: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        decoded = output
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();
    }

    let target_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let target_samples = target_frames.saturating_mul(channels);
    let silence_samples =
        (leading_silence_seconds * sample_rate as f64).round() as usize * channels;
    let mut samples = vec![0.0_f32; target_samples];
    let copy_len = decoded
        .len()
        .min(samples.len().saturating_sub(silence_samples));
    if copy_len > 0 {
        samples[silence_samples..silence_samples + copy_len].copy_from_slice(&decoded[..copy_len]);
    }
    for sample in &mut samples {
        *sample *= volume;
    }
    mix_detection_beep(
        &mut samples,
        channels,
        sample_rate,
        beep_offset_seconds,
        volume,
    );

    let shared = Arc::new(Mutex::new((samples, 0usize)));
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            build_preview_stream::<f32>(&device, &config, shared, generation)
        }
        cpal::SampleFormat::I16 => {
            build_preview_stream::<i16>(&device, &config, shared, generation)
        }
        cpal::SampleFormat::U16 => {
            build_preview_stream::<u16>(&device, &config, shared, generation)
        }
        format => return Err(format!("format audio non pris en charge: {format:?}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("lecture audio impossible: {error}"))?;

    let started = std::time::Instant::now();
    while started.elapsed().as_secs_f64() < duration_seconds + 0.08 {
        if AUDITION_GENERATION.load(Ordering::SeqCst) != generation {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(stream);
    Ok(())
}

fn mix_detection_beep(
    samples: &mut [f32],
    channels: usize,
    sample_rate: u32,
    offset_seconds: f64,
    volume: f32,
) {
    let start_frame = (offset_seconds * sample_rate as f64).round().max(0.0) as usize;
    let beep_frames = (0.075 * sample_rate as f64).round() as usize;
    let amplitude = (0.42 * volume.max(0.35)).min(0.55);
    for frame in 0..beep_frames {
        let envelope = 1.0 - frame as f32 / beep_frames.max(1) as f32;
        let phase = std::f32::consts::TAU * 1046.5 * frame as f32 / sample_rate as f32;
        let value = phase.sin() * amplitude * envelope;
        let output_frame = start_frame + frame;
        for channel in 0..channels {
            let index = output_frame
                .saturating_mul(channels)
                .saturating_add(channel);
            if let Some(sample) = samples.get_mut(index) {
                *sample = (*sample + value).clamp(-1.0, 1.0);
            }
        }
    }
}

fn build_preview_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<(Vec<f32>, usize)>>,
    generation: u64,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                let active = AUDITION_GENERATION.load(Ordering::SeqCst) == generation;
                let Ok(mut guard) = shared.lock() else {
                    for sample in output {
                        *sample = T::from_sample(0.0);
                    }
                    return;
                };
                let (samples, cursor) = &mut *guard;
                for output_sample in output {
                    let value = if active {
                        samples.get(*cursor).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    *output_sample = T::from_sample(value);
                    *cursor = (*cursor).saturating_add(1);
                }
            },
            move |error| log::warn!("Detection preview stream error: {error}"),
            None,
        )
        .map_err(|error| format!("création du flux audio impossible: {error}"))
}
