//! Drawing-tool controller for the rythmo workspace.
//!
//! This module owns the transient stroke lifecycle. It emits application
//! commands for erasing and committing strokes; it never mutates the project.

use super::*;

pub(crate) fn handle_drawing_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
) -> Option<EventResponse> {
    use crate::rythmo_drawing::screen_to_drawing;

    let in_zone = |x: f32, y: f32| {
        x >= ctx.zone.x
            && x <= ctx.zone.x + ctx.zone.width
            && y >= ctx.zone.y
            && y <= ctx.zone.y + ctx.zone.height
    };
    let ppf = crate::rythmo_drawing::ppf_for_scale(1.0, ctx.project.settings().scroll_speed);
    let reading_bar_offset_frames = crate::rythmo_layout::reading_bar_offset_seconds(
        ctx.project.settings().reading_bar_offset_percent,
        ctx.zone.width,
        ctx.fps,
        ppf,
    ) * ctx.fps;

    match event {
        UiEvent::MousePress { x, y } if in_zone(*x, *y) => {
            let (frame, y_frac) = screen_to_drawing(
                *x,
                *y,
                ctx.zone.x,
                ctx.zone.y,
                ctx.zone.width,
                ctx.zone.height,
                ctx.current_frame,
                ppf,
                reading_bar_offset_frames,
            );
            if erasing {
                let stroke_ids = ctx.project.drawing().strokes_within_radius(
                    frame,
                    y_frac,
                    ppf,
                    ctx.zone.height,
                    brush_radius_frac,
                );
                if !stroke_ids.is_empty() {
                    return Some(EventResponse::Action(UiAction::EraseDrawingStrokes(
                        stroke_ids,
                    )));
                }
            } else {
                let mut stroke = DrawingStroke::new(
                    ctx.project.drawing().peek_id(),
                    brush_color,
                    brush_radius_frac,
                );
                stroke.points.push((frame, y_frac));
                state.active_stroke = Some(stroke);
                state.drawing_dirty = true;
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::MouseMove { x, y } if erasing && in_zone(*x, *y) => {
            let (frame, y_frac) = screen_to_drawing(
                *x,
                *y,
                ctx.zone.x,
                ctx.zone.y,
                ctx.zone.width,
                ctx.zone.height,
                ctx.current_frame,
                ppf,
                reading_bar_offset_frames,
            );
            let stroke_ids = ctx.project.drawing().strokes_within_radius(
                frame,
                y_frac,
                ppf,
                ctx.zone.height,
                brush_radius_frac,
            );
            if !stroke_ids.is_empty() {
                return Some(EventResponse::Action(UiAction::EraseDrawingStrokes(
                    stroke_ids,
                )));
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::MouseMove { x, y } if state.active_stroke.is_some() && in_zone(*x, *y) => {
            let (frame, y_frac) = screen_to_drawing(
                *x,
                *y,
                ctx.zone.x,
                ctx.zone.y,
                ctx.zone.width,
                ctx.zone.height,
                ctx.current_frame,
                ppf,
                reading_bar_offset_frames,
            );
            if let Some(stroke) = state.active_stroke.as_mut() {
                stroke.points.push((frame, y_frac));
                state.drawing_dirty = true;
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::MouseRelease { .. } if state.active_stroke.is_some() => {
            if let Some(stroke) = state.active_stroke.take() {
                if stroke.points.len() > 1 {
                    state.drawing_dirty = true;
                    return Some(EventResponse::Action(UiAction::AddDrawingStroke(stroke)));
                }
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::MousePress { x, y }
        | UiEvent::MouseMove { x, y }
        | UiEvent::MouseRelease { x, y }
            if in_zone(*x, *y) =>
        {
            Some(EventResponse::Consumed)
        }
        UiEvent::MousePress { .. } | UiEvent::MouseMove { .. } | UiEvent::MouseRelease { .. } => {
            Some(EventResponse::Ignored)
        }
        _ => None,
    }
}
