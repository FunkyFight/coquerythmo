//! Pointer press controller for the rythmo workspace.

use super::*;

pub(crate) fn handle_mouse_press(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    // (autocomplete click already handled before color picker in handle_rythmo_event)

    // Click outside zone while editing → finalize
    if !ctx.zone.contains(x, y) {
        let char_id = state.editing_character;
        let was_editing_line = state.editing_line.is_some();
        let was_editing_note = state.editing_note.is_some();
        if char_id.is_some() {
            state.stop_char_editing();
        }
        if was_editing_line {
            state.stop_line_editing();
        }
        if was_editing_note {
            state.stop_note_editing();
        }
        if let Some(line_id) = char_id {
            return EventResponse::Action(UiAction::FinalizeCharacter { line_id });
        }
        return if was_editing_line {
            EventResponse::Action(UiAction::StopEditing)
        } else {
            EventResponse::Ignored
        };
    }

    // If we have a stroke selection with transform handles, check those first
    if matches!(state.selected, Some(Selection::Strokes(_))) {
        if let Some(handle_kind) = hit_test_transform_handles(ctx, state, ctx.project, x, y) {
            start_transform_drag(state, ctx.project, handle_kind, x, y);
            return EventResponse::Consumed;
        }
    }

    // Check markers first (smaller hit targets, on top visually)
    let marker_hit_w = 12.0;
    for (i, marker) in ctx.project.markers().iter().enumerate() {
        let mx = frame_to_x(marker.frame, ctx.current_frame, ctx.zone);
        if (x - mx).abs() < marker_hit_w {
            state.selected = Some(Selection::Marker(i));
            state.dragging = Some(DragState {
                target: DragTarget::Marker(i),
                drag_start_x: x,
                original_frame: marker.frame,
                original_duration: 0,
                original_y_slot: 0.0,
                drag_start_y: y,
                handle: DragHandle::Body,
                group_origins: Vec::new(),
            });
            return EventResponse::Consumed;
        }
    }

    // Check lines
    for line in ctx.project.lines() {
        let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
        if !r.contains(x, y) {
            continue;
        }

        // If editing this line, single click positions cursor instead of starting a generic drag
        // Only exceptions are the resize handles which should still resize the line
        let is_left_handle = x < r.x + constants::HANDLE_WIDTH;
        let is_right_handle = x > r.x + r.width - constants::HANDLE_WIDTH;
        let is_editing = state.editing_line == Some(line.id);

        if is_editing && !is_left_handle && !is_right_handle {
            if !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                state.pending_cursor_click = Some((ratio, false));

                let lang = ctx.project.syllable_language_code();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.start_selection(char_pos);
            }
            // Add a special drag handle for mouse selection to allow mouse drag selection
            state.dragging = Some(DragState {
                target: DragTarget::Line(line.id),
                handle: DragHandle::Selection,
                drag_start_x: x,
                original_frame: line.start_frame,
                original_duration: line.duration_frames,
                original_y_slot: line.y_slot,
                drag_start_y: y,
                group_origins: Vec::new(),
            });
            return EventResponse::Consumed;
        }

        let handle = if is_left_handle {
            DragHandle::Left
        } else if is_right_handle {
            DragHandle::Right
        } else {
            DragHandle::Body
        };
        let group_origins = if handle == DragHandle::Body {
            if let Some(selection) = state.selected.clone() {
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
            }
        } else {
            state.selected = Some(Selection::Line(line.id));
            Vec::new()
        };

        state.dragging = Some(DragState {
            target: DragTarget::Line(line.id),
            handle,
            drag_start_x: x,
            original_frame: line.start_frame,
            original_duration: line.duration_frames,
            original_y_slot: line.y_slot,
            drag_start_y: y,
            group_origins,
        });
        return EventResponse::Consumed;
    }

    // Click on empty space in Select mode → hit-test a stroke, else start marquee
    if ctx.active_mode == ToolMode::Select {
        let ppf = crate::rythmo_drawing::ppf_for_scale(1.0);
        let (frame, y_frac) = crate::rythmo_drawing::screen_to_drawing(
            x,
            y,
            ctx.zone.x,
            ctx.zone.y,
            ctx.zone.width,
            ctx.zone.height,
            ctx.current_frame,
            ppf,
        );
        let ids =
            ctx.project
                .drawing()
                .strokes_within_radius(frame, y_frac, ppf, ctx.zone.height, 0.025);
        if !ids.is_empty() {
            state.selected = Some(Selection::Strokes(ids));
            return EventResponse::Consumed;
        }
        return handle_selection_drag(state, x, y, false);
    }

    // Click on empty space → deselect & stop editing
    state.selected = None;
    let char_id = state.editing_character;
    let was_editing_line = state.editing_line.is_some();
    let was_editing_note = state.editing_note.is_some();
    if char_id.is_some() {
        state.stop_char_editing();
    }
    if was_editing_line {
        state.stop_line_editing();
    }
    if was_editing_note {
        state.stop_note_editing();
    }
    if let Some(line_id) = char_id {
        return EventResponse::Action(UiAction::FinalizeCharacter { line_id });
    }
    if was_editing_line || was_editing_note {
        return EventResponse::Action(UiAction::StopEditing);
    }
    EventResponse::Ignored
}
