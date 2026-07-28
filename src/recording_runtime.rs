//! Main-thread orchestration for microphone capture and incoming FLAC files.
//!
//! Durable timeline mutations remain in `recording`; this adapter owns only
//! transient CPAL/FFmpeg state and temporary files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::audio_transfer::{AudioTransferMetadata, AudioTransferReceiver, ReceivedAudio};
use crate::media_recording::FfmpegFlacRecorder;
use crate::recording::{
    CaptureController, CaptureEvent, CaptureState, CaptureTarget, CompletedCapture, RecordingError,
    RecordingProject, SystemClock, WaveformData,
};

static RECORDING_NONCE: AtomicU64 = AtomicU64::new(0);
const RECORDING_COUNTDOWN: Duration = Duration::from_secs(3);

type NativeCapture = CaptureController<FfmpegFlacRecorder, SystemClock>;

struct ActiveCapture {
    controller: NativeCapture,
    output_path: PathBuf,
}

struct ObservedCapture {
    state: CaptureState,
    started_at: Instant,
}

#[derive(Debug)]
pub enum RecordingRuntimeEvent {
    None,
    CountdownStarted,
    CaptureStarted {
        target: CaptureTarget,
    },
    Finalizing {
        target: CaptureTarget,
    },
    Cancelled,
    Finished {
        completed: CompletedCapture,
        path: PathBuf,
    },
    Failed {
        message: String,
    },
}

pub struct RecordingRuntime {
    capture: Option<ActiveCapture>,
    observed_capture: Option<ObservedCapture>,
    incoming: AudioTransferReceiver,
    temporary_dir: PathBuf,
    owned_files: Vec<PathBuf>,
    audio_paths_by_checksum: HashMap<String, PathBuf>,
}

impl RecordingRuntime {
    pub fn new() -> Self {
        let nonce = RECORDING_NONCE.fetch_add(1, Ordering::Relaxed);
        let temporary_dir = crate::media_binary::installation_temp_dir().join(format!(
            "coquerythmo-recording-{}-{nonce}",
            std::process::id()
        ));
        Self {
            capture: None,
            observed_capture: None,
            incoming: AudioTransferReceiver::default(),
            temporary_dir,
            owned_files: Vec::new(),
            audio_paths_by_checksum: HashMap::new(),
        }
    }

    pub fn capture_state(&self) -> Option<&CaptureState> {
        self.capture
            .as_ref()
            .map(|capture| capture.controller.state())
            .or_else(|| self.observed_capture.as_ref().map(|capture| &capture.state))
    }

    pub fn countdown_seconds_remaining(&self) -> Option<u32> {
        self.capture
            .as_ref()
            .and_then(|capture| capture.controller.countdown_seconds_remaining())
            .or_else(|| {
                let capture = self.observed_capture.as_ref()?;
                matches!(capture.state, CaptureState::Countdown { .. }).then(|| {
                    RECORDING_COUNTDOWN
                        .saturating_sub(capture.started_at.elapsed())
                        .as_secs_f64()
                        .ceil() as u32
                })
            })
    }

    pub fn live_waveform(&self) -> WaveformData {
        self.capture
            .as_ref()
            .map(|capture| capture.controller.live_waveform())
            .unwrap_or_default()
    }

    pub fn is_active(&self) -> bool {
        self.capture_state().is_some_and(|state| {
            matches!(
                state,
                CaptureState::Countdown { .. }
                    | CaptureState::Capturing { .. }
                    | CaptureState::Finalizing { .. }
            )
        })
    }

    pub fn begin_capture(
        &mut self,
        project: &RecordingProject,
        start_frame: i64,
        username: &str,
    ) -> Result<RecordingRuntimeEvent, RecordingError> {
        if self.capture_state().is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        let track_id = project
            .armed_track_id()
            .ok_or_else(|| RecordingError::Recorder("no recording track is armed".into()))?;

        // Propose IDs without touching durable allocator state. A cancelled
        // countdown therefore keeps strict transaction reconstruction equal.
        let target = project.propose_capture_target(track_id, start_frame)?;
        self.begin_capture_target(target, username)
    }

    /// Start a capture using IDs reserved by the online controller. Actors do
    /// not commit these IDs locally; the DA remaps and broadcasts the final
    /// transaction after receiving each participant's FLAC.
    pub fn begin_capture_target(
        &mut self,
        target: CaptureTarget,
        username: &str,
    ) -> Result<RecordingRuntimeEvent, RecordingError> {
        if self.capture_state().is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        let nonce = RECORDING_NONCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = recording_timestamp();
        let prefix = portable_username(username);
        let mut output_path = self
            .temporary_dir
            .join(format!("{prefix}_{timestamp}.flac"));
        if output_path.exists() {
            output_path = self
                .temporary_dir
                .join(format!("{prefix}_{timestamp}-{nonce}.flac"));
        }
        let recorder = FfmpegFlacRecorder::new(&output_path);
        let mut controller = CaptureController::new(recorder, SystemClock::default());
        controller.begin_countdown(target)?;
        self.capture = Some(ActiveCapture {
            controller,
            output_path,
        });
        Ok(RecordingRuntimeEvent::CountdownStarted)
    }

    pub fn begin_observed_capture(
        &mut self,
        target: CaptureTarget,
    ) -> Result<RecordingRuntimeEvent, RecordingError> {
        if self.capture_state().is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        self.observed_capture = Some(ObservedCapture {
            state: CaptureState::Countdown {
                target,
                deadline: RECORDING_COUNTDOWN,
            },
            started_at: Instant::now(),
        });
        Ok(RecordingRuntimeEvent::CountdownStarted)
    }

    pub fn cancel_or_stop(&mut self) -> Result<RecordingRuntimeEvent, RecordingError> {
        if let Some(observed) = self.observed_capture.take() {
            return match observed.state {
                CaptureState::Countdown { .. } => Ok(RecordingRuntimeEvent::Cancelled),
                CaptureState::Capturing { target, .. } | CaptureState::Finalizing { target } => {
                    Ok(RecordingRuntimeEvent::Finalizing { target })
                }
                CaptureState::Idle | CaptureState::Error { .. } => {
                    Err(RecordingError::CaptureNotActive)
                }
            };
        }
        let capture = self
            .capture
            .as_mut()
            .ok_or(RecordingError::CaptureNotActive)?;
        match capture.controller.cancel_or_stop()? {
            CaptureEvent::Cancelled => {
                self.capture = None;
                Ok(RecordingRuntimeEvent::Cancelled)
            }
            CaptureEvent::Finalizing { target } => Ok(RecordingRuntimeEvent::Finalizing { target }),
            event => Ok(map_event(event)),
        }
    }

    pub fn tick(&mut self) -> RecordingRuntimeEvent {
        if self.capture.is_none() {
            let Some(observed) = self.observed_capture.as_mut() else {
                return RecordingRuntimeEvent::None;
            };
            if let CaptureState::Countdown { target, .. } = observed.state {
                if observed.started_at.elapsed() >= RECORDING_COUNTDOWN {
                    observed.state = CaptureState::Capturing {
                        target,
                        started_at: RECORDING_COUNTDOWN,
                    };
                    return RecordingRuntimeEvent::CaptureStarted { target };
                }
            }
            return RecordingRuntimeEvent::None;
        }
        let event = match self.capture.as_mut() {
            Some(capture) => capture.controller.tick(),
            None => return RecordingRuntimeEvent::None,
        };
        match event {
            CaptureEvent::Finished(completed) => {
                let capture = self
                    .capture
                    .take()
                    .expect("a finished event requires an active capture");
                self.owned_files.push(capture.output_path.clone());
                RecordingRuntimeEvent::Finished {
                    completed,
                    path: capture.output_path,
                }
            }
            CaptureEvent::Failed { message } => {
                self.capture = None;
                RecordingRuntimeEvent::Failed { message }
            }
            other => map_event(other),
        }
    }

    pub fn begin_audio_receive(&mut self, metadata: AudioTransferMetadata) -> Result<(), String> {
        self.incoming.begin(metadata, &self.temporary_dir)
    }

    pub fn push_audio_chunk(
        &mut self,
        transfer_id: &str,
        index: usize,
        data_base64: &str,
    ) -> Result<(), String> {
        self.incoming.push_base64(transfer_id, index, data_base64)
    }

    pub fn finish_audio_receive(&mut self, transfer_id: &str) -> Result<ReceivedAudio, String> {
        let received = self.incoming.finish(transfer_id)?;
        self.owned_files.push(received.path.clone());
        Ok(received)
    }

    pub fn remember_audio_path(&mut self, checksum: &str, path: &Path) {
        self.audio_paths_by_checksum
            .insert(checksum.to_owned(), path.to_owned());
    }

    pub fn audio_path(&self, checksum: &str) -> Option<&PathBuf> {
        self.audio_paths_by_checksum.get(checksum)
    }

    pub fn owns(&self, path: &Path) -> bool {
        self.owned_files.iter().any(|candidate| candidate == path)
    }
}

fn recording_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}")
}

fn portable_username(username: &str) -> String {
    let username = username
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let username = username.trim().trim_matches(['.', ' ']);
    if username.is_empty() {
        "user".into()
    } else {
        username.chars().take(80).collect()
    }
}

impl Default for RecordingRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RecordingRuntime {
    fn drop(&mut self) {
        // Only exact files created by this runtime are removed. `remove_dir`
        // is deliberately non-recursive, so an unexpected path can never
        // broaden cleanup scope.
        for path in self.owned_files.drain(..) {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir(&self.temporary_dir);
    }
}

fn map_event(event: CaptureEvent) -> RecordingRuntimeEvent {
    match event {
        CaptureEvent::None => RecordingRuntimeEvent::None,
        CaptureEvent::CountdownStarted => RecordingRuntimeEvent::CountdownStarted,
        CaptureEvent::CaptureStarted { target } => RecordingRuntimeEvent::CaptureStarted { target },
        CaptureEvent::Finalizing { target } => RecordingRuntimeEvent::Finalizing { target },
        CaptureEvent::Finished(_) => unreachable!("finished events retain their output path"),
        CaptureEvent::Cancelled => RecordingRuntimeEvent::Cancelled,
        CaptureEvent::Failed { message } => RecordingRuntimeEvent::Failed { message },
    }
}

#[cfg(test)]
mod tests {
    use super::portable_username;
    use super::{RecordingRuntime, RecordingRuntimeEvent};
    use crate::recording::{AudioAssetId, AudioClipId, AudioTrackId, CaptureState, CaptureTarget};

    #[test]
    fn recording_file_prefix_is_portable() {
        assert_eq!(portable_username(" Comé/dien:* "), "Comé_dien__");
        assert_eq!(portable_username("..."), "user");
    }

    #[test]
    fn observed_capture_changes_view_without_opening_a_recorder() {
        let mut runtime = RecordingRuntime::new();
        let target = CaptureTarget {
            track_id: AudioTrackId::new(1),
            asset_id: AudioAssetId::new(2),
            clip_id: AudioClipId::new(3),
            start_frame: 48,
        };

        runtime.begin_observed_capture(target).unwrap();
        assert!(matches!(
            runtime.capture_state(),
            Some(CaptureState::Countdown { .. })
        ));
        assert!(runtime.capture.is_none());
        assert!(matches!(
            runtime.cancel_or_stop().unwrap(),
            RecordingRuntimeEvent::Cancelled
        ));
    }

    #[test]
    fn remembers_local_audio_by_checksum() {
        let mut runtime = RecordingRuntime::new();
        let path = runtime.temporary_dir.join("take.flac");

        runtime.remember_audio_path("checksum", &path);

        assert_eq!(runtime.audio_path("checksum"), Some(&path));
    }
}
