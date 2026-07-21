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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        } else if self.project_session.project.get_line(address.line_id).is_some() {
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

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .cloned()
        else {
            return;
        };
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
            .detection(address)
            .cloned()
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
            .detection(address)
            .cloned()
        else {
            return;
        };
        self.move_detection(
            address,
            MediaTick(cue.media_tick.raw().saturating_add(delta_ticks)),
        );
        self.announce_detection_visual(cue.kind, "déplacé");
    }

    /// Ctrl+Space seeks to the available two-second lead-in, starts the real
    /// video player and mixes a clearly audible cue at the exact detection.
    pub fn audition_selected_detection(&mut self) {
        let Some(address) = self.selected_detection_address() else {
            return;
        };
        let fps = self.fps().max(1.0);
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .cloned()
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
        let mut end_frame = window_end.as_frame_position().ceil().max(start_frame as f64) as i64;
        if total_frames > 0 {
            end_frame = end_frame.min(total_frames);
        }
        if end_frame <= start_frame {
            return;
        }

        self.seek_absolute(start_frame);
        self.finish_seek();
        if !self.is_video_playing() {
            self.toggle_play_pause();
        }

        // The real player supplies the selected source/instrumental audio. The
        // auxiliary CPAL stream contains only the cue, otherwise the dialogue
        // would be decoded and heard twice.
        let start_seconds = start_frame as f64 / fps;
        let end_seconds = end_frame as f64 / fps;
        let cue_seconds = cue.media_tick.as_frame_position() / fps;
        let duration_seconds = (end_seconds - start_seconds).max(0.01);
        let beep_offset_seconds = (cue_seconds - start_seconds).clamp(0.0, duration_seconds);
        let volume = self.ui_shell.ui.volume().clamp(0.0, 1.0);
        let generation = AUDITION_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        std::thread::spawn(move || {
            if let Err(error) = play_detection_beep(
                generation,
                duration_seconds,
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

    fn announce_detection_visual(&self, kind: DetectionKind, verb: &str) {
        let object = if kind.is_sync_point() {
            "Point de synchronisation"
        } else {
            "Symbole de détection"
        };
        self.narration
            .announce_event(AccessibilityEvent::Success {
                message: format!("{object} {verb}"),
            });
    }
}

fn play_detection_beep(
    generation: u64,
    duration_seconds: f64,
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

    let target_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let target_samples = target_frames.saturating_mul(channels);
    let mut samples = vec![0.0_f32; target_samples];
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
    let beep_frames = (0.090 * sample_rate as f64).round() as usize;
    let amplitude = (0.82 * volume.max(0.55)).min(0.92);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn louder_beep_is_mixed_at_the_requested_frame() {
        let sample_rate = 48_000;
        let channels = 2;
        let mut samples = vec![0.0; sample_rate as usize * channels];
        mix_detection_beep(&mut samples, channels, sample_rate, 0.25, 1.0);
        let start = (sample_rate as f64 * 0.25).round() as usize * channels;
        let peak = samples[start..]
            .iter()
            .take((sample_rate as f32 * 0.09) as usize * channels)
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.7);
        assert!(peak <= 1.0);
    }

    #[test]
    fn auxiliary_preview_contains_no_dialogue_before_the_beep() {
        let sample_rate = 48_000;
        let channels = 2;
        let mut samples = vec![0.0; sample_rate as usize * channels];
        mix_detection_beep(&mut samples, channels, sample_rate, 0.25, 1.0);
        let start = (sample_rate as f64 * 0.25).round() as usize * channels;
        assert!(samples[..start].iter().all(|sample| *sample == 0.0));
    }
}
