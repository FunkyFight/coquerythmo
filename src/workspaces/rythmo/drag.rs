//! Drawing transform drag controller for the rythmo workspace.

use super::*;

pub(crate) fn handle_transform_drag(
    ctx: &mut RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    let Some(ref mut handle) = state.transform_handle else {
        return EventResponse::Ignored;
    };

    let dx = x - handle.start_mouse.0;
    let dy = y - handle.start_mouse.1;

    let start_bbox = handle.start_bbox;
    let cx = (start_bbox.0 + start_bbox.2) / 2.0;
    let cy = (start_bbox.1 + start_bbox.3) / 2.0;

    let ppf = crate::rythmo_drawing::ppf_for_scale(1.0);

    // Screen-space bbox center (for rotation, which works in pixels).
    let center_x = ctx.zone.x + ctx.zone.width / 2.0;
    let cx_screen = center_x + (cx - ctx.current_frame) as f32 * ppf;
    let cy_screen = ctx.zone.y + cy * ctx.zone.height;

    let (translate, rotate, scale) = match handle.kind {
        TransformHandleKind::Move => {
            let dx_frames = dx / ppf;
            let mut dy_frac = dy / ctx.zone.height;
            // Keep the selection inside the vertical drawing area (y_frac in [0, 1])
            // so it cannot be dragged out of the zone, e.g. above the top.
            let new_min_y = start_bbox.1 + dy_frac;
            let new_max_y = start_bbox.3 + dy_frac;
            if new_min_y < 0.0 {
                dy_frac = -start_bbox.1;
            } else if new_max_y > 1.0 {
                dy_frac = 1.0 - start_bbox.3;
            }
            ((dx_frames as f64, dy_frac), 0.0, 1.0)
        }
        TransformHandleKind::TopLeft
        | TransformHandleKind::TopRight
        | TransformHandleKind::BottomLeft
        | TransformHandleKind::BottomRight => {
            let bbox_w = start_bbox.2 - start_bbox.0;
            let bbox_h = start_bbox.3 - start_bbox.1;
            if bbox_w.abs() < f64::EPSILON || bbox_h.abs() < f32::EPSILON {
                ((0.0, 0.0), 0.0, 1.0)
            } else {
                let scale_x = match handle.kind {
                    TransformHandleKind::TopLeft | TransformHandleKind::BottomLeft => {
                        1.0 - dx / (bbox_w as f32 * ppf)
                    }
                    TransformHandleKind::TopRight | TransformHandleKind::BottomRight => {
                        1.0 + dx / (bbox_w as f32 * ppf)
                    }
                    _ => 1.0,
                };
                let scale_y = match handle.kind {
                    TransformHandleKind::TopLeft | TransformHandleKind::TopRight => {
                        1.0 - dy / (bbox_h * ctx.zone.height)
                    }
                    TransformHandleKind::BottomLeft | TransformHandleKind::BottomRight => {
                        1.0 + dy / (bbox_h * ctx.zone.height)
                    }
                    _ => 1.0,
                };
                let scale = (scale_x * scale_y).sqrt().max(0.01);
                ((0.0, 0.0), 0.0, scale)
            }
        }
        TransformHandleKind::Rotate => {
            // Convert the mouse vector (relative to the bbox center) into
            // world space (frames, y_frac) so the rotation matches the mouse
            // under the anisotropic x/y projection instead of skewing.
            let start_dx = handle.start_mouse.0 - cx_screen;
            let start_dy = handle.start_mouse.1 - cy_screen;
            let cur_dx = x - cx_screen;
            let cur_dy = y - cy_screen;
            let start_world_angle = (start_dy / ctx.zone.height).atan2(start_dx / ppf);
            let cur_world_angle = (cur_dy / ctx.zone.height).atan2(cur_dx / ppf);
            let angle_diff = cur_world_angle - start_world_angle;
            ((0.0, 0.0), angle_diff, 1.0)
        }
    };

    handle.current_stroke_points = handle
        .current_stroke_points
        .iter()
        .map(|points| {
            crate::rythmo_drawing::transformed_points(points, (cx, cy), translate, rotate, scale)
        })
        .collect();

    handle.start_mouse = (x, y);

    EventResponse::Action(UiAction::TransformStrokes {
        stroke_ids: handle.stroke_ids.clone(),
        old_points: handle.start_stroke_points.clone(),
        new_points: handle.current_stroke_points.clone(),
    })
}
