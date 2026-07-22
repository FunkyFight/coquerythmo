//! Shortcut interceptor for the text-emotion palette.

pub(super) use super::{event_loop, file_picker};

#[path = "dispatcher.rs"]
mod base;

use super::event_loop::AppEvent;
use crate::application::command::UiAction;
use crate::state::State;
use crate::ui::primitives::{EventResponse, UiEvent};
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
        dispatch_with_text_rebase(action, state, elwt, true)
    }

    pub(crate) fn dispatch(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        if is_text_emotion_shortcut(&action) {
            return open_text_emotion_palette(state);
        }
        dispatch_with_text_rebase(action, state, elwt, false)
    }
}

fn dispatch_with_text_rebase(
    action: UiAction,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
    shortcut: bool,
) -> bool {
    let text_edit = match &action {
        UiAction::UpdateLineText { id, text }
        | UiAction::SetClipboardAndUpdateLineText { id, text, .. } => state
            .project_session
            .project
            .get_line(*id)
            .map(|line| (*id, line.text.clone(), text.clone())),
        _ => None,
    };
    let should_exit = if shortcut {
        base::CommandDispatcher::dispatch_shortcut(action, state, elwt)
    } else {
        base::CommandDispatcher::dispatch(action, state, elwt)
    };
    if let Some((line_id, old_text, new_text)) = text_edit {
        if old_text != new_text {
            crate::text_emotion::rebase_after_text_edit(line_id, &old_text, &new_text);
        }
    }
    should_exit
}

fn is_text_emotion_shortcut(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Opened { label })
            if label == crate::input::TEXT_EMOTION_SHORTCUT_SENTINEL
    )
}

fn open_text_emotion_palette(state: &mut State) -> bool {
    let size = state.render.gfx.size;
    let (screen_w, screen_h) =
        state.window_to_ui_position(size.width as f32, size.height as f32);
    let opened = {
        let ui = &state.ui_shell.ui;
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
        state.announce_shortcut_accessibility(
            crate::accessibility::AccessibilityEvent::Opened {
                label: "Menu des émotions du texte. Utilisez les flèches haut et bas puis Entrée."
                    .to_string(),
            },
        );
        state.announce_shortcut_accessibility(
            crate::accessibility::AccessibilityEvent::Focus {
                label: "Retirer l’émotion".to_string(),
                role: "menu button".to_string(),
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

pub(crate) fn dispatch(
    ui_event: UiEvent,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) {
    let response = state.handle_ui_event(&ui_event);
    let response_changed_ui = !matches!(response, EventResponse::Ignored);
    let is_pointer_move = matches!(ui_event, UiEvent::MouseMove { .. });

    match response {
        EventResponse::Action(action) => {
            if CommandDispatcher::dispatch(action, state, elwt) {
                elwt.exit();
            }
        }
        EventResponse::Actions(actions) => {
            for action in actions {
                if CommandDispatcher::dispatch(action, state, elwt) {
                    elwt.exit();
                    break;
                }
            }
        }
        EventResponse::Ignored | EventResponse::Consumed => {}
    }

    if should_request_redraw(
        is_pointer_move,
        response_changed_ui,
        state.needs_continuous_redraw(),
    ) {
        state.request_redraw();
    }
}

fn should_request_redraw(
    is_pointer_move: bool,
    _response_changed_ui: bool,
    continuous_redraw: bool,
) -> bool {
    !is_pointer_move || !continuous_redraw
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

    #[test]
    fn pointer_moves_stay_on_the_paced_redraw_loop() {
        assert!(!should_request_redraw(true, true, true));
        assert!(should_request_redraw(false, true, true));
    }
}
