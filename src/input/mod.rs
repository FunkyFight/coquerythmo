//! Normalized keyboard input and context-sensitive shortcut routing.

pub mod binding;
pub mod context;
pub mod key;

// Keep the established shortcut table untouched and wrap it with detection and
// non-exported production-marker chords.
#[path = "router.rs"]
mod router_base;

pub mod router {
    pub use super::router_base::*;

    use super::binding::RepeatPolicy;
    use super::context::InputContext;
    use super::key::{KeyCode, Modifiers};
    use crate::application::command::UiAction;
    use crate::rythmo_special_markers::SpecialMarkerKind;

    pub fn existing_shortcuts() -> ShortcutRouter<UiAction> {
        let mut router = super::router_base::existing_shortcuts();
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        for context in [InputContext::Workspace, InputContext::Global] {
            router.bind(
                context,
                KeyCode::Space,
                ctrl,
                RepeatPolicy::PressOnly,
                UiAction::NudgeSelectedDetection { delta_ticks: 0 },
            );
        }

        let marker_chord = Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::NONE
        };
        for (key, kind) in [
            (KeyCode::Digit1, SpecialMarkerKind::Start),
            (KeyCode::Digit2, SpecialMarkerKind::Bip1000),
            (KeyCode::Digit3, SpecialMarkerKind::FirstImage),
            (KeyCode::Digit4, SpecialMarkerKind::LastImage),
        ] {
            router.bind(
                InputContext::Workspace,
                key,
                marker_chord,
                RepeatPolicy::PressOnly,
                crate::rythmo_special_markers::add_action(kind),
            );
        }
        router
    }
}
