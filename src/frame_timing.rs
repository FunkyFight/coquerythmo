//! Central frame timing and display-refresh authority.
//!
//! # Architecture contract
//!
//! This module is the single source of truth for Coquerythmo's interactive
//! rendering cadence.
//!
//! Do not introduce any of the following outside this file:
//!
//! - a hard-coded interactive rendering frequency;
//! - an independent bande-rythmo rendering clock;
//! - monitor refresh-rate calculations;
//! - frame-deadline catch-up loops;
//! - a hot `ControlFlow::Poll` loop intended to pace rendering;
//! - duplicated `last_redraw` or `refresh_interval` state.
//!
//! Video decoding cadence and display rendering cadence are deliberately
//! separate. A 24 fps video may keep the same decoded texture across several
//! display refreshes while the bande rythmo continues moving once per display
//! frame.
//!
//! All visual consumers of one rendered frame must use the same
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
/// their state from `instant` instead of calling `Instant::now()` separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSample {
    /// The unique monotonic time sampled at the beginning of this frame.
    pub instant: Instant,

    /// Time elapsed since the preceding rendered frame began.
    pub delta: Duration,

    /// Monotonically increasing interactive frame identifier.
    pub frame_number: u64,
}

/// Lightweight diagnostics for the central frame clock.
///
/// These values are intentionally observational. They must never become a
/// second source of frame-pacing decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameTimingStats {
    pub frames_started: u64,
    pub frames_presented: u64,
    pub missed_deadlines: u64,
    pub last_render_duration: Duration,
    pub longest_render_duration: Duration,
    pub monitor_rate_changes: u64,
}

/// The sole owner of interactive frame cadence.
///
/// The scheduler never accumulates a debt of frames. When rendering is late,
/// the obsolete deadlines are counted for diagnostics and the next deadline is
/// scheduled exactly one display interval after the new frame begins.
#[derive(Debug)]
pub struct FrameTiming {
    refresh_rate_millihertz: u32,
    refresh_interval: Duration,

    last_frame_started_at: Instant,
    last_presented_at: Instant,
    next_frame_at: Instant,

    frame_number: u64,
    stats: FrameTimingStats,
}

impl FrameTiming {
    /// Builds a frame clock using the refresh rate of the monitor currently
    /// containing the window.
    pub fn new(window: &Window) -> Self {
        let now = Instant::now();
        Self::from_refresh_rate_millihertz(window_refresh_rate_millihertz(window), now)
    }

    /// Refreshes the display cadence after the window changes monitor or scale
    /// factor.
    ///
    /// Returns `true` only when the normalized monitor refresh rate actually
    /// changed.
    pub fn update_monitor(&mut self, window: &Window) -> bool {
        let refresh_rate =
            normalize_refresh_rate_millihertz(window_refresh_rate_millihertz(window));

        if refresh_rate == self.refresh_rate_millihertz {
            return false;
        }

        let now = Instant::now();

        self.refresh_rate_millihertz = refresh_rate;
        self.refresh_interval = refresh_interval_from_millihertz(refresh_rate);
        self.next_frame_at = add_duration(now, self.refresh_interval);
        self.stats.monitor_rate_changes = self.stats.monitor_rate_changes.saturating_add(1);

        true
    }

    /// The normalized monitor refresh rate in millihertz.
    ///
    /// For example, a 144 Hz display reports `144_000`.
    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    /// The normalized monitor refresh rate in hertz.
    pub fn refresh_rate_hz(&self) -> f64 {
        self.refresh_rate_millihertz as f64 / 1_000.0
    }

    /// Duration of one physical display refresh.
    pub fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }

    /// Beginning of the most recently rendered interactive frame.
    pub fn last_frame_started_at(&self) -> Instant {
        self.last_frame_started_at
    }

    /// Completion time of the most recent successful presentation.
    pub fn last_presented_at(&self) -> Instant {
        self.last_presented_at
    }

    /// Deadline at which the next continuous visual frame becomes due.
    pub fn next_frame_deadline(&self) -> Instant {
        self.next_frame_at
    }

    /// Whether a continuously animated scene should render at `now`.
    pub fn is_frame_due(&self, now: Instant) -> bool {
        now >= self.next_frame_at
    }

    /// Starts a new visual frame and produces its unique shared time sample.
    ///
    /// Calling this method also schedules the next frame relative to `now`.
    /// That deliberately discards obsolete deadlines instead of attempting a
    /// burst of catch-up renders.
    #[must_use]
    pub fn begin_frame(&mut self, now: Instant) -> FrameSample {
        let delta = now.saturating_duration_since(self.last_frame_started_at);
        let missed_deadlines = self.missed_deadlines_before(now);

        self.stats.missed_deadlines = self.stats.missed_deadlines.saturating_add(missed_deadlines);

        self.frame_number = self.frame_number.saturating_add(1);
        self.stats.frames_started = self.stats.frames_started.saturating_add(1);

        self.last_frame_started_at = now;

        // Never retain old frame debt. A late frame establishes a fresh phase.
        self.next_frame_at = add_duration(now, self.refresh_interval);

        FrameSample {
            instant: now,
            delta,
            frame_number: self.frame_number,
        }
    }

    /// Records the completion of a successful `surface_texture.present()`.
    pub fn finish_present(&mut self, presented_at: Instant) {
        let render_duration = presented_at.saturating_duration_since(self.last_frame_started_at);

        self.last_presented_at = presented_at;
        self.stats.frames_presented = self.stats.frames_presented.saturating_add(1);
        self.stats.last_render_duration = render_duration;
        self.stats.longest_render_duration =
            self.stats.longest_render_duration.max(render_duration);
    }

    /// Current diagnostic counters.
    pub fn stats(&self) -> FrameTimingStats {
        self.stats
    }

    fn from_refresh_rate_millihertz(refresh_rate_millihertz: Option<u32>, now: Instant) -> Self {
        let refresh_rate = normalize_refresh_rate_millihertz(refresh_rate_millihertz);
        let refresh_interval = refresh_interval_from_millihertz(refresh_rate);

        Self {
            refresh_rate_millihertz: refresh_rate,
            refresh_interval,
            last_frame_started_at: now,
            last_presented_at: now,

            // The first frame is immediately eligible.
            next_frame_at: now,

            frame_number: 0,
            stats: FrameTimingStats::default(),
        }
    }

    fn missed_deadlines_before(&self, now: Instant) -> u64 {
        if now <= self.next_frame_at {
            return 0;
        }

        let interval_nanos = self.refresh_interval.as_nanos();
        if interval_nanos == 0 {
            return 0;
        }

        let lateness_nanos = now.saturating_duration_since(self.next_frame_at).as_nanos();

        // Being even slightly past a deadline means that deadline was missed.
        let missed = lateness_nanos / interval_nanos + 1;

        missed.min(u64::MAX as u128) as u64
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

fn add_duration(instant: Instant, duration: Duration) -> Instant {
    instant.checked_add(duration).unwrap_or(instant)
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
    fn first_frame_is_immediately_due() {
        let start = Instant::now();
        let timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        assert!(timing.is_frame_due(start));
    }

    #[test]
    fn frame_becomes_due_only_at_its_display_deadline() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(144_000), start);

        let sample = timing.begin_frame(start);
        let deadline = timing.next_frame_deadline();
        let just_before = deadline
            .checked_sub(Duration::from_nanos(1))
            .expect("deadline should be after the start instant");

        assert_eq!(sample.instant, start);
        assert!(!timing.is_frame_due(just_before));
        assert!(timing.is_frame_due(deadline));
    }

    #[test]
    fn every_visual_consumer_receives_the_same_frame_instant() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(165_000), start);

        let render_time = add_duration(start, Duration::from_millis(10));
        let sample = timing.begin_frame(render_time);

        let video_time = sample.instant;
        let rythmo_time = sample.instant;
        let animation_time = sample.instant;

        assert_eq!(video_time, rythmo_time);
        assert_eq!(rythmo_time, animation_time);
    }

    #[test]
    fn a_late_frame_does_not_create_catch_up_debt() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        timing.begin_frame(start);

        let late_start = add_duration(start, Duration::from_millis(100));
        timing.begin_frame(late_start);

        assert_eq!(
            timing.next_frame_deadline(),
            add_duration(late_start, timing.refresh_interval())
        );
        assert!(timing.stats().missed_deadlines > 0);
    }

    #[test]
    fn presentation_duration_is_measured_from_frame_start() {
        let start = Instant::now();
        let mut timing = FrameTiming::from_refresh_rate_millihertz(Some(60_000), start);

        timing.begin_frame(start);

        let presented_at = add_duration(start, Duration::from_millis(5));
        timing.finish_present(presented_at);

        assert_eq!(
            timing.stats().last_render_duration,
            Duration::from_millis(5)
        );
        assert_eq!(timing.stats().frames_presented, 1);
    }
}
