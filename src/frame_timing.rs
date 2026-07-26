//! Central frame sampling and display-refresh metadata.
//!
//! # Architecture contract
//!
//! This module does not schedule, pace or request rendered frames.
//!
//! Actual interactive presentation cadence belongs exclusively to the GPU
//! swapchain through FIFO VSync. Do not attempt to predict display VBlank
//! deadlines with `Instant`, `WaitUntil` or a fixed rendering frequency.
//!
//! This module is responsible only for:
//!
//! - detecting the monitor refresh rate as useful metadata;
//! - producing one shared monotonic time sample per rendered frame;
//! - measuring the elapsed time between rendered frames;
//! - recording passive rendering diagnostics.
//!
//! Video decoding cadence and display rendering cadence remain separate. A
//! 24 fps video may keep the same decoded texture across several display
//! refreshes while the bande rythmo is resampled from the continuous playback
//! clock for every rendered frame.
//!
//! Every time-dependent visual consumer in one rendered frame must use the same
//! [`FrameSample::instant`].

use std::time::{Duration, Instant};

use winit::window::Window;

const DEFAULT_REFRESH_RATE_MILLIHERTZ: u32 = 60_000;
const MIN_REFRESH_RATE_MILLIHERTZ: u32 = 30_000;
const MAX_REFRESH_RATE_MILLIHERTZ: u32 = 360_000;

/// One immutable time sample shared by every visual consumer of a rendered
/// frame.
///
/// Video playback, bande-rythmo positioning and UI animations must all derive
/// their state from `instant` instead of independently calling
/// `Instant::now()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSample {
    /// Unique monotonic time sampled at the beginning of this frame's visual
    /// work.
    pub instant: Instant,

    /// Time elapsed since the preceding rendered frame began.
    ///
    /// This is zero for the first rendered frame.
    pub delta: Duration,

    /// Monotonically increasing interactive frame identifier.
    pub frame_number: u64,
}

/// Passive diagnostics collected from observed rendered frames.
///
/// None of these values may be used to decide when the next frame should be
/// rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTimingStats {
    pub frames_started: u64,
    pub frames_presented: u64,

    /// Approximate number of display refreshes skipped between observed frame
    /// starts.
    ///
    /// This is diagnostic only. It is not a scheduling deadline counter.
    pub estimated_missed_refreshes: u64,

    pub last_render_duration: Duration,
    pub longest_render_duration: Duration,
    pub monitor_rate_changes: u64,
}

/// Shared frame clock and display metadata.
///
/// This type deliberately owns no next-frame deadline. FIFO presentation is
/// the only authority controlling when frames reach the display.
#[derive(Debug)]
pub struct FrameTiming {
    refresh_rate_millihertz: u32,
    refresh_interval: Duration,

    last_frame_started_at: Instant,
    last_presented_at: Instant,

    frame_number: u64,
    stats: FrameTimingStats,
}

impl FrameTiming {
    /// Builds frame timing metadata using the refresh rate of the monitor
    /// currently containing the window.
    pub fn new(window: &Window) -> Self {
        let now = Instant::now();

        Self::from_refresh_rate_millihertz(window_refresh_rate_millihertz(window), now)
    }

    /// Refreshes display metadata after the window changes monitor or scale
    /// factor.
    ///
    /// This does not alter rendering phase or create a future frame deadline.
    ///
    /// Returns `true` only when the normalized refresh rate changed.
    pub fn update_monitor(&mut self, window: &Window) -> bool {
        let refresh_rate =
            normalize_refresh_rate_millihertz(window_refresh_rate_millihertz(window));

        if refresh_rate == self.refresh_rate_millihertz {
            return false;
        }

        self.refresh_rate_millihertz = refresh_rate;
        self.refresh_interval = refresh_interval_from_millihertz(refresh_rate);

        self.stats.monitor_rate_changes = self.stats.monitor_rate_changes.saturating_add(1);

        true
    }

    /// Normalized monitor refresh rate in millihertz.
    ///
    /// For example, a 144 Hz display reports `144_000`.
    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    /// Normalized monitor refresh rate in hertz.
    pub fn refresh_rate_hz(&self) -> f64 {
        self.refresh_rate_millihertz as f64 / 1_000.0
    }

    /// Approximate duration of one physical display refresh.
    ///
    /// This value is metadata for diagnostics and input throttling. It must not
    /// be used to predict a swapchain presentation deadline.
    pub fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }

    /// Beginning of the most recently sampled rendered frame.
    pub fn last_frame_started_at(&self) -> Instant {
        self.last_frame_started_at
    }

    /// Completion time of the most recent successful presentation.
    pub fn last_presented_at(&self) -> Instant {
        self.last_presented_at
    }

    /// Starts a new visual frame and produces its unique shared time sample.
    ///
    /// This method records observed timing only. It does not calculate or store
    /// the time at which another frame should begin.
    #[must_use]
    pub fn begin_frame(&mut self, now: Instant) -> FrameSample {
        let delta = if self.frame_number == 0 {
            Duration::ZERO
        } else {
            now.saturating_duration_since(self.last_frame_started_at)
        };

        if self.frame_number > 0 {
            let missed_refreshes = estimate_missed_refreshes(delta, self.refresh_interval);

            self.stats.estimated_missed_refreshes = self
                .stats
                .estimated_missed_refreshes
                .saturating_add(missed_refreshes);
        }

        self.frame_number = self.frame_number.saturating_add(1);
        self.stats.frames_started = self.stats.frames_started.saturating_add(1);

        self.last_frame_started_at = now;

        FrameSample {
            instant: now,
            delta,
            frame_number: self.frame_number,
        }
    }

    /// Records completion of a successful `surface_texture.present()`.
    pub fn finish_present(&mut self, presented_at: Instant) {
        let render_duration = presented_at.saturating_duration_since(self.last_frame_started_at);

        self.last_presented_at = presented_at;
        self.stats.frames_presented = self.stats.frames_presented.saturating_add(1);
        self.stats.last_render_duration = render_duration;
        self.stats.longest_render_duration =
            self.stats.longest_render_duration.max(render_duration);
    }

    /// Current passive diagnostic counters.
    pub fn stats(&self) -> FrameTimingStats {
        self.stats
    }

    fn from_refresh_rate_millihertz(refresh_rate_millihertz: Option<u32>, now: Instant) -> Self {
        let refresh_rate = normalize_refresh_rate_millihertz(refresh_rate_millihertz);

        Self {
            refresh_rate_millihertz: refresh_rate,
            refresh_interval: refresh_interval_from_millihertz(refresh_rate),

            last_frame_started_at: now,
            last_presented_at: now,

            frame_number: 0,
            stats: FrameTimingStats::default(),
        }
    }
}

fn window_refresh_rate_millihertz(window: &Window) -> Option<u32> {
    window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
}

fn normalize_refresh_rate_millihertz(refresh_rate_millihertz: Option<u32>) -> u32 {
    refresh_rate_millihertz
        .unwrap_or(DEFAULT_REFRESH_RATE_MILLIHERTZ)
        .clamp(MIN_REFRESH_RATE_MILLIHERTZ, MAX_REFRESH_RATE_MILLIHERTZ)
}

fn refresh_interval_from_millihertz(refresh_rate_millihertz: u32) -> Duration {
    Duration::from_secs_f64(1_000.0 / refresh_rate_millihertz as f64)
}

/// Estimates skipped display refreshes from the observed distance between two
/// rendered frame starts.
///
/// Rounding to the nearest refresh prevents tiny scheduler jitter from being
/// reported as a skipped display frame.
fn estimate_missed_refreshes(frame_delta: Duration, refresh_interval: Duration) -> u64 {
    let interval_nanos = refresh_interval.as_nanos();

    if interval_nanos == 0 {
        return 0;
    }

    let rounded_intervals =
        frame_delta.as_nanos().saturating_add(interval_nanos / 2) / interval_nanos;

    rounded_intervals.saturating_sub(1).min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_tracks_common_real_monitor_rates() {
        assert_eq!(
            refresh_interval_from_millihertz(60_000).as_nanos(),
            16_666_667
        );
        assert_eq!(
            refresh_interval_from_millihertz(120_000).as_nanos(),
            8_333_333
        );
        assert_eq!(
            refresh_interval_from_millihertz(144_000).as_nanos(),
            6_944_444
        );
        assert_eq!(
            refresh_interval_from_millihertz(165_000).as_nanos(),
            6_060_606
        );
        assert_eq!(
            refresh_interval_from_millihertz(240_000).as_nanos(),
            4_166_667
        );
    }

    #[test]
    fn missing_monitor_information_falls_back_to_60_hz() {
        assert_eq!(
            normalize_refresh_rate_millihertz(None),
            DEFAULT_REFRESH_RATE_MILLIHERTZ
        );
    }

    #[test]
    fn implausible_monitor_rates_are_clamped() {
        assert_eq!(
            normalize_refresh_rate_millihertz(Some(1_000)),
            MIN_REFRESH_RATE_MILLIHERTZ
        );
        assert_eq!(
            normalize_refresh_rate_millihertz(Some(1_000_000)),
            MAX_REFRESH_RATE_MILLIHERTZ
        );
    }

    #[test]
    fn first_frame_has_zero_delta() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        let sample = timing.begin_frame(start);

        assert_eq!(sample.instant, start);
        assert_eq!(sample.delta, Duration::ZERO);
        assert_eq!(sample.frame_number, 1);
    }

    #[test]
    fn frame_delta_is_measured_between_observed_frame_starts() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(144_000), start);

        timing.begin_frame(start);

        let second_start = start
            .checked_add(Duration::from_millis(7))
            .expect("test instant should be representable");

        let sample = timing.begin_frame(second_start);

        assert_eq!(sample.instant, second_start);
        assert_eq!(sample.delta, Duration::from_millis(7));
        assert_eq!(sample.frame_number, 2);
    }

    #[test]
    fn every_visual_consumer_receives_the_same_frame_instant() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(165_000), start);

        let sample = timing.begin_frame(start);

        let video_time = sample.instant;
        let rythmo_time = sample.instant;
        let animation_time = sample.instant;

        assert_eq!(video_time, rythmo_time);
        assert_eq!(rythmo_time, animation_time);
    }

    #[test]
    fn tiny_scheduler_jitter_is_not_counted_as_a_missed_refresh() {
        let interval = refresh_interval_from_millihertz(60_000);

        assert_eq!(
            estimate_missed_refreshes(Duration::from_millis(17), interval,),
            0
        );
    }

    #[test]
    fn long_frame_gap_is_recorded_without_creating_a_deadline() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        timing.begin_frame(start);

        let late_start = start
            .checked_add(Duration::from_millis(100))
            .expect("test instant should be representable");

        timing.begin_frame(late_start);

        assert_eq!(timing.stats().estimated_missed_refreshes, 5);
    }

    #[test]
    fn presentation_duration_is_measured_from_frame_start() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        timing.begin_frame(start);

        let presented_at = start
            .checked_add(Duration::from_millis(5))
            .expect("test instant should be representable");

        timing.finish_present(presented_at);

        assert_eq!(
            timing.stats().last_render_duration,
            Duration::from_millis(5)
        );
        assert_eq!(timing.stats().frames_presented, 1);
    }
}
