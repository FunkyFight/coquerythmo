//! Application command dispatcher facade.
//!
//! The established dispatcher remains authoritative. This boundary normalizes
//! note-editing actions so line presentation metadata never enters the text
//! editor and cannot be erased by cut, paste or undo.

pub(crate) use super::event_loop;
pub(crate) use super::file_picker;

#[path = "dispatcher.rs"]
mod legacy;

use crate::state::State;
use crate::ui::primitives::{EventResponse, UiAction, UiEvent};
use crate::workspaces::rythmo::view::Selection;
use winit::event_loop::EventLoopWindowTarget;

use super::event_loop::AppEvent;

pub(crate) use legacy::handle_file_picker_selected;

pub(crate) struct CommandDispatcher;

fn selected_note_line_id(state: &State) -> Option<u64> {
    let rythmo = &state.ui_shell.ui.rythmo_state;
    let candidate = match rythmo.selected.as_ref() {
        Some(Selection::Line(line_id)) => Some(*line_id),
        Some(Selection::Lines(line_ids)) => line_ids.first().copied(),
        Some(Selection::AllLines) => state.project_session.project.lines().next().map(|line| line.id),
        Some(Selection::Detection(address)) if address.track().is_none() => Some(address.line_id),
        _ => rythmo.hovered_line,
    }?;
    state.project_session.project.get_line(candidate).map(|line| line.id)
}

fn begin_visible_note_edit(state: &mut State) -> bool {
    let Some(line_id) = selected_note_line_id(state) else {
        return false;
    };
    let note = state
        .project_session
        .project
        .get_line(line_id)
        .map(|line| crate::rythmo_line_metadata::user_note(&line.note).to_string())
        .unwrap_or_default();
    state
        .ui_shell
        .ui
        .rythmo_state
        .start_editing_note(line_id, &note);
    true
}

fn normalize_note_action(action: UiAction, state: &State) -> UiAction {
    match action {
        UiAction::UpdateLineNote { line_id, note } => {
            let note = state
                .project_session
                .project
                .get_line(line_id)
                .map(|line| crate::rythmo_line_metadata::merge_note_update(&line.note, &note))
                .unwrap_or(note);
            UiAction::UpdateLineNote { line_id, note }
        }
        UiAction::SetClipboardAndUpdateLineNote {
            clipboard,
            line_id,
            note,
        } => {
            let note = state
                .project_session
                .project
                .get_line(line_id)
                .map(|line| crate::rythmo_line_metadata::merge_note_update(&line.note, &note))
                .unwrap_or(note);
            UiAction::SetClipboardAndUpdateLineNote {
                clipboard,
                line_id,
                note,
            }
        }
        other => other,
    }
}

impl CommandDispatcher {
    pub(crate) fn announce_shortcut(action: &UiAction, state: &State) {
        legacy::CommandDispatcher::announce_shortcut(action, state);
    }

    pub(crate) fn dispatch_shortcut(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        if matches!(action, UiAction::AddNote) {
            let opened = begin_visible_note_edit(state);
            if opened {
                legacy::CommandDispatcher::announce_shortcut(&UiAction::AddNote, state);
            }
            return false;
        }
        let action = normalize_note_action(action, state);
        legacy::CommandDispatcher::dispatch_shortcut(action, state, elwt)
    }

    pub(crate) fn dispatch(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        if matches!(action, UiAction::AddNote) {
            return if begin_visible_note_edit(state) {
                false
            } else {
                legacy::CommandDispatcher::dispatch(UiAction::AddNote, state, elwt)
            };
        }
        let action = normalize_note_action(action, state);
        legacy::CommandDispatcher::dispatch(action, state, elwt)
    }
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
    response_changed_ui: bool,
    continuous_redraw: bool,
) -> bool {
    !is_pointer_move || response_changed_ui || !continuous_redraw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_pointer_moves_keep_the_paced_redraw_loop() {
        assert!(!should_request_redraw(true, false, true));
        assert!(should_request_redraw(true, true, true));
        assert!(should_request_redraw(true, false, false));
        assert!(should_request_redraw(false, false, true));
    }
}
