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

    // Find which line was clicked
    let line = ctx
        .project
        .lines()
        .find(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y))?;
    if ctx.karaoke_preview && line.karaoke {
        return None;
    }
    if state.hovered_line != Some(line.id) {
        return None;
    }

    let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);

    let lang = ctx.project.syllable_language_code();
    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), lang, state)?;
    if ratios.len() <= 1 {
        return None;
    }

    // Find which separator is closest to click
    let mut sep_x = r.x;
    let hit_w = 7.0;
    let top_y = r.y + 1.0;
    if y < top_y - 6.0 || y > top_y + 14.0 {
        return None;
    }
    for (i, ratio) in ratios.iter().enumerate() {
        sep_x += ratio * r.width;
        if i < ratios.len() - 1 && (x - sep_x).abs() < hit_w {
            state.syllable_drag = Some(SyllableDrag {
                line_id: line.id,
                separator_index: i,
                ratios: ratios.clone(),
                drag_start_x: x,
                line_rect: r,
                preserve_prefix,
            });
            return Some(EventResponse::Consumed);
        }
    }
    None
}

pub(crate) fn syllable_mouse_move(state: &mut RythmoState, x: f32) -> Option<EventResponse> {
    let drag = state.syllable_drag.as_mut()?;

    let dx = x - drag.drag_start_x;
    let delta_ratio = dx / drag.line_rect.width;
    drag.drag_start_x = x;

    let i = drag.separator_index;
    let min_ratio = syllable_drag_min_ratio(drag.ratios.len(), drag.line_rect.width);
    if delta_ratio.abs() <= 0.0001 || i + 1 >= drag.ratios.len() {
        return Some(EventResponse::Consumed);
    }

    if drag.preserve_prefix {
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

    let left_end = i + 1;
    let right_start = i + 1;
    let left_total: f32 = drag.ratios[..left_end].iter().sum();
    let right_total: f32 = drag.ratios[right_start..].iter().sum();
    let left_min_total = min_ratio * left_end as f32;
    let right_min_total = min_ratio * (drag.ratios.len() - right_start) as f32;

    if delta_ratio > 0.0 {
        let applied = delta_ratio.min((right_total - right_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[..left_end],
                left_total + applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..],
                right_total - applied,
                min_ratio,
            );
        }
    } else {
        let applied = (-delta_ratio).min((left_total - left_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[..left_end],
                left_total - applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..],
                right_total + applied,
                min_ratio,
            );
        }
    }

    normalize_ratios_in_place(&mut drag.ratios);

    Some(EventResponse::Consumed)
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
    Some(EventResponse::Action(UiAction::SetSyllableRatios {
        line_id: drag.line_id,
        ratios: drag.ratios,
    }))
}
