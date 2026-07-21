//! Rythmo view facade.
//!
//! The established renderer remains the source of geometry and text data. This
//! boundary removes syllable authoring chrome from ordinary adaptation lines;
//! karaoke lines keep their handles unchanged.

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
        let quad = &syllable_quads[index];
        let (x, y) = quad_center(quad);
        let belongs_to_normal_line = normal_rects.iter().any(|rect| rect.contains(x, y));
        if is_syllable_handle(quad) && belongs_to_normal_line {
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

    fn red_handle(rect: [f32; 4]) -> QuadInstance {
        QuadInstance {
            rect,
            color: [0.95, 0.08, 0.03, 1.0],
            color_bottom: [0.95, 0.08, 0.03, 1.0],
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
    fn handle_classifier_only_accepts_red_syllable_chrome() {
        assert!(is_syllable_handle(&red_handle([0.0, 0.0, 10.0, 3.0])));
        let mut other = red_handle([0.0, 0.0, 10.0, 3.0]);
        other.color = [0.48, 0.72, 1.0, 1.0];
        assert!(!is_syllable_handle(&other));
    }
}
