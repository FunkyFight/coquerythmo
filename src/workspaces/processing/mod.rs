//! Adapter for the audio processing workspace (vocal removal, stems).

use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

#[derive(Default)]
pub struct ProcessingWorkspace {
    needs_redraw: bool,
}

impl ProcessingWorkspace {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Workspace for ProcessingWorkspace {
    fn id(&self) -> WorkspaceId {
        WorkspaceId::Processing
    }

    fn input_context(&self) -> &'static str {
        "processing"
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
