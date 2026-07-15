//! Deterministic shortcut resolution.

use super::binding::{Binding, KeyPattern, RepeatPolicy};
use super::context::{InputContext, InputContextStack};
use super::key::{KeyCode, KeyStroke, Modifiers};
use crate::application::command::{TextCommand, UiAction};

#[derive(Debug, Clone, Default)]
pub struct ShortcutRouter<C> {
    bindings: Vec<Binding<C>>,
}

impl<C> ShortcutRouter<C> {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn bind(
        &mut self,
        context: InputContext,
        key: KeyCode,
        modifiers: Modifiers,
        repeat: RepeatPolicy,
        command: C,
    ) {
        self.bindings.push(Binding {
            context,
            pattern: KeyPattern {
                key,
                modifiers,
                repeat,
            },
            command,
        });
    }

    pub fn resolve<'a>(
        &'a self,
        stroke: &KeyStroke,
        contexts: &InputContextStack,
    ) -> Option<&'a C> {
        contexts.iter().find_map(|context| {
            if (*context == InputContext::SecondaryWindow)
                != (stroke.window == super::key::InputWindow::Secondary)
            {
                return None;
            }
            self.bindings
                .iter()
                .find(|binding| binding.context == *context && binding.pattern.matches(stroke))
                .map(|binding| &binding.command)
        })
    }
}

/// Initial bindings for the existing global and studio shortcuts.
///
/// The event loop supplies the active context stack; this table only declares
/// the semantic command and its repeat policy.
pub fn existing_shortcuts() -> ShortcutRouter<UiAction> {
    let mut router = ShortcutRouter::new();
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let ctrl_shift = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::NONE
    };

    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('k'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::SplitDialogue,
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('a'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::SelectAll,
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('c'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::Text(TextCommand::Copy),
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('x'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::Text(TextCommand::Cut),
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('v'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::Text(TextCommand::Paste),
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::Character('z'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::Text(TextCommand::Undo),
    );
    for (key, command) in [
        (KeyCode::ArrowLeft, TextCommand::CursorLeft),
        (KeyCode::ArrowRight, TextCommand::CursorRight),
        (KeyCode::ArrowUp, TextCommand::CursorUp),
        (KeyCode::ArrowDown, TextCommand::CursorDown),
        (KeyCode::Delete, TextCommand::Delete),
    ] {
        router.bind(
            InputContext::TextEditing,
            key,
            Modifiers::NONE,
            RepeatPolicy::PressAndRepeat,
            UiAction::Text(command),
        );
    }
    router.bind(
        InputContext::TextEditing,
        KeyCode::ArrowLeft,
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
        RepeatPolicy::PressAndRepeat,
        UiAction::Text(TextCommand::SelectLeft),
    );
    router.bind(
        InputContext::TextEditing,
        KeyCode::ArrowRight,
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
        RepeatPolicy::PressAndRepeat,
        UiAction::Text(TextCommand::SelectRight),
    );

    router.bind(
        InputContext::VideoLoaded,
        KeyCode::F5,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::ShowStudioWarning,
    );
    router.bind(
        InputContext::Studio,
        KeyCode::Escape,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::ExitStudioMode,
    );
    router.bind(
        InputContext::Studio,
        KeyCode::Space,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::TogglePlayPause,
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::Space,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::TogglePlayPause,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Tab,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::ToggleActiveAudio,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('k'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::SplitDialogue,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('s'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::QuickSave,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('c'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::CopySelectedLine,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('x'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::CutSelectedLine,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('v'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::PasteLine,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('z'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::Undo,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('z'),
        ctrl_shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::Redo,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('n'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::NewProject,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('i'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::OpenLinesPanel,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('p'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::OpenRolesPanel,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Delete,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::DeleteSelected,
    );
    router.bind(
        InputContext::Global,
        KeyCode::Character('a'),
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::SelectAll,
    );
    router.bind(
        InputContext::SecondaryWindow,
        KeyCode::Space,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::TogglePlayPause,
    );
    router.bind(
        InputContext::SecondaryWindow,
        KeyCode::Escape,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::CloseSecondaryDisplay,
    );
    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::key::InputWindow;

    fn stroke(key: KeyCode, modifiers: Modifiers, repeat: bool) -> KeyStroke {
        KeyStroke {
            key,
            modifiers,
            pressed: true,
            repeat,
            window: InputWindow::Main,
        }
    }

    #[test]
    fn first_active_context_wins() {
        let mut router = ShortcutRouter::new();
        router.bind(
            InputContext::TextEditing,
            KeyCode::Character('c'),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
            RepeatPolicy::PressOnly,
            "copy-text",
        );
        router.bind(
            InputContext::Global,
            KeyCode::Character('c'),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
            RepeatPolicy::PressOnly,
            "copy-line",
        );

        let contexts = InputContextStack::new([
            InputContext::TextEditing,
            InputContext::Workspace,
            InputContext::Global,
        ]);
        assert_eq!(
            router.resolve(
                &stroke(
                    KeyCode::Character('c'),
                    Modifiers {
                        ctrl: true,
                        ..Modifiers::NONE
                    },
                    false,
                ),
                &contexts,
            ),
            Some(&"copy-text")
        );
    }

    #[test]
    fn same_stroke_can_have_a_different_fake_context_command() {
        let mut router = ShortcutRouter::new();
        router.bind(
            InputContext::Workspace,
            KeyCode::Character('x'),
            Modifiers::NONE,
            RepeatPolicy::PressOnly,
            "workspace-x",
        );
        router.bind(
            InputContext::Modal,
            KeyCode::Character('x'),
            Modifiers::NONE,
            RepeatPolicy::PressOnly,
            "modal-x",
        );

        let stroke = stroke(KeyCode::Character('x'), Modifiers::NONE, false);
        assert_eq!(
            router.resolve(
                &stroke,
                &InputContextStack::new([InputContext::Modal, InputContext::Global]),
            ),
            Some(&"modal-x")
        );
        assert_eq!(
            router.resolve(
                &stroke,
                &InputContextStack::new([InputContext::Workspace, InputContext::Global]),
            ),
            Some(&"workspace-x")
        );
    }

    #[test]
    fn press_only_binding_does_not_repeat() {
        let mut router = ShortcutRouter::new();
        router.bind(
            InputContext::Global,
            KeyCode::F5,
            Modifiers::NONE,
            RepeatPolicy::PressOnly,
            "studio-warning",
        );
        let contexts = InputContextStack::new([InputContext::Global]);
        assert!(router
            .resolve(&stroke(KeyCode::F5, Modifiers::NONE, true), &contexts)
            .is_none());
    }

    #[test]
    fn existing_shortcuts_characterization_table() {
        let router = existing_shortcuts();
        let global = InputContextStack::new([InputContext::Global]);
        let workspace = InputContextStack::new([InputContext::Workspace, InputContext::Global]);
        let studio = InputContextStack::new([InputContext::Studio]);

        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        let resolve = |key, modifiers, contexts: &InputContextStack, repeat| {
            router.resolve(&stroke(key, modifiers, repeat), contexts)
        };

        assert_eq!(resolve(KeyCode::F5, Modifiers::NONE, &global, false), None);
        assert_eq!(
            resolve(
                KeyCode::F5,
                Modifiers::NONE,
                &InputContextStack::new([InputContext::VideoLoaded]),
                false
            ),
            Some(&UiAction::ShowStudioWarning)
        );
        assert_eq!(
            resolve(KeyCode::Escape, Modifiers::NONE, &studio, false),
            Some(&UiAction::ExitStudioMode)
        );
        assert_eq!(
            resolve(KeyCode::Space, Modifiers::NONE, &workspace, false),
            Some(&UiAction::TogglePlayPause)
        );
        assert_eq!(
            resolve(KeyCode::Tab, Modifiers::NONE, &global, false),
            Some(&UiAction::ToggleActiveAudio)
        );
        assert_eq!(
            resolve(KeyCode::Character('k'), ctrl, &global, false),
            Some(&UiAction::SplitDialogue)
        );
        assert_eq!(
            resolve(KeyCode::Character('s'), ctrl, &global, false),
            Some(&UiAction::QuickSave)
        );
        assert_eq!(
            resolve(KeyCode::Character('n'), ctrl, &global, false),
            Some(&UiAction::NewProject)
        );
        assert_eq!(
            resolve(KeyCode::Character('i'), ctrl, &global, false),
            Some(&UiAction::OpenLinesPanel)
        );
        assert_eq!(
            resolve(KeyCode::Character('p'), ctrl, &global, false),
            Some(&UiAction::OpenRolesPanel)
        );
        assert_eq!(resolve(KeyCode::Character('i'), ctrl, &global, true), None);
        assert_eq!(resolve(KeyCode::Character('p'), ctrl, &global, true), None);
        assert_eq!(
            resolve(KeyCode::Character('a'), ctrl, &global, false),
            Some(&UiAction::SelectAll)
        );
        assert_eq!(
            resolve(KeyCode::Character('c'), ctrl, &global, false),
            Some(&UiAction::CopySelectedLine)
        );
        assert_eq!(
            resolve(KeyCode::Character('x'), ctrl, &global, false),
            Some(&UiAction::CutSelectedLine)
        );
        assert_eq!(
            resolve(KeyCode::Character('v'), ctrl, &global, false),
            Some(&UiAction::PasteLine)
        );
        assert_eq!(
            resolve(KeyCode::Character('z'), ctrl, &global, false),
            Some(&UiAction::Undo)
        );
        assert_eq!(resolve(KeyCode::Character('z'), ctrl, &global, true), None);
        let text = InputContextStack::new([InputContext::TextEditing]);
        assert_eq!(
            resolve(KeyCode::Character('c'), ctrl, &text, false),
            Some(&UiAction::Text(TextCommand::Copy))
        );
        assert_eq!(resolve(KeyCode::Character('z'), ctrl, &text, true), None);
        assert_eq!(
            resolve(KeyCode::ArrowLeft, Modifiers::NONE, &text, false),
            Some(&UiAction::Text(TextCommand::CursorLeft))
        );
        assert_eq!(
            resolve(
                KeyCode::ArrowRight,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
                &text,
                false,
            ),
            Some(&UiAction::Text(TextCommand::SelectRight))
        );
        assert_eq!(
            resolve(KeyCode::Character('z'), ctrl_shift, &global, false),
            Some(&UiAction::Redo)
        );
        assert_eq!(
            resolve(KeyCode::Delete, Modifiers::NONE, &global, false),
            Some(&UiAction::DeleteSelected)
        );
        assert_eq!(
            resolve(KeyCode::ArrowLeft, Modifiers::NONE, &global, false),
            None
        );

        let secondary = InputContextStack::new([InputContext::SecondaryWindow]);
        let secondary_stroke = KeyStroke {
            key: KeyCode::Space,
            modifiers: Modifiers::NONE,
            pressed: true,
            repeat: false,
            window: InputWindow::Secondary,
        };
        assert_eq!(
            router.resolve(&secondary_stroke, &secondary),
            Some(&UiAction::TogglePlayPause)
        );
        assert_eq!(
            router.resolve(&stroke(KeyCode::Space, Modifiers::NONE, false), &secondary,),
            None
        );
    }
}
