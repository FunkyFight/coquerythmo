//! Display-paced State adapter.
//!
//! The existing State implementation remains the application owner. This
//! adapter replaces only the redraw policy used by the native event loop so a
//! 60/120/144 Hz surface is not fed by an unrelated fixed 240 Hz presentation
//! clock.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::accessibility::AccessibilityEvent;
use crate::application::workspace_service::WorkspaceId;

#[path = "state.rs"]
mod implementation;

pub struct State(implementation::State);

impl State {
    pub async fn new(
        window: Arc<Window>,
        accessibility_sender: Option<std::sync::mpsc::Sender<AccessibilityEvent>>,
    ) -> Self {
        Self(implementation::State::new(window, accessibility_sender).await)
    }

    /// The text-emotion palette is a native modal surface even though it lives
    /// beside the legacy UI modal host. This keeps arrows, Home/End, Enter and
    /// Escape inside the palette before workspace shortcuts can consume them.
    pub fn captures_modal_input(&self) -> bool {
        crate::text_emotion_foreground::captures_input() || self.0.captures_modal_input()
    }

    /// Present the interactive band at the monitor cadence. Its position still
    /// comes from a continuous f64 media clock, so interpolation is preserved
    /// without asking wgpu to present frames the monitor can never display.
    pub fn rythmo_refresh_interval(&self) -> Duration {
        self.display_refresh_interval()
    }

    pub fn needs_redraw_now(&self) -> bool {
        if (self.is_video_playing() || crate::text_emotion::has_any())
            && self.active_workspace() == WorkspaceId::Rythmo
        {
            return redraw_due_at_display_rate(
                Instant::now(),
                self.render.last_redraw,
                self.display_refresh_interval(),
            );
        }
        self.0.needs_redraw_now()
    }

    pub fn next_wake_deadline(&self) -> Option<Instant> {
        if (self.is_video_playing() || crate::text_emotion::has_any())
            && self.active_workspace() == WorkspaceId::Rythmo
        {
            return Some(self.render.last_redraw + self.display_refresh_interval());
        }
        self.0.next_wake_deadline()
    }
}

impl Deref for State {
    type Target = implementation::State;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn redraw_due_at_display_rate(now: Instant, last_redraw: Instant, interval: Duration) -> bool {
    now.saturating_duration_since(last_redraw) >= interval
}

#[cfg(test)]
mod tests {
    use super::redraw_due_at_display_rate;
    use std::time::{Duration, Instant};

    #[test]
    fn sixty_hz_display_is_not_redrawn_at_four_milliseconds() {
        let start = Instant::now();
        let interval = Duration::from_nanos(16_666_667);
        assert!(!redraw_due_at_display_rate(
            start + Duration::from_millis(4),
            start,
            interval
        ));
        assert!(redraw_due_at_display_rate(
            start + Duration::from_millis(17),
            start,
            interval
        ));
    }

    #[test]
    fn high_refresh_display_keeps_its_native_cadence() {
        let start = Instant::now();
        let interval = Duration::from_nanos(6_944_444);
        assert!(!redraw_due_at_display_rate(
            start + Duration::from_millis(6),
            start,
            interval
        ));
        assert!(redraw_due_at_display_rate(
            start + Duration::from_millis(7),
            start,
            interval
        ));
    }
}
