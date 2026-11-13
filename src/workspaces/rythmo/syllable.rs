//! Syllable editing controller for the rythmo workspace.

use super::*;

// ── Syllable mode helpers ──────────────────────────────────────────────────

pub(crate) fn syllable_mouse_press(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
    preserve_prefix: bool,
) -> Option<EventResponse> {
    if !ctx.zone.contains(x, y) {
        return None;
    }

    let lang = ctx.project.syllable_language_code();
    let pointer_frame = x_to_frame(x, ctx.current_frame, ctx.zone, ctx.fps);
    let mut candidate_ids =
        ctx.render_index
            .visible_line_ids(ctx.project, pointer_frame, pointer_frame);
    candidate_ids.sort_by_key(|line_id| ctx.render_index.line_order_index(*line_id));

    let candidates: Vec<(u64, Rect)> = {
        let layout_ctx = state.get_or_create_layout_ctx(
            ctx.project,
            ctx.render_index,
            ctx.current_frame,
            ctx.fps,
            ctx.zone,
        );
        candidate_ids
            .into_iter()
            .filter_map(|line_id| {
                let line = ctx.project.get_line(line_id)?;
                if !line.karaoke || ctx.karaoke_preview {
                    return None;
                }
                let rect = layout_ctx.line_rect_with_karaoke_width(
                    line,
                    ctx.current_frame,
                    ctx.zone,
                    false,
                    None,
                    crate::config::reading_bar_offset_seconds(),
                    ctx.fps,
                );
                Some((line_id, rect))
            })
            .collect()
    };

    for (line_id, r) in candidates {
        let Some(line) = ctx.project.get_line(line_id) else {
            continue;
        };

        // The pin under a word switches it between its spoken syllables and
        // all its written ones. Like the handles, it shows on the hovered line.
        if state.hovered_line == Some(line.id) && state.syllable_drag.is_none() {
            let pin_top = syllable_pin_top(&r);
            if y >= pin_top - 3.0 && y <= pin_top + SYLLABLE_PIN_SIZE + 3.0 {
                let pins = syllable_pin_positions(ctx.project, line, None, lang, state);
                let hit = pins.into_iter().find(|(_, pin_x)| {
                    (x - (r.x + pin_x * r.width)).abs() <= SYLLABLE_PIN_SIZE / 2.0 + 3.0
                });
                if let Some((pin, _)) = hit {
                    return Some(EventResponse::Action(UiAction::ToggleSyllableSplitWord {
                        line_id: line.id,
                        word: pin.word,
                    }));
                }
                // The arrow between two words that can be linked: nothing,
                // liaison made, liaison not made.
                let arrows = liaison_arrow_positions(ctx.project, line, None, lang, state);
                let hit = arrows.into_iter().find(|(_, from, to)| {
                    x >= r.x + from * r.width - 4.0 && x <= r.x + to * r.width + 4.0
                });
                if let Some((site, _, _)) = hit {
                    return Some(EventResponse::Action(UiAction::CycleLiaisonMark {
                        line_id: line.id,
                        pair: site.pair,
                    }));
                }
            }
        }

        // A handle runs down the whole line: grabbed wide along the top,
        // and on the bar itself further down.
        let Some(hit_w) = syllable_handle_reach(&r, y, state.editing_line == Some(line.id))
        else {
            continue;
        };

        if state.slot_reservation_for(line).is_some() {
            // The handles around a reserved slot are not draggable: the slot
            // is settled by what gets typed (or erased) there.
            continue;
        }
        let Some(ratios) =
            syllable_ratios_for_line(line, state.syllable_drag.as_ref(), lang, state)
        else {
            continue;
        };
        if ratios.len() <= 1 {
            continue;
        }

        if let Some(boundaries) = sync_syllable_boundary_ratios(
            ctx.project,
            line,
            state.syllable_drag.as_ref(),
            lang,
            state,
        ) {
            for separator_index in 0..ratios.len() - 1 {
                let Some(boundary_ratio) = boundaries.get(separator_index + 1).copied() else {
                    continue;
                };
                let separator_x = r.x + boundary_ratio * r.width;
                if (x - separator_x).abs() < hit_w {
                    state.hovered_line = None;
                    let encoded_line_id = encode_sync_syllable_drag_line_id(line.id);
                    state.syllable_drag = Some(SyllableDrag {
                        line_id: encoded_line_id,
                        separator_index,
                        ratios: ratios.clone(),
                        original_ratios: ratios.clone(),
                        drag_start_x: x,
                        line_rect: r,
                        preserve_prefix,
                    });
                    crate::workspaces::rythmo::detection_ui::begin_sync_syllable_drag(
                        ctx.project,
                        line,
                        encoded_line_id,
                        separator_index,
                        state,
                    );
                    return Some(EventResponse::Consumed);
                }
            }
            continue;
        }

        if state.hovered_line != Some(line.id) || !r.contains(x, y) {
            continue;
        }

        let mut separator_x = r.x;
        for (separator_index, ratio) in ratios.iter().enumerate() {
            separator_x += ratio * r.width;
            if separator_index < ratios.len() - 1 && (x - separator_x).abs() < hit_w {
                crate::workspaces::rythmo::detection_ui::clear_sync_syllable_drag();
                state.syllable_drag = Some(SyllableDrag {
                    line_id: line.id,
                    separator_index,
                    ratios: ratios.clone(),
                    original_ratios: ratios.clone(),
                    drag_start_x: x,
                    line_rect: r,
                    preserve_prefix,
                });
                return Some(EventResponse::Consumed);
            }
        }
    }

    None
}

/// The karaoke line already hovered, while the pointer is over the pins
/// drawn just under it: leaving the line by its bottom edge to reach a pin
/// must not hide the pins.
pub(crate) fn hovered_line_under_pointer_pins(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> Option<u64> {
    let line_id = state.hovered_line?;
    let line = ctx.project.get_line(line_id)?;
    if !line.karaoke || ctx.karaoke_preview {
        return None;
    }
    let rect = state
        .get_or_create_layout_ctx(
            ctx.project,
            ctx.render_index,
            ctx.current_frame,
            ctx.fps,
            ctx.zone,
        )
        .line_rect_with_karaoke_width(
            line,
            ctx.current_frame,
            ctx.zone,
            false,
            None,
            crate::config::reading_bar_offset_seconds(),
            ctx.fps,
        );
    if !syllable_pin_strip_contains(&rect, x, y) {
        return None;
    }
    let lang = ctx.project.syllable_language_code();
    let has_pins =
        !crate::syllable::syllable_split_pins(&line.text, line.syllable_rules(lang)).is_empty();
    let has_liaisons = || !crate::syllable::liaison_sites(&line.text, lang).is_empty();
    (has_pins || has_liaisons()).then_some(line_id)
}

pub(crate) fn syllable_mouse_move(state: &mut RythmoState, x: f32) -> Option<EventResponse> {
    // Handles already placed on this line bound the proportional drag.
    let free_range = state.syllable_drag.as_ref().map(|drag| {
        state.syllable_drag_free_range(
            decode_sync_syllable_drag_line_id(drag.line_id).unwrap_or(drag.line_id),
            drag.separator_index,
            drag.ratios.len(),
        )
    })?;
    let drag = state.syllable_drag.as_mut()?;

    let dx = x - drag.drag_start_x;
    let delta_ratio = dx / drag.line_rect.width;
    drag.drag_start_x = x;

    let i = drag.separator_index;
    let segment_count = drag.ratios.len();
    let min_ratio = syllable_drag_min_ratio(segment_count, drag.line_rect.width);
    if delta_ratio.abs() <= 0.0001 || i + 1 >= segment_count {
        return Some(EventResponse::Consumed);
    }

    // Resolve the synchronization interval before handling either drag mode.
    // In particular, Ctrl/preserve-prefix used to bypass this guard entirely
    // and could move a separator across an adjacent synchronization point.
    let local_range = crate::workspaces::rythmo::detection_ui::active_sync_syllable_edit_range(
        drag.line_id,
        segment_count,
    );

    if drag.preserve_prefix {
        if !separator_is_inside_edit_range(i, local_range) {
            return Some(EventResponse::Consumed);
        }
        let left = i;
        let right = i + 1;
        let applied = if delta_ratio > 0.0 {
            delta_ratio.min((drag.ratios[right] - min_ratio).max(0.0))
        } else {
            (-delta_ratio).min((drag.ratios[left] - min_ratio).max(0.0))
        };
        if applied > 0.0 {
            if delta_ratio > 0.0 {
                drag.ratios[left] += applied;
                drag.ratios[right] -= applied;
            } else {
                drag.ratios[left] -= applied;
                drag.ratios[right] += applied;
            }
        }
        return Some(EventResponse::Consumed);
    }

    // With synchronization points, redistribution is local to the interval
    // bounded by the previous and next point. Handles already placed narrow
    // it further: a line is timed by halves, each drag only sharing out what
    // is still free around its handle.
    let (range_start, range_end) = local_range.unwrap_or((0, segment_count));
    let range_start = range_start.max(free_range.0);
    let range_end = range_end.min(free_range.1);
    let whole_line = range_start == 0 && range_end == segment_count;
    let left_end = i + 1;
    let right_start = i + 1;
    if range_start >= left_end || right_start >= range_end {
        return Some(EventResponse::Consumed);
    }

    let left_total: f32 = drag.ratios[range_start..left_end].iter().sum();
    let right_total: f32 = drag.ratios[right_start..range_end].iter().sum();
    let left_count = left_end - range_start;
    let right_count = range_end - right_start;
    let left_min_total = min_ratio * left_count as f32;
    let right_min_total = min_ratio * right_count as f32;

    if delta_ratio > 0.0 {
        let applied = delta_ratio.min((right_total - right_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[range_start..left_end],
                left_total + applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..range_end],
                right_total - applied,
                min_ratio,
            );
        }
    } else {
        let applied = (-delta_ratio).min((left_total - left_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[range_start..left_end],
                left_total - applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..range_end],
                right_total + applied,
                min_ratio,
            );
        }
    }

    // The local transfer preserves the total exactly. Avoid a whole-vector
    // normalization here: even tiny renormalization would move text outside the
    // active synchronization interval.
    if local_range.is_none() && whole_line {
        normalize_ratios_in_place(&mut drag.ratios);
    }

    Some(EventResponse::Consumed)
}

pub(crate) fn separator_is_inside_edit_range(
    separator_index: usize,
    edit_range: Option<(usize, usize)>,
) -> bool {
    edit_range.is_none_or(|(start, end)| start <= separator_index && separator_index + 1 < end)
}

pub(crate) fn syllable_drag_min_ratio(segment_count: usize, line_width: f32) -> f32 {
    if segment_count == 0 || line_width <= 1.0 {
        return 0.001;
    }

    // Keep handles usable without reserving a large percentage of the line.
    // A fixed 5% minimum made separators feel blocked on lines with many syllables.
    let pixel_min = 3.0 / line_width.max(1.0);
    let total_budget_min = 0.35 / segment_count as f32;
    pixel_min
        .clamp(0.001, 0.02)
        .min(total_budget_min.max(0.001))
}

pub(crate) fn redistribute_group_to_total(ratios: &mut [f32], target_total: f32, min_ratio: f32) {
    if ratios.is_empty() {
        return;
    }

    let count = ratios.len() as f32;
    let min_total = min_ratio * count;
    let target_total = target_total.max(min_total);
    let target_free = (target_total - min_total).max(0.0);
    let free_sum: f32 = ratios
        .iter()
        .map(|ratio| (*ratio - min_ratio).max(0.0))
        .sum();

    if free_sum <= f32::EPSILON {
        let each = target_total / count;
        for ratio in ratios.iter_mut() {
            *ratio = each;
        }
        return;
    }

    for ratio in ratios.iter_mut() {
        let free = (*ratio - min_ratio).max(0.0);
        *ratio = min_ratio + free / free_sum * target_free;
    }
}

pub(crate) fn normalize_ratios_in_place(ratios: &mut [f32]) {
    let sum: f32 = ratios.iter().sum();
    if sum <= f32::EPSILON {
        return;
    }
    for ratio in ratios.iter_mut() {
        *ratio /= sum;
    }
}

pub(crate) fn syllable_mouse_release(state: &mut RythmoState) -> Option<EventResponse> {
    let drag = state.syllable_drag.take()?;
    let line_id = decode_sync_syllable_drag_line_id(drag.line_id).unwrap_or(drag.line_id);
    // A handle that was moved is placed: the next proportional drags stop
    // at it. Ctrl + click places it even without moving it, to set the limit
    // where the handle already stands.
    if drag.preserve_prefix || drag.ratios != drag.original_ratios {
        state.place_syllable_handle(line_id, drag.separator_index, drag.ratios.len());
    }
    let fitted = crate::workspaces::rythmo::detection_ui::finish_sync_syllable_drag(
        drag.line_id,
        &drag.ratios,
    );
    let mut actions = vec![UiAction::SetSyllableRatios {
        line_id,
        ratios: drag.ratios,
    }];
    if let Some((start_frame, duration_frames)) = fitted {
        actions.push(UiAction::ResizeLine {
            id: line_id,
            start_frame,
            duration_frames,
        });
    }
    Some(if actions.len() == 1 {
        EventResponse::Action(actions.remove(0))
    } else {
        EventResponse::Actions(actions)
    })
}
