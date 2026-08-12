//! Adapter for the audio-only voiceline cutting workspace.

use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

#[derive(Default)]
pub struct VoicelinesWorkspace {
    needs_redraw: bool,
}

impl VoicelinesWorkspace {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Workspace for VoicelinesWorkspace {
    fn id(&self) -> WorkspaceId {
        WorkspaceId::Voicelines
    }

    fn input_context(&self) -> &'static str {
        "voicelines"
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
