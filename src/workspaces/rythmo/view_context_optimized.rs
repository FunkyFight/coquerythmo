//! Performance facade for the rythmo view.
//!
//! The established view keeps ownership of rendering and every interaction
//! except opening a line context menu. Right-click hit testing used to walk the
//! complete project and rebuild the complete editor layout twice for every
//! line. Large projects therefore turned one context-menu request into
//! quadratic work.

#![allow(hidden_glob_reexports)]

#[path = "view.rs"]
mod base;

pub use base::*;

use crate::project::Project;
use crate::ui::primitives::{EventResponse, Rect, UiEvent};

/// Dispatches context-menu interaction while replacing only the expensive
/// initial line hit test.
pub fn handle_context_menu_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    screen_w: f32,
    screen_h: f32,
    fps: f64,
    state: &mut RythmoState,
) -> EventResponse {
    let UiEvent::ContextMenu { x, y } = event else {
        return base::handle_context_menu_event(
            event,
            project,
            current_frame,
            zone,
            screen_w,
            screen_h,
            fps,
            state,
        );
    };

    if !zone.contains(*x, *y) {
        state.context_menu = None;
        return EventResponse::Ignored;
    }

    let pointer_frame = base::x_to_frame(*x, current_frame, zone, fps);
    let visible_span =
        (zone.width / base::ppf().max(0.001)).ceil().max(1.0) as i64;
    let badge_lookahead_end = pointer_frame
        .saturating_add(visible_span)
        .saturating_add(8);

    // One project-wide layout computation per right click, instead of one for
    // the body and another for the badge of every project line.
    let layout = base::EditorLayoutCtx::new_at_frame_with_fps(
        project,
        current_frame,
        fps,
        zone,
    );
    let reading_bar_offset_seconds = crate::config::reading_bar_offset_seconds();

    let line_id = project
        .lines()
        .filter(|line| {
            let body_can_contain_pointer =
                line.start_frame <= pointer_frame && line.end_frame() >= pointer_frame;
            let badge_can_reach_pointer = line.start_frame > pointer_frame
                && line.start_frame <= badge_lookahead_end;
            body_can_contain_pointer || badge_can_reach_pointer
        })
        .find_map(|line| {
            let line_rect = layout.line_rect_with_karaoke_width(
                line,
                current_frame,
                zone,
                false,
                None,
                reading_bar_offset_seconds,
                fps,
            );
            if line_rect.contains(*x, *y) {
                return Some(line.id);
            }

            let badge_rect = layout.badge_rect_for_name(
                line,
                &line.character_name,
                line_rect.x,
                zone,
                reading_bar_offset_seconds,
                fps,
            );
            badge_rect.contains(*x, *y).then_some(line.id)
        });

    let Some(line_id) = line_id else {
        state.context_menu = None;
        return EventResponse::Ignored;
    };

    let line_was_selected = match state.selected.as_ref() {
        Some(Selection::Line(id)) => *id == line_id,
        Some(Selection::Lines(ids)) => ids.contains(&line_id),
        Some(Selection::AllLines) => true,
        _ => false,
    };

    state.context_menu = Some(LineContextMenu {
        line_id,
        x: *x,
        y: *y,
        hover_main: true,
        hover_change_character: false,
        hover_actor_index: None,
        hover_action_index: None,
        actor_scroll: 0.0,
    });
    if !line_was_selected {
        state.selected = Some(Selection::Line(line_id));
    }
    state.dragging = None;

    EventResponse::Consumed
}
