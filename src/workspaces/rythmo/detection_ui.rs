//! Detection UI facade.
//!
//! The legacy detector still owns persistence and the information menu. This
//! facade owns the stable add button, cross-track source-sign dragging and the
//! piecewise text layout used around synchronization points.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;

#[path = "detection_ui_base.rs"]
mod base;

pub(crate) use base::{
    decode_sync_syllable_drag_line_id, encode_sync_syllable_drag_line_id,
    line_has_visible_sync_points, waveform_drag_markers,
};
pub use base::{DetectionDrag, DetectionHover, DetectionMenu};

const SIGN_BADGE_SIZE: f32 = 26.0;
const SIGN_ICON_SIZE: f32 = 18.0;
const SIGN_BOTTOM_MARGIN: f32 = 2.0;
const ADD_BUTTON_SIZE: f32 = 18.0;
const DRAG_THRESHOLD: f32 = 4.0;

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
struct ResizeDrag {
    address: DetectionAddress,
    fixed_tick: MediaTick,
    moving_left: bool,
}

#[derive(Clone)]
struct SyncSyllableDragContext {
    encoded_line_id: u64,
    line_id: u64,
    start_frame: i64,
    duration_frames: i64,
    character_count: usize,
    breaks: Vec<usize>,
    anchors: Vec<(usize, MediaTick)>,
    edit_range: (usize, usize),
}

fn source_drag_slot() -> &'static Mutex<Option<SourceDrag>> {
    static SLOT: OnceLock<Mutex<Option<SourceDrag>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn resize_drag_slot() -> &'static Mutex<Option<ResizeDrag>> {
    static SLOT: OnceLock<Mutex<Option<ResizeDrag>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn resize_drag() -> std::sync::MutexGuard<'static, Option<ResizeDrag>> {
    resize_drag_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cue_width(cue: &DetectionCue) -> f32 {
    (cue.duration.as_frame_position().abs() as f32 * ppf()).max(SIGN_ICON_SIZE + 8.0)
}

fn hit_resize_handle(
    project: &Project,
    state: &RythmoState,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    x: f32,
    y: f32,
) -> Option<ResizeDrag> {
    let address = selected_detection(state)?;
    let cue = project.detections().detection(address)?;
    let rect = if let Some(track) = address.track() {
        track_rect(project, track as usize, current_frame, zone)
    } else {
        let line = project.get_line(address.line_id)?;
        line_rect(
            project,
            line,
            current_frame,
            zone,
            crate::config::reading_bar_offset_seconds(),
            fps,
        )
    };
    let center_x = tick_x(cue.media_tick, current_frame, zone, fps);
    let half = cue_width(cue) / 2.0;
    let top = rect.y - SIGN_BADGE_SIZE + 2.0;
    [(center_x - half, true), (center_x + half, false)]
        .into_iter()
        .find_map(|(handle_x, moving_left)| {
            Rect {
                x: handle_x - 6.0,
                y: top + 3.0,
                width: 12.0,
                height: 20.0,
            }
            .contains(x, y)
            .then_some(ResizeDrag {
                address,
                fixed_tick: pointer_tick(
                    if moving_left {
                        center_x + half
                    } else {
                        center_x - half
                    },
                    current_frame,
                    zone,
                    fps,
                ),
                moving_left,
            })
        })
}

fn sync_drag_slot() -> &'static Mutex<Option<SyncSyllableDragContext>> {
    static SLOT: OnceLock<Mutex<Option<SyncSyllableDragContext>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn source_drag() -> std::sync::MutexGuard<'static, Option<SourceDrag>> {
    source_drag_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn sync_drag() -> std::sync::MutexGuard<'static, Option<SyncSyllableDragContext>> {
    sync_drag_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect, fps: f64) -> MediaTick {
    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    let frame = current_frame + offset_frames + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect, fps: f64) -> f32 {
    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    zone.x
        + zone.width / 2.0
        + (tick.as_frame_position() - current_frame - offset_frames) as f32 * ppf()
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

fn sign_badge_rect(
    tick: MediaTick,
    track: Rect,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
) -> Rect {
    Rect {
        x: tick_x(tick, current_frame, zone, fps) - SIGN_BADGE_SIZE / 2.0,
        y: track.y - SIGN_BADGE_SIZE - SIGN_BOTTOM_MARGIN,
        width: SIGN_BADGE_SIZE,
        height: SIGN_BADGE_SIZE,
    }
}

fn sign_icon_rect(tick: MediaTick, track: Rect, current_frame: f64, zone: &Rect, fps: f64) -> Rect {
    let badge = sign_badge_rect(tick, track, current_frame, zone, fps);
    Rect {
        x: badge.x + (badge.width - SIGN_ICON_SIZE) / 2.0,
        y: badge.y + (badge.height - SIGN_ICON_SIZE) / 2.0,
        width: SIGN_ICON_SIZE,
        height: SIGN_ICON_SIZE,
    }
}

fn palette_uv(cue: &DetectionCue, uvs: [[f32; 4]; 18]) -> [f32; 4] {
    let alternate = matches!(&cue.target, TextAnchor::AfterText);
    let index = match cue.kind {
        DetectionKind::Labial => Some(0),
        DetectionKind::SemiLabial => Some(1),
        DetectionKind::MouthOpen => Some(2),
        DetectionKind::MouthClosed => Some(3),
        DetectionKind::TeethVisible if !alternate => Some(4),
        DetectionKind::Breath if !alternate => Some(5),
        DetectionKind::Reaction => Some(6),
        DetectionKind::Pucker => return uvs[9],
        DetectionKind::OpeningWave | DetectionKind::ForwardWave => return [0.0; 4],
        DetectionKind::TeethVisible | DetectionKind::Breath => None,
        DetectionKind::TextSyncPoint => return [0.0; 4],
    };
    if let Some(index) = index {
        return uvs[index];
    }
    let extra_index = if matches!(cue.kind, DetectionKind::TeethVisible) {
        7
    } else {
        8
    };
    uvs[extra_index]
}

fn selected_detection(state: &RythmoState) -> Option<DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
}

fn hit_source_detection(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    x: f32,
    y: f32,
) -> Option<(DetectionAddress, DetectionCue)> {
    (0..rythmo_layout::track_count()).find_map(|track| {
        let line_id = track_storage_line_id(track as u8);
        let data = project.detections().line(line_id)?;
        let rect = track_rect(project, track, current_frame, zone);
        data.source_detections().find_map(|cue| {
            sign_badge_rect(cue.media_tick, rect, current_frame, zone, fps)
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

fn begin_source_drag(address: DetectionAddress, cue: DetectionCue, x: f32, y: f32, lock_x: bool) {
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

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    crate::detection_foreground::reconcile_legacy_menu(state);

    if let UiEvent::MouseMove { x, y } = event {
        if let Some(drag) = *resize_drag() {
            const MINIMUM_DURATION: i64 = 5;
            let pointer = pointer_tick(*x, ctx.current_frame, ctx.zone, ctx.fps);
            let moving_tick = if drag.moving_left {
                MediaTick(pointer.raw().min(drag.fixed_tick.raw() - MINIMUM_DURATION))
            } else {
                MediaTick(pointer.raw().max(drag.fixed_tick.raw() + MINIMUM_DURATION))
            };
            let start = MediaTick(moving_tick.raw().min(drag.fixed_tick.raw()));
            let end = MediaTick(moving_tick.raw().max(drag.fixed_tick.raw()));
            return Some(EventResponse::Action(UiAction::ResizeDetection {
                address: drag.address,
                media_tick: MediaTick(start.raw() + (end.raw() - start.raw()) / 2),
                duration: MediaTick(end.raw() - start.raw()),
            }));
        }
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
                    pointer_tick(*x, ctx.current_frame, ctx.zone, ctx.fps)
                };
            }
            return Some(EventResponse::Consumed);
        }
    }

    if let UiEvent::MouseRelease { .. } = event {
        if resize_drag().take().is_some() {
            return Some(EventResponse::Consumed);
        }
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
                // The add action allocates the target bucket's own stable id and
                // selects it after the old address has been removed.
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

        if state.detection_drag.is_some() {
            if let Some(_address) = selected_detection(state).filter(|address| {
                address.track().is_none() && ctx.project.detections().sync_point(*address).is_some()
            }) {
                state.detection_drag = None;
                // Moving a synchronization point never changes the line's
                // global start or duration.
                return Some(EventResponse::Consumed);
            }
        }
    }

    if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
        *source_drag() = None;
        *resize_drag() = None;
    }

    if state.detection_menu.is_none() {
        if let UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } = event {
            if let Some(drag) = hit_resize_handle(
                ctx.project,
                state,
                ctx.current_frame,
                ctx.zone,
                ctx.fps,
                *x,
                *y,
            ) {
                *resize_drag() = Some(drag);
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }
            if let Some((address, cue)) =
                hit_source_detection(ctx.project, ctx.current_frame, ctx.zone, ctx.fps, *x, *y)
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
        }
    }

    base::handle_detection_event(ctx, event, state)
}

fn sync_anchors(project: &Project, line: &crate::rythmo_line::RythmoLine) -> Vec<(usize, f32)> {
    if line.karaoke || line.duration_frames <= 0 {
        return Vec::new();
    }
    let Some(data) = project.detections().line(line.id) else {
        return Vec::new();
    };
    let grapheme_count = UnicodeSegmentation::graphemes(line.text.as_str(), true).count();
    let spans = grapheme_char_spans(&line.text);
    let mut anchors = BTreeMap::new();
    for point in data.sync_points() {
        let index = point.grapheme_boundary as usize;
        if index < grapheme_count {
            let ratio = ((point.line_tick.as_frame_position() - line.start_frame as f64)
                / line.duration_frames as f64) as f32;
            if let Some(boundary) =
                sync_anchor_char_boundary(&line.text, &spans, index, point.affinity)
            {
                anchors.insert(boundary, ratio);
            }
        }
    }
    anchors.into_iter().collect()
}

fn grapheme_char_spans(text: &str) -> Vec<(usize, usize)> {
    let mut char_start = 0usize;
    UnicodeSegmentation::graphemes(text, true)
        .map(|grapheme| {
            let char_end = char_start + grapheme.chars().count();
            let span = (char_start, char_end);
            char_start = char_end;
            span
        })
        .collect()
}

fn uniform_grapheme_character_positions(spans: &[(usize, usize)]) -> Vec<f32> {
    let character_count = spans.last().map_or(0, |(_, end)| *end);
    let mut positions = vec![0.0; character_count + 1];
    let grapheme_count = spans.len().max(1) as f32;
    for (grapheme_index, (start, end)) in spans.iter().copied().enumerate() {
        let scalar_count = end.saturating_sub(start).max(1) as f32;
        for offset in 0..=end.saturating_sub(start) {
            positions[start + offset] =
                (grapheme_index as f32 + offset as f32 / scalar_count) / grapheme_count;
        }
    }
    positions
}

fn sync_anchor_char_boundary(
    text: &str,
    spans: &[(usize, usize)],
    grapheme_index: usize,
    affinity: crate::detection::SyncAffinity,
) -> Option<usize> {
    let (start, end) = *spans.get(grapheme_index)?;
    let characters = text.chars().collect::<Vec<_>>();
    let punctuation = characters
        .get(start..end)?
        .iter()
        .all(|character| crate::detection::is_sync_punctuation(*character));
    Some(match affinity {
        crate::detection::SyncAffinity::Left => end,
        crate::detection::SyncAffinity::Right => start,
        crate::detection::SyncAffinity::Auto if punctuation => end,
        crate::detection::SyncAffinity::Auto => start,
    })
}

fn base_character_positions(character_count: usize, ratios: &[f32], breaks: &[usize]) -> Vec<f32> {
    let mut positions = (0..=character_count)
        .map(|index| index as f32 / character_count.max(1) as f32)
        .collect::<Vec<_>>();
    if breaks.is_empty() || ratios.len() != breaks.len() + 1 {
        return positions;
    }
    let mut character_start = 0usize;
    let mut ratio_start = 0.0_f32;
    for (segment_index, segment_ratio) in ratios.iter().copied().enumerate() {
        let character_end = breaks
            .get(segment_index)
            .copied()
            .unwrap_or(character_count)
            .min(character_count);
        let length = character_end.saturating_sub(character_start);
        if length > 0 {
            for local_index in 0..=length {
                positions[character_start + local_index] =
                    ratio_start + segment_ratio * local_index as f32 / length as f32;
            }
        }
        ratio_start += segment_ratio;
        character_start = character_end;
    }
    if let Some(last) = positions.last_mut() {
        *last = 1.0;
    }
    positions
}

fn map_character_positions(base: &[f32], anchor_boundaries: &[(usize, f32)]) -> Vec<f32> {
    let mut controls = anchor_boundaries
        .iter()
        .filter_map(|(boundary, target)| {
            Some((base.get(*boundary)?.to_owned(), target.clamp(0.0, 1.0)))
        })
        .collect::<Vec<_>>();
    if controls.is_empty() {
        return base.to_vec();
    }
    controls.push((0.0, 0.0));
    controls.push((1.0, 1.0));
    controls.sort_by(|left, right| left.0.total_cmp(&right.0));
    controls.dedup_by(|left, right| (left.0 - right.0).abs() < 0.000_01);
    base.iter()
        .copied()
        .map(|source| {
            for pair in controls.windows(2) {
                let (source_start, target_start) = pair[0];
                let (source_end, target_end) = pair[1];
                if source <= source_end {
                    let local = ((source - source_start)
                        / (source_end - source_start).max(0.000_1))
                    .clamp(0.0, 1.0);
                    return target_start + (target_end - target_start) * local;
                }
            }
            1.0
        })
        .collect()
}

fn character_layout(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<(Vec<f32>, Vec<f32>, Vec<usize>, Vec<(usize, f32)>)> {
    let breaks = state.get_syllable_breaks(line, lang);
    let anchors = sync_anchors(project, line);
    let spans = grapheme_char_spans(line.text.as_str());
    if !anchors.is_empty() {
        let base = crate::rythmo_line::text_emotion_char_ratios(
            &line.text,
            crate::config::get().ui.font_size * 2.0,
        )
        .filter(|ratios| ratios.len() == line.text.chars().count() + 1)
        .unwrap_or_else(|| uniform_grapheme_character_positions(&spans));
        let mapped = map_character_positions(&base, &anchors);
        return Some((base, mapped, breaks, anchors));
    }
    let effective_drag = drag.filter(|drag| {
        drag.line_id == line.id || decode_sync_syllable_drag_line_id(drag.line_id) == Some(line.id)
    });
    let ratios =
        if let Some(drag) = effective_drag.filter(|drag| drag.ratios.len() == breaks.len() + 1) {
            drag.ratios.clone()
        } else {
            syllable_ratios_for_line(line, None, lang, state)?
        };
    let base = base_character_positions(line.text.chars().count(), &ratios, &breaks);
    let mapped = map_character_positions(&base, &[]);
    Some((base, mapped, breaks, anchors))
}

pub(crate) fn sync_syllable_boundary_ratios(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<Vec<f32>> {
    if !line_has_visible_sync_points(project, line) {
        return None;
    }
    let (_, mapped, breaks, _) = character_layout(project, line, drag, lang, state)?;
    if breaks.is_empty() {
        return None;
    }
    let character_count = line.text.chars().count();
    let mut boundaries = Vec::with_capacity(breaks.len() + 2);
    boundaries.push(*mapped.first()?);
    boundaries.extend(
        breaks
            .into_iter()
            .filter(|index| *index < mapped.len())
            .map(|index| mapped[index]),
    );
    boundaries.push(mapped[character_count]);
    Some(boundaries)
}

fn sync_segment_cache_id(line_id: u64, start: usize, end: usize) -> u64 {
    (1_u64 << 61)
        ^ line_id.wrapping_mul(1_000_003)
        ^ (start as u64).wrapping_mul(65_537)
        ^ end as u64
}

fn sync_cursor_segments_from_layout(
    line_id: u64,
    character_count: usize,
    boundaries: &BTreeSet<usize>,
    mapped: &[f32],
) -> Vec<CursorSegmentInfo> {
    boundaries
        .iter()
        .copied()
        .collect::<Vec<_>>()
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            if end <= start || end > character_count {
                return None;
            }
            Some(CursorSegmentInfo {
                cache_id: sync_segment_cache_id(line_id, start, end),
                start_char: start,
                end_char: end,
                start_ratio: mapped[start],
                width_ratio: (mapped[end] - mapped[start]).max(0.000_1),
            })
        })
        .collect()
}

pub(crate) fn sync_cursor_segments_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<Vec<CursorSegmentInfo>> {
    let (_, mapped, syllable_breaks, anchors) = character_layout(project, line, drag, lang, state)?;
    if anchors.is_empty() {
        return None;
    }
    let character_count = line.text.chars().count();
    let mut boundaries = BTreeSet::from([0, character_count]);
    boundaries.extend(
        syllable_breaks
            .into_iter()
            .filter(|index| *index < character_count),
    );
    boundaries.extend(anchors.into_iter().map(|(boundary, _)| boundary));
    let segments = sync_cursor_segments_from_layout(line.id, character_count, &boundaries, &mapped);
    (!segments.is_empty()).then_some(segments)
}

pub(crate) fn sync_cursor_index_for_line_at_ratio(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
    ratio: f32,
) -> Option<usize> {
    if !line_has_visible_sync_points(project, line) {
        return None;
    }
    let (_, mapped, _, _) = character_layout(project, line, drag, lang, state)?;
    mapped
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| (**left - ratio).abs().total_cmp(&(**right - ratio).abs()))
        .map(|(index, _)| index)
}

pub(crate) fn render_sync_text_segments(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    rect: Rect,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
    read_highlight_end: Option<usize>,
    tint: [f32; 4],
    emotion_seconds: Option<f32>,
    stretched: &mut Vec<StretchedText>,
) -> Option<Vec<CursorSegmentInfo>> {
    if line.karaoke || line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }
    let (_, mapped, syllable_breaks, anchors) = character_layout(project, line, drag, lang, state)?;
    if anchors.is_empty() {
        return None;
    }
    let character_count = line.text.chars().count();
    let mut boundaries = BTreeSet::new();
    boundaries.insert(0usize);
    boundaries.insert(character_count);
    boundaries.extend(
        syllable_breaks
            .into_iter()
            .filter(|index| *index < character_count),
    );
    boundaries.extend(anchors.iter().map(|(boundary, _)| *boundary));
    let cursor_segments =
        sync_cursor_segments_from_layout(line.id, character_count, &boundaries, &mapped);
    let characters = line.text.chars().collect::<Vec<_>>();
    if let Some(seconds) = emotion_seconds {
        super::push_emotional_text(
            stretched,
            line,
            rect,
            seconds,
            tint,
            Some(&mapped),
            project.settings().show_text_emotion_lanes,
        );
    } else {
        for segment in &cursor_segments {
            let start = segment.start_char;
            let end = segment.end_char;
            let start_ratio = segment.start_ratio;
            let width_ratio = segment.width_ratio;
            let width = rect.width * width_ratio;
            if width <= 0.5 {
                continue;
            }
            let text = characters[start..end].iter().collect::<String>();
            if text.is_empty() {
                continue;
            }
            let cache_id = segment.cache_id;
            // The implicit line edges and explicit synchronization points split
            // the cue into independent intervals. Each text portion stretches to
            // fill its complete interval, so the line still fills the whole box.
            push_read_word_rythmo_text(
                stretched,
                cache_id,
                text,
                Rect {
                    x: rect.x + rect.width * start_ratio,
                    y: rect.y,
                    width,
                    height: rect.height,
                },
                start,
                read_highlight_end,
                tint,
            );
        }
    }
    (!cursor_segments.is_empty()).then_some(cursor_segments)
}

pub(crate) fn begin_sync_syllable_drag(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    encoded_line_id: u64,
    separator_index: usize,
    state: &RythmoState,
) {
    let breaks = state.get_syllable_breaks(line, project.syllable_language_code());
    let boundary = breaks.get(separator_index).copied().unwrap_or(0);
    let spans = grapheme_char_spans(line.text.as_str());
    let anchors = project
        .detections()
        .line(line.id)
        .map(|data| {
            data.sync_points()
                .iter()
                .filter_map(|point| {
                    sync_anchor_char_boundary(
                        &line.text,
                        &spans,
                        point.grapheme_boundary as usize,
                        point.affinity,
                    )
                    .map(|boundary| (boundary, point.line_tick))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let previous = anchors
        .iter()
        .filter(|(index, _)| *index < boundary)
        .map(|(index, _)| *index)
        .max();
    let next = anchors
        .iter()
        .filter(|(index, _)| *index > boundary)
        .map(|(index, _)| *index)
        .min();
    let segment_count = breaks.len() + 1;
    let start = previous
        .map(|index| breaks.iter().take_while(|value| **value <= index).count())
        .unwrap_or(0)
        .min(segment_count.saturating_sub(1));
    let end = next
        .map(|index| breaks.iter().take_while(|value| **value < index).count() + 1)
        .unwrap_or(segment_count)
        .min(segment_count);
    let edit_range = if anchors.iter().any(|(index, _)| *index == boundary) {
        // A separator carrying a sync point is itself fixed. Adjacent handles
        // remain editable, but this exact anchor cannot redistribute either side.
        (separator_index + 1, separator_index + 1)
    } else if start <= separator_index && separator_index + 1 < end {
        (start, end)
    } else {
        (0, segment_count)
    };
    *sync_drag() = Some(SyncSyllableDragContext {
        encoded_line_id,
        line_id: line.id,
        start_frame: line.start_frame,
        duration_frames: line.duration_frames,
        character_count: line.text.chars().count(),
        breaks,
        anchors,
        edit_range,
    });
}

pub(crate) fn active_sync_syllable_edit_range(
    encoded_line_id: u64,
    segment_count: usize,
) -> Option<(usize, usize)> {
    let slot = sync_drag();
    let context = slot.as_ref()?;
    (context.encoded_line_id == encoded_line_id).then_some((
        context.edit_range.0.min(segment_count),
        context.edit_range.1.min(segment_count),
    ))
}

fn bounds_from_context(context: &SyncSyllableDragContext, ratios: &[f32]) -> Option<(i64, i64)> {
    if context.duration_frames <= 0 || context.character_count == 0 {
        return None;
    }
    let base = base_character_positions(context.character_count, ratios, &context.breaks);
    let line_start = MediaTick::from_frame(context.start_frame);
    let duration_ticks = MediaTick::from_frame(context.duration_frames).raw().max(1) as f32;
    let anchors = context
        .anchors
        .iter()
        .map(|(index, tick)| {
            (
                *index,
                tick.raw().saturating_sub(line_start.raw()) as f32 / duration_ticks,
            )
        })
        .collect::<Vec<_>>();
    let mapped = map_character_positions(&base, &anchors);
    let first = *mapped.first()? as f64;
    let last = *mapped.last()? as f64;
    let text_start =
        (context.start_frame as f64 + first * context.duration_frames as f64).floor() as i64;
    let text_end =
        (context.start_frame as f64 + last * context.duration_frames as f64).ceil() as i64;
    let sync_start = context
        .anchors
        .iter()
        .map(|(_, tick)| tick.as_frame_position().floor() as i64)
        .min()
        .unwrap_or(text_start);
    let sync_end = context
        .anchors
        .iter()
        .map(|(_, tick)| tick.as_frame_position().ceil() as i64)
        .max()
        .unwrap_or(text_end);
    let start = text_start.min(sync_start);
    let end = text_end.max(sync_end);
    Some((start, end.saturating_sub(start).max(1)))
}

pub(crate) fn finish_sync_syllable_drag(
    encoded_line_id: u64,
    ratios: &[f32],
) -> Option<(i64, i64)> {
    let context = sync_drag().take()?;
    (context.encoded_line_id == encoded_line_id)
        .then(|| bounds_from_context(&context, ratios))
        .flatten()
}

pub(crate) fn clear_sync_syllable_drag() {
    *sync_drag() = None;
}

/// Fit the line to the rendered text while retaining every absolute sync point.
/// Moving text outside expands the box; leaving unused space at either edge
/// shrinks it, but never so far that a point falls outside its parent line.
fn synchronized_line_bounds(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<(i64, i64)> {
    let (_, mapped, _, anchors) = character_layout(project, line, drag, lang, state)?;
    if anchors.is_empty() {
        return None;
    }
    let first = *mapped.first()? as f64;
    let last = *mapped.last()? as f64;
    let text_start = (line.start_frame as f64 + first * line.duration_frames as f64).floor() as i64;
    let text_end = (line.start_frame as f64 + last * line.duration_frames as f64).ceil() as i64;
    let sync_start = anchors
        .iter()
        .map(|(_, ratio)| line.start_frame as f64 + *ratio as f64 * line.duration_frames as f64)
        .map(|frame| frame.floor() as i64)
        .min()
        .unwrap_or(text_start);
    let sync_end = anchors
        .iter()
        .map(|(_, ratio)| line.start_frame as f64 + *ratio as f64 * line.duration_frames as f64)
        .map(|frame| frame.ceil() as i64)
        .max()
        .unwrap_or(text_end);
    let start = text_start.min(sync_start);
    let end = text_end.max(sync_end);
    Some((start, end.saturating_sub(start).max(1)))
}

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
    // Extend adjoining procedural segments under one another. Without this
    // small overlap, antialiased rounded caps make a continuous curve look
    // dotted at normal zoom levels.
    let length = (dx * dx + dy * dy).sqrt().max(0.1) + thickness;
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

fn render_sync_handles(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
) {
    let Some(boundaries) = sync_syllable_boundary_ratios(
        project,
        line,
        state.syllable_drag.as_ref(),
        project.syllable_language_code(),
        state,
    ) else {
        return;
    };
    if boundaries.len() <= 2 {
        return;
    }
    let rect = line_rect(
        project,
        line,
        current_frame,
        zone,
        crate::config::reading_bar_offset_seconds(),
        fps,
    );
    let points = boundaries
        .into_iter()
        .map(|ratio| rect.x + rect.width * ratio)
        .collect::<Vec<_>>();
    let color = [0.95, 0.08, 0.03, 1.0];
    for pair in points.windows(2) {
        let start = pair[0] + 2.0;
        let end = pair[1] - 2.0;
        if end > start {
            push_quad(
                quads,
                Rect {
                    x: start,
                    y: rect.y + 1.0,
                    width: end - start,
                    height: 3.0,
                },
                color,
                1.5,
            );
        }
    }
    for x in points {
        push_quad(
            quads,
            Rect {
                x: x - 1.5,
                y: rect.y + 1.0,
                width: 3.0,
                height: 9.0,
            },
            color,
            1.5,
        );
    }
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
    detection_uvs: [[f32; 4]; 18],
    editable: bool,
) {
    let mut detector_quads = Vec::new();
    let mut detector_labels = Vec::new();
    let mut detector_icons = Vec::new();
    base::render_detection_overlay(
        zone,
        project,
        current_frame,
        fps,
        state,
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
        detection_uvs,
        editable,
    );
    strip_legacy_popup(
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
    );

    // Remove the legacy whole-suffix handles; the replacement below follows
    // the same piecewise positions as the text and hit testing.
    detector_quads.retain(|quad| quad.color != [0.95, 0.08, 0.03, 1.0]);
    detector_quads.retain(|quad| quad.color != [0.20, 0.42, 0.88, 0.24]);

    if state.detection_menu.is_none() {
        if let Some(hover) = state.detection_hover {
            let old_button = Rect {
                x: hover.screen_x - ADD_BUTTON_SIZE / 2.0,
                y: hover.track_rect.y + hover.track_rect.height + 4.0,
                width: ADD_BUTTON_SIZE,
                height: ADD_BUTTON_SIZE,
            };
            detector_quads.retain(|quad| {
                let (x, y) = rect_center(quad.rect);
                !old_button.contains(x, y)
            });
            // The transient plus and its click target are intentionally absent:
            // the palette is opened explicitly with Alt+D.
        }
    }

    let drag_snapshot = editable
        .then(|| source_drag().clone())
        .flatten()
        .filter(|drag| drag.moved);
    if let Some(drag) = drag_snapshot.as_ref() {
        let original_track = track_rect(project, drag.origin_track as usize, current_frame, zone);
        let original_icon =
            sign_icon_rect(drag.origin_tick, original_track, current_frame, zone, fps);
        detector_icons.retain(|icon| {
            let (x, y) = rect_center(icon.rect);
            !original_icon.contains(x, y)
        });
        let target_track = track_rect(project, drag.target_track as usize, current_frame, zone);
        let badge = sign_badge_rect(drag.target_tick, target_track, current_frame, zone, fps);
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
        let icon = sign_icon_rect(drag.target_tick, target_track, current_frame, zone, fps);
        detector_icons.push(IconInstance {
            rect: [icon.x, icon.y, icon.width, icon.height],
            uv_rect: palette_uv(&drag.cue, detection_uvs),
            tint: [0.78, 0.88, 1.0, 1.0],
            transform: [0.0, 0.0, 0.5, 0.5],
        });
    }

    // Scale bitmap signs horizontally with their semantic duration. Waves are
    // drawn below; the MouthOpen sign keeps its dedicated arrow icon.
    for track in 0..rythmo_layout::track_count() {
        let line_id = track_storage_line_id(track as u8);
        let Some(data) = project.detections().line(line_id) else {
            continue;
        };
        let rect = track_rect(project, track, current_frame, zone);
        for cue in data.source_detections() {
            let original = sign_icon_rect(cue.media_tick, rect, current_frame, zone, fps);
            if matches!(
                cue.kind,
                DetectionKind::OpeningWave | DetectionKind::ForwardWave
            ) {
                detector_icons.retain(|icon| {
                    let (x, y) = rect_center(icon.rect);
                    !original.contains(x, y)
                });
            } else if let Some(icon) = detector_icons.iter_mut().find(|icon| {
                let (x, y) = rect_center(icon.rect);
                original.contains(x, y)
            }) {
                let width = cue_width(cue);
                icon.rect[0] = original.x + original.width / 2.0 - width / 2.0;
                icon.rect[2] = width;
            }
        }
    }

    quads.extend(detector_quads);
    labels.extend(detector_labels);
    icons.extend(detector_icons);

    // Detection symbols stay unframed. Selection is shown with two compact
    // horizontal resize handles instead of the former circular badge.
    let selected = editable.then(|| selected_detection(state)).flatten();
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
            let center = tick_x(cue.media_tick, current_frame, zone, fps);
            let width =
                (cue.duration.as_frame_position().abs() as f32 * ppf()).max(SIGN_ICON_SIZE + 8.0);
            let top = rect.y - SIGN_BADGE_SIZE + 2.0;

            if matches!(
                cue.kind,
                DetectionKind::OpeningWave | DetectionKind::ForwardWave
            ) {
                let color = if selected == Some(address) {
                    [0.78, 0.88, 1.0, 1.0]
                } else {
                    [0.92, 0.92, 0.95, 0.94]
                };
                let segments = 20;
                for index in 0..segments {
                    let u0 = index as f32 / segments as f32;
                    let u1 = (index + 1) as f32 / segments as f32;
                    let curve = |u: f32| {
                        let arch = (std::f32::consts::PI * u).sin() * 9.0;
                        if matches!(cue.kind, DetectionKind::ForwardWave) {
                            top + 13.0 - arch
                        } else {
                            top + 4.0 + arch
                        }
                    };
                    push_line(
                        quads,
                        center - width / 2.0 + width * u0,
                        curve(u0),
                        center - width / 2.0 + width * u1,
                        curve(u1),
                        2.0,
                        color,
                    );
                }
            }

            if matches!(cue.kind, DetectionKind::Pucker) {
                let color = if selected == Some(address) {
                    [0.78, 0.88, 1.0, 1.0]
                } else {
                    [0.92, 0.92, 0.95, 0.94]
                };
                icons.push(IconInstance {
                    rect: [center - width / 2.0, top, width, SIGN_ICON_SIZE + 4.0],
                    uv_rect: palette_uv(cue, detection_uvs),
                    tint: color,
                    transform: [0.0, 0.0, 0.5, 0.5],
                });
            }

            if selected == Some(address) {
                for x in [center - width / 2.0, center + width / 2.0] {
                    push_quad(
                        quads,
                        Rect {
                            x: x - 2.0,
                            y: top + 7.0,
                            width: 4.0,
                            height: 12.0,
                        },
                        [0.68, 0.82, 1.0, 1.0],
                        2.0,
                    );
                }
            }
        }
    }

    for line in project.lines() {
        if !line.kind.is_dialogue() {
            continue;
        }
        let Some(data) = project.detections().line(line.id) else {
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
        for cue in data.source_detections() {
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: cue.id,
            };
            if matches!(
                cue.kind,
                DetectionKind::OpeningWave | DetectionKind::ForwardWave
            ) {
                let center = tick_x(cue.media_tick, current_frame, zone, fps);
                let width = cue_width(cue);
                let top = rect.y - SIGN_BADGE_SIZE + 2.0;
                let color = if selected == Some(address) {
                    [0.78, 0.88, 1.0, 1.0]
                } else {
                    [0.92, 0.92, 0.95, 0.94]
                };
                let segments = 20;
                for index in 0..segments {
                    let u0 = index as f32 / segments as f32;
                    let u1 = (index + 1) as f32 / segments as f32;
                    let curve = |u: f32| {
                        let arch = (std::f32::consts::PI * u).sin() * 9.0;
                        if matches!(cue.kind, DetectionKind::ForwardWave) {
                            top + 13.0 - arch
                        } else {
                            top + 4.0 + arch
                        }
                    };
                    push_line(
                        quads,
                        center - width / 2.0 + width * u0,
                        curve(u0),
                        center - width / 2.0 + width * u1,
                        curve(u1),
                        2.0,
                        color,
                    );
                }
            }
            if selected != Some(address) {
                continue;
            }
            let center = tick_x(cue.media_tick, current_frame, zone, fps);
            let width =
                (cue.duration.as_frame_position().abs() as f32 * ppf()).max(SIGN_ICON_SIZE + 8.0);
            let top = rect.y - SIGN_BADGE_SIZE + 2.0;
            for x in [center - width / 2.0, center + width / 2.0] {
                push_quad(
                    quads,
                    Rect {
                        x: x - 2.0,
                        y: top + 7.0,
                        width: 4.0,
                        height: 12.0,
                    },
                    [0.68, 0.82, 1.0, 1.0],
                    2.0,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piecewise_mapping_keeps_edges_and_natural_order() {
        let base = vec![0.0, 0.2, 0.4, 0.7, 1.0];
        let mapped = map_character_positions(&base, &[(1, 0.25), (3, 0.8)]);
        assert_eq!(mapped.first().copied(), Some(0.0));
        assert_eq!(mapped.last().copied(), Some(1.0));
        assert!(mapped.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn piecewise_mapping_keeps_sync_limits_after_text_length_changes() {
        // Simulate replacing the text between two persistent synchronization
        // points with a longer passage. Both point positions must stay exact.
        let spans = (0..10).map(|index| (index, index + 1)).collect::<Vec<_>>();
        let base = uniform_grapheme_character_positions(&spans);
        let mapped = map_character_positions(&base, &[(2, 0.25), (8, 0.75)]);

        assert!((mapped[2] - 0.25).abs() < 0.000_01);
        assert!((mapped[8] - 0.75).abs() < 0.000_01);
        assert!(mapped[2..=8].windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn sync_cursor_segments_use_the_same_mapped_geometry_as_rendering() {
        let boundaries = BTreeSet::from([0, 2, 5, 8]);
        let mapped = vec![0.0, 0.1, 0.25, 0.35, 0.45, 0.6, 0.72, 0.86, 1.0];
        let segments = sync_cursor_segments_from_layout(42, 8, &boundaries, &mapped);

        assert_eq!(segments.len(), 3);
        assert_eq!((segments[1].start_char, segments[1].end_char), (2, 5));
        assert!((segments[1].start_ratio - 0.25).abs() < 0.000_01);
        assert!((segments[1].width_ratio - 0.35).abs() < 0.000_01);
    }

    #[test]
    fn sync_text_uses_the_active_workspace_rect() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(100, 60, 0.25);
        project.get_line_mut(line_id).unwrap().text = "un cadeau pour Moondancer".into();
        let line = project.get_line(line_id).unwrap();
        let mut detections = crate::detection::DetectionDocument::default();
        detections
            .add_sync_point(
                line_id,
                line.text.graphemes(true).count(),
                MediaTick::from_frame(line.start_frame),
                MediaTick::from_frame(line.end_frame()),
                9,
                MediaTick::from_frame(130),
            )
            .unwrap();
        project.restore_line_detections(line_id, detections.line(line_id).unwrap().clone());

        let expected = Rect {
            x: 120.0,
            y: 340.0,
            width: 500.0,
            height: 48.0,
        };
        let mut stretched = Vec::new();
        let line = project.get_line(line_id).unwrap();
        render_sync_text_segments(
            &project,
            line,
            expected,
            None,
            project.syllable_language_code(),
            &RythmoState::new(),
            None,
            [1.0; 4],
            None,
            &mut stretched,
        )
        .unwrap();

        assert!(!stretched.is_empty());
        assert!(
            stretched
                .iter()
                .all(|text| text.dest_rect.y == expected.y
                    && text.dest_rect.height == expected.height)
        );
    }

    #[test]
    fn point_on_comma_keeps_comma_with_the_left_text_group() {
        let text = "You two, are our last hope.";
        let graphemes = UnicodeSegmentation::graphemes(text, true).collect::<Vec<_>>();
        let comma = graphemes
            .iter()
            .position(|grapheme| *grapheme == ",")
            .unwrap();
        let are = graphemes
            .windows(3)
            .position(|window| window == ["a", "r", "e"])
            .unwrap();
        let spans = grapheme_char_spans(text);
        let base = uniform_grapheme_character_positions(&spans);
        let comma_boundary =
            sync_anchor_char_boundary(text, &spans, comma, crate::detection::SyncAffinity::Auto)
                .unwrap();
        let are_boundary =
            sync_anchor_char_boundary(text, &spans, are, crate::detection::SyncAffinity::Auto)
                .unwrap();
        let comma_inverted =
            sync_anchor_char_boundary(text, &spans, comma, crate::detection::SyncAffinity::Right)
                .unwrap();
        let are_inverted =
            sync_anchor_char_boundary(text, &spans, are, crate::detection::SyncAffinity::Left)
                .unwrap();
        let mapped = map_character_positions(&base, &[(comma_boundary, 0.3), (are_boundary, 0.55)]);

        assert_eq!(
            text.chars().take(comma_boundary).collect::<String>(),
            "You two,"
        );
        assert_eq!(
            text.chars().skip(are_boundary).take(3).collect::<String>(),
            "are"
        );
        assert_eq!(comma_inverted, spans[comma].0);
        assert_eq!(are_inverted, spans[are].1);
        assert!((mapped[comma_boundary] - 0.3).abs() < 0.000_01);
        assert!((mapped[are_boundary] - 0.55).abs() < 0.000_01);
    }
}
