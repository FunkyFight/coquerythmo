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
use crate::rendering::rythmo::placement::{self, KaraokeRowPriority};
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
use unicode_segmentation::UnicodeSegmentation;

const PLAYHEAD_WIDTH: f32 = 3.0;
const PLAYHEAD_COLOR: [f32; 4] = [1.0, 0.02, 0.05, 1.0];

const HANDLE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
const LINE_BORDER: [f32; 4] = [0.5, 0.5, 0.55, 0.3];
const LINE_BORDER_HOVER: [f32; 4] = [0.6, 0.6, 0.65, 0.5];
const LINE_RADIUS: f32 = 2.0;
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];
const KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS: f64 = 60.0;
const KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME: usize = 32;
const KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME: usize = 2;

fn character_badge_collision_layout(
    line_id: u64,
    character_name: &str,
    badge_rect: &Rect,
    line_x: f32,
    other_lines: &[(u64, Rect, &str)],
) -> (bool, Rect, f32) {
    let collides = |candidate: &Rect| {
        other_lines.iter().any(|(other_id, other_rect, _)| {
            *other_id != line_id && rects_overlap(candidate, other_rect)
        })
    };
    for (other_id, other_rect, other_character_name) in other_lines {
        if *other_id == line_id || !rects_overlap(badge_rect, other_rect) {
            continue;
        }
        if *other_character_name == character_name {
            return (true, *badge_rect, 1.0);
        }
    }
    if !collides(badge_rect) {
        return (false, *badge_rect, 1.0);
    }

    let mut fitted = *badge_rect;
    fitted.x = line_x - BADGE_GAP - fitted.width;
    if !collides(&fitted) {
        return (false, fitted, 1.0);
    }

    let top = fitted.y;
    let base_width = fitted.width;
    let base_height = fitted.height;
    for step in 1..=95 {
        let scale = 1.0 - step as f32 * 0.01;
        fitted.width = base_width * scale;
        fitted.height = base_height * scale;
        fitted.x = line_x - BADGE_GAP - fitted.width;
        fitted.y = top;
        if !collides(&fitted) {
            return (false, fitted, scale);
        }
    }
    (false, fitted, 0.05)
}

#[path = "detection_ui.rs"]
mod detection_ui;
pub(crate) use detection_ui::*;
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

    #[test]
    fn active_karaoke_candidate_beats_future_preview() {
        assert!(placement::karaoke_row_candidate_wins(
            KaraokeRowPriority {
                active: true,
                start_frame: 0,
                line_id: 1
            },
            KaraokeRowPriority {
                active: false,
                start_frame: 24,
                line_id: 2
            }
        ));
        assert!(!placement::karaoke_row_candidate_wins(
            KaraokeRowPriority {
                active: false,
                start_frame: 24,
                line_id: 2
            },
            KaraokeRowPriority {
                active: true,
                start_frame: 0,
                line_id: 1
            }
        ));
    }

    #[test]
    fn nearest_future_karaoke_candidate_wins() {
        assert!(placement::karaoke_row_candidate_wins(
            KaraokeRowPriority {
                active: false,
                start_frame: 12,
                line_id: 1
            },
            KaraokeRowPriority {
                active: false,
                start_frame: 24,
                line_id: 2
            }
        ));
        assert!(!placement::karaoke_row_candidate_wins(
            KaraokeRowPriority {
                active: false,
                start_frame: 24,
                line_id: 2
            },
            KaraokeRowPriority {
                active: false,
                start_frame: 12,
                line_id: 1
            }
        ));
    }

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
    fn ctrl_syllable_drag_cannot_cross_synchronization_interval_edges() {
        assert!(separator_is_inside_edit_range(2, Some((1, 4))));
        assert!(!separator_is_inside_edit_range(0, Some((1, 4))));
        assert!(!separator_is_inside_edit_range(3, Some((1, 4))));
        assert!(!separator_is_inside_edit_range(2, Some((3, 3))));
        assert!(separator_is_inside_edit_range(3, None));
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
        let normal_rect = line_rect(
            &project,
            project.get_line(normal_id).unwrap(),
            0.0,
            &zone,
            0.0,
            24.0,
        );
        let karaoke_body = editor_track_body_rect_at_frame(&project, 0.5, 24.0, &zone);
        let karaoke_rect = karaoke_preview_line_rect(
            &project,
            project.get_line(karaoke_id).unwrap(),
            24.0,
            &zone,
            karaoke_adjacent_max_gap_frames(24.0),
            0.0,
            24.0,
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
    fn text_emotion_copy_uses_space_outside_the_line_hitbox() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        let line = project.get_line_mut(line_id).unwrap();
        line.text = "Bonjour".into();
        line.set_text_emotion(0, 7, Some(crate::rythmo_line::TextEmotion::Wave));
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };

        let layout = EditorLayoutCtx::new_at_frame(&project, 0.0, &zone);
        let line = project.get_line(line_id).unwrap();
        let hitbox = layout.line_rect_with_karaoke_width(line, 0.0, &zone, false, None, 0.0, 24.0);
        let track = layout.track_body_rect(line.y_slot, &zone);
        let badge = layout.badge_rect_for_name(line, "A", hitbox.x, &zone, 0.0, 24.0);
        let (copy_y, copy_height) =
            rythmo_layout::text_emotion_copy_rect(hitbox.y, hitbox.height, 1.0);

        assert!(copy_y >= hitbox.y + hitbox.height);
        assert!(copy_y + copy_height <= track.y + track.height + 0.01);
        assert!(track.height > hitbox.height);
        assert_eq!(badge.height, hitbox.height);
    }

    #[test]
    fn caret_hit_test_uses_the_rendered_character_widths() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        project.get_line_mut(line_id).unwrap().text = "cataclysme".into();
        let line = project.get_line(line_id).unwrap();
        let state = RythmoState::new();
        let ratios = crate::rythmo_line::text_emotion_char_ratios(
            &line.text,
            crate::config::get().ui.font_size * 2.0,
        )
        .unwrap();

        for (expected, ratio) in ratios.into_iter().enumerate() {
            assert_eq!(
                cursor_index_for_line_at_ratio(&project, line, None, "fr", false, &state, ratio,),
                expected
            );
        }
    }

    #[test]
    fn first_text_emotion_reflows_the_cached_layout_inside_the_br() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        project.get_line_mut(line_id).unwrap().text = "Bonjour".into();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };
        let mut state = RythmoState::new();
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);

        let before = state
            .get_or_create_layout_ctx(&project, &render_index, 0.0, 24.0, &zone)
            .normal_body_h;
        project.get_line_mut(line_id).unwrap().set_text_emotion(
            0,
            7,
            Some(crate::rythmo_line::TextEmotion::Wave),
        );
        render_index.refresh(&project);
        let layout = state.get_or_create_layout_ctx(&project, &render_index, 0.0, 24.0, &zone);

        assert!(layout.normal_body_h < before);
        let bottom = layout
            .track_layouts()
            .last()
            .map(|track| track.top + track.reserved_h)
            .unwrap_or(0.0);
        assert!(bottom <= zone.height - constants::RULER_HEIGHT + 0.01);
        let line = project.get_line(line_id).unwrap();
        let hitbox = layout.line_rect_with_karaoke_width(line, 0.0, &zone, false, None, 0.0, 24.0);
        drop(layout);

        let response = handle_rythmo_event(
            &UiEvent::MouseMove {
                x: hitbox.x + hitbox.width / 2.0,
                y: hitbox.y + hitbox.height / 2.0,
            },
            &zone,
            &project,
            &render_index,
            0.0,
            false,
            24.0,
            &mut state,
            ToolMode::Select,
            [1.0; 4],
            0.012,
            false,
            RythmoInteractionMode::Editable,
        );
        assert_eq!(response, EventResponse::Consumed);
        assert_eq!(state.hovered_line, Some(line_id));
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
    fn character_badge_prewarm_matches_visible_raster_inputs() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        project.get_line_mut(line_id).unwrap().character_name = "Alice".into();
        let line = project.get_line(line_id).unwrap();
        let rect = Rect {
            x: 100.0,
            y: 20.0,
            width: 60.0,
            height: 18.0,
        };

        let prewarm = character_badge_text(line, rect, 0.8, true);
        let visible = character_badge_text(line, rect, 0.8, false);

        assert!(prewarm.prewarm);
        assert!(!visible.prewarm);
        assert!(prewarm.emphasized);
        assert_eq!(prewarm.line_id, visible.line_id);
        assert_eq!(prewarm.text, visible.text);
        assert_eq!(prewarm.dest_rect, visible.dest_rect);
        assert_eq!(prewarm.font_scale, visible.font_scale);
        assert_eq!(prewarm.stretch, visible.stretch);
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
            0.0,
            24.0,
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

        let (hidden, fitted, scale) = character_badge_collision_layout(
            1,
            "Alice",
            &badge,
            200.0,
            &[(2, colliding_line, "Bob")],
        );
        assert!(!hidden);
        assert!(scale < 1.0);
        assert_eq!(fitted.y, badge.y);
        assert!(!rects_overlap(&fitted, &colliding_line));

        let (hidden, _, _) = character_badge_collision_layout(
            1,
            "Alice",
            &badge,
            200.0,
            &[(2, colliding_line, "Alice")],
        );
        assert!(hidden);
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

        let (hidden, _, _) =
            character_badge_collision_layout(1, "Alice", &badge, 200.0, &other_lines);
        assert!(hidden);
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
                0.0,
                24.0,
            ),
            line_rect(
                &project,
                project.get_line(normal_id).unwrap(),
                0.0,
                &zone,
                0.0,
                24.0,
            ),
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
                0.0,
                24.0,
            ),
            karaoke_preview_line_rect(&project, karaoke, 24.0, &zone, max_gap_frames, 0.0, 24.0),
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

        let whole_frame_x = frame_to_x(100, 100.0, &zone, 24.0);
        let half_frame_x = frame_to_x(100, 100.5, &zone, 24.0);

        assert!((half_frame_x - (whole_frame_x - ppf() * 0.5)).abs() < 0.01);
        assert_eq!(x_to_frame(half_frame_x, 100.5, &zone, 24.0), 100);
        assert_eq!(
            x_to_frame(frame_to_x(101, 100.5, &zone, 24.0), 100.5, &zone, 24.0),
            101
        );
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
            let (_, editor_width) =
                line_visual_x_width(line, current_frame, &zone, false, 0.0, 24.0);
            let (_, playback_width) =
                line_visual_x_width(line, current_frame, &zone, true, 0.0, 24.0);

            assert_eq!(editor_width.to_bits(), expected_width.to_bits());
            assert_eq!(playback_width.to_bits(), expected_width.to_bits());
            assert_eq!(editor_width.ceil() as u32, expected_width.ceil() as u32);
            assert_eq!(playback_width.ceil() as u32, expected_width.ceil() as u32);
        }
    }

    #[test]
    fn pointer_hover_finds_line_through_render_index() {
        crate::config::init();
        crate::config::set_reading_bar_offset_seconds(0.0);
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
            0.0,
            24.0,
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
            0.0,
            24.0,
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
        assert_eq!(state.selected, None);
        assert_eq!(state.hovered_line, None);
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
    fn read_only_render_hides_authoring_chrome() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(40, 20, 0.0);
        project.get_line_mut(line_id).unwrap().text = "Bonjour monde".into();
        let mut detections = crate::detection::DetectionDocument::default();
        detections
            .add_sync_point(
                line_id,
                13,
                crate::detection::MediaTick::from_frame(40),
                crate::detection::MediaTick::from_frame(60),
                7,
                crate::detection::MediaTick::from_frame(50),
            )
            .unwrap();
        project.restore_line_detections(line_id, detections.line(line_id).unwrap().clone());
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);
        let mut state = RythmoState::new();
        state.hovered_line = Some(line_id);
        state.selected = Some(Selection::Line(line_id));
        let lint = HashMap::from([(line_id, crate::lint::Severity::Error)]);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let mut quads = Vec::new();
        let mut syllables = Vec::new();
        let mut labels = Vec::new();
        let mut stretched = Vec::new();
        let mut notes = Vec::new();
        let mut actors = Vec::new();

        render_lines(
            &zone,
            &project,
            &render_index,
            50.0,
            false,
            false,
            24.0,
            &state,
            &lint,
            &mut quads,
            &mut syllables,
            &mut labels,
            &mut stretched,
            &mut notes,
            &mut actors,
            [0.0; 4],
            [[0.0; 4]; 18],
        );

        assert!(!stretched.is_empty());
        assert!(quads.iter().all(|quad| {
            quad.border_width == 0.0
                && quad.color != HANDLE_COLOR
                && quad.color != [0.95, 0.18, 0.18, 0.98]
                && quad.color != [0.48, 0.72, 1.0, 0.96]
        }));
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
        let quads = render_rythmo_base(
            &zone,
            &project,
            &render_index,
            current_frame as f64,
            &waveform,
            waveform_offset_frames,
            true,
            false,
            24.0,
            &state,
        );

        assert!(quads.iter().any(|quad| {
            quad.color == [0.30, 0.90, 0.45, 0.85]
                && quad.rect[0] >= zone.x
                && quad.rect[0] <= zone.x + zone.width
                && (quad.rect[3] - constants::RULER_HEIGHT).abs() < 0.01
        }));
    }

    #[test]
    fn hidden_voice_actor_menu_cannot_open_the_create_actor_action() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        let mut state = RythmoState::new();
        state.context_menu = Some(LineContextMenu {
            line_id,
            x: 100.0,
            y: 100.0,
            hover_main: false,
            hover_change_character: false,
            hover_text_emotion: true,
            hover_generate_detection: false,
            hover_emotion_index: None,
            hover_emotion_variant: None,
            text_range: None,
            hover_actor_index: None,
            hover_action_index: None,
            actor_scroll: 0.0,
        });
        let (_, actor_rect, _, _, _, _) = context_menu_layout(
            &project,
            1200.0,
            800.0,
            state.context_menu.as_ref().unwrap(),
        );
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
        };

        let response = handle_context_menu_event(
            &UiEvent::MousePress {
                x: actor_rect.x + actor_rect.width / 2.0,
                y: actor_rect.y + actor_rect.height / 2.0,
            },
            &project,
            0.0,
            &zone,
            1200.0,
            800.0,
            24.0,
            &mut state,
        );

        assert_eq!(response, EventResponse::Consumed);
    }

    #[test]
    fn context_menu_keyboard_navigation_announces_every_level() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        let mut state = RythmoState::new();
        state.context_menu = Some(LineContextMenu {
            line_id,
            x: 100.0,
            y: 100.0,
            hover_main: false,
            hover_change_character: false,
            hover_text_emotion: true,
            hover_generate_detection: false,
            hover_emotion_index: Some(0),
            hover_emotion_variant: None,
            text_range: None,
            hover_actor_index: None,
            hover_action_index: None,
            actor_scroll: 0.0,
        });
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
        };

        let response = handle_context_menu_event(
            &UiEvent::CursorUp,
            &project,
            0.0,
            &zone,
            1200.0,
            800.0,
            24.0,
            &mut state,
        );

        assert!(matches!(
            response,
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection { .. }
            ))
        ));
        assert_eq!(
            state.context_menu.as_ref().unwrap().hover_emotion_index,
            Some(EMOTION_CATEGORIES.len())
        );
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
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    playhead_x: f32,
) -> Vec<(f32, f32)> {
    if !karaoke_preview {
        return Vec::new();
    }

    let layout_ctx =
        state.get_or_create_layout_ctx(project, render_index, current_frame, fps, zone);
    let karaoke_index =
        state.cached_karaoke_ui_index(project, karaoke_adjacent_max_gap_frames(fps));
    let frame = visual_frame_to_i64(current_frame);
    render_index
        .visible_line_ids(project, frame, frame)
        .into_iter()
        .filter_map(|line_id| project.get_line(line_id))
        .filter(|line| line.karaoke_active(current_frame))
        .filter_map(|line| {
            let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
            let rect = karaoke_stack_rect(
                Rect {
                    x: body_rect.x,
                    y: body_rect.y,
                    width: body_rect.width,
                    height: body_rect.height,
                },
                karaoke_index.stack_row(line),
                1.0,
            );

            let karaoke_width = karaoke_ui_text_width(&line.text);
            let center_x = zone.x + zone.width / 2.0;
            let karaoke_left = center_x - karaoke_width / 2.0;
            let karaoke_right = center_x + karaoke_width / 2.0;

            if playhead_x + PLAYHEAD_WIDTH > karaoke_left && playhead_x < karaoke_right {
                Some((rect.y, rect.y + rect.height))
            } else {
                None
            }
        })
        .collect()
}

pub fn render_rythmo_base(
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    waveform: &[f32],
    waveform_offset_frames: i64,
    waveform_is_instrumental: bool,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
) -> Vec<QuadInstance> {
    crate::config::set_project_view_settings(
        project.settings().scroll_speed,
        project.settings().reading_bar_offset_percent,
        zone.width,
        fps,
    );
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
        let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
        let first_frame = f64_floor_to_i64(current_frame - half_visible_frames + offset_frames);
        let last_frame = f64_ceil_to_i64(current_frame + half_visible_frames + offset_frames);
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
            let x = frame_to_x(frame, current_frame, zone, fps) + sub_offset * sub_ppf;
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

    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    let playhead_x = zone.x + (zone.width - PLAYHEAD_WIDTH) / 2.0 - offset_frames as f32 * ppf();
    let skip_ranges = active_karaoke_skip_ranges(
        project,
        render_index,
        current_frame,
        zone,
        karaoke_preview,
        fps,
        state,
        playhead_x,
    );
    push_playhead_segments(
        &mut quads,
        playhead_x,
        PLAYHEAD_WIDTH,
        zone.y,
        zone.height,
        PLAYHEAD_COLOR,
        [0.0; 4],
        0.0,
        &skip_ranges,
    );

    quads
}

/// Returns optional (line_id, cursor_pos, text_x, text_w, rect_y, rect_h) for cursor rendering.
const BADGE_HEIGHT: f32 = 13.0;
const BADGE_PADDING_H: f32 = 8.0;
const BADGE_GAP: f32 = 2.0;
const BADGE_MIN_W: f32 = 24.0;
const AMBIANCE_LIAISON_SIZE: f32 = 46.0;
const AMBIANCE_LIAISON_GAP: f32 = 8.0;

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

pub(crate) fn ambiance_description_rect(
    rect: Rect,
    kind: crate::rythmo_line::RythmoLineKind,
) -> Rect {
    if !kind.is_ambiance() {
        return rect;
    }
    let reserve = (AMBIANCE_LIAISON_SIZE + AMBIANCE_LIAISON_GAP).min(rect.width);
    if matches!(kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
        Rect {
            x: rect.x + reserve,
            width: (rect.width - reserve).max(1.0),
            ..rect
        }
    } else {
        Rect {
            width: (rect.width - reserve).max(1.0),
            ..rect
        }
    }
}

fn render_ambiance_name_cursor(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    value: &str,
    input: &crate::ui::text_input::TextInputState,
    focused: bool,
) {
    render_emphasized_label_cursor(
        quads,
        rect,
        crate::rythmo_line::AMBIANCE_LABEL_PREFIX,
        value,
        input,
        focused,
        1.0,
    );
}

fn render_character_name_cursor(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    value: &str,
    input: &crate::ui::text_input::TextInputState,
    focused: bool,
    font_scale: f32,
) {
    render_emphasized_label_cursor(quads, rect, "", value, input, focused, font_scale);
}

/// Use the exact left anchor and font size of the emphasized rythmo renderer.
/// Generic UI text inputs are centered and use approximate advances, which is
/// visibly wrong for these natural-width italic labels.
fn render_emphasized_label_cursor(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    prefix: &str,
    value: &str,
    input: &crate::ui::text_input::TextInputState,
    focused: bool,
    font_scale: f32,
) {
    if !focused {
        return;
    }
    let font_size = crate::config::get().ui.font_size * 2.0 * font_scale.max(0.1);
    let text_inset = font_size * 0.25;
    let x_for = |pos: usize| {
        let suffix: String = value.chars().take(pos.min(value.chars().count())).collect();
        let displayed = format!("{prefix}{suffix}");
        rect.x
            + text_inset
            + crate::vector_text::measure_rythmo_text_width_standalone(&displayed, font_size)
                .unwrap_or_else(|| text_input::text_width(&displayed, font_size))
    };
    if let Some((start, end)) = input.selection_range() {
        let left = x_for(start)
            .min(x_for(end))
            .clamp(rect.x, rect.x + rect.width);
        let right = x_for(start)
            .max(x_for(end))
            .clamp(rect.x, rect.x + rect.width);
        if right - left > 1.0 {
            quads.push(QuadInstance {
                rect: [left, rect.y + 3.0, right - left, rect.height - 6.0],
                color: [0.25, 0.45, 0.95, 0.32],
                color_bottom: [0.25, 0.45, 0.95, 0.32],
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
    }
    if input.cursor_visible() {
        quads.push(QuadInstance {
            rect: [
                x_for(input.cursor_pos).clamp(rect.x, rect.x + rect.width),
                rect.y + 3.0,
                1.5,
                rect.height - 6.0,
            ],
            color: CURSOR_COLOR,
            color_bottom: CURSOR_COLOR,
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

fn emotion_grapheme_cache_id(line_id: u64, grapheme_index: usize, static_copy: bool) -> u64 {
    (1_u64 << 60)
        ^ line_id.rotate_left(17)
        ^ ((grapheme_index as u64) << 1)
        ^ u64::from(static_copy)
}

fn push_emotional_text(
    stretched: &mut Vec<StretchedText>,
    line: &crate::rythmo_line::RythmoLine,
    rect: Rect,
    seconds: f32,
    base_tint: [f32; 4],
    character_positions: Option<&[f32]>,
    show_lane: bool,
) -> Vec<CursorSegmentInfo> {
    let graphemes: Vec<&str> = line.text.graphemes(true).collect();
    let char_count = line.text.chars().count().max(1);
    let ratios = if character_positions.is_some() {
        None
    } else {
        crate::rythmo_line::text_emotion_char_ratios(
            &line.text,
            crate::config::get().ui.font_size * 2.0,
        )
        .filter(|ratios| ratios.len() == char_count + 1)
    };
    let mut char_start = 0usize;
    let mut segments = Vec::with_capacity(graphemes.len());
    for (index, grapheme) in graphemes.iter().enumerate() {
        let char_end = char_start + grapheme.chars().count();
        let start_ratio = character_positions
            .and_then(|positions| positions.get(char_start).copied())
            .or_else(|| ratios.as_ref().map(|ratios| ratios[char_start]))
            .unwrap_or(char_start as f32 / char_count as f32);
        let end_ratio = character_positions
            .and_then(|positions| positions.get(char_end).copied())
            .or_else(|| ratios.as_ref().map(|ratios| ratios[char_end]))
            .unwrap_or(char_end as f32 / char_count as f32);
        let width_ratio = (end_ratio - start_ratio).max(0.001);
        let glyph_rect = Rect {
            x: rect.x + start_ratio * rect.width,
            y: rect.y,
            width: width_ratio * rect.width,
            height: rect.height,
        };
        let cache_id = emotion_grapheme_cache_id(line.id, index, false);
        let mut text = StretchedText::new(cache_id, (*grapheme).to_string(), glyph_rect);
        text.tint = base_tint;
        if let Some(emotion) = line.emotion_at_char(char_start) {
            let animation = crate::rythmo_line::text_emotion_transform(
                emotion,
                index,
                graphemes.len(),
                seconds,
            );
            text.draw_rect.x += animation.offset[0];
            text.draw_rect.y += animation.offset[1] - rect.height * 0.08;
            text.tint = if emotion == crate::rythmo_line::TextEmotion::Yay {
                animation.tint
            } else {
                [
                    base_tint[0] * animation.tint[0],
                    base_tint[1] * animation.tint[1],
                    base_tint[2] * animation.tint[2],
                    base_tint[3] * animation.tint[3],
                ]
            };
            text.transform = animation.transform;
            stretched.push(text);

            if show_lane {
                let mut readable = StretchedText::new(
                    emotion_grapheme_cache_id(line.id, index, true),
                    (*grapheme).to_string(),
                    {
                        let (y, height) =
                            rythmo_layout::text_emotion_copy_rect(rect.y, rect.height, 1.0);
                        Rect {
                            y,
                            height,
                            ..glyph_rect
                        }
                    },
                );
                readable.font_scale = 0.68;
                readable.tint = [base_tint[0], base_tint[1], base_tint[2], 0.82];
                stretched.push(readable);
            }
        } else {
            stretched.push(text);
        }
        segments.push(CursorSegmentInfo {
            cache_id,
            start_char: char_start,
            end_char: char_end,
            start_ratio,
            width_ratio,
        });
        char_start = char_end;
    }
    segments
}

fn karaoke_text_cache_id(line_id: u64) -> u64 {
    (1_u64 << 62) ^ line_id
}

fn character_badge_text(
    line: &crate::rythmo_line::RythmoLine,
    rect: Rect,
    font_scale: f32,
    prewarm: bool,
) -> StretchedText {
    let mut label = StretchedText::natural(
        line.id ^ 0x4348_4152_4143_5445,
        line.character_name.clone(),
        rect,
        font_scale,
        line.character_color,
    );
    label.prewarm = prewarm;
    label.emphasized = true;
    label
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
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    let (x1, width) = if line.karaoke_active(current_frame) || count_in || upcoming_stack {
        let width = centered_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        karaoke_centered_x_width_with_width(zone, width)
    } else {
        geometry::line_visual_x_width(
            line,
            current_frame,
            zone,
            true,
            reading_bar_offset_seconds,
            fps,
        )
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
    reading_bar_offset_seconds: f64,
    fps: f64,
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
        reading_bar_offset_seconds,
        fps,
    )
}

fn badge_rect_for_karaoke_rect(line: &crate::rythmo_line::RythmoLine, line_rect: &Rect) -> Rect {
    let width = badge_width(&line.character_name);
    let badge_h = line_rect.height;
    // During karaoke playback the label belongs to the centered preview row,
    // so keep it visually attached instead of applying the editing lead-in.
    Rect {
        x: line_rect.x - width - BADGE_GAP,
        y: line_rect.y,
        width,
        height: badge_h,
    }
}

fn label_underline_span(rect: Rect, text: &str, font_scale: f32) -> (f32, f32) {
    let font_size = crate::config::get().ui.font_size * 2.0 * font_scale;
    let left_bearing_space = font_size * 0.25;
    let ink_width = crate::vector_text::measure_rythmo_text_width_standalone(text, font_size)
        .unwrap_or_else(|| text_input::text_width(text, font_size));
    let x = rect.x + left_bearing_space;
    (x, ink_width.min((rect.x + rect.width - x).max(0.0)))
}

fn visible_syllable_segments(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    _karaoke_preview: bool,
    state: &RythmoState,
) -> Option<(Vec<usize>, Vec<f32>)> {
    if line.kind.is_ambiance() || line.text.is_empty() || line.text == "↑" || line.text == "↓" {
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
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
    x_ratio: f32,
) -> usize {
    if let Some(idx) =
        detection_ui::sync_cursor_index_for_line_at_ratio(project, line, drag, lang, state, x_ratio)
            .or_else(|| {
                segmented_cursor_index_for_line_at_ratio(
                    line,
                    drag,
                    lang,
                    karaoke_preview,
                    state,
                    x_ratio,
                )
            })
    {
        idx
    } else {
        crate::rythmo_line::text_emotion_char_ratios(
            &line.text,
            crate::config::get().ui.font_size * 2.0,
        )
        .and_then(|ratios| closest_cursor_index_from_ratios(&ratios, x_ratio))
        .unwrap_or_else(|| (x_ratio * line.text.chars().count() as f32).round() as usize)
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
    editable: bool,
    fps: f64,
    state: &RythmoState,
    lint_severities: &HashMap<u64, crate::lint::Severity>,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
    detection_uvs: [[f32; 4]; 18],
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
    state.update_text_emotion_presence(render_index);
    if let Some(drag) = editable
        .then_some(())
        .and_then(|_| state.dragging.as_ref())
        .filter(|drag| {
            drag.handle == DragHandle::VerticalOnly && matches!(drag.target, DragTarget::Line(_))
        })
    {
        if let DragTarget::Line(line_id) = drag.target {
            if project.get_line(line_id).is_some() {
                let guide_x = frame_to_x(drag.original_frame, current_frame, zone, fps);
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
    let layout_ctx =
        state.get_or_create_layout_ctx(project, render_index, current_frame, fps, zone);

    // Rend le highlight de la track survolée (s'il y en a une et qu'elle est valide)
    if let Some(track_idx) = editable.then_some(()).and(state.hovered_track) {
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
    let karaoke_lang = project.syllable_language_code();
    let margin_frames = interactive_render_margin_frames(fps, render_index);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames, fps);
    // A badge is rendered before its line body, so its pixels can enter from
    // the right while the body's start frame is still just outside the query.
    let max_leading_visual_span = state.max_leading_visual_span(project);
    let last_frame = last_frame.saturating_add(f64_ceil_to_i64(
        max_leading_visual_span as f64 / ppf().max(0.001) as f64,
    ));
    let mut visible_line_ids = render_index.visible_line_ids(project, first_frame, last_frame);
    visible_line_ids.sort_by_key(|id| render_index.line_order_index(*id));

    // Precompute line data ONCE - rect, karaoke flags, badge rect, character name
    #[derive(Clone, Copy)]
    struct LineRenderData {
        rect: Rect,
        badge_rect: Rect,
        show_badge: bool,
        karaoke_playback: bool,
        karaoke_count_in: bool,
        karaoke_progress_info: Option<KaraokeProgressRenderInfo>,
        karaoke_row_key: Option<(usize, usize)>,
        karaoke_priority: (bool, i64, u64),
    }

    let mut line_data: Vec<(u64, LineRenderData)> = Vec::with_capacity(visible_line_ids.len());
    let mut badge_prewarm_candidates = Vec::new();
    let mut collision_line_rects: HashMap<u64, Rect> = HashMap::new();
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
        let karaoke_upcoming_stack = karaoke_playback
            && karaoke_count_in
            && karaoke_index.upcoming_stack_visible(line, current_frame);

        if karaoke_playback && !karaoke_active && !karaoke_count_in && !karaoke_upcoming_stack {
            continue;
        }

        let karaoke_stack_row = if karaoke_playback {
            karaoke_index.stack_row(line)
        } else {
            0
        };
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
                karaoke_stack_row,
                centered_karaoke_width,
                crate::config::reading_bar_offset_seconds(),
                fps,
            )
        } else {
            layout_ctx.line_rect_with_karaoke_width(
                line,
                current_frame,
                zone,
                karaoke_preview,
                None,
                crate::config::reading_bar_offset_seconds(),
                fps,
            )
        };
        collision_line_rects.insert(lid, r);

        let mut badge_rect = if karaoke_playback {
            badge_rect_for_karaoke_rect(line, &r)
        } else {
            layout_ctx.badge_rect_for_name(
                line,
                &line.character_name,
                r.x,
                zone,
                crate::config::reading_bar_offset_seconds(),
                fps,
            )
        };
        // A karaoke track can contain several stacked dialogue rows. The
        // label belongs to the actual row, never to the full track body.
        badge_rect.y = r.y;
        badge_rect.height = r.height;
        let show_badge = line.kind.is_dialogue()
            && (!karaoke_playback || karaoke_index.character_label_visible(line));
        let has_leading_label =
            show_badge || matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
        let leading_visual = has_leading_label.then(|| {
            rythmo_layout::leading_visual_bounds(
                badge_rect.x,
                badge_rect.width,
                if !karaoke_playback && line.kind.is_dialogue() {
                    line.voice_actor_names.len()
                } else {
                    0
                },
                ACTOR_ICON_SIZE,
                ACTOR_ICON_GAP,
            )
        });
        let intersects_viewport = rythmo_layout::line_or_badge_intersects_viewport(
            r.x,
            r.width,
            leading_visual,
            zone.x,
            zone.x + zone.width,
        );
        if !intersects_viewport {
            if karaoke_preview
                && show_badge
                && !line.character_name.is_empty()
                && leading_visual.is_some_and(|(x, _)| x > zone.x + zone.width)
            {
                badge_prewarm_candidates.push((lid, badge_rect, r.x));
            }
            continue;
        }

        let karaoke_progress_info = if karaoke_playback {
            karaoke_progress_render_info(line, current_frame, karaoke_lang)
        } else {
            None
        };

        line_data.push((
            lid,
            LineRenderData {
                rect: r,
                badge_rect,
                show_badge,
                karaoke_playback,
                karaoke_count_in,
                karaoke_progress_info,
                karaoke_row_key: karaoke_playback.then_some((
                    rythmo_layout::track_index_for_y_slot(line.y_slot),
                    karaoke_stack_row,
                )),
                karaoke_priority: (karaoke_active, line.start_frame, line.id),
            },
        ));
    }

    let karaoke_winners =
        placement::select_karaoke_winners(line_data.iter().filter_map(|(lid, data)| {
            let key = data.karaoke_row_key?;
            let priority = KaraokeRowPriority {
                active: data.karaoke_priority.0,
                start_frame: data.karaoke_priority.1,
                line_id: *lid,
            };
            Some((key, priority, *lid))
        }));
    line_data.retain(|(lid, data)| {
        if data.karaoke_row_key.is_none() {
            return true;
        }
        karaoke_winners.contains(lid)
    });

    // Keep a stable vertical draw order, then compare every badge with every
    // queried body, including bodies already culled from the viewport. Relative
    // overlaps must not change when the leading line leaves the screen.
    line_data.sort_by(|a, b| {
        a.1.badge_rect
            .y
            .partial_cmp(&b.1.badge_rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut badge_hidden: HashMap<u64, bool> = HashMap::new();
    let mut fitted_badges: HashMap<u64, (Rect, f32)> = HashMap::new();
    let collision_targets: Vec<(u64, Rect, &str)> = collision_line_rects
        .iter()
        .filter_map(|(&line_id, &rect)| {
            project
                .get_line(line_id)
                .map(|line| (line_id, rect, line.character_name.as_str()))
        })
        .collect();

    for (line_id, badge_rect, line_x) in badge_prewarm_candidates {
        let Some(line) = project.get_line(line_id) else {
            continue;
        };
        let (hidden, fitted_rect, scale) = character_badge_collision_layout(
            line_id,
            &line.character_name,
            &badge_rect,
            line_x,
            &collision_targets,
        );
        if !hidden {
            stretched.push(character_badge_text(line, fitted_rect, scale, true));
        }
    }

    for (line_id, data) in &line_data {
        let Some(line) = project.get_line(*line_id) else {
            continue;
        };
        let (hidden, fitted_rect, scale) = character_badge_collision_layout(
            *line_id,
            &line.character_name,
            &data.badge_rect,
            data.rect.x,
            &collision_targets,
        );
        badge_hidden.insert(*line_id, hidden);
        fitted_badges.insert(*line_id, (fitted_rect, scale));
    }

    // Now render all lines using precomputed data
    for (line_id, data) in line_data {
        let Some(line) = project.get_line(line_id) else {
            continue;
        };

        let is_hovered = editable && state.hovered_line == Some(line.id);
        let is_selected = editable
            && (matches!(state.selected, Some(Selection::Line(id)) if id == line.id)
                || matches!(state.selected, Some(Selection::Lines(ref ids)) if ids.contains(&line.id))
                || matches!(state.selected, Some(Selection::AllLines)));
        let is_editing = editable && state.editing_line == Some(line.id);
        let lint_severity = editable
            .then(|| lint_severities.get(&line.id).copied())
            .flatten();
        let karaoke_playback_line = data.karaoke_playback;
        let read_highlight_end = if project.settings().highlight_read_word && !line.karaoke {
            let progress =
                (current_frame - line.start_frame as f64) / line.duration_frames.max(1) as f64;
            crate::syllable::read_highlight_end_from_timing(
                &line.text,
                &line.syllable_ratios,
                karaoke_lang,
                progress as f32,
            )
        } else {
            None
        };

        if editable && !karaoke_playback_line && line.kind.is_dialogue() {
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
            if let Some(severity) = lint_severity {
                push_lint_wave(
                    quads,
                    data.rect.x,
                    data.rect.x + data.rect.width,
                    data.rect.y + data.rect.height - 2.0,
                    severity,
                );
            }
        }

        let scrolling_text_tint = if line.kind.is_ambiance() {
            [0.95, 0.12, 0.16, 1.0]
        } else if project.settings().scrolling_text_uses_character_color {
            [
                line.character_color[0].clamp(0.0, 1.0),
                line.character_color[1].clamp(0.0, 1.0),
                line.character_color[2].clamp(0.0, 1.0),
                1.0,
            ]
        } else {
            [1.0; 4]
        };

        if line.kind.is_ambiance() {
            let at_start = matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
            if at_start {
                let ambiance_label = crate::rythmo_line::ambiance_label(&line.character_name);
                let (underline_x, underline_width) =
                    label_underline_span(data.badge_rect, &ambiance_label, 1.0);
                let mut label = StretchedText::natural(
                    line.id ^ 0x414D_4249_414E_4345,
                    ambiance_label,
                    data.badge_rect,
                    1.0,
                    [0.2, 0.55, 1.0, 1.0],
                );
                label.emphasized = true;
                stretched.push(label);
                quads.push(QuadInstance {
                    rect: [
                        underline_x,
                        data.badge_rect.y + data.badge_rect.height - 2.0,
                        underline_width,
                        1.5,
                    ],
                    color: [0.2, 0.55, 1.0, 1.0],
                    color_bottom: [0.2, 0.55, 1.0, 1.0],
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
                    rect: [
                        underline_x,
                        data.badge_rect.y + data.badge_rect.height - 5.5,
                        underline_width,
                        1.5,
                    ],
                    color: [0.2, 0.55, 1.0, 1.0],
                    color_bottom: [0.2, 0.55, 1.0, 1.0],
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

        // Stretched text or special rendering for breath arrows
        let mut cursor_segments = None;
        let description_rect = ambiance_description_rect(data.rect, line.kind);
        if !line.text.is_empty() {
            if line.text == "↑" || line.text == "↓" {
                render_breath_arrow(&data.rect, line.text == "↑", quads);
            } else if karaoke_playback_line {
                push_karaoke_rythmo_text(stretched, line, data.rect, data.karaoke_progress_info);
            } else {
                let drag_ratios = state
                    .syllable_drag
                    .as_ref()
                    .filter(|drag| drag.line_id == line.id);
                if let Some(segments) = render_sync_text_segments(
                    project,
                    line,
                    description_rect,
                    drag_ratios,
                    karaoke_lang,
                    state,
                    read_highlight_end,
                    scrolling_text_tint,
                    (!line.text_emotions.is_empty()).then(|| state.text_emotion_seconds()),
                    stretched,
                ) {
                    if is_editing {
                        cursor_segments = Some(segments);
                    }
                } else if !line.text_emotions.is_empty() {
                    let segments = push_emotional_text(
                        stretched,
                        line,
                        description_rect,
                        state.text_emotion_seconds(),
                        scrolling_text_tint,
                        None,
                        project.settings().show_text_emotion_lanes,
                    );
                    if is_editing {
                        cursor_segments = Some(segments);
                    }
                } else if let Some((breaks, ratios)) = visible_syllable_segments(
                    line,
                    drag_ratios,
                    karaoke_lang,
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
                        description_rect,
                        0,
                        read_highlight_end,
                        scrolling_text_tint,
                    );
                }
            }
        }

        // Keep a genuine clear gutter for the liaison. This is drawn after
        // text so even a long stretched description cannot cover the symbol.
        if line.kind.is_ambiance() {
            let at_start = matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
            let gutter = 14.0_f32.min(data.rect.width);
            let gx = if at_start {
                data.rect.x
            } else {
                data.rect.x + data.rect.width - gutter
            };
            let _ = (gx, gutter); // space is reserved in description_rect; no background panel.
        }

        if !line.presence.is_on() && !line.text.is_empty() {
            let y = data.rect.y + data.rect.height - 3.0;
            let color = scrolling_text_tint;
            if line.presence == crate::rythmo_line::LinePresence::Off {
                quads.push(QuadInstance {
                    rect: [data.rect.x, y, data.rect.width, 1.5],
                    color,
                    color_bottom: color,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            } else {
                let mut x = data.rect.x;
                while x < data.rect.x + data.rect.width {
                    quads.push(QuadInstance {
                        rect: [x, y, 7.0_f32.min(data.rect.x + data.rect.width - x), 1.5],
                        color,
                        color_bottom: color,
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 0.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                    x += 12.0;
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
                    description_rect.x,
                    description_rect.width,
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
            editable && state.syllable_drag.as_ref().map(|d| d.line_id) == Some(line.id);
        if line.karaoke && !karaoke_playback_line && (is_hovered || is_syllable_drag_line) {
            if let Some(ratios) =
                syllable_ratios_for_line(line, state.syllable_drag.as_ref(), karaoke_lang, state)
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

        // Ambiance labels use the configured rythmo font through the stretched
        // text path above. They never receive a character badge or actor icon.
        if line.kind.is_ambiance() {
            if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                render_ambiance_name_cursor(
                    quads,
                    data.badge_rect,
                    &line.character_name,
                    &state.char_input,
                    editable && state.editing_character == Some(line.id),
                );
            }
            continue;
        }

        // Character badge — use precomputed badge_rect
        let (br, badge_scale) = fitted_badges
            .get(&line_id)
            .copied()
            .unwrap_or((data.badge_rect, 1.0));

        // Overlap detection vs OTHER lines: use precomputed HashMaps
        let badge_hidden = *badge_hidden.get(&line_id).unwrap_or(&false);

        if data.show_badge && !badge_hidden {
            let badge_color = line.character_color;
            let is_editing_char = editable && state.editing_character == Some(line.id);
            // Same emphasized typography as ambiance labels, tinted with the
            // character colour and deliberately left without an underline.
            if !line.character_name.is_empty() {
                stretched.push(character_badge_text(line, br, badge_scale, false));
                let (underline_x, underline_width) =
                    label_underline_span(br, &line.character_name, badge_scale);
                for y_offset in [2.0, 5.5] {
                    quads.push(QuadInstance {
                        rect: [
                            underline_x,
                            br.y + br.height - y_offset * badge_scale,
                            underline_width,
                            1.5 * badge_scale,
                        ],
                        color: badge_color,
                        color_bottom: badge_color,
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

            if !karaoke_playback_line {
                render_voice_actor_icons_for_line(
                    line,
                    project,
                    zone,
                    br,
                    ACTOR_ICON_SIZE * badge_scale,
                    quads,
                    labels,
                    actor_icons,
                );
            }

            render_character_name_cursor(
                quads,
                br,
                &line.character_name,
                &state.char_input,
                is_editing_char,
                badge_scale,
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
                    transform: [0.0, 0.0, 0.5, 0.5],
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

        let is_editing_note = editable && state.editing_note == Some(line.id);
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

    // Detection signs remain part of the rythmo display during playback. Their
    // position is tied to `current_frame`, so the overlay must be rendered on
    // every frame rather than only while the video is paused.
    render_detection_overlay(
        zone,
        project,
        current_frame,
        fps,
        state,
        quads,
        labels,
        note_icons,
        detection_uvs,
        editable,
    );

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
    if let Some(ghost) = editable
        .then_some(())
        .and_then(|_| state.ghost_preview.as_ref())
    {
        let body_rect = layout_ctx.track_body_rect(ghost.y_slot, zone);
        let ghost_rect_x = frame_to_x(ghost.frame, current_frame, zone, fps);
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
    fps: f64,
) {
    let line_id = match state.editing_character {
        Some(id) => id,
        None => return,
    };
    let line = match project.get_line(line_id) {
        Some(l) => l,
        None => return,
    };
    let suggestions = project.autocomplete_entries_for_line(line);
    if suggestions.is_empty() {
        return;
    }

    let r = line_rect(
        project,
        line,
        current_frame,
        zone,
        crate::config::reading_bar_offset_seconds(),
        fps,
    );
    let br = badge_rect_for_line(
        project,
        line,
        current_frame,
        zone,
        crate::config::reading_bar_offset_seconds(),
        fps,
    );
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

        // Ambiance names have no character colour swatch.
        if line.kind.is_dialogue() {
            quads.push(QuadInstance {
                rect: [dropdown_x + 4.0, dropdown_y + 4.0, 12.0, item_h - 8.0],
                color: suggestion.1,
                color_bottom: suggestion.1,
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
        // Name label
        labels.push(LabelInfo {
            text: suggestion.0,
            bounds: Rect {
                x: dropdown_x + if line.kind.is_dialogue() { 20.0 } else { 4.0 },
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
    fps: f64,
    lint_diagnostics: &[crate::lint::Diagnostic],
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    liaison_icons: &mut Vec<IconInstance>,
    liaison_left_uv: [f32; 4],
    liaison_right_uv: [f32; 4],
) {
    let _ = fps;
    for diagnostic in lint_diagnostics {
        if let crate::lint::Scope::Zone {
            start_frame,
            end_frame,
        } = diagnostic.scope
        {
            let left = frame_to_x(start_frame, current_frame, zone, fps).max(zone.x);
            let right = frame_to_x(end_frame, current_frame, zone, fps).min(zone.x + zone.width);
            if right > left {
                // Zone diagnostics sit outside the dialogue rows, directly
                // under the ruler, and therefore remain visible on empty parts.
                push_lint_wave(
                    quads,
                    left,
                    right,
                    zone.y + constants::RULER_HEIGHT + 3.0,
                    diagnostic.severity,
                );
            }
        }
    }
    let margin_frames = f64_ceil_to_i64(20.0 / ppf().max(0.001) as f64).saturating_add(1);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames, fps);
    for marker_index in render_index.visible_marker_indices(first_frame, last_frame) {
        let Some(marker) = project.marker(marker_index) else {
            continue;
        };
        let x = frame_to_x(marker.frame, current_frame, zone, fps);
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
                let number = project.markers()[..marker_index]
                    .iter()
                    .filter(|marker| matches!(marker.kind, MarkerKind::SceneChange))
                    .count()
                    + 1;
                labels.push(LabelInfo {
                    text: scene_number_label(number),
                    bounds: Rect {
                        x: x + 5.0,
                        y: zone.y + 4.0,
                        width: 28.0,
                        height: 24.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Top,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(18.0),
                    color_override: Some([235, 235, 245]),
                    font_family_override: None,
                });
            }
            MarkerKind::LiaisonLeft => {
                let uv = liaison_left_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                    transform: [0.0, 0.0, 0.5, 0.5],
                });
            }
            MarkerKind::LiaisonRight => {
                let uv = liaison_right_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                    transform: [0.0, 0.0, 0.5, 0.5],
                });
            }
        }
    }
}

fn scene_number_label(number: usize) -> &'static str {
    const LABELS: [&str; 31] = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    ];
    LABELS.get(number).copied().unwrap_or("30+")
}

/// Place the actual liaison SVG from the icon atlas inside ambiance lines.
/// Start lines use the right-facing glyph after their label; end lines use
/// the left-facing glyph at the right edge of the writable span.
pub fn render_ambiance_liaison_icons(
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    fps: f64,
    icons: &mut Vec<IconInstance>,
    liaison_left_uv: [f32; 4],
    liaison_right_uv: [f32; 4],
) {
    let (first_frame, last_frame) = render_window(zone, current_frame, 4, fps);
    for line_id in render_index.visible_line_ids(project, first_frame, last_frame) {
        let Some(line) = project
            .get_line(line_id)
            .filter(|line| line.kind.is_ambiance())
        else {
            continue;
        };
        let rect = line_rect(
            project,
            line,
            current_frame,
            zone,
            crate::config::reading_bar_offset_seconds(),
            fps,
        );
        if rect.x + rect.width < zone.x || rect.x > zone.x + zone.width {
            continue;
        }
        let size = AMBIANCE_LIAISON_SIZE;
        let start = matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
        let x = if start {
            rect.x + 2.0
        } else {
            rect.x + rect.width - size - 2.0
        };
        icons.push(IconInstance {
            rect: [x, rect.y + (rect.height - size) * 0.5, size, size],
            uv_rect: if start {
                liaison_right_uv
            } else {
                liaison_left_uv
            },
            // The liaison SVG is used as an alpha mask by the icon shader.
            // Keep it fully opaque and white so it remains visible on the
            // dark rythmo band and retains the glyph's solid silhouette.
            tint: [1.0, 1.0, 1.0, 1.0],
            transform: [0.0, 0.0, 0.5, 0.5],
        });
    }
}

fn push_lint_wave(
    quads: &mut Vec<QuadInstance>,
    left: f32,
    right: f32,
    y: f32,
    severity: crate::lint::Severity,
) {
    let color = match severity {
        crate::lint::Severity::Error => [0.95, 0.18, 0.18, 0.98],
        crate::lint::Severity::Warning => [0.96, 0.72, 0.12, 0.98],
    };
    let step = 5.0_f32;
    let mut x = left;
    let mut rising = true;
    while x < right {
        let width = step.min(right - x);
        quads.push(QuadInstance {
            rect: [x, y, width + 0.5, 1.5],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.75,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: if rising { 0.38 } else { -0.38 },
            _padding: [0.0; 2],
        });
        rising = !rising;
        x += step;
    }
}

pub fn lint_zone_diagnostics(
    zone: &Rect,
    _project: &Project,
    current_frame: f64,
    fps: f64,
    diagnostics: &[crate::lint::Diagnostic],
    cursor_x: f32,
    cursor_y: f32,
) -> Vec<crate::lint::Diagnostic> {
    let wave_y = zone.y + constants::RULER_HEIGHT + 3.0;
    if (cursor_y - wave_y).abs() > 7.0 {
        return Vec::new();
    }
    let diagnostics: Vec<_> = diagnostics
        .iter()
        .cloned()
        .filter(|diagnostic| {
            if let crate::lint::Scope::Zone {
                start_frame,
                end_frame,
            } = diagnostic.scope
            {
                let left = frame_to_x(start_frame, current_frame, zone, fps).max(zone.x);
                let right =
                    frame_to_x(end_frame, current_frame, zone, fps).min(zone.x + zone.width);
                cursor_x >= left && cursor_x <= right
            } else {
                false
            }
        })
        .collect();
    diagnostics
}

pub fn autocomplete_hit(
    zone: &Rect,
    project: &Project,
    current_frame: f64,
    state: &RythmoState,
    click_x: f32,
    click_y: f32,
    fps: f64,
) -> Option<(String, [f32; 4])> {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = project.lines().find(|l| l.id == line_id) {
            let br = badge_rect_for_line(
                project,
                line,
                current_frame,
                zone,
                crate::config::reading_bar_offset_seconds(),
                fps,
            );
            let lr = line_rect(
                project,
                line,
                current_frame,
                zone,
                crate::config::reading_bar_offset_seconds(),
                fps,
            );
            let suggestions = project.autocomplete_entries_for_line(line);
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
                        return Some((suggestion.0.to_string(), suggestion.1));
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
const MENU_EMOTION_W: f32 = 190.0;
const MENU_GAP: f32 = 0.0;
const MENU_MARGIN: f32 = 8.0;
const MENU_MAX_ACTOR_H: f32 = 260.0;

const EMOTION_ANGER: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::AngerSoft,
    crate::rythmo_line::TextEmotion::Shake,
    crate::rythmo_line::TextEmotion::AngerContained,
    crate::rythmo_line::TextEmotion::AngerHeavy,
    crate::rythmo_line::TextEmotion::AngerExtreme,
];
const EMOTION_JOY: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::JoySoft,
    crate::rythmo_line::TextEmotion::Yay,
    crate::rythmo_line::TextEmotion::Bounce,
    crate::rythmo_line::TextEmotion::JoyBurst,
    crate::rythmo_line::TextEmotion::JoyExtreme,
];
const EMOTION_FEAR: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::FearSoft,
    crate::rythmo_line::TextEmotion::Wiggle,
    crate::rythmo_line::TextEmotion::FearPanic,
    crate::rythmo_line::TextEmotion::FearStrong,
    crate::rythmo_line::TextEmotion::FearExtreme,
];
const EMOTION_SADNESS: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::SadnessSoft,
    crate::rythmo_line::TextEmotion::Pendulum,
    crate::rythmo_line::TextEmotion::SadnessDeep,
    crate::rythmo_line::TextEmotion::SadnessStrong,
    crate::rythmo_line::TextEmotion::SadnessExtreme,
];
const EMOTION_TENDERNESS: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::TendernessSoft,
    crate::rythmo_line::TextEmotion::Swing,
    crate::rythmo_line::TextEmotion::LoveTender,
    crate::rythmo_line::TextEmotion::TendernessStrong,
    crate::rythmo_line::TextEmotion::TendernessExtreme,
];
const EMOTION_DISGUST: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::DisgustSoft,
    crate::rythmo_line::TextEmotion::Slide,
    crate::rythmo_line::TextEmotion::Disgust,
    crate::rythmo_line::TextEmotion::DisgustStrong,
    crate::rythmo_line::TextEmotion::DisgustExtreme,
];
const EMOTION_DOUBT: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::DoubtSoft,
    crate::rythmo_line::TextEmotion::Oscillation,
    crate::rythmo_line::TextEmotion::Doubt,
    crate::rythmo_line::TextEmotion::DoubtStrong,
    crate::rythmo_line::TextEmotion::DoubtExtreme,
];
const EMOTION_QUESTION: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::QuestionSoft,
    crate::rythmo_line::TextEmotion::Question,
    crate::rythmo_line::TextEmotion::QuestionStrong,
    crate::rythmo_line::TextEmotion::QuestionExtreme,
    crate::rythmo_line::TextEmotion::QuestionFast,
];
const EMOTION_EXCLAMATION: &[crate::rythmo_line::TextEmotion] = &[
    crate::rythmo_line::TextEmotion::ExclamationSoft,
    crate::rythmo_line::TextEmotion::Exclamation,
    crate::rythmo_line::TextEmotion::ExclamationStrong,
    crate::rythmo_line::TextEmotion::ExclamationExtreme,
    crate::rythmo_line::TextEmotion::ExclamationHuge,
];
const EMOTION_CATEGORIES: &[(&str, &[crate::rythmo_line::TextEmotion])] = &[
    ("text_emotion.category.anger", EMOTION_ANGER),
    ("text_emotion.category.joy", EMOTION_JOY),
    ("text_emotion.category.fear", EMOTION_FEAR),
    ("text_emotion.category.sadness", EMOTION_SADNESS),
    ("text_emotion.category.tenderness", EMOTION_TENDERNESS),
    ("text_emotion.category.disgust", EMOTION_DISGUST),
    ("text_emotion.category.doubt", EMOTION_DOUBT),
    ("text_emotion.category.question", EMOTION_QUESTION),
    ("text_emotion.category.exclamation", EMOTION_EXCLAMATION),
];

fn emotion_category(
    index: usize,
) -> Option<&'static (&'static str, &'static [crate::rythmo_line::TextEmotion])> {
    EMOTION_CATEGORIES.get(index)
}

fn emotion_group(
    menu_index: usize,
) -> Option<&'static (&'static str, &'static [crate::rythmo_line::TextEmotion])> {
    menu_index.checked_sub(1).and_then(emotion_category)
}

pub fn context_menu_accessibility_label(project: &Project, line_id: u64) -> String {
    let mut items = vec![
        t("context.voice_actor.assign_to_actor"),
        t("context.change_character"),
    ];
    if project
        .get_line(line_id)
        .is_some_and(|line| line.can_have_text_emotions())
    {
        items.push(t("text_emotion.menu"));
    }
    if can_generate_detection_signs(project, line_id) {
        items.push(t("context.generate_detection_signs"));
    }
    format!(
        "{} : {}",
        t("accessibility.line_context_menu"),
        items.join(", ")
    )
}

fn can_generate_detection_signs(project: &Project, line_id: u64) -> bool {
    crate::config::dev_mode()
        && project.get_line(line_id).is_some_and(|line| {
            line.kind.is_dialogue() && !line.text.trim().is_empty() && line.duration_frames > 0
        })
}

fn generation_root_index(project: &Project, line_id: u64) -> Option<usize> {
    can_generate_detection_signs(project, line_id).then(|| {
        if project
            .get_line(line_id)
            .is_some_and(|line| line.can_have_text_emotions())
        {
            3
        } else {
            2
        }
    })
}

fn root_item_count(project: &Project, line_id: u64) -> usize {
    if let Some(index) = generation_root_index(project, line_id) {
        index + 1
    } else if project
        .get_line(line_id)
        .is_some_and(|line| line.can_have_text_emotions())
    {
        3
    } else {
        2
    }
}

fn root_hover_index(project: &Project, menu: &LineContextMenu) -> usize {
    if menu.hover_change_character {
        1
    } else if menu.hover_text_emotion {
        2
    } else if menu.hover_generate_detection {
        generation_root_index(project, menu.line_id).unwrap_or(0)
    } else {
        0
    }
}

fn set_root_hover(project: &Project, menu: &mut LineContextMenu, index: usize) {
    menu.hover_main = index == 0;
    menu.hover_change_character = index == 1;
    menu.hover_text_emotion = index == 2
        && project
            .get_line(menu.line_id)
            .is_some_and(|line| line.can_have_text_emotions());
    menu.hover_generate_detection = generation_root_index(project, menu.line_id) == Some(index);
    menu.hover_emotion_index = None;
    menu.hover_emotion_variant = None;
}

fn selected_context_menu_label<'a>(
    project: &'a Project,
    menu: &LineContextMenu,
) -> Option<&'a str> {
    if let (Some(category), Some(variant)) = (menu.hover_emotion_index, menu.hover_emotion_variant)
    {
        emotion_group(category)
            .and_then(|(_, emotions)| emotions.get(variant))
            .map(|emotion| t(emotion.i18n_key()))
    } else if let Some(index) = menu.hover_emotion_index {
        Some(if index == 0 {
            t("text_emotion.remove")
        } else {
            t(EMOTION_CATEGORIES[index - 1].0)
        })
    } else if let Some(action) = menu.hover_action_index {
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
    } else if menu.hover_generate_detection {
        Some(t("context.generate_detection_signs"))
    } else if menu.hover_text_emotion {
        Some(t("text_emotion.menu"))
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
}

fn announce_context_menu_selection(project: &Project, state: &RythmoState) -> EventResponse {
    state
        .context_menu
        .as_ref()
        .and_then(|menu| selected_context_menu_label(project, menu))
        .map_or(EventResponse::Consumed, |label| {
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection {
                    label: label.to_string(),
                },
            ))
        })
}

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
    crate::config::set_project_view_settings(
        project.settings().scroll_speed,
        project.settings().reading_bar_offset_percent,
        zone.width,
        fps,
    );
    match event {
        UiEvent::ContextMenu { x, y } => {
            let line_id = project
                .lines()
                .find(|line| {
                    line_rect(
                        project,
                        line,
                        current_frame,
                        zone,
                        crate::config::reading_bar_offset_seconds(),
                        fps,
                    )
                    .contains(*x, *y)
                        || badge_rect_for_line(
                            project,
                            line,
                            current_frame,
                            zone,
                            crate::config::reading_bar_offset_seconds(),
                            fps,
                        )
                        .contains(*x, *y)
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
                    hover_text_emotion: false,
                    hover_generate_detection: false,
                    hover_emotion_index: None,
                    hover_emotion_variant: None,
                    text_range: (state.editing_line == Some(line_id))
                        .then(|| state.line_input.selection_range())
                        .flatten(),
                    hover_actor_index: None,
                    hover_action_index: None,
                    actor_scroll: 0.0,
                });
                if !line_was_selected {
                    state.selected = Some(Selection::Line(line_id));
                }
                state.dragging = None;
                return EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus {
                        label: context_menu_accessibility_label(project, line_id),
                        role: "menu".to_string(),
                    },
                ));
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
            let (_, actor_rect, _, _, max_scroll, _) =
                context_menu_layout(project, screen_w, screen_h, menu);
            if context_actor_menu_visible(menu) && actor_rect.contains(*x, *y) {
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
            let (root_rect, actor_rect, action_rect, actor_scroll, _, emotion_rect) =
                context_menu_layout(project, screen_w, screen_h, menu);
            let variant_rect = emotion_variant_rect(screen_w, screen_h, menu, emotion_rect);

            if root_rect.contains(*x, *y) {
                let root_item = ((*y - root_rect.y) / MENU_ITEM_H).floor() as usize;
                if root_item == 1 {
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::OpenLinesPanel);
                }
                if generation_root_index(project, menu.line_id) == Some(root_item) {
                    let line_id = menu.line_id;
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::GenerateDetectionSigns { line_id });
                }
            }

            if emotion_rect.contains(*x, *y) {
                let index = ((*y - emotion_rect.y) / MENU_ITEM_H).floor() as usize;
                if index == 0 {
                    let line_id = menu.line_id;
                    let range = menu.text_range;
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::SetTextEmotion {
                        line_id,
                        range,
                        emotion: None,
                    });
                }
            }

            if variant_rect.contains(*x, *y) {
                if let (Some(category), Some(variant)) =
                    (menu.hover_emotion_index, menu.hover_emotion_variant)
                {
                    if let Some((_, emotions)) = emotion_group(category) {
                        if let Some(&emotion) = emotions.get(variant) {
                            let line_id = menu.line_id;
                            let range = menu.text_range;
                            state.context_menu = None;
                            return EventResponse::Action(UiAction::SetTextEmotion {
                                line_id,
                                range,
                                emotion: Some(emotion),
                            });
                        }
                    }
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

            if context_actor_menu_visible(menu) && actor_rect.contains(*x, *y) {
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
                || emotion_rect.contains(*x, *y)
                || variant_rect.contains(*x, *y)
                || context_menu_bridge_contains(root_rect, actor_rect, action_rect, *x, *y)
                || bridge_rect(root_rect, emotion_rect).contains(*x, *y)
                || bridge_rect(emotion_rect, variant_rect).contains(*x, *y)
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
            } else if menu.hover_text_emotion {
                menu.hover_emotion_index = Some(0);
                menu.hover_emotion_variant = None;
                menu.hover_text_emotion = false;
            } else if menu.hover_emotion_index.is_some() {
                menu.hover_emotion_variant = Some(0);
                menu.hover_text_emotion = false;
            } else if menu.hover_actor_index.is_some() {
                menu.hover_action_index = Some(0);
            }
            announce_context_menu_selection(project, state)
        }
        UiEvent::CursorLeft => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            if menu.hover_emotion_variant.take().is_some() {
                // Stay in the category list.
            } else if menu.hover_emotion_index.take().is_some() {
                menu.hover_text_emotion = true;
            } else if menu.hover_action_index.take().is_none() {
                menu.hover_actor_index = None;
                menu.hover_main = true;
                menu.hover_change_character = false;
                menu.hover_text_emotion = false;
                menu.hover_generate_detection = false;
            }
            announce_context_menu_selection(project, state)
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
            if let (Some(category), Some(variant)) = (
                menu.hover_emotion_index,
                menu.hover_emotion_variant.as_mut(),
            ) {
                if let Some((_, emotions)) = emotion_group(category) {
                    let len = emotions.len();
                    *variant = (*variant as i32 + direction).rem_euclid(len as i32) as usize;
                }
            } else if let Some(category) = menu.hover_emotion_index.as_mut() {
                let len = EMOTION_CATEGORIES.len() + 1;
                *category = (*category as i32 + direction).rem_euclid(len as i32) as usize;
            } else if let Some(action) = menu.hover_action_index.as_mut() {
                *action = (*action as i32 + direction).rem_euclid(4) as usize;
            } else if menu.hover_actor_index.is_some()
                && !menu.hover_main
                && !menu.hover_change_character
                && !menu.hover_text_emotion
                && !menu.hover_generate_detection
            {
                let len = project.voice_actors().len() + 1;
                let current = menu.hover_actor_index.unwrap_or(0);
                menu.hover_actor_index =
                    Some((current as i32 + direction).rem_euclid(len as i32) as usize);
            } else {
                let root_len = root_item_count(project, menu.line_id) as i32;
                let current = root_hover_index(project, menu) as i32;
                let next = (current + direction).rem_euclid(root_len) as usize;
                set_root_hover(project, menu, next);
            }
            announce_context_menu_selection(project, state)
        }
        UiEvent::Activate => {
            let Some(menu) = state.context_menu.as_ref() else {
                return EventResponse::Ignored;
            };
            if menu.hover_generate_detection {
                let line_id = menu.line_id;
                state.context_menu = None;
                return EventResponse::Action(UiAction::GenerateDetectionSigns { line_id });
            }
            if menu.hover_main {
                let menu = state.context_menu.as_mut().unwrap();
                menu.hover_main = false;
                menu.hover_actor_index = Some(0);
                return announce_context_menu_selection(project, state);
            }
            if let (Some(category), Some(variant)) =
                (menu.hover_emotion_index, menu.hover_emotion_variant)
            {
                let line_id = menu.line_id;
                let range = menu.text_range;
                if let Some((_, emotions)) = emotion_group(category) {
                    if let Some(&emotion) = emotions.get(variant) {
                        state.context_menu = None;
                        return EventResponse::Action(UiAction::SetTextEmotion {
                            line_id,
                            range,
                            emotion: Some(emotion),
                        });
                    }
                }
            }
            if let Some(index) = menu.hover_emotion_index {
                if index == 0 {
                    let line_id = menu.line_id;
                    let range = menu.text_range;
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::SetTextEmotion {
                        line_id,
                        range,
                        emotion: None,
                    });
                }
                state.context_menu.as_mut().unwrap().hover_emotion_variant = Some(0);
                return announce_context_menu_selection(project, state);
            }
            if menu.hover_text_emotion {
                let menu = state.context_menu.as_mut().unwrap();
                menu.hover_emotion_index = Some(0);
                menu.hover_emotion_variant = None;
                return EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: t("text_emotion.remove").to_string(),
                    },
                ));
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
    let (root_rect, actor_rect, action_rect, actor_scroll, max_scroll, emotion_rect) =
        context_menu_layout(project, screen_w, screen_h, menu);
    let variant_rect = emotion_variant_rect(screen_w, screen_h, menu, emotion_rect);

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
    if project
        .get_line(menu.line_id)
        .is_some_and(|line| line.can_have_text_emotions())
    {
        render_menu_item(
            quads,
            labels,
            Rect {
                x: root_rect.x,
                y: root_rect.y + MENU_ITEM_H * 2.0,
                width: root_rect.width,
                height: MENU_ITEM_H,
            },
            t("text_emotion.menu"),
            menu.hover_text_emotion,
            true,
        );
    }

    if let Some(index) = generation_root_index(project, menu.line_id) {
        render_menu_item(
            quads,
            labels,
            Rect {
                x: root_rect.x,
                y: root_rect.y + MENU_ITEM_H * index as f32,
                width: root_rect.width,
                height: MENU_ITEM_H,
            },
            t("context.generate_detection_signs"),
            menu.hover_generate_detection,
            false,
        );
    }

    if menu.hover_text_emotion || menu.hover_emotion_index.is_some() {
        render_menu_panel(quads, emotion_rect);
        for index in 0..=EMOTION_CATEGORIES.len() {
            let label = if index == 0 {
                t("text_emotion.remove")
            } else {
                t(EMOTION_CATEGORIES[index - 1].0)
            };
            render_menu_item(
                quads,
                labels,
                Rect {
                    x: emotion_rect.x,
                    y: emotion_rect.y + index as f32 * MENU_ITEM_H,
                    width: emotion_rect.width,
                    height: MENU_ITEM_H,
                },
                label,
                menu.hover_emotion_index == Some(index),
                index > 0,
            );
        }
    }

    if let (Some(category), Some(_)) = (menu.hover_emotion_index, menu.hover_emotion_variant) {
        if let Some((_, emotions)) = emotion_group(category) {
            render_menu_panel(quads, variant_rect);
            for (index, emotion) in emotions.iter().enumerate() {
                render_menu_item(
                    quads,
                    labels,
                    Rect {
                        x: variant_rect.x,
                        y: variant_rect.y + index as f32 * MENU_ITEM_H,
                        width: variant_rect.width,
                        height: MENU_ITEM_H,
                    },
                    t(emotion.i18n_key()),
                    menu.hover_emotion_variant == Some(index),
                    false,
                );
            }
        }
    }

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
) -> (Rect, Rect, Rect, f32, f32, Rect) {
    let root_h = MENU_ITEM_H * root_item_count(project, menu.line_id) as f32;
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

    let emotion_h = MENU_ITEM_H * (EMOTION_CATEGORIES.len() + 1) as f32;
    let emotion_x_right = root_rect.x + root_rect.width + MENU_GAP;
    let emotion_x = if emotion_x_right + MENU_EMOTION_W <= screen_w - MENU_MARGIN {
        emotion_x_right
    } else {
        (root_rect.x - MENU_EMOTION_W - MENU_GAP).max(MENU_MARGIN)
    };
    let emotion_rect = Rect {
        x: emotion_x,
        y: (root_rect.y + MENU_ITEM_H * 2.0).clamp(
            MENU_MARGIN,
            (screen_h - emotion_h - MENU_MARGIN).max(MENU_MARGIN),
        ),
        width: MENU_EMOTION_W,
        height: emotion_h,
    };

    (
        root_rect,
        actor_rect,
        action_rect,
        actor_scroll,
        max_scroll,
        emotion_rect,
    )
}

fn emotion_variant_rect(
    screen_w: f32,
    screen_h: f32,
    menu: &LineContextMenu,
    emotion_rect: Rect,
) -> Rect {
    let count = menu
        .hover_emotion_index
        .and_then(|index| emotion_group(index).map(|(_, emotions)| emotions.len()))
        .unwrap_or(1);
    let height = MENU_ITEM_H * count as f32;
    let x_right = emotion_rect.x + emotion_rect.width + MENU_GAP;
    let x = if x_right + MENU_EMOTION_W <= screen_w - MENU_MARGIN {
        x_right
    } else {
        (emotion_rect.x - MENU_EMOTION_W - MENU_GAP).max(MENU_MARGIN)
    };
    let y = (emotion_rect.y + menu.hover_emotion_index.unwrap_or(0) as f32 * MENU_ITEM_H).clamp(
        MENU_MARGIN,
        (screen_h - height - MENU_MARGIN).max(MENU_MARGIN),
    );
    Rect {
        x,
        y,
        width: MENU_EMOTION_W,
        height,
    }
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
    let (root_rect, actor_rect, action_rect, actor_scroll, _, emotion_rect) =
        context_menu_layout(project, screen_w, screen_h, menu);
    let variant_rect = emotion_variant_rect(screen_w, screen_h, menu, emotion_rect);

    let root_hover = root_rect.contains(x, y);
    let mut actor_hover = None;
    let mut action_hover = None;
    let mut emotion_hover = None;
    let mut emotion_variant_hover = None;
    let actor_menu_visible = context_actor_menu_visible(menu);
    let root_actor_bridge = actor_menu_visible && bridge_rect(root_rect, actor_rect).contains(x, y);
    let actor_action_bridge = actor_menu_visible
        && menu.hover_actor_index.is_some()
        && bridge_rect(actor_rect, action_rect).contains(x, y);

    if actor_menu_visible && actor_rect.contains(x, y) {
        let index = ((y - actor_rect.y + actor_scroll) / MENU_ITEM_H).floor() as usize;
        if index <= project.voice_actors().len() {
            actor_hover = Some(index);
        }
    }

    if actor_menu_visible && action_rect.contains(x, y) {
        let index = ((y - action_rect.y) / MENU_ITEM_H).floor() as usize;
        if index < 4 {
            action_hover = Some(index);
            actor_hover = menu.hover_actor_index;
        }
    }
    let emotion_available = project
        .get_line(menu.line_id)
        .is_some_and(|line| line.can_have_text_emotions());
    if emotion_available && emotion_rect.contains(x, y) {
        let index = ((y - emotion_rect.y) / MENU_ITEM_H).floor() as usize;
        if index <= EMOTION_CATEGORIES.len() {
            emotion_hover = Some(index);
        }
    }
    if variant_rect.contains(x, y) {
        if let Some(category) = menu.hover_emotion_index {
            if let Some((_, emotions)) = emotion_group(category) {
                let index = ((y - variant_rect.y) / MENU_ITEM_H).floor() as usize;
                if index < emotions.len() {
                    emotion_variant_hover = Some(index);
                }
            }
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
    menu.hover_generate_detection = generation_root_index(project, menu.line_id) == root_item;
    menu.hover_text_emotion = (emotion_available && root_item == Some(2))
        || emotion_hover.is_some()
        || emotion_variant_hover.is_some()
        || (menu.hover_text_emotion && bridge_rect(root_rect, emotion_rect).contains(x, y))
        || (menu.hover_emotion_index.is_some()
            && bridge_rect(emotion_rect, variant_rect).contains(x, y));
    if emotion_hover.is_some() {
        menu.hover_emotion_index = emotion_hover;
        menu.hover_emotion_variant = emotion_hover.filter(|index| *index > 0).map(|_| 0);
    } else if emotion_variant_hover.is_some() {
        menu.hover_emotion_variant = emotion_variant_hover;
    }
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
    let suggestions = ctx.project.autocomplete_entries_for_line(line);
    if suggestions.is_empty() {
        return None;
    }

    let r = line_rect(
        ctx.project,
        line,
        ctx.current_frame,
        ctx.zone,
        crate::config::reading_bar_offset_seconds(),
        ctx.fps,
    );
    let br = badge_rect_for_line(
        ctx.project,
        line,
        ctx.current_frame,
        ctx.zone,
        crate::config::reading_bar_offset_seconds(),
        ctx.fps,
    );
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
