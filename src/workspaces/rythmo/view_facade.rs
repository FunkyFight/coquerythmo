//! Focused view facade for the rythmo workspace.
//!
//! The established renderer remains authoritative. This boundary removes
//! syllable-authoring handles from ordinary adaptation lines and replaces the
//! legacy stretched synchronization intervals with letter-anchored natural text.

#[path = "view.rs"]
mod legacy;

pub use legacy::*;

use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCueId, DetectionKind, LineDetectionData,
    MediaTick, TextAnchor,
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

#[derive(Clone, Debug, PartialEq)]
struct SyncTextSegment {
    cache_id: u64,
    start_char: usize,
    end_char: usize,
    start_ratio: f32,
    width_ratio: f32,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LetterAnchor {
    line_id: u64,
    character_index: usize,
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

fn sync_segment_cache_id(line_id: u64, start: usize, end: usize) -> u64 {
    (1_u64 << 61)
        ^ line_id.wrapping_mul(1_000_003)
        ^ (start as u64).wrapping_mul(65_537)
        ^ end as u64
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

fn build_sync_segments_with_measure<F>(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    line_width: f32,
    mut measure_width: F,
) -> Vec<SyncTextSegment>
where
    F: FnMut(&str) -> f32,
{
    let boundaries = sync_boundaries(project, line);
    if boundaries.len() <= 2 {
        return Vec::new();
    }

    let characters = line.text.chars().collect::<Vec<_>>();
    let line_start = MediaTick::from_frame(line.start_frame);
    let line_duration = MediaTick::from_frame(line.duration_frames).raw().max(1) as f32;
    let line_width = line_width.max(1.0);
    let mut segments = Vec::new();

    for pair in boundaries.windows(2) {
        let (start_char, start_tick) = pair[0];
        let (end_char, end_tick) = pair[1];
        if end_char <= start_char || end_char > characters.len() || end_tick <= start_tick {
            continue;
        }
        let text = characters[start_char..end_char].iter().collect::<String>();
        if text.is_empty() {
            continue;
        }
        let natural_width = measure_width(&text).max(1.0);
        segments.push(SyncTextSegment {
            cache_id: sync_segment_cache_id(line.id, start_char, end_char),
            start_char,
            end_char,
            start_ratio: ((start_tick.raw() - line_start.raw()) as f32 / line_duration)
                .clamp(0.0, 1.0),
            width_ratio: natural_width / line_width,
            text,
        });
    }
    segments
}

fn rythmo_font_size() -> f32 {
    crate::config::get().ui.font_size * 2.0
}

fn natural_text_width(text: &str) -> f32 {
    crate::vector_text::measure_rythmo_text_width_standalone(text, rythmo_font_size())
        .unwrap_or_else(|| text.chars().count().max(1) as f32 * rythmo_font_size() * 0.5)
        .max(1.0)
}

fn character_ratios(text: &str) -> Vec<f32> {
    crate::vector_text::measure_rythmo_text_char_ratios_standalone(text, rythmo_font_size())
        .filter(|ratios| ratios.len() == text.chars().count() + 1)
        .unwrap_or_else(|| {
            let count = text.chars().count().max(1);
            (0..=count)
                .map(|index| index as f32 / count as f32)
                .collect()
        })
}

fn display_segments_for_hit_test(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    line_width: f32,
) -> Vec<SyncTextSegment> {
    let synced = build_sync_segments_with_measure(project, line, line_width, natural_text_width);
    if !synced.is_empty() {
        return synced;
    }

    vec![SyncTextSegment {
        cache_id: line.id,
        start_char: 0,
        end_char: line.text.chars().count(),
        start_ratio: 0.0,
        width_ratio: 1.0,
        text: line.text.clone(),
    }]
}

fn character_index_at_x_with_ratios<F>(
    characters: &[char],
    segments: &[SyncTextSegment],
    line_rect: Rect,
    x: f32,
    mut ratios_for: F,
) -> Option<(usize, f32)>
where
    F: FnMut(&str) -> Vec<f32>,
{
    for segment in segments {
        let local_count = segment.end_char.saturating_sub(segment.start_char);
        if local_count == 0 {
            continue;
        }
        let ratios = ratios_for(&segment.text);
        if ratios.len() != local_count + 1 {
            continue;
        }
        for local_index in 0..local_count {
            let character_index = segment.start_char + local_index;
            let Some(character) = characters.get(character_index) else {
                continue;
            };
            if character.is_whitespace() {
                continue;
            }
            let start_ratio = segment.start_ratio + ratios[local_index] * segment.width_ratio;
            let end_ratio = segment.start_ratio + ratios[local_index + 1] * segment.width_ratio;
            let start_x = line_rect.x + line_rect.width * start_ratio;
            let end_x = line_rect.x + line_rect.width * end_ratio;
            if x >= start_x.min(end_x) && x <= start_x.max(end_x) {
                return Some((character_index, start_ratio));
            }
        }
    }
    None
}

fn closest_cursor_index_at_x_with_ratios<F>(
    segments: &[SyncTextSegment],
    line_rect: Rect,
    x: f32,
    mut ratios_for: F,
) -> Option<usize>
where
    F: FnMut(&str) -> Vec<f32>,
{
    let mut closest = None;
    let mut closest_distance = f32::MAX;
    for segment in segments {
        let local_count = segment.end_char.saturating_sub(segment.start_char);
        let ratios = ratios_for(&segment.text);
        if ratios.len() != local_count + 1 {
            continue;
        }
        for (local_index, ratio) in ratios.iter().copied().enumerate() {
            let global_ratio = segment.start_ratio + ratio * segment.width_ratio;
            let boundary_x = line_rect.x + line_rect.width * global_ratio;
            let distance = (boundary_x - x).abs();
            if distance < closest_distance
                || (distance == closest_distance
                    && local_index == 0
                    && segment.start_char > closest.unwrap_or(0))
            {
                closest_distance = distance;
                closest = Some(segment.start_char + local_index);
            }
        }
    }
    closest
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

fn letter_anchor_at(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    x: f32,
    y: f32,
) -> Option<LetterAnchor> {
    let line = normal_line_at(project, current_frame, zone, x, y)?;
    if line.text.is_empty() || line.duration_frames <= 0 {
        return None;
    }
    let line_rect = legacy::line_rect(project, line, current_frame, zone);
    let characters = line.text.chars().collect::<Vec<_>>();
    let segments = display_segments_for_hit_test(project, line, line_rect.width);
    let (character_index, anchor_ratio) =
        character_index_at_x_with_ratios(&characters, &segments, line_rect, x, character_ratios)?;
    if character_index == 0 {
        return None;
    }
    let duplicate = project.detections().line(line.id).is_some_and(|data| {
        data.text_sync_cues()
            .any(|cue| cue.target.grapheme_index() == Some(character_index as u32))
    });
    if duplicate {
        return None;
    }

    let line_start = MediaTick::from_frame(line.start_frame);
    let line_end = MediaTick::from_frame(line.end_frame());
    let duration = MediaTick::from_frame(line.duration_frames).raw().max(1) as f64;
    let media_tick = MediaTick(
        line_start
            .raw()
            .saturating_add((duration * anchor_ratio as f64).round() as i64),
    )
    .clamp(
        MediaTick(line_start.raw().saturating_add(1)),
        MediaTick(line_end.raw().saturating_sub(1)),
    );

    Some(LetterAnchor {
        line_id: line.id,
        character_index,
        media_tick,
        x: tick_x(media_tick, current_frame, zone),
        line_rect,
    })
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
            let badge = Rect {
                x: tick_x(cue.media_tick, current_frame, zone) - SOURCE_SIGN_SIZE / 2.0,
                y: (track_rect.y + track_rect.height - SOURCE_SIGN_SIZE - SOURCE_SIGN_BOTTOM_MARGIN)
                    .max(track_rect.y)
                    + SOURCE_SIGN_DISPLAY_DROP,
                width: SOURCE_SIGN_SIZE,
                height: SOURCE_SIGN_SIZE,
            };
            expanded_rect(badge, 3.0).contains(x, y)
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

fn append_natural_segment_text(
    stretched: &mut Vec<StretchedText>,
    segment: &SyncTextSegment,
    line_rect: Rect,
    read_highlight_end: Option<usize>,
    tint: [f32; 4],
) {
    let destination = Rect {
        x: line_rect.x + line_rect.width * segment.start_ratio,
        y: line_rect.y,
        width: (line_rect.width * segment.width_ratio).max(1.0),
        height: line_rect.height,
    };
    let mut base = StretchedText::new(segment.cache_id, segment.text.clone(), destination);
    base.tint = tint;

    let Some(highlight_end) = read_highlight_end else {
        stretched.push(base);
        return;
    };
    if highlight_end <= segment.start_char {
        stretched.push(base);
        return;
    }
    if highlight_end >= segment.end_char {
        base.tint = [1.0, 0.82, 0.08, 1.0];
        stretched.push(base);
        return;
    }

    stretched.push(base);
    let local_end = highlight_end.saturating_sub(segment.start_char);
    let ratios = character_ratios(&segment.text);
    let clip_ratio = ratios
        .get(local_end)
        .copied()
        .unwrap_or(local_end as f32 / segment.text.chars().count().max(1) as f32)
        .clamp(0.0, 1.0);
    let mut overlay = StretchedText::new(segment.cache_id, segment.text.clone(), destination);
    overlay.draw_rect.width *= clip_ratio;
    overlay.uv_rect[2] = clip_ratio;
    overlay.tint = [1.0, 0.82, 0.08, 1.0];
    stretched.push(overlay);
}

fn replace_synced_text_layout(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    first_stretched: usize,
    editing_line: Option<u64>,
    stretched: &mut Vec<StretchedText>,
) -> Option<Vec<CursorSegmentInfo>> {
    let mut legacy_cache_ids = HashSet::new();
    for line in project.lines().filter(|line| line_has_sync_points(project, line.id)) {
        for pair in sync_boundaries(project, line).windows(2) {
            legacy_cache_ids.insert(sync_segment_cache_id(line.id, pair[0].0, pair[1].0));
        }
    }

    let mut index = first_stretched.min(stretched.len());
    while index < stretched.len() {
        if legacy_cache_ids.contains(&stretched[index].line_id) {
            stretched.remove(index);
        } else {
            index += 1;
        }
    }

    let language = project.syllable_language_code();
    let mut editing_segments = None;
    for line in project
        .lines()
        .filter(|line| !line.karaoke && line_has_sync_points(project, line.id))
    {
        let line_rect = legacy::line_rect(project, line, current_frame, zone);
        if line_rect.x + line_rect.width < zone.x
            || line_rect.x > zone.x + zone.width
            || line_rect.y + line_rect.height < zone.y
            || line_rect.y > zone.y + zone.height
        {
            continue;
        }
        let segments =
            build_sync_segments_with_measure(project, line, line_rect.width, natural_text_width);
        let read_highlight_end = if project.settings().highlight_read_word {
            let progress =
                (current_frame - line.start_frame as f64) / line.duration_frames.max(1) as f64;
            crate::syllable::read_highlight_end_from_timing(
                &line.text,
                &line.syllable_ratios,
                language,
                progress as f32,
            )
        } else {
            None
        };
        let tint = if project.settings().scrolling_text_uses_character_color {
            [
                line.character_color[0].clamp(0.0, 1.0),
                line.character_color[1].clamp(0.0, 1.0),
                line.character_color[2].clamp(0.0, 1.0),
                1.0,
            ]
        } else {
            [1.0; 4]
        };

        for segment in &segments {
            append_natural_segment_text(
                stretched,
                segment,
                line_rect,
                read_highlight_end,
                tint,
            );
        }
        if editing_line == Some(line.id) {
            editing_segments = Some(
                segments
                    .iter()
                    .map(|segment| CursorSegmentInfo {
                        cache_id: segment.cache_id,
                        start_char: segment.start_char,
                        end_char: segment.end_char,
                        start_ratio: segment.start_ratio,
                        width_ratio: segment.width_ratio,
                    })
                    .collect(),
            );
        }
    }
    editing_segments
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
    let first_stretched = stretched.len();
    let mut result = legacy::render_lines(
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
    let editing_line = result.as_ref().map(|cursor| cursor.0);
    let natural_cursor_segments = replace_synced_text_layout(
        project,
        current_frame,
        zone,
        first_stretched,
        editing_line,
        stretched,
    );
    if let Some((_, _, _, _, _, _, _, cursor_segments)) = result.as_mut() {
        if natural_cursor_segments.is_some() {
            *cursor_segments = natural_cursor_segments;
        }
    }
    result
}

fn direct_line_pointer_response(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
    active_mode: ToolMode,
) -> Option<EventResponse> {
    let (x, y) = match event {
        UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } => (*x, *y),
        _ => return None,
    };
    let line = normal_line_at(project, current_frame, zone, x, y)?;
    if state.editing_character.is_some()
        || state.audio_offset_mode
        || state.panning
        || state.syllable_drag.is_some()
    {
        return None;
    }
    if hit_existing_sync(project, current_frame, zone, x, y)
        || hit_source_detection(project, current_frame, zone, x, y)
    {
        return None;
    }

    let ctx = legacy::RythmoCtx {
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        active_mode,
    };

    if matches!(event, UiEvent::ShiftMousePress { .. }) {
        return Some(legacy::handle_shift_mouse_press(&ctx, state, x, y));
    }

    let line_rect = legacy::line_rect(project, line, current_frame, zone);
    let on_resize_handle = x < line_rect.x + crate::constants::HANDLE_WIDTH
        || x > line_rect.x + line_rect.width - crate::constants::HANDLE_WIDTH;
    if state.editing_line == Some(line.id) && !on_resize_handle {
        let response = legacy::handle_mouse_press(&ctx, state, x, y);
        let segments = display_segments_for_hit_test(project, line, line_rect.width);
        if let Some(cursor_index) =
            closest_cursor_index_at_x_with_ratios(&segments, line_rect, x, character_ratios)
        {
            state.pending_cursor_click = None;
            state.line_input.start_selection(cursor_index);
        }
        return Some(response);
    }
    if on_resize_handle {
        return Some(legacy::handle_mouse_press(&ctx, state, x, y));
    }

    if let Some(anchor) = letter_anchor_at(project, current_frame, zone, x, y) {
        if let Some(address) = next_detection_address(project, anchor.line_id) {
            state.selected = Some(Selection::Detection(address));
        }
        state.detection_menu = None;
        state.detection_drag = None;
        return Some(EventResponse::Action(UiAction::AddDetection {
            line_id: anchor.line_id,
            kind: DetectionKind::TextSyncPoint,
            media_tick: anchor.media_tick,
            target: TextAnchor::Grapheme {
                index: anchor.character_index as u32,
            },
        }));
    }

    Some(legacy::handle_mouse_press(&ctx, state, x, y))
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
        if let Some(response) = direct_line_pointer_response(
            event,
            zone,
            project,
            render_index,
            current_frame,
            karaoke_preview,
            fps,
            state,
            active_mode,
        ) {
            crate::detection_foreground::sync_from_state(
                project,
                state,
                *zone,
                current_frame,
                event,
            );
            return response;
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
    use crate::detection::DetectionCue;

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

    fn project_with_sync_points(first_frame: i64) -> (Project, u64) {
        let mut project = Project::new();
        let line_id = project.add_line(100, 100, 0.25);
        project.get_line_mut(line_id).unwrap().text = "abcdefghij".to_string();
        for (id, frame, index) in [(1, first_frame, 3), (2, 170, 7)] {
            let cue = DetectionCue {
                id: DetectionCueId(id),
                kind: DetectionKind::TextSyncPoint,
                media_tick: MediaTick::from_frame(frame),
                target: TextAnchor::Grapheme { index },
            };
            assert!(project
                .detections_mut()
                .insert_detection(
                    DetectionAddress {
                        line_id,
                        detection_id: cue.id,
                    },
                    cue,
                ));
        }
        (project, line_id)
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
    fn sync_points_anchor_letter_inclusive_groups_without_squashing_them() {
        let (project, line_id) = project_with_sync_points(130);
        let line = project.get_line(line_id).unwrap();
        let segments = build_sync_segments_with_measure(&project, line, 200.0, |text| {
            text.chars().count() as f32 * 10.0
        });

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "abc");
        assert_eq!(segments[1].text, "defg");
        assert_eq!(segments[2].text, "hij");
        assert_eq!(segments[1].start_char, 3, "the anchored letter stays in its group");
        assert!((segments[0].width_ratio - 0.15).abs() < 0.0001);
        assert!((segments[1].width_ratio - 0.20).abs() < 0.0001);
        assert!((segments[2].width_ratio - 0.15).abs() < 0.0001);
        assert!((segments[1].start_ratio - 0.30).abs() < 0.0001);
        assert!(segments[0].start_ratio + segments[0].width_ratio < segments[1].start_ratio);
    }

    #[test]
    fn moving_a_point_moves_the_anchored_group_but_keeps_glyph_widths() {
        let (before, line_id) = project_with_sync_points(130);
        let (after, _) = project_with_sync_points(150);
        let before_segments = build_sync_segments_with_measure(
            &before,
            before.get_line(line_id).unwrap(),
            200.0,
            |text| text.chars().count() as f32 * 10.0,
        );
        let after_segments = build_sync_segments_with_measure(
            &after,
            after.get_line(line_id).unwrap(),
            200.0,
            |text| text.chars().count() as f32 * 10.0,
        );

        assert_eq!(before_segments[1].width_ratio, after_segments[1].width_ratio);
        assert_eq!(before_segments[2].start_ratio, after_segments[2].start_ratio);
        assert!((before_segments[1].start_ratio - 0.30).abs() < 0.0001);
        assert!((after_segments[1].start_ratio - 0.50).abs() < 0.0001);
    }

    #[test]
    fn point_creation_hits_a_letter_and_not_the_generated_gap() {
        let (project, line_id) = project_with_sync_points(130);
        let line = project.get_line(line_id).unwrap();
        let segments = build_sync_segments_with_measure(&project, line, 200.0, |text| {
            text.chars().count() as f32 * 10.0
        });
        let characters = line.text.chars().collect::<Vec<_>>();
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 30.0,
        };
        let uniform_ratios = |text: &str| {
            let count = text.chars().count();
            (0..=count)
                .map(|index| index as f32 / count.max(1) as f32)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            character_index_at_x_with_ratios(&characters, &segments, rect, 65.0, uniform_ratios),
            Some((3, 0.30))
        );
        assert_eq!(
            character_index_at_x_with_ratios(&characters, &segments, rect, 45.0, uniform_ratios),
            None,
            "the empty synchronization gap is not a valid point target"
        );
    }

    #[test]
    fn caret_hit_testing_uses_the_same_generated_spaces() {
        let (project, line_id) = project_with_sync_points(130);
        let line = project.get_line(line_id).unwrap();
        let segments = build_sync_segments_with_measure(&project, line, 200.0, |text| {
            text.chars().count() as f32 * 10.0
        });
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 30.0,
        };
        let uniform_ratios = |text: &str| {
            let count = text.chars().count();
            (0..=count)
                .map(|index| index as f32 / count.max(1) as f32)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            closest_cursor_index_at_x_with_ratios(&segments, rect, 58.0, uniform_ratios),
            Some(3),
            "a click in the gap snaps to the anchored letter, not a globally remapped index"
        );
    }
}
