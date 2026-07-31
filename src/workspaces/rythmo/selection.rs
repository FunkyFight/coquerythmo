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

pub(crate) fn selected_line_origins(
    project: &Project,
    selection: &Selection,
) -> Vec<DragLineOrigin> {
    match selection {
        Selection::AllLines => all_line_origins(project),
        Selection::Lines(ids) => project
            .lines()
            .filter(|line| ids.contains(&line.id))
            .map(|line| DragLineOrigin {
                line_id: line.id,
                original_frame: line.start_frame,
                original_y_slot: line.y_slot,
            })
            .collect(),
        Selection::Line(_)
        | Selection::Marker(_)
        | Selection::Detection(_)
        | Selection::Strokes(_) => Vec::new(),
    }
}

pub(crate) fn clamp_group_y_delta(origins: &[DragLineOrigin], requested_delta: f32) -> f32 {
    let min_y = origins
        .iter()
        .map(|origin| origin.original_y_slot)
        .fold(f32::INFINITY, f32::min);
    let max_y = origins
        .iter()
        .map(|origin| origin.original_y_slot)
        .fold(f32::NEG_INFINITY, f32::max);
    if origins.is_empty() {
        requested_delta
    } else {
        requested_delta.clamp(-min_y, 0.75 - max_y)
    }
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
    let ppf = crate::rythmo_drawing::ppf_for_scale(1.0, project.settings().scroll_speed);
    let reading_bar_offset_frames = project.settings().reading_bar_offset_percent as f64 / 100.0
        * zone.width as f64
        / ppf as f64;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for s in &strokes {
        for (f, y_frac) in &s.points {
            let (x, y) = crate::rythmo_drawing::drawing_to_screen(
                *f,
                *y_frac,
                zone.x,
                zone.y,
                zone.width,
                zone.height,
                current_frame,
                ppf,
                reading_bar_offset_frames,
            );
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
        let rect = drag.rect;
        let x = rect.x.min(rect.x + rect.width);
        let y = rect.y.min(rect.y + rect.height);
        let w = rect.width.abs();
        let h = rect.height.abs();
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

/// Finalize a marquee selection: select overlapping lines first, then drawing
/// strokes. An additive drag merges with the compatible current selection.
pub(crate) fn finalize_marquee_selection(ctx: &RythmoCtx, state: &mut RythmoState) {
    if let Some(drag) = state.selection_drag.take() {
        let rect = drag.rect;
        let zone_min_x = ctx.zone.x;
        let zone_max_x = ctx.zone.x + ctx.zone.width;
        let zone_min_y = ctx.zone.y;
        let zone_max_y = ctx.zone.y + ctx.zone.height;
        let start_x = rect.x.clamp(zone_min_x, zone_max_x);
        let end_x = (rect.x + rect.width).clamp(zone_min_x, zone_max_x);
        let start_y = rect.y.clamp(zone_min_y, zone_max_y);
        let end_y = (rect.y + rect.height).clamp(zone_min_y, zone_max_y);
        let min_x = start_x.min(end_x);
        let max_x = start_x.max(end_x);
        let min_y = start_y.min(end_y);
        let max_y = start_y.max(end_y);
        let ppf = crate::rythmo_drawing::ppf_for_scale(1.0, ctx.project.settings().scroll_speed);
        let reading_bar_offset_frames = crate::rythmo_layout::reading_bar_offset_seconds(
            ctx.project.settings().reading_bar_offset_percent,
            ctx.zone.width,
            ctx.fps,
            ppf,
        ) * ctx.fps;
        let (min_frame, min_y_frac) = crate::rythmo_drawing::screen_to_drawing(
            min_x,
            min_y,
            ctx.zone.x,
            ctx.zone.y,
            ctx.zone.width,
            ctx.zone.height,
            ctx.current_frame,
            ppf,
            reading_bar_offset_frames,
        );
        let (max_frame, max_y_frac) = crate::rythmo_drawing::screen_to_drawing(
            max_x,
            max_y,
            ctx.zone.x,
            ctx.zone.y,
            ctx.zone.width,
            ctx.zone.height,
            ctx.current_frame,
            ppf,
            reading_bar_offset_frames,
        );

        let selection_rect = Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        };
        let line_ids: Vec<u64> = ctx
            .project
            .lines()
            .filter(|line| {
                rects_overlap(
                    &line_rect(
                        ctx.project,
                        line,
                        ctx.current_frame,
                        ctx.zone,
                        crate::config::reading_bar_offset_seconds(),
                        ctx.fps,
                    ),
                    &selection_rect,
                )
            })
            .map(|line| line.id)
            .collect();
        if !line_ids.is_empty() {
            if drag.additive && matches!(state.selected.as_ref(), Some(Selection::AllLines)) {
                return;
            }
            let mut selected_ids = if drag.additive {
                match state.selected.as_ref() {
                    Some(Selection::Line(id)) => vec![*id],
                    Some(Selection::Lines(ids)) => ids.clone(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            for line_id in line_ids {
                if !selected_ids.contains(&line_id) {
                    selected_ids.push(line_id);
                }
            }
            let selected_ids: Vec<u64> = ctx
                .project
                .lines()
                .filter(|line| selected_ids.contains(&line.id))
                .map(|line| line.id)
                .collect();
            state.selected = Some(if selected_ids.len() == 1 {
                Selection::Line(selected_ids[0])
            } else {
                Selection::Lines(selected_ids)
            });
            return;
        }

        let stroke_ids = ctx.project.drawing().strokes_in_rect(
            min_frame.min(max_frame),
            min_y_frac.min(max_y_frac),
            min_frame.max(max_frame),
            min_y_frac.max(max_y_frac),
        );
        state.selected = if drag.additive {
            match state.selected.take() {
                Some(Selection::Strokes(mut selected_ids)) => {
                    for stroke_id in stroke_ids {
                        if !selected_ids.contains(&stroke_id) {
                            selected_ids.push(stroke_id);
                        }
                    }
                    Some(Selection::Strokes(selected_ids))
                }
                Some(selection) => Some(selection),
                None if stroke_ids.is_empty() => None,
                None => Some(Selection::Strokes(stroke_ids)),
            }
        } else if stroke_ids.is_empty() {
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
    additive: bool,
) -> EventResponse {
    state.selection_drag = Some(SelectionDrag {
        rect: Rect {
            x,
            y,
            width: 0.0,
            height: 0.0,
        },
        additive,
    });
    EventResponse::Consumed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_selects_multiple_lines() {
        crate::config::init();
        let offset = crate::config::reading_bar_offset_seconds();
        let fps = 24.0;
        let mut project = Project::new();
        let first = project.add_line_full(
            0,
            20,
            0.0,
            "First".into(),
            "Alice".into(),
            [1.0, 0.0, 0.0, 1.0],
        );
        let second = project.add_line_full(
            30,
            20,
            0.25,
            "Second".into(),
            "Bob".into(),
            [0.0, 1.0, 0.0, 1.0],
        );
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 600.0,
        };
        let first_rect = line_rect(
            &project,
            project.get_line(first).unwrap(),
            0.0,
            &zone,
            offset,
            fps,
        );
        let second_rect = line_rect(
            &project,
            project.get_line(second).unwrap(),
            0.0,
            &zone,
            offset,
            fps,
        );
        let min_x = first_rect.x.min(second_rect.x) - 2.0;
        let min_y = first_rect.y.min(second_rect.y) - 2.0;
        let max_x = (first_rect.x + first_rect.width).max(second_rect.x + second_rect.width) + 2.0;
        let max_y =
            (first_rect.y + first_rect.height).max(second_rect.y + second_rect.height) + 2.0;
        let render_index = ProjectRenderIndex::new();
        let ctx = RythmoCtx {
            zone: &zone,
            project: &project,
            render_index: &render_index,
            current_frame: 0.0,
            karaoke_preview: false,
            fps,
            active_mode: ToolMode::Select,
        };
        let mut state = RythmoState::new();
        state.selection_drag = Some(SelectionDrag {
            rect: Rect {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            },
            additive: false,
        });

        finalize_marquee_selection(&ctx, &mut state);

        match state.selected.as_ref() {
            Some(Selection::Lines(ids)) => {
                assert_eq!(ids.as_slice(), &[first, second]);
            }
            other => panic!("expected multiple line selection, got {other:?}"),
        }

        state.selected = Some(Selection::Line(first));
        let response = handle_shift_mouse_press(&ctx, &mut state, zone.x + 2.0, zone.y + 2.0);
        assert!(matches!(response, EventResponse::Consumed));
        assert!(state
            .selection_drag
            .take()
            .is_some_and(|drag| drag.additive));

        state.selection_drag = Some(SelectionDrag {
            rect: Rect {
                x: second_rect.x - 2.0,
                y: second_rect.y - 2.0,
                width: second_rect.width + 4.0,
                height: second_rect.height + 4.0,
            },
            additive: true,
        });

        finalize_marquee_selection(&ctx, &mut state);

        match state.selected.as_ref() {
            Some(Selection::Lines(ids)) => assert_eq!(ids.as_slice(), &[first, second]),
            other => panic!("expected additive line selection, got {other:?}"),
        }
    }

    #[test]
    fn group_vertical_delta_preserves_spacing_at_track_boundaries() {
        let origins = vec![
            DragLineOrigin {
                line_id: 1,
                original_frame: 0,
                original_y_slot: 0.25,
            },
            DragLineOrigin {
                line_id: 2,
                original_frame: 0,
                original_y_slot: 0.75,
            },
        ];

        assert_eq!(clamp_group_y_delta(&origins, 0.25), 0.0);
        assert_eq!(clamp_group_y_delta(&origins, -0.5), -0.25);
    }
}
