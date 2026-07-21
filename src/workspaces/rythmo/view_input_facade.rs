//! Single-click synchronization-point interaction facade.
//!
//! The rendering facade owns the fitted text boxes. This layer reuses the same
//! segment geometry for hit-testing so hover, click and click-drag always snap
//! to an actual displayed glyph, including after one or more points exist.

#[path = "view_facade.rs"]
mod base;

pub use base::*;

use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCueId, DetectionKind, LineDetectionData, MediaTick,
    TextAnchor,
};
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::{
    EventResponse, IconInstance, LabelInfo, QuadInstance, Rect, UiAction, UiEvent,
};
use crate::ui::ToolMode;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};

const DRAG_THRESHOLD: f32 = 4.0;
const SYNC_DOT_SIZE: f32 = 6.0;
const SYNC_DOT_HIT_PADDING: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct LetterAnchor {
    line_id: u64,
    boundary_index: usize,
    media_tick: MediaTick,
    x: f32,
    line_rect: Rect,
}

#[derive(Clone, Copy)]
struct PendingCreationDrag {
    address: DetectionAddress,
    start_x: f32,
    start_y: f32,
    moved: bool,
}

fn pending_creation_slot() -> &'static Mutex<Option<PendingCreationDrag>> {
    static SLOT: OnceLock<Mutex<Option<PendingCreationDrag>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn pending_creation() -> std::sync::MutexGuard<'static, Option<PendingCreationDrag>> {
    pending_creation_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

fn normal_line_at<'a>(
    project: &'a Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project.lines().find(|line| {
        !line.karaoke && base::line_rect(project, line, current_frame, zone).contains(x, y)
    })
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

fn letter_anchor_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    x: f32,
) -> Option<LetterAnchor> {
    if line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }

    let line_rect = base::line_rect(project, line, current_frame, zone);
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
            return Some(LetterAnchor {
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

fn letter_anchor_at(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<LetterAnchor> {
    let line = normal_line_at(project, current_frame, zone, x, y)?;
    letter_anchor_for_line(project, line, current_frame, zone, x)
}

fn near_existing_sync_x(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    line_id: u64,
    x: f32,
) -> bool {
    project.detections().line(line_id).is_some_and(|data| {
        data.text_sync_cues().any(|cue| {
            (tick_x(cue.media_tick, current_frame, zone) - x).abs()
                <= SYNC_DOT_SIZE / 2.0 + SYNC_DOT_HIT_PADDING
        })
    })
}

fn next_detection_address(project: &Project, line_id: u64) -> Option<DetectionAddress> {
    let detection_id = project
        .detections()
        .line(line_id)
        .map(LineDetectionData::next_detection_id)
        .unwrap_or(Some(DetectionCueId(1)))?;
    Some(DetectionAddress {
        line_id,
        detection_id,
    })
}

fn clamp_sync_tick(project: &Project, address: DetectionAddress, tick: MediaTick) -> MediaTick {
    let Some(line) = project.get_line(address.line_id) else {
        return tick;
    };
    let Some(data) = project.detections().line(address.line_id) else {
        return tick.clamp(
            MediaTick::from_frame(line.start_frame),
            MediaTick::from_frame(line.end_frame()),
        );
    };
    let Some(current) = data.detection(address.detection_id) else {
        return tick;
    };
    let Some(current_index) = current.target.grapheme_index() else {
        return tick;
    };

    let mut minimum = MediaTick::from_frame(line.start_frame);
    let mut maximum = MediaTick::from_frame(line.end_frame());
    for cue in data.text_sync_cues() {
        if cue.id == address.detection_id {
            continue;
        }
        let Some(index) = cue.target.grapheme_index() else {
            continue;
        };
        if index < current_index {
            minimum = MediaTick(minimum.raw().max(cue.media_tick.raw().saturating_add(1)));
        } else if index > current_index {
            maximum = MediaTick(maximum.raw().min(cue.media_tick.raw().saturating_sub(1)));
        }
    }
    tick.clamp(minimum, maximum)
}

fn handle_pending_creation_drag(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
) -> Option<EventResponse> {
    match event {
        UiEvent::MouseMove { x, y } => {
            let mut slot = pending_creation();
            let drag = slot.as_mut()?;
            if !drag.moved {
                let dx = *x - drag.start_x;
                let dy = *y - drag.start_y;
                drag.moved = dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD;
            }
            if drag.moved {
                let address = drag.address;
                let media_tick = clamp_sync_tick(
                    project,
                    address,
                    pointer_tick(*x, current_frame, zone),
                );
                drop(slot);
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address,
                    media_tick,
                }));
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::MouseRelease { .. } => {
            pending_creation().take().map(|_| EventResponse::Consumed)
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            pending_creation().take();
            None
        }
        _ => None,
    }
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
    if let Some(response) = handle_pending_creation_drag(event, project, current_frame, zone) {
        return response;
    }

    if interaction_mode == RythmoInteractionMode::Editable && active_mode != ToolMode::Draw {
        if let UiEvent::MousePress { x, y } = event {
            let can_create = state.detection_menu.is_none()
                && state.editing_character.is_none()
                && !state.audio_offset_mode
                && !state.panning
                && state.syllable_drag.is_none();
            if can_create {
                if let Some(anchor) = letter_anchor_at(project, current_frame, zone, *x, *y) {
                    if !near_existing_sync_x(
                        project,
                        current_frame,
                        zone,
                        anchor.line_id,
                        *x,
                    ) {
                        if let Some(address) = next_detection_address(project, anchor.line_id) {
                            state.dragging = None;
                            state.detection_menu = None;
                            state.detection_drag = None;
                            state.selected = Some(Selection::Detection(address));
                            *pending_creation() = Some(PendingCreationDrag {
                                address,
                                start_x: *x,
                                start_y: *y,
                                moved: false,
                            });
                            return EventResponse::Action(UiAction::AddDetection {
                                line_id: anchor.line_id,
                                kind: DetectionKind::TextSyncPoint,
                                media_tick: anchor.media_tick,
                                target: TextAnchor::Grapheme {
                                    index: anchor.boundary_index as u32,
                                },
                            });
                        }
                    }
                }
            }
        }

        // A double-click is no longer a second creation gesture. Its first
        // regular press already created (or selected) the point.
        if let UiEvent::DoubleClick { x, y } = event {
            if normal_line_at(project, current_frame, zone, *x, *y).is_some() {
                return EventResponse::Consumed;
            }
        }
    }

    base::handle_rythmo_event(
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

fn push_hover_dot(quads: &mut Vec<QuadInstance>, anchor: LetterAnchor) {
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
    base::render_detection_overlay(
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
            if let Some(anchor) = letter_anchor_at(
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
    use crate::detection::{DetectionCue, DetectionCueId};

    fn add_sync(project: &mut Project, line_id: u64, id: u64, index: u32, frame: i64) {
        let cue = DetectionCue {
            id: DetectionCueId(id),
            kind: DetectionKind::TextSyncPoint,
            media_tick: MediaTick::from_frame(frame),
            target: TextAnchor::Grapheme { index },
        };
        let address = DetectionAddress {
            line_id,
            detection_id: cue.id,
        };
        assert!(project.apply_detection_change(
            &DetectionChange::Add { address, cue },
            true,
        ));
    }

    #[test]
    fn letter_snap_remains_available_inside_a_line_that_already_has_a_point() {
        let mut project = Project::new();
        let line_id = project.add_line_full(
            100,
            100,
            0.0,
            "Bonjour à tous".into(),
            String::new(),
            [1.0; 4],
        );
        add_sync(&mut project, line_id, 1, 7, 140);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 1200.0,
            height: 800.0,
        };
        let line = project.get_line(line_id).unwrap();
        let segment = " à tous";
        let ratios = character_ratios(segment);
        let start_x = tick_x(MediaTick::from_frame(140), 150.0, &zone);
        let end_x = tick_x(MediaTick::from_frame(200), 150.0, &zone);
        let x_ratio = (ratios[1] + ratios[2]) * 0.5;
        let x = start_x + (end_x - start_x) * x_ratio;

        let anchor = letter_anchor_for_line(&project, line, 150.0, &zone, x).unwrap();
        assert_eq!(anchor.boundary_index, 8, "the `à` still snaps after the first cut");
        assert!(anchor.media_tick > MediaTick::from_frame(140));
        assert!(anchor.media_tick < MediaTick::from_frame(200));
    }

    #[test]
    fn creation_action_uses_the_snapped_boundary_and_starts_at_that_tick() {
        let mut project = Project::new();
        let line_id = project.add_line_full(
            0,
            100,
            0.0,
            "Bonjour à tous".into(),
            String::new(),
            [1.0; 4],
        );
        let address = next_detection_address(&project, line_id).unwrap();
        assert_eq!(address.detection_id, DetectionCueId(1));

        let anchor = LetterAnchor {
            line_id,
            boundary_index: 7,
            media_tick: MediaTick::from_frame(40),
            x: 10.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
        };
        let action = UiAction::AddDetection {
            line_id: anchor.line_id,
            kind: DetectionKind::TextSyncPoint,
            media_tick: anchor.media_tick,
            target: TextAnchor::Grapheme {
                index: anchor.boundary_index as u32,
            },
        };
        assert!(matches!(
            action,
            UiAction::AddDetection {
                line_id: id,
                media_tick,
                target: TextAnchor::Grapheme { index: 7 },
                ..
            } if id == line_id && media_tick == MediaTick::from_frame(40)
        ));
    }
}
