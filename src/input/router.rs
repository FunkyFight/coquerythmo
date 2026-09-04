//! Deterministic shortcut resolution.

use super::binding::{Binding, KeyPattern, RepeatPolicy};
use super::context::{InputContext, InputContextStack};
use super::key::{KeyCode, KeyStroke, Modifiers};
use crate::application::command::{TextCommand, ToolMode, ToolbarDropdown, UiAction};
use crate::rythmo_line::MarkerKind;

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
                pressed: true,
            },
            command,
        });
    }

    pub fn bind_release(
        &mut self,
        context: InputContext,
        key: KeyCode,
        modifiers: Modifiers,
        command: C,
    ) {
        self.bindings.push(Binding {
            context,
            pattern: KeyPattern {
                key,
                modifiers,
                repeat: RepeatPolicy::PressOnly,
                pressed: false,
            },
            command,
        });
    }

    /// Read-only view of the declared bindings, used by the contextual
    /// shortcut panel to list what is available in the active context.
    pub fn bindings(&self) -> &[Binding<C>] {
        &self.bindings
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

/// Initial bindings for the existing global and workspace shortcuts.
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
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };

    for context in [InputContext::TextEditing, InputContext::Workspace] {
        router.bind(
            context,
            KeyCode::Character('e'),
            alt,
            RepeatPolicy::PressOnly,
            UiAction::OpenTextEmotionMenu,
        );
    }

    router.bind(
        InputContext::Accessibility,
        KeyCode::Character('n'),
        ctrl_shift,
        RepeatPolicy::PressOnly,
        UiAction::ToggleScreenReader,
    );

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
        InputContext::Workspace,
        KeyCode::Space,
        Modifiers::NONE,
        RepeatPolicy::PressAndRepeat,
        UiAction::TogglePlayPause,
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::Escape,
        Modifiers::NONE,
        RepeatPolicy::PressOnly,
        UiAction::ClearLineSelection,
    );
    for (key, command) in [
        (KeyCode::Numpad1, UiAction::AddMarker(MarkerKind::Boucle)),
        (KeyCode::Numpad2, UiAction::AddMarker(MarkerKind::Out)),
        (
            KeyCode::Numpad3,
            UiAction::AddMarker(MarkerKind::SceneChange),
        ),
        (
            KeyCode::Numpad4,
            UiAction::OpenDropdown(ToolbarDropdown::Respirations),
        ),
        (
            KeyCode::Numpad5,
            UiAction::OpenDropdown(ToolbarDropdown::Reactions),
        ),
        (KeyCode::Numpad6, UiAction::AddNote),
        (
            KeyCode::Numpad7,
            UiAction::AddMarker(MarkerKind::LiaisonLeft),
        ),
        (
            KeyCode::Numpad8,
            UiAction::AddMarker(MarkerKind::LiaisonRight),
        ),
        (KeyCode::Numpad9, UiAction::ToggleKaraokeForSelection),
    ] {
        router.bind(
            InputContext::Workspace,
            key,
            Modifiers::NONE,
            RepeatPolicy::PressOnly,
            command,
        );
    }
    for (key, track) in [
        (KeyCode::Numpad1, 0usize),
        (KeyCode::Numpad2, 1usize),
        (KeyCode::Numpad3, 2usize),
        (KeyCode::Numpad4, 3usize),
    ] {
        router.bind(
            InputContext::Workspace,
            key,
            ctrl,
            RepeatPolicy::PressOnly,
            UiAction::CreateLineAtTrack { track },
        );
    }
    for (key, command) in [
        (KeyCode::Enter, UiAction::SelectLineAtPlayhead),
        (
            KeyCode::Character('i'),
            UiAction::SetSelectedLineStartAtPlayhead,
        ),
        (
            KeyCode::Character('o'),
            UiAction::SetSelectedLineEndAtPlayhead,
        ),
        (KeyCode::Character('t'), UiAction::StartEditingSelectedLine),
        (
            KeyCode::Character('p'),
            UiAction::StartEditingSelectedCharacter,
        ),
        (
            KeyCode::Character('c'),
            UiAction::SetToolMode(ToolMode::Select),
        ),
    ] {
        router.bind(
            InputContext::Workspace,
            key,
            Modifiers::NONE,
            RepeatPolicy::PressOnly,
            command,
        );
    }
    router.bind(
        InputContext::Workspace,
        KeyCode::Character('d'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::SetToolMode(ToolMode::Draw),
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::Character('q'),
        Modifiers::NONE,
        RepeatPolicy::PressOnly,
        UiAction::BeginKeyboardPan { direction: -1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::Character('d'),
        Modifiers::NONE,
        RepeatPolicy::PressOnly,
        UiAction::BeginKeyboardPan { direction: 1 },
    );
    router.bind_release(
        InputContext::Workspace,
        KeyCode::Character('q'),
        Modifiers::NONE,
        UiAction::EndKeyboardPan,
    );
    router.bind_release(
        InputContext::Workspace,
        KeyCode::Character('d'),
        Modifiers::NONE,
        UiAction::EndKeyboardPan,
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowLeft,
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::PrevFrame,
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowRight,
        ctrl,
        RepeatPolicy::PressAndRepeat,
        UiAction::NextFrame,
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowLeft,
        ctrl_shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::NudgeSelectedLines { delta_frames: -1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowRight,
        ctrl_shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::NudgeSelectedLines { delta_frames: 1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowLeft,
        shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::NavigateLines { direction: -1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowRight,
        shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::NavigateLines { direction: 1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowUp,
        Modifiers::NONE,
        RepeatPolicy::PressOnly,
        UiAction::MoveSelectedLineTrack { direction: -1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowDown,
        Modifiers::NONE,
        RepeatPolicy::PressOnly,
        UiAction::MoveSelectedLineTrack { direction: 1 },
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowUp,
        shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::AdjustVolume(0.05),
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::ArrowDown,
        shift,
        RepeatPolicy::PressAndRepeat,
        UiAction::AdjustVolume(-0.05),
    );
    router.bind(
        InputContext::Workspace,
        KeyCode::NumpadSubtract,
        shift,
        RepeatPolicy::PressOnly,
        UiAction::ToggleMute,
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
        KeyCode::Tab,
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::ToggleActiveAudio,
    );
    router.bind(
        InputContext::Recording,
        KeyCode::Tab,
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::RecordingToggleSharedAudio,
    );
    router.bind(
        InputContext::Recording,
        KeyCode::Character('l'),
        ctrl,
        RepeatPolicy::PressOnly,
        UiAction::RecordingCycleLanguage,
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
        ctrl_shift,
        RepeatPolicy::PressOnly,
        UiAction::PasteLineWithTrackCharacter,
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
    for (key, modifiers, command) in [
        (
            KeyCode::Character('i'),
            ctrl_shift,
            UiAction::ImportSubtitles,
        ),
        (KeyCode::Character('r'), ctrl_shift, UiAction::RestoreBackup),
        (KeyCode::Character('r'), ctrl, UiAction::OpenRecentProjects),
        (KeyCode::Delete, ctrl, UiAction::CloseProject),
        (KeyCode::Character('m'), ctrl, UiAction::OpenExportModal),
        (
            KeyCode::Character('p'),
            ctrl_shift,
            UiAction::OpenProxyModal,
        ),
        (KeyCode::Character('o'), ctrl, UiAction::OpenProjectSettings),
    ] {
        router.bind(
            InputContext::Global,
            key,
            modifiers,
            RepeatPolicy::PressOnly,
            command,
        );
    }
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
            physical_key: None,
            location: winit::keyboard::KeyLocation::Standard,
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
            "one-shot",
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
        let recording = InputContextStack::new([InputContext::Recording, InputContext::Global]);

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
            resolve(KeyCode::F5, Modifiers::NONE, &recording, false),
            None
        );
        assert_eq!(
            resolve(KeyCode::Space, Modifiers::NONE, &recording, false),
            None
        );
        assert_eq!(
            resolve(KeyCode::Space, Modifiers::NONE, &workspace, false),
            Some(&UiAction::TogglePlayPause)
        );
        assert_eq!(resolve(KeyCode::Tab, Modifiers::NONE, &global, false), None);
        assert_eq!(
            resolve(KeyCode::Tab, ctrl, &global, false),
            Some(&UiAction::ToggleActiveAudio)
        );
        assert_eq!(
            resolve(KeyCode::Tab, ctrl, &recording, false),
            Some(&UiAction::RecordingToggleSharedAudio)
        );
        assert_eq!(
            resolve(KeyCode::Character('l'), ctrl, &recording, false),
            Some(&UiAction::RecordingCycleLanguage)
        );
        assert_eq!(resolve(KeyCode::Tab, ctrl, &global, true), None);
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
            None
        );
        assert_eq!(
            resolve(KeyCode::ArrowLeft, Modifiers::NONE, &global, false),
            None
        );

        let secondary = InputContextStack::new([InputContext::SecondaryWindow]);
        let secondary_stroke = KeyStroke {
            key: KeyCode::Space,
            physical_key: None,
            location: winit::keyboard::KeyLocation::Standard,
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

    #[test]
    fn accessibility_shortcuts_table_is_complete() {
        let router = existing_shortcuts();
        let global = InputContextStack::new([InputContext::Global]);
        let workspace = InputContextStack::new([InputContext::Workspace, InputContext::Global]);
        let accessibility = InputContextStack::new([InputContext::Accessibility]);
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };

        for (key, modifiers, contexts, expected) in [
            (
                KeyCode::Character('i'),
                ctrl_shift,
                &global,
                UiAction::ImportSubtitles,
            ),
            (
                KeyCode::Character('r'),
                ctrl_shift,
                &global,
                UiAction::RestoreBackup,
            ),
            (
                KeyCode::Character('r'),
                ctrl,
                &global,
                UiAction::OpenRecentProjects,
            ),
            (KeyCode::Delete, ctrl, &global, UiAction::CloseProject),
            (
                KeyCode::Character('m'),
                ctrl,
                &global,
                UiAction::OpenExportModal,
            ),
            (
                KeyCode::Character('p'),
                ctrl_shift,
                &global,
                UiAction::OpenProxyModal,
            ),
            (
                KeyCode::Character('o'),
                ctrl,
                &global,
                UiAction::OpenProjectSettings,
            ),
            (
                KeyCode::Character('n'),
                ctrl_shift,
                &accessibility,
                UiAction::ToggleScreenReader,
            ),
            (
                KeyCode::ArrowLeft,
                ctrl_shift,
                &workspace,
                UiAction::NudgeSelectedLines { delta_frames: -1 },
            ),
            (
                KeyCode::ArrowRight,
                ctrl_shift,
                &workspace,
                UiAction::NudgeSelectedLines { delta_frames: 1 },
            ),
            (KeyCode::ArrowLeft, ctrl, &workspace, UiAction::PrevFrame),
            (KeyCode::ArrowRight, ctrl, &workspace, UiAction::NextFrame),
            (
                KeyCode::ArrowLeft,
                shift,
                &workspace,
                UiAction::NavigateLines { direction: -1 },
            ),
            (
                KeyCode::ArrowRight,
                shift,
                &workspace,
                UiAction::NavigateLines { direction: 1 },
            ),
            (
                KeyCode::ArrowUp,
                shift,
                &workspace,
                UiAction::AdjustVolume(0.05),
            ),
            (
                KeyCode::ArrowDown,
                shift,
                &workspace,
                UiAction::AdjustVolume(-0.05),
            ),
            (
                KeyCode::NumpadSubtract,
                shift,
                &workspace,
                UiAction::ToggleMute,
            ),
            (
                KeyCode::Numpad1,
                ctrl,
                &workspace,
                UiAction::CreateLineAtTrack { track: 0 },
            ),
            (
                KeyCode::Numpad2,
                ctrl,
                &workspace,
                UiAction::CreateLineAtTrack { track: 1 },
            ),
            (
                KeyCode::Numpad3,
                ctrl,
                &workspace,
                UiAction::CreateLineAtTrack { track: 2 },
            ),
            (
                KeyCode::Numpad4,
                ctrl,
                &workspace,
                UiAction::CreateLineAtTrack { track: 3 },
            ),
            (
                KeyCode::Enter,
                Modifiers::NONE,
                &workspace,
                UiAction::SelectLineAtPlayhead,
            ),
            (
                KeyCode::Character('i'),
                Modifiers::NONE,
                &workspace,
                UiAction::SetSelectedLineStartAtPlayhead,
            ),
            (
                KeyCode::Character('o'),
                Modifiers::NONE,
                &workspace,
                UiAction::SetSelectedLineEndAtPlayhead,
            ),
            (
                KeyCode::Character('t'),
                Modifiers::NONE,
                &workspace,
                UiAction::StartEditingSelectedLine,
            ),
            (
                KeyCode::Character('p'),
                Modifiers::NONE,
                &workspace,
                UiAction::StartEditingSelectedCharacter,
            ),
            (
                KeyCode::Character('c'),
                Modifiers::NONE,
                &workspace,
                UiAction::SetToolMode(ToolMode::Select),
            ),
            (
                KeyCode::Character('d'),
                ctrl,
                &workspace,
                UiAction::SetToolMode(ToolMode::Draw),
            ),
        ] {
            assert_eq!(
                router.resolve(&stroke(key, modifiers, false), contexts),
                Some(&expected)
            );
            let repeats = matches!(
                key,
                KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown
            );
            assert_eq!(
                router.resolve(&stroke(key, modifiers, true), contexts),
                repeats.then_some(&expected)
            );
        }

        let numpad_actions = [
            UiAction::AddMarker(MarkerKind::Boucle),
            UiAction::AddMarker(MarkerKind::Out),
            UiAction::AddMarker(MarkerKind::SceneChange),
            UiAction::OpenDropdown(ToolbarDropdown::Respirations),
            UiAction::OpenDropdown(ToolbarDropdown::Reactions),
            UiAction::AddNote,
            UiAction::AddMarker(MarkerKind::LiaisonLeft),
            UiAction::AddMarker(MarkerKind::LiaisonRight),
            UiAction::ToggleKaraokeForSelection,
        ];
        let numpad_keys = [
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad5,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
        ];
        for (key, expected) in numpad_keys.into_iter().zip(numpad_actions) {
            assert_eq!(
                router.resolve(&stroke(key, Modifiers::NONE, false), &workspace),
                Some(&expected)
            );
        }
    }

    #[test]
    fn continuous_pan_starts_once_and_stops_on_key_up() {
        let router = existing_shortcuts();
        let workspace = InputContextStack::new([InputContext::Workspace]);
        for (key, direction) in [(KeyCode::Character('q'), -1), (KeyCode::Character('d'), 1)] {
            assert_eq!(
                router.resolve(&stroke(key, Modifiers::NONE, false), &workspace),
                Some(&UiAction::BeginKeyboardPan { direction })
            );
            assert_eq!(
                router.resolve(&stroke(key, Modifiers::NONE, true), &workspace),
                None
            );
            let mut released = stroke(key, Modifiers::NONE, false);
            released.pressed = false;
            assert_eq!(
                router.resolve(&released, &workspace),
                Some(&UiAction::EndKeyboardPan)
            );
        }
    }

    #[test]
    fn alt_e_opens_text_emotions_while_editing_or_browsing_lines() {
        let router = existing_shortcuts();
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        for context in [InputContext::TextEditing, InputContext::Workspace] {
            assert_eq!(
                router.resolve(
                    &stroke(KeyCode::Character('e'), alt, false),
                    &InputContextStack::new([context]),
                ),
                Some(&UiAction::OpenTextEmotionMenu)
            );
        }
    }

    #[test]
    fn named_shortcuts_never_announce_their_physical_keys() {
        let router = existing_shortcuts();
        for binding in &router.bindings {
            let stroke = KeyStroke {
                key: binding.pattern.key,
                physical_key: None,
                location: winit::keyboard::KeyLocation::Standard,
                modifiers: binding.pattern.modifiers,
                pressed: binding.pattern.pressed,
                repeat: false,
                window: if binding.context == InputContext::SecondaryWindow {
                    InputWindow::Secondary
                } else {
                    InputWindow::Main
                },
            };
            let shortcut_label = stroke.accessibility_label();
            assert!(!shortcut_label.trim().is_empty());
            assert!(!shortcut_label.contains("shortcut."));
            if let Some(event) = crate::accessibility::event_for_keyboard_shortcut(&binding.command)
            {
                let crate::accessibility::AccessibilityEvent::Activation { label } = event else {
                    panic!("named shortcut announcement must be an activation event");
                };
                assert!(!label.trim().is_empty());
                assert!(!label.contains(shortcut_label.as_str()));
            }
        }
    }
}
