//! Output adapter for the legacy rythmo view.
//!
//! The underlying view still owns layout and drawing. This adapter applies the
//! stable character-label policies and the global playhead origin to completed
//! draw lists without changing media timing.

use std::collections::{HashMap, HashSet};

use crate::lint::Severity;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{
    EventResponse, IconInstance, LabelInfo, QuadInstance, Rect, UiEvent,
};
use crate::ui::renderer::StretchedText;
use crate::ui::ToolMode;

#[allow(hidden_glob_reexports)]
pub use super::view_implementation::*;

const CHARACTER_LABEL_CACHE_XOR: u64 = 0x4348_4152_4143_5445;
const KARAOKE_TEXT_CACHE_XOR: u64 = 1_u64 << 62;
const RECT_EPSILON: f32 = 0.05;
const PLAYHEAD_WIDTH: f32 = 3.0;
const PLAYHEAD_COLOR: [f32; 4] = [1.0, 0.02, 0.05, 1.0];

fn playhead_delta(zone: &Rect) -> f32 {
    crate::config::playhead_delta_pixels(zone.width)
}

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
    let syllable_quad_start = syllable_quads.len();
    let label_start = labels.len();
    let stretched_start = stretched.len();
    let note_icon_start = note_icons.len();
    let actor_icon_start = actor_icons.len();

    let mut result = super::view_implementation::render_lines(
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

    // Only labels emitted by this render pass need a decision. Their collision
    // targets still cover the complete project, which keeps the result stable
    // when another line is culled outside the viewport.
    let rendered_character_line_ids: HashSet<u64> = stretched[stretched_start..]
        .iter()
        .filter_map(|text| character_label_line_id(project, text))
        .collect();

    if !rendered_character_line_ids.is_empty() {
        let collision_layout =
            super::badge_policy::CharacterBadgeLayoutContext::new(project, current_frame, zone);
        let mut hidden_line_ids = HashSet::new();
        for line_id in rendered_character_line_ids {
            let Some(line) = project.get_line(line_id) else {
                continue;
            };

            let hidden = if karaoke_preview && line.karaoke {
                !super::badge_policy::karaoke_character_label_visible(project, line)
            } else {
                line.kind.is_dialogue() && collision_layout.badge_layout(line).0
            };
            if hidden {
                hidden_line_ids.insert(line_id);
            }
        }

        if !hidden_line_ids.is_empty() {
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
        }
    }

    let delta = playhead_delta(zone);
    if delta.abs() > f32::EPSILON {
        let center_x = zone.x + zone.width * 0.5;
        let centered_karaoke_ids: HashSet<u64> = stretched[stretched_start..]
            .iter()
            .filter_map(|text| {
                let line_id = text.line_id ^ KARAOKE_TEXT_CACHE_XOR;
                let line = project.get_line(line_id)?;
                let text_center = text.dest_rect.x + text.dest_rect.width * 0.5;
                (line.karaoke && (text_center - center_x).abs() <= 1.5).then_some(line_id)
            })
            .collect();
        let centered_karaoke_rects: Vec<Rect> = stretched[stretched_start..]
            .iter()
            .filter_map(|text| {
                let line_id = text.line_id ^ KARAOKE_TEXT_CACHE_XOR;
                centered_karaoke_ids
                    .contains(&line_id)
                    .then_some(text.dest_rect)
            })
            .collect();

        for quad in &mut quads[quad_start..] {
            if !quad_belongs_to_centered_karaoke(quad, &centered_karaoke_rects) {
                quad.rect[0] += delta;
            }
        }
        for quad in &mut syllable_quads[syllable_quad_start..] {
            quad.rect[0] += delta;
        }
        for label in &mut labels[label_start..] {
            label.bounds.x += delta;
        }
        for text in &mut stretched[stretched_start..] {
            if !text_belongs_to_centered_karaoke(project, text, &centered_karaoke_ids) {
                text.dest_rect.x += delta;
                text.draw_rect.x += delta;
            }
        }
        for icon in &mut note_icons[note_icon_start..] {
            icon.rect[0] += delta;
        }
        for icon in &mut actor_icons[actor_icon_start..] {
            icon.rect.x += delta;
        }

        if let Some((_, _, _, text_x, _, _, _, _)) = result.as_mut() {
            *text_x += delta;
        }
    }

    result
}

#[allow(clippy::too_many_arguments)]
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
    let mut quads = super::view_implementation::render_rythmo_base(
        zone,
        project,
        current_frame,
        waveform,
        waveform_offset_frames,
        waveform_is_instrumental,
        karaoke_preview,
        fps,
        state,
        scene,
    );

    let delta = playhead_delta(zone);
    if delta.abs() <= f32::EPSILON {
        return quads;
    }

    let centered_playhead_x = zone.x + (zone.width - PLAYHEAD_WIDTH) * 0.5;
    let mut original_segments: Vec<(f32, f32)> = quads
        .iter()
        .filter(|quad| is_playhead_quad(quad, centered_playhead_x))
        .map(|quad| (quad.rect[1], quad.rect[1] + quad.rect[3]))
        .collect();
    original_segments.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let original_gaps = complement_ranges(zone.y, zone.y + zone.height, &original_segments);

    quads.retain(|quad| !is_playhead_quad(quad, centered_playhead_x));
    for quad in &mut quads {
        let covers_zone = quad.rect[2] >= zone.width - 0.5
            && quad.rect[0] <= zone.x + 0.5
            && quad.rect[0] + quad.rect[2] >= zone.x + zone.width - 0.5;
        if !covers_zone {
            quad.rect[0] += delta;
        }
    }

    let playhead_x = crate::config::playhead_x(zone.x, zone.width, PLAYHEAD_WIDTH);
    let overlaps_centered_karaoke = scene.lines.iter().any(|scene_line| {
        if !scene_line.karaoke_active && scene_line.karaoke_count_in_progress.is_none() {
            return false;
        }
        let width = state.karaoke_ui_text_width_for_render(&scene_line.line);
        let left = zone.x + (zone.width - width) * 0.5;
        playhead_x + PLAYHEAD_WIDTH > left && playhead_x < left + width
    });
    let gaps = if overlaps_centered_karaoke {
        original_gaps.as_slice()
    } else {
        &[]
    };
    push_playhead_segments(&mut quads, playhead_x, zone.y, zone.height, gaps);
    quads
}

#[allow(clippy::too_many_arguments)]
pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
    tool_mode: ToolMode,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
    interaction_mode: RythmoInteractionMode,
) -> EventResponse {
    let adjusted = event_with_shifted_x(event, -playhead_delta(zone));
    super::view_implementation::handle_rythmo_event(
        &adjusted,
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        tool_mode,
        brush_color,
        brush_radius_frac,
        erasing,
        interaction_mode,
    )
}

pub fn handle_context_menu_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
) -> EventResponse {
    let adjusted = event_with_shifted_x(event, -playhead_delta(zone));
    super::view_implementation::handle_context_menu_event(
        &adjusted,
        project,
        current_frame,
        zone,
        screen_w,
        screen_h,
        state,
    )
}

fn event_with_shifted_x(event: &UiEvent, delta: f32) -> UiEvent {
    match event {
        UiEvent::MouseMove { x, y } => UiEvent::MouseMove { x: x + delta, y: *y },
        UiEvent::MousePress { x, y } => UiEvent::MousePress { x: x + delta, y: *y },
        UiEvent::MouseRelease { x, y } => UiEvent::MouseRelease { x: x + delta, y: *y },
        UiEvent::CtrlClick { x, y } => UiEvent::CtrlClick { x: x + delta, y: *y },
        UiEvent::ShiftMousePress { x, y } => UiEvent::ShiftMousePress { x: x + delta, y: *y },
        UiEvent::DoubleClick { x, y } => UiEvent::DoubleClick { x: x + delta, y: *y },
        UiEvent::MiddlePress { x, y } => UiEvent::MiddlePress { x: x + delta, y: *y },
        UiEvent::MiddleRelease { x, y } => UiEvent::MiddleRelease { x: x + delta, y: *y },
        UiEvent::ContextMenu { x, y } => UiEvent::ContextMenu { x: x + delta, y: *y },
        UiEvent::Scroll {
            x,
            y,
            delta: scroll_delta,
            fast,
            ctrl,
        } => UiEvent::Scroll {
            x: x + delta,
            y: *y,
            delta: *scroll_delta,
            fast: *fast,
            ctrl: *ctrl,
        },
        other => other.clone(),
    }
}

fn text_belongs_to_centered_karaoke(
    project: &Project,
    text: &StretchedText,
    centered_ids: &HashSet<u64>,
) -> bool {
    let karaoke_id = text.line_id ^ KARAOKE_TEXT_CACHE_XOR;
    if centered_ids.contains(&karaoke_id) {
        return true;
    }
    character_label_line_id(project, text).is_some_and(|line_id| centered_ids.contains(&line_id))
}

fn quad_belongs_to_centered_karaoke(quad: &QuadInstance, rects: &[Rect]) -> bool {
    let cx = quad.rect[0] + quad.rect[2] * 0.5;
    let cy = quad.rect[1] + quad.rect[3] * 0.5;
    rects.iter().any(|rect| {
        cy >= rect.y - 4.0
            && cy <= rect.y + rect.height + 4.0
            && cx >= rect.x - 140.0
            && cx <= rect.x + rect.width + 24.0
    })
}

fn is_playhead_quad(quad: &QuadInstance, centered_x: f32) -> bool {
    quad.color == PLAYHEAD_COLOR
        && quad.color_bottom == PLAYHEAD_COLOR
        && (quad.rect[0] - centered_x).abs() <= 0.1
        && (quad.rect[2] - PLAYHEAD_WIDTH).abs() <= 0.1
}

fn complement_ranges(start: f32, end: f32, segments: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut gaps = Vec::new();
    let mut cursor = start;
    for &(segment_start, segment_end) in segments {
        if segment_start > cursor {
            gaps.push((cursor, segment_start));
        }
        cursor = cursor.max(segment_end);
    }
    if cursor < end {
        gaps.push((cursor, end));
    }
    gaps
}

fn push_playhead_segments(
    quads: &mut Vec<QuadInstance>,
    x: f32,
    y: f32,
    height: f32,
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
            quads.push(playhead_quad(x, cursor_y, skip_start - cursor_y));
        }
        cursor_y = cursor_y.max(skip_end);
    }
    let end_y = y + height;
    if cursor_y < end_y {
        quads.push(playhead_quad(x, cursor_y, end_y - cursor_y));
    }
}

fn playhead_quad(x: f32, y: f32, height: f32) -> QuadInstance {
    QuadInstance {
        rect: [x, y, PLAYHEAD_WIDTH, height],
        color: PLAYHEAD_COLOR,
        color_bottom: PLAYHEAD_COLOR,
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

fn character_label_line_id(project: &Project, text: &StretchedText) -> Option<u64> {
    if !text.emphasized {
        return None;
    }
    let line_id = text.line_id ^ CHARACTER_LABEL_CACHE_XOR;
    project
        .get_line(line_id)
        .filter(|line| line.kind.is_dialogue() && line.character_name.as_str() == text.text.as_str())
        .map(|line| line.id)
}

fn hidden_character_label(
    project: &Project,
    hidden_line_ids: &HashSet<u64>,
    text: &StretchedText,
) -> bool {
    character_label_line_id(project, text).is_some_and(|line_id| hidden_line_ids.contains(&line_id))
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
    use super::{array_rect_inside, complement_ranges, is_badge_underline};
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

    #[test]
    fn playhead_gaps_are_the_complement_of_rendered_segments() {
        assert_eq!(
            complement_ranges(0.0, 100.0, &[(0.0, 20.0), (40.0, 100.0)]),
            vec![(20.0, 40.0)]
        );
    }
}
