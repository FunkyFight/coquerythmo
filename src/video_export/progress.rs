#![allow(clippy::items_after_test_module)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::types::StdinWriteError;
use super::EXPORT_CANCELLED_MESSAGE;

pub fn is_cancelled_error(error: &str) -> bool {
    error == EXPORT_CANCELLED_MESSAGE
        || error
            .strip_suffix(EXPORT_CANCELLED_MESSAGE)
            .is_some_and(|prefix| prefix.ends_with(": "))
}

pub(super) fn export_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_export_errors_still_report_cancellation() {
        assert!(is_cancelled_error(EXPORT_CANCELLED_MESSAGE));
        assert!(is_cancelled_error("Video / Français: Export canceled"));
        assert!(!is_cancelled_error("Export canceled after a codec failure"));
    }
}

pub(super) fn check_export_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if export_cancelled(cancel) {
        Err(EXPORT_CANCELLED_MESSAGE.into())
    } else {
        Ok(())
    }
}

pub(super) struct ProgressState {
    pub(super) callback: Mutex<Box<dyn FnMut(f32) + Send>>,
    pub(super) reported: AtomicU32,
}

pub(super) type ProgressCallback = Arc<ProgressState>;

pub(super) fn emit_progress(progress_cb: &ProgressCallback, progress: f32) {
    let progress = progress.clamp(0.0, 1.0);
    let mut current = progress_cb.reported.load(Ordering::Relaxed);
    loop {
        if progress <= f32::from_bits(current) {
            return;
        }
        match progress_cb.reported.compare_exchange(
            current,
            progress.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                if let Ok(mut cb) = progress_cb.callback.lock() {
                    cb(progress);
                }
                return;
            }
            Err(next) => current = next,
        }
    }
}

pub(super) fn check_stdin_cancel(cancel: &AtomicBool) -> Result<(), StdinWriteError> {
    if export_cancelled(cancel) {
        Err(StdinWriteError::cancelled())
    } else {
        Ok(())
    }
}

pub(super) fn report_baked_progress(
    completed_frames: u64,
    total_frames: u64,
    ffmpeg_progress: &AtomicU32,
    progress_cb: &ProgressCallback,
) {
    if total_frames == 0 {
        return;
    }
    let raw = (completed_frames.min(total_frames) as f32 / total_frames as f32).clamp(0.0, 1.0);
    let writer_progress = map_progress(raw, 0.01, 0.985);
    let pipe_progress = f32::from_bits(ffmpeg_progress.load(Ordering::Relaxed));
    emit_progress(
        progress_cb,
        writer_progress.max(pipe_progress).clamp(0.01, 0.985),
    );
}

pub(super) fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(super) fn avg_ms(duration: Duration, frames: u64) -> f64 {
    if frames == 0 {
        0.0
    } else {
        ms(duration) / frames as f64
    }
}

pub(super) fn fps(frames: u64, duration: Duration) -> f64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        0.0
    } else {
        frames as f64 / secs
    }
}

pub(super) fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

pub(super) fn map_progress(progress: f32, start: f32, end: f32) -> f32 {
    start + progress.clamp(0.0, 1.0) * (end - start)
}

pub(super) fn store_progress_max(progress: &AtomicU32, value: f32) -> bool {
    let value = value.clamp(0.0, 1.0);
    let mut current = progress.load(Ordering::Relaxed);
    loop {
        if value <= f32::from_bits(current) {
            return false;
        }
        match progress.compare_exchange(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(next) => current = next,
        }
    }
}

pub(super) fn ffmpeg_progress_from_line(line: &str, duration_secs: f64) -> Option<f32> {
    if duration_secs <= 0.0 {
        return None;
    }

    if let Some(raw) = line
        .strip_prefix("out_time_ms=")
        .or_else(|| line.strip_prefix("out_time_us="))
    {
        let micros = raw.trim().parse::<f64>().ok()?;
        return Some((micros / 1_000_000.0 / duration_secs) as f32);
    }

    if let Some(raw) = line.strip_prefix("out_time=") {
        let secs = parse_ffmpeg_time(raw.trim())?;
        return Some((secs / duration_secs) as f32);
    }

    None
}

pub(super) fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}
