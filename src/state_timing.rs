//! Display-paced State adapter.
//!
//! The existing State implementation remains the application owner. This
//! adapter replaces only the redraw policy used by the native event loop so a
//! 60/120/144 Hz surface is not fed by an unrelated fixed 240 Hz presentation
//! clock.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::window::Window;

use crate::accessibility::AccessibilityEvent;
use crate::application::job_service::SaveContinuation;
use crate::application::project_service::ProjectSession;
use crate::application::workspace_service::WorkspaceId;

#[path = "state.rs"]
mod implementation;

fn ensure_transaction_checkpoint_fps(session: &mut ProjectSession, fps: f64) -> bool {
    if !fps.is_finite()
        || fps <= 0.0
        || session.transaction_journal.checkpoint().source_fps == fps
    {
        return false;
    }

    session.replace_transaction_checkpoint(fps);
    true
}

pub struct State(implementation::State);

impl State {
    pub async fn new(
        window: Arc<Window>,
        accessibility_sender: Option<std::sync::mpsc::Sender<AccessibilityEvent>>,
    ) -> Self {
        Self(implementation::State::new(window, accessibility_sender).await)
    }

    /// Keep a fresh project's durable transaction checkpoint on the source
    /// video's actual timebase before edits start accumulating in the journal.
    pub fn load_video(&mut self, path: &Path) -> bool {
        let loaded = self.0.load_video(path);
        if loaded {
            let fps = self.0.fps();
            ensure_transaction_checkpoint_fps(&mut self.0.project_session, fps);
        }
        loaded
    }

    /// Repair stale checkpoints created before the source FPS was known. This
    /// also lets older affected projects save again without weakening archive
    /// integrity checks or trusting a journal expressed in another timebase.
    pub(crate) fn start_project_save(
        &mut self,
        path: PathBuf,
        source_video: PathBuf,
        proxy_video: Option<PathBuf>,
        font_asset: PathBuf,
        continuation: SaveContinuation,
    ) -> bool {
        let fps = self.0.fps();
        if ensure_transaction_checkpoint_fps(&mut self.0.project_session, fps) {
            log::warn!(
                "Rebuilt stale transaction checkpoint at {fps:.6} FPS before project save"
            );
        }
        self.0.start_project_save(
            path,
            source_video,
            proxy_video,
            font_asset,
            continuation,
        )
    }

    /// Present the interactive band at the monitor cadence. Its position still
    /// comes from a continuous f64 media clock, so interpolation is preserved
    /// without asking wgpu to present frames the monitor can never display.
    pub fn rythmo_refresh_interval(&self) -> Duration {
        self.display_refresh_interval()
    }

    pub fn needs_redraw_now(&self) -> bool {
        if self.is_video_playing() && self.active_workspace() == WorkspaceId::Rythmo {
            return redraw_due_at_display_rate(
                Instant::now(),
                self.render.last_redraw,
                self.display_refresh_interval(),
            );
        }
        self.0.needs_redraw_now()
    }

    pub fn next_wake_deadline(&self) -> Option<Instant> {
        if self.is_video_playing() && self.active_workspace() == WorkspaceId::Rythmo {
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
    use super::{ensure_transaction_checkpoint_fps, redraw_due_at_display_rate};
    use crate::application::edit_service::EditExecutor;
    use crate::application::project_service::ProjectSession;
    use std::time::{Duration, Instant};

    #[test]
    fn sixty_hz_display_is_not_redrawn_at_four_milliseconds() {
        let start = Instant::now();
        let interval = Duration::from_nanos(16_666_667);
        assert!(!redraw_due_at_display_rate(
            start + Duration::from_millis(4),
            start,
            interval,
        ));
        assert!(redraw_due_at_display_rate(
            start + Duration::from_millis(17),
            start,
            interval,
        ));
    }

    #[test]
    fn high_refresh_display_keeps_its_native_cadence() {
        let start = Instant::now();
        let interval = Duration::from_nanos(6_944_444);
        assert!(!redraw_due_at_display_rate(
            start + Duration::from_millis(6),
            start,
            interval,
        ));
        assert!(redraw_due_at_display_rate(
            start + Duration::from_millis(7),
            start,
            interval,
        ));
    }

    #[test]
    fn stale_checkpoint_is_rebuilt_from_the_current_project_at_video_fps() {
        let mut session = ProjectSession::new();
        let original_fps = session.transaction_journal.checkpoint().source_fps;
        let target_fps = if original_fps == 30.0 { 25.0 } else { 30.0 };
        EditExecutor::create_line(&mut session, 12, 24, 0.0, "Test".to_string());
        assert_eq!(session.transaction_journal.entries().len(), 1);

        assert!(ensure_transaction_checkpoint_fps(&mut session, target_fps));
        assert_eq!(
            session.transaction_journal.checkpoint().source_fps,
            target_fps
        );
        assert!(session.transaction_journal.entries().is_empty());
        assert_eq!(session.transaction_journal.cursor(), 0);
        assert_eq!(
            session
                .transaction_journal
                .replay(target_fps)
                .expect("the repaired checkpoint must replay")
                .line_count(),
            session.project.line_count()
        );
    }

    #[test]
    fn matching_checkpoint_is_left_untouched() {
        let mut session = ProjectSession::new();
        let fps = session.transaction_journal.checkpoint().source_fps;
        let hash = session.transaction_journal.checkpoint_hash();

        assert!(!ensure_transaction_checkpoint_fps(&mut session, fps));
        assert_eq!(session.transaction_journal.checkpoint_hash(), hash);
    }
}
