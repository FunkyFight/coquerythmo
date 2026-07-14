//! Autocomplete, selection and cursor keyboard interactions.

use super::*;

pub(crate) fn handle_autocomplete_nav(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    dir: i32,
) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            let suggestions = ctx.project.autocomplete(&line.character_name);
            if suggestions.is_empty() {
                return EventResponse::Ignored;
            }

            let count = suggestions.len();
            let new_idx = match state.autocomplete_index {
                Some(idx) => {
                    let next = idx as i32 + dir;
                    if next < 0 {
                        None
                    } else {
                        Some((next as usize).min(count - 1))
                    }
                }
                None => {
                    if dir > 0 {
                        Some(0)
                    } else {
                        None
                    }
                }
            };
            state.autocomplete_index = new_idx;
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}

pub(crate) fn handle_select_all(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.char_input.select_all(&line.character_name);
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.line_input.select_all(&line.text);
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.note_input.select_all(&line.note);
            return EventResponse::Consumed;
        }
    }
    if ctx.project.lines().next().is_some() {
        state.selected = Some(Selection::AllLines);
        state.dragging = None;
        state.stop_line_editing();
        state.stop_char_editing();
        state.stop_note_editing();
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

pub(crate) fn handle_copy(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.note_input.selected_text(&line.note) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.char_input.selected_text(&line.character_name) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.selected_text(&line.text) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    EventResponse::Consumed
}

pub(crate) fn handle_cut(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    let delete = "\x08";
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.note_input.selected_text(&line.note) {
                if let Some(crate::ui::text_input::TextInputAction::Changed(note)) =
                    state.note_input.handle_key(delete, &line.note)
                {
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateLineNote {
                        clipboard: text,
                        line_id,
                        note,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.char_input.selected_text(&line.character_name) {
                if let Some(crate::ui::text_input::TextInputAction::Changed(name)) =
                    state.char_input.handle_key(delete, &line.character_name)
                {
                    state.autocomplete_index = Some(0);
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateCharacterName {
                        clipboard: text,
                        line_id,
                        name,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.selected_text(&line.text) {
                if let Some(crate::ui::text_input::TextInputAction::Changed(new_text)) =
                    state.line_input.handle_key(delete, &line.text)
                {
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateLineText {
                        clipboard: text,
                        id: line_id,
                        text: new_text,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    EventResponse::Consumed
}

pub(crate) fn handle_cursor_move(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    dir: i32,
    shift: bool,
) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.char_input.move_left_shift();
                } else {
                    state.char_input.move_left();
                }
            } else {
                if shift {
                    state.char_input.move_right_shift(&line.character_name);
                } else {
                    state.char_input.move_right(&line.character_name);
                }
            }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.line_input.move_left_shift();
                } else {
                    state.line_input.move_left();
                }
            } else {
                if shift {
                    state.line_input.move_right_shift(&line.text);
                } else {
                    state.line_input.move_right(&line.text);
                }
            }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.note_input.move_left_shift();
                } else {
                    state.note_input.move_left();
                }
            } else {
                if shift {
                    state.note_input.move_right_shift(&line.note);
                } else {
                    state.note_input.move_right(&line.note);
                }
            }
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}
