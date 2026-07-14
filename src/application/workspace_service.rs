//! Application-facing workspace host boundary.

/// Stable identity for the one product workspace currently registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceId {
    Rythmo,
}

/// Commands a workspace can ask the shell to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCommand {
    RequestRedraw,
}

/// Small contract for future workspace adapters. It contains no `State`, UI
/// widget tree or project collection, and therefore cannot create visible tabs
/// as a side effect of registration.
pub trait Workspace {
    fn id(&self) -> WorkspaceId;
    fn input_context(&self) -> &'static str;
    fn toolbar_model(&self) -> &'static [WorkspaceCommand];
    fn handle_content_event(&mut self, _event: WorkspaceCommand) -> bool;
    fn needs_redraw(&self) -> bool;
}

pub struct WorkspaceHost<W: Workspace> {
    active: W,
}

impl<W: Workspace> WorkspaceHost<W> {
    pub fn new(active: W) -> Self {
        Self { active }
    }

    pub fn active(&self) -> &W {
        &self.active
    }

    pub fn active_mut(&mut self) -> &mut W {
        &mut self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeWorkspace {
        redraw: bool,
    }

    impl Workspace for FakeWorkspace {
        fn id(&self) -> WorkspaceId {
            WorkspaceId::Rythmo
        }

        fn input_context(&self) -> &'static str {
            "fake"
        }

        fn toolbar_model(&self) -> &'static [WorkspaceCommand] {
            &[]
        }

        fn handle_content_event(&mut self, event: WorkspaceCommand) -> bool {
            self.redraw = matches!(event, WorkspaceCommand::RequestRedraw);
            self.redraw
        }

        fn needs_redraw(&self) -> bool {
            self.redraw
        }
    }

    #[test]
    fn workspace_host_exposes_one_active_workspace() {
        let mut host = WorkspaceHost::new(FakeWorkspace { redraw: false });
        assert_eq!(host.active().id(), WorkspaceId::Rythmo);
        assert!(!host.active().needs_redraw());
        assert!(host
            .active_mut()
            .handle_content_event(WorkspaceCommand::RequestRedraw));
        assert!(host.active().needs_redraw());
    }
}
