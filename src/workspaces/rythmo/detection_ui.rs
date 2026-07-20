//! Editor-only interaction and rendering for professional detection signs and
//! per-letter synchronization points.

use super::*;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionKind, MediaTick, TextAnchor,
};

const DETECTION_ICON_SIZE: f32 = 18.0;
const DETECTION_HIT_SIZE: f32 = 26.0;
const DETECTION_BUTTON_SIZE: f32 = 18.0;
const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const MENU_WIDTH: f32 = MENU_PADDING * 2.0
    + MENU_ICON_SIZE * DetectionKind::ALL.len() as f32
    + MENU_GAP * (DetectionKind::ALL.len() as f32 - 1.0);
const MENU_HEIGHT: f32 = MENU_ICON_SIZE + MENU_PADDING * 2.0;
const SYNC_DOT_SIZE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionHover {
    pub track: u8,
    pub media_tick: MediaTick,
    pub screen_x: f32,
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
        y: hover.track_rect.y + hover.track_rect.height - DETECTION_BUTTON_SIZE - 2.0,
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

fn sync_anchor_x(line_rect: Rect, character_index: usize, character_count: usize) -> f32 {
    line_rect.x
        + line_rect.width * ((character_index as f32 + 0.5) / character_count.max(1) as f32)
}

fn sync_dot_rect(x: f32, line_rect: Rect) -> Rect {
    Rect {
        x: x - SYNC_DOT_SIZE / 2.0,
        y: line_rect.y + line_rect.height - SYNC_DOT_SIZE - 2.0,
        width: SYNC_DOT_SIZE,
        height: SYNC_DOT_SIZE,
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

fn hit_existing_detection(ctx: &RythmoCtx<'_>, x: f32, y: f32) -> Option<DetectionAddress> {
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
        let Some(data) = ctx.project.detections().line(line.id) else {
            continue;
        };
        let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        for cue in data.text_sync_cues() {
            let dot = sync_dot_rect(tick_x(cue.media_tick, ctx.current_frame, ctx.zone), rect);
            let hit = Rect {
                x: dot.x - 5.0,
                y: dot.y - 5.0,
                width: dot.width + 10.0,
                height: dot.height + 10.0,
            };
            if hit.contains(x, y) {
                return Some(DetectionAddress {
                    line_id: line.id,
                    detection_id: cue.id,
                });
            }
        }
    }
    None
}

fn existing_sync_at(project: &Project, line_id: u64, character_index: usize) -> bool {
    project.detections().line(line_id).is_some_and(|data| {
        data.text_sync_cues()
            .any(|cue| cue.target.grapheme_index() == Some(character_index as u32))
    })
}

fn hit_sync_placeholder(
    ctx: &RythmoCtx<'_>,
    x: f32,
    y: f32,
) -> Option<(u64, usize, MediaTick)> {
    for line in ctx.project.lines() {
        let character_count = line.text.chars().count();
        if character_count == 0 || line.duration_frames <= 0 {
            continue;
        }
        let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        if y < rect.y || y > rect.y + rect.height {
            continue;
        }
        for character_index in 0..character_count {
            if existing_sync_at(ctx.project, line.id, character_index) {
                continue;
            }
            let anchor_x = sync_anchor_x(rect, character_index, character_count);
            let dot = sync_dot_rect(anchor_x, rect);
            let hit = Rect {
                x: dot.x - 4.0,
                y: dot.y - 4.0,
                width: dot.width + 8.0,
                height: dot.height + 8.0,
            };
            if hit.contains(x, y) {
                let ratio = (character_index as f64 + 0.5) / character_count as f64;
                let frame = line.start_frame as f64 + line.duration_frames as f64 * ratio;
                return Some((
                    line.id,
                    character_index,
                    MediaTick::from_frame_position(frame),
                ));
            }
        }
    }
    None
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
                    let line = ctx.project.get_line(drag.address.line_id)?;
                    tick = tick.clamp(
                        MediaTick::from_frame(line.start_frame),
                        MediaTick::from_frame(line.end_frame()),
                    );
                }
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address: drag.address,
                    media_tick: tick,
                }));
            }

            if let Some(mut menu) = state.detection_menu {
                menu.hover_index = DetectionKind::ALL
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

            state.detection_hover = track_under_pointer(ctx, *y).map(|(track, rect)| DetectionHover {
                track,
                media_tick: pointer_tick(*x, ctx.current_frame, ctx.zone),
                screen_x: *x,
                track_rect: rect,
            });
        }
        UiEvent::MousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                if let Some((_, kind)) = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                {
                    state.detection_menu = None;
                    return Some(EventResponse::Action(UiAction::AddDetection {
                        line_id: track_storage_line_id(menu.track),
                        kind: *kind,
                        media_tick: menu.media_tick,
                        target: TextAnchor::BeforeText,
                    }));
                }
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }

            if let Some(address) = hit_existing_detection(ctx, *x, *y) {
                state.selected = Some(Selection::Detection(address));
                state.detection_drag = Some(DetectionDrag { address });
                return Some(EventResponse::Consumed);
            }

            if let Some((line_id, character_index, tick)) = hit_sync_placeholder(ctx, *x, *y) {
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
    quads.push(QuadInstance {
        rect: [x1, y1 - thickness / 2.0, length, thickness],
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

/// The geometry is identical to the SVG assets loaded from
/// `src/icons/detection`; no textual fallback is drawn.
fn render_detection_asset(
    quads: &mut Vec<QuadInstance>,
    kind: DetectionKind,
    bounds: Rect,
    color: [f32; 4],
) {
    let cx = bounds.x + bounds.width / 2.0;
    let cy = bounds.y + bounds.height / 2.0;
    let s = bounds.width.min(bounds.height) * 0.36;
    let t = (bounds.width.min(bounds.height) * 0.085).max(1.2);
    match kind {
        DetectionKind::Labial => {
            push_line(quads, cx - s, cy, cx + s, cy, t, color);
            push_line(quads, cx, cy - s, cx, cy + s, t, color);
            push_line(quads, cx - s * 0.72, cy - s * 0.72, cx + s * 0.72, cy + s * 0.72, t, color);
        }
        DetectionKind::SemiLabial => {
            push_line(quads, cx - s, cy, cx + s, cy, t, color);
            push_line(quads, cx, cy - s, cx, cy + s, t, color);
            push_line(quads, cx, cy, cx + s * 0.72, cy + s * 0.72, t, color);
        }
        DetectionKind::MouthOpen => {
            push_line(quads, cx, cy - s, cx + s, cy, t, color);
            push_line(quads, cx + s, cy, cx, cy + s, t, color);
            push_line(quads, cx, cy + s, cx - s, cy, t, color);
            push_line(quads, cx - s, cy, cx, cy - s, t, color);
        }
        DetectionKind::MouthClosed => {
            push_line(quads, cx - s, cy, cx + s, cy, t * 1.35, color);
            push_line(quads, cx - s * 0.45, cy - s * 0.35, cx + s * 0.45, cy - s * 0.35, t * 0.65, color);
        }
        DetectionKind::TeethVisible => {
            push_line(quads, cx - s, cy - s * 0.55, cx + s, cy - s * 0.55, t, color);
            push_line(quads, cx - s, cy + s * 0.55, cx + s, cy + s * 0.55, t, color);
            for offset in [-0.62_f32, 0.0, 0.62] {
                push_line(quads, cx + s * offset, cy - s * 0.55, cx + s * offset, cy + s * 0.55, t * 0.75, color);
            }
        }
        DetectionKind::Breath => {
            for offset in [-0.52_f32, 0.0, 0.52] {
                push_line(quads, cx - s, cy + s * offset, cx + s, cy + s * (offset - 0.28), t * 0.72, color);
            }
        }
        DetectionKind::Reaction => {
            for angle in [0.0_f32, 0.785, 1.57, 2.355] {
                let dx = angle.cos() * s;
                let dy = angle.sin() * s;
                push_line(quads, cx - dx, cy - dy, cx + dx, cy + dy, t, color);
            }
        }
        DetectionKind::TextSyncPoint => {
            push_quad(
                quads,
                Rect {
                    x: cx - s * 0.28,
                    y: cy - s * 0.28,
                    width: s * 0.56,
                    height: s * 0.56,
                },
                color,
                s * 0.28,
            );
        }
    }
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    _labels: &mut Vec<LabelInfo<'a>>,
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
            render_detection_asset(
                quads,
                cue.kind,
                Rect {
                    x: hit.x + (hit.width - DETECTION_ICON_SIZE) / 2.0,
                    y: hit.y + (hit.height - DETECTION_ICON_SIZE) / 2.0,
                    width: DETECTION_ICON_SIZE,
                    height: DETECTION_ICON_SIZE,
                },
                if selected {
                    [0.78, 0.88, 1.0, 1.0]
                } else {
                    [0.92, 0.92, 0.95, 0.94]
                },
            );
        }
    }

    for line in project.lines() {
        let character_count = line.text.chars().count();
        if character_count == 0 || line.duration_frames <= 0 {
            continue;
        }
        let rect = line_rect(project, line, current_frame, zone);
        let data = project.detections().line(line.id);
        for character_index in 0..character_count {
            let anchor_x = sync_anchor_x(rect, character_index, character_count);
            let existing = data.and_then(|data| {
                data.text_sync_cues()
                    .find(|cue| cue.target.grapheme_index() == Some(character_index as u32))
            });
            if let Some(cue) = existing {
                let cue_x = tick_x(cue.media_tick, current_frame, zone);
                let address = DetectionAddress {
                    line_id: line.id,
                    detection_id: cue.id,
                };
                let selected = selected_address == Some(address);
                if (cue_x - anchor_x).abs() > 1.5 {
                    push_line(
                        quads,
                        anchor_x,
                        rect.y + rect.height - 5.0,
                        cue_x,
                        rect.y + rect.height - 5.0,
                        1.0,
                        [0.43, 0.68, 1.0, 0.46],
                    );
                }
                push_line(
                    quads,
                    cue_x,
                    rect.y + 2.0,
                    cue_x,
                    rect.y + rect.height - 2.0,
                    if selected { 1.5 } else { 1.0 },
                    if selected {
                        [0.55, 0.78, 1.0, 0.92]
                    } else {
                        [0.48, 0.70, 0.96, 0.62]
                    },
                );
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
            } else {
                push_quad(
                    quads,
                    sync_dot_rect(anchor_x, rect),
                    [0.70, 0.72, 0.78, 0.28],
                    6.0,
                );
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
        for (index, kind) in DetectionKind::ALL.iter().copied().enumerate() {
            let item = menu_item_rect(&menu, zone, index);
            if menu.hover_index == Some(index) {
                push_quad(quads, item, [0.18, 0.32, 0.58, 0.82], 5.0);
            }
            render_detection_asset(
                quads,
                kind,
                Rect {
                    x: item.x + 5.0,
                    y: item.y + 5.0,
                    width: item.width - 10.0,
                    height: item.height - 10.0,
                },
                [0.94, 0.95, 0.98, 1.0],
            );
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
    fn palette_contains_only_the_seven_professional_signs() {
        assert_eq!(DetectionKind::ALL.len(), 7);
        assert!(!DetectionKind::ALL.contains(&DetectionKind::TextSyncPoint));
    }
}