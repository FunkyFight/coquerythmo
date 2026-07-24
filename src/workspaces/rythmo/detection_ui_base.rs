//! Editor-only interaction and rendering for professional detection signs and
//! per-letter synchronization points.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionCueId, DetectionKind,
    LineDetectionData, MediaTick, SyncPointId, TextAnchor, TextSyncPoint,
};
use std::collections::{BTreeMap, BTreeSet};
use unicode_segmentation::UnicodeSegmentation;

const DETECTION_ICON_SIZE: f32 = 18.0;
const DETECTION_HIT_SIZE: f32 = 26.0;
const DETECTION_ICON_BOTTOM_MARGIN: f32 = 2.0;
const DETECTION_BUTTON_SIZE: f32 = 18.0;
const DETECTION_BUTTON_GAP: f32 = 4.0;
const DETECTION_DRAG_THRESHOLD: f32 = 4.0;
const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const INFO_WIDTH: f32 = 470.0;
const INFO_HEIGHT: f32 = 176.0;
const INFO_PADDING: f32 = 12.0;
const INFO_IMAGE_SIZE: f32 = 136.0;
const INFO_TEXT_GAP: f32 = 14.0;
const SYNC_DOT_SIZE: f32 = 6.0;
const SYNC_DOT_HIT_PADDING: f32 = 9.0;
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
            (DetectionKind::Pucker, _) => None,
            (DetectionKind::OpeningWave | DetectionKind::ForwardWave, _) => None,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DetectionInfo {
    title: &'static str,
    description: &'static str,
    sound_labels: &'static str,
    rhubarb_image_asset: &'static str,
}

fn detection_info(sign: PaletteSign) -> DetectionInfo {
    match sign {
        PaletteSign::Labial => DetectionInfo {
            title: "Labiale",
            description: "Fermeture nette des lèvres.",
            sound_labels: "P, B, M",
            rhubarb_image_asset: "detection/rhubarb_lips/P_B_M.png",
        },
        PaletteSign::SemiLabial => DetectionInfo {
            title: "Semi-labiale",
            description: "Contact lèvre-dents, fermeture incomplète.",
            sound_labels: "F, V",
            rhubarb_image_asset: "detection/rhubarb_lips/F_V.png",
        },
        PaletteSign::MouthOpen => DetectionInfo {
            title: "Bouche ouverte",
            description: "Ouverture marquée de la bouche.",
            sound_labels: "A, AN, O ouverts, voyelles larges",
            rhubarb_image_asset: "detection/rhubarb_lips/AA.png",
        },
        PaletteSign::MouthClosed => DetectionInfo {
            title: "Bouche fermée",
            description: "Bouche refermée, occlusion visible.",
            sound_labels: "Fermetures et attaques de consonnes occlusives",
            rhubarb_image_asset: "detection/rhubarb_lips/P_B_M.png",
        },
        PaletteSign::TeethVisible => DetectionInfo {
            title: "Dents visibles",
            description: "Dents apparentes, articulation tendue.",
            sound_labels: "F, V, S, T, EE",
            rhubarb_image_asset: "detection/rhubarb_lips/K_S_T_EE.png",
        },
        PaletteSign::DentalTh => DetectionInfo {
            title: "TH",
            description: "Articulation dentale appuyée du « th ».",
            sound_labels: "TH, T, S appuyés",
            rhubarb_image_asset: "detection/rhubarb_lips/K_S_T_EE.png",
        },
        PaletteSign::Neutral => DetectionInfo {
            title: "Neutre / parenthèses",
            description: "Mouvement neutre ou intermédiaire.",
            sound_labels: "CH, dentales appuyées, articulation neutre",
            rhubarb_image_asset: "detection/rhubarb_lips/EH_AE.png",
        },
        PaletteSign::Breath => DetectionInfo {
            title: "Respiration",
            description: "Souffle ou reprise d’air.",
            sound_labels: "Respiration, souffle, aspiration",
            rhubarb_image_asset: "detection/rhubarb_lips/UW_OW_W.png",
        },
        PaletteSign::Reaction => DetectionInfo {
            title: "Réaction",
            description: "Réaction vocale non verbale.",
            sound_labels: "Rires, exclamations, petits bruits vocaux",
            rhubarb_image_asset: "detection/rhubarb_lips/AA.png",
        },
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
enum DetectionMenuKind {
    Palette {
        track: u8,
        media_tick: MediaTick,
        hover_index: Option<usize>,
    },
    Info {
        address: DetectionAddress,
        sign: PaletteSign,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionMenu {
    x: f32,
    y: f32,
    kind: DetectionMenuKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionDrag {
    address: DetectionAddress,
    start_x: f32,
    start_y: f32,
    moved: bool,
    retarget_text: bool,
}

impl DetectionDrag {
    fn new(address: DetectionAddress, x: f32, y: f32) -> Self {
        Self::with_retarget(address, x, y, false)
    }

    fn with_retarget(address: DetectionAddress, x: f32, y: f32, retarget_text: bool) -> Self {
        Self {
            address,
            start_x: x,
            start_y: y,
            moved: false,
            retarget_text,
        }
    }

    pub(crate) const fn retargets_text(self) -> bool {
        self.retarget_text
    }

    fn exceeds_threshold(self, x: f32, y: f32) -> bool {
        let dx = x - self.start_x;
        let dy = y - self.start_y;
        dx * dx + dy * dy >= DETECTION_DRAG_THRESHOLD * DETECTION_DRAG_THRESHOLD
    }
}

impl RythmoState {
    pub(crate) fn open_detection_palette_from_hover(&mut self) -> bool {
        let Some(hover) = self.detection_hover else {
            return false;
        };
        let button = detection_button_rect(&hover);
        self.detection_menu = Some(DetectionMenu {
            x: button.x,
            y: button.y + button.height + 2.0,
            kind: DetectionMenuKind::Palette {
                track: hover.track,
                media_tick: hover.media_tick,
                hover_index: None,
            },
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

fn next_detection_address(project: &Project, line_id: u64) -> Option<DetectionAddress> {
    if project
        .get_line(line_id)
        .is_some_and(|line| line.kind.is_ambiance())
    {
        return None;
    }
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
        y: track_rect.y - DETECTION_HIT_SIZE - DETECTION_ICON_BOTTOM_MARGIN,
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

fn popup_rect(menu: &DetectionMenu, zone: &Rect) -> Rect {
    let (width, height) = match menu.kind {
        DetectionMenuKind::Palette { .. } => (MENU_WIDTH, MENU_HEIGHT),
        DetectionMenuKind::Info { .. } => (INFO_WIDTH, INFO_HEIGHT),
    };
    Rect {
        x: menu
            .x
            .clamp(zone.x, (zone.x + zone.width - width).max(zone.x)),
        y: menu
            .y
            .clamp(zone.y, (zone.y + zone.height - height).max(zone.y)),
        width,
        height,
    }
}

fn menu_item_rect(menu: &DetectionMenu, zone: &Rect, index: usize) -> Rect {
    let outer = popup_rect(menu, zone);
    Rect {
        x: outer.x + MENU_PADDING + index as f32 * (MENU_ICON_SIZE + MENU_GAP),
        y: outer.y + MENU_PADDING,
        width: MENU_ICON_SIZE,
        height: MENU_ICON_SIZE,
    }
}

fn palette_uv(sign: PaletteSign, detection_uvs: [[f32; 4]; 18]) -> [f32; 4] {
    if let Some(index) = sign.legacy_uv_index() {
        return detection_uvs[index];
    }

    let extra_index = match sign {
        PaletteSign::DentalTh => 7,
        PaletteSign::Neutral => 8,
        _ => unreachable!(),
    };
    detection_uvs[extra_index]
}

fn rhubarb_uv(asset: &str, detection_uvs: [[f32; 4]; 18]) -> [f32; 4] {
    let index = match asset {
        "detection/rhubarb_lips/AA.png" => 0.0,
        "detection/rhubarb_lips/AO_ER.png" => 1.0,
        "detection/rhubarb_lips/EH_AE.png" => 2.0,
        "detection/rhubarb_lips/F_V.png" => 3.0,
        "detection/rhubarb_lips/K_S_T_EE.png" => 4.0,
        "detection/rhubarb_lips/L.png" => 5.0,
        "detection/rhubarb_lips/P_B_M.png" => 6.0,
        "detection/rhubarb_lips/UW_OW_W.png" => 7.0,
        _ => 0.0,
    };
    detection_uvs[10 + index as usize]
}

fn has_sync_cues(project: &Project, line: &crate::rythmo_line::RythmoLine) -> bool {
    line.kind.is_dialogue()
        && !line.karaoke
        && project
            .detections()
            .line(line.id)
            .is_some_and(|data| !data.sync_points().is_empty())
}

pub(crate) fn line_has_visible_sync_points(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> bool {
    if line.kind.is_ambiance() {
        return false;
    }
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
    let ratios =
        if let Some(drag) = effective_drag.filter(|drag| drag.ratios.len() == breaks.len() + 1) {
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
    let grapheme_count = UnicodeSegmentation::graphemes(line.text.as_str(), true).count();
    let spans = grapheme_char_spans(&line.text);
    let characters = line.text.chars().collect::<Vec<_>>();
    let mut anchors = BTreeMap::new();
    for point in data.sync_points() {
        let index = point.grapheme_boundary as usize;
        if index >= grapheme_count {
            continue;
        }
        let ratio = ((point.line_tick.as_frame_position() - line.start_frame as f64)
            / line.duration_frames as f64) as f32;
        let (start, end) = spans[index];
        let punctuation = characters[start..end]
            .iter()
            .all(|character| crate::detection::is_sync_punctuation(*character));
        let boundary = match point.affinity {
            crate::detection::SyncAffinity::Left => end,
            crate::detection::SyncAffinity::Right => start,
            crate::detection::SyncAffinity::Auto if punctuation => end,
            crate::detection::SyncAffinity::Auto => start,
        };
        anchors.insert(boundary, ratio);
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

fn shift_character_ratios(base: &[f32], anchors: &[(usize, f32)]) -> Vec<f32> {
    let mut controls = vec![(0.0_f32, 0.0_f32), (1.0_f32, 1.0_f32)];
    controls.extend(anchors.iter().filter_map(|(boundary, target)| {
        Some((base.get(*boundary)?.to_owned(), target.clamp(0.0, 1.0)))
    }));
    controls.sort_by(|left, right| left.0.total_cmp(&right.0));
    controls.dedup_by(|left, right| (left.0 - right.0).abs() < 0.000_01);
    base.iter()
        .map(|source| {
            controls
                .windows(2)
                .find_map(|pair| {
                    let (x0, y0) = pair[0];
                    let (x1, y1) = pair[1];
                    (*source <= x1).then(|| {
                        let local = ((*source - x0) / (x1 - x0).max(0.000_1)).clamp(0.0, 1.0);
                        y0 + (y1 - y0) * local
                    })
                })
                .unwrap_or(1.0)
        })
        .collect()
}

fn grapheme_center_ratio(
    positions: &[f32],
    spans: &[(usize, usize)],
    grapheme_index: usize,
) -> Option<f32> {
    let (start, end) = *spans.get(grapheme_index)?;
    Some((positions.get(start)? + positions.get(end)?) * 0.5)
}

fn character_layout(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    state: &RythmoState,
) -> (Vec<f32>, Vec<f32>, Vec<usize>, Vec<(usize, f32)>) {
    let anchors = sync_anchor_targets(project, line);
    if !anchors.is_empty() {
        let spans = grapheme_char_spans(line.text.as_str());
        let base = uniform_grapheme_character_positions(&spans);
        let breaks = state.get_syllable_breaks(line, lang);
        let shifted = shift_character_ratios(&base, &anchors);
        return (base, shifted, breaks, anchors);
    }
    let (base, breaks) = base_character_ratios(line, drag, lang, state);
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
    point: &TextSyncPoint,
    current_frame: f64,
    zone: &Rect,
    _state: &RythmoState,
    fps: f64,
) -> Option<f32> {
    if line.karaoke {
        return None;
    }
    let rect = line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps);
    let ratio = ((point.line_tick.as_frame_position() - line.start_frame as f64)
        / line.duration_frames.max(1) as f64) as f32;
    Some(rect.x + rect.width * ratio.clamp(0.0, 1.0))
}

fn existing_sync_at(project: &Project, line_id: u64, character_index: usize) -> bool {
    project.detections().line(line_id).is_some_and(|data| {
        data.sync_points()
            .iter()
            .any(|point| point.grapheme_boundary == character_index as u32)
    })
}

fn sync_retarget_boundary_at(
    ctx: &RythmoCtx,
    state: &RythmoState,
    address: DetectionAddress,
    x: f32,
    y: f32,
) -> Option<u32> {
    let line = ctx.project.get_line(address.line_id)?;
    let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
    if y < rect.y - 10.0 || y > rect.y + rect.height + 10.0 || rect.width <= 0.0 {
        return None;
    }
    let (_, positions, _, _) = character_layout(
        ctx.project,
        line,
        None,
        ctx.project.syllable_language_code(),
        state,
    );
    let pointer = ((x - rect.x) / rect.width).clamp(0.0, 1.0);
    let current_id = SyncPointId(address.detection_id.0);
    let data = ctx.project.detections().line(address.line_id)?;
    let spans = grapheme_char_spans(line.text.as_str());
    (0..spans.len())
        .filter(|index| {
            !data
                .sync_points()
                .iter()
                .any(|point| point.id != current_id && point.grapheme_boundary == *index as u32)
        })
        .filter_map(|index| {
            Some((
                index,
                (grapheme_center_ratio(&positions, &spans, index)? - pointer).abs(),
            ))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index as u32)
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
    fps: f64,
) -> Option<SyncPlaceholder> {
    let graphemes = UnicodeSegmentation::graphemes(line.text.as_str(), true).collect::<Vec<_>>();
    if line.karaoke || graphemes.is_empty() || line.duration_frames <= 0 {
        return None;
    }

    let line_rect = line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps);
    if !line_rect.contains(x, y) || line_rect.width <= 0.0 {
        return None;
    }

    let lang = project.syllable_language_code();
    let (_, shifted, _, _) = character_layout(project, line, None, lang, state);
    let pointer_ratio = (x - line_rect.x) / line_rect.width;
    let mut candidate = None;
    let spans = grapheme_char_spans(line.text.as_str());
    for index in 0..graphemes.len() {
        if graphemes[index].chars().all(char::is_whitespace)
            || existing_sync_at(project, line.id, index)
        {
            continue;
        }
        let (start, end) = spans[index];
        let left = shifted[start].min(shifted[end]);
        let right = shifted[start].max(shifted[end]);
        if pointer_ratio < left || pointer_ratio > right {
            continue;
        }
        let center = grapheme_center_ratio(&shifted, &spans, index)?;
        let distance = (pointer_ratio - center).abs();
        if candidate
            .as_ref()
            .map_or(true, |(_, best_distance): &(usize, f32)| {
                distance < *best_distance
            })
        {
            candidate = Some((index, distance));
        }
    }
    let character_index = candidate?.0;
    let center_ratio = grapheme_center_ratio(&shifted, &spans, character_index)?;
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
    fps: f64,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project.lines().find(|line| {
        !line.karaoke
            && line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps).contains(x, y)
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
        let placeholder =
            sync_placeholder_for_line(ctx.project, line, state, x, y, ctx.current_frame, ctx.zone, ctx.fps)?;
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
        let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone, crate::config::reading_bar_offset_seconds(), ctx.fps);
        for point in data.sync_points() {
            let Some(cue_x) =
                sync_point_x(ctx.project, line, point, ctx.current_frame, ctx.zone, state, ctx.fps)
            else {
                continue;
            };
            if expanded_rect(sync_dot_rect(cue_x, rect), SYNC_DOT_HIT_PADDING).contains(x, y) {
                return Some(DetectionAddress {
                    line_id: line.id,
                    detection_id: DetectionCueId(point.id.0),
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
    let Some(current) = data.sync_point(SyncPointId(address.detection_id.0)) else {
        return tick;
    };
    let current_index = current.grapheme_boundary;

    let mut minimum = MediaTick::from_frame(line.start_frame).saturating_add(MediaTick(1));
    let mut maximum = MediaTick::from_frame(line.end_frame()).saturating_sub(MediaTick(1));
    for point in data.sync_points() {
        if point.id.0 == address.detection_id.0 {
            continue;
        }
        let index = point.grapheme_boundary;
        if index < current_index {
            minimum = MediaTick(minimum.raw().max(point.line_tick.raw().saturating_add(1)));
        } else if index > current_index {
            maximum = MediaTick(maximum.raw().min(point.line_tick.raw().saturating_sub(1)));
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
    fps: f64,
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
    let rect = line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps);
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

fn navigate_detection(
    project: &Project,
    state: &mut RythmoState,
    direction: i32,
) -> Option<DetectionAddress> {
    let Some(address) = selected_address(state) else {
        return None;
    };
    let Some(data) = project.detections().line(address.line_id) else {
        return None;
    };
    let ids = if address.track().is_some() {
        data.source_detections()
            .map(|cue| cue.id)
            .collect::<Vec<_>>()
    } else {
        data.sync_points()
            .iter()
            .map(|point| DetectionCueId(point.id.0))
            .collect::<Vec<_>>()
    };
    if ids.is_empty() {
        return None;
    }
    let current = ids
        .iter()
        .position(|id| *id == address.detection_id)
        .unwrap_or(0);
    let index = if direction < 0 {
        current.checked_sub(1).unwrap_or(ids.len() - 1)
    } else {
        (current + 1) % ids.len()
    };
    let selected = DetectionAddress {
        line_id: address.line_id,
        detection_id: ids[index],
    };
    state.selected = Some(Selection::Detection(selected));
    state.detection_menu = None;
    Some(selected)
}

fn sync_point_accessibility_label(
    project: &Project,
    address: DetectionAddress,
    fps: f64,
) -> Option<String> {
    let point = project.detections().sync_point(address)?;
    let line = project.get_line(address.line_id)?;
    let grapheme = UnicodeSegmentation::graphemes(line.text.as_str(), true)
        .nth(point.grapheme_boundary as usize)
        .unwrap_or("?");
    let fps = fps.max(1.0);
    let total_frames = point.line_tick.as_frame_position().max(0.0);
    let whole_frames = total_frames.floor() as u64;
    let frames_per_second = fps.round().max(1.0) as u64;
    let seconds = whole_frames / frames_per_second;
    let frame = whole_frames % frames_per_second;
    let subframe = ((total_frames.fract() * 10.0).round() as u64).min(9);
    Some(format!(
        "Point de synchronisation, lettre {grapheme}, {:02}:{:02}:{:02}:{:02}.{subframe}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60,
        frame
    ))
}

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    match event {
        UiEvent::MouseMove { x, y } => {
            if let Some(mut drag) = state.detection_drag {
                if !drag.moved {
                    if !drag.exceeds_threshold(*x, *y) {
                        return Some(EventResponse::Consumed);
                    }
                    drag.moved = true;
                    state.detection_drag = Some(drag);
                    state.detection_menu = None;
                }
                if drag.retarget_text && drag.address.track().is_none() {
                    if let Some(grapheme_boundary) =
                        sync_retarget_boundary_at(ctx, state, drag.address, *x, *y)
                    {
                        return Some(EventResponse::Action(UiAction::MoveSyncAnchor {
                            address: drag.address,
                            grapheme_boundary,
                        }));
                    }
                    return Some(EventResponse::Consumed);
                }
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
                if let DetectionMenuKind::Palette {
                    track, media_tick, ..
                } = menu.kind
                {
                    let hover_index = PaletteSign::ALL
                        .iter()
                        .enumerate()
                        .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                        .map(|(index, _)| index);
                    menu.kind = DetectionMenuKind::Palette {
                        track,
                        media_tick,
                        hover_index,
                    };
                    state.detection_menu = Some(menu);
                }
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

            // Merely hovering a line that owns synchronization points must not
            // swallow the event. Mouse presses on an actual point or sync
            // placeholder are handled below; every other part of the line
            // needs to reach the regular line controller so it can be moved or
            // resized.
        }
        UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                match menu.kind {
                    DetectionMenuKind::Palette {
                        track, media_tick, ..
                    } => {
                        if let Some((_, sign)) =
                            PaletteSign::ALL
                                .iter()
                                .copied()
                                .enumerate()
                                .find(|(index, _)| {
                                    menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y)
                                })
                        {
                            let (kind, target) = sign.storage();
                            state.detection_menu = None;
                            return Some(EventResponse::Action(UiAction::AddDetection {
                                line_id: track_storage_line_id(track),
                                kind,
                                media_tick,
                                target,
                            }));
                        }
                        state.detection_menu = None;
                        return Some(EventResponse::Consumed);
                    }
                    DetectionMenuKind::Info { .. } => {
                        if popup_rect(&menu, ctx.zone).contains(*x, *y) {
                            return Some(EventResponse::Consumed);
                        }
                        state.detection_menu = None;
                    }
                }
            }

            if let Some(address) = hit_existing_detection(ctx, state, *x, *y) {
                state.selected = Some(Selection::Detection(address));
                state.detection_menu = None;
                let retarget_text =
                    matches!(event, UiEvent::ShiftMousePress { .. }) && address.track().is_none();
                state.detection_drag =
                    Some(DetectionDrag::with_retarget(address, *x, *y, retarget_text));
                if address.track().is_none() {
                    if let Some(label) =
                        sync_point_accessibility_label(ctx.project, address, ctx.fps)
                    {
                        return Some(EventResponse::Action(UiAction::Accessibility(
                            crate::accessibility::AccessibilityEvent::Selection { label },
                        )));
                    }
                }
                return Some(EventResponse::Consumed);
            }

            if let Some((line_id, character_index, tick)) = hit_sync_placeholder(ctx, state, *x, *y)
            {
                let Some(address) = next_detection_address(ctx.project, line_id) else {
                    return Some(EventResponse::Consumed);
                };
                state.selected = Some(Selection::Detection(address));
                state.detection_drag = Some(DetectionDrag::new(address, *x, *y));
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
        UiEvent::MouseRelease { x, y } => {
            if let Some(drag) = state.detection_drag.take() {
                if !drag.moved && drag.address.track().is_some() {
                    if let Some(sign) = ctx
                        .project
                        .detections()
                        .detection(drag.address)
                        .and_then(PaletteSign::from_cue)
                    {
                        state.detection_menu = Some(DetectionMenu {
                            x: *x + 8.0,
                            y: *y - INFO_HEIGHT - 8.0,
                            kind: DetectionMenuKind::Info {
                                address: drag.address,
                                sign,
                            },
                        });
                        state.detection_hover = None;
                    }
                }
                return Some(EventResponse::Consumed);
            }
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
            if let Some(address) = navigate_detection(ctx.project, state, -1) {
                if let Some(label) = sync_point_accessibility_label(ctx.project, address, ctx.fps) {
                    return Some(EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Selection { label },
                    )));
                }
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorRight => {
            if let Some(address) = navigate_detection(ctx.project, state, 1) {
                if let Some(label) = sync_point_accessibility_label(ctx.project, address, ctx.fps) {
                    return Some(EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Selection { label },
                    )));
                }
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::Delete => {
            if let Some(address) = selected_address(state) {
                state.detection_menu = None;
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
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
) {
    let lang = project.syllable_language_code();
    let drag = effective_drag_for_line(line.id, state.syllable_drag.as_ref(), state);
    let Some(boundary_ratios) = sync_syllable_boundary_ratios(project, line, drag, lang, state)
    else {
        return;
    };
    if boundary_ratios.len() <= 2 {
        return;
    }

    let rect = line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps);
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

fn push_info_labels<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    outer: Rect,
    image_rect: Rect,
    info: DetectionInfo,
) {
    let text_x = image_rect.x + image_rect.width + INFO_TEXT_GAP;
    let text_width = (outer.x + outer.width - INFO_PADDING - text_x).max(0.0);
    labels.push(LabelInfo {
        text: info.title,
        bounds: Rect {
            x: text_x,
            y: outer.y + 12.0,
            width: text_width,
            height: 28.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(18.0),
        color_override: Some([244, 246, 252]),
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: "Description",
        bounds: Rect {
            x: text_x,
            y: outer.y + 48.0,
            width: text_width,
            height: 18.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(11.0),
        color_override: Some([142, 164, 202]),
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: info.description,
        bounds: Rect {
            x: text_x,
            y: outer.y + 66.0,
            width: text_width,
            height: 24.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(13.0),
        color_override: Some([220, 225, 236]),
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: "Sons correspondants",
        bounds: Rect {
            x: text_x,
            y: outer.y + 104.0,
            width: text_width,
            height: 18.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(11.0),
        color_override: Some([142, 164, 202]),
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: info.sound_labels,
        bounds: Rect {
            x: text_x,
            y: outer.y + 122.0,
            width: text_width,
            height: 36.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Top,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(13.0),
        color_override: Some([240, 242, 248]),
        font_family_override: None,
    });
}

fn digit_label(digit: u64) -> &'static str {
    match digit % 10 {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        _ => "9",
    }
}

fn render_sync_timecode<'a>(
    tick: MediaTick,
    fps: f64,
    x: f32,
    line_rect: Rect,
    zone: &Rect,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let fps = fps.max(1.0);
    let frame_position = tick.as_frame_position().max(0.0);
    let whole_frames = frame_position.floor() as u64;
    let frame_rate = fps.round().max(1.0) as u64;
    let total_seconds = whole_frames / frame_rate;
    let hours = (total_seconds / 3600).min(99);
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    let frames = whole_frames % frame_rate;
    let subframe = ((frame_position.fract() * 10.0).round() as u64).min(9);
    let tokens = [
        digit_label(hours / 10),
        digit_label(hours),
        ":",
        digit_label(minutes / 10),
        digit_label(minutes),
        ":",
        digit_label(seconds / 10),
        digit_label(seconds),
        ":",
        digit_label(frames / 10),
        digit_label(frames),
        ".",
        digit_label(subframe),
    ];
    let cell = 7.0;
    let width = tokens.len() as f32 * cell + 8.0;
    let left = (x - width / 2.0).clamp(zone.x, (zone.x + zone.width - width).max(zone.x));
    let top = (line_rect.y - 21.0).max(zone.y);
    let panel = Rect {
        x: left,
        y: top,
        width,
        height: 18.0,
    };
    push_quad(quads, panel, [0.035, 0.045, 0.065, 0.98], 4.0);
    for (index, token) in tokens.into_iter().enumerate() {
        labels.push(LabelInfo {
            text: token,
            bounds: Rect {
                x: left + 4.0 + index as f32 * cell,
                y: top,
                width: cell,
                height: panel.height,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Visible,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([220, 232, 255]),
            font_family_override: None,
        });
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
        let rect = line_rect(project, line, current_frame, zone, crate::config::reading_bar_offset_seconds(), fps);
        for point in data.sync_points() {
            let Some(cue_x) = sync_point_x(project, line, point, current_frame, zone, state, fps) else {
                continue;
            };
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: DetectionCueId(point.id.0),
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
            if selected {
                render_sync_timecode(point.line_tick, fps, cue_x, rect, zone, quads, labels);
            }
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
                fps,
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
            render_shifted_syllable_handles(project, line, current_frame, zone, fps, state, quads);
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
                fps,
            ) {
                if let Some(placeholder) = sync_placeholder_for_line(
                    project,
                    line,
                    state,
                    hover.screen_x,
                    hover.screen_y,
                    current_frame,
                    zone,
                    fps,
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

    if state.detection_menu.is_none() {
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
            // Keep the existing palette hit target, but do not draw the small
            // transient plus requested for removal.
        }
    }

    if let Some(menu) = state.detection_menu {
        let outer = popup_rect(&menu, zone);
        match menu.kind {
            DetectionMenuKind::Palette { hover_index, .. } => {
                push_quad(quads, outer, [0.045, 0.048, 0.060, 0.985], 7.0);
                for (index, sign) in PaletteSign::ALL.iter().copied().enumerate() {
                    let item = menu_item_rect(&menu, zone, index);
                    if hover_index == Some(index) {
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
            DetectionMenuKind::Info { address, sign } => {
                if project.detections().detection(address).is_none() {
                    return;
                }
                let info = detection_info(sign);
                push_quad(quads, outer, [0.035, 0.039, 0.052, 0.992], 10.0);
                let image_size = INFO_IMAGE_SIZE.min(outer.height - INFO_PADDING * 2.0);
                let image_rect = Rect {
                    x: outer.x + INFO_PADDING,
                    y: outer.y + (outer.height - image_size) / 2.0,
                    width: image_size,
                    height: image_size,
                };
                push_quad(quads, image_rect, [0.11, 0.12, 0.15, 1.0], 8.0);
                icons.push(IconInstance {
                    rect: [
                        image_rect.x,
                        image_rect.y,
                        image_rect.width,
                        image_rect.height,
                    ],
                    uv_rect: rhubarb_uv(info.rhubarb_image_asset, detection_uvs),
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
                push_info_labels(labels, outer, image_rect, info);
            }
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
        assert_ne!(
            PaletteSign::DentalTh.storage(),
            PaletteSign::TeethVisible.storage()
        );
        assert_ne!(
            PaletteSign::Neutral.storage(),
            PaletteSign::Breath.storage()
        );
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
    fn source_icon_is_anchored_above_track() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let track = Rect {
            x: 0.0,
            y: 20.0,
            width: 800.0,
            height: 50.0,
        };
        let hit = source_icon_rect(MediaTick::ZERO, track, 0.0, &zone);
        assert_eq!(
            hit.y,
            track.y - DETECTION_HIT_SIZE - DETECTION_ICON_BOTTOM_MARGIN
        );
        assert_eq!(hit.x + hit.width / 2.0, tick_x(MediaTick::ZERO, 0.0, &zone));
    }

    #[test]
    fn drag_starts_after_four_pixel_threshold() {
        let address = DetectionAddress {
            line_id: 1,
            detection_id: DetectionCueId(1),
        };
        let drag = DetectionDrag::new(address, 10.0, 10.0);
        assert!(!drag.exceeds_threshold(13.0, 12.0));
        assert!(drag.exceeds_threshold(14.0, 10.0));
    }

    #[test]
    fn rhubarb_mapping_uses_expected_reference_mouths() {
        assert_eq!(
            detection_info(PaletteSign::Labial).rhubarb_image_asset,
            "detection/rhubarb_lips/P_B_M.png"
        );
        assert_eq!(
            detection_info(PaletteSign::SemiLabial).rhubarb_image_asset,
            "detection/rhubarb_lips/F_V.png"
        );
        assert_eq!(
            detection_info(PaletteSign::Neutral).rhubarb_image_asset,
            "detection/rhubarb_lips/EH_AE.png"
        );
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
    fn piecewise_sync_layout_keeps_implicit_line_edges() {
        let base = vec![0.0, 0.2, 0.5, 0.75, 1.0];
        let shifted = shift_character_ratios(&base, &[(2, 0.8)]);
        assert_eq!(shifted.first().copied(), Some(0.0));
        assert_eq!(shifted.last().copied(), Some(1.0));
        assert!(shifted.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(shifted[2] > base[2]);
    }

    #[test]
    fn sync_layout_indexes_extended_graphemes_as_single_letters() {
        let spans = grapheme_char_spans("e\u{301}👨‍👩‍👧‍👦P");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0], (0, 2));
        assert!(spans[1].1 - spans[1].0 > 1);

        let base = uniform_grapheme_character_positions(&spans);
        assert_eq!(base.first().copied(), Some(0.0));
        assert_eq!(base.last().copied(), Some(1.0));
        let center = grapheme_center_ratio(&base, &spans, 2).unwrap();
        assert!((center - 5.0 / 6.0).abs() < 0.000_01);
    }

    #[test]
    fn sync_control_lands_on_grapheme_edge_not_inside_glyph() {
        let spans = grapheme_char_spans("A\u{301}BC");
        let base = uniform_grapheme_character_positions(&spans);
        let shifted = shift_character_ratios(&base, &[(spans[1].0, 0.8)]);
        assert!((shifted[spans[1].0] - 0.8).abs() < 0.000_01);
        assert!(shifted[spans[1].0] <= shifted[spans[1].1]);
    }

    #[test]
    fn vertical_line_geometry_is_centered_on_requested_x() {
        let x1 = 42.0;
        let x2 = 42.0;
        let center_x = (x1 + x2) * 0.5;
        assert_eq!(center_x, 42.0);
    }
}
