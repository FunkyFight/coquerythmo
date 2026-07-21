//! Focused view facade for the rythmo workspace.
//!
//! Ordinary lines keep the historical fit-to-line rendering. Synchronization
//! points split that fitted text into independent time boxes; every box is
//! stretched only between its own two temporal boundaries.

#[path = "view.rs"]
mod legacy;

pub use legacy::*;

use crate::detection::{
    track_storage_line_id, DetectionKind, LineDetectionData, MediaTick, TextAnchor,
};
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{
    EventResponse, IconInstance, LabelInfo, QuadInstance, Rect, UiAction, UiEvent,
};
use crate::ui::renderer::StretchedText;
use crate::ui::ToolMode;
use std::collections::{BTreeMap, HashSet};

const SYNC_DOT_SIZE: f32 = 6.0;
const SYNC_DOT_HIT_PADDING: f32 = 4.0;
const SOURCE_SIGN_SIZE: f32 = 26.0;
const SOURCE_SIGN_BOTTOM_MARGIN: f32 = 2.0;
const SOURCE_SIGN_DISPLAY_DROP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct SyncBoundaryAnchor {
    line_id: u64,
    boundary_index: usize,
    media_tick: MediaTick,
    x: f32,
    line_rect: Rect,
}

fn ppf() -> f32 {
    crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf().max(0.001)) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

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

fn sync_boundaries(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Vec<(usize, MediaTick)> {
    let character_count = line.text.chars().count();
    if line.karaoke || character_count == 0 || line.duration_frames <= 0 {
        return Vec::new();
    }

    let line_start = MediaTick::from_frame(line.start_frame);
    let line_end = MediaTick::from_frame(line.end_frame());
    let mut interior = BTreeMap::<usize, MediaTick>::new();
    if let Some(data) = project.detections().line(line.id) {
        for cue in data.text_sync_cues() {
            let Some(index) = cue.target.grapheme_index().map(|index| index as usize) else {
                continue;
            };
            if index == 0 || index >= character_count {
                continue;
            }
            interior.insert(index, cue.media_tick.clamp(line_start, line_end));
        }
    }

    let mut boundaries = Vec::with_capacity(interior.len() + 2);
    boundaries.push((0, line_start));
    let mut previous_tick = line_start;
    for (index, tick) in interior {
        let tick = tick.clamp(
            MediaTick(previous_tick.raw().saturating_add(1)),
            line_end,
        );
        previous_tick = tick;
        boundaries.push((index, tick));
    }
    boundaries.push((character_count, line_end));

    for index in (0..boundaries.len().saturating_sub(1)).rev() {
        let maximum = MediaTick(boundaries[index + 1].1.raw().saturating_sub(1));
        boundaries[index].1 = boundaries[index].1.clamp(line_start, maximum);
    }
    boundaries
}

fn character_ratios(text: &str) -> Vec<f32> {
    let font_size = crate::config::get().ui.font_size * 2.0;
    crate::vector_text::measure_rythmo_text_char_ratios_standalone(text, font_size)
        .filter(|ratios| ratios.len() == text.chars().count() + 1)
        .unwrap_or_else(|| {
            let count = text.chars().count().max(1);
            (0..=count)
                .map(|index| index as f32 / count as f32)
                .collect()
        })
}

fn normal_line_at<'a>(
    project: &'a Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project.lines().find(|line| {
        !line.karaoke && legacy::line_rect(project, line, current_frame, zone).contains(x, y)
    })
}

fn choose_boundary_for_character(
    characters: &[char],
    character_index: usize,
    local_position: f32,
    segment_start: usize,
    segment_end: usize,
    existing: &HashSet<usize>,
) -> Option<usize> {
    let before = character_index;
    let after = character_index.saturating_add(1);
    let previous_is_space = character_index > 0 && characters[character_index - 1].is_whitespace();
    let next_is_space = characters
        .get(character_index + 1)
        .is_some_and(|character| character.is_whitespace());

    let mut candidates = Vec::with_capacity(4);
    if previous_is_space {
        candidates.push(before);
    }
    if next_is_space {
        candidates.push(after);
    }
    if local_position <= 0.5 {
        candidates.extend([before, after]);
    } else {
        candidates.extend([after, before]);
    }

    candidates.into_iter().find(|boundary| {
        *boundary > segment_start
            && *boundary < segment_end
            && *boundary > 0
            && *boundary < characters.len()
            && !existing.contains(boundary)
    })
}

fn sync_boundary_anchor_at(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<SyncBoundaryAnchor> {
    let line = normal_line_at(project, current_frame, zone, x, y)?;
    if line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }

    let line_rect = legacy::line_rect(project, line, current_frame, zone);
    let characters = line.text.chars().collect::<Vec<_>>();
    let boundaries = sync_boundaries(project, line);
    let existing = boundaries
        .iter()
        .skip(1)
        .take(boundaries.len().saturating_sub(2))
        .map(|(index, _)| *index)
        .collect::<HashSet<_>>();

    for pair in boundaries.windows(2) {
        let (segment_start, start_tick) = pair[0];
        let (segment_end, end_tick) = pair[1];
        if segment_end <= segment_start || segment_end > characters.len() || end_tick <= start_tick {
            continue;
        }
        let start_x = tick_x(start_tick, current_frame, zone);
        let end_x = tick_x(end_tick, current_frame, zone);
        if x < start_x.min(end_x) || x > start_x.max(end_x) {
            continue;
        }

        let text = characters[segment_start..segment_end]
            .iter()
            .collect::<String>();
        let ratios = character_ratios(&text);
        let width = (end_x - start_x).abs().max(0.001);
        let x_ratio = ((x - start_x) / width).clamp(0.0, 1.0);
        for local_index in 0..segment_end.saturating_sub(segment_start) {
            let character_index = segment_start + local_index;
            if characters[character_index].is_whitespace() {
                continue;
            }
            let left = ratios[local_index];
            let right = ratios[local_index + 1];
            if x_ratio < left.min(right) || x_ratio > left.max(right) {
                continue;
            }
            let glyph_width = (right - left).abs().max(0.000_001);
            let local_position = ((x_ratio - left) / glyph_width).clamp(0.0, 1.0);
            let boundary_index = choose_boundary_for_character(
                &characters,
                character_index,
                local_position,
                segment_start,
                segment_end,
                &existing,
            )?;
            let local_boundary = boundary_index.saturating_sub(segment_start);
            let boundary_ratio = ratios
                .get(local_boundary)
                .copied()
                .unwrap_or(local_boundary as f32 / text.chars().count().max(1) as f32)
                .clamp(0.0, 1.0);
            let duration = end_tick.raw().saturating_sub(start_tick.raw()).max(1);
            let media_tick = MediaTick(
                start_tick
                    .raw()
                    .saturating_add((duration as f64 * boundary_ratio as f64).round() as i64),
            )
            .clamp(
                MediaTick(start_tick.raw().saturating_add(1)),
                MediaTick(end_tick.raw().saturating_sub(1)),
            );
            return Some(SyncBoundaryAnchor {
                line_id: line.id,
                boundary_index,
                media_tick,
                x: tick_x(media_tick, current_frame, zone),
                line_rect,
            });
        }
    }
    None
}

fn sync_dot_rect(x: f32, line_rect: Rect) -> Rect {
    Rect {
        x: x - SYNC_DOT_SIZE / 2.0,
        y: line_rect.y + line_rect.height - SYNC_DOT_SIZE - 2.0,
        width: SYNC_DOT_SIZE,
        height: SYNC_DOT_SIZE,
    }
}

fn expanded_rect(rect: Rect, padding: f32) -> Rect {
    Rect {
        x: rect.x - padding,
        y: rect.y - padding,
        width: rect.width + padding * 2.0,
        height: rect.height + padding * 2.0,
    }
}

fn hit_existing_sync(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> bool {
    project.lines().filter(|line| !line.karaoke).any(|line| {
        let line_rect = legacy::line_rect(project, line, current_frame, zone);
        project.detections().line(line.id).is_some_and(|data| {
            data.text_sync_cues().any(|cue| {
                expanded_rect(
                    sync_dot_rect(tick_x(cue.media_tick, current_frame, zone), line_rect),
                    SYNC_DOT_HIT_PADDING,
                )
                .contains(x, y)
            })
        })
    })
}

fn hit_source_detection(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> bool {
    (0..crate::rythmo_layout::track_count()).any(|track| {
        let line_id = track_storage_line_id(track as u8);
        let Some(data) = project.detections().line(line_id) else {
            return false;
        };
        let track_rect = legacy::editor_track_body_rect_at_frame(
            project,
            crate::rythmo_layout::y_slot_for_track_index(track),
            current_frame,
            zone,
        );
        data.source_detections().any(|cue| {
            let center = tick_x(cue.media_tick, current_frame, zone);
            let base_y = (track_rect.y + track_rect.height - SOURCE_SIGN_SIZE - SOURCE_SIGN_BOTTOM_MARGIN)
                .max(track_rect.y);
            [base_y, base_y + SOURCE_SIGN_DISPLAY_DROP]
                .into_iter()
                .any(|badge_y| {
                    expanded_rect(
                        Rect {
                            x: center - SOURCE_SIGN_SIZE / 2.0,
                            y: badge_y,
                            width: SOURCE_SIGN_SIZE,
                            height: SOURCE_SIGN_SIZE,
                        },
                        3.0,
                    )
                    .contains(x, y)
                })
        })
    })
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

fn sync_foreground(
    project: &Project,
    state: &RythmoState,
    zone: &Rect,
    current_frame: f64,
    event: &UiEvent,
) {
    crate::detection_foreground::sync_from_state(project, state, *zone, current_frame, event);
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
    active_mode: ToolMode,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
    interaction_mode: RythmoInteractionMode,
) -> EventResponse {
    if interaction_mode == RythmoInteractionMode::Editable && active_mode != ToolMode::Draw {
        if let UiEvent::DoubleClick { x, y } = event {
            if !hit_existing_sync(project, current_frame, zone, *x, *y) {
                if let Some(anchor) = sync_boundary_anchor_at(project, current_frame, zone, *x, *y) {
                    state.dragging = None;
                    state.detection_menu = None;
                    state.detection_drag = None;
                    let response = EventResponse::Action(UiAction::AddDetection {
                        line_id: anchor.line_id,
                        kind: DetectionKind::TextSyncPoint,
                        media_tick: anchor.media_tick,
                        target: TextAnchor::Grapheme {
                            index: anchor.boundary_index as u32,
                        },
                    });
                    sync_foreground(project, state, zone, current_frame, event);
                    return response;
                }
            }
        }

        if let UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } = event {
            let plain_line_press = state.detection_menu.is_none()
                && state.editing_character.is_none()
                && !state.audio_offset_mode
                && !state.panning
                && state.syllable_drag.is_none()
                && normal_line_at(project, current_frame, zone, *x, *y).is_some()
                && !hit_existing_sync(project, current_frame, zone, *x, *y)
                && !hit_source_detection(project, current_frame, zone, *x, *y);
            if plain_line_press {
                let ctx = legacy::RythmoCtx {
                    zone,
                    project,
                    render_index,
                    current_frame,
                    karaoke_preview,
                    fps,
                    active_mode,
                };
                let response = if matches!(event, UiEvent::ShiftMousePress { .. }) {
                    legacy::handle_shift_mouse_press(&ctx, state, *x, *y)
                } else {
                    legacy::handle_mouse_press(&ctx, state, *x, *y)
                };
                sync_foreground(project, state, zone, current_frame, event);
                return response;
            }
        }
    }

    legacy::handle_rythmo_event(
        event,
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        active_mode,
        brush_color,
        brush_radius_frac,
        erasing,
        interaction_mode,
    )
}

fn push_hover_dot(quads: &mut Vec<QuadInstance>, anchor: SyncBoundaryAnchor) {
    let dot = sync_dot_rect(anchor.x, anchor.line_rect);
    quads.push(QuadInstance {
        rect: [dot.x, dot.y, dot.width, dot.height],
        color: [0.48, 0.72, 1.0, 0.45],
        color_bottom: [0.48, 0.72, 1.0, 0.45],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 8.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
    detection_uvs: [[f32; 4]; 7],
) {
    let first_quad = quads.len();
    legacy::render_detection_overlay(
        zone,
        project,
        current_frame,
        state,
        quads,
        labels,
        icons,
        detection_uvs,
    );

    let mut index = first_quad.min(quads.len());
    while index < quads.len() {
        if quads[index].color == [0.48, 0.72, 1.0, 0.45] {
            quads.remove(index);
        } else {
            index += 1;
        }
    }

    if state.detection_menu.is_none() {
        if let Some(hover) = state.detection_hover {
            if let Some(anchor) = sync_boundary_anchor_at(
                project,
                current_frame,
                zone,
                hover.screen_x,
                hover.screen_y,
            ) {
                push_hover_dot(quads, anchor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn word_start_anchors_before_the_letter_and_word_end_anchors_after_it() {
        let characters = "Bonjour à tous".chars().collect::<Vec<_>>();
        let existing = HashSet::new();
        let a_index = characters.iter().position(|character| *character == 'à').unwrap();
        let r_index = characters.iter().position(|character| *character == 'r').unwrap();

        assert_eq!(
            choose_boundary_for_character(
                &characters,
                a_index,
                0.5,
                0,
                characters.len(),
                &existing,
            ),
            Some(a_index),
            "a word-initial letter starts the following fitted box"
        );
        assert_eq!(
            choose_boundary_for_character(
                &characters,
                r_index,
                0.5,
                0,
                characters.len(),
                &existing,
            ),
            Some(r_index + 1),
            "a word-final letter remains in the preceding fitted box"
        );
    }

    #[test]
    fn an_existing_boundary_is_never_created_twice() {
        let characters = "bonjour".chars().collect::<Vec<_>>();
        let existing = HashSet::from([3]);
        assert_ne!(
            choose_boundary_for_character(&characters, 3, 0.1, 0, characters.len(), &existing),
            Some(3)
        );
    }
}
