//! Read-only application context views used at composition boundaries.

use super::collaboration_service::CollaborationSession;
use super::playback_service::PlaybackSession;
use super::project_service::ProjectSession;

/// A narrow view of the active application state. It intentionally exposes
/// components, not the legacy `State`, UI internals or concrete commands.
pub struct AppContext<'a> {
    pub project: &'a ProjectSession,
    pub playback: &'a PlaybackSession,
    pub collaboration: &'a CollaborationSession,
}
