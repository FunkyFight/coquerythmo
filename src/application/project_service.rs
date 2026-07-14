//! Project session state owned by application use cases.

use std::path::PathBuf;

use crate::command::CommandHistory;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;

/// Project data and its derived session state.
///
/// The session deliberately contains no window, UI, network or filesystem
/// adapter.  Those effects remain at the application/platform boundaries.
pub struct ProjectSession {
    pub project: Project,
    pub render_index: ProjectRenderIndex,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    pub history: CommandHistory,
}

impl ProjectSession {
    pub fn new() -> Self {
        Self {
            project: Project::new(),
            render_index: ProjectRenderIndex::new(),
            project_path: None,
            dirty: false,
            history: CommandHistory::new(),
        }
    }

}

impl Default for ProjectSession {
    fn default() -> Self {
        Self::new()
    }
}
