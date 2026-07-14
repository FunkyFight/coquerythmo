//! Adapter for the existing rythmo band workspace.

use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

pub mod view;

/// The sole product workspace. Its transient interaction state remains in the
/// existing UI while the adapter establishes the future extension boundary.
pub struct RythmoWorkspace {
    needs_redraw: bool,
}

impl RythmoWorkspace {
    pub fn new() -> Self {
        Self {
            needs_redraw: false,
        }
    }
}

impl Default for RythmoWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace for RythmoWorkspace {
    fn id(&self) -> WorkspaceId {
        WorkspaceId::Rythmo
    }

    fn input_context(&self) -> &'static str {
        "rythmo"
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
