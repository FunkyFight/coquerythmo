//! Playback timing adapter.
//!
//! The decoder still owns media IO and texture uploads. This adapter changes
//! only the visual clock exposed to the rest of the application so the
//! scrolling band cannot run several source frames ahead of the displayed
//! video texture when FFmpeg briefly stalls.

use std::ops::{Deref, DerefMut};

#[path = "video.rs"]
mod implementation;

pub use self::implementation::{AudioTrack, VideoFrame};

/// Public player used by the application. Every media operation delegates to
/// the existing implementation; only the render-time clock is phase-locked to
/// the last frame actually consumed from the decoder.
pub struct VideoPlayer {
    inner: implementation::VideoPlayer,
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self {
            inner: implementation::VideoPlayer::new(),
        }
    }

    pub fn current_frame_for_render(&self) -> f64 {
        phase_locked_render_frame(
            self.inner.current_frame_for_render(),
            self.inner.current_frame(),
            self.inner.is_playing(),
        )
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

fn phase_locked_render_frame(raw_clock_frame: f64, decoded_frame: i64, playing: bool) -> f64 {
    let decoded = decoded_frame.max(0) as f64;
    if !playing || !raw_clock_frame.is_finite() {
        return decoded;
    }

    // The uploaded texture represents decoded_frame. Fractional movement inside
    // its interval is valid, but advancing into later source frames while the
    // decoder queue is empty makes the band visibly detach from the picture.
    raw_clock_frame.clamp(decoded, decoded + 1.0)
}

#[cfg(test)]
mod tests {
    use super::phase_locked_render_frame;

    #[test]
    fn visual_clock_cannot_run_past_the_next_decoded_frame() {
        assert_eq!(phase_locked_render_frame(3.4, 0, true), 1.0);
        assert_eq!(phase_locked_render_frame(13.2, 12, true), 13.0);
    }

    #[test]
    fn healthy_decoder_keeps_fractional_interpolation() {
        assert!((phase_locked_render_frame(12.4, 12, true) - 12.4).abs() < 1.0e-9);
        assert!((phase_locked_render_frame(12.8, 12, true) - 12.8).abs() < 1.0e-9);
    }

    #[test]
    fn paused_clock_matches_the_displayed_frame_exactly() {
        assert_eq!(phase_locked_render_frame(12.8, 12, false), 12.0);
    }
}
