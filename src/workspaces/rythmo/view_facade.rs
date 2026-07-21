//! Focused view facade for the rythmo workspace.
//!
//! The established renderer remains authoritative. This boundary only removes
//! syllable-authoring handles from ordinary adaptation lines; karaoke lines keep
//! the same rendering and interaction geometry.

#[path = "view.rs"]
mod legacy;

pub use legacy::*;

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{IconInstance, LabelInfo, QuadInstance, Rect};
use crate::ui::renderer::StretchedText;

fn quad_center(quad: &QuadInstance) -> (f32, f32) {
    (
        quad.rect[0] + quad.rect[2] * 0.5,
        quad.rect[1] + quad.rect[3] * 0.5,
    )
}

fn is_syllable_handle(quad: &QuadInstance) -> bool {
    quad.color[0] >= 0.94
        && quad.color[1] <= 0.10
        && quad.color[2] <= 0.06
        && quad.rect[2] > 0.0
        && quad.rect[3] > 0.0
}

fn should_strip_handle(quad: &QuadInstance, normal_rects: &[Rect]) -> bool {
    let (x, y) = quad_center(quad);
    is_syllable_handle(quad) && normal_rects.iter().any(|rect| rect.contains(x, y))
}

fn strip_normal_line_syllable_handles(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    first_new_quad: usize,
    syllable_quads: &mut Vec<QuadInstance>,
) {
    let normal_rects = project
        .lines()
        .filter(|line| !line.karaoke)
        .map(|line| legacy::line_rect(project, line, current_frame, zone))
        .collect::<Vec<_>>();

    let mut index = first_new_quad.min(syllable_quads.len());
    while index < syllable_quads.len() {
        if should_strip_handle(&syllable_quads[index], &normal_rects) {
            syllable_quads.remove(index);
        } else {
            index += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
    detection_uvs: [[f32; 4]; 7],
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
    let first_new_quad = syllable_quads.len();
    let result = legacy::render_lines(
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        quads,
        syllable_quads,
        labels,
        stretched,
        note_icons,
        actor_icons,
        note_uv,
        detection_uvs,
    );
    strip_normal_line_syllable_handles(
        project,
        current_frame,
        zone,
        first_new_quad,
        syllable_quads,
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{
        DetectionAddress, DetectionCue, DetectionCueId, DetectionKind, MediaTick, TextAnchor,
    };

    fn quad(rect: [f32; 4], color: [f32; 4]) -> QuadInstance {
        QuadInstance {
            rect,
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
        }
    }

    #[test]
    fn only_red_syllable_handle_scene_data_is_removed_from_normal_lines() {
        let normal_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        };
        let red_handle = quad([10.0, 10.0, 3.0, 3.0], [0.95, 0.08, 0.03, 1.0]);
        let blue_dot = quad([10.0, 10.0, 3.0, 3.0], [0.48, 0.72, 1.0, 1.0]);

        assert!(should_strip_handle(&red_handle, &[normal_rect]));
        assert!(!should_strip_handle(&blue_dot, &[normal_rect]));
        assert!(!should_strip_handle(&red_handle, &[]));
    }

    #[test]
    fn sync_scene_and_caret_segments_share_strict_interval_geometry() {
        let mut project = Project::new();
        let line_id = project.add_line(100, 100, 0.25);
        project.get_line_mut(line_id).unwrap().text = "abcdefghij".to_string();
        for (id, frame, index) in [(1, 130, 3), (2, 170, 7)] {
            let cue = DetectionCue {
                id: DetectionCueId(id),
                kind: DetectionKind::TextSyncPoint,
                media_tick: MediaTick::from_frame(frame),
                target: TextAnchor::Grapheme { index },
            };
            assert!(project
                .detections_mut()
                .insert_detection(
                    DetectionAddress {
                        line_id,
                        detection_id: cue.id,
                    },
                    cue,
                ));
        }

        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
        };
        let line = project.get_line(line_id).unwrap();
        let line_rect = legacy::line_rect(&project, line, 150.0, &zone);
        let state = RythmoState::new();
        let mut stretched = Vec::new();
        let cursor_segments = legacy::render_sync_text_segments(
            &project,
            line,
            150.0,
            &zone,
            None,
            "fr",
            &state,
            None,
            [1.0; 4],
            &mut stretched,
        )
        .expect("synchronization points must segment both text and caret");

        assert_eq!(stretched.len(), 3);
        assert_eq!(cursor_segments.len(), 3);
        let expected = [(0.0, 0.3), (0.3, 0.4), (0.7, 0.3)];
        for (text, (start, width)) in stretched.iter().zip(expected) {
            assert!((text.dest_rect.x - (line_rect.x + line_rect.width * start)).abs() < 0.01);
            assert!((text.dest_rect.width - line_rect.width * width).abs() < 0.01);
        }
    }
}
