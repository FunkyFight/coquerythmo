//! Main-thread orchestration for microphone capture and incoming FLAC files.
//!
//! Durable timeline mutations remain in `recording`; this adapter owns only
//! transient CPAL/FFmpeg state and temporary files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio_transfer::{AudioTransferMetadata, AudioTransferReceiver, ReceivedAudio};
use crate::media_recording::FfmpegFlacRecorder;
use crate::recording::{
    CaptureController, CaptureEvent, CaptureState, CaptureTarget, CompletedCapture, RecordingError,
    RecordingProject, SystemClock, WaveformData,
};

static RECORDING_NONCE: AtomicU64 = AtomicU64::new(0);

type NativeCapture = CaptureController<FfmpegFlacRecorder, SystemClock>;

struct ActiveCapture {
    controller: NativeCapture,
    output_path: PathBuf,
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
    incoming: AudioTransferReceiver,
    temporary_dir: PathBuf,
    owned_files: Vec<PathBuf>,
}

impl RecordingRuntime {
    pub fn new() -> Self {
        let nonce = RECORDING_NONCE.fetch_add(1, Ordering::Relaxed);
        let temporary_dir = std::env::temp_dir().join(format!(
            "coquerythmo-recording-{}-{nonce}",
            std::process::id()
        ));
        Self {
            capture: None,
            incoming: AudioTransferReceiver::default(),
            temporary_dir,
            owned_files: Vec::new(),
        }
    }

    pub fn capture_state(&self) -> Option<&CaptureState> {
        self.capture
            .as_ref()
            .map(|capture| capture.controller.state())
    }

    pub fn countdown_seconds_remaining(&self) -> Option<u32> {
        self.capture
            .as_ref()
            .and_then(|capture| capture.controller.countdown_seconds_remaining())
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
    ) -> Result<RecordingRuntimeEvent, RecordingError> {
        if self.capture.is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        let track_id = project
            .armed_track_id()
            .ok_or_else(|| RecordingError::Recorder("no recording track is armed".into()))?;

        // Propose IDs without touching durable allocator state. A cancelled
        // countdown therefore keeps strict transaction reconstruction equal.
        let target = project.propose_capture_target(track_id, start_frame)?;
        self.begin_capture_target(target)
    }

    /// Start a capture using IDs reserved by the online controller. Actors do
    /// not commit these IDs locally; the DA remaps and broadcasts the final
    /// transaction after receiving each participant's FLAC.
    pub fn begin_capture_target(
        &mut self,
        target: CaptureTarget,
    ) -> Result<RecordingRuntimeEvent, RecordingError> {
        if self.capture.is_some() {
            return Err(RecordingError::CaptureBusy);
        }
        let nonce = RECORDING_NONCE.fetch_add(1, Ordering::Relaxed);
        let output_path = self.temporary_dir.join(format!("take-{nonce}.flac"));
        let recorder = FfmpegFlacRecorder::new(&output_path);
        let mut controller = CaptureController::new(recorder, SystemClock::default());
        controller.begin_countdown(target)?;
        self.capture = Some(ActiveCapture {
            controller,
            output_path,
        });
        Ok(RecordingRuntimeEvent::CountdownStarted)
    }

    pub fn cancel_or_stop(&mut self) -> Result<RecordingRuntimeEvent, RecordingError> {
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

    pub fn owns(&self, path: &Path) -> bool {
        self.owned_files.iter().any(|candidate| candidate == path)
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
