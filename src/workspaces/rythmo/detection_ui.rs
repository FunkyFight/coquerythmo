//! Detection UI facade.
//!
//! Source signs keep the established detector menu and drag behaviour. Text
//! synchronization is implemented here as independent Mosaic-style intervals:
//! a point is an absolute time boundary attached to a grapheme and moving it
//! changes only the two neighbouring text segments.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionCueId, DetectionKind,
    LineDetectionData, MediaTick, TextAnchor,
};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[path = "detection_ui_base.rs"]
mod base;

pub use base::{DetectionDrag, DetectionHover, DetectionMenu};
pub(crate) use base::{
    decode_sync_syllable_drag_line_id, encode_sync_syllable_drag_line_id,
    line_has_visible_sync_points,
};

const SIGN_BADGE_SIZE: f32 = 26.0;
const SIGN_ICON_SIZE: f32 = 18.0;
const SIGN_BOTTOM_MARGIN: f32 = 2.0;
const SIGN_DISPLAY_DROP: f32 = 8.0;
const LEGACY_ADD_BUTTON_SIZE: f32 = 18.0;
const LEGACY_ADD_BUTTON_GAP: f32 = 4.0;
const DRAG_THRESHOLD: f32 = 4.0;
const SYNC_DOT_SIZE: f32 = 6.0;
const SYNC_DOT_HIT_PADDING: f32 = 4.0;

#[derive(Clone)]
struct SourceDrag {
    address: DetectionAddress,
    cue: DetectionCue,
    start_x: f32,
    start_y: f32,
    origin_track: u8,
    target_track: u8,
    origin_tick: MediaTick,
    target_tick: MediaTick,
    moved: bool,
    lock_x: bool,
}

#[derive(Clone, Copy)]
struct SyncDrag {
    address: DetectionAddress,
    start_x: f32,
    start_y: f32,
    moved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SyncPlaceholder {
    line_id: u64,
    character_index: usize,
    media_tick: MediaTick,
    x: f32,
    line_rect: Rect,
}

#[derive(Default)]
struct SyncInteraction {
    drag: Option<SyncDrag>,
    hover: Option<SyncPlaceholder>,
}

fn source_drag_slot() -> &'static Mutex<Option<SourceDrag>> {
    static SLOT: OnceLock<Mutex<Option<SourceDrag>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn source_drag() -> std::sync::MutexGuard<'static, Option<SourceDrag>> {
    source_drag_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn sync_interaction_slot() -> &'static Mutex<SyncInteraction> {
    static SLOT: OnceLock<Mutex<SyncInteraction>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(SyncInteraction::default()))
}

fn sync_interaction() -> std::sync::MutexGuard<'static, SyncInteraction> {
    sync_interaction_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn track_rect(project: &Project, track: usize, current_frame: f64, zone: &Rect) -> Rect {
    editor_track_body_rect_at_frame(
        project,
        rythmo_layout::y_slot_for_track_index(track),
        current_frame,
        zone,
    )
}

fn track_under_pointer(
    project: &Project,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<(u8, Rect)> {
    (0..rythmo_layout::track_count()).find_map(|track| {
        let rect = track_rect(project, track, current_frame, zone);
        (y >= rect.y && y <= rect.y + rect.height).then_some((track as u8, rect))
    })
}

fn legacy_add_button_rect(hover: &DetectionHover) -> Rect {
    Rect {
        x: hover.screen_x - LEGACY_ADD_BUTTON_SIZE / 2.0,
        y: hover.track_rect.y + hover.track_rect.height + LEGACY_ADD_BUTTON_GAP,
        width: LEGACY_ADD_BUTTON_SIZE,
        height: LEGACY_ADD_BUTTON_SIZE,
    }
}

fn sign_badge_rect(
    tick: MediaTick,
    track: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    Rect {
        x: tick_x(tick, current_frame, zone) - SIGN_BADGE_SIZE / 2.0,
        y: (track.y + track.height - SIGN_BADGE_SIZE - SIGN_BOTTOM_MARGIN).max(track.y),
        width: SIGN_BADGE_SIZE,
        height: SIGN_BADGE_SIZE,
    }
}

fn sign_icon_rect(tick: MediaTick, track: Rect, current_frame: f64, zone: &Rect) -> Rect {
    let badge = sign_badge_rect(tick, track, current_frame, zone);
    Rect {
        x: badge.x + (badge.width - SIGN_ICON_SIZE) / 2.0,
        y: badge.y + (badge.height - SIGN_ICON_SIZE) / 2.0,
        width: SIGN_ICON_SIZE,
        height: SIGN_ICON_SIZE,
    }
}

fn displayed_badge_rect(
    tick: MediaTick,
    track: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    let mut rect = sign_badge_rect(tick, track, current_frame, zone);
    rect.y += SIGN_DISPLAY_DROP;
    rect
}

fn displayed_icon_rect(
    tick: MediaTick,
    track: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    let badge = displayed_badge_rect(tick, track, current_frame, zone);
    Rect {
        x: badge.x + (badge.width - SIGN_ICON_SIZE) / 2.0,
        y: badge.y + (badge.height - SIGN_ICON_SIZE) / 2.0,
        width: SIGN_ICON_SIZE,
        height: SIGN_ICON_SIZE,
    }
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

fn palette_uv(cue: &DetectionCue, uvs: [[f32; 4]; 7]) -> [f32; 4] {
    let alternate = matches!(&cue.target, TextAnchor::AfterText);
    let index = match cue.kind {
        DetectionKind::Labial => Some(0),
        DetectionKind::SemiLabial => Some(1),
        DetectionKind::MouthOpen => Some(2),
        DetectionKind::MouthClosed => Some(3),
        DetectionKind::TeethVisible if !alternate => Some(4),
        DetectionKind::Breath if !alternate => Some(5),
        DetectionKind::Reaction => Some(6),
        DetectionKind::TeethVisible | DetectionKind::Breath => None,
        DetectionKind::TextSyncPoint => return [0.0; 4],
    };
    if let Some(index) = index {
        return uvs[index];
    }
    let cell_width = uvs[0][2] - uvs[0][0];
    let extra_index = if matches!(cue.kind, DetectionKind::TeethVisible) {
        0.0
    } else {
        1.0
    };
    let u_min = uvs[6][2] + cell_width * extra_index;
    [u_min, uvs[6][1], u_min + cell_width, uvs[6][3]]
}

fn selected_detection(state: &RythmoState) -> Option<DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
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

fn hit_source_detection(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<(DetectionAddress, DetectionCue)> {
    (0..rythmo_layout::track_count()).find_map(|track| {
        let line_id = track_storage_line_id(track as u8);
        let data = project.detections().line(line_id)?;
        let rect = track_rect(project, track, current_frame, zone);
        data.source_detections().find_map(|cue| {
            sign_badge_rect(cue.media_tick, rect, current_frame, zone)
                .contains(x, y)
                .then_some((
                    DetectionAddress {
                        line_id,
                        detection_id: cue.id,
                    },
                    cue.clone(),
                ))
        })
    })
}

fn begin_source_drag(
    address: DetectionAddress,
    cue: DetectionCue,
    x: f32,
    y: f32,
    lock_x: bool,
) {
    let Some(track) = address.track() else {
        return;
    };
    *source_drag() = Some(SourceDrag {
        address,
        start_x: x,
        start_y: y,
        origin_track: track,
        target_track: track,
        origin_tick: cue.media_tick,
        target_tick: cue.media_tick,
        cue,
        moved: false,
        lock_x,
    });
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

fn line_has_sync_points(project: &Project, line_id: u64) -> bool {
    project
        .detections()
        .line(line_id)
        .is_some_and(|data| data.text_sync_cues().next().is_some())
}

fn normal_line_under_pointer<'a>(
    project: &'a Project,
    x: f32,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project.lines().find(|line| {
        !line.karaoke && line_rect(project, line, current_frame, zone).contains(x, y)
    })
}

fn sync_placeholder_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    x: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<SyncPlaceholder> {
    let characters = line.text.chars().collect::<Vec<_>>();
    if characters.len() < 2 {
        return None;
    }
    let rect = line_rect(project, line, current_frame, zone);
    if x < rect.x || x > rect.x + rect.width || rect.width <= 0.0 {
        return None;
    }

    let pointer_tick = pointer_tick(x, current_frame, zone);
    let boundaries = sync_boundaries(project, line);
    for pair in boundaries.windows(2) {
        let (start_char, start_tick) = pair[0];
        let (end_char, end_tick) = pair[1];
        if end_char <= start_char + 1 || pointer_tick < start_tick || pointer_tick > end_tick {
            continue;
        }
        let duration = (end_tick.raw() - start_tick.raw()).max(1) as f64;
        let local = ((pointer_tick.raw() - start_tick.raw()) as f64 / duration).clamp(0.0, 1.0);
        let local_boundary = (local * (end_char - start_char) as f64).round() as usize;
        let mut candidate = (start_char + local_boundary)
            .clamp(start_char + 1, end_char.saturating_sub(1));

        if characters.get(candidate).is_some_and(|character| character.is_whitespace()) {
            let left = (start_char + 1..candidate)
                .rev()
                .find(|index| !characters[*index].is_whitespace());
            let right = (candidate + 1..end_char)
                .find(|index| !characters[*index].is_whitespace());
            candidate = match (left, right) {
                (Some(left), Some(right)) => {
                    if candidate - left <= right - candidate {
                        left
                    } else {
                        right
                    }
                }
                (Some(left), None) => left,
                (None, Some(right)) => right,
                (None, None) => return None,
            };
        }

        let already_exists = project.detections().line(line.id).is_some_and(|data| {
            data.text_sync_cues()
                .any(|cue| cue.target.grapheme_index() == Some(candidate as u32))
        });
        if already_exists {
            return None;
        }

        return Some(SyncPlaceholder {
            line_id: line.id,
            character_index: candidate,
            media_tick: pointer_tick.clamp(
                MediaTick(start_tick.raw().saturating_add(1)),
                MediaTick(end_tick.raw().saturating_sub(1)),
            ),
            x,
            line_rect: rect,
        });
    }
    None
}

fn hit_existing_sync(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<DetectionAddress> {
    for line in project.lines() {
        if line.karaoke {
            continue;
        }
        let Some(data) = project.detections().line(line.id) else {
            continue;
        };
        let rect = line_rect(project, line, current_frame, zone);
        for cue in data.text_sync_cues() {
            let cue_x = tick_x(cue.media_tick, current_frame, zone);
            if expanded_rect(sync_dot_rect(cue_x, rect), SYNC_DOT_HIT_PADDING).contains(x, y) {
                return Some(DetectionAddress {
                    line_id: line.id,
                    detection_id: cue.id,
                });
            }
        }
    }
    None
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

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    crate::detection_foreground::reconcile_legacy_menu(state);

    if let UiEvent::MouseMove { x, y } = event {
        let mut slot = source_drag();
        if let Some(drag) = slot.as_mut() {
            if !drag.moved {
                let dx = *x - drag.start_x;
                let dy = *y - drag.start_y;
                if dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD {
                    drag.moved = true;
                    state.detection_menu = None;
                }
            }
            if drag.moved {
                if let Some((track, _)) =
                    track_under_pointer(ctx.project, *y, ctx.current_frame, ctx.zone)
                {
                    drag.target_track = track;
                }
                drag.target_tick = if drag.lock_x {
                    drag.origin_tick
                } else {
                    pointer_tick(*x, ctx.current_frame, ctx.zone)
                };
            }
            return Some(EventResponse::Consumed);
        }
        drop(slot);

        {
            let mut sync = sync_interaction();
            if let Some(mut drag) = sync.drag {
                if !drag.moved {
                    let dx = *x - drag.start_x;
                    let dy = *y - drag.start_y;
                    drag.moved = dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD;
                    sync.drag = Some(drag);
                }
                if drag.moved {
                    sync.hover = None;
                    return Some(EventResponse::Action(UiAction::MoveDetection {
                        address: drag.address,
                        media_tick: clamp_sync_tick(
                            ctx.project,
                            drag.address,
                            pointer_tick(*x, ctx.current_frame, ctx.zone),
                        ),
                    }));
                }
                return Some(EventResponse::Consumed);
            }
        }

        state.detection_hover =
            track_under_pointer(ctx.project, *y, ctx.current_frame, ctx.zone).map(
                |(track, rect)| DetectionHover {
                    track,
                    media_tick: pointer_tick(*x, ctx.current_frame, ctx.zone),
                    screen_x: *x,
                    screen_y: *y,
                    track_rect: rect,
                },
            );

        let line = normal_line_under_pointer(ctx.project, *x, *y, ctx.current_frame, ctx.zone);
        let hover = line.and_then(|line| {
            sync_placeholder_for_line(ctx.project, line, *x, ctx.current_frame, ctx.zone)
        });
        sync_interaction().hover = hover;
        if line.is_some() {
            return None;
        }
    }

    if let UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } = event {
        if state.detection_menu.is_some() {
            return base::handle_detection_event(ctx, event, state);
        }

        if let Some(address) = hit_existing_sync(ctx.project, ctx.current_frame, ctx.zone, *x, *y) {
            state.selected = Some(Selection::Detection(address));
            state.detection_menu = None;
            state.detection_drag = None;
            let mut sync = sync_interaction();
            sync.drag = Some(SyncDrag {
                address,
                start_x: *x,
                start_y: *y,
                moved: false,
            });
            sync.hover = None;
            return Some(EventResponse::Consumed);
        }

        let placeholder = {
            let cached = sync_interaction().hover;
            cached.or_else(|| {
                normal_line_under_pointer(
                    ctx.project,
                    *x,
                    *y,
                    ctx.current_frame,
                    ctx.zone,
                )
                .and_then(|line| {
                    sync_placeholder_for_line(
                        ctx.project,
                        line,
                        *x,
                        ctx.current_frame,
                        ctx.zone,
                    )
                })
            })
        };
        if let Some(placeholder) = placeholder {
            let Some(address) = next_detection_address(ctx.project, placeholder.line_id) else {
                return Some(EventResponse::Consumed);
            };
            state.selected = Some(Selection::Detection(address));
            state.detection_menu = None;
            sync_interaction().drag = Some(SyncDrag {
                address,
                start_x: *x,
                start_y: *y,
                moved: false,
            });
            return Some(EventResponse::Action(UiAction::AddDetection {
                line_id: placeholder.line_id,
                kind: DetectionKind::TextSyncPoint,
                media_tick: placeholder.media_tick,
                target: TextAnchor::Grapheme {
                    index: placeholder.character_index as u32,
                },
            }));
        }

        if let Some(hover) = state.detection_hover {
            if legacy_add_button_rect(&hover).contains(*x, *y) {
                // The old plus control is deliberately removed. Alt+D remains
                // the keyboard entry point for the detector palette.
                return Some(EventResponse::Consumed);
            }
        }

        if let Some((address, cue)) =
            hit_source_detection(ctx.project, ctx.current_frame, ctx.zone, *x, *y)
        {
            let lock_x = matches!(event, UiEvent::ShiftMousePress { .. });
            begin_source_drag(address, cue, *x, *y, lock_x);
            if lock_x {
                state.selected = Some(Selection::Detection(address));
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }
            return base::handle_detection_event(ctx, event, state)
                .or(Some(EventResponse::Consumed));
        }

        if normal_line_under_pointer(ctx.project, *x, *y, ctx.current_frame, ctx.zone).is_some() {
            return None;
        }
    }

    if let UiEvent::MouseRelease { .. } = event {
        if let Some(drag) = source_drag().take() {
            if drag.moved {
                state.detection_drag = None;
                state.detection_menu = None;
                if drag.target_track == drag.origin_track {
                    return (drag.target_tick != drag.origin_tick)
                        .then_some(EventResponse::Action(UiAction::MoveDetection {
                            address: drag.address,
                            media_tick: drag.target_tick,
                        }))
                        .or(Some(EventResponse::Consumed));
                }
                return Some(EventResponse::Actions(vec![
                    UiAction::DeleteDetection {
                        address: drag.address,
                    },
                    UiAction::AddDetection {
                        line_id: track_storage_line_id(drag.target_track),
                        kind: drag.cue.kind,
                        media_tick: drag.target_tick,
                        target: drag.cue.target,
                    },
                ]));
            }
            if drag.lock_x {
                state.detection_drag = None;
                return Some(EventResponse::Consumed);
            }
            return base::handle_detection_event(ctx, event, state);
        }

        let sync_drag = sync_interaction().drag.take();
        if sync_drag.is_some() {
            state.detection_drag = None;
            return Some(EventResponse::Consumed);
        }
    }

    if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
        *source_drag() = None;
        *sync_interaction() = SyncInteraction::default();
    }

    base::handle_detection_event(ctx, event, state)
}

/// Normal adaptation lines never expose syllable handles. Synchronization is
/// authored exclusively through letter-attached timing points.
pub(crate) fn sync_syllable_boundary_ratios(
    _project: &Project,
    _line: &crate::rythmo_line::RythmoLine,
    _drag: Option<&SyllableDrag>,
    _lang: &str,
    _state: &RythmoState,
) -> Option<Vec<f32>> {
    None
}

fn sync_segment_cache_id(line_id: u64, start: usize, end: usize) -> u64 {
    (1_u64 << 61)
        ^ line_id.wrapping_mul(1_000_003)
        ^ (start as u64).wrapping_mul(65_537)
        ^ end as u64
}

/// Render every interval independently. Moving one synchronization point changes
/// only the interval immediately before it and the interval immediately after it.
pub(crate) fn render_sync_text_segments(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    _drag: Option<&SyllableDrag>,
    _lang: &str,
    _state: &RythmoState,
    read_highlight_end: Option<usize>,
    tint: [f32; 4],
    stretched: &mut Vec<StretchedText>,
) -> Option<Vec<CursorSegmentInfo>> {
    if line.karaoke || line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }
    let boundaries = sync_boundaries(project, line);
    if boundaries.len() <= 2 {
        return None;
    }

    let characters = line.text.chars().collect::<Vec<_>>();
    let rect = line_rect(project, line, current_frame, zone);
    let line_start = MediaTick::from_frame(line.start_frame);
    let line_duration = MediaTick::from_frame(line.duration_frames).raw().max(1) as f32;
    let mut cursor_segments = Vec::new();

    for pair in boundaries.windows(2) {
        let (start_char, start_tick) = pair[0];
        let (end_char, end_tick) = pair[1];
        if end_char <= start_char || end_char > characters.len() || end_tick <= start_tick {
            continue;
        }

        let start_ratio =
            (start_tick.raw() - line_start.raw()) as f32 / line_duration;
        let width_ratio =
            (end_tick.raw() - start_tick.raw()) as f32 / line_duration;
        let text = characters[start_char..end_char].iter().collect::<String>();
        if text.is_empty() || width_ratio <= 0.0 {
            continue;
        }

        let cache_id = sync_segment_cache_id(line.id, start_char, end_char);
        push_read_word_rythmo_text(
            stretched,
            cache_id,
            text,
            Rect {
                x: rect.x + rect.width * start_ratio,
                y: rect.y,
                width: rect.width * width_ratio,
                height: rect.height,
            },
            start_char,
            read_highlight_end,
            tint,
        );
        cursor_segments.push(CursorSegmentInfo {
            cache_id,
            start_char,
            end_char,
            start_ratio,
            width_ratio,
        });
    }

    (!cursor_segments.is_empty()).then_some(cursor_segments)
}

pub(crate) fn begin_sync_syllable_drag(
    _project: &Project,
    _line: &crate::rythmo_line::RythmoLine,
    _encoded_line_id: u64,
    _separator_index: usize,
    _state: &RythmoState,
) {
}

pub(crate) fn active_sync_syllable_edit_range(
    _encoded_line_id: u64,
    _segment_count: usize,
) -> Option<(usize, usize)> {
    None
}

pub(crate) fn finish_sync_syllable_drag(
    _encoded_line_id: u64,
    _ratios: &[f32],
) -> Option<(i64, i64)> {
    None
}

pub(crate) fn clear_sync_syllable_drag() {}

fn rect_center(rect: [f32; 4]) -> (f32, f32) {
    (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0)
}

fn pop_icon_tail_inside(icons: &mut Vec<IconInstance>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = icons.last() else { break };
        let (x, y) = rect_center(last.rect);
        if !outer.contains(x, y) {
            break;
        }
        icons.pop();
    }
}

fn pop_quad_tail_inside(quads: &mut Vec<QuadInstance>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = quads.last() else { break };
        let (x, y) = rect_center(last.rect);
        if !outer.contains(x, y) {
            break;
        }
        quads.pop();
    }
}

fn pop_label_tail_inside<'a>(labels: &mut Vec<LabelInfo<'a>>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = labels.last() else { break };
        let x = last.bounds.x + last.bounds.width / 2.0;
        let y = last.bounds.y + last.bounds.height / 2.0;
        if !outer.contains(x, y) {
            break;
        }
        labels.pop();
    }
}

fn strip_legacy_popup<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
) {
    let Some((kind, outer)) = crate::detection_foreground::suppressed_popup() else {
        return;
    };
    match kind {
        crate::detection_foreground::PopupKind::Palette => {
            pop_icon_tail_inside(icons, outer, 9);
            pop_quad_tail_inside(quads, outer, 2);
        }
        crate::detection_foreground::PopupKind::Info => {
            pop_icon_tail_inside(icons, outer, 1);
            pop_label_tail_inside(labels, outer, 5);
            pop_quad_tail_inside(quads, outer, 2);
        }
    }
}

fn push_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], radius: f32) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_line(
    quads: &mut Vec<QuadInstance>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt().max(0.1);
    quads.push(QuadInstance {
        rect: [
            (x1 + x2) * 0.5 - length / 2.0,
            (y1 + y2) * 0.5 - thickness / 2.0,
            length,
            thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: thickness / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: dy.atan2(dx),
        _padding: [0.0; 2],
    });
}

fn strip_legacy_sync_and_handles(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    quads: &mut Vec<QuadInstance>,
) {
    quads.retain(|quad| {
        if quad.color == [0.95, 0.08, 0.03, 1.0] {
            return false;
        }
        let (x, y) = rect_center(quad.rect);
        let inside_sync_line = project.lines().any(|line| {
            !line.karaoke
                && line_has_sync_points(project, line.id)
                && line_rect(project, line, current_frame, zone).contains(x, y)
        });
        !(inside_sync_line
            && quad.rect[2] <= SYNC_DOT_SIZE + 4.0
            && quad.rect[3] <= SYNC_DOT_SIZE + 4.0
            && quad.color[2] > quad.color[0])
    });
}

fn strip_legacy_plus(
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    icons: &mut Vec<IconInstance>,
) {
    let Some(hover) = state.detection_hover else {
        return;
    };
    let outer = legacy_add_button_rect(&hover);
    quads.retain(|quad| {
        let (x, y) = rect_center(quad.rect);
        !outer.contains(x, y)
    });
    icons.retain(|icon| {
        let (x, y) = rect_center(icon.rect);
        !outer.contains(x, y)
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
    crate::rythmo_lint_overlay::sync_geometry(project, state, *zone, current_frame);

    let mut detector_quads = Vec::new();
    let mut detector_labels = Vec::new();
    let mut detector_icons = Vec::new();
    base::render_detection_overlay(
        zone,
        project,
        current_frame,
        state,
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
        detection_uvs,
    );
    strip_legacy_popup(
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
    );
    strip_legacy_sync_and_handles(project, current_frame, zone, &mut detector_quads);
    strip_legacy_plus(state, &mut detector_quads, &mut detector_icons);

    // Reposition source signs below synchronization points.
    let selected = selected_detection(state);
    for track in 0..rythmo_layout::track_count() {
        let line_id = track_storage_line_id(track as u8);
        let Some(data) = project.detections().line(line_id) else {
            continue;
        };
        let rect = track_rect(project, track, current_frame, zone);
        for cue in data.source_detections() {
            let address = DetectionAddress {
                line_id,
                detection_id: cue.id,
            };
            let original_icon = sign_icon_rect(cue.media_tick, rect, current_frame, zone);
            detector_icons.retain(|icon| {
                let (x, y) = rect_center(icon.rect);
                !original_icon.contains(x, y)
            });
            let original_badge = sign_badge_rect(cue.media_tick, rect, current_frame, zone);
            detector_quads.retain(|quad| {
                let (x, y) = rect_center(quad.rect);
                !original_badge.contains(x, y) || quad.rect[3] > original_badge.height + 2.0
            });

            if source_drag()
                .as_ref()
                .is_some_and(|drag| drag.address == address && drag.moved)
            {
                continue;
            }
            let badge = displayed_badge_rect(cue.media_tick, rect, current_frame, zone);
            push_quad(
                &mut detector_quads,
                badge,
                if selected == Some(address) {
                    [0.09, 0.16, 0.29, 0.998]
                } else {
                    [0.055, 0.059, 0.074, 0.998]
                },
                badge.width / 2.0,
            );
            let icon = displayed_icon_rect(cue.media_tick, rect, current_frame, zone);
            detector_icons.push(IconInstance {
                rect: [icon.x, icon.y, icon.width, icon.height],
                uv_rect: palette_uv(cue, detection_uvs),
                tint: if selected == Some(address) {
                    [0.78, 0.88, 1.0, 1.0]
                } else {
                    [0.92, 0.92, 0.95, 0.94]
                },
            });
        }
    }

    if let Some(drag) = source_drag().clone().filter(|drag| drag.moved) {
        let target_track = track_rect(project, drag.target_track as usize, current_frame, zone);
        let badge = displayed_badge_rect(drag.target_tick, target_track, current_frame, zone);
        let x = badge.x + badge.width / 2.0;
        push_line(
            &mut detector_quads,
            x,
            target_track.y + 2.0,
            x,
            target_track.y + target_track.height - 2.0,
            1.5,
            [0.55, 0.73, 1.0, 0.82],
        );
        push_quad(
            &mut detector_quads,
            badge,
            [0.09, 0.16, 0.29, 0.998],
            badge.width / 2.0,
        );
        let icon = displayed_icon_rect(drag.target_tick, target_track, current_frame, zone);
        detector_icons.push(IconInstance {
            rect: [icon.x, icon.y, icon.width, icon.height],
            uv_rect: palette_uv(&drag.cue, detection_uvs),
            tint: [0.78, 0.88, 1.0, 1.0],
        });
    }

    // Draw the authoritative Mosaic-style synchronization points at their
    // absolute media positions.
    for line in project.lines() {
        if line.karaoke {
            continue;
        }
        let Some(data) = project.detections().line(line.id) else {
            continue;
        };
        let rect = line_rect(project, line, current_frame, zone);
        for cue in data.text_sync_cues() {
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: cue.id,
            };
            let cue_x = tick_x(cue.media_tick, current_frame, zone);
            let dot = sync_dot_rect(cue_x, rect);
            let extra = if selected == Some(address) { 1.5 } else { 0.0 };
            push_quad(
                &mut detector_quads,
                Rect {
                    x: dot.x - extra,
                    y: dot.y - extra,
                    width: dot.width + extra * 2.0,
                    height: dot.height + extra * 2.0,
                },
                if selected == Some(address) {
                    [0.72, 0.88, 1.0, 1.0]
                } else {
                    [0.48, 0.72, 1.0, 0.96]
                },
                8.0,
            );
        }
    }

    if let Some(placeholder) = sync_interaction().hover {
        let dot = sync_dot_rect(placeholder.x, placeholder.line_rect);
        push_quad(
            &mut detector_quads,
            dot,
            [0.48, 0.72, 1.0, 0.45],
            8.0,
        );
    }

    quads.extend(detector_quads);
    labels.extend(detector_labels);
    icons.extend(detector_icons);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synchronized_intervals_keep_independent_absolute_bounds() {
        let mut project = Project::new();
        let line_id = project.add_line(100, 100, 0.0);
        project.get_line_mut(line_id).unwrap().text = "abcdefghij".to_string();
        let first = DetectionCue {
            id: DetectionCueId(1),
            kind: DetectionKind::TextSyncPoint,
            media_tick: MediaTick::from_frame(130),
            target: TextAnchor::Grapheme { index: 3 },
        };
        let second = DetectionCue {
            id: DetectionCueId(2),
            kind: DetectionKind::TextSyncPoint,
            media_tick: MediaTick::from_frame(170),
            target: TextAnchor::Grapheme { index: 7 },
        };
        assert!(project
            .detections_mut()
            .insert_detection(DetectionAddress {
                line_id,
                detection_id: first.id,
            }, first));
        assert!(project
            .detections_mut()
            .insert_detection(DetectionAddress {
                line_id,
                detection_id: second.id,
            }, second));

        let line = project.get_line(line_id).unwrap();
        let boundaries = sync_boundaries(&project, line);
        assert_eq!(
            boundaries,
            vec![
                (0, MediaTick::from_frame(100)),
                (3, MediaTick::from_frame(130)),
                (7, MediaTick::from_frame(170)),
                (10, MediaTick::from_frame(200)),
            ]
        );
    }

    #[test]
    fn moving_one_point_is_clamped_only_by_neighbours_and_line_bounds() {
        let mut project = Project::new();
        let line_id = project.add_line(100, 100, 0.0);
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
                .insert_detection(DetectionAddress {
                    line_id,
                    detection_id: cue.id,
                }, cue));
        }
        let address = DetectionAddress {
            line_id,
            detection_id: DetectionCueId(1),
        };
        assert_eq!(
            clamp_sync_tick(&project, address, MediaTick::from_frame(160)),
            MediaTick::from_frame(160)
        );
        assert!(clamp_sync_tick(&project, address, MediaTick::from_frame(190))
            < MediaTick::from_frame(170));
    }

    #[test]
    fn normal_lines_never_expose_syllable_boundaries() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        let state = RythmoState::new();
        let line = project.get_line(line_id).unwrap();
        assert!(sync_syllable_boundary_ratios(
            &project,
            line,
            None,
            "fr",
            &state
        )
        .is_none());
    }
}
