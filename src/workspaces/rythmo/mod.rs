//! Adapter for the existing rythmo band workspace.

use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

pub mod view;

/// Narrow bridge for sibling controller modules that need the synchronization
/// drag helpers implemented by the view adapter.
pub(crate) mod detection_ui {
    pub(crate) use super::view::{
        active_sync_syllable_edit_range, begin_sync_syllable_drag, clear_sync_syllable_drag,
        finish_sync_syllable_drag,
    };
}

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
