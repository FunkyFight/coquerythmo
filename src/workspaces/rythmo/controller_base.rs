//! Event-to-command controller for the rythmo workspace.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RythmoInteractionMode {
    Editable,
    ReadOnly,
}

pub(crate) struct RythmoCtx<'a> {
    pub(crate) zone: &'a Rect,
    pub(crate) project: &'a Project,
    pub(crate) render_index: &'a ProjectRenderIndex,
    pub(crate) current_frame: f64,
    pub(crate) karaoke_preview: bool,
    pub(crate) fps: f64,
    pub(crate) active_mode: ToolMode,
}

pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
    active_mode: ToolMode,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
    interaction_mode: RythmoInteractionMode,
) -> EventResponse {
    let mut ctx = RythmoCtx {
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        active_mode,
    };

    if interaction_mode == RythmoInteractionMode::ReadOnly {
        return handle_read_only_event(&mut ctx, event, state);
    }

    // Drawing mode handling - early return to block line editing.
    if active_mode == ToolMode::Draw {
        if let Some(response) =
            handle_drawing_event(&ctx, event, state, brush_color, brush_radius_frac, erasing)
        {
            return response;
        }
    }

    match event {
        UiEvent::DoubleClick { x, y }
            if *x >= ctx.zone.x
                && *x <= ctx.zone.x + ctx.zone.width
                && *y >= ctx.zone.y
                && *y <= ctx.zone.y + constants::RULER_HEIGHT =>
        {
            state.audio_offset_mode = !state.audio_offset_mode;
            state.audio_offset_drag = None;
            return EventResponse::Consumed;
        }
        UiEvent::MousePress { x, y }
            if state.audio_offset_mode
                && *x >= ctx.zone.x
                && *x <= ctx.zone.x + ctx.zone.width
                && *y >= ctx.zone.y
                && *y <= ctx.zone.y + constants::RULER_HEIGHT =>
        {
            state.audio_offset_drag = Some(AudioOffsetDrag {
                last_x: *x,
                accum_px: 0.0,
            });
            return EventResponse::Consumed;
        }
        UiEvent::MousePress { .. } if state.audio_offset_mode => {
            state.audio_offset_mode = false;
            state.audio_offset_drag = None;
            return EventResponse::Consumed;
        }
        UiEvent::MouseMove { x, .. } => {
            if let Some(drag) = &mut state.audio_offset_drag {
                let dx = *x - drag.last_x;
                drag.last_x = *x;
                drag.accum_px += dx;
                let frames = (drag.accum_px / ppf()).round() as i64;
                if frames != 0 {
                    drag.accum_px -= frames as f32 * ppf();
                    return EventResponse::Action(UiAction::OffsetActiveAudioBy(frames));
                }
                return EventResponse::Consumed;
            }
        }
        UiEvent::MouseRelease { .. } => {
            if state.audio_offset_drag.is_some() {
                state.audio_offset_drag = None;
                return EventResponse::Consumed;
            }
        }
        _ => {}
    }

    // The persistent character list owns wheel input while it is open.
    if let UiEvent::Scroll { x, y, delta, .. } = event {
        if let Some(line_id) = state.editing_character {
            if let Some(line) = ctx.project.get_line(line_id) {
                let characters = ctx.project.autocomplete_entries_for_line(line);
                let visible_rows = characters.len().min(8);
                let max_scroll = characters.len().saturating_sub(visible_rows);
                let badge = badge_rect_for_line(
                    ctx.project,
                    line,
                    ctx.current_frame,
                    ctx.zone,
                    crate::config::reading_bar_offset_seconds(),
                    ctx.fps,
                );
                let line_rect = line_rect(
                    ctx.project,
                    line,
                    ctx.current_frame,
                    ctx.zone,
                    crate::config::reading_bar_offset_seconds(),
                    ctx.fps,
                );
                let list = Rect {
                    x: badge.x,
                    y: line_rect.y + line_rect.height + 2.0,
                    width: 140.0,
                    height: visible_rows as f32 * 20.0,
                };
                if max_scroll > 0 && list.contains(*x, *y) {
                    if *delta > 0.0 {
                        state.autocomplete_scroll = state.autocomplete_scroll.saturating_sub(1);
                    } else {
                        state.autocomplete_scroll = (state.autocomplete_scroll + 1).min(max_scroll);
                    }
                    return EventResponse::Consumed;
                }
            }
        }
    }

    // Autocomplete click has highest priority (before color picker eats it)
    if let UiEvent::MousePress { x, y } = event {
        if let Some((name, color)) = autocomplete_hit(
            ctx.zone,
            ctx.project,
            ctx.current_frame,
            state,
            *x,
            *y,
            ctx.fps,
        ) {
            if let Some(line_id) = state.editing_character {
                state.stop_char_editing();
                return EventResponse::Action(UiAction::SetCharacter {
                    line_id,
                    name,
                    color,
                });
            }
        }
    }

    // Color picker overlay
    if state.color_picker.handle_event(event) {
        if let Some(line_id) = state.editing_character {
            return EventResponse::Action(UiAction::SetCharacterColor {
                line_id,
                color: state.color_picker.current_color(),
            });
        }
        return EventResponse::Consumed;
    }

    // Middle mouse pan
    if let UiEvent::MiddlePress { x, y } = event {
        if ctx.zone.contains(*x, *y) {
            state.panning = true;
            state.pan_last_x = *x;
            state.pan_accum = 0.0;
            return EventResponse::Consumed;
        }
    }
    if let UiEvent::MiddleRelease { .. } = event {
        if state.panning {
            state.panning = false;
            return EventResponse::Consumed;
        }
    }
    if let UiEvent::MouseMove { x, .. } = event {
        if state.panning {
            let dx = *x - state.pan_last_x;
            state.pan_last_x = *x;
            state.pan_accum -= dx;
            let frames = (state.pan_accum / ppf()).round() as i32;
            if frames != 0 {
                state.pan_accum -= frames as f32 * ppf();
                return EventResponse::Action(UiAction::SeekRelative(frames));
            }
            return EventResponse::Consumed;
        }
    }

    // Once a syllable handle owns the pointer, detection hover must not consume
    // its move or release events. This keeps shifted handles interactive.
    if state.syllable_drag.is_some() {
        match event {
            UiEvent::MouseMove { x, .. } => {
                if let Some(response) = syllable_mouse_move(state, *x) {
                    return response;
                }
            }
            UiEvent::MouseRelease { .. } => {
                if let Some(response) = syllable_mouse_release(state) {
                    return response;
                }
            }
            _ => {}
        }
    }

    if let Some(response) = handle_detection_event(&ctx, event, state) {
        return response;
    }

    match event {
        UiEvent::MousePress { x, y } => {
            if let Some(resp) = syllable_mouse_press(&ctx, state, *x, *y, false) {
                return resp;
            }
        }
        UiEvent::MouseMove { x, .. } => {
            if let Some(resp) = syllable_mouse_move(state, *x) {
                return resp;
            }
        }
        UiEvent::MouseRelease { .. } => {
            if let Some(resp) = syllable_mouse_release(state) {
                return resp;
            }
        }
        _ => {}
    }

    match event {
        UiEvent::MouseMove { x, y } => handle_mouse_move(&mut ctx, state, *x, *y),
        UiEvent::MousePress { x, y } => handle_mouse_press(&ctx, state, *x, *y),
        UiEvent::MouseRelease { .. } => handle_mouse_release(state, &ctx),
        UiEvent::CtrlClick { x, y } => handle_ctrl_click(&ctx, state, *x, *y),
        UiEvent::ShiftMousePress { x, y } => handle_shift_mouse_press(&ctx, state, *x, *y),
        UiEvent::DoubleClick { x, y } => handle_double_click(&ctx, state, *x, *y),
        UiEvent::KeyInput { text } => handle_key_input(&ctx, state, text),
        UiEvent::CursorLeft => handle_cursor_move(&ctx, state, -1, false),
        UiEvent::CursorRight => handle_cursor_move(&ctx, state, 1, false),
        UiEvent::ShiftCursorLeft => handle_cursor_move(&ctx, state, -1, true),
        UiEvent::ShiftCursorRight => handle_cursor_move(&ctx, state, 1, true),
        UiEvent::SelectWordLeft => handle_word_selection(&ctx, state, -1),
        UiEvent::SelectWordRight => handle_word_selection(&ctx, state, 1),
        UiEvent::CursorUp => {
            if state.editing_line.is_some() || state.editing_note.is_some() {
                // A line/note editor is single-line: Up/Down have no
                // vertical target, but must still be consumed so they never
                // fall through to the workspace volume shortcuts.
                EventResponse::Consumed
            } else {
                handle_autocomplete_nav(&ctx, state, -1)
            }
        }
        UiEvent::CursorDown => {
            if state.editing_line.is_some() {
                reread_editing_line(&ctx, state)
            } else if state.editing_note.is_some() {
                EventResponse::Consumed
            } else {
                handle_autocomplete_nav(&ctx, state, 1)
            }
        }
        UiEvent::Home => handle_cursor_boundary(&ctx, state, false, false),
        UiEvent::End => handle_cursor_boundary(&ctx, state, true, false),
        UiEvent::SelectAll => handle_select_all(&ctx, state),
        UiEvent::Copy => handle_copy(&ctx, state),
        UiEvent::Cut => handle_cut(&ctx, state),
        UiEvent::UndoTextEdit => handle_text_undo(&ctx, state),
        UiEvent::Delete => {
            if state.selected.is_some() {
                EventResponse::Action(UiAction::DeleteSelected)
            } else {
                EventResponse::Ignored
            }
        }
        _ => EventResponse::Ignored,
    }
}

fn handle_read_only_event(
    ctx: &mut RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> EventResponse {
    // Defensive cleanup prevents a gesture started in the authoring workspace
    // from completing after a keyboard-driven workspace transition.
    state.dragging = None;
    state.selection_drag = None;
    state.transform_handle = None;
    state.syllable_drag = None;
    state.active_stroke = None;
    state.audio_offset_mode = false;
    state.audio_offset_drag = None;
    state.context_menu = None;
    state.detection_hover = None;
    state.detection_menu = None;
    state.detection_drag = None;
    if matches!(state.selected, Some(Selection::Detection(_))) {
        state.selected = None;
    }
    if state.is_editing() {
        state.stop_line_editing();
        state.stop_note_editing();
        state.stop_char_editing();
    }

    match event {
        UiEvent::MiddlePress { x, y } if ctx.zone.contains(*x, *y) => {
            state.panning = true;
            state.pan_last_x = *x;
            state.pan_accum = 0.0;
            EventResponse::Consumed
        }
        UiEvent::MiddleRelease { .. } if state.panning => {
            state.panning = false;
            EventResponse::Consumed
        }
        UiEvent::MouseMove { x, .. } if state.panning => {
            let dx = *x - state.pan_last_x;
            state.pan_last_x = *x;
            state.pan_accum -= dx;
            let frames = (state.pan_accum / ppf()).round() as i32;
            if frames != 0 {
                state.pan_accum -= frames as f32 * ppf();
                EventResponse::Action(UiAction::SeekRelative(frames))
            } else {
                EventResponse::Consumed
            }
        }
        UiEvent::MouseMove { x, y } => handle_mouse_move(ctx, state, *x, *y),
        UiEvent::MousePress { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y }
            if ctx.zone.contains(*x, *y) =>
        {
            // Selection is view state only. No drag handle is armed.
            let _ = handle_mouse_move(ctx, state, *x, *y);
            state.selected = state.hovered_line.map(Selection::Line);
            EventResponse::Consumed
        }
        UiEvent::Delete | UiEvent::Cut | UiEvent::KeyInput { .. } | UiEvent::UndoTextEdit => {
            EventResponse::Consumed
        }
        _ => EventResponse::Ignored,
    }
}
