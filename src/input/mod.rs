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

    use crate::application::command::UiAction;
    use super::binding::RepeatPolicy;
    use super::context::InputContext;
    use super::key::{KeyCode, Modifiers};

    pub fn existing_shortcuts() -> ShortcutRouter<UiAction> {
        let mut router = super::router_base::existing_shortcuts();
        router.bind(
            InputContext::Workspace,
            KeyCode::Space,
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
            RepeatPolicy::PressOnly,
            UiAction::NudgeSelectedDetection { delta_ticks: 0 },
        );
        router
    }
}
