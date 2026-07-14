//! Text undo controller for the rythmo workspace.

use super::*;

pub(crate) fn handle_text_undo(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(note) = state.note_input.undo(&line.note) {
                return EventResponse::Action(UiAction::UpdateLineNote { line_id, note });
            }
        }
        return EventResponse::Consumed;
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(name) = state.char_input.undo(&line.character_name) {
                state.autocomplete_index = Some(0);
                return EventResponse::Action(UiAction::UpdateCharacterName { line_id, name });
            }
        }
        return EventResponse::Consumed;
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.undo(&line.text) {
                return EventResponse::Action(UiAction::UpdateLineText { id: line_id, text });
            }
        }
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

