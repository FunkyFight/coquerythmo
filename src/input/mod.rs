//! Normalized keyboard input and context-sensitive shortcut routing.

pub mod binding;
pub mod context;
pub mod key;

pub const TEXT_EMOTION_SHORTCUT_SENTINEL: &str = "__coquerythmo_open_text_emotions__";

// Keep the established shortcut table untouched and wrap it with the detection
// audition chord and the text-emotion palette. This makes semantic chords
// resolve before the event loop's generic key fallbacks.
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

        let text_emotion_chord = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        for context in [InputContext::Workspace, InputContext::TextEditing] {
            router.bind(
                context,
                KeyCode::Character('e'),
                text_emotion_chord,
                RepeatPolicy::PressOnly,
                UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Opened {
                    label: super::TEXT_EMOTION_SHORTCUT_SENTINEL.to_string(),
                }),
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

    fn stroke(key: KeyCode, modifiers: Modifiers) -> KeyStroke {
        KeyStroke {
            key,
            physical_key: None,
            location: KeyLocation::Standard,
            modifiers,
            pressed: true,
            repeat: false,
            window: InputWindow::Main,
        }
    }

    #[test]
    fn ctrl_shift_space_creates_sync_point_in_workspace_and_text_editor() {
        let router = super::router::existing_shortcuts();
        let stroke = stroke(
            KeyCode::Space,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            },
        );
        for context in [InputContext::Workspace, InputContext::TextEditing] {
            assert_eq!(
                router.resolve(&stroke, &InputContextStack::new([context])),
                Some(&UiAction::AddSyncPointAtPlayhead)
            );
        }
    }

    #[test]
    fn alt_e_opens_text_emotions_in_workspace_and_text_editor() {
        let router = super::router::existing_shortcuts();
        let stroke = stroke(
            KeyCode::Character('e'),
            Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
        );
        for context in [InputContext::Workspace, InputContext::TextEditing] {
            let action = router
                .resolve(&stroke, &InputContextStack::new([context]))
                .expect("Alt+E should be bound");
            assert!(matches!(
                action,
                UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Opened { label })
                    if label == super::TEXT_EMOTION_SHORTCUT_SENTINEL
            ));
        }
    }
}
