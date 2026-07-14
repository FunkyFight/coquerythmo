//! Mouse button controllers for the rythmo workspace.

use super::*;

pub(crate) fn handle_mouse_release(state: &mut RythmoState, ctx: &RythmoCtx) -> EventResponse {
    // Handle transform handle release
    if state.transform_handle.take().is_some() {
        return EventResponse::Consumed;
    }

    // Handle marquee selection release
    if state.selection_drag.is_some() {
        finalize_marquee_selection(ctx, state);
        return EventResponse::Consumed;
    }

    if state.dragging.take().is_some() {
        EventResponse::Consumed
    } else {
        EventResponse::Ignored
    }
}

pub(crate) fn handle_ctrl_click(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    if !ctx.zone.contains(x, y) {
        return EventResponse::Ignored;
    }
    state.stop_line_editing();
    state.stop_char_editing();
    state.stop_note_editing();
    EventResponse::Action(UiAction::CreateLine {
        frame: x_to_frame(x, ctx.current_frame, ctx.zone),
        y_slot: y_to_slot(ctx.project, y, ctx.zone),
    })
}

pub(crate) fn handle_shift_mouse_press(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    if !ctx.zone.contains(x, y) {
        return EventResponse::Ignored;
    }

    // Line text editing selection
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
            if r.contains(x, y) && !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                state.pending_cursor_click = Some((ratio, true));

                // If there's no selection, start one from current cursor
                if !state.line_input.has_selection() {
                    let current = state.line_input.cursor_pos;
                    state.line_input.selection = Some((current, current));
                }

                let lang = crate::config::get().lang.clone();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    &lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.update_selection(char_pos);

                return EventResponse::Consumed;
            }
        }
    }

    EventResponse::Ignored
}

pub(crate) fn handle_double_click(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    // Save current character edit before switching
    let finalize_line_id = state.editing_character;

    // Badge → character editing
    for line in ctx.project.lines() {
        let br = badge_rect_for_line(ctx.project, line, ctx.current_frame, ctx.zone);
        if br.contains(x, y) {
            if let Some(old_id) = finalize_line_id {
                if old_id != line.id {
                    state.stop_char_editing();
                    // Can't dispatch two actions, so finalize happens via FinalizeCharacter below
                }
            }
            state.editing_character = Some(line.id);
            state.char_input.activate(&line.character_name);
            state.char_input.select_all(&line.character_name);
            let (picker_x, picker_y) = color_picker_origin_for_badge(&br, ctx.zone);
            state
                .color_picker
                .open(picker_x, picker_y, line.character_color);
            state.stop_line_editing();
            state.stop_note_editing();
            return if let Some(old_id) = finalize_line_id.filter(|&id| id != line.id) {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Line body → note editing (if has note and click is in note area) or text editing
    for line in ctx.project.lines() {
        let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        if r.contains(x, y) {
            // If the line has a note and click is in the bottom part, edit note
            if !line.note.is_empty() {
                let note_label_h = 12.0;
                let note_y = r.y + r.height - note_label_h - 1.0;
                if y >= note_y {
                    state.stop_line_editing();
                    state.stop_char_editing();
                    return EventResponse::Action(UiAction::AddNote);
                }
            }
            // If already editing this line, select the clicked word.
            if state.editing_line == Some(line.id) && !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                let lang = crate::config::get().lang.clone();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    &lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.select_word_at(&line.text, char_pos);
                return EventResponse::Consumed;
            }
            state.editing_line = Some(line.id);
            state.line_input.activate(&line.text);
            state.stop_char_editing();
            state.stop_note_editing();
            return if let Some(old_id) = finalize_line_id {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Click empty → stop editing
    if let Some(old_id) = finalize_line_id {
        state.stop_char_editing();
        return EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id });
    }
    if state.editing_line.is_some() {
        state.stop_line_editing();
        return EventResponse::Action(UiAction::StopEditing);
    }
    EventResponse::Ignored
}
