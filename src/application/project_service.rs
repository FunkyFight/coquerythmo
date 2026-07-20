//! Project session state owned by application use cases.

use std::collections::BTreeMap;
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
    /// Identity of the last successfully loaded or saved project snapshot.
    pub huuid: Option<crate::project_metadata::Huuid>,
    pub dirty: bool,
    pub history: CommandHistory,
    /// Integrity-checked command journal used to rebuild the editable rythmo
    /// document independently from the transient undo stacks.
    pub transaction_journal: crate::project_metadata::TransactionJournal,
    /// Durable audio timeline shown by the Recording workspace.
    pub recording_project: crate::recording::RecordingProject,
    /// Forward recording operations, kept separate from rythmo undo/redo.
    pub recording_transactions: crate::recording::TransactionLog,
    /// Runtime locations for FLAC assets. Portable archives rewrite these
    /// paths to extracted entries when loading.
    pub recording_asset_paths: BTreeMap<crate::recording::AudioAssetId, PathBuf>,
    /// Monotonic save-snapshot marker for recording-only changes.
    pub recording_revision: u64,
    /// Keeps extracted bundle assets alive while media decoders use them.
    pub loaded_project: Option<crate::project_archive::LoadedProject>,
}

impl ProjectSession {
    fn new_recording_session(
        timeline_fps: f64,
    ) -> (
        crate::recording::RecordingProject,
        crate::recording::TransactionLog,
    ) {
        use crate::recording::{AudioTrack, RecordingOperation};

        let mut project = crate::recording::RecordingProject::new(timeline_fps)
            .expect("the default recording FPS must be valid");
        let mut transactions = crate::recording::TransactionLog::default();
        let track_id = project.allocate_track_id();
        transactions
            .append_and_apply(
                &mut project,
                RecordingOperation::Batch {
                    operations: vec![
                        RecordingOperation::AddTrack {
                            track: AudioTrack::new(
                                track_id,
                                crate::i18n::t("recording.track.default"),
                            ),
                        },
                        RecordingOperation::ArmTrack {
                            track_id: Some(track_id),
                        },
                    ],
                },
            )
            .expect("the default recording track must be valid");
        (project, transactions)
    }

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
        let project = Self::project_for_ui_language();
        let transaction_journal =
            crate::project_metadata::TransactionJournal::from_project(&project, 24.0)
                .expect("a fresh project must form a valid transaction checkpoint");
        let (recording_project, recording_transactions) = Self::new_recording_session(24.0);
        Self {
            project,
            render_index: ProjectRenderIndex::new(),
            project_path: None,
            huuid: None,
            dirty: false,
            history: CommandHistory::new(),
            transaction_journal,
            recording_project,
            recording_transactions,
            recording_asset_paths: BTreeMap::new(),
            recording_revision: 0,
            loaded_project: None,
        }
    }

    /// Reset both durable workspace documents while keeping their histories
    /// and asset maps internally consistent.
    pub fn reset_documents(&mut self, timeline_fps: f64) {
        self.project = Self::project_for_ui_language();
        self.transaction_journal =
            crate::project_metadata::TransactionJournal::from_project(&self.project, timeline_fps)
                .expect("a fresh project must form a valid transaction checkpoint");
        (self.recording_project, self.recording_transactions) =
            Self::new_recording_session(timeline_fps);
        self.recording_asset_paths.clear();
        self.recording_revision = 0;
    }

    pub fn reset_recording_document(&mut self, timeline_fps: f64) {
        (self.recording_project, self.recording_transactions) =
            Self::new_recording_session(timeline_fps);
        self.recording_asset_paths.clear();
        self.recording_revision = 0;
    }

    pub fn replace_transaction_checkpoint(&mut self, fps: f64) {
        self.transaction_journal =
            crate::project_metadata::TransactionJournal::from_project(&self.project, fps)
                .expect("an in-memory project must form a valid transaction checkpoint");
    }

    pub fn mark_recording_changed(&mut self) {
        self.recording_revision = self.recording_revision.saturating_add(1);
        self.dirty = true;
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
