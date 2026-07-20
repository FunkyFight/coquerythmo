//! Application-facing workspace host boundary.

/// Stable identity for a product workspace registered in the application shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceId {
    Rythmo,
    Recording,
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

pub struct WorkspaceHost {
    workspaces: Vec<Box<dyn Workspace>>,
    active: usize,
}

impl WorkspaceHost {
    pub fn new(workspaces: Vec<Box<dyn Workspace>>, initial: WorkspaceId) -> Self {
        assert!(!workspaces.is_empty(), "at least one workspace is required");
        let active = workspaces
            .iter()
            .position(|workspace| workspace.id() == initial)
            .expect("initial workspace must be registered");
        Self { workspaces, active }
    }

    pub fn active(&self) -> &dyn Workspace {
        self.workspaces[self.active].as_ref()
    }

    pub fn active_mut(&mut self) -> &mut dyn Workspace {
        self.workspaces[self.active].as_mut()
    }

    pub fn active_id(&self) -> WorkspaceId {
        self.active().id()
    }

    /// Activate a registered workspace. Returns true when the active identity changed.
    pub fn activate(&mut self, id: WorkspaceId) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id() == id)
        else {
            return false;
        };
        if index == self.active {
            return false;
        }
        self.active = index;
        true
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
        let mut host = WorkspaceHost::new(
            vec![Box::new(FakeWorkspace { redraw: false })],
            WorkspaceId::Rythmo,
        );
        assert_eq!(host.active().id(), WorkspaceId::Rythmo);
        assert!(!host.active().needs_redraw());
        assert!(host
            .active_mut()
            .handle_content_event(WorkspaceCommand::RequestRedraw));
        assert!(host.active().needs_redraw());
    }

    struct IdentifiedWorkspace(WorkspaceId);

    impl Workspace for IdentifiedWorkspace {
        fn id(&self) -> WorkspaceId {
            self.0
        }

        fn input_context(&self) -> &'static str {
            match self.0 {
                WorkspaceId::Rythmo => "rythmo",
                WorkspaceId::Recording => "recording",
            }
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

    #[test]
    fn workspace_host_switches_registered_workspaces_idempotently() {
        let mut host = WorkspaceHost::new(
            vec![
                Box::new(IdentifiedWorkspace(WorkspaceId::Rythmo)),
                Box::new(IdentifiedWorkspace(WorkspaceId::Recording)),
            ],
            WorkspaceId::Rythmo,
        );

        assert!(host.activate(WorkspaceId::Recording));
        assert_eq!(host.active_id(), WorkspaceId::Recording);
        assert!(!host.activate(WorkspaceId::Recording));
        assert_eq!(host.active_id(), WorkspaceId::Recording);
        assert!(host.activate(WorkspaceId::Rythmo));
    }
}
