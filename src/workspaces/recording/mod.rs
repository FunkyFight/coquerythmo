//! Adapter for the recording workspace shell.
//!
//! The mini-DAW and capture services live behind later application/domain
//! boundaries. This adapter only registers the workspace identity and redraw
//! contract needed by the shared shell.

use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

#[derive(Default)]
pub struct RecordingWorkspace {
    needs_redraw: bool,
}

impl RecordingWorkspace {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Workspace for RecordingWorkspace {
    fn id(&self) -> WorkspaceId {
        WorkspaceId::Recording
    }

    fn input_context(&self) -> &'static str {
        "recording"
    }

    fn toolbar_model(&self) -> &'static [WorkspaceCommand] {
        &[]
    }

    fn handle_content_event(&mut self, event: WorkspaceCommand) -> bool {
        if matches!(event, WorkspaceCommand::RequestRedraw) {
            self.needs_redraw = true;
        }
        self.needs_redraw
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
}
