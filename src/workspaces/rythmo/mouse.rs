//! Pointer move controller for the rythmo workspace.

use super::*;

pub(crate) fn handle_mouse_move(ctx: &mut RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // Autocomplete hover tracking
    if state.editing_character.is_some() {
        let new_hover = autocomplete_hover_index(ctx, state, x, y);
        if new_hover != state.autocomplete_hover {
            state.autocomplete_hover = new_hover;
            // Also set keyboard index to match mouse for Enter to work
            if new_hover.is_some() {
                state.autocomplete_index = new_hover;
            }
            return EventResponse::Consumed;
        }
    }

    // Handle transform handle drag
    if state.transform_handle.is_some() {
        return handle_transform_drag(ctx, state, x, y);
    }

    // Handle marquee selection drag
    if ctx.active_mode == ToolMode::Select {
        if let Some(ref mut drag) = state.selection_drag {
            drag.width = x - drag.x;
            drag.height = y - drag.y;
            return EventResponse::Consumed;
        }
    }

    if let Some(drag) = &state.dragging {
        let dx_frames = ((x - drag.drag_start_x) / ppf()) as i64;
        return match &drag.target {
            DragTarget::Marker(idx) => {
                let new_frame = drag.original_frame + dx_frames;
                EventResponse::Action(UiAction::MoveMarker {
                    index: *idx,
                    frame: new_frame,
                })
            }
            DragTarget::Line(line_id) => {
                let line_id = *line_id;
                match drag.handle {
                    DragHandle::Left => {
                        let end = drag.original_frame + drag.original_duration;
                        let ns = (drag.original_frame + dx_frames).min(end - 1);
                        EventResponse::Action(UiAction::ResizeLine {
                            id: line_id,
                            start_frame: ns,
                            duration_frames: end - ns,
                        })
                    }
                    DragHandle::Right => EventResponse::Action(UiAction::ResizeLine {
                        id: line_id,
                        start_frame: drag.original_frame,
                        duration_frames: (drag.original_duration + dx_frames).max(1),
                    }),
                    DragHandle::Selection => {
                        if let Some(line) = ctx.project.get_line(line_id) {
                            let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
                            let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                            state.pending_cursor_click = Some((ratio, true));

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
                        }
                        EventResponse::Consumed
                    }
                    DragHandle::Body => {
                        let candidate = y_to_slot(ctx.project, y, ctx.zone);
                        let new_y_slot = if candidate != drag.original_y_slot {
                            let layouts = editor_track_layouts(ctx.project, ctx.zone);
                            let orig_track_idx =
                                rythmo_layout::track_index_for_y_slot(drag.original_y_slot);
                            let orig_track =
                                rythmo_layout::track_for_index(&layouts, orig_track_idx)
                                    .unwrap_or_else(|| {
                                        layouts
                                            .first()
                                            .expect("editor track layout should not be empty")
                                    });
                            let orig_center = ctx.zone.y
                                + constants::RULER_HEIGHT
                                + orig_track.top
                                + orig_track.total_h / 2.0;
                            if (y - orig_center).abs() > orig_track.total_h * 0.6 {
                                candidate
                            } else {
                                drag.original_y_slot
                            }
                        } else {
                            drag.original_y_slot
                        };
                        if !drag.group_origins.is_empty()
                            && matches!(state.selected, Some(Selection::AllLines))
                        {
                            let y_delta = new_y_slot - drag.original_y_slot;
                            let moves = drag
                                .group_origins
                                .iter()
                                .map(|origin| {
                                    (
                                        origin.line_id,
                                        origin.original_frame + dx_frames,
                                        (origin.original_y_slot + y_delta).clamp(0.0, 0.75),
                                    )
                                })
                                .collect();
                            return EventResponse::Action(UiAction::MoveLines { moves });
                        }
                        EventResponse::Action(UiAction::MoveLine {
                            id: line_id,
                            start_frame: drag.original_frame + dx_frames,
                            y_slot: new_y_slot,
                        })
                    }
                }
            }
        };
    }

    // Ghost preview when CTRL held and hovering empty BR space
    if state.ctrl_held && ctx.zone.contains(x, y) {
        let on_line = ctx
            .project
            .lines()
            .any(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y));
        if !on_line {
            let frame = x_to_frame(x, ctx.current_frame, ctx.zone);
            let y_slot = y_to_slot(ctx.project, y, ctx.zone);
            state.ghost_preview = Some(GhostPreview {
                frame,
                y_slot,
                duration_frames: clamped_new_line_duration(ctx.project, frame, y_slot, ctx.fps),
            });
            return EventResponse::Consumed;
        }
    }
    // Clear ghost when not applicable
    if state.ghost_preview.is_some() {
        state.ghost_preview = None;
    }

    if !ctx.zone.contains(x, y) {
        let mut consumed = false;
        if state.hovered_line.take().is_some() {
            consumed = true;
        }
        if state.hovered_track.take().is_some() {
            consumed = true;
        }
        return if consumed {
            EventResponse::Consumed
        } else {
            EventResponse::Ignored
        };
    }

    let found = ctx
        .project
        .lines()
        .find(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y))
        .map(|l| l.id);

    let hovered_track = {
        let relative_y = y - ctx.zone.y - constants::RULER_HEIGHT;
        editor_track_layouts(ctx.project, ctx.zone)
            .iter()
            .find(|layout| relative_y >= layout.top && relative_y < layout.top + layout.total_h)
            .map(|layout| layout.track_index)
    };

    let mut changed = false;
    if found != state.hovered_line {
        state.hovered_line = found;
        changed = true;
    }
    if hovered_track != state.hovered_track {
        state.hovered_track = hovered_track;
        changed = true;
    }

    if changed {
        EventResponse::Consumed
    } else {
        EventResponse::Ignored
    }
}

