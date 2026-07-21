//! Normalized keyboard input and context-sensitive shortcut routing.

pub mod binding;
pub mod context;
pub mod key;

// Keep the established shortcut table untouched and wrap it with the detection
// audition chord. This makes Ctrl+Space resolve before the event loop's generic
// Space playback fallback, while plain Space and D keep their existing jobs.
#[path = "router.rs"]
mod router_base;

pub mod router {
    pub use super::router_base::*;

    use super::binding::RepeatPolicy;
    use super::context::InputContext;
    use super::key::{KeyCode, Modifiers};
    use crate::application::command::UiAction;

    pub fn existing_shortcuts() -> ShortcutRouter<UiAction> {
        let mut router = super::router_base::existing_shortcuts();
        let chord = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let create_sync_chord = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        for context in [
            InputContext::Workspace,
            InputContext::TextEditing,
            InputContext::Global,
        ] {
            router.bind(
                context,
                KeyCode::Space,
                chord,
                RepeatPolicy::PressOnly,
                UiAction::NudgeSelectedDetection { delta_ticks: 0 },
            );
            router.bind(
                context,
                KeyCode::Space,
                create_sync_chord,
                RepeatPolicy::PressOnly,
                UiAction::AddSyncPointAtPlayhead,
            );
        }
        router
    }
}

#[cfg(test)]
mod tests {
    use super::context::{InputContext, InputContextStack};
    use super::key::{InputWindow, KeyCode, KeyStroke, Modifiers};
    use crate::application::command::UiAction;
    use winit::keyboard::KeyLocation;

    #[test]
    fn ctrl_shift_space_creates_sync_point_in_workspace_and_text_editor() {
        let router = super::router::existing_shortcuts();
        let stroke = KeyStroke {
            key: KeyCode::Space,
            physical_key: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            },
            pressed: true,
            repeat: false,
            window: InputWindow::Main,
        };
        for context in [InputContext::Workspace, InputContext::TextEditing] {
            assert_eq!(
                router.resolve(&stroke, &InputContextStack::new([context])),
                Some(&UiAction::AddSyncPointAtPlayhead)
            );
        }
    }
}
