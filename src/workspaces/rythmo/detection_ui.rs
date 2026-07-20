//! Editor-only interaction and rendering for semantic detection cues.

use super::*;
use crate::detection::{DetectionAddress, DetectionKind, MediaTick, TextAnchor};

const DETECTION_BADGE_W: f32 = 24.0;
const DETECTION_BADGE_H: f32 = 16.0;
const DETECTION_BUTTON_SIZE: f32 = 18.0;
const MENU_COLUMNS: usize = 2;
const MENU_ROW_H: f32 = 24.0;
const MENU_PADDING: f32 = 6.0;
const MENU_WIDTH: f32 = 370.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionHover {
    pub line_id: u64,
    pub media_tick: MediaTick,
    pub line_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionMenu {
    pub line_id: u64,
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
            line_id: hover.line_id,
            media_tick: hover.media_tick,
            x: button.x,
            y: button.y + button.height + 2.0,
            hover_index: None,
        });
        true
    }
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame)
}

fn clamp_tick_to_line(tick: MediaTick, line: &crate::rythmo_line::RythmoLine) -> MediaTick {
    tick.clamp(
        MediaTick::from_frame(line.start_frame),
        MediaTick::from_frame(line.end_frame()),
    )
}

fn detection_button_rect(hover: &DetectionHover) -> Rect {
    let x = hover.line_rect.x.max(
        hover
            .line_rect
            .x
            .min(hover.line_rect.x + hover.line_rect.width),
    );
    Rect {
        x: x + (tick_x_with_rect(hover.media_tick, &hover.line_rect) - x)
            .clamp(0.0, hover.line_rect.width)
            - DETECTION_BUTTON_SIZE / 2.0,
        y: hover.line_rect.y + hover.line_rect.height + 2.0,
        width: DETECTION_BUTTON_SIZE,
        height: DETECTION_BUTTON_SIZE,
    }
}

fn tick_x_with_rect(tick: MediaTick, rect: &Rect) -> f32 {
    // The hover tick is already clamped to this line. The caller stores the
    // line rectangle only to keep the D button alive while crossing its gap.
    let _ = tick;
    rect.x + rect.width / 2.0
}

fn actual_detection_button_rect(hover: &DetectionHover, current_frame: f64, zone: &Rect) -> Rect {
    Rect {
        x: tick_x(hover.media_tick, current_frame, zone) - DETECTION_BUTTON_SIZE / 2.0,
        y: hover.line_rect.y + hover.line_rect.height + 2.0,
        width: DETECTION_BUTTON_SIZE,
        height: DETECTION_BUTTON_SIZE,
    }
}

fn detection_badge_rect(
    cue: &crate::detection::DetectionCue,
    line_rect: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    Rect {
        x: tick_x(cue.media_tick, current_frame, zone) - DETECTION_BADGE_W / 2.0,
        y: line_rect.y + line_rect.height - DETECTION_BADGE_H - 2.0,
        width: DETECTION_BADGE_W,
        height: DETECTION_BADGE_H,
    }
}

fn menu_rows() -> usize {
    DetectionKind::ALL.len().div_ceil(MENU_COLUMNS)
}

fn menu_rect(menu: &DetectionMenu, zone: &Rect) -> Rect {
    let height = menu_rows() as f32 * MENU_ROW_H + MENU_PADDING * 2.0;
    Rect {
        x: menu
            .x
            .clamp(zone.x, (zone.x + zone.width - MENU_WIDTH).max(zone.x)),
        y: menu
            .y
            .clamp(zone.y, (zone.y + zone.height - height).max(zone.y)),
        width: MENU_WIDTH,
        height,
    }
}

fn menu_item_rect(menu: &DetectionMenu, zone: &Rect, index: usize) -> Rect {
    let menu_rect = menu_rect(menu, zone);
    let rows = menu_rows();
    let column = index / rows;
    let row = index % rows;
    let column_width = (menu_rect.width - MENU_PADDING * 2.0) / MENU_COLUMNS as f32;
    Rect {
        x: menu_rect.x + MENU_PADDING + column as f32 * column_width,
        y: menu_rect.y + MENU_PADDING + row as f32 * MENU_ROW_H,
        width: column_width,
        height: MENU_ROW_H,
    }
}

fn hit_existing_detection(
    ctx: &RythmoCtx<'_>,
    state: &RythmoState,
    x: f32,
    y: f32,
) -> Option<DetectionAddress> {
    for line in ctx.project.lines() {
        let line_rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        let Some(data) = ctx.project.detections().line(line.id) else {
            continue;
        };
        for cue in data.detections() {
            if detection_badge_rect(cue, line_rect, ctx.current_frame, ctx.zone).contains(x, y) {
                return Some(DetectionAddress {
                    line_id: line.id,
                    detection_id: cue.id,
                });
            }
        }
    }
    let _ = state;
    None
}

fn anchor_for_tick(line: &crate::rythmo_line::RythmoLine, tick: MediaTick) -> TextAnchor {
    let count = line.text.chars().count();
    if count == 0 || line.duration_frames <= 0 {
        return TextAnchor::BeforeText;
    }
    let frame = tick.as_frame_position();
    let ratio = ((frame - line.start_frame as f64) / line.duration_frames as f64).clamp(0.0, 1.0);
    let index = (ratio * count as f64).round() as usize;
    if index == 0 {
        TextAnchor::BeforeText
    } else if index >= count {
        TextAnchor::AfterText
    } else {
        TextAnchor::Grapheme {
            index: index as u32,
        }
    }
}

fn navigate_detection(project: &Project, state: &mut RythmoState, direction: i32) -> bool {
    let (line_id, current) = match state.selected {
        Some(Selection::Line(line_id)) => (line_id, None),
        Some(Selection::Detection(address)) => (address.line_id, Some(address.detection_id)),
        _ => return false,
    };
    let Some(data) = project.detections().line(line_id) else {
        return false;
    };
    let cues = data.detections();
    if cues.is_empty() {
        return false;
    }
    let index = if let Some(current) = current {
        let current_index = cues.iter().position(|cue| cue.id == current).unwrap_or(0);
        if direction < 0 {
            current_index.saturating_sub(1)
        } else {
            (current_index + 1).min(cues.len() - 1)
        }
    } else if direction < 0 {
        cues.len() - 1
    } else {
        0
    };
    state.selected = Some(Selection::Detection(DetectionAddress {
        line_id,
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
                let line = ctx.project.get_line(drag.address.line_id)?;
                let tick = clamp_tick_to_line(pointer_tick(*x, ctx.current_frame, ctx.zone), line);
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address: drag.address,
                    media_tick: tick,
                }));
            }

            if let Some(mut menu) = state.detection_menu {
                let hover = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                    .map(|(index, _)| index);
                if hover != menu.hover_index {
                    menu.hover_index = hover;
                    state.detection_menu = Some(menu);
                }
                return Some(EventResponse::Consumed);
            }

            if let Some(hover) = state.detection_hover {
                if actual_detection_button_rect(&hover, ctx.current_frame, ctx.zone)
                    .contains(*x, *y)
                {
                    return Some(EventResponse::Consumed);
                }
            }

            let found = hit_test_line_and_track(ctx, state, *x, *y).0;
            let next = found.and_then(|line_id| {
                let line = ctx.project.get_line(line_id)?;
                let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
                Some(DetectionHover {
                    line_id,
                    media_tick: clamp_tick_to_line(
                        pointer_tick(*x, ctx.current_frame, ctx.zone),
                        line,
                    ),
                    line_rect: rect,
                })
            });
            if next != state.detection_hover {
                state.detection_hover = next;
            }
            // Keep propagating ordinary pointer movement so the established
            // line-hover state and cursor feedback remain in sync with the
            // detection preview.
        }
        UiEvent::MousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                if let Some((_, kind)) = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                {
                    let line = ctx.project.get_line(menu.line_id)?;
                    let target = anchor_for_tick(line, menu.media_tick);
                    state.detection_menu = None;
                    return Some(EventResponse::Action(UiAction::AddDetection {
                        line_id: menu.line_id,
                        kind: *kind,
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

            if let Some(hover) = state.detection_hover {
                if actual_detection_button_rect(&hover, ctx.current_frame, ctx.zone)
                    .contains(*x, *y)
                {
                    state.open_detection_palette_from_hover();
                    return Some(EventResponse::Consumed);
                }
            }
        }
        UiEvent::MouseRelease { .. } if state.detection_drag.is_some() => {
            state.detection_drag = None;
            return Some(EventResponse::Consumed);
        }
        UiEvent::KeyInput { text } if text.eq_ignore_ascii_case("d") => {
            if state.open_detection_palette_from_hover() {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            if state.detection_menu.take().is_some() {
                return Some(EventResponse::Consumed);
            }
            if let Some(Selection::Detection(address)) = state.selected {
                state.selected = Some(Selection::Line(address.line_id));
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
            if let Some(Selection::Detection(address)) = state.selected {
                return Some(EventResponse::Action(UiAction::DeleteDetection { address }));
            }
        }
        _ => {}
    }
    None
}

fn push_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], border: [f32; 4]) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: 3.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0, 0.0, 0.0, 0.22],
        shadow_blur: 2.0,
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
) {
    for line in project.lines() {
        let line_rect = line_rect(project, line, current_frame, zone);
        let Some(data) = project.detections().line(line.id) else {
            continue;
        };
        for cue in data.detections() {
            let x = tick_x(cue.media_tick, current_frame, zone);
            if x < zone.x - DETECTION_BADGE_W || x > zone.x + zone.width + DETECTION_BADGE_W {
                continue;
            }
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: cue.id,
            };
            let selected =
                matches!(state.selected, Some(Selection::Detection(current)) if current == address);
            let badge = detection_badge_rect(cue, line_rect, current_frame, zone);
            push_quad(
                quads,
                Rect {
                    x: x - 0.75,
                    y: line_rect.y,
                    width: 1.5,
                    height: line_rect.height,
                },
                if selected {
                    [1.0, 0.72, 0.12, 0.95]
                } else {
                    [0.78, 0.78, 0.82, 0.72]
                },
                [0.0; 4],
            );
            push_quad(
                quads,
                badge,
                if selected {
                    [0.40, 0.24, 0.04, 0.98]
                } else {
                    [0.11, 0.11, 0.14, 0.96]
                },
                if selected {
                    [1.0, 0.72, 0.12, 1.0]
                } else {
                    [0.75, 0.75, 0.82, 0.75]
                },
            );
            labels.push(LabelInfo {
                text: cue.kind.short_label(),
                bounds: badge,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 1.0,
                font_size_override: Some(9.0),
                color_override: Some([238, 238, 244]),
                font_family_override: None,
            });
        }
    }

    if let Some(hover) = state.detection_hover {
        let x = tick_x(hover.media_tick, current_frame, zone);
        let mut y = hover.line_rect.y;
        while y < hover.line_rect.y + hover.line_rect.height {
            push_quad(
                quads,
                Rect {
                    x: x - 0.5,
                    y,
                    width: 1.0,
                    height: 3.0_f32.min(hover.line_rect.y + hover.line_rect.height - y),
                },
                [0.65, 0.65, 0.68, 0.72],
                [0.0; 4],
            );
            y += 6.0;
        }
        let button = actual_detection_button_rect(&hover, current_frame, zone);
        push_quad(
            quads,
            button,
            [0.15, 0.15, 0.18, 0.98],
            [0.72, 0.72, 0.78, 0.9],
        );
        labels.push(LabelInfo {
            text: "D",
            bounds: button,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([240, 240, 245]),
            font_family_override: None,
        });
    }

    if let Some(menu) = state.detection_menu {
        let outer = menu_rect(&menu, zone);
        push_quad(
            quads,
            outer,
            [0.055, 0.055, 0.07, 0.99],
            [0.48, 0.48, 0.56, 0.9],
        );
        for (index, kind) in DetectionKind::ALL.iter().enumerate() {
            let row = menu_item_rect(&menu, zone, index);
            if menu.hover_index == Some(index) {
                push_quad(
                    quads,
                    row,
                    [0.19, 0.27, 0.46, 0.98],
                    [0.40, 0.58, 0.92, 0.7],
                );
            }
            let sigle = Rect {
                x: row.x + 3.0,
                y: row.y + 3.0,
                width: 30.0,
                height: row.height - 6.0,
            };
            labels.push(LabelInfo {
                text: kind.short_label(),
                bounds: sigle,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(9.0),
                color_override: Some([245, 210, 90]),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: kind.display_name(),
                bounds: Rect {
                    x: row.x + 37.0,
                    y: row.y,
                    width: row.width - 40.0,
                    height: row.height,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 2.0,
                font_size_override: Some(11.0),
                color_override: Some([232, 232, 238]),
                font_family_override: None,
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
    fn palette_contains_every_detection_kind() {
        assert_eq!(DetectionKind::ALL.len(), 17);
        assert_eq!(menu_rows(), 9);
    }
}
