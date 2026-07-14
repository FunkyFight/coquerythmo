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
    /// Keeps extracted bundle assets alive while media decoders use them.
    pub loaded_project: Option<crate::project_archive::LoadedProject>,
}

impl ProjectSession {
    pub(crate) fn project_for_ui_language() -> Project {
        let language_code = crate::config::language_or_default();
        let language_name = match language_code
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "fr" => "Français".to_string(),
            "en" => "English".to_string(),
            "es" => "Español".to_string(),
            _ => language_code.clone(),
        };
        Project::new_with_language(language_name, language_code)
    }

    pub fn new() -> Self {
        Self {
            project: Self::project_for_ui_language(),
            render_index: ProjectRenderIndex::new(),
            project_path: None,
            dirty: false,
            history: CommandHistory::new(),
            loaded_project: None,
        }
    }
}

impl Default for ProjectSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_language_uses_the_ui_language() {
        let expected = crate::config::language_or_default();
        let session = ProjectSession::new();

        assert_eq!(session.project.active_language().code, expected);
        assert_eq!(session.project.language_count(), 1);
    }

    #[test]
    fn fresh_project_factory_uses_one_ui_language() {
        let project = ProjectSession::project_for_ui_language();
        assert_eq!(project.language_count(), 1);
        assert_eq!(
            project.active_language().code,
            crate::config::language_or_default()
        );
    }
}
