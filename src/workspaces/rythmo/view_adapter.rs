//! Output adapter for the legacy rythmo view.
//!
//! The underlying view still owns layout and drawing. This adapter applies the
//! stable character-label policies to the completed draw lists so text, its two
//! underlines and its note icon disappear together.

use std::collections::{HashMap, HashSet};

use crate::lint::Severity;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{IconInstance, LabelInfo, QuadInstance, Rect};
use crate::ui::renderer::StretchedText;

#[allow(hidden_glob_reexports)]
pub use super::view_implementation::*;

const CHARACTER_LABEL_CACHE_XOR: u64 = 0x4348_4152_4143_5445;
const RECT_EPSILON: f32 = 0.05;

type CursorInfo = Option<(
    u64,
    usize,
    Option<(usize, usize)>,
    f32,
    f32,
    f32,
    f32,
    Option<Vec<CursorSegmentInfo>>,
)>;

#[allow(clippy::too_many_arguments)]
pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    lint_severities: &HashMap<u64, Severity>,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
    detection_uvs: [[f32; 4]; 18],
) -> CursorInfo {
    let quad_start = quads.len();
    let stretched_start = stretched.len();
    let note_icon_start = note_icons.len();

    let result = super::view_implementation::render_lines(
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        lint_severities,
        quads,
        syllable_quads,
        labels,
        stretched,
        note_icons,
        actor_icons,
        note_uv,
        detection_uvs,
    );

    let mut hidden_line_ids: HashSet<u64> = project
        .lines()
        .filter(|line| {
            line.kind.is_dialogue()
                && !line.character_name.is_empty()
                // Karaoke playback uses centered stacked rows rather than the
                // ordinary scrolling badge geometry. Its visibility is handled
                // below by the per-track singer-continuity rule.
                && (!karaoke_preview || !line.karaoke)
                && super::badge_policy::stable_character_badge_layout(
                    project,
                    line,
                    current_frame,
                    zone,
                )
                .0
        })
        .map(|line| line.id)
        .collect();

    if karaoke_preview {
        hidden_line_ids.extend(
            project
                .lines()
                .filter(|line| {
                    line.karaoke
                        && line.kind.is_dialogue()
                        && !super::badge_policy::karaoke_character_label_visible(project, line)
                })
                .map(|line| line.id),
        );
    }

    if hidden_line_ids.is_empty() {
        return result;
    }

    // Build the geometry list before deleting the matching text entries.
    let hidden_badges: Vec<(Rect, [f32; 4])> = stretched[stretched_start..]
        .iter()
        .filter_map(|text| {
            hidden_character_label(project, &hidden_line_ids, text)
                .then_some((text.dest_rect, text.tint))
        })
        .collect();

    let mut added_text = stretched.split_off(stretched_start);
    added_text.retain(|text| !hidden_character_label(project, &hidden_line_ids, text));
    stretched.extend(added_text);

    let mut added_quads = quads.split_off(quad_start);
    added_quads.retain(|quad| {
        !hidden_badges
            .iter()
            .any(|(badge, tint)| is_badge_underline(quad, *badge, *tint))
    });
    quads.extend(added_quads);

    let mut added_note_icons = note_icons.split_off(note_icon_start);
    added_note_icons.retain(|icon| {
        icon.uv_rect != note_uv
            || !hidden_badges
                .iter()
                .any(|(badge, _)| array_rect_inside(icon.rect, *badge))
    });
    note_icons.extend(added_note_icons);

    result
}

fn hidden_character_label(
    project: &Project,
    hidden_line_ids: &HashSet<u64>,
    text: &StretchedText,
) -> bool {
    if !text.emphasized {
        return false;
    }
    let line_id = text.line_id ^ CHARACTER_LABEL_CACHE_XOR;
    hidden_line_ids.contains(&line_id)
        && project
            .get_line(line_id)
            .is_some_and(|line| line.character_name.as_str() == text.text.as_str())
}

fn is_badge_underline(quad: &QuadInstance, badge: Rect, tint: [f32; 4]) -> bool {
    quad.color == tint
        && quad.color_bottom == tint
        && quad.border_width == 0.0
        && quad.rect[3] <= 2.0 + RECT_EPSILON
        && array_rect_inside(quad.rect, badge)
}

fn array_rect_inside(rect: [f32; 4], container: Rect) -> bool {
    rect[0] >= container.x - RECT_EPSILON
        && rect[1] >= container.y - RECT_EPSILON
        && rect[0] + rect[2] <= container.x + container.width + RECT_EPSILON
        && rect[1] + rect[3] <= container.y + container.height + RECT_EPSILON
}

#[cfg(test)]
mod tests {
    use super::{array_rect_inside, is_badge_underline};
    use crate::ui::primitives::{QuadInstance, Rect};

    #[test]
    fn only_badge_underlines_are_removed_from_the_badge_rectangle() {
        let badge = Rect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 24.0,
        };
        let tint = [0.4, 0.7, 0.9, 1.0];
        let underline = QuadInstance {
            rect: [14.0, 40.0, 50.0, 1.5],
            color: tint,
            color_bottom: tint,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        };
        let line_body = QuadInstance {
            rect: [14.0, 25.0, 50.0, 12.0],
            ..underline
        };

        assert!(is_badge_underline(&underline, badge, tint));
        assert!(!is_badge_underline(&line_body, badge, tint));
        assert!(array_rect_inside(underline.rect, badge));
    }
}
