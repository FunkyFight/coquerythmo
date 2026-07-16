//! Event-to-command controller for the rythmo workspace.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::*;

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

    // Autocomplete click has highest priority (before color picker eats it)
    if let UiEvent::MousePress { x, y } = event {
        if let Some((name, color)) =
            autocomplete_hit(ctx.zone, ctx.project, ctx.current_frame, state, *x, *y)
        {
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

    match event {
        UiEvent::MousePress { x, y } => {
            if let Some(resp) = syllable_mouse_press(&ctx, state, *x, *y) {
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
            if state.editing_line.is_some() || state.editing_note.is_some() {
                EventResponse::Consumed
            } else {
                handle_autocomplete_nav(&ctx, state, 1)
            }
        }
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
