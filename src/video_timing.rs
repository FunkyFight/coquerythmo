//! Playback timing adapter.
//!
//! The decoder still owns media IO and texture uploads. This adapter only
//! changes the visual clock exposed to the rest of the application so the
//! scrolling band cannot run several source frames ahead of the video texture
//! when FFmpeg briefly stalls.

use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[path = "video.rs"]
mod implementation;

pub use implementation::{AudioTrack, VideoFrame};

#[derive(Debug)]
struct VisualPhaseLock {
    last_output: f64,
    last_decoded_frame: i64,
    last_sample: Option<Instant>,
    was_playing: bool,
}

impl Default for VisualPhaseLock {
    fn default() -> Self {
        Self {
            last_output: 0.0,
            last_decoded_frame: 0,
            last_sample: None,
            was_playing: false,
        }
    }
}

impl VisualPhaseLock {
    fn sample(
        &mut self,
        raw_clock_frame: f64,
        decoded_frame: i64,
        fps: f64,
        playing: bool,
        now: Instant,
    ) -> f64 {
        let decoded = decoded_frame.max(0) as f64;
        if !playing {
            self.last_output = decoded;
            self.last_decoded_frame = decoded_frame;
            self.last_sample = Some(now);
            self.was_playing = false;
            return decoded;
        }

        // The displayed texture represents decoded_frame. Let the band move
        // continuously toward the next source frame, but never beyond it until
        // the decoder has actually supplied more video.
        let target = raw_clock_frame
            .max(decoded)
            .min(decoded + 1.0)
            .max(0.0);

        if !self.was_playing
            || decoded_frame < self.last_decoded_frame
            || raw_clock_frame + 0.5 < self.last_output
        {
            self.last_output = target;
            self.last_decoded_frame = decoded_frame;
            self.last_sample = Some(now);
            self.was_playing = true;
            return target;
        }

        let elapsed = self
            .last_sample
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or(Duration::ZERO);
        // Follow the normal clock exactly when decoding is healthy. If several
        // frames arrive after a stall, absorb the correction over a few display
        // refreshes instead of making the whole band jump in one presentation.
        let max_advance = elapsed.as_secs_f64() * fps.max(1.0) * 1.5 + 0.05;
        let output = target
            .max(self.last_output)
            .min(self.last_output + max_advance.max(0.0));

        self.last_output = output;
        self.last_decoded_frame = decoded_frame;
        self.last_sample = Some(now);
        self.was_playing = true;
        output
    }
}

/// Public player used by the application. Every media operation delegates to
/// the existing implementation; only the render-time clock is phase-locked.
pub struct VideoPlayer {
    inner: implementation::VideoPlayer,
    visual_phase_lock: Mutex<VisualPhaseLock>,
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self {
            inner: implementation::VideoPlayer::new(),
            visual_phase_lock: Mutex::new(VisualPhaseLock::default()),
        }
    }

    pub fn current_frame_for_render(&self) -> f64 {
        let now = Instant::now();
        let raw_clock_frame = self.inner.current_frame_for_render();
        let decoded_frame = self.inner.current_frame();
        let fps = self.inner.fps();
        let playing = self.inner.is_playing();
        let mut phase_lock = self
            .visual_phase_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        phase_lock.sample(raw_clock_frame, decoded_frame, fps, playing, now)
    }
}

impl Default for VideoPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for VideoPlayer {
    type Target = implementation::VideoPlayer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for VideoPlayer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::VisualPhaseLock;
    use std::time::{Duration, Instant};

    #[test]
    fn visual_clock_cannot_run_past_the_next_decoded_frame() {
        let start = Instant::now();
        let mut clock = VisualPhaseLock::default();
        assert_eq!(clock.sample(0.0, 0, 24.0, true, start), 0.0);
        let held = clock.sample(3.0, 0, 24.0, true, start + Duration::from_millis(100));
        assert!((held - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn decoder_recovery_is_absorbed_over_multiple_presentations() {
        let start = Instant::now();
        let mut clock = VisualPhaseLock::default();
        clock.sample(0.0, 0, 24.0, true, start);
        clock.sample(3.0, 0, 24.0, true, start + Duration::from_millis(100));

        let recovered = clock.sample(
            4.0,
            4,
            24.0,
            true,
            start + Duration::from_millis(116),
        );
        assert!(recovered > 1.0);
        assert!(recovered < 4.0);
    }

    #[test]
    fn paused_clock_matches_the_displayed_frame_exactly() {
        let now = Instant::now();
        let mut clock = VisualPhaseLock::default();
        assert_eq!(clock.sample(12.8, 12, 24.0, false, now), 12.0);
    }
}
