use crate::application::workspace_service::{Workspace, WorkspaceCommand, WorkspaceId};

pub struct ComicDubsWorkspace;

impl ComicDubsWorkspace {
    pub fn new() -> Self {
        Self
    }
}

impl Workspace for ComicDubsWorkspace {
    fn id(&self) -> WorkspaceId {
        WorkspaceId::ComicDubs
    }

    fn input_context(&self) -> &'static str {
        "comic_dubs"
    }

    fn toolbar_model(&self) -> &'static [WorkspaceCommand] {
        &[]
    }

    fn handle_content_event(&mut self, _event: WorkspaceCommand) -> bool {
        false
    }

    fn needs_redraw(&self) -> bool {
        false
    }
}
