//! Keyboard text-editing controller for the rythmo workspace.

use super::*;

pub(crate) fn handle_key_input(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    text: &str,
) -> EventResponse {
    use crate::ui::text_input::TextInputAction;

    // Note editing takes priority
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            match state.note_input.handle_key(text, &line.note) {
                Some(TextInputAction::Changed(new_note)) => {
                    return EventResponse::Action(UiAction::UpdateLineNote {
                        line_id,
                        note: new_note,
                    })
                }
                Some(TextInputAction::Finished) => {
                    state.stop_note_editing();
                    return EventResponse::Action(UiAction::StopEditing);
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }

    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            // Escape abandons the picker without committing its current color.
            if text == "\x1b" {
                state.stop_char_editing();
                return EventResponse::Action(UiAction::StopEditing);
            }

            // Enter with autocomplete → confirm suggestion (default to first)
            if text == "\r" || text == "\n" {
                let entries = ctx.project.autocomplete_entries_for_line(line);
                let selected = state
                    .autocomplete_index
                    .and_then(|index| entries.get(index));
                let exact = entries.iter().find(|entry| entry.0 == line.character_name);
                if let Some((entry_name, color)) = selected.or(exact) {
                    let name = (*entry_name).to_string();
                    let color = *color;
                    state.stop_char_editing();
                    return EventResponse::Action(UiAction::SetCharacter {
                        line_id,
                        name,
                        color,
                    });
                }
            }

            match state.char_input.handle_key(text, &line.character_name) {
                Some(TextInputAction::Changed(name)) => {
                    state.autocomplete_index = None;
                    let br =
                        badge_rect_for_name(ctx.project, line, &name, ctx.current_frame, ctx.zone);
                    let (picker_x, picker_y) = color_picker_origin_for_badge(&br, ctx.zone);
                    state.color_picker.move_to(picker_x, picker_y);
                    return EventResponse::Action(UiAction::UpdateCharacterName { line_id, name });
                }
                Some(TextInputAction::Finished) => {
                    let name = line.character_name.clone();
                    let color = state.color_picker.current_color();
                    state.stop_char_editing();
                    return if !name.is_empty() {
                        EventResponse::Action(UiAction::SetCharacter {
                            line_id,
                            name,
                            color,
                        })
                    } else {
                        EventResponse::Action(UiAction::StopEditing)
                    };
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }

    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            match state.line_input.handle_key(text, &line.text) {
                Some(TextInputAction::Changed(new_text)) => {
                    return EventResponse::Action(UiAction::UpdateLineText {
                        id: line_id,
                        text: new_text,
                    })
                }
                Some(TextInputAction::Finished) => {
                    state.stop_line_editing();
                    return EventResponse::Action(UiAction::StopEditing);
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}
