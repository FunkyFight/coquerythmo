//! Shortcut interceptor for the text-emotion palette.

pub(super) use super::{event_loop, file_picker};

#[path = "dispatcher.rs"]
mod base;

use super::event_loop::AppEvent;
use crate::application::command::UiAction;
use crate::state::State;
use winit::event_loop::EventLoopWindowTarget;

pub(crate) struct CommandDispatcher;

pub(crate) use base::handle_file_picker_selected;

impl CommandDispatcher {
    pub(crate) fn announce_shortcut(action: &UiAction, state: &State) {
        base::CommandDispatcher::announce_shortcut(action, state);
    }

    pub(crate) fn dispatch_shortcut(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        if is_text_emotion_shortcut(&action) {
            return open_text_emotion_palette(state);
        }
        base::CommandDispatcher::dispatch_shortcut(action, state, elwt)
    }

    pub(crate) fn dispatch(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        if is_text_emotion_shortcut(&action) {
            return open_text_emotion_palette(state);
        }
        base::CommandDispatcher::dispatch(action, state, elwt)
    }
}

fn is_text_emotion_shortcut(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Opened { label })
            if label == crate::input::TEXT_EMOTION_SHORTCUT_SENTINEL
    )
}

fn open_text_emotion_palette(state: &mut State) -> bool {
    let opened = {
        let ui = &state.ui_shell.ui;
        let (screen_w, screen_h) = ui.text_emotion_screen_size();
        let (cursor_x, cursor_y) = ui.cursor_pos;
        crate::text_emotion_foreground::open_keyboard(
            &state.project_session.project,
            &ui.rythmo_state,
            cursor_x,
            cursor_y,
            screen_w,
            screen_h,
        )
    };

    if opened {
        state
            .ui_shell
            .ui
            .open_text_emotion_accessibility_scope();
        state.announce_shortcut_accessibility(
            crate::accessibility::AccessibilityEvent::Opened {
                label: "Menu des émotions du texte. Retirer l’émotion. Utilisez les flèches haut et bas puis Entrée."
                    .to_string(),
            },
        );
    } else {
        state.announce_shortcut_accessibility(
            crate::accessibility::AccessibilityEvent::Error {
                message: "Sélectionnez une ligne de dialogue non karaoké pour appliquer une émotion du texte."
                    .to_string(),
            },
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_is_intercepted_without_exposing_it_to_narration() {
        let action = UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Opened {
                label: crate::input::TEXT_EMOTION_SHORTCUT_SENTINEL.to_string(),
            },
        );
        assert!(is_text_emotion_shortcut(&action));
    }
}
