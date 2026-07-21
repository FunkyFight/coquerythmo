//! Detection UI facade.
//!
//! The legacy detector owns persistence, hit testing and menu semantics. This
//! facade owns cross-track source-sign dragging and the single piecewise text
//! geometry shared by rendering and caret placement.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use std::collections::{BTreeMap, BTreeSet};
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
const ADD_BUTTON_SIZE: f32 = 18.0;
const ADD_BUTTON_INSET: f32 = 2.0;
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

fn source_drag_slot() -> &'static Mutex<Option<SourceDrag>> {
    static SLOT: OnceLock<Mutex<Option<SourceDrag>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn source_drag() -> std::sync::MutexGuard<'static, Option<SourceDrag>> {
    source_drag_slot()
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

fn add_button_rect(hover: &DetectionHover) -> Rect {
    Rect {
        x: hover.screen_x - ADD_BUTTON_SIZE / 2.0,
        y: hover.track_rect.y + ADD_BUTTON_INSET,
        width: ADD_BUTTON_SIZE,
        height: ADD_BUTTON_SIZE,
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

fn clamp_sync_tick(project: &Project, address: DetectionAddress, tick: MediaTick) -> MediaTick {
    let Some(data) = project.detections().line(address.line_id) else {
        return tick;
    };
    let Some(current) = data.detection(address.detection_id) else {
        return tick;
    };
    let Some(current_index) = current.target.grapheme_index() else {
        return tick;
    };
    let mut minimum = MediaTick::ZERO;
    let mut maximum = MediaTick(i64::MAX);
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

        if state.detection_drag.is_some() {
            if let Some(address) = selected_detection(state).filter(|address| address.track().is_none())
            {
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address,
                    media_tick: clamp_sync_tick(
                        ctx.project,
                        address,
                        pointer_tick(*x, ctx.current_frame, ctx.zone),
                    ),
                }));
            }
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

        if state.detection_drag.is_some() {
            if let Some(address) = selected_detection(state).filter(|address| address.track().is_none())
            {
                state.detection_drag = None;
                if let Some(line) = ctx.project.get_line(address.line_id) {
                    if let Some((start_frame, duration_frames)) =
                        synchronized_line_bounds(ctx.project, line)
                    {
                        if start_frame != line.start_frame
                            || duration_frames != line.duration_frames
                        {
                            return Some(EventResponse::Action(UiAction::ResizeLine {
                                id: line.id,
                                start_frame,
                                duration_frames,
                            }));
                        }
                    }
                }
                return Some(EventResponse::Consumed);
            }
        }
    }

    if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
        *source_drag() = None;
    }

    if state.detection_menu.is_none() {
        match event {
            UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } => {
                if let Some(hover) = state.detection_hover {
                    if add_button_rect(&hover).contains(*x, *y) {
                        if state.open_detection_palette_from_hover() {
                            crate::detection_foreground::sync_from_state(
                                ctx.project,
                                state,
                                *ctx.zone,
                                ctx.current_frame,
                                event,
                            );
                        }
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
            }
            _ => {}
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
    let character_count = line.text.chars().count();
    let mut anchors = BTreeMap::new();
    for cue in data.text_sync_cues() {
        let Some(index) = cue.target.grapheme_index().map(|index| index as usize) else {
            continue;
        };
        if index < character_count {
            let ratio = ((cue.media_tick.as_frame_position() - line.start_frame as f64)
                / line.duration_frames as f64) as f32;
            anchors.insert(index, ratio.clamp(0.0, 1.0));
        }
    }
    anchors.into_iter().collect()
}

fn base_character_positions(
    character_count: usize,
    ratios: &[f32],
    breaks: &[usize],
) -> Vec<f32> {
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
    if let Some(first) = positions.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = positions.last_mut() {
        *last = 1.0;
    }
    positions
}

/// Piecewise affine mapping with fixed line endpoints. Sync points constrain the
/// character centres between those endpoints; every interval is therefore fully
/// occupied, without extrapolated whitespace or text spilling past its limits.
fn map_character_positions(base: &[f32], anchors: &[(usize, f32)]) -> Vec<f32> {
    if base.len() < 2 || anchors.is_empty() {
        return base.to_vec();
    }

    let mut controls = vec![(0.0_f32, 0.0_f32), (1.0_f32, 1.0_f32)];
    controls.extend(anchors.iter().filter_map(|(index, target)| {
        let left = *base.get(*index)?;
        let right = *base.get(index + 1)?;
        Some(((left + right) * 0.5, target.clamp(0.0, 1.0)))
    }));
    controls.sort_by(|left, right| left.0.total_cmp(&right.0));
    controls.dedup_by(|left, right| {
        if (left.0 - right.0).abs() <= 0.000_001 {
            right.1 = right.1.max(left.1);
            true
        } else {
            false
        }
    });

    let mut previous_target = 0.0_f32;
    for control in &mut controls {
        control.1 = control.1.max(previous_target).clamp(0.0, 1.0);
        previous_target = control.1;
    }
    if let Some(first) = controls.first_mut() {
        *first = (0.0, 0.0);
    }
    if let Some(last) = controls.last_mut() {
        *last = (1.0, 1.0);
    }

    let mut mapped = Vec::with_capacity(base.len());
    for source in base.iter().copied() {
        let mut value = source;
        for pair in controls.windows(2) {
            let (source_start, target_start) = pair[0];
            let (source_end, target_end) = pair[1];
            if source <= source_end || (source_end - 1.0).abs() <= f32::EPSILON {
                let local = ((source - source_start)
                    / (source_end - source_start).max(0.000_001))
                .clamp(0.0, 1.0);
                value = target_start + (target_end - target_start) * local;
                break;
            }
        }
        mapped.push(value.clamp(0.0, 1.0));
    }

    for index in 1..mapped.len() {
        mapped[index] = mapped[index].max(mapped[index - 1]);
    }
    if let Some(first) = mapped.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = mapped.last_mut() {
        *last = 1.0;
    }
    mapped
}

fn character_layout(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<(Vec<f32>, Vec<f32>, Vec<usize>, Vec<(usize, f32)>)> {
    let breaks = state.get_syllable_breaks(line, lang);
    let effective_drag = drag.filter(|drag| drag.line_id == line.id);
    let ratios = if let Some(drag) =
        effective_drag.filter(|drag| drag.ratios.len() == breaks.len() + 1)
    {
        drag.ratios.clone()
    } else {
        syllable_ratios_for_line(line, None, lang, state)?
    };
    let base = base_character_positions(line.text.chars().count(), &ratios, &breaks);
    let anchors = sync_anchors(project, line);
    let mapped = map_character_positions(&base, &anchors);
    Some((base, mapped, breaks, anchors))
}

/// Normal lines no longer expose syllable handles. Karaoke keeps the existing
/// native handle geometry and never has text synchronization points.
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

pub(crate) fn render_sync_text_segments(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
    read_highlight_end: Option<usize>,
    tint: [f32; 4],
    stretched: &mut Vec<StretchedText>,
) -> Option<Vec<CursorSegmentInfo>> {
    if line.karaoke || line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }
    let (_, mapped, syllable_breaks, anchors) =
        character_layout(project, line, drag, lang, state)?;
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
    boundaries.extend(anchors.iter().map(|(index, _)| *index));
    let boundaries = boundaries.into_iter().collect::<Vec<_>>();
    let characters = line.text.chars().collect::<Vec<_>>();
    let rect = line_rect(project, line, current_frame, zone);
    let mut cursor_segments = Vec::new();
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if end <= start || end > character_count {
            continue;
        }
        let start_ratio = mapped[start];
        let width_ratio = (mapped[end] - mapped[start]).max(0.000_1);
        let width = rect.width * width_ratio;
        if width <= 0.5 {
            continue;
        }
        let text = characters[start..end].iter().collect::<String>();
        if text.is_empty() {
            continue;
        }
        let cache_id = sync_segment_cache_id(line.id, start, end);
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
        cursor_segments.push(CursorSegmentInfo {
            cache_id,
            start_char: start,
            end_char: end,
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

fn synchronized_line_bounds(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Option<(i64, i64)> {
    let data = project.detections().line(line.id)?;
    let mut start = line.start_frame;
    let mut end = line.end_frame();
    let mut found = false;
    for cue in data.text_sync_cues() {
        found = true;
        start = start.min(cue.media_tick.as_frame_position().floor() as i64);
        end = end.max(cue.media_tick.as_frame_position().ceil() as i64);
    }
    found.then_some((start, end.saturating_sub(start).max(1)))
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

    // Normal lines no longer display the legacy red syllable-handle suffix.
    detector_quads.retain(|quad| quad.color != [0.95, 0.08, 0.03, 1.0]);

    // The add button is composed once in detection_foreground, after text,
    // handles and icons. Remove the legacy below-track copy from this pass.
    if state.detection_menu.is_none() {
        if let Some(hover) = state.detection_hover {
            let legacy_button = Rect {
                x: hover.screen_x - ADD_BUTTON_SIZE / 2.0,
                y: hover.track_rect.y + hover.track_rect.height + 4.0,
                width: ADD_BUTTON_SIZE,
                height: ADD_BUTTON_SIZE,
            };
            detector_quads.retain(|quad| {
                let (x, y) = rect_center(quad.rect);
                !legacy_button.contains(x, y)
            });
        }
    }

    // Move source signs below synchronization points. Hit testing keeps the
    // generous original badge rectangle, while the visible glyph is dropped.
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
                !original_badge.contains(x, y)
                    || quad.rect[3] > original_badge.height + 2.0
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

    let drag_snapshot = source_drag().clone().filter(|drag| drag.moved);
    if let Some(drag) = drag_snapshot.as_ref() {
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

    quads.extend(detector_quads);
    labels.extend(detector_labels);
    icons.extend(detector_icons);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_button_is_reachable_without_leaving_track() {
        let hover = DetectionHover {
            track: 0,
            media_tick: MediaTick::ZERO,
            screen_x: 120.0,
            screen_y: 40.0,
            track_rect: Rect {
                x: 0.0,
                y: 20.0,
                width: 300.0,
                height: 50.0,
            },
        };
        let button = add_button_rect(&hover);
        assert!(hover.track_rect.contains(button.x, button.y));
        assert!(hover
            .track_rect
            .contains(button.x + button.width, button.y + button.height));
    }

    #[test]
    fn synchronized_mapping_fills_the_complete_line() {
        let base = vec![0.0, 0.2, 0.4, 0.7, 1.0];
        let mapped = map_character_positions(&base, &[(1, 0.25), (3, 0.8)]);
        assert_eq!(mapped.first().copied(), Some(0.0));
        assert_eq!(mapped.last().copied(), Some(1.0));
        assert!(mapped.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!((((mapped[1] + mapped[2]) * 0.5) - 0.25).abs() < 0.0001);
        assert!((((mapped[3] + mapped[4]) * 0.5) - 0.8).abs() < 0.0001);
    }

    #[test]
    fn source_sign_display_is_lower_than_its_hit_badge() {
        let track = Rect {
            x: 0.0,
            y: 20.0,
            width: 300.0,
            height: 50.0,
        };
        let original = sign_badge_rect(MediaTick::ZERO, track, 0.0, &track);
        let displayed = displayed_badge_rect(MediaTick::ZERO, track, 0.0, &track);
        assert!(displayed.y > original.y);
    }
}
