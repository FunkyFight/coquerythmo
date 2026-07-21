//! Editor-only interaction and rendering for professional detection signs and
//! per-letter synchronization points.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use std::collections::{BTreeMap, BTreeSet};

const DETECTION_ICON_SIZE: f32 = 18.0;
const DETECTION_HIT_SIZE: f32 = 26.0;
const DETECTION_BUTTON_SIZE: f32 = 18.0;
const DETECTION_BUTTON_GAP: f32 = 4.0;
const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const SYNC_DOT_SIZE: f32 = 6.0;
const SYNC_DOT_HIT_PADDING: f32 = 3.0;
const SYNC_SYLLABLE_DRAG_MASK: u64 = 1_u64 << 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteSign {
    Labial,
    SemiLabial,
    MouthOpen,
    MouthClosed,
    TeethVisible,
    DentalTh,
    Breath,
    Neutral,
    Reaction,
}

impl PaletteSign {
    const ALL: [Self; 9] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpen,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::DentalTh,
        Self::Breath,
        Self::Neutral,
        Self::Reaction,
    ];

    fn storage(self) -> (DetectionKind, TextAnchor) {
        match self {
            Self::Labial => (DetectionKind::Labial, TextAnchor::BeforeText),
            Self::SemiLabial => (DetectionKind::SemiLabial, TextAnchor::BeforeText),
            Self::MouthOpen => (DetectionKind::MouthOpen, TextAnchor::BeforeText),
            Self::MouthClosed => (DetectionKind::MouthClosed, TextAnchor::BeforeText),
            Self::TeethVisible => (DetectionKind::TeethVisible, TextAnchor::BeforeText),
            Self::DentalTh => (DetectionKind::TeethVisible, TextAnchor::AfterText),
            Self::Breath => (DetectionKind::Breath, TextAnchor::BeforeText),
            Self::Neutral => (DetectionKind::Breath, TextAnchor::AfterText),
            Self::Reaction => (DetectionKind::Reaction, TextAnchor::BeforeText),
        }
    }

    fn from_cue(cue: &DetectionCue) -> Option<Self> {
        let alternate = matches!(&cue.target, TextAnchor::AfterText);
        match (cue.kind, alternate) {
            (DetectionKind::Labial, _) => Some(Self::Labial),
            (DetectionKind::SemiLabial, _) => Some(Self::SemiLabial),
            (DetectionKind::MouthOpen, _) => Some(Self::MouthOpen),
            (DetectionKind::MouthClosed, _) => Some(Self::MouthClosed),
            (DetectionKind::TeethVisible, false) => Some(Self::TeethVisible),
            (DetectionKind::TeethVisible, true) => Some(Self::DentalTh),
            (DetectionKind::Breath, false) => Some(Self::Breath),
            (DetectionKind::Breath, true) => Some(Self::Neutral),
            (DetectionKind::Reaction, _) => Some(Self::Reaction),
            (DetectionKind::TextSyncPoint, _) => None,
        }
    }

    fn legacy_uv_index(self) -> Option<usize> {
        match self {
            Self::Labial => Some(0),
            Self::SemiLabial => Some(1),
            Self::MouthOpen => Some(2),
            Self::MouthClosed => Some(3),
            Self::TeethVisible => Some(4),
            Self::Breath => Some(5),
            Self::Reaction => Some(6),
            Self::DentalTh | Self::Neutral => None,
        }
    }
}

const MENU_WIDTH: f32 = MENU_PADDING * 2.0
    + MENU_ICON_SIZE * PaletteSign::ALL.len() as f32
    + MENU_GAP * (PaletteSign::ALL.len() as f32 - 1.0);
const MENU_HEIGHT: f32 = MENU_ICON_SIZE + MENU_PADDING * 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionHover {
    pub track: u8,
    pub media_tick: MediaTick,
    pub screen_x: f32,
    pub screen_y: f32,
    pub track_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionMenu {
    pub track: u8,
    pub media_tick: MediaTick,
    pub x: f32,
    pub y: f32,
    pub hover_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionDrag {
    pub address: DetectionAddress,
}

impl RythmoState {
    pub(crate) fn open_detection_palette_from_hover(&mut self) -> bool {
        let Some(hover) = self.detection_hover else {
            return false;
        };
        let button = detection_button_rect(&hover);
        self.detection_menu = Some(DetectionMenu {
            track: hover.track,
            media_tick: hover.media_tick,
            x: button.x,
            y: button.y + button.height + 2.0,
            hover_index: None,
        });
        true
    }
}

pub(crate) fn encode_sync_syllable_drag_line_id(line_id: u64) -> u64 {
    line_id | SYNC_SYLLABLE_DRAG_MASK
}

pub(crate) fn decode_sync_syllable_drag_line_id(line_id: u64) -> Option<u64> {
    (line_id & SYNC_SYLLABLE_DRAG_MASK != 0).then_some(line_id & !SYNC_SYLLABLE_DRAG_MASK)
}

fn selected_address(state: &RythmoState) -> Option<DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

fn track_body_rect(ctx: &RythmoCtx<'_>, track: usize) -> Rect {
    editor_track_body_rect_at_frame(
        ctx.project,
        rythmo_layout::y_slot_for_track_index(track),
        ctx.current_frame,
        ctx.zone,
    )
}

fn track_under_pointer(ctx: &RythmoCtx<'_>, y: f32) -> Option<(u8, Rect)> {
    (0..rythmo_layout::track_count()).find_map(|track| {
        let rect = track_body_rect(ctx, track);
        (y >= rect.y && y <= rect.y + rect.height).then_some((track as u8, rect))
    })
}

fn detection_button_rect(hover: &DetectionHover) -> Rect {
    Rect {
        x: hover.screen_x - DETECTION_BUTTON_SIZE / 2.0,
        y: hover.track_rect.y + hover.track_rect.height + DETECTION_BUTTON_GAP,
        width: DETECTION_BUTTON_SIZE,
        height: DETECTION_BUTTON_SIZE,
    }
}

fn source_icon_rect(tick: MediaTick, track_rect: Rect, current_frame: f64, zone: &Rect) -> Rect {
    Rect {
        x: tick_x(tick, current_frame, zone) - DETECTION_HIT_SIZE / 2.0,
        y: track_rect.y + (track_rect.height - DETECTION_HIT_SIZE) / 2.0,
        width: DETECTION_HIT_SIZE,
        height: DETECTION_HIT_SIZE,
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

fn menu_rect(menu: &DetectionMenu, zone: &Rect) -> Rect {
    Rect {
        x: menu
            .x
            .clamp(zone.x, (zone.x + zone.width - MENU_WIDTH).max(zone.x)),
        y: menu
            .y
            .clamp(zone.y, (zone.y + zone.height - MENU_HEIGHT).max(zone.y)),
        width: MENU_WIDTH,
        height: MENU_HEIGHT,
    }
}

fn menu_item_rect(menu: &DetectionMenu, zone: &Rect, index: usize) -> Rect {
    let outer = menu_rect(menu, zone);
    Rect {
        x: outer.x + MENU_PADDING + index as f32 * (MENU_ICON_SIZE + MENU_GAP),
        y: outer.y + MENU_PADDING,
        width: MENU_ICON_SIZE,
        height: MENU_ICON_SIZE,
    }
}

fn palette_uv(sign: PaletteSign, detection_uvs: [[f32; 4]; 7]) -> [f32; 4] {
    if let Some(index) = sign.legacy_uv_index() {
        return detection_uvs[index];
    }

    let cell_width = detection_uvs[0][2] - detection_uvs[0][0];
    let extra_index = match sign {
        PaletteSign::DentalTh => 0.0,
        PaletteSign::Neutral => 1.0,
        _ => unreachable!(),
    };
    let u_min = detection_uvs[6][2] + cell_width * extra_index;
    [
        u_min,
        detection_uvs[6][1],
        u_min + cell_width,
        detection_uvs[6][3],
    ]
}

fn has_sync_cues(project: &Project, line: &crate::rythmo_line::RythmoLine) -> bool {
    !line.karaoke
        && project
            .detections()
            .line(line.id)
            .is_some_and(|data| data.text_sync_cues().next().is_some())
}

pub(crate) fn line_has_visible_sync_points(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> bool {
    has_sync_cues(project, line)
}

fn effective_drag_for_line<'a>(
    line_id: u64,
    drag: Option<&'a SyllableDrag>,
    state: &'a RythmoState,
) -> Option<&'a SyllableDrag> {
    drag.filter(|drag| drag.line_id == line_id).or_else(|| {
        state.syllable_drag.as_ref().filter(|drag| {
            drag.line_id == line_id
                || decode_sync_syllable_drag_line_id(drag.line_id) == Some(line_id)
        })
    })
}

fn base_character_ratios(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> (Vec<f32>, Vec<usize>) {
    let character_count = line.text.chars().count();
    let mut positions = (0..=character_count)
        .map(|index| index as f32 / character_count.max(1) as f32)
        .collect::<Vec<_>>();

    let breaks = state.get_syllable_breaks(line, lang);
    if breaks.is_empty() {
        return (positions, Vec::new());
    }
    let effective_drag = effective_drag_for_line(line.id, drag, state);
    let ratios = if let Some(drag) = effective_drag
        .filter(|drag| drag.ratios.len() == breaks.len() + 1)
    {
        drag.ratios.clone()
    } else if let Some(ratios) = syllable_ratios_for_line(line, None, lang, state) {
        ratios
    } else {
        return (positions, Vec::new());
    };

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
    (positions, breaks)
}

fn sync_anchor_targets(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Vec<(usize, f32)> {
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
        if index >= character_count {
            continue;
        }
        let ratio = ((cue.media_tick.as_frame_position() - line.start_frame as f64)
            / line.duration_frames as f64) as f32;
        anchors.insert(index, ratio);
    }
    anchors.into_iter().collect()
}

fn shift_character_ratios(base: &[f32], anchors: &[(usize, f32)]) -> Vec<f32> {
    let mut shifted = base.to_vec();
    let mut offset = 0.0_f32;
    let mut anchor_index = 0usize;

    for boundary in 0..base.len() {
        while anchor_index < anchors.len() && anchors[anchor_index].0 == boundary {
            if boundary + 1 < base.len() {
                let base_center = (base[boundary] + base[boundary + 1]) * 0.5;
                offset = anchors[anchor_index].1 - base_center;
            }
            anchor_index += 1;
        }
        shifted[boundary] = base[boundary] + offset;
    }
    shifted
}

fn character_center_ratio(positions: &[f32], character_index: usize) -> Option<f32> {
    Some((positions.get(character_index)? + positions.get(character_index + 1)?) * 0.5)
}

fn character_layout(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> (Vec<f32>, Vec<f32>, Vec<usize>, Vec<(usize, f32)>) {
    let (base, breaks) = base_character_ratios(line, drag, lang, state);
    let anchors = sync_anchor_targets(project, line);
    let shifted = if anchors.is_empty() {
        base.clone()
    } else {
        shift_character_ratios(&base, &anchors)
    };
    (base, shifted, breaks, anchors)
}

pub(crate) fn sync_syllable_boundary_ratios(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> Option<Vec<f32>> {
    if !has_sync_cues(project, line) {
        return None;
    }
    let (_, shifted, breaks, _) = character_layout(project, line, drag, lang, state);
    if breaks.is_empty() {
        return None;
    }
    let character_count = line.text.chars().count();
    let mut boundaries = Vec::with_capacity(breaks.len() + 2);
    boundaries.push(*shifted.first()?);
    boundaries.extend(
        breaks
            .into_iter()
            .filter(|index| *index < shifted.len())
            .map(|index| shifted[index]),
    );
    boundaries.push(shifted[character_count]);
    Some(boundaries)
}

fn sync_point_x(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    cue: &DetectionCue,
    current_frame: f64,
    zone: &Rect,
    state: &RythmoState,
) -> Option<f32> {
    if line.karaoke {
        return None;
    }
    let index = cue.target.grapheme_index()? as usize;
    let lang = project.syllable_language_code();
    let (_, shifted, _, _) = character_layout(project, line, None, lang, state);
    let center = character_center_ratio(&shifted, index)?;
    let rect = line_rect(project, line, current_frame, zone);
    Some(rect.x + rect.width * center)
}

fn existing_sync_at(project: &Project, line_id: u64, character_index: usize) -> bool {
    project.detections().line(line_id).is_some_and(|data| {
        data.text_sync_cues()
            .any(|cue| cue.target.grapheme_index() == Some(character_index as u32))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SyncPlaceholder {
    line_id: u64,
    character_index: usize,
    media_tick: MediaTick,
    x: f32,
    line_rect: Rect,
}

fn sync_placeholder_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    state: &RythmoState,
    x: f32,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<SyncPlaceholder> {
    let characters: Vec<char> = line.text.chars().collect();
    if line.karaoke || characters.is_empty() || line.duration_frames <= 0 {
        return None;
    }

    let line_rect = line_rect(project, line, current_frame, zone);
    if !line_rect.contains(x, y) || line_rect.width <= 0.0 {
        return None;
    }

    let lang = project.syllable_language_code();
    let (_, shifted, _, _) = character_layout(project, line, None, lang, state);
    let pointer_ratio = (x - line_rect.x) / line_rect.width;
    let mut candidate = None;
    for index in 0..characters.len() {
        if characters[index].is_whitespace() || existing_sync_at(project, line.id, index) {
            continue;
        }
        let left = shifted[index].min(shifted[index + 1]);
        let right = shifted[index].max(shifted[index + 1]);
        if pointer_ratio < left || pointer_ratio > right {
            continue;
        }
        let center = character_center_ratio(&shifted, index)?;
        let distance = (pointer_ratio - center).abs();
        if candidate
            .as_ref()
            .map_or(true, |(_, best_distance): &(usize, f32)| distance < *best_distance)
        {
            candidate = Some((index, distance));
        }
    }
    let character_index = candidate?.0;
    let center_ratio = character_center_ratio(&shifted, character_index)?;
    let frame = line.start_frame as f64 + line.duration_frames as f64 * center_ratio as f64;
    Some(SyncPlaceholder {
        line_id: line.id,
        character_index,
        media_tick: MediaTick::from_frame_position(frame),
        x: line_rect.x + line_rect.width * center_ratio,
        line_rect,
    })
}

fn line_under_pointer<'a>(
    project: &'a Project,
    state: &RythmoState,
    x: f32,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project.lines().find(|line| {
        !line.karaoke
            && line_rect(project, line, current_frame, zone).contains(x, y)
            && (!line.text.is_empty() || has_sync_cues(project, line))
            && state.editing_character != Some(line.id)
    })
}

fn hit_sync_placeholder(
    ctx: &RythmoCtx<'_>,
    state: &RythmoState,
    x: f32,
    y: f32,
) -> Option<(u64, usize, MediaTick)> {
    ctx.project.lines().find_map(|line| {
        let placeholder = sync_placeholder_for_line(
            ctx.project,
            line,
            state,
            x,
            y,
            ctx.current_frame,
            ctx.zone,
        )?;
        let hit = expanded_rect(
            sync_dot_rect(placeholder.x, placeholder.line_rect),
            SYNC_DOT_HIT_PADDING,
        );
        hit.contains(x, y).then_some((
            placeholder.line_id,
            placeholder.character_index,
            placeholder.media_tick,
        ))
    })
}

fn hit_existing_detection(
    ctx: &RythmoCtx<'_>,
    state: &RythmoState,
    x: f32,
    y: f32,
) -> Option<DetectionAddress> {
    for track in 0..rythmo_layout::track_count() {
        let line_id = track_storage_line_id(track as u8);
        let Some(data) = ctx.project.detections().line(line_id) else {
            continue;
        };
        let rect = track_body_rect(ctx, track);
        for cue in data.source_detections() {
            if source_icon_rect(cue.media_tick, rect, ctx.current_frame, ctx.zone).contains(x, y) {
                return Some(DetectionAddress {
                    line_id,
                    detection_id: cue.id,
                });
            }
        }
    }

    for line in ctx.project.lines() {
        if line.karaoke {
            continue;
        }
        let Some(data) = ctx.project.detections().line(line.id) else {
            continue;
        };
        let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        for cue in data.text_sync_cues() {
            let Some(cue_x) = sync_point_x(
                ctx.project,
                line,
                cue,
                ctx.current_frame,
                ctx.zone,
                state,
            ) else {
                continue;
            };
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

fn clamp_sync_drag_tick(
    project: &Project,
    address: DetectionAddress,
    tick: MediaTick,
) -> MediaTick {
    if address.track().is_some() {
        return tick;
    }
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
    if minimum > maximum {
        return minimum;
    }
    tick.clamp(minimum, maximum)
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
    if line.karaoke {
        return None;
    }
    let character_count = line.text.chars().count();
    if character_count == 0 || line.duration_frames <= 0 {
        return None;
    }

    let (base_positions, shifted_positions, syllable_breaks, anchors) =
        character_layout(project, line, drag, lang, state);
    if anchors.is_empty() {
        return None;
    }

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

        let start_ratio = shifted_positions[start];
        let width_ratio = (base_positions[end] - base_positions[start]).max(0.0);
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

fn navigate_detection(project: &Project, state: &mut RythmoState, direction: i32) -> bool {
    let Some(address) = selected_address(state) else {
        return false;
    };
    let Some(data) = project.detections().line(address.line_id) else {
        return false;
    };
    let cues = if address.track().is_some() {
        data.source_detections().collect::<Vec<_>>()
    } else {
        data.text_sync_cues().collect::<Vec<_>>()
    };
    if cues.is_empty() {
        return false;
    }
    let current = cues
        .iter()
        .position(|cue| cue.id == address.detection_id)
        .unwrap_or(0);
    let index = if direction < 0 {
        current.checked_sub(1).unwrap_or(cues.len() - 1)
    } else {
        (current + 1) % cues.len()
    };
    state.selected = Some(Selection::Detection(DetectionAddress {
        line_id: address.line_id,
        detection_id: cues[index].id,
    }));
    true
}

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    match event {
        UiEvent::MouseMove { x, y } => {
            if let Some(drag) = state.detection_drag {
                let mut tick = pointer_tick(*x, ctx.current_frame, ctx.zone);
                if drag.address.track().is_none() {
                    tick = clamp_sync_drag_tick(ctx.project, drag.address, tick);
                }
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address: drag.address,
                    media_tick: tick,
                }));
            }

            if let Some(mut menu) = state.detection_menu {
                menu.hover_index = PaletteSign::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                    .map(|(index, _)| index);
                state.detection_menu = Some(menu);
                return Some(EventResponse::Consumed);
            }

            if state
                .detection_hover
                .is_some_and(|hover| detection_button_rect(&hover).contains(*x, *y))
            {
                return Some(EventResponse::Consumed);
            }

            state.detection_hover =
                track_under_pointer(ctx, *y).map(|(track, rect)| DetectionHover {
                    track,
                    media_tick: pointer_tick(*x, ctx.current_frame, ctx.zone),
                    screen_x: *x,
                    screen_y: *y,
                    track_rect: rect,
                });

            if line_under_pointer(
                ctx.project,
                state,
                *x,
                *y,
                ctx.current_frame,
                ctx.zone,
            )
            .is_some_and(|line| has_sync_cues(ctx.project, line))
            {
                state.hovered_line = None;
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::MousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                if let Some((_, sign)) = PaletteSign::ALL
                    .iter()
                    .copied()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                {
                    let (kind, target) = sign.storage();
                    state.detection_menu = None;
                    return Some(EventResponse::Action(UiAction::AddDetection {
                        line_id: track_storage_line_id(menu.track),
                        kind,
                        media_tick: menu.media_tick,
                        target,
                    }));
                }
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }

            if let Some(address) = hit_existing_detection(ctx, state, *x, *y) {
                state.selected = Some(Selection::Detection(address));
                state.detection_drag = Some(DetectionDrag { address });
                return Some(EventResponse::Consumed);
            }

            if let Some((line_id, character_index, tick)) =
                hit_sync_placeholder(ctx, state, *x, *y)
            {
                return Some(EventResponse::Action(UiAction::AddDetection {
                    line_id,
                    kind: DetectionKind::TextSyncPoint,
                    media_tick: tick,
                    target: TextAnchor::Grapheme {
                        index: character_index as u32,
                    },
                }));
            }

            if state
                .detection_hover
                .is_some_and(|hover| detection_button_rect(&hover).contains(*x, *y))
            {
                state.open_detection_palette_from_hover();
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::MouseRelease { .. } if state.detection_drag.is_some() => {
            state.detection_drag = None;
            return Some(EventResponse::Consumed);
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            if state.detection_menu.take().is_some() {
                return Some(EventResponse::Consumed);
            }
            if let Some(address) = selected_address(state) {
                state.selected = if address.track().is_some() {
                    None
                } else {
                    Some(Selection::Line(address.line_id))
                };
                state.detection_drag = None;
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorLeft => {
            if navigate_detection(ctx.project, state, -1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorRight => {
            if navigate_detection(ctx.project, state, 1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::Delete => {
            if let Some(address) = selected_address(state) {
                return Some(EventResponse::Action(UiAction::DeleteDetection { address }));
            }
        }
        _ => {}
    }
    None
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
        shadow_color: [0.0, 0.0, 0.0, 0.18],
        shadow_blur: 1.5,
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
    let center_x = (x1 + x2) * 0.5;
    let center_y = (y1 + y2) * 0.5;
    quads.push(QuadInstance {
        rect: [
            center_x - length / 2.0,
            center_y - thickness / 2.0,
            length,
            thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: thickness / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0, 0.0, 0.0, 0.18],
        shadow_blur: 1.0,
        rotation: dy.atan2(dx),
        _padding: [0.0; 2],
    });
}

fn render_shifted_syllable_handles(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
) {
    let lang = project.syllable_language_code();
    let drag = effective_drag_for_line(line.id, state.syllable_drag.as_ref(), state);
    let Some(boundary_ratios) =
        sync_syllable_boundary_ratios(project, line, drag, lang, state)
    else {
        return;
    };
    if boundary_ratios.len() <= 2 {
        return;
    }

    let rect = line_rect(project, line, current_frame, zone);
    let color = [0.95, 0.08, 0.03, 1.0];
    let stroke = 3.0;
    let tick_h = 9.0;
    let top_y = rect.y + 1.0;
    let cap_gap = 2.0;
    let boundaries = boundary_ratios
        .into_iter()
        .map(|ratio| rect.x + rect.width * ratio)
        .collect::<Vec<_>>();

    for pair in boundaries.windows(2) {
        let start = pair[0] + cap_gap;
        let end = pair[1] - cap_gap;
        if end > start {
            push_quad(
                quads,
                Rect {
                    x: start,
                    y: top_y,
                    width: end - start,
                    height: stroke,
                },
                color,
                stroke / 2.0,
            );
        }
    }
    for boundary in boundaries {
        push_quad(
            quads,
            Rect {
                x: boundary - stroke / 2.0,
                y: top_y,
                width: stroke,
                height: tick_h,
            },
            color,
            stroke / 2.0,
        );
    }
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    _labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
    detection_uvs: [[f32; 4]; 7],
) {
    let selected_address = selected_address(state);
    for track in 0..rythmo_layout::track_count() {
        let line_id = track_storage_line_id(track as u8);
        let Some(data) = project.detections().line(line_id) else {
            continue;
        };
        let rect = editor_track_body_rect_at_frame(
            project,
            rythmo_layout::y_slot_for_track_index(track),
            current_frame,
            zone,
        );
        for cue in data.source_detections() {
            let x = tick_x(cue.media_tick, current_frame, zone);
            if x < zone.x - DETECTION_HIT_SIZE || x > zone.x + zone.width + DETECTION_HIT_SIZE {
                continue;
            }
            let address = DetectionAddress {
                line_id,
                detection_id: cue.id,
            };
            let selected = selected_address == Some(address);
            let hit = source_icon_rect(cue.media_tick, rect, current_frame, zone);
            if selected {
                push_quad(
                    quads,
                    Rect {
                        x: hit.x + 1.0,
                        y: hit.y + 1.0,
                        width: hit.width - 2.0,
                        height: hit.height - 2.0,
                    },
                    [0.20, 0.42, 0.88, 0.24],
                    hit.width / 2.0,
                );
            }
            push_line(
                quads,
                x,
                rect.y + 2.0,
                x,
                rect.y + rect.height - 2.0,
                if selected { 1.5 } else { 1.0 },
                if selected {
                    [0.55, 0.73, 1.0, 0.82]
                } else {
                    [0.72, 0.74, 0.80, 0.42]
                },
            );
            if let Some(sign) = PaletteSign::from_cue(cue) {
                icons.push(IconInstance {
                    rect: [
                        hit.x + (hit.width - DETECTION_ICON_SIZE) / 2.0,
                        hit.y + (hit.height - DETECTION_ICON_SIZE) / 2.0,
                        DETECTION_ICON_SIZE,
                        DETECTION_ICON_SIZE,
                    ],
                    uv_rect: palette_uv(sign, detection_uvs),
                    tint: if selected {
                        [0.78, 0.88, 1.0, 1.0]
                    } else {
                        [0.92, 0.92, 0.95, 0.94]
                    },
                });
            }
        }
    }

    for line in project.lines() {
        if line.karaoke {
            continue;
        }
        let Some(data) = project.detections().line(line.id) else {
            continue;
        };
        let rect = line_rect(project, line, current_frame, zone);
        for cue in data.text_sync_cues() {
            let Some(cue_x) = sync_point_x(project, line, cue, current_frame, zone, state) else {
                continue;
            };
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: cue.id,
            };
            let selected = selected_address == Some(address);
            let dot = sync_dot_rect(cue_x, rect);
            let extra = if selected { 1.5 } else { 0.0 };
            push_quad(
                quads,
                Rect {
                    x: dot.x - extra,
                    y: dot.y - extra,
                    width: dot.width + extra * 2.0,
                    height: dot.height + extra * 2.0,
                },
                if selected {
                    [0.72, 0.88, 1.0, 1.0]
                } else {
                    [0.48, 0.72, 1.0, 0.96]
                },
                8.0,
            );
        }
    }

    let hovered_sync_line = state.detection_hover.and_then(|hover| {
        line_under_pointer(
            project,
            state,
            hover.screen_x,
            hover.screen_y,
            current_frame,
            zone,
        )
        .filter(|line| has_sync_cues(project, line))
        .map(|line| line.id)
    });
    let dragged_sync_line = state
        .syllable_drag
        .as_ref()
        .and_then(|drag| decode_sync_syllable_drag_line_id(drag.line_id));
    let mut handle_lines = BTreeSet::new();
    handle_lines.extend(hovered_sync_line);
    handle_lines.extend(dragged_sync_line);
    for line_id in handle_lines {
        if let Some(line) = project.get_line(line_id) {
            render_shifted_syllable_handles(project, line, current_frame, zone, state, quads);
        }
    }

    if state.detection_menu.is_none() && state.detection_drag.is_none() {
        if let Some(hover) = state.detection_hover {
            if let Some(line) = line_under_pointer(
                project,
                state,
                hover.screen_x,
                hover.screen_y,
                current_frame,
                zone,
            ) {
                if let Some(placeholder) = sync_placeholder_for_line(
                    project,
                    line,
                    state,
                    hover.screen_x,
                    hover.screen_y,
                    current_frame,
                    zone,
                ) {
                    push_quad(
                        quads,
                        sync_dot_rect(placeholder.x, placeholder.line_rect),
                        [0.70, 0.72, 0.78, 0.48],
                        6.0,
                    );
                }
            }
        }
    }

    if let Some(hover) = state.detection_hover {
        let x = tick_x(hover.media_tick, current_frame, zone);
        let mut y = hover.track_rect.y + 2.0;
        while y < hover.track_rect.y + hover.track_rect.height - 2.0 {
            push_quad(
                quads,
                Rect {
                    x: x - 0.5,
                    y,
                    width: 1.0,
                    height: 3.0_f32.min(hover.track_rect.y + hover.track_rect.height - y),
                },
                [0.68, 0.70, 0.76, 0.52],
                0.5,
            );
            y += 6.0;
        }
        let button = detection_button_rect(&DetectionHover {
            screen_x: x,
            ..hover
        });
        push_quad(quads, button, [0.10, 0.11, 0.14, 0.94], 4.0);
        push_line(
            quads,
            button.x + 5.0,
            button.y + button.height / 2.0,
            button.x + button.width - 5.0,
            button.y + button.height / 2.0,
            1.5,
            [0.90, 0.92, 0.96, 1.0],
        );
        push_line(
            quads,
            button.x + button.width / 2.0,
            button.y + 5.0,
            button.x + button.width / 2.0,
            button.y + button.height - 5.0,
            1.5,
            [0.90, 0.92, 0.96, 1.0],
        );
    }

    if let Some(menu) = state.detection_menu {
        let outer = menu_rect(&menu, zone);
        push_quad(quads, outer, [0.045, 0.048, 0.060, 0.985], 7.0);
        for (index, sign) in PaletteSign::ALL.iter().copied().enumerate() {
            let item = menu_item_rect(&menu, zone, index);
            if menu.hover_index == Some(index) {
                push_quad(quads, item, [0.18, 0.32, 0.58, 0.82], 5.0);
            }
            icons.push(IconInstance {
                rect: [
                    item.x + 5.0,
                    item.y + 5.0,
                    item.width - 10.0,
                    item.height - 10.0,
                ],
                uv_rect: palette_uv(sign, detection_uvs),
                tint: [0.94, 0.95, 0.98, 1.0],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_time_rounds_to_a_tenth_frame() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let center = zone.x + zone.width / 2.0;
        assert_eq!(
            pointer_tick(center + ppf() * 0.34, 100.0, &zone),
            MediaTick(1003)
        );
        assert_eq!(
            pointer_tick(center + ppf() * 0.36, 100.0, &zone),
            MediaTick(1004)
        );
    }

    #[test]
    fn palette_contains_nine_distinct_professional_signs() {
        assert_eq!(PaletteSign::ALL.len(), 9);
        assert_ne!(PaletteSign::DentalTh.storage(), PaletteSign::TeethVisible.storage());
        assert_ne!(PaletteSign::Neutral.storage(), PaletteSign::Breath.storage());
    }

    #[test]
    fn detection_button_is_below_track_body() {
        let hover = DetectionHover {
            track: 0,
            media_tick: MediaTick::ZERO,
            screen_x: 100.0,
            screen_y: 30.0,
            track_rect: Rect {
                x: 0.0,
                y: 20.0,
                width: 200.0,
                height: 30.0,
            },
        };
        assert!(detection_button_rect(&hover).y > hover.track_rect.y + hover.track_rect.height);
    }

    #[test]
    fn click_must_hit_the_visible_sync_dot() {
        let line_rect = Rect {
            x: 0.0,
            y: 20.0,
            width: 200.0,
            height: 30.0,
        };
        let dot = sync_dot_rect(50.0, line_rect);
        assert!(expanded_rect(dot, SYNC_DOT_HIT_PADDING).contains(50.0, dot.y + 2.0));
        assert!(!expanded_rect(dot, SYNC_DOT_HIT_PADDING).contains(50.0, line_rect.y + 2.0));
    }

    #[test]
    fn moved_anchor_centres_dot_and_translates_suffix_without_resizing_it() {
        let base = vec![0.0, 0.2, 0.5, 0.75, 1.0];
        let shifted = shift_character_ratios(&base, &[(2, 0.8)]);
        let delta = 0.8 - 0.625;

        assert_eq!(shifted[0], base[0]);
        assert_eq!(shifted[1], base[1]);
        assert!((shifted[2] - (base[2] + delta)).abs() < 0.0001);
        assert!((shifted[3] - (base[3] + delta)).abs() < 0.0001);
        assert!((shifted[4] - (base[4] + delta)).abs() < 0.0001);
        assert!((character_center_ratio(&shifted, 2).unwrap() - 0.8).abs() < 0.0001);
        assert!(((shifted[3] - shifted[2]) - (base[3] - base[2])).abs() < 0.0001);
        assert!(((shifted[4] - shifted[3]) - (base[4] - base[3])).abs() < 0.0001);
    }

    #[test]
    fn vertical_line_geometry_is_centered_on_requested_x() {
        let x1 = 42.0;
        let x2 = 42.0;
        let center_x = (x1 + x2) * 0.5;
        assert_eq!(center_x, 42.0);
    }
}
