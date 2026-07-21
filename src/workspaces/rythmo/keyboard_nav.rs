//! Autocomplete, selection and cursor keyboard interactions.

use super::*;

pub(crate) fn handle_autocomplete_nav(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    dir: i32,
) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if ctx.project.get_line(line_id).is_some() {
            let characters = ctx.project.known_characters();
            if characters.is_empty() {
                return EventResponse::Ignored;
            }

            let count = characters.len();
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
            if let Some(index) = new_idx {
                const VISIBLE_ROWS: usize = 8;
                if index < state.autocomplete_scroll {
                    state.autocomplete_scroll = index;
                } else if index >= state.autocomplete_scroll + VISIBLE_ROWS {
                    state.autocomplete_scroll = index + 1 - VISIBLE_ROWS;
                }
            }
            return new_idx
                .and_then(|index| characters.get(index).map(|character| (index, character)))
                .map(|(index, character)| {
                    EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Selection {
                            label: format!(
                                "{}. {} {} / {}",
                                character.name,
                                t("accessibility.choice"),
                                index + 1,
                                count
                            ),
                        },
                    ))
                })
                .unwrap_or(EventResponse::Consumed);
        }
    }
    EventResponse::Ignored
}

pub(crate) fn selection_response(
    input: &crate::ui::text_input::TextInputState,
    text: &str,
) -> EventResponse {
    input
        .selected_text(text)
        .map(|label| {
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection { label },
            ))
        })
        .unwrap_or(EventResponse::Consumed)
}

pub(crate) fn handle_select_all(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.char_input.select_all(&line.character_name);
            return selection_response(&state.char_input, &line.character_name);
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.line_input.select_all(&line.text);
            return selection_response(&state.line_input, &line.text);
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            let note = crate::rythmo_line_metadata::user_note(&line.note);
            state.note_input.select_all(note);
            return selection_response(&state.note_input, note);
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
            let note = crate::rythmo_line_metadata::user_note(&line.note);
            if let Some(text) = state.note_input.selected_text(note) {
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
            let visible_note = crate::rythmo_line_metadata::user_note(&line.note);
            if let Some(text) = state.note_input.selected_text(visible_note) {
                if let Some(crate::ui::text_input::TextInputAction::Changed(note)) =
                    state.note_input.handle_key(delete, visible_note)
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
                    state.autocomplete_index = None;
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
            } else if shift {
                state.char_input.move_right_shift(&line.character_name);
            } else {
                state.char_input.move_right(&line.character_name);
            }
            return if shift {
                selection_response(&state.char_input, &line.character_name)
            } else {
                EventResponse::Consumed
            };
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
            } else if shift {
                state.line_input.move_right_shift(&line.text);
            } else {
                state.line_input.move_right(&line.text);
            }
            return if shift {
                selection_response(&state.line_input, &line.text)
            } else {
                EventResponse::Consumed
            };
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            let note = crate::rythmo_line_metadata::user_note(&line.note);
            if dir < 0 {
                if shift {
                    state.note_input.move_left_shift();
                } else {
                    state.note_input.move_left();
                }
            } else if shift {
                state.note_input.move_right_shift(note);
            } else {
                state.note_input.move_right(note);
            }
            return if shift {
                selection_response(&state.note_input, note)
            } else {
                EventResponse::Consumed
            };
        }
    }
    EventResponse::Ignored
}

pub(crate) fn handle_word_selection(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    dir: i32,
) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                state.char_input.move_word_left_shift(&line.character_name);
            } else {
                state.char_input.move_word_right_shift(&line.character_name);
            }
            return selection_response(&state.char_input, &line.character_name);
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                state.line_input.move_word_left_shift(&line.text);
            } else {
                state.line_input.move_word_right_shift(&line.text);
            }
            return selection_response(&state.line_input, &line.text);
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            let note = crate::rythmo_line_metadata::user_note(&line.note);
            if dir < 0 {
                state.note_input.move_word_left_shift(note);
            } else {
                state.note_input.move_word_right_shift(note);
            }
            return selection_response(&state.note_input, note);
        }
    }
    EventResponse::Ignored
}

fn editing_line_label(ctx: &RythmoCtx<'_>, line_id: u64) -> Option<String> {
    let line = ctx.project.get_line(line_id)?;
    let mut parts = Vec::new();
    if !line.character_name.trim().is_empty() {
        parts.push(line.character_name.clone());
    }
    if !line.text.trim().is_empty() {
        parts.push(line.text.clone());
    }
    let mut label = if parts.is_empty() {
        crate::i18n::t("accessibility.line").to_string()
    } else {
        parts.join(", ")
    };
    label.push_str(&crate::rythmo_lint::line_accessibility_suffix(
        ctx.project,
        line_id,
    ));
    Some(label)
}

pub(crate) fn reread_editing_line(ctx: &RythmoCtx<'_>, state: &RythmoState) -> EventResponse {
    state
        .editing_line
        .and_then(|line_id| editing_line_label(ctx, line_id))
        .map(|label| {
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection { label },
            ))
        })
        .unwrap_or(EventResponse::Consumed)
}

pub(crate) fn handle_cursor_boundary(
    ctx: &RythmoCtx<'_>,
    state: &mut RythmoState,
    end: bool,
    shift: bool,
) -> EventResponse {
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if end {
                state.line_input.move_end(&line.text, shift);
            } else {
                state.line_input.move_home(shift);
            }
            return EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Activation {
                    label: crate::i18n::t(if end {
                        "accessibility.caret_dialogue_end"
                    } else {
                        "accessibility.caret_dialogue_start"
                    })
                    .to_string(),
                },
            ));
        }
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if end {
                state.char_input.move_end(&line.character_name, shift);
            } else {
                state.char_input.move_home(shift);
            }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            let note = crate::rythmo_line_metadata::user_note(&line.note);
            if end {
                state.note_input.move_end(note, shift);
            } else {
                state.note_input.move_home(shift);
            }
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}
