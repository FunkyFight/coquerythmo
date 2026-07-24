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

    if let Some(drag) = state.dragging.take() {
        if let DragState {
            target: DragTarget::Line(line_id),
            handle: DragHandle::Selection,
            ..
        } = drag
        {
            if let Some(line) = ctx.project.get_line(line_id) {
                return selection_response(&state.line_input, &line.text);
            }
        }
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

    // Ctrl+clicking a syllable handle edits that handle in place instead of
    // creating a line. The drag keeps all earlier syllable boundaries fixed.
    if let Some(response) = syllable_mouse_press(ctx, state, x, y, true) {
        return response;
    }

    state.stop_line_editing();
    state.stop_char_editing();
    state.stop_note_editing();
    EventResponse::Action(UiAction::CreateLine {
        frame: x_to_frame(x, ctx.current_frame, ctx.zone),
        y_slot: y_to_slot_at_frame(ctx.project, y, ctx.current_frame, ctx.zone),
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
            let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
            if r.contains(x, y) && !line.text.is_empty() {
                let text_rect = ambiance_description_rect(r, line.kind);
                let ratio = ((x - text_rect.x) / text_rect.width).clamp(0.0, 1.0);
                state.pending_cursor_click = Some((ratio, true));

                // If there's no selection, start one from current cursor
                if !state.line_input.has_selection() {
                    let current = state.line_input.cursor_pos;
                    state.line_input.selection = Some((current, current));
                }

                let lang = ctx.project.syllable_language_code();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.update_selection(char_pos);

                return selection_response(&state.line_input, &line.text);
            }
        }
    }

    // Outside text editing, Shift+drag on a line locks timing and changes
    // only its vertical track. Preserve an existing multi-line selection so
    // the whole group can move vertically together.
    for line in ctx.project.lines() {
        let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
        if !rect.contains(x, y) {
            continue;
        }
        let group_origins = if let Some(selection) = state.selected.clone() {
            let origins = selected_line_origins(ctx.project, &selection);
            if !origins.is_empty() && origins.iter().any(|origin| origin.line_id == line.id) {
                origins
            } else {
                state.selected = Some(Selection::Line(line.id));
                Vec::new()
            }
        } else {
            state.selected = Some(Selection::Line(line.id));
            Vec::new()
        };
        state.dragging = Some(DragState {
            target: DragTarget::Line(line.id),
            drag_start_x: x,
            original_frame: line.start_frame,
            original_duration: line.duration_frames,
            original_y_slot: line.y_slot,
            drag_start_y: y,
            handle: DragHandle::VerticalOnly,
            group_origins,
        });
        return EventResponse::Consumed;
    }

    if ctx.active_mode == ToolMode::Select {
        return handle_selection_drag(state, x, y, true);
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

    // Badge → character/ambiance-name editing. End markers have no label.
    for line in ctx.project.lines() {
        if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceEnd) {
            continue;
        }
        let br = badge_rect_for_line(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
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
            state.autocomplete_index = None;
            state.autocomplete_hover = None;
            state.autocomplete_scroll = 0;
            if line.kind.is_dialogue() {
                let (picker_x, picker_y) = color_picker_origin_for_badge(&br, ctx.zone);
                state
                    .color_picker
                    .open(picker_x, picker_y, line.character_color);
            } else {
                state.color_picker.close();
            }
            state.stop_line_editing();
            state.stop_note_editing();
            return if let Some(old_id) = finalize_line_id.filter(|&id| id != line.id) {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: t("accessibility.edit_character").to_string(),
                    },
                ))
            };
        }
    }
    // Line body → note editing (if has note and click is in note area) or text editing
    for line in ctx.project.lines() {
        let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
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
                let text_rect = ambiance_description_rect(r, line.kind);
                let ratio = ((x - text_rect.x) / text_rect.width).clamp(0.0, 1.0);
                let lang = ctx.project.syllable_language_code();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.select_word_at(&line.text, char_pos);
                return selection_response(&state.line_input, &line.text);
            }
            state.editing_line = Some(line.id);
            state.line_input.activate(&line.text);
            // An ambiance end has no label: its whole visible content is the
            // editable description. Selecting it on entry also lets users
            // immediately replace legacy placeholders such as "(fin amb.)".
            if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceEnd) {
                state.line_input.select_all(&line.text);
            }
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
