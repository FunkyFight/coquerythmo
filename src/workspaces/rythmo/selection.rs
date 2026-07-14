//! Stroke selection and marquee interaction for the rythmo workspace.

use super::*;

pub(crate) fn all_line_origins(project: &Project) -> Vec<DragLineOrigin> {
    project
        .lines()
        .map(|line| DragLineOrigin {
            line_id: line.id,
            original_frame: line.start_frame,
            original_y_slot: line.y_slot,
        })
        .collect()
}

/// Compute the screen-space bounding box of selected strokes.
/// Returns (min_x, min_y, max_x, max_y) in screen pixels, or None if no strokes selected.
pub(crate) fn selected_strokes_screen_bbox(
    zone: &Rect,
    current_frame: f64,
    project: &Project,
    state: &RythmoState,
) -> Option<(f32, f32, f32, f32)> {
    let Selection::Strokes(ref ids) = state.selected.as_ref()? else {
        return None;
    };
    let strokes: Vec<&DrawingStroke> = ids
        .iter()
        .filter_map(|id| project.drawing().get(*id))
        .collect();
    if strokes.is_empty() {
        return None;
    }
    let ppf = crate::rythmo_drawing::ppf_for_scale(1.0);
    let center_x = zone.x + zone.width / 2.0;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for s in &strokes {
        for (f, y_frac) in &s.points {
            let x = center_x + (*f - current_frame) as f32 * ppf;
            let y = zone.y + y_frac * zone.height;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    Some((min_x, min_y, max_x, max_y))
}

/// Draw the live marquee rectangle and the selected-strokes bounding box with
/// transform handles into the given quad list. The quad list is expected to be
/// composited above the drawing overlay so the handles remain visible.
pub(crate) fn render_selection_overlay(
    zone: &Rect,
    current_frame: f64,
    project: &Project,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
) {
    let accent: [f32; 4] = [0.3, 0.9, 1.0, 1.0];

    // Live marquee rectangle (screen-space Rect in pixels)
    if let Some(drag) = &state.selection_drag {
        let x = drag.x.min(drag.x + drag.width);
        let y = drag.y.min(drag.y + drag.height);
        let w = drag.width.abs();
        let h = drag.height.abs();
        let fill: [f32; 4] = [0.2, 0.8, 0.9, 0.12];
        quads.push(QuadInstance {
            rect: [x, y, w, h],
            color: fill,
            color_bottom: fill,
            border_color: accent,
            border_width: 1.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    // Selected strokes bounding box + corner & rotate handles
    let Some((min_x, min_y, max_x, max_y)) =
        selected_strokes_screen_bbox(zone, current_frame, project, state)
    else {
        return;
    };

    // Bounding box outline
    quads.push(QuadInstance {
        rect: [min_x, min_y, max_x - min_x, max_y - min_y],
        color: [0.0; 4],
        color_bottom: [0.0; 4],
        border_color: accent,
        border_width: 1.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });

    let hs = 6.0; // half-size of handle squares in pixels
    let corners = [
        (min_x, min_y),
        (max_x, min_y),
        (min_x, max_y),
        (max_x, max_y),
    ];
    for (cx, cy) in corners {
        quads.push(QuadInstance {
            rect: [cx - hs, cy - hs, hs * 2.0, hs * 2.0],
            color: accent,
            color_bottom: accent,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    // Rotate handle above the top-center edge, with a connector line
    let rotate_offset = 24.0;
    let cx = (min_x + max_x) / 2.0;
    quads.push(QuadInstance {
        rect: [cx - 0.5, min_y - rotate_offset, 1.0, rotate_offset],
        color: accent,
        color_bottom: accent,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [cx - hs, min_y - rotate_offset - hs, hs * 2.0, hs * 2.0],
        color: accent,
        color_bottom: accent,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

/// Hit-test the transform handles of the selected strokes' bounding box.
/// Returns the handle kind if hit, or None.
pub(crate) fn hit_test_transform_handles(
    ctx: &RythmoCtx,
    state: &RythmoState,
    project: &Project,
    x: f32,
    y: f32,
) -> Option<TransformHandleKind> {
    let (min_x, min_y, max_x, max_y) =
        selected_strokes_screen_bbox(ctx.zone, ctx.current_frame, project, state)?;
    let handle_size = 12.0;
    let rotate_offset = 24.0;

    // Check corner handles
    if (x - min_x).abs() < handle_size && (y - min_y).abs() < handle_size {
        return Some(TransformHandleKind::TopLeft);
    }
    if (x - max_x).abs() < handle_size && (y - min_y).abs() < handle_size {
        return Some(TransformHandleKind::TopRight);
    }
    if (x - min_x).abs() < handle_size && (y - max_y).abs() < handle_size {
        return Some(TransformHandleKind::BottomLeft);
    }
    if (x - max_x).abs() < handle_size && (y - max_y).abs() < handle_size {
        return Some(TransformHandleKind::BottomRight);
    }

    // Check rotate handle (top-center, above bbox)
    let cx = (min_x + max_x) / 2.0;
    if (x - cx).abs() < handle_size && (y - (min_y - rotate_offset)).abs() < handle_size {
        return Some(TransformHandleKind::Rotate);
    }

    // Check move handle (bbox body)
    if x >= min_x && x <= max_x && y >= min_y && y <= max_y {
        return Some(TransformHandleKind::Move);
    }

    None
}

/// Start transform handle drag, capturing original stroke points.
pub(crate) fn start_transform_drag(
    state: &mut RythmoState,
    project: &Project,
    kind: TransformHandleKind,
    x: f32,
    y: f32,
) {
    let Selection::Strokes(ids) = state.selected.clone().unwrap() else {
        return;
    };
    let strokes: Vec<&DrawingStroke> = ids
        .iter()
        .filter_map(|id| project.drawing().get(*id))
        .collect();
    if strokes.is_empty() {
        return;
    }
    let start_stroke_points: Vec<Vec<(f64, f32)>> =
        strokes.iter().map(|s| s.points.clone()).collect();
    let bbox = strokes_bbox(&strokes);
    let start_bbox = bbox.unwrap_or((0.0, 0.0, 0.0, 0.0));
    state.transform_handle = Some(TransformHandle {
        kind,
        start_mouse: (x, y),
        start_bbox,
        current_stroke_points: start_stroke_points.clone(),
        start_stroke_points,
        stroke_ids: ids,
    });
}

/// Finalize a marquee selection: convert the live drag rectangle into a
/// frame-space query and select the enclosed strokes (clears selection if none).
pub(crate) fn finalize_marquee_selection(ctx: &RythmoCtx, state: &mut RythmoState) {
    if let Some(drag) = state.selection_drag.take() {
        let min_x = drag.x.min(drag.x + drag.width);
        let max_x = drag.x.max(drag.x + drag.width);
        let min_y = drag.y.min(drag.y + drag.height);
        let max_y = drag.y.max(drag.y + drag.height);
        let ppf = crate::rythmo_drawing::ppf_for_scale(1.0);
        let center_x = ctx.zone.x + ctx.zone.width / 2.0;
        let min_frame = ctx.current_frame + (min_x - center_x) as f64 / ppf as f64;
        let max_frame = ctx.current_frame + (max_x - center_x) as f64 / ppf as f64;
        let min_y_frac = ((min_y - ctx.zone.y) / ctx.zone.height).clamp(0.0, 1.0);
        let max_y_frac = ((max_y - ctx.zone.y) / ctx.zone.height).clamp(0.0, 1.0);
        let stroke_ids = ctx.project.drawing().strokes_in_rect(
            min_frame.min(max_frame),
            min_y_frac.min(max_y_frac),
            min_frame.max(max_frame),
            min_y_frac.max(max_y_frac),
        );
        state.selected = if stroke_ids.is_empty() {
            None
        } else {
            Some(Selection::Strokes(stroke_ids))
        };
    }
}

/// Handle marquee selection drag start (move is handled directly in
/// `handle_mouse_move`, finalize in `handle_mouse_release`).
pub(crate) fn handle_selection_drag(
    state: &mut RythmoState,
    x: f32,
    y: f32,
    event: &UiEvent,
) -> Option<EventResponse> {
    match event {
        UiEvent::MousePress { .. } => {
            // Start marquee selection on empty space
            state.selection_drag = Some(Rect {
                x,
                y,
                width: 0.0,
                height: 0.0,
            });
            Some(EventResponse::Consumed)
        }
        _ => None,
    }
}
