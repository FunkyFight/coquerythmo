//! View and rendering adapter for the rythmo workspace.
//!
//! Rendering helpers keep the complete layout context in their signatures so
//! CPU, GPU and interactive paths use the same geometry decisions.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_filter)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_borrow)]

use crate::constants;
use crate::i18n::t;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_drawing::{strokes_bbox, DrawingStroke};
use crate::rythmo_layout;
use crate::rythmo_line::MarkerKind;
use crate::ui::context_menu;
use crate::ui::primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign,
};
use crate::ui::renderer::StretchedText;
use crate::ui::text_input::{self, TextInputMetrics};
use crate::ui::ToolMode;
use std::cell::{Ref, RefCell};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

const PLAYHEAD_WIDTH: f32 = 3.0;
const PLAYHEAD_COLOR: [f32; 4] = [1.0, 0.02, 0.05, 1.0];
const PLAYHEAD_GLOW: [f32; 4] = [1.0, 0.0, 0.03, 0.55];

const HANDLE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
const LINE_BORDER: [f32; 4] = [0.5, 0.5, 0.55, 0.3];
const LINE_BORDER_HOVER: [f32; 4] = [0.6, 0.6, 0.65, 0.5];
const LINE_RADIUS: f32 = 2.0;
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];
const KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS: f64 = 60.0;
const KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME: usize = 32;
const KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME: usize = 2;

fn character_badge_collision_style(
    line_id: u64,
    character_name: &str,
    badge_rect: &Rect,
    other_lines: &[(u64, Rect, &str)],
) -> (bool, f32) {
    let mut alpha = 1.0;
    for (other_id, other_rect, other_character_name) in other_lines {
        if *other_id == line_id || !rects_overlap(badge_rect, other_rect) {
            continue;
        }
        if *other_character_name == character_name {
            return (true, 1.0);
        }
        alpha = constants::CHARACTER_BADGE_COLLISION_OPACITY;
    }
    (false, alpha)
}

#[path = "state.rs"]
mod state;
pub use state::*;
#[path = "geometry.rs"]
mod geometry;
pub(crate) use geometry::*;
#[path = "controller.rs"]
mod controller;
pub(crate) use controller::RythmoCtx;
pub use controller::*;
#[path = "text_controller.rs"]
mod text_controller;
pub(crate) use text_controller::*;
#[path = "drag.rs"]
mod drag;
pub(crate) use drag::*;
#[path = "mouse.rs"]
mod mouse;
pub(crate) use mouse::*;
#[path = "mouse_buttons.rs"]
mod mouse_buttons;
pub(crate) use mouse_buttons::*;
#[path = "syllable.rs"]
mod syllable;
pub(crate) use syllable::*;
#[path = "press.rs"]
mod press;
pub(crate) use press::*;
#[path = "selection.rs"]
mod selection;
pub(crate) use selection::*;
#[path = "drawing.rs"]
mod drawing;
pub(crate) use drawing::*;
#[path = "keyboard.rs"]
mod keyboard;
pub(crate) use keyboard::*;
#[path = "keyboard_nav.rs"]
mod keyboard_nav;
pub(crate) use keyboard_nav::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rect_approx_eq(left: Rect, right: Rect) {
        assert!((left.x - right.x).abs() < 0.01, "x: {left:?} != {right:?}");
        assert!((left.y - right.y).abs() < 0.01, "y: {left:?} != {right:?}");
        assert!(
            (left.width - right.width).abs() < 0.01,
            "width: {left:?} != {right:?}"
        );
        assert!(
            (left.height - right.height).abs() < 0.01,
            "height: {left:?} != {right:?}"
        );
    }

    #[test]
    fn redistribute_group_preserves_proportions_above_minimum() {
        let mut ratios = vec![0.2, 0.3, 0.5];
        redistribute_group_to_total(&mut ratios, 0.5, 0.05);

        let sum: f32 = ratios.iter().sum();
        assert!((sum - 0.5).abs() < 0.0001);
        assert!(ratios.iter().all(|ratio| *ratio >= 0.05));
        assert!(ratios[2] > ratios[1]);
        assert!(ratios[1] > ratios[0]);
    }

    #[test]
    fn reducing_left_group_expands_right_group_proportionally() {
        let mut state = RythmoState::new();
        state.syllable_drag = Some(SyllableDrag {
            line_id: 1,
            separator_index: 1,
            ratios: vec![0.2, 0.3, 0.2, 0.3],
            drag_start_x: 100.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            preserve_prefix: false,
        });

        let _ = syllable_mouse_move(&mut state, 90.0);
        let ratios = &state.syllable_drag.as_ref().unwrap().ratios;

        assert!((ratios[..2].iter().sum::<f32>() - 0.4).abs() < 0.0001);
        assert!((ratios[2..].iter().sum::<f32>() - 0.6).abs() < 0.0001);
        assert!(ratios[2] > 0.2);
        assert!(ratios[3] > 0.3);
        assert!(ratios[3] > ratios[2]);
    }

    #[test]
    fn ctrl_syllable_drag_keeps_previous_boundaries_fixed() {
        let mut state = RythmoState::new();
        state.syllable_drag = Some(SyllableDrag {
            line_id: 1,
            separator_index: 1,
            ratios: vec![0.2, 0.3, 0.2, 0.3],
            drag_start_x: 100.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            preserve_prefix: true,
        });

        let _ = syllable_mouse_move(&mut state, 90.0);
        let ratios = &state.syllable_drag.as_ref().unwrap().ratios;

        assert!((ratios[0] - 0.2).abs() < 0.0001);
        assert!((ratios[1] - 0.2).abs() < 0.0001);
        assert!((ratios[2] - 0.3).abs() < 0.0001);
        assert!((ratios[3] - 0.3).abs() < 0.0001);
    }

    #[test]
    fn syllable_drag_does_not_block_at_old_five_percent_minimum() {
        let mut state = RythmoState::new();
        state.syllable_drag = Some(SyllableDrag {
            line_id: 1,
            separator_index: 1,
            ratios: vec![0.48, 0.48, 0.04],
            drag_start_x: 100.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            preserve_prefix: false,
        });

        let _ = syllable_mouse_move(&mut state, 102.0);
        let ratios = &state.syllable_drag.as_ref().unwrap().ratios;

        assert!(ratios[2] < 0.04, "right group should still compress");
        assert!(ratios[..2].iter().sum::<f32>() > 0.96);
    }

    #[test]
    fn untouched_line_keeps_a_single_text_run() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.25);
        project.get_line_mut(line_id).unwrap().text = "tambourine".to_string();
        let state = RythmoState::new();
        let line = project.get_line(line_id).unwrap();

        assert!(
            visible_syllable_segments(line, None, "en-us", false, &state).is_none(),
            "default timings must not split and independently stretch syllables"
        );
    }

    #[test]
    fn explicitly_saved_syllable_timings_enable_segmented_rendering() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.25);
        let line = project.get_line_mut(line_id).unwrap();
        line.text = "tambourine".to_string();
        line.syllable_ratios = vec![0.2, 0.3, 0.5];
        let state = RythmoState::new();
        let line = project.get_line(line_id).unwrap();

        let (_, ratios) = visible_syllable_segments(line, None, "en-us", false, &state)
            .expect("saved timings should render as syllable segments");
        assert_eq!(ratios, vec![0.2, 0.3, 0.5]);
    }

    #[test]
    fn adjacent_karaoke_preview_ignores_distant_lines() {
        let mut project = Project::new();
        let active_id = project.add_line(0, 24, 0.25);
        let near_id = project.add_line(24 * 20, 24, 0.25);
        let far_id = project.add_line(24 * 40, 24, 0.25);
        for id in [active_id, near_id, far_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let active = project.get_line(active_id).unwrap();
        assert_eq!(
            next_karaoke_line_after(&project, active, karaoke_adjacent_max_gap_frames(24.0))
                .map(|line| line.id),
            Some(near_id)
        );

        project.remove_line(near_id);
        let active = project.get_line(active_id).unwrap();
        assert!(
            next_karaoke_line_after(&project, active, karaoke_adjacent_max_gap_frames(24.0))
                .is_none()
        );
    }

    #[test]
    fn first_karaoke_line_scrolls_before_island_starts() {
        let mut project = Project::new();
        let first_id = project.add_line(24 * 10, 24, 0.25);
        let second_id = project.add_line(24 * 12, 24, 0.25);
        for id in [first_id, second_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();

        assert!(!karaoke_prestart_scroll_visible(
            &project,
            first,
            0.0,
            max_gap_frames,
            count_in_frames
        ));
        assert!(karaoke_prestart_scroll_visible(
            &project,
            first,
            (first.start_frame - count_in_frames) as f64,
            max_gap_frames,
            count_in_frames
        ));
        assert!(!karaoke_prestart_scroll_visible(
            &project,
            second,
            0.0,
            max_gap_frames,
            count_in_frames
        ));
        assert!(!karaoke_prestart_scroll_visible(
            &project,
            first,
            first.start_frame as f64,
            max_gap_frames,
            count_in_frames
        ));
    }

    #[test]
    fn normal_line_splits_karaoke_islands() {
        let mut project = Project::new();
        let previous_karaoke_id = project.add_line(0, 24, 0.25);
        let normal_id = project.add_line(24 * 2, 24, 0.25);
        let next_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(previous_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(next_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let previous_karaoke = project.get_line(previous_karaoke_id).unwrap();
        let next_karaoke = project.get_line(next_karaoke_id).unwrap();

        assert!(next_karaoke_line_after(&project, previous_karaoke, max_gap_frames).is_none());
        assert!(previous_karaoke_line_before(&project, next_karaoke, max_gap_frames).is_none());
        assert!(karaoke_prestart_scroll_visible(
            &project,
            next_karaoke,
            (next_karaoke.start_frame - count_in_frames) as f64,
            max_gap_frames,
            count_in_frames
        ));
    }

    #[test]
    fn karaoke_island_after_normal_line_continues_alternating_rows() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.25);
        let first_karaoke_id = project.add_line(24 * 2, 24, 0.25);
        let second_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(first_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(second_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first_karaoke = project.get_line(first_karaoke_id).unwrap();
        let second_karaoke = project.get_line(second_karaoke_id).unwrap();
        let index = KaraokeUiIndex::new(&project, max_gap_frames);

        assert_eq!(
            karaoke_stack_row(&project, first_karaoke, max_gap_frames),
            1
        );
        assert_eq!(
            karaoke_stack_row(&project, second_karaoke, max_gap_frames),
            0
        );
        assert_eq!(index.stack_row(first_karaoke), 1);
        assert_eq!(index.stack_row(second_karaoke), 0);
    }

    #[test]
    fn karaoke_island_lines_alternate_stack_rows() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        let normal_id = project.add_line(24 * 6, 24, 0.25);
        let next_island_id = project.add_line(24 * 8, 24, 0.25);
        for id in [first_id, second_id, third_id, next_island_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }
        project.get_line_mut(normal_id).unwrap().karaoke = false;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();
        let next_island = project.get_line(next_island_id).unwrap();

        assert_eq!(karaoke_stack_row(&project, first, max_gap_frames), 0);
        assert_eq!(karaoke_stack_row(&project, second, max_gap_frames), 1);
        assert_eq!(karaoke_stack_row(&project, third, max_gap_frames), 0);
        assert_eq!(karaoke_stack_row(&project, next_island, max_gap_frames), 1);
    }

    #[test]
    fn next_karaoke_line_stays_visible_inside_started_island() {
        let mut project = Project::new();
        let first_id = project.add_line(24 * 10, 24, 0.25);
        let second_id = project.add_line(24 * 14, 24, 0.25);
        for id in [first_id, second_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();

        assert!(!karaoke_upcoming_stack_visible(
            &project,
            second,
            (first.start_frame - 1) as f64,
            max_gap_frames
        ));
        assert!(karaoke_upcoming_stack_visible(
            &project,
            second,
            first.start_frame as f64,
            max_gap_frames
        ));
        assert!(karaoke_upcoming_stack_visible(
            &project,
            second,
            (first.end_frame() + 1) as f64,
            max_gap_frames
        ));
        assert!(!karaoke_upcoming_stack_visible(
            &project,
            second,
            second.start_frame as f64,
            max_gap_frames
        ));
    }

    #[test]
    fn karaoke_stack_rows_stay_inside_track_body() {
        let row_height = 40.0;
        let base = Rect {
            x: 0.0,
            y: 10.0,
            width: 200.0,
            height: rythmo_layout::karaoke_track_body_height(row_height, 1.0),
        };
        let top = karaoke_stack_rect(base, 0, 1.0);
        let bottom = karaoke_stack_rect(base, 1, 1.0);

        assert!(top.y >= base.y);
        assert!(bottom.y > top.y);
        assert!(top.y + top.height <= bottom.y);
        assert!(bottom.y + bottom.height <= base.y + base.height);
        assert!((top.height - row_height).abs() < f32::EPSILON);
        assert!((bottom.height - row_height).abs() < f32::EPSILON);
    }

    #[test]
    fn editor_only_karaoke_tracks_get_double_body_height() {
        crate::config::init();
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };

        let normal_body_h = editor_normal_body_height_for_karaoke_tracks(1, &zone);
        let normal_rect = line_rect(&project, project.get_line(normal_id).unwrap(), 0.0, &zone);
        let karaoke_body = editor_track_body_rect_at_frame(&project, 0.5, 24.0, &zone);
        let karaoke_rect = karaoke_preview_line_rect(
            &project,
            project.get_line(karaoke_id).unwrap(),
            24.0,
            &zone,
            karaoke_adjacent_max_gap_frames(24.0),
        );

        assert!((normal_rect.height - normal_body_h).abs() < f32::EPSILON);
        let active_normal_body_h = normal_body_h;
        assert!(
            (karaoke_body.height
                - rythmo_layout::karaoke_track_body_height(active_normal_body_h, 1.0))
            .abs()
                < 0.01
        );
        assert!((karaoke_rect.height - active_normal_body_h).abs() < 0.01);
    }

    #[test]
    fn first_karaoke_line_enters_playback_mode_during_count_in() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(48, 24, 0.25);
        project.get_line_mut(line_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };
        let line = project.get_line(line_id).unwrap();

        let count_in = EditorLayoutCtx::new_at_frame(&project, 12.0, &zone);
        let active = EditorLayoutCtx::new_at_frame(&project, 48.0, &zone);
        let after = EditorLayoutCtx::new_at_frame(&project, 72.1, &zone);

        assert!(karaoke_line_uses_playback_mode(&count_in, line, true));
        assert!(karaoke_line_uses_playback_mode(&active, line, true));
        assert!(!karaoke_line_uses_playback_mode(&after, line, true));
    }

    #[test]
    fn karaoke_playback_mode_survives_only_gaps_before_karaoke_lines() {
        crate::config::init();
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let next_id = project.add_line(96, 24, 0.25);
        project.get_line_mut(first_id).unwrap().karaoke = true;
        project.get_line_mut(next_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };

        let karaoke_gap = EditorLayoutCtx::new_at_frame(&project, 48.0, &zone);
        assert!(karaoke_gap.track_for_y_slot(0.25).has_karaoke);

        project.get_line_mut(next_id).unwrap().karaoke = false;
        let normal_gap = EditorLayoutCtx::new_at_frame(&project, 48.0, &zone);
        assert!(!normal_gap.track_for_y_slot(0.25).has_karaoke);
    }

    #[test]
    fn karaoke_mode_changes_do_not_move_other_tracks() {
        crate::config::init();
        let mut project = Project::new();
        let karaoke_id = project.add_line(240, 24, 0.0);
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        project.add_line(0, 24, 0.5);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };

        let before = EditorLayoutCtx::new_at_frame(&project, 0.0, &zone);
        let count_in = EditorLayoutCtx::new_at_frame(&project, 204.0, &zone);
        let active = EditorLayoutCtx::new_at_frame(&project, 240.0, &zone);
        let after = EditorLayoutCtx::new_at_frame(&project, 264.1, &zone);
        let stable_top = before.track_for_y_slot(0.5).top;

        assert_eq!(count_in.track_for_y_slot(0.5).top, stable_top);
        assert_eq!(active.track_for_y_slot(0.5).top, stable_top);
        assert_eq!(after.track_for_y_slot(0.5).top, stable_top);
    }

    #[test]
    fn karaoke_and_scrolling_text_use_distinct_cache_entries() {
        let line_id = 42;

        assert_ne!(karaoke_text_cache_id(line_id), line_id);
        assert_ne!(
            karaoke_text_cache_id(line_id),
            syllable_segment_cache_id(line_id, 0)
        );
    }

    #[test]
    fn karaoke_count_in_rect_is_centered_immediately() {
        crate::config::init();
        let mut project = Project::new();
        let previous_id = project.add_line(30, 24, 0.25);
        let line_id = project.add_line(48, 24, 0.25);
        project.get_line_mut(previous_id).unwrap().karaoke = true;
        {
            let line = project.get_line_mut(line_id).unwrap();
            line.karaoke = true;
            line.text = "Directly centered".to_string();
        }
        let zone = Rect {
            x: 20.0,
            y: 10.0,
            width: 800.0,
            height: 300.0,
        };
        let layout_ctx = EditorLayoutCtx::new(&project, &zone);
        let line = project.get_line(line_id).unwrap();
        let count_in_frames = karaoke_count_in_frames(24.0);
        let index = KaraokeUiIndex::new(&project, karaoke_adjacent_max_gap_frames(24.0));
        assert!(karaoke_count_in_visible(line, 24.0, count_in_frames));
        assert!(!index.prestart_scroll_visible(line, 24.0, count_in_frames));
        assert!(!index.upcoming_stack_visible(line, 24.0));
        let rect = karaoke_preview_line_rect_with_state(
            &layout_ctx,
            line,
            24.0,
            &zone,
            true,
            false,
            0,
            None,
        );
        let expected_width = karaoke_ui_text_width(&line.text);
        let expected_x = zone.x + (zone.width - expected_width) / 2.0;

        assert!((rect.x - expected_x).abs() < 0.01);
        assert!((rect.width - expected_width).abs() < 0.01);
    }

    #[test]
    fn character_badge_collision_uses_the_other_line_body() {
        let badge = Rect {
            x: 10.0,
            y: 10.0,
            width: 80.0,
            height: 20.0,
        };
        let colliding_line = Rect {
            x: 50.0,
            y: 5.0,
            width: 100.0,
            height: 30.0,
        };

        assert_eq!(
            character_badge_collision_style(1, "Alice", &badge, &[(2, colliding_line, "Bob")]),
            (false, constants::CHARACTER_BADGE_COLLISION_OPACITY)
        );
        assert_eq!(
            character_badge_collision_style(1, "Alice", &badge, &[(2, colliding_line, "Alice")]),
            (true, 1.0)
        );
    }

    #[test]
    fn same_character_collision_takes_priority_over_transparency() {
        let badge = Rect {
            x: 10.0,
            y: 10.0,
            width: 80.0,
            height: 20.0,
        };
        let colliding_line = Rect {
            x: 50.0,
            y: 5.0,
            width: 100.0,
            height: 30.0,
        };
        let other_lines = [(2, colliding_line, "Bob"), (3, colliding_line, "Alice")];

        assert_eq!(
            character_badge_collision_style(1, "Alice", &badge, &other_lines),
            (true, 1.0)
        );
    }

    #[test]
    fn y_to_slot_uses_variable_track_offsets() {
        let mut project = Project::new();
        let karaoke_id = project.add_line(0, 24, 0.25);
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 10.0,
            width: 800.0,
            height: 300.0,
        };
        let layouts = editor_track_layouts(&project, &zone);
        let karaoke_track = rythmo_layout::track_for_index(&layouts, 1).unwrap();
        let next_track = rythmo_layout::track_for_index(&layouts, 2).unwrap();

        let karaoke_y =
            zone.y + constants::RULER_HEIGHT + karaoke_track.top + karaoke_track.total_h - 1.0;
        let next_y = zone.y + constants::RULER_HEIGHT + next_track.top + 1.0;

        assert_eq!(y_to_slot(&project, karaoke_y, &zone), 0.25);
        assert_eq!(y_to_slot(&project, next_y, &zone), 0.5);
    }

    #[test]
    fn karaoke_character_label_only_on_first_or_character_change() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        for id in [first_id, second_id, third_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        project.get_line_mut(third_id).unwrap().character_name = "Bob".to_string();

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();

        assert!(karaoke_character_label_visible(
            &project,
            first,
            max_gap_frames
        ));
        assert!(!karaoke_character_label_visible(
            &project,
            second,
            max_gap_frames
        ));
        assert!(karaoke_character_label_visible(
            &project,
            third,
            max_gap_frames
        ));
    }

    #[test]
    fn karaoke_ui_index_uses_chronological_order_not_insertion_order() {
        let mut project = Project::new();
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let first_id = project.add_line(0, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        for id in [first_id, second_id, third_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        project.get_line_mut(third_id).unwrap().character_name = "Bob".to_string();

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();

        assert_eq!(index.stack_row(first), 0);
        assert_eq!(index.stack_row(second), 1);
        assert_eq!(index.stack_row(third), 0);
        assert_eq!(index.previous_adjacent_karaoke_id(second), Some(first_id));
        assert!(index.character_label_visible(first));
        assert!(!index.character_label_visible(second));
        assert!(index.character_label_visible(third));
    }

    #[test]
    fn karaoke_ui_index_normal_line_cuts_island() {
        let mut project = Project::new();
        let previous_karaoke_id = project.add_line(0, 24, 0.25);
        let normal_id = project.add_line(24 * 2, 24, 0.25);
        let next_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(previous_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(next_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let next_karaoke = project.get_line(next_karaoke_id).unwrap();

        assert_eq!(index.previous_adjacent_karaoke_id(next_karaoke), None);
        assert_eq!(index.stack_row(next_karaoke), 1);
        assert!(!index.prestart_scroll_visible(next_karaoke, 0.0, count_in_frames));
        assert!(index.prestart_scroll_visible(
            next_karaoke,
            (next_karaoke.start_frame - count_in_frames) as f64,
            count_in_frames
        ));
    }

    #[test]
    fn karaoke_ui_index_uses_quantized_track_for_drifted_slots() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let drifted_id = project.add_line(24 * 2, 24, 0.26);
        for id in [first_id, drifted_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let first = project.get_line(first_id).unwrap();
        let drifted = project.get_line(drifted_id).unwrap();

        assert_eq!(index.stack_row(first), 0);
        assert_eq!(index.stack_row(drifted), 1);
        assert_eq!(index.previous_adjacent_karaoke_id(drifted), Some(first_id));
    }

    #[test]
    fn editor_layout_ctx_matches_wrapper_rects() {
        crate::config::init();
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 12.0,
            y: 8.0,
            width: 800.0,
            height: 300.0,
        };
        let ctx = EditorLayoutCtx::new_at_frame(&project, 0.0, &zone);

        assert!(
            (ctx.normal_body_h - editor_normal_body_height_for_karaoke_tracks(1, &zone)).abs()
                < 0.01
        );
        assert_rect_approx_eq(
            ctx.track_body_rect(0.5, &zone),
            editor_track_body_rect_at_frame(&project, 0.5, 0.0, &zone),
        );
        assert_rect_approx_eq(
            ctx.line_rect_with_karaoke_width(
                project.get_line(normal_id).unwrap(),
                0.0,
                &zone,
                false,
                None,
            ),
            line_rect(&project, project.get_line(normal_id).unwrap(), 0.0, &zone),
        );

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let karaoke = project.get_line(karaoke_id).unwrap();
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let karaoke_ctx = EditorLayoutCtx::new_at_frame(&project, 24.0, &zone);
        assert_rect_approx_eq(
            karaoke_preview_line_rect_with_state(
                &karaoke_ctx,
                karaoke,
                24.0,
                &zone,
                false,
                index.upcoming_stack_visible(karaoke, 24.0),
                index.stack_row(karaoke),
                None,
            ),
            karaoke_preview_line_rect(&project, karaoke, 24.0, &zone, max_gap_frames),
        );
    }

    #[test]
    fn new_karaoke_line_render_width_uses_measured_width() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.25);
        let line = project.get_line_mut(line_id).unwrap();
        line.karaoke = true;
        line.text = "Karaoke width check".to_string();

        let state = RythmoState::new();
        let line = project.get_line(line_id).unwrap();
        assert_eq!(
            state.karaoke_ui_text_width_for_render(line),
            karaoke_ui_text_width(&line.text)
        );
    }

    #[test]
    fn karaoke_count_in_dot_moves_from_left_onto_text() {
        let line_rect = Rect {
            x: 300.0,
            y: 80.0,
            width: 120.0,
            height: 32.0,
        };
        let start = karaoke_count_in_dot_rect(&line_rect, 0.0, 1.0);
        let mid = karaoke_count_in_dot_rect(&line_rect, 0.5, 1.0);
        let end = karaoke_count_in_dot_rect(&line_rect, 1.0, 1.0);

        assert!(start.x + start.width <= line_rect.x);
        assert!(mid.x > start.x);
        assert!(mid.x < line_rect.x);
        assert!((end.x - line_rect.x).abs() < 0.01);
    }

    #[test]
    fn fractional_frame_geometry_shifts_by_subframe_amounts() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };

        let whole_frame_x = frame_to_x(100, 100.0, &zone);
        let half_frame_x = frame_to_x(100, 100.5, &zone);

        assert!((half_frame_x - (whole_frame_x - ppf() * 0.5)).abs() < 0.01);
        assert_eq!(x_to_frame(half_frame_x, 100.5, &zone), 100);
        assert_eq!(x_to_frame(frame_to_x(101, 100.5, &zone), 100.5, &zone), 101);
    }

    #[test]
    fn scrolling_does_not_change_line_texture_width() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(1_000, 48, 0.0);
        let line = project.get_line(line_id).unwrap();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let expected_width = line.duration_frames as f32 * ppf();

        for step in 0..=240 {
            let current_frame = 1_000.0 + step as f64 / 240.0;
            let (_, editor_width) = line_visual_x_width(line, current_frame, &zone, false);
            let (_, playback_width) = line_visual_x_width(line, current_frame, &zone, true);

            assert_eq!(editor_width.to_bits(), expected_width.to_bits());
            assert_eq!(playback_width.to_bits(), expected_width.to_bits());
            assert_eq!(editor_width.ceil() as u32, expected_width.ceil() as u32);
            assert_eq!(playback_width.ceil() as u32, expected_width.ceil() as u32);
        }
    }

    #[test]
    fn pointer_hover_finds_line_through_render_index() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(40, 20, 0.0);
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let current_frame = 50.0;
        let line_rect = EditorLayoutCtx::new(&project, &zone).line_rect_with_karaoke_width(
            project.get_line(line_id).unwrap(),
            current_frame,
            &zone,
            false,
            None,
        );
        let mut state = RythmoState::new();

        let response = handle_rythmo_event(
            &UiEvent::MouseMove {
                x: line_rect.x + line_rect.width / 2.0,
                y: line_rect.y + line_rect.height / 2.0,
            },
            &zone,
            &project,
            &render_index,
            current_frame,
            false,
            24.0,
            &mut state,
            ToolMode::Select,
            [1.0, 1.0, 1.0, 1.0],
            0.012,
            false,
            RythmoInteractionMode::Editable,
        );

        assert_eq!(response, EventResponse::Consumed);
        assert_eq!(state.hovered_line, Some(line_id));
        assert_eq!(state.hovered_track, Some(0));
    }

    #[test]
    fn read_only_controller_never_arms_or_emits_authoring_actions() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(40, 20, 0.0);
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let current_frame = 50.0;
        let line_rect = EditorLayoutCtx::new(&project, &zone).line_rect_with_karaoke_width(
            project.get_line(line_id).unwrap(),
            current_frame,
            &zone,
            false,
            None,
        );
        let mut state = RythmoState::new();

        let press = handle_rythmo_event(
            &UiEvent::MousePress {
                x: line_rect.x + line_rect.width / 2.0,
                y: line_rect.y + line_rect.height / 2.0,
            },
            &zone,
            &project,
            &render_index,
            current_frame,
            false,
            24.0,
            &mut state,
            ToolMode::Select,
            [1.0, 1.0, 1.0, 1.0],
            0.012,
            false,
            RythmoInteractionMode::ReadOnly,
        );
        assert_eq!(press, EventResponse::Consumed);
        assert_eq!(state.selected, Some(Selection::Line(line_id)));
        assert!(state.dragging.is_none());

        let delete = handle_rythmo_event(
            &UiEvent::Delete,
            &zone,
            &project,
            &render_index,
            current_frame,
            false,
            24.0,
            &mut state,
            ToolMode::Select,
            [1.0, 1.0, 1.0, 1.0],
            0.012,
            false,
            RythmoInteractionMode::ReadOnly,
        );
        assert_eq!(delete, EventResponse::Consumed);
        assert!(state.dragging.is_none());
    }

    #[test]
    fn waveform_offset_keeps_visible_audio_peaks_rendered() {
        crate::config::init();
        let project = Project::new();
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);
        let state = RythmoState::new();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let current_frame = 10_000;
        let waveform_offset_frames = 9_000;
        let visible_audio_frame = current_frame - waveform_offset_frames;
        let mut waveform = vec![0.0; (visible_audio_frame as usize + 1) * 4];
        waveform[visible_audio_frame as usize * 4] = 1.0;
        let scene = crate::rendering::rythmo::scene::RythmoScene::build(
            &project,
            &render_index,
            crate::rendering::rythmo::scene::SceneOptions {
                frame_window: crate::rendering::rythmo::scene::FrameWindow {
                    first: current_frame - 1_000,
                    last: current_frame + 1_000,
                },
                current_frame: current_frame as f64,
                ..crate::rendering::rythmo::scene::SceneOptions::default()
            },
        );

        let quads = render_rythmo_base(
            &zone,
            &project,
            current_frame as f64,
            &waveform,
            waveform_offset_frames,
            true,
            false,
            24.0,
            &state,
            &scene,
        );

        assert!(quads.iter().any(|quad| {
            quad.color == [0.30, 0.90, 0.45, 0.85]
                && quad.rect[0] >= zone.x
                && quad.rect[0] <= zone.x + zone.width
                && (quad.rect[3] - constants::RULER_HEIGHT).abs() < 0.01
        }));
    }
}

fn push_playhead_segments(
    quads: &mut Vec<QuadInstance>,
    x: f32,
    width: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    shadow_color: [f32; 4],
    shadow_blur: f32,
    skip_ranges: &[(f32, f32)],
) {
    let mut ranges: Vec<(f32, f32)> = skip_ranges
        .iter()
        .map(|(start, end)| (start.max(y), end.min(y + height)))
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut cursor_y = y;
    for (skip_start, skip_end) in ranges {
        if skip_start > cursor_y {
            quads.push(QuadInstance {
                rect: [x, cursor_y, width, skip_start - cursor_y],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0, 0.0],
                shadow_color,
                shadow_blur,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        cursor_y = cursor_y.max(skip_end);
    }

    let end_y = y + height;
    if cursor_y < end_y {
        quads.push(QuadInstance {
            rect: [x, cursor_y, width, end_y - cursor_y],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0, 0.0],
            shadow_color,
            shadow_blur,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
}

fn active_karaoke_skip_ranges(
    project: &Project,
    scene: &crate::rendering::rythmo::scene::RythmoScene,
    zone: &Rect,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
) -> Vec<(f32, f32)> {
    if !karaoke_preview {
        return Vec::new();
    }

    let layout_ctx = state.get_or_create_layout_ctx(project, scene.current_frame, fps, zone);
    scene
        .lines
        .iter()
        .filter(|scene_line| scene_line.karaoke_active)
        .map(|line| {
            let body_rect = layout_ctx.track_body_rect(line.line.y_slot, zone);
            let rect = karaoke_stack_rect(
                Rect {
                    x: body_rect.x,
                    y: body_rect.y,
                    width: body_rect.width,
                    height: body_rect.height,
                },
                line.karaoke_stack_row,
                1.0,
            );
            (rect.y, rect.y + rect.height)
        })
        .collect()
}

pub fn render_rythmo_base(
    zone: &Rect,
    project: &Project,
    current_frame: f64,
    waveform: &[f32],
    waveform_offset_frames: i64,
    waveform_is_instrumental: bool,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    scene: &crate::rendering::rythmo::scene::RythmoScene,
) -> Vec<QuadInstance> {
    let mut quads = Vec::new();

    // Waveform (rendered first, behind playhead)
    // waveform has WAVEFORM_SUBDIVISIONS (4) entries per video frame
    let (wave_top, wave_bottom) = if waveform_is_instrumental {
        ([0.30, 0.90, 0.45, 0.85], [0.10, 0.62, 0.25, 0.4])
    } else {
        ([0.4, 0.65, 1.0, 0.85], [0.2, 0.45, 0.85, 0.4])
    };
    if !waveform.is_empty() {
        let subs = 4usize; // must match WAVEFORM_SUBDIVISIONS in video.rs
        let ruler_h = constants::RULER_HEIGHT;
        let sub_ppf = ppf() / subs as f32; // pixels per sub-frame
        let bar_w = sub_ppf.max(1.0);
        let visible_frames = (zone.width / ppf()) as i64 + 4;
        let half_visible_frames = visible_frames as f64 / 2.0;
        let first_frame = f64_floor_to_i64(current_frame - half_visible_frames);
        let last_frame = f64_ceil_to_i64(current_frame + half_visible_frames);
        let first_wave_frame = first_frame.saturating_sub(waveform_offset_frames);
        let last_wave_frame = last_frame.saturating_sub(waveform_offset_frames);
        let first_sub = first_wave_frame
            .saturating_mul(subs as i64)
            .clamp(0, waveform.len() as i64);
        let last_sub = last_wave_frame
            .saturating_add(1)
            .saturating_mul(subs as i64)
            .clamp(0, waveform.len() as i64);

        for si in first_sub..last_sub {
            let amp = waveform[si as usize].min(1.0);
            let bar_h = amp * ruler_h;
            if bar_h < 0.3 {
                continue;
            }

            // Position: which video frame + sub offset
            let frame = (si / subs as i64).saturating_add(waveform_offset_frames);
            let sub_offset = (si % subs as i64) as f32;
            let x = frame_to_x(frame, current_frame, zone) + sub_offset * sub_ppf;
            if x < zone.x || x > zone.x + zone.width {
                continue;
            }

            quads.push(QuadInstance {
                rect: [x, zone.y + ruler_h - bar_h, bar_w, bar_h],
                color: wave_top,
                color_bottom: wave_bottom,
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
    } else if waveform_is_instrumental {
        let ruler_h = constants::RULER_HEIGHT;
        let bar_w = 3.0;
        let step = 8.0;
        let mut x = zone.x;
        let mut i = 0.0_f32;
        while x < zone.x + zone.width {
            let amp = (0.25 + (i * 0.55).sin().abs() * 0.55).clamp(0.0, 1.0);
            let bar_h = amp * ruler_h;
            quads.push(QuadInstance {
                rect: [x, zone.y + ruler_h - bar_h, bar_w, bar_h],
                color: wave_top,
                color_bottom: wave_bottom,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            x += step;
            i += 1.0;
        }
    }

    if state.audio_offset_mode {
        quads.push(QuadInstance {
            rect: [zone.x, zone.y, zone.width, constants::RULER_HEIGHT],
            color: [1.0, 0.55, 0.10, 0.10],
            color_bottom: [1.0, 0.55, 0.10, 0.10],
            border_color: [1.0, 0.62, 0.18, 0.9],
            border_width: 1.5,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    // Ticks removed from UI (kept in CPU/GPU export renderers)

    let playhead_x = zone.x + (zone.width - PLAYHEAD_WIDTH) / 2.0;
    let skip_ranges = active_karaoke_skip_ranges(project, scene, zone, karaoke_preview, fps, state);
    push_playhead_segments(
        &mut quads,
        playhead_x,
        PLAYHEAD_WIDTH,
        zone.y,
        zone.height,
        PLAYHEAD_COLOR,
        PLAYHEAD_GLOW,
        7.0,
        &skip_ranges,
    );

    quads
}

/// Returns optional (line_id, cursor_pos, text_x, text_w, rect_y, rect_h) for cursor rendering.
const BADGE_HEIGHT: f32 = 13.0;
const BADGE_PADDING_H: f32 = 8.0;
const BADGE_GAP: f32 = 2.0;
const BADGE_MIN_W: f32 = 24.0;
const BADGE_FONT_SIZE: f32 = 13.0;

// Character badge overlaps the upper part of the line body.
const BADGE_OVERLAP_HEIGHT_RATIO: f32 = constants::BADGE_OVERLAP_HEIGHT_RATIO;
const ACTOR_ICON_SIZE: f32 = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE;
const ACTOR_ICON_GAP: f32 = 3.0;

fn slot_header_height() -> f32 {
    BADGE_HEIGHT.max(ACTOR_ICON_SIZE)
}

fn line_color_tint(line: &crate::rythmo_line::RythmoLine) -> [f32; 4] {
    [
        line.character_color[0].clamp(0.0, 1.0),
        line.character_color[1].clamp(0.0, 1.0),
        line.character_color[2].clamp(0.0, 1.0),
        1.0,
    ]
}

fn line_color_label(line: &crate::rythmo_line::RythmoLine) -> [u8; 3] {
    [
        (line.character_color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (line.character_color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (line.character_color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

#[derive(Clone, Copy)]
struct KaraokeProgressRenderInfo {
    visual_progress: f32,
    local_progress: f32,
}

fn karaoke_progress_render_info(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    lang: &str,
) -> Option<KaraokeProgressRenderInfo> {
    let progress = line.karaoke_progress(current_frame)?;
    let ratios = crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
    let local_progress = crate::syllable::active_syllable_local_progress(&ratios, progress)
        .unwrap_or(progress)
        .clamp(0.0, 1.0);
    let visual_progress = crate::syllable::visual_progress_from_timing(
        &line.text,
        &line.syllable_ratios,
        lang,
        progress,
    );

    Some(KaraokeProgressRenderInfo {
        visual_progress,
        local_progress,
    })
}

fn push_plain_rythmo_text(
    stretched: &mut Vec<StretchedText>,
    line_id: u64,
    text: String,
    dest_rect: Rect,
    tint: [f32; 4],
) {
    let mut stretched_text = StretchedText::new(line_id, text, dest_rect);
    stretched_text.tint = tint;
    stretched.push(stretched_text);
}

fn push_read_word_rythmo_text(
    stretched: &mut Vec<StretchedText>,
    line_id: u64,
    text: String,
    dest_rect: Rect,
    segment_start: usize,
    highlight_end: Option<usize>,
    base_tint: [f32; 4],
) {
    let char_count = text.chars().count();
    let Some(highlight_end) = highlight_end else {
        push_plain_rythmo_text(stretched, line_id, text, dest_rect, base_tint);
        return;
    };
    if char_count == 0 || highlight_end <= segment_start {
        push_plain_rythmo_text(stretched, line_id, text, dest_rect, base_tint);
        return;
    }
    if highlight_end >= segment_start + char_count {
        let mut highlighted = StretchedText::new(line_id, text, dest_rect);
        highlighted.tint = [1.0, 0.82, 0.08, 1.0];
        stretched.push(highlighted);
        return;
    }
    push_plain_rythmo_text(stretched, line_id, text.clone(), dest_rect, base_tint);
    let ratio = (highlight_end - segment_start) as f32 / char_count as f32;
    let mut overlay = StretchedText::new(line_id, text, dest_rect);
    overlay.draw_rect.width *= ratio;
    overlay.uv_rect[2] = ratio;
    overlay.tint = [1.0, 0.82, 0.08, 1.0];
    stretched.push(overlay);
}

fn push_natural_karaoke_text(
    stretched: &mut Vec<StretchedText>,
    line_id: u64,
    text: String,
    dest_rect: Rect,
    tint: [f32; 4],
) {
    stretched.push(StretchedText::natural(
        line_id,
        text,
        dest_rect,
        constants::KARAOKE_TEXT_FONT_SCALE,
        tint,
    ));
}

fn syllable_segment_cache_id(line_id: u64, segment_index: usize) -> u64 {
    (1_u64 << 63) ^ line_id.wrapping_mul(1_000_003) ^ (segment_index as u64).wrapping_add(1)
}

fn karaoke_text_cache_id(line_id: u64) -> u64 {
    (1_u64 << 62) ^ line_id
}

#[cfg(test)]
fn same_karaoke_track(
    a: &crate::rythmo_line::RythmoLine,
    b: &crate::rythmo_line::RythmoLine,
) -> bool {
    rythmo_layout::track_index_for_y_slot(a.y_slot)
        == rythmo_layout::track_index_for_y_slot(b.y_slot)
}

fn karaoke_adjacent_max_gap_frames(fps: f64) -> i64 {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    };
    (constants::KARAOKE_ADJACENT_MAX_GAP_SECONDS * fps).round() as i64
}

fn karaoke_count_in_frames(fps: f64) -> i64 {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    };
    (constants::KARAOKE_COUNT_IN_SECONDS * fps).round().max(1.0) as i64
}

fn karaoke_count_in_progress(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    count_in_frames: i64,
) -> Option<f32> {
    if !line.karaoke || current_frame >= line.start_frame as f64 || count_in_frames <= 0 {
        return None;
    }

    let count_in_start = (line.start_frame - count_in_frames) as f64;
    if current_frame < count_in_start {
        return None;
    }

    Some(((current_frame - count_in_start) as f32 / count_in_frames as f32).clamp(0.0, 1.0))
}

fn karaoke_count_in_visible(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    count_in_frames: i64,
) -> bool {
    karaoke_count_in_progress(line, current_frame, count_in_frames).is_some()
}

fn karaoke_previous_gap_frames(
    previous: &crate::rythmo_line::RythmoLine,
    line: &crate::rythmo_line::RythmoLine,
) -> i64 {
    (line.start_frame - previous.end_frame()).max(0)
}

#[cfg(test)]
fn karaoke_next_gap_frames(
    line: &crate::rythmo_line::RythmoLine,
    next: &crate::rythmo_line::RythmoLine,
) -> i64 {
    (next.start_frame - line.end_frame()).max(0)
}

const KARAOKE_UI_SIGNATURE_OFFSET: u64 = 0xcbf29ce484222325;
const KARAOKE_UI_SIGNATURE_PRIME: u64 = 0x100000001b3;

fn karaoke_signature_mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(KARAOKE_UI_SIGNATURE_PRIME);
}

#[cfg(test)]
fn karaoke_signature_mix_str(hash: &mut u64, value: &str) {
    karaoke_signature_mix(hash, value.len() as u64);
    for &byte in value.as_bytes() {
        karaoke_signature_mix(hash, byte as u64);
    }
}

fn karaoke_ui_index_revision_signature(project: &Project, max_gap_frames: i64) -> u64 {
    let mut hash = KARAOKE_UI_SIGNATURE_OFFSET;
    karaoke_signature_mix(&mut hash, project.revision());
    karaoke_signature_mix(&mut hash, max_gap_frames as u64);
    hash
}

#[cfg(test)]
fn karaoke_ui_index_signature(project: &Project, max_gap_frames: i64) -> u64 {
    let mut hash = KARAOKE_UI_SIGNATURE_OFFSET;
    karaoke_signature_mix(&mut hash, project.line_count() as u64);
    karaoke_signature_mix(&mut hash, max_gap_frames as u64);
    for line in project.lines() {
        karaoke_signature_mix(&mut hash, line.id);
        karaoke_signature_mix(&mut hash, line.start_frame as u64);
        karaoke_signature_mix(&mut hash, line.duration_frames as u64);
        karaoke_signature_mix(
            &mut hash,
            rythmo_layout::track_index_for_y_slot(line.y_slot) as u64,
        );
        karaoke_signature_mix(&mut hash, line.karaoke as u64);
        if line.karaoke {
            karaoke_signature_mix_str(&mut hash, &line.character_name);
        }
    }
    hash
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct KaraokeLineUiState {
    previous_adjacent_karaoke_id: Option<u64>,
    previous_adjacent_start_frame: Option<i64>,
    stack_row: usize,
    label_visible: bool,
    island_index: usize,
}

struct KaraokeUiIndex {
    signature: u64,
    by_line_id: HashMap<u64, KaraokeLineUiState>,
    karaoke_timeline: Vec<(i64, u64)>,
}

impl KaraokeUiIndex {
    #[cfg(test)]
    fn new(project: &Project, max_gap_frames: i64) -> Self {
        Self::new_with_signature(
            project,
            max_gap_frames,
            karaoke_ui_index_signature(project, max_gap_frames),
        )
    }

    fn new_with_signature(project: &Project, max_gap_frames: i64, signature: u64) -> Self {
        let track_count = rythmo_layout::track_count();
        let mut karaoke_timeline = Vec::new();
        let mut lines_by_track: Vec<Vec<&crate::rythmo_line::RythmoLine>> =
            (0..track_count).map(|_| Vec::new()).collect();
        for line in project.lines() {
            let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
            if line.karaoke {
                karaoke_timeline.push((line.start_frame, line.id));
            }
            if let Some(track_lines) = lines_by_track.get_mut(track_index) {
                track_lines.push(line);
            }
        }
        karaoke_timeline.sort_unstable_by_key(|&(start_frame, line_id)| (start_frame, line_id));

        let mut by_line_id = HashMap::with_capacity(project.line_count());
        for track_lines in &mut lines_by_track {
            track_lines.sort_by_key(|line| (line.start_frame, line.id));
            let mut previous_line: Option<&crate::rythmo_line::RythmoLine> = None;
            for line in track_lines.iter().copied() {
                if line.karaoke {
                    let previous_adjacent = previous_line.and_then(|previous| {
                        if previous.karaoke
                            && karaoke_previous_gap_frames(previous, line) <= max_gap_frames
                        {
                            Some(previous)
                        } else {
                            None
                        }
                    });
                    let island_index = previous_adjacent
                        .and_then(|previous| by_line_id.get(&previous.id))
                        .map(|previous_state: &KaraokeLineUiState| previous_state.island_index + 1)
                        .unwrap_or_else(|| {
                            if previous_line.is_some_and(|previous| !previous.karaoke) {
                                1
                            } else {
                                0
                            }
                        });
                    let label_visible = !line.character_name.is_empty()
                        && previous_adjacent
                            .map(|previous| previous.character_name != line.character_name)
                            .unwrap_or(true);
                    by_line_id.insert(
                        line.id,
                        KaraokeLineUiState {
                            previous_adjacent_karaoke_id: previous_adjacent.map(|line| line.id),
                            previous_adjacent_start_frame: previous_adjacent
                                .map(|line| line.start_frame),
                            stack_row: island_index % 2,
                            label_visible,
                            island_index,
                        },
                    );
                }
                previous_line = Some(line);
            }
        }

        Self {
            signature,
            by_line_id,
            karaoke_timeline,
        }
    }

    fn timeline_cursor_at(&self, frame: i64) -> usize {
        self.karaoke_timeline
            .partition_point(|(start_frame, _)| *start_frame < frame)
            .min(self.karaoke_timeline.len().saturating_sub(1))
    }

    fn line_state(&self, line: &crate::rythmo_line::RythmoLine) -> KaraokeLineUiState {
        self.by_line_id.get(&line.id).copied().unwrap_or_default()
    }

    #[cfg(test)]
    fn previous_adjacent_karaoke_id(&self, line: &crate::rythmo_line::RythmoLine) -> Option<u64> {
        self.line_state(line).previous_adjacent_karaoke_id
    }

    fn stack_row(&self, line: &crate::rythmo_line::RythmoLine) -> usize {
        self.line_state(line).stack_row
    }

    #[cfg(test)]
    fn prestart_scroll_visible(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
        count_in_frames: i64,
    ) -> bool {
        line.karaoke
            && karaoke_count_in_visible(line, current_frame, count_in_frames)
            && self.line_state(line).previous_adjacent_karaoke_id.is_none()
    }

    fn upcoming_stack_visible(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
    ) -> bool {
        if !line.karaoke || current_frame >= line.start_frame as f64 {
            return false;
        }

        self.line_state(line)
            .previous_adjacent_start_frame
            .is_some_and(|start_frame| current_frame >= start_frame as f64)
    }

    fn character_label_visible(&self, line: &crate::rythmo_line::RythmoLine) -> bool {
        self.line_state(line).label_visible
    }
}

#[cfg(test)]
fn previous_line_on_same_track_before<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && same_karaoke_track(candidate, line)
                && (candidate.start_frame < line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id < line.id))
        })
        .max_by_key(|candidate| (candidate.start_frame, candidate.id))
}

#[cfg(test)]
fn next_line_on_same_track_after<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && same_karaoke_track(candidate, line)
                && (candidate.start_frame > line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id > line.id))
        })
        .min_by_key(|candidate| (candidate.start_frame, candidate.id))
}

#[cfg(test)]
fn previous_karaoke_line_before<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    let previous = previous_line_on_same_track_before(project, line)?;
    if previous.karaoke && karaoke_previous_gap_frames(previous, line) <= max_gap_frames {
        Some(previous)
    } else {
        None
    }
}

#[cfg(test)]
fn next_karaoke_line_after<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    let next = next_line_on_same_track_after(project, line)?;
    if next.karaoke && karaoke_next_gap_frames(line, next) <= max_gap_frames {
        Some(next)
    } else {
        None
    }
}

#[cfg(test)]
fn karaoke_prestart_scroll_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
    count_in_frames: i64,
) -> bool {
    line.karaoke
        && karaoke_count_in_visible(line, current_frame, count_in_frames)
        && previous_karaoke_line_before(project, line, max_gap_frames).is_none()
}

#[cfg(test)]
fn karaoke_upcoming_stack_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || current_frame >= line.start_frame as f64 {
        return false;
    }

    previous_karaoke_line_before(project, line, max_gap_frames)
        .is_some_and(|previous| current_frame >= previous.start_frame as f64)
}

#[cfg(test)]
fn karaoke_island_index(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> usize {
    let mut index = 0;
    let mut current = line;
    while let Some(previous) = previous_karaoke_line_before(project, current, max_gap_frames) {
        index += 1;
        current = previous;
    }
    if previous_line_on_same_track_before(project, current)
        .is_some_and(|previous| !previous.karaoke)
    {
        index += 1;
    }
    index
}

#[cfg(test)]
fn karaoke_stack_row(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> usize {
    karaoke_island_index(project, line, max_gap_frames) % 2
}

fn karaoke_stack_rect(mut rect: Rect, row: usize, scale: f32) -> Rect {
    let gap = rythmo_layout::karaoke_stack_gap(rect.height, scale);
    let row_h = ((rect.height - gap).max(1.0) / 2.0).max(1.0);
    rect.y += row.min(1) as f32 * (row_h + gap);
    rect.height = row_h;
    rect
}

#[cfg(test)]
fn karaoke_character_label_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || line.character_name.is_empty() {
        return false;
    }

    previous_karaoke_line_before(project, line, max_gap_frames)
        .map(|previous| previous.character_name != line.character_name)
        .unwrap_or(true)
}

fn karaoke_centered_x_width_with_width(zone: &Rect, width: f32) -> (f32, f32) {
    let center_x = zone.x + zone.width / 2.0;
    (center_x - width / 2.0, width)
}

fn karaoke_preview_line_rect_with_state(
    layout_ctx: &EditorLayoutCtx,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    count_in: bool,
    upcoming_stack: bool,
    stack_row: usize,
    centered_karaoke_width: Option<f32>,
) -> Rect {
    let (x1, width) = if line.karaoke_active(current_frame) || count_in || upcoming_stack {
        let width = centered_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        karaoke_centered_x_width_with_width(zone, width)
    } else {
        line_visual_x_width(line, current_frame, zone, true)
    };
    let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
    let rect = Rect {
        x: x1,
        y: body_rect.y,
        width,
        height: body_rect.height,
    };
    if layout_ctx.track_for_y_slot(line.y_slot).has_karaoke {
        karaoke_stack_rect(rect, stack_row, 1.0)
    } else {
        rect
    }
}

fn karaoke_line_uses_playback_mode(
    layout_ctx: &EditorLayoutCtx,
    line: &crate::rythmo_line::RythmoLine,
    karaoke_preview: bool,
) -> bool {
    karaoke_preview && line.karaoke && layout_ctx.track_for_y_slot(line.y_slot).has_karaoke
}

#[cfg(test)]
fn karaoke_preview_line_rect(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    max_gap_frames: i64,
) -> Rect {
    let upcoming_stack =
        karaoke_upcoming_stack_visible(project, line, current_frame, max_gap_frames);
    let layout_ctx = EditorLayoutCtx::new_at_frame(project, current_frame, zone);
    karaoke_preview_line_rect_with_state(
        &layout_ctx,
        line,
        current_frame,
        zone,
        false,
        upcoming_stack,
        karaoke_stack_row(project, line, max_gap_frames),
        None,
    )
}

fn badge_rect_for_karaoke_rect(line: &crate::rythmo_line::RythmoLine, line_rect: &Rect) -> Rect {
    let width = badge_width(&line.character_name);
    let badge_h = line_rect.height * BADGE_OVERLAP_HEIGHT_RATIO;
    // Right edge a few px to the left of the line's top-left corner, top-aligned.
    Rect {
        x: line_rect.x - width - BADGE_GAP,
        y: line_rect.y,
        width,
        height: badge_h,
    }
}

fn visible_syllable_segments(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    _karaoke_preview: bool,
    state: &RythmoState,
) -> Option<(Vec<usize>, Vec<f32>)> {
    if line.text.is_empty() || line.text == "↑" || line.text == "↓" {
        return None;
    }

    let breaks = state.get_syllable_breaks(line, lang);
    if breaks.is_empty() {
        return None;
    }

    if let Some(drag) = drag.filter(|drag| drag.line_id == line.id) {
        return Some((breaks, drag.ratios.clone()));
    }

    // An untouched line must be shaped and stretched as one text run. Splitting
    // every syllable into the default timing slots scales each run separately,
    // which makes some syllables look condensed and others widely spaced.
    // Segment the text only after the user has explicitly saved valid timings.
    valid_saved_syllable_ratios(line, breaks.len() + 1, lang).map(|ratios| (breaks, ratios))
}

fn valid_saved_syllable_ratios(
    line: &crate::rythmo_line::RythmoLine,
    syllable_count: usize,
    lang: &str,
) -> Option<Vec<f32>> {
    (line.syllable_ratios.len() == syllable_count
        && line
            .syllable_ratios
            .iter()
            .all(|ratio| ratio.is_finite() && *ratio > 0.0))
    .then(|| crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang))
}

fn cursor_ratios_from_segments(text: &str, breaks: &[usize], ratios: &[f32]) -> Vec<f32> {
    let char_count = text.chars().count();
    let mut cursor_ratios = vec![0.0; char_count + 1];
    let mut seg_start = 0usize;
    let mut x = 0.0;

    for (i, ratio) in ratios.iter().enumerate() {
        let seg_end = if i < breaks.len() {
            breaks[i].min(char_count)
        } else {
            char_count
        };
        let seg_len = seg_end.saturating_sub(seg_start);
        if seg_len > 0 {
            for local_idx in 0..=seg_len {
                let char_idx = seg_start + local_idx;
                if char_idx <= char_count {
                    let local_ratio = local_idx as f32 / seg_len as f32;
                    cursor_ratios[char_idx] = (x + local_ratio * ratio).clamp(0.0, 1.0);
                }
            }
        }
        x += ratio;
        seg_start = seg_end;
    }

    if let Some(last) = cursor_ratios.last_mut() {
        *last = 1.0;
    }
    cursor_ratios
}

fn closest_cursor_index_from_ratios(ratios: &[f32], x_ratio: f32) -> Option<usize> {
    ratios
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - x_ratio)
                .abs()
                .partial_cmp(&(*b - x_ratio).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
}

fn segmented_cursor_ratios_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
) -> Option<Vec<f32>> {
    let (breaks, ratios) = visible_syllable_segments(line, drag, lang, karaoke_preview, state)?;
    Some(cursor_ratios_from_segments(&line.text, &breaks, &ratios))
}

pub fn cursor_segments_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
) -> Option<Vec<CursorSegmentInfo>> {
    let (breaks, ratios) = visible_syllable_segments(line, drag, lang, karaoke_preview, state)?;
    let mut start_char = 0usize;
    let mut start_ratio = 0.0;
    let mut segments = Vec::new();

    for (i, ratio) in ratios.iter().enumerate() {
        let end_char = if i < breaks.len() {
            breaks[i]
        } else {
            line.text.chars().count()
        };
        if end_char > start_char && *ratio > 0.0 {
            segments.push(CursorSegmentInfo {
                cache_id: syllable_segment_cache_id(line.id, i),
                start_char,
                end_char,
                start_ratio,
                width_ratio: *ratio,
            });
        }
        start_char = end_char;
        start_ratio = (start_ratio + ratio).clamp(0.0, 1.0);
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

pub fn segmented_cursor_index_for_line_at_ratio(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
    x_ratio: f32,
) -> Option<usize> {
    let ratios = segmented_cursor_ratios_for_line(line, drag, lang, karaoke_preview, state)?;
    closest_cursor_index_from_ratios(&ratios, x_ratio)
}

fn cursor_index_for_line_at_ratio(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
    x_ratio: f32,
) -> usize {
    if let Some(idx) =
        segmented_cursor_index_for_line_at_ratio(line, drag, lang, karaoke_preview, state, x_ratio)
    {
        idx
    } else {
        (x_ratio * line.text.chars().count() as f32).round() as usize
    }
}

fn push_karaoke_rythmo_text(
    stretched: &mut Vec<StretchedText>,
    line: &crate::rythmo_line::RythmoLine,
    dest_rect: Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
) {
    let cache_id = karaoke_text_cache_id(line.id);
    push_natural_karaoke_text(
        stretched,
        cache_id,
        line.text.clone(),
        dest_rect,
        [1.0, 1.0, 1.0, 1.0],
    );

    let Some(progress_info) = progress_info else {
        return;
    };
    if let Some(colored) = StretchedText::natural_clipped(
        cache_id,
        line.text.clone(),
        dest_rect,
        progress_info.visual_progress,
        constants::KARAOKE_TEXT_FONT_SCALE,
        line_color_tint(line),
    ) {
        stretched.push(colored);
    }
}

fn push_editor_karaoke_texture_prewarm_texts(
    stretched: &mut Vec<StretchedText>,
    state: &RythmoState,
    project: &Project,
    index: &KaraokeUiIndex,
    layout_ctx: &EditorLayoutCtx,
    current_frame: i64,
    fps: f64,
    zone: &Rect,
) {
    let lookahead_frames =
        (fps.max(1.0) * KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS).round() as i64;
    let start = index.timeline_cursor_at(current_frame - lookahead_frames / 10);
    let end_frame = current_frame + lookahead_frames;
    let mut pushed = 0;

    for &(start_frame, line_id) in index
        .karaoke_timeline
        .iter()
        .skip(start)
        .take(KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME)
    {
        if start_frame > end_frame {
            break;
        }
        let Some(line) = project.get_line(line_id) else {
            continue;
        };
        if line.text.is_empty() || line.text == "↑" || line.text == "↓" {
            continue;
        }

        let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
        let row_rect = karaoke_stack_rect(
            Rect {
                x: zone.x,
                y: body_rect.y,
                width: state.karaoke_ui_text_width_for_render(line),
                height: body_rect.height,
            },
            index.stack_row(line),
            1.0,
        );
        stretched.push(StretchedText::natural_prewarm(
            karaoke_text_cache_id(line.id),
            line.text.clone(),
            row_rect,
            constants::KARAOKE_TEXT_FONT_SCALE,
        ));
        pushed += 1;
        if pushed >= KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME {
            break;
        }
    }
}

fn syllable_ratios_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<Vec<f32>> {
    let breaks = state.get_syllable_breaks(line, lang);
    if breaks.is_empty() {
        return None;
    }

    if let Some(drag) = drag.filter(|drag| drag.line_id == line.id) {
        return Some(drag.ratios.clone());
    }

    if let Some(ratios) = valid_saved_syllable_ratios(line, breaks.len() + 1, lang) {
        return Some(ratios);
    }

    Some(state.default_syllable_visual_ratios(line, lang, &breaks))
}

fn render_syllable_handles(
    rect: &Rect,
    ratios: &[f32],
    active: bool,
    quads: &mut Vec<QuadInstance>,
) {
    if ratios.len() <= 1 || rect.width <= 2.0 {
        return;
    }

    let alpha = if active { 1.0 } else { 0.78 };
    let color = [0.95, 0.08, 0.03, alpha];
    let stroke = if active { 3.0 } else { 2.5 };
    let tick_h = if active { 9.0 } else { 7.0 };
    let top_y = rect.y + 1.0;
    let cap_gap = 2.0;

    let mut x = rect.x;
    let mut boundaries = vec![rect.x];
    for ratio in ratios.iter().take(ratios.len() - 1) {
        x += ratio * rect.width;
        boundaries.push(x.clamp(rect.x, rect.x + rect.width));
    }
    boundaries.push(rect.x + rect.width);

    for pair in boundaries.windows(2) {
        let start = pair[0] + cap_gap;
        let end = pair[1] - cap_gap;
        if end > start {
            quads.push(QuadInstance {
                rect: [start, top_y, end - start, stroke],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: stroke / 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0, 0.0, 0.0, 0.22],
                shadow_blur: 2.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }

    for boundary in boundaries {
        quads.push(QuadInstance {
            rect: [boundary - stroke / 2.0, top_y, stroke, tick_h],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: stroke / 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0, 0.0, 0.0, 0.22],
            shadow_blur: 2.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
}

pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
) -> Option<(
    u64,
    usize,
    Option<(usize, usize)>,
    f32,
    f32,
    f32,
    f32,
    Option<Vec<CursorSegmentInfo>>,
)> {
    if let Some(drag) = state.dragging.as_ref().filter(|drag| {
        drag.handle == DragHandle::VerticalOnly && matches!(drag.target, DragTarget::Line(_))
    }) {
        if let DragTarget::Line(line_id) = drag.target {
            if project.get_line(line_id).is_some() {
                let guide_x = frame_to_x(drag.original_frame, current_frame, zone);
                syllable_quads.push(QuadInstance {
                    rect: [guide_x - 1.0, zone.y, 2.0, zone.height],
                    color: [0.68, 0.68, 0.72, 0.9],
                    color_bottom: [0.68, 0.68, 0.72, 0.9],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 1.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0, 0.0, 0.0, 0.25],
                    shadow_blur: 2.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
        }
    }
    state.prune_karaoke_text_width_cache(project);
    let karaoke_max_gap_frames = karaoke_adjacent_max_gap_frames(fps);
    let karaoke_index = state.cached_karaoke_ui_index(project, karaoke_max_gap_frames);
    let current_frame_i64 = visual_frame_to_i64(current_frame);
    state.prewarm_karaoke_text_widths(
        project,
        &karaoke_index,
        current_frame_i64,
        (fps.max(1.0) * 10.0).round() as i64,
        if karaoke_preview { 2 } else { 8 },
    );
    let karaoke_count_in_frame_count = karaoke_count_in_frames(fps);
    let layout_ctx = state.get_or_create_layout_ctx(project, current_frame, fps, zone);

    // Rend le highlight de la track survolée (s'il y en a une et qu'elle est valide)
    if let Some(track_idx) = state.hovered_track {
        if let Some(track) = layout_ctx.track_for_index(track_idx) {
            let y_base = zone.y + constants::RULER_HEIGHT + track.top;
            quads.push(QuadInstance {
                rect: [zone.x, y_base, zone.width, track.total_h],
                color: [1.0, 1.0, 1.0, 0.03],
                color_bottom: [1.0, 1.0, 1.0, 0.03],
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
    }

    let mut cursor_info = None;
    let karaoke_lang = crate::config::get().lang.clone();
    let margin_frames = interactive_render_margin_frames(fps, render_index);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames);
    let mut visible_line_ids = render_index.visible_line_ids(project, first_frame, last_frame);
    visible_line_ids.sort_by_key(|id| render_index.line_order_index(*id));

    // Precompute line data ONCE - rect, karaoke flags, badge rect, character name
    #[derive(Clone, Copy)]
    struct LineRenderData {
        rect: Rect,
        badge_rect: Rect,
        karaoke_playback: bool,
        karaoke_count_in: bool,
        karaoke_progress_info: Option<KaraokeProgressRenderInfo>,
    }

    let mut line_data: Vec<(u64, LineRenderData)> = Vec::with_capacity(visible_line_ids.len());
    for &lid in &visible_line_ids {
        let Some(line) = project.get_line(lid) else {
            continue;
        };
        let karaoke_active = line.karaoke_active(current_frame);
        let karaoke_playback = karaoke_line_uses_playback_mode(&layout_ctx, line, karaoke_preview);
        // Playback never renders karaoke as an ordinary scrolling line.
        if karaoke_preview && line.karaoke && !karaoke_playback {
            continue;
        }
        let karaoke_count_in = karaoke_playback
            && karaoke_count_in_visible(line, current_frame, karaoke_count_in_frame_count);
        let karaoke_upcoming_stack =
            karaoke_playback && karaoke_index.upcoming_stack_visible(line, current_frame);

        if karaoke_playback && !karaoke_active && !karaoke_count_in && !karaoke_upcoming_stack {
            continue;
        }

        let centered_karaoke_width =
            if karaoke_playback && (karaoke_active || karaoke_count_in || karaoke_upcoming_stack) {
                Some(state.karaoke_ui_text_width_for_render(line))
            } else {
                None
            };
        let r = if karaoke_playback {
            karaoke_preview_line_rect_with_state(
                &layout_ctx,
                line,
                current_frame,
                zone,
                karaoke_count_in,
                karaoke_upcoming_stack,
                karaoke_index.stack_row(line),
                centered_karaoke_width,
            )
        } else {
            layout_ctx.line_rect_with_karaoke_width(
                line,
                current_frame,
                zone,
                karaoke_preview,
                None,
            )
        };

        let badge_rect = if karaoke_playback {
            badge_rect_for_karaoke_rect(line, &r)
        } else {
            layout_ctx.badge_rect_for_name(line, &line.character_name, r.x, zone)
        };
        let show_badge = !karaoke_playback || karaoke_index.character_label_visible(line);
        let leading_visual = show_badge.then(|| {
            rythmo_layout::leading_visual_bounds(
                badge_rect.x,
                badge_rect.width,
                if !karaoke_playback {
                    line.voice_actor_names.len()
                } else {
                    0
                },
                ACTOR_ICON_SIZE,
                ACTOR_ICON_GAP,
            )
        });
        if !rythmo_layout::line_or_badge_intersects_viewport(
            r.x,
            r.width,
            leading_visual,
            zone.x,
            zone.x + zone.width,
        ) {
            continue;
        }

        let karaoke_progress_info = if karaoke_playback {
            karaoke_progress_render_info(line, current_frame, &karaoke_lang)
        } else {
            None
        };

        line_data.push((
            lid,
            LineRenderData {
                rect: r,
                badge_rect,
                karaoke_playback,
                karaoke_count_in,
                karaoke_progress_info,
            },
        ));
    }

    // Keep a stable vertical draw order, then compare every badge with the
    // actual body of the other visible lines.
    line_data.sort_by(|a, b| {
        a.1.badge_rect
            .y
            .partial_cmp(&b.1.badge_rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut badge_hidden: HashMap<u64, bool> = HashMap::new();
    let mut badge_overlap_alpha: HashMap<u64, f32> = HashMap::new();
    let collision_targets: Vec<(u64, Rect, &str)> = line_data
        .iter()
        .filter_map(|(line_id, data)| {
            project
                .get_line(*line_id)
                .map(|line| (*line_id, data.rect, line.character_name.as_str()))
        })
        .collect();

    for (line_id, data) in &line_data {
        let Some(line) = project.get_line(*line_id) else {
            continue;
        };
        let (hidden, alpha) = character_badge_collision_style(
            *line_id,
            &line.character_name,
            &data.badge_rect,
            &collision_targets,
        );
        badge_hidden.insert(*line_id, hidden);
        badge_overlap_alpha.insert(*line_id, alpha);
    }

    // Now render all lines using precomputed data
    for (line_id, data) in line_data {
        let Some(line) = project.get_line(line_id) else {
            continue;
        };

        let is_hovered = state.hovered_line == Some(line.id);
        let is_selected = matches!(state.selected, Some(Selection::Line(id)) if id == line.id)
            || matches!(state.selected, Some(Selection::Lines(ref ids)) if ids.contains(&line.id))
            || matches!(state.selected, Some(Selection::AllLines));
        let is_editing = state.editing_line == Some(line.id);
        let karaoke_playback_line = data.karaoke_playback;
        let read_highlight_end = if project.settings().highlight_read_word && !line.karaoke {
            let progress =
                (current_frame - line.start_frame as f64) / line.duration_frames.max(1) as f64;
            crate::syllable::read_highlight_end_from_timing(
                &line.text,
                &line.syllable_ratios,
                &karaoke_lang,
                progress as f32,
            )
        } else {
            None
        };

        if !karaoke_playback_line {
            // Subtle dark background + border
            let bg = if is_editing {
                [0.12, 0.12, 0.15, 0.6]
            } else if is_hovered {
                [0.10, 0.10, 0.13, 0.4]
            } else {
                [0.08, 0.08, 0.10, 0.3]
            };
            let border = if is_selected {
                [0.90, 0.78, 0.30, 0.75]
            } else if line.karaoke {
                [0.35, 0.72, 1.0, 0.75]
            } else if is_hovered || is_editing {
                LINE_BORDER_HOVER
            } else {
                LINE_BORDER
            };
            quads.push(QuadInstance {
                rect: [data.rect.x, data.rect.y, data.rect.width, data.rect.height],
                color: bg,
                color_bottom: bg,
                border_color: border,
                border_width: if is_selected { 1.5 } else { 1.0 },
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        let scrolling_text_tint = if project.settings().scrolling_text_uses_character_color {
            [
                line.character_color[0].clamp(0.0, 1.0),
                line.character_color[1].clamp(0.0, 1.0),
                line.character_color[2].clamp(0.0, 1.0),
                1.0,
            ]
        } else {
            [1.0; 4]
        };

        // Stretched text or special rendering for breath arrows
        let mut cursor_segments = None;
        if !line.text.is_empty() {
            if line.text == "↑" || line.text == "↓" {
                render_breath_arrow(&data.rect, line.text == "↑", quads);
            } else if karaoke_playback_line {
                push_karaoke_rythmo_text(stretched, line, data.rect, data.karaoke_progress_info);
            } else {
                let drag_ratios = state
                    .syllable_drag
                    .as_ref()
                    .filter(|d| d.line_id == line.id);
                if let Some((breaks, ratios)) = visible_syllable_segments(
                    line,
                    drag_ratios,
                    &karaoke_lang,
                    karaoke_preview,
                    state,
                ) {
                    let chars: Vec<char> = line.text.chars().collect();
                    let mut seg_x = data.rect.x;
                    let mut prev_break = 0usize;
                    let mut editing_segments = if is_editing { Some(Vec::new()) } else { None };
                    for (i, &ratio) in ratios.iter().enumerate() {
                        let seg_w = ratio * data.rect.width;
                        let end_break = if i < breaks.len() {
                            breaks[i]
                        } else {
                            chars.len()
                        };
                        let segment: String = chars[prev_break..end_break].iter().collect();
                        if !segment.is_empty() && seg_w > 1.0 {
                            let cache_id = syllable_segment_cache_id(line.id, i);
                            if let Some(segments) = &mut editing_segments {
                                segments.push(CursorSegmentInfo {
                                    cache_id,
                                    start_char: prev_break,
                                    end_char: end_break,
                                    start_ratio: ((seg_x - data.rect.x) / data.rect.width)
                                        .clamp(0.0, 1.0),
                                    width_ratio: (seg_w / data.rect.width).clamp(0.0, 1.0),
                                });
                            }
                            push_read_word_rythmo_text(
                                stretched,
                                cache_id,
                                segment,
                                Rect {
                                    x: seg_x,
                                    y: data.rect.y,
                                    width: seg_w,
                                    height: data.rect.height,
                                },
                                prev_break,
                                read_highlight_end,
                                scrolling_text_tint,
                            );
                        }
                        seg_x += seg_w;
                        prev_break = end_break;
                    }
                    cursor_segments = editing_segments.filter(|segments| !segments.is_empty());
                } else {
                    push_read_word_rythmo_text(
                        stretched,
                        line.id,
                        line.text.clone(),
                        Rect {
                            x: data.rect.x,
                            y: data.rect.y,
                            width: data.rect.width,
                            height: data.rect.height,
                        },
                        0,
                        read_highlight_end,
                        scrolling_text_tint,
                    );
                }
            }
        }

        // Cursor info for mod.rs to resolve with renderer
        if is_editing {
            if state.line_input.cursor_visible() || state.line_input.has_selection() {
                cursor_info = Some((
                    line.id,
                    state.line_input.cursor_pos,
                    state.line_input.selection_range(),
                    data.rect.x,
                    data.rect.width,
                    data.rect.y,
                    data.rect.height,
                    cursor_segments.clone(),
                ));
            }
        }

        if data.karaoke_count_in {
            render_karaoke_count_in_dot_scaled(
                line,
                current_frame,
                &data.rect,
                karaoke_count_in_frame_count,
                1.0,
                quads,
            );
        } else if karaoke_playback_line {
            render_karaoke_dot(line, &data.rect, data.karaoke_progress_info, quads);
        }

        let is_syllable_drag_line =
            state.syllable_drag.as_ref().map(|d| d.line_id) == Some(line.id);
        if !karaoke_playback_line && (is_hovered || is_syllable_drag_line) {
            if let Some(ratios) =
                syllable_ratios_for_line(line, state.syllable_drag.as_ref(), &karaoke_lang, state)
            {
                render_syllable_handles(&data.rect, &ratios, true, syllable_quads);
            }
        }

        // Handles (only on hover/editing)
        if (is_hovered || is_editing) && !karaoke_playback_line {
            quads.push(QuadInstance {
                rect: [
                    data.rect.x,
                    data.rect.y,
                    constants::HANDLE_WIDTH,
                    data.rect.height,
                ],
                color: HANDLE_COLOR,
                color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            quads.push(QuadInstance {
                rect: [
                    data.rect.x + data.rect.width - constants::HANDLE_WIDTH,
                    data.rect.y,
                    constants::HANDLE_WIDTH,
                    data.rect.height,
                ],
                color: HANDLE_COLOR,
                color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Character badge — use precomputed badge_rect
        let br = data.badge_rect;

        // Overlap detection vs OTHER lines: use precomputed HashMaps
        let badge_hidden = *badge_hidden.get(&line_id).unwrap_or(&false);
        let badge_overlap_alpha = *badge_overlap_alpha.get(&line_id).unwrap_or(&1.0);

        if karaoke_playback_line {
            if karaoke_index.character_label_visible(line) {
                labels.push(LabelInfo {
                    text: &line.character_name,
                    bounds: br,
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Visible,
                    padding: 0.0,
                    font_size_override: Some(BADGE_FONT_SIZE),
                    color_override: Some(line_color_label(line)),
                    font_family_override: None,
                });
            }
            continue;
        }

        if !badge_hidden {
            let mut badge_color = line.character_color;
            badge_color[3] *= badge_overlap_alpha;
            let is_editing_char = state.editing_character == Some(line.id);
            let badge_border = if is_editing_char {
                [0.8, 0.8, 0.85, 0.8]
            } else {
                [0.0_f32; 4]
            };
            quads.push(QuadInstance {
                rect: [br.x, br.y, br.width, br.height],
                color: badge_color,
                color_bottom: badge_color,
                border_color: badge_border,
                border_width: if is_editing_char { 1.0 } else { 0.0 },
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            // Character name text — black on bright backgrounds for contrast
            if !line.character_name.is_empty() {
                let [r, g, b, _] = line.character_color;
                let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                let text_color = if luminance > 0.55 {
                    Some([0, 0, 0])
                } else {
                    None
                };

                labels.push(LabelInfo {
                    text: &line.character_name,
                    bounds: br,
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: BADGE_PADDING_H,
                    font_size_override: Some(BADGE_FONT_SIZE),
                    color_override: text_color,
                    font_family_override: None,
                });
            }

            render_voice_actor_icons_for_line(
                line,
                project,
                zone,
                br,
                ACTOR_ICON_SIZE,
                quads,
                labels,
                actor_icons,
            );

            text_input::render_selection_and_cursor(
                quads,
                br,
                &line.character_name,
                &state.char_input,
                is_editing_char,
                badge_text_metrics(),
                3.0,
                3.0,
                [0.25, 0.45, 0.95, 0.45],
                CURSOR_COLOR,
            );

            // Note indicator: small icon at the end of the badge if line has a note
            if !line.note.is_empty() {
                let icon_size = 10.0;
                note_icons.push(IconInstance {
                    rect: [
                        br.x + br.width - icon_size - 2.0,
                        br.y + (br.height - icon_size) / 2.0,
                        icon_size,
                        icon_size,
                    ],
                    uv_rect: note_uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
        }

        // Note text: small italic label at the bottom of the line
        let note_label_h = 12.0;
        let note_y = data.rect.y + data.rect.height - note_label_h - 1.0;
        let note_rect = Rect {
            x: data.rect.x + 4.0,
            y: note_y,
            width: data.rect.width - 8.0,
            height: note_label_h,
        };
        if !line.note.is_empty() {
            labels.push(LabelInfo {
                text: &line.note,
                bounds: note_rect,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(9.0),
                color_override: Some([160, 160, 170]),
                font_family_override: None,
            });
        }

        let is_editing_note = state.editing_note == Some(line.id);
        text_input::render_selection_and_cursor(
            quads,
            note_rect,
            &line.note,
            &state.note_input,
            is_editing_note,
            note_text_metrics(),
            1.0,
            1.0,
            [0.25, 0.45, 0.95, 0.45],
            CURSOR_COLOR,
        );
    }

    push_editor_karaoke_texture_prewarm_texts(
        stretched,
        state,
        project,
        &karaoke_index,
        &layout_ctx,
        current_frame_i64,
        fps,
        zone,
    );

    // Ghost preview line when holding click on empty space
    if let Some(ghost) = &state.ghost_preview {
        let body_rect = layout_ctx.track_body_rect(ghost.y_slot, zone);
        let ghost_rect_x = frame_to_x(ghost.frame, current_frame, zone);
        let ghost_w = (ghost.duration_frames as f32 * ppf()).max(2.0);

        let ghost_bg = [0.25, 0.25, 0.35, 0.2];
        let ghost_border = [0.5, 0.5, 0.6, 0.3];
        quads.push(QuadInstance {
            rect: [ghost_rect_x, body_rect.y, ghost_w, layout_ctx.normal_body_h],
            color: ghost_bg,
            color_bottom: ghost_bg,
            border_color: ghost_border,
            border_width: 1.0,
            border_radius: LINE_RADIUS,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Ghost badge — rectangular, top-aligned, right edge a few px left of line
        let ghost_badge_w = BADGE_MIN_W;
        let ghost_badge_h = body_rect.height * BADGE_OVERLAP_HEIGHT_RATIO;
        let ghost_badge_x = ghost_rect_x - BADGE_GAP - ghost_badge_w;
        quads.push(QuadInstance {
            rect: [ghost_badge_x, body_rect.y, ghost_badge_w, ghost_badge_h],
            color: [0.4, 0.4, 0.5, 0.2],
            color_bottom: [0.4, 0.4, 0.5, 0.2],
            border_color: ghost_border,
            border_width: 1.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    cursor_info
}

fn render_voice_actor_icons_for_line<'a>(
    line: &'a crate::rythmo_line::RythmoLine,
    project: &'a Project,
    zone: &Rect,
    badge: Rect,
    icon_size: f32,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
) {
    if line.voice_actor_names.is_empty() {
        return;
    }

    let size = icon_size;
    let gap = ACTOR_ICON_GAP;
    // The badge ends immediately before the line body. Keep actor icons on
    // the outer side of the badge so they cannot cover the line text.
    let mut x = badge.x - gap - size;
    let y = badge.y + (badge.height - size) * 0.5;
    for actor_name in &line.voice_actor_names {
        if x > zone.x + zone.width {
            break;
        }
        let rect = Rect {
            x,
            y,
            width: size,
            height: size,
        };
        quads.push(QuadInstance {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: [0.05, 0.05, 0.07, 0.92],
            color_bottom: [0.02, 0.02, 0.03, 0.92],
            border_color: [0.75, 0.75, 0.85, 0.45],
            border_width: 1.0,
            border_radius: 3.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        if let Some(actor) = project.find_voice_actor(actor_name) {
            if actor.icon_png_base64.is_some() {
                actor_icons.push(VoiceActorIconDraw {
                    actor_name: actor.name.clone(),
                    rect,
                });
            } else {
                labels.push(LabelInfo {
                    text: &actor.name,
                    bounds: rect,
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 1.0,
                    font_size_override: Some((size * 0.55).max(8.0)),
                    color_override: Some([230, 230, 238]),
                    font_family_override: None,
                });
            }
        } else {
            labels.push(LabelInfo {
                text: actor_name,
                bounds: rect,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 1.0,
                font_size_override: Some((size * 0.55).max(8.0)),
                color_override: Some([230, 230, 238]),
                font_family_override: None,
            });
        }
        x -= size + gap;
    }
}

/// Render a diagonal arrow for breath markers using rotated quads.
/// `up` = bottom-left → top-right (inspiration), `!up` = top-left → bottom-right (expiration).
fn render_breath_arrow(r: &Rect, up: bool, quads: &mut Vec<QuadInstance>) {
    let margin = 4.0;
    let cx = r.x + r.width / 2.0;
    let cy = r.y + r.height / 2.0;
    let dx = r.width - margin * 2.0;
    let dy = r.height - margin * 2.0;
    let length = (dx * dx + dy * dy).sqrt();
    let angle = if up {
        -(dy).atan2(dx) // bottom-left to top-right
    } else {
        (dy).atan2(dx) // top-left to bottom-right
    };
    let thickness = 2.0;
    let color = [0.85, 0.85, 0.90, 0.9];

    // Main diagonal line — a thin rectangle rotated
    quads.push(QuadInstance {
        rect: [cx - length / 2.0, cy - thickness / 2.0, length, thickness],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: angle,
        _padding: [0.0; 2],
    });

    // Arrowhead at the end (top-right for up, bottom-right for down)
    let tip_x = r.x + r.width - margin;
    let tip_y = if up {
        r.y + margin
    } else {
        r.y + r.height - margin
    };
    let arrow_len = 8.0;
    let arrow_thickness = 2.0;
    let spread = 0.5; // ~30 degrees from the main line

    // Two short lines forming the arrowhead
    let base_angle = std::f32::consts::PI + angle;
    quads.push(QuadInstance {
        rect: [
            tip_x - arrow_len / 2.0,
            tip_y - arrow_thickness / 2.0,
            arrow_len,
            arrow_thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: base_angle + spread,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [
            tip_x - arrow_len / 2.0,
            tip_y - arrow_thickness / 2.0,
            arrow_len,
            arrow_thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: base_angle - spread,
        _padding: [0.0; 2],
    });
}

fn render_karaoke_dot(
    line: &crate::rythmo_line::RythmoLine,
    line_rect: &Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
    quads: &mut Vec<QuadInstance>,
) {
    render_karaoke_dot_scaled(line, line_rect, progress_info, 1.0, quads);
}

fn karaoke_count_in_dot_rect(line_rect: &Rect, count_in_progress: f32, scale: f32) -> Rect {
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let progress = count_in_progress.clamp(0.0, 1.0);
    let bounce_progress = (progress * constants::KARAOKE_COUNT_IN_BOUNCES).fract();
    let bounce = (bounce_progress * std::f32::consts::PI).sin().max(0.0);
    let travel = constants::KARAOKE_NEXT_PREVIEW_GAP * 4.0 * scale + size * 2.0;
    let start_x = line_rect.x - travel;
    let end_x = line_rect.x;
    Rect {
        x: start_x + (end_x - start_x) * progress,
        y: line_rect.y + 3.0 * scale.max(0.5)
            - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE,
        width: size,
        height: size,
    }
}

fn render_karaoke_count_in_dot_scaled(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    line_rect: &Rect,
    count_in_frames: i64,
    scale: f32,
    quads: &mut Vec<QuadInstance>,
) {
    let Some(count_in_progress) = karaoke_count_in_progress(line, current_frame, count_in_frames)
    else {
        return;
    };

    let dot = karaoke_count_in_dot_rect(line_rect, count_in_progress, scale);
    let tint = line_color_tint(line);
    quads.push(QuadInstance {
        rect: [dot.x - 1.5, dot.y - 1.5, dot.width + 3.0, dot.height + 3.0],
        color: [0.0, 0.0, 0.0, 0.35],
        color_bottom: [0.0, 0.0, 0.0, 0.35],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: (dot.width + 3.0) / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [dot.x, dot.y, dot.width, dot.height],
        color: tint,
        color_bottom: tint,
        border_color: [1.0, 1.0, 1.0, 0.85],
        border_width: 1.0,
        border_radius: dot.width / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn render_karaoke_dot_scaled(
    line: &crate::rythmo_line::RythmoLine,
    line_rect: &Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
    scale: f32,
    quads: &mut Vec<QuadInstance>,
) {
    let Some(progress_info) = progress_info else {
        return;
    };

    let bounce = (progress_info.local_progress * std::f32::consts::PI)
        .sin()
        .max(0.0);
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let x = if line_rect.width > size {
        line_rect.x + progress_info.visual_progress.clamp(0.0, 1.0) * (line_rect.width - size)
    } else {
        line_rect.x + (line_rect.width - size) * 0.5
    };
    let y = line_rect.y + 3.0 * scale.max(0.5)
        - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
    let tint = line_color_tint(line);

    quads.push(QuadInstance {
        rect: [x - 1.5, y - 1.5, size + 3.0, size + 3.0],
        color: [0.0, 0.0, 0.0, 0.35],
        color_bottom: [0.0, 0.0, 0.0, 0.35],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: (size + 3.0) / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [x, y, size, size],
        color: tint,
        color_bottom: tint,
        border_color: [1.0, 1.0, 1.0, 0.85],
        border_width: 1.0,
        border_radius: size / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

/// Render autocomplete dropdown AFTER all lines (so it's on top).
pub fn render_autocomplete<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let line_id = match state.editing_character {
        Some(id) => id,
        None => return,
    };
    let line = match project.get_line(line_id) {
        Some(l) => l,
        None => return,
    };
    let suggestions = project.known_characters();
    if suggestions.is_empty() {
        return;
    }

    let r = line_rect(project, line, current_frame, zone);
    let br = badge_rect_for_line(project, line, current_frame, zone);
    let dropdown_x = br.x;
    let mut dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;
    const VISIBLE_ROWS: usize = 8;
    let visible_rows = suggestions.len().min(VISIBLE_ROWS);
    let max_scroll = suggestions.len().saturating_sub(visible_rows);
    let scroll = state.autocomplete_scroll.min(max_scroll);
    let dropdown_h = visible_rows as f32 * item_h;

    // Background
    quads.push(QuadInstance {
        rect: [dropdown_x, dropdown_y, dropdown_w, dropdown_h],
        color: [0.15, 0.15, 0.17, 0.95],
        color_bottom: [0.12, 0.12, 0.14, 0.95],
        border_color: [0.3, 0.3, 0.36, 0.6],
        border_width: 1.0,
        border_radius: 3.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.4],
        shadow_blur: 6.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });

    for (i, suggestion) in suggestions
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_rows)
    {
        let is_selected = state.autocomplete_index == Some(i);
        let is_hovered = state.autocomplete_hover == Some(i);

        // Highlight
        if is_selected || is_hovered {
            quads.push(QuadInstance {
                rect: [
                    dropdown_x + 2.0,
                    dropdown_y + 1.0,
                    dropdown_w - 4.0,
                    item_h - 2.0,
                ],
                color: [0.18, 0.52, 1.0, if is_selected { 0.75 } else { 0.45 }],
                color_bottom: [0.10, 0.38, 0.86, if is_selected { 0.75 } else { 0.45 }],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Color swatch
        quads.push(QuadInstance {
            rect: [dropdown_x + 4.0, dropdown_y + 4.0, 12.0, item_h - 8.0],
            color: suggestion.color,
            color_bottom: suggestion.color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Name label
        labels.push(LabelInfo {
            text: &suggestion.name,
            bounds: Rect {
                x: dropdown_x + 20.0,
                y: dropdown_y,
                width: dropdown_w - 24.0,
                height: item_h,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 2.0,
            font_size_override: Some(11.0),
            color_override: None,
            font_family_override: None,
        });
        dropdown_y += item_h;
    }

    if max_scroll > 0 {
        let track_x = dropdown_x + dropdown_w - 5.0;
        let thumb_h = (dropdown_h * visible_rows as f32 / suggestions.len() as f32).max(12.0);
        let thumb_y =
            r.y + r.height + 2.0 + (dropdown_h - thumb_h) * scroll as f32 / max_scroll as f32;
        for (y, height, color) in [
            (r.y + r.height + 2.0, dropdown_h, [0.04, 0.07, 0.12, 0.9]),
            (thumb_y, thumb_h, [0.30, 0.62, 1.0, 0.95]),
        ] {
            quads.push(QuadInstance {
                rect: [track_x, y, 3.0, height],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 1.5,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }
}

/// Returns the autocomplete suggestion rect for hit testing
/// Render markers (boucle, out, scene change, liaisons) on the bande rythmo.
pub fn render_markers<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    liaison_icons: &mut Vec<IconInstance>,
    liaison_left_uv: [f32; 4],
    liaison_right_uv: [f32; 4],
) {
    let margin_frames = f64_ceil_to_i64(20.0 / ppf().max(0.001) as f64).saturating_add(1);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames);
    for marker_index in render_index.visible_marker_indices(first_frame, last_frame) {
        let Some(marker) = project.marker(marker_index) else {
            continue;
        };
        let x = frame_to_x(marker.frame, current_frame, zone);
        if x < zone.x - 20.0 || x > zone.x + zone.width + 20.0 {
            continue;
        }

        match &marker.kind {
            MarkerKind::Boucle => {
                let red = [0.85, 0.15, 0.15, 0.9];
                // Red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Big "X" — two smooth rotated bars
                let cy = zone.y + zone.height / 2.0;
                let arm_len = 20.0;
                let thickness = 2.5;
                let pi4 = std::f32::consts::FRAC_PI_4;
                // "\" bar
                quads.push(QuadInstance {
                    rect: [x - arm_len / 2.0, cy - thickness / 2.0, arm_len, thickness],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: pi4,
                    _padding: [0.0; 2],
                });
                // "/" bar
                quads.push(QuadInstance {
                    rect: [x - arm_len / 2.0, cy - thickness / 2.0, arm_len, thickness],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: -pi4,
                    _padding: [0.0; 2],
                });
            }
            MarkerKind::Out => {
                let col = [0.85, 0.45, 0.45, 0.7];
                // Light red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: col,
                    color_bottom: col,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Two parallel oblique bars crossing the vertical bar
                let cy = zone.y + zone.height / 2.0;
                let bar_len = zone.height * 0.25;
                let thickness = 2.0;
                let angle = 0.5; // ~30 degrees
                for offset in &[-5.0_f32, 5.0] {
                    quads.push(QuadInstance {
                        rect: [
                            x + offset - bar_len / 2.0,
                            cy - thickness / 2.0,
                            bar_len,
                            thickness,
                        ],
                        color: col,
                        color_bottom: col,
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 0.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: angle,
                        _padding: [0.0; 2],
                    });
                }
                // "out" text
                labels.push(LabelInfo {
                    text: "out",
                    bounds: Rect {
                        x: x + 12.0,
                        y: cy - 8.0,
                        width: 30.0,
                        height: 16.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(10.0),
                    color_override: Some([220, 120, 120]),
                    font_family_override: None,
                });
            }
            MarkerKind::SceneChange => {
                // White bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: [0.9, 0.9, 0.95, 0.8],
                    color_bottom: [0.9, 0.9, 0.95, 0.8],
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
            MarkerKind::LiaisonLeft => {
                let uv = liaison_left_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
            MarkerKind::LiaisonRight => {
                let uv = liaison_right_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
        }
    }
}

pub fn autocomplete_hit(
    zone: &Rect,
    project: &Project,
    current_frame: f64,
    state: &RythmoState,
    click_x: f32,
    click_y: f32,
) -> Option<(String, [f32; 4])> {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = project.lines().find(|l| l.id == line_id) {
            let br = badge_rect_for_line(project, line, current_frame, zone);
            let lr = line_rect(project, line, current_frame, zone);
            let suggestions = project.known_characters();
            if !suggestions.is_empty() {
                let dropdown_x = br.x;
                let mut dropdown_y = lr.y + lr.height + 2.0;
                let item_h = 20.0;
                let dropdown_w = 140.0;

                const VISIBLE_ROWS: usize = 8;
                let visible_rows = suggestions.len().min(VISIBLE_ROWS);
                let scroll = state
                    .autocomplete_scroll
                    .min(suggestions.len().saturating_sub(visible_rows));
                for suggestion in suggestions.iter().skip(scroll).take(visible_rows) {
                    let item_rect = Rect {
                        x: dropdown_x,
                        y: dropdown_y,
                        width: dropdown_w,
                        height: item_h,
                    };
                    if item_rect.contains(click_x, click_y) {
                        return Some((suggestion.name.clone(), suggestion.color));
                    }
                    dropdown_y += item_h;
                }
            }
        }
    }
    None
}

const MENU_ITEM_H: f32 = 26.0;
const MENU_ROOT_W: f32 = 230.0;
const MENU_ACTOR_W: f32 = 240.0;
const MENU_ACTION_W: f32 = 285.0;
const MENU_GAP: f32 = 0.0;
const MENU_MARGIN: f32 = 8.0;
const MENU_MAX_ACTOR_H: f32 = 260.0;

pub fn handle_context_menu_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
) -> EventResponse {
    match event {
        UiEvent::ContextMenu { x, y } => {
            let line_id = project
                .lines()
                .find(|line| {
                    line_rect(project, line, current_frame, zone).contains(*x, *y)
                        || badge_rect_for_line(project, line, current_frame, zone).contains(*x, *y)
                })
                .map(|line| line.id);
            if let Some(line_id) = line_id {
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
                return EventResponse::Consumed;
            }
            state.context_menu = None;
            EventResponse::Ignored
        }
        UiEvent::MouseMove { x, y } => {
            if state.context_menu.is_none() {
                return EventResponse::Ignored;
            }
            update_context_menu_hover(project, screen_w, screen_h, state, *x, *y);
            EventResponse::Consumed
        }
        UiEvent::Scroll { x, y, delta, .. } => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            let (_, actor_rect, _, _, max_scroll) =
                context_menu_layout(project, screen_w, screen_h, menu);
            if actor_rect.contains(*x, *y) {
                menu.actor_scroll =
                    (menu.actor_scroll - delta * MENU_ITEM_H * 2.0).clamp(0.0, max_scroll);
                return EventResponse::Consumed;
            }
            EventResponse::Consumed
        }
        UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
            if state.context_menu.is_none() {
                return EventResponse::Ignored;
            }
            update_context_menu_hover(project, screen_w, screen_h, state, *x, *y);
            let Some(menu) = state.context_menu.as_ref() else {
                return EventResponse::Ignored;
            };
            let (root_rect, actor_rect, action_rect, actor_scroll, _) =
                context_menu_layout(project, screen_w, screen_h, menu);

            if root_rect.contains(*x, *y) {
                let root_item = ((*y - root_rect.y) / MENU_ITEM_H).floor() as usize;
                if root_item == 1 {
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::OpenLinesPanel);
                }
            }

            if let (Some(actor_index), Some(action_index)) =
                (menu.hover_actor_index, menu.hover_action_index)
            {
                if action_rect.contains(*x, *y) {
                    if let Some(actor) = project.voice_actor(actor_index) {
                        let line_id = menu.line_id;
                        let actor_name = actor.name.clone();
                        state.context_menu = None;
                        return match action_index {
                            0 => EventResponse::Action(UiAction::AssignVoiceActorLine {
                                line_id,
                                actor_name,
                            }),
                            1 => EventResponse::Action(UiAction::AssignVoiceActorCharacter {
                                line_id,
                                actor_name,
                            }),
                            2 => EventResponse::Action(UiAction::UnassignVoiceActorLine {
                                line_id,
                                actor_name,
                            }),
                            3 => EventResponse::Action(UiAction::UnassignVoiceActorCharacter {
                                line_id,
                                actor_name,
                            }),
                            _ => EventResponse::Consumed,
                        };
                    }
                }
            }

            if actor_rect.contains(*x, *y) {
                let item_index =
                    ((*y - actor_rect.y + actor_scroll) / MENU_ITEM_H).floor() as usize;
                if item_index == project.voice_actors().len() {
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::OpenVoiceActorModal);
                }
                return EventResponse::Consumed;
            }

            if root_rect.contains(*x, *y)
                || action_rect.contains(*x, *y)
                || context_menu_bridge_contains(root_rect, actor_rect, action_rect, *x, *y)
            {
                return EventResponse::Consumed;
            }

            state.context_menu = None;
            EventResponse::Consumed
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            state.context_menu = None;
            EventResponse::Consumed
        }
        UiEvent::CursorRight => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            if menu.hover_main {
                menu.hover_main = false;
                menu.hover_actor_index = Some(0);
            } else if menu.hover_actor_index.is_some() {
                menu.hover_action_index = Some(0);
            }
            EventResponse::Consumed
        }
        UiEvent::CursorLeft => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            if menu.hover_action_index.take().is_none() {
                menu.hover_actor_index = None;
                menu.hover_main = true;
                menu.hover_change_character = false;
            }
            EventResponse::Consumed
        }
        UiEvent::CursorUp | UiEvent::CursorDown => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            let direction = if matches!(event, UiEvent::CursorDown) {
                1
            } else {
                -1
            };
            if menu.hover_main && direction > 0 {
                menu.hover_main = false;
                menu.hover_change_character = true;
            } else if menu.hover_change_character && direction < 0 {
                menu.hover_change_character = false;
                menu.hover_main = true;
            } else if let Some(action) = menu.hover_action_index.as_mut() {
                *action = (*action as i32 + direction).rem_euclid(4) as usize;
            } else if !menu.hover_main && !menu.hover_change_character {
                let len = project.voice_actors().len() + 1;
                let current = menu.hover_actor_index.unwrap_or(0);
                menu.hover_actor_index =
                    Some((current as i32 + direction).rem_euclid(len as i32) as usize);
            }
            let label = state.context_menu.as_ref().and_then(|menu| {
                if let Some(action) = menu.hover_action_index {
                    Some(
                        [
                            t("context.voice_actor.assign_line"),
                            t("context.voice_actor.assign_character"),
                            t("context.voice_actor.unassign_line"),
                            t("context.voice_actor.unassign_character"),
                        ][action],
                    )
                } else if menu.hover_change_character {
                    Some(t("context.change_character"))
                } else if let Some(actor_index) = menu.hover_actor_index {
                    if actor_index == project.voice_actors().len() {
                        Some(t("context.voice_actor.create"))
                    } else {
                        project
                            .voice_actor(actor_index)
                            .map(|actor| actor.name.as_str())
                    }
                } else {
                    Some(t("context.voice_actor.assign_to_actor"))
                }
            });
            label.map_or(EventResponse::Consumed, |label| {
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: label.to_string(),
                    },
                ))
            })
        }
        UiEvent::Activate => {
            let Some(menu) = state.context_menu.as_ref() else {
                return EventResponse::Ignored;
            };
            if menu.hover_main {
                let menu = state.context_menu.as_mut().unwrap();
                menu.hover_main = false;
                menu.hover_actor_index = Some(0);
                return EventResponse::Consumed;
            }
            if menu.hover_change_character {
                state.context_menu = None;
                return EventResponse::Action(UiAction::OpenLinesPanel);
            }
            let line_id = menu.line_id;
            let Some(actor_index) = menu.hover_actor_index else {
                return EventResponse::Consumed;
            };
            if actor_index == project.voice_actors().len() {
                state.context_menu = None;
                return EventResponse::Action(UiAction::OpenVoiceActorModal);
            }
            let Some(action_index) = menu.hover_action_index else {
                state.context_menu.as_mut().unwrap().hover_action_index = Some(0);
                return EventResponse::Consumed;
            };
            let actor_name = project
                .voice_actor(actor_index)
                .map(|actor| actor.name.clone());
            state.context_menu = None;
            let Some(actor_name) = actor_name else {
                return EventResponse::Consumed;
            };
            match action_index {
                0 => EventResponse::Action(UiAction::AssignVoiceActorLine {
                    line_id,
                    actor_name,
                }),
                1 => EventResponse::Action(UiAction::AssignVoiceActorCharacter {
                    line_id,
                    actor_name,
                }),
                2 => EventResponse::Action(UiAction::UnassignVoiceActorLine {
                    line_id,
                    actor_name,
                }),
                3 => EventResponse::Action(UiAction::UnassignVoiceActorCharacter {
                    line_id,
                    actor_name,
                }),
                _ => EventResponse::Consumed,
            }
        }
        _ => {
            if state.context_menu.is_some() {
                EventResponse::Consumed
            } else {
                EventResponse::Ignored
            }
        }
    }
}

pub fn render_context_menu<'a>(
    project: &'a Project,
    screen_w: f32,
    screen_h: f32,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let Some(menu) = &state.context_menu else {
        return;
    };
    let (root_rect, actor_rect, action_rect, actor_scroll, max_scroll) =
        context_menu_layout(project, screen_w, screen_h, menu);

    render_menu_panel(quads, root_rect);
    render_menu_item(
        quads,
        labels,
        Rect {
            x: root_rect.x,
            y: root_rect.y,
            width: root_rect.width,
            height: MENU_ITEM_H,
        },
        t("context.voice_actor.assign_to_actor"),
        menu.hover_main,
        true,
    );
    render_menu_item(
        quads,
        labels,
        Rect {
            x: root_rect.x,
            y: root_rect.y + MENU_ITEM_H,
            width: root_rect.width,
            height: MENU_ITEM_H,
        },
        t("context.change_character"),
        menu.hover_change_character,
        false,
    );

    if !context_actor_menu_visible(menu) {
        return;
    }

    render_menu_panel(quads, actor_rect);
    let assigned_names = project
        .get_line(menu.line_id)
        .map(|line| line.voice_actor_names.as_slice())
        .unwrap_or(&[]);
    for (index, actor) in project.voice_actors().iter().enumerate() {
        let y = actor_rect.y + index as f32 * MENU_ITEM_H - actor_scroll;
        if y + MENU_ITEM_H < actor_rect.y || y > actor_rect.y + actor_rect.height {
            continue;
        }
        let item_rect = Rect {
            x: actor_rect.x,
            y,
            width: actor_rect.width,
            height: MENU_ITEM_H,
        };
        let assigned = assigned_names.iter().any(|name| name == &actor.name);
        render_menu_item(
            quads,
            labels,
            item_rect,
            &actor.name,
            menu.hover_actor_index == Some(index) || assigned,
            true,
        );
    }

    let create_index = project.voice_actors().len();
    let create_y = actor_rect.y + create_index as f32 * MENU_ITEM_H - actor_scroll;
    if create_y + MENU_ITEM_H >= actor_rect.y && create_y <= actor_rect.y + actor_rect.height {
        render_menu_separator(quads, actor_rect.x, create_y, actor_rect.width);
        render_menu_item(
            quads,
            labels,
            Rect {
                x: actor_rect.x,
                y: create_y,
                width: actor_rect.width,
                height: MENU_ITEM_H,
            },
            t("context.voice_actor.create"),
            menu.hover_actor_index == Some(create_index),
            false,
        );
    }

    if max_scroll > 0.0 {
        render_menu_scrollbar(quads, actor_rect, actor_scroll, max_scroll);
    }

    if let Some(actor_index) = menu.hover_actor_index {
        if actor_index < project.voice_actors().len() {
            render_menu_panel(quads, action_rect);
            let actions = [
                t("context.voice_actor.assign_line"),
                t("context.voice_actor.assign_character"),
                t("context.voice_actor.unassign_line"),
                t("context.voice_actor.unassign_character"),
            ];
            for (index, label) in actions.iter().enumerate() {
                render_menu_item(
                    quads,
                    labels,
                    Rect {
                        x: action_rect.x,
                        y: action_rect.y + index as f32 * MENU_ITEM_H,
                        width: action_rect.width,
                        height: MENU_ITEM_H,
                    },
                    label,
                    menu.hover_action_index == Some(index),
                    false,
                );
            }
        }
    }
}

fn context_menu_layout(
    project: &Project,
    screen_w: f32,
    screen_h: f32,
    menu: &LineContextMenu,
) -> (Rect, Rect, Rect, f32, f32) {
    let root_h = MENU_ITEM_H * 2.0;
    let (root_x, root_y) =
        clamped_menu_origin(menu.x, menu.y, MENU_ROOT_W, root_h, screen_w, screen_h);
    let root_rect = Rect {
        x: root_x,
        y: root_y,
        width: MENU_ROOT_W,
        height: root_h,
    };

    let actor_items = project.voice_actors().len() + 1;
    let total_actor_h = actor_items as f32 * MENU_ITEM_H;
    let actor_h = total_actor_h
        .min(MENU_MAX_ACTOR_H)
        .min((screen_h - MENU_MARGIN * 2.0).max(MENU_ITEM_H));
    let actor_x_right = root_rect.x + root_rect.width + MENU_GAP;
    let actor_x = if actor_x_right + MENU_ACTOR_W <= screen_w - MENU_MARGIN {
        actor_x_right
    } else {
        (root_rect.x - MENU_ACTOR_W - MENU_GAP).max(MENU_MARGIN)
    };
    let actor_y = root_rect.y.clamp(
        MENU_MARGIN,
        (screen_h - actor_h - MENU_MARGIN).max(MENU_MARGIN),
    );
    let actor_rect = Rect {
        x: actor_x,
        y: actor_y,
        width: MENU_ACTOR_W,
        height: actor_h,
    };
    let max_scroll = (total_actor_h - actor_h).max(0.0);
    let actor_scroll = menu.actor_scroll.clamp(0.0, max_scroll);

    let hovered_actor_y = menu
        .hover_actor_index
        .map(|index| actor_rect.y + index as f32 * MENU_ITEM_H - actor_scroll)
        .unwrap_or(actor_rect.y)
        .clamp(
            MENU_MARGIN,
            (screen_h - MENU_ITEM_H * 4.0 - MENU_MARGIN).max(MENU_MARGIN),
        );
    let action_x_right = actor_rect.x + actor_rect.width + MENU_GAP;
    let action_x = if action_x_right + MENU_ACTION_W <= screen_w - MENU_MARGIN {
        action_x_right
    } else {
        (actor_rect.x - MENU_ACTION_W - MENU_GAP).max(MENU_MARGIN)
    };
    let action_rect = Rect {
        x: action_x,
        y: hovered_actor_y,
        width: MENU_ACTION_W,
        height: MENU_ITEM_H * 4.0,
    };

    (root_rect, actor_rect, action_rect, actor_scroll, max_scroll)
}

fn bridge_rect(a: Rect, b: Rect) -> Rect {
    let a_right = a.x + a.width;
    let b_right = b.x + b.width;
    let (x, width) = if a_right <= b.x {
        (a_right, b.x - a_right)
    } else if b_right <= a.x {
        (b_right, a.x - b_right)
    } else {
        (a.x.max(b.x), 0.0)
    };
    let y = a.y.min(b.y);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect {
        x,
        y,
        width,
        height: bottom - y,
    }
}

fn context_menu_bridge_contains(
    root_rect: Rect,
    actor_rect: Rect,
    action_rect: Rect,
    x: f32,
    y: f32,
) -> bool {
    bridge_rect(root_rect, actor_rect).contains(x, y)
        || bridge_rect(actor_rect, action_rect).contains(x, y)
}

fn update_context_menu_hover(
    project: &Project,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) {
    let Some(menu) = state.context_menu.as_mut() else {
        return;
    };
    let (root_rect, actor_rect, action_rect, actor_scroll, _) =
        context_menu_layout(project, screen_w, screen_h, menu);

    let root_hover = root_rect.contains(x, y);
    let mut actor_hover = None;
    let mut action_hover = None;
    let root_actor_bridge = bridge_rect(root_rect, actor_rect).contains(x, y);
    let actor_action_bridge =
        menu.hover_actor_index.is_some() && bridge_rect(actor_rect, action_rect).contains(x, y);

    if actor_rect.contains(x, y) {
        let index = ((y - actor_rect.y + actor_scroll) / MENU_ITEM_H).floor() as usize;
        if index <= project.voice_actors().len() {
            actor_hover = Some(index);
        }
    }

    if action_rect.contains(x, y) {
        let index = ((y - action_rect.y) / MENU_ITEM_H).floor() as usize;
        if index < 4 {
            action_hover = Some(index);
            actor_hover = menu.hover_actor_index;
        }
    }

    if actor_action_bridge {
        actor_hover = menu.hover_actor_index;
    }

    let root_item = if root_hover {
        Some(((y - root_rect.y) / MENU_ITEM_H).floor() as usize)
    } else {
        None
    };
    menu.hover_main = root_actor_bridge || root_item == Some(0);
    menu.hover_change_character = root_item == Some(1);
    menu.hover_actor_index = actor_hover;
    menu.hover_action_index = action_hover;
}

fn context_actor_menu_visible(menu: &LineContextMenu) -> bool {
    menu.hover_main || menu.hover_actor_index.is_some() || menu.hover_action_index.is_some()
}

fn clamped_menu_origin(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    screen_w: f32,
    screen_h: f32,
) -> (f32, f32) {
    context_menu::clamped_origin(x, y, width, height, screen_w, screen_h)
}

fn render_menu_panel(quads: &mut Vec<QuadInstance>, rect: Rect) {
    context_menu::render_panel(quads, rect);
}

fn render_menu_item<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    hovered: bool,
    arrow: bool,
) {
    context_menu::render_item(quads, labels, rect, text, hovered, arrow, 12.0);
}

fn render_menu_separator(quads: &mut Vec<QuadInstance>, x: f32, y: f32, width: f32) {
    quads.push(QuadInstance {
        rect: [x + 8.0, y, width - 16.0, 1.0],
        color: [0.42, 0.42, 0.50, 0.55],
        color_bottom: [0.42, 0.42, 0.50, 0.55],
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

fn render_menu_scrollbar(quads: &mut Vec<QuadInstance>, rect: Rect, scroll: f32, max_scroll: f32) {
    let track_h = rect.height - 10.0;
    let thumb_h = (track_h * (rect.height / (rect.height + max_scroll))).clamp(24.0, track_h);
    let thumb_y = rect.y + 5.0 + (track_h - thumb_h) * (scroll / max_scroll.max(1.0));
    quads.push(QuadInstance {
        rect: [rect.x + rect.width - 6.0, thumb_y, 3.0, thumb_h],
        color: [0.70, 0.70, 0.78, 0.45],
        color_bottom: [0.70, 0.70, 0.78, 0.45],
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

fn autocomplete_hover_index(ctx: &RythmoCtx, state: &RythmoState, x: f32, y: f32) -> Option<usize> {
    let line_id = state.editing_character?;
    let line = ctx.project.get_line(line_id)?;
    let suggestions = ctx.project.known_characters();
    if suggestions.is_empty() {
        return None;
    }

    let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
    let br = badge_rect_for_line(ctx.project, line, ctx.current_frame, ctx.zone);
    let dropdown_x = br.x;
    let dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;

    const VISIBLE_ROWS: usize = 8;
    let visible_rows = suggestions.len().min(VISIBLE_ROWS);
    let scroll = state
        .autocomplete_scroll
        .min(suggestions.len().saturating_sub(visible_rows));
    for (i, _) in suggestions
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_rows)
    {
        let iy = dropdown_y + (i - scroll) as f32 * item_h;
        let item_rect = Rect {
            x: dropdown_x,
            y: iy,
            width: dropdown_w,
            height: item_h,
        };
        if item_rect.contains(x, y) {
            return Some(i);
        }
    }
    None
}
