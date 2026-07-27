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
                    let br = badge_rect_for_name(
                        ctx.project,
                        line,
                        &name,
                        ctx.current_frame,
                        ctx.zone,
                        crate::config::reading_bar_offset_seconds(),
                        ctx.fps,
                    );
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
            let input = if line.text.is_empty() {
                normalize_first_line_input(text, &mut state.line_lowercase_override)
            } else {
                text.to_string()
            };
            match state.line_input.handle_key(&input, &line.text) {
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

fn normalize_first_line_input(text: &str, lowercase_override: &mut bool) -> String {
    if *lowercase_override {
        return text.to_string();
    }
    if let Some(rest) = text.strip_prefix('!') {
        *lowercase_override = true;
        return rest.to_string();
    }
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::normalize_first_line_input;

    #[test]
    fn first_line_input_capitalizes_unless_prefixed_with_exclamation() {
        let mut override_lowercase = false;
        assert_eq!(
            normalize_first_line_input("bonjour", &mut override_lowercase),
            "Bonjour"
        );
        assert_eq!(
            normalize_first_line_input("!salut", &mut override_lowercase),
            "salut"
        );
        assert!(override_lowercase);
        assert_eq!(
            normalize_first_line_input("encore", &mut override_lowercase),
            "encore"
        );
    }
}
