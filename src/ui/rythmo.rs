use super::renderer::StretchedText;
use super::widget::{EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign};
use crate::project::Project;
use crate::rythmo_line::MarkerKind;

const TICK_WIDTH: f32 = 1.0;
const TICK_LONG: f32 = 12.0;
const TICK_SHORT: f32 = 6.0;
const TICK_GAP: f32 = 8.0;
const TICK_COLOR: [f32; 4] = [0.40, 0.40, 0.45, 0.5];

const PLAYHEAD_WIDTH: f32 = 2.0;
const PLAYHEAD_COLOR: [f32; 4] = [0.85, 0.15, 0.15, 1.0];

const RULER_HEIGHT: f32 = 14.0;
const PIXELS_PER_FRAME: f32 = 6.0;
const NUM_SLOTS: f32 = 4.0;
const HANDLE_WIDTH: f32 = 6.0;
const HANDLE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
const LINE_BORDER: [f32; 4] = [0.5, 0.5, 0.55, 0.3];
const LINE_BORDER_HOVER: [f32; 4] = [0.6, 0.6, 0.65, 0.5];
const LINE_RADIUS: f32 = 2.0;
const CURSOR_WIDTH: f32 = 1.5;
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];

pub struct RythmoState {
    pub hovered_line: Option<u64>,
    pub editing_line: Option<u64>,
    pub line_input: super::text_input::TextInputState,
    pub editing_character: Option<u64>,
    pub char_input: super::text_input::TextInputState,
    pub color_picker: super::color_picker::ColorPickerState,
    pub autocomplete_index: Option<usize>,  // keyboard selection
    pub autocomplete_hover: Option<usize>,  // mouse hover
    pub dragging: Option<DragState>,
}

pub struct DragState {
    pub line_id: u64,
    pub handle: DragHandle,
    pub drag_start_x: f32,
    pub original_start: i64,
    pub original_duration: i64,
    pub original_y_slot: f32,
    pub drag_start_y: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DragHandle {
    Left,
    Right,
    Body,
}

impl RythmoState {
    pub fn new() -> Self {
        Self {
            hovered_line: None,
            editing_line: None,
            line_input: super::text_input::TextInputState::new(),
            editing_character: None,
            char_input: super::text_input::TextInputState::new(),
            color_picker: super::color_picker::ColorPickerState::new(),
            autocomplete_index: None,
            autocomplete_hover: None,
            dragging: None,
        }
    }

    pub fn is_editing(&self) -> bool {
        self.editing_line.is_some() || self.editing_character.is_some()
    }

    pub fn stop_line_editing(&mut self) {
        self.editing_line = None;
        self.line_input.deactivate();
    }

    pub fn stop_char_editing(&mut self) {
        self.editing_character = None;
        self.char_input.deactivate();
        self.color_picker.close();
        self.autocomplete_index = None;
        self.autocomplete_hover = None;
    }
}

fn frame_to_x(frame: i64, current_frame: i64, zone: &Rect) -> f32 {
    let center_x = zone.x + zone.width / 2.0;
    center_x + (frame - current_frame) as f32 * PIXELS_PER_FRAME
}

fn x_to_frame(x: f32, current_frame: i64, zone: &Rect) -> i64 {
    let center_x = zone.x + zone.width / 2.0;
    current_frame + ((x - center_x) / PIXELS_PER_FRAME) as i64
}

fn y_to_slot(y: f32, zone: &Rect) -> f32 {
    let (total_slot_h, _) = slot_metrics(zone);
    let relative_y = y - zone.y - RULER_HEIGHT;
    let slot_index = (relative_y / total_slot_h).floor().clamp(0.0, NUM_SLOTS - 1.0);
    (slot_index / NUM_SLOTS).clamp(0.0, 0.75)
}

fn badge_rect_for_line(line: &crate::rythmo_line::RythmoLine, current_frame: i64, zone: &Rect) -> Rect {
    let x1 = frame_to_x(line.start_frame, current_frame, zone);
    let (total_slot_h, _) = slot_metrics(zone);
    let slot_index = (line.y_slot * NUM_SLOTS).round() as usize;
    let y_base = zone.y + RULER_HEIGHT + slot_index as f32 * total_slot_h;
    let w = badge_width(&line.character_name);
    Rect { x: x1, y: y_base, width: w, height: BADGE_HEIGHT }
}

fn slot_metrics(zone: &Rect) -> (f32, f32) {
    // Each slot = badge + gap + line body. 4 slots fit in the usable area.
    let usable_h = zone.height - RULER_HEIGHT;
    let total_slot_h = usable_h / NUM_SLOTS;
    let line_h = (total_slot_h - BADGE_HEIGHT - BADGE_GAP).max(8.0);
    (total_slot_h, line_h)
}

fn line_rect(line: &crate::rythmo_line::RythmoLine, current_frame: i64, zone: &Rect) -> Rect {
    let x1 = frame_to_x(line.start_frame, current_frame, zone);
    let x2 = frame_to_x(line.end_frame(), current_frame, zone);
    let (total_slot_h, line_h) = slot_metrics(zone);
    // y_slot is 0.0, 0.25, 0.5, 0.75 → maps to slot index 0,1,2,3
    let slot_index = (line.y_slot * NUM_SLOTS).round() as usize;
    let y_base = zone.y + RULER_HEIGHT + slot_index as f32 * total_slot_h;
    let y = y_base + BADGE_HEIGHT + BADGE_GAP;
    Rect { x: x1, y, width: (x2 - x1).max(2.0), height: line_h }
}

fn badge_width(name: &str) -> f32 {
    let chars = name.chars().count().max(1) as f32;
    (chars * BADGE_CHAR_W + BADGE_PADDING_H * 2.0).max(BADGE_MIN_W)
}

pub fn render_rythmo_base(zone: &Rect, current_frame: i64) -> Vec<QuadInstance> {
    let mut quads = Vec::new();

    // Ticks anchored to absolute frame positions via frame_to_x (DRY)
    const FRAMES_PER_TICK: i64 = 2;

    let visible_frames = (zone.width / PIXELS_PER_FRAME) as i64 + 4;
    let first_tick = ((current_frame - visible_frames / 2) / FRAMES_PER_TICK) * FRAMES_PER_TICK;

    let mut tick_frame = first_tick;
    loop {
        let x = frame_to_x(tick_frame, current_frame, zone);
        if x > zone.x + zone.width { break; }
        if x >= zone.x {
            let tick_index = tick_frame / FRAMES_PER_TICK;
            let h = if tick_index % 2 == 0 { TICK_LONG } else { TICK_SHORT };
            quads.push(QuadInstance {
                rect: [x, zone.y, TICK_WIDTH, h],
                color: TICK_COLOR, color_bottom: TICK_COLOR,
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                _padding: [0.0; 3],
            });
        }
        tick_frame += FRAMES_PER_TICK;
    }

    let playhead_x = zone.x + (zone.width - PLAYHEAD_WIDTH) / 2.0;
    quads.push(QuadInstance {
        rect: [playhead_x, zone.y, PLAYHEAD_WIDTH, zone.height],
        color: PLAYHEAD_COLOR, color_bottom: PLAYHEAD_COLOR,
        border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
        shadow_offset: [0.0, 0.0],
        shadow_color: [0.85, 0.15, 0.15, 0.3],
        shadow_blur: 4.0,
        _padding: [0.0; 3],
    });

    quads
}

/// Returns optional (line_id, cursor_pos, text_x, text_w, rect_y, rect_h) for cursor rendering.
const BADGE_HEIGHT: f32 = 16.0;
const BADGE_PADDING_H: f32 = 6.0;
const BADGE_GAP: f32 = 2.0;
const BADGE_RADIUS: f32 = 2.0;
const BADGE_CHAR_W: f32 = 6.0; // approximate char width at font size 10
const BADGE_MIN_W: f32 = 16.0;
const BADGE_FONT_SIZE: f32 = 10.0;

pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: i64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
) -> Option<(u64, usize, f32, f32, f32, f32)> {
    let mut cursor_info = None;
    for line in &project.lines {
        let r = line_rect(line, current_frame, zone);

        if r.x + r.width < zone.x || r.x > zone.x + zone.width {
            continue;
        }

        let is_hovered = state.hovered_line == Some(line.id);
        let is_editing = state.editing_line == Some(line.id);

        // Subtle dark background + border
        let bg = if is_editing {
            [0.12, 0.12, 0.15, 0.6]
        } else if is_hovered {
            [0.10, 0.10, 0.13, 0.4]
        } else {
            [0.08, 0.08, 0.10, 0.3]
        };
        let border = if is_hovered || is_editing { LINE_BORDER_HOVER } else { LINE_BORDER };
        quads.push(QuadInstance {
            rect: [r.x, r.y, r.width, r.height],
            color: bg, color_bottom: bg,
            border_color: border,
            border_width: 1.0,
            border_radius: LINE_RADIUS,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });

        // Stretched text or special rendering for breath arrows
        if !line.text.is_empty() {
            if line.text == "↑" || line.text == "↓" {
                render_breath_arrow(&r, line.text == "↑", quads);
            } else {
                stretched.push(StretchedText {
                    line_id: line.id,
                    text: line.text.clone(),
                    dest_rect: Rect { x: r.x, y: r.y, width: r.width, height: r.height },
                });
            }
        }

        // Cursor info for mod.rs to resolve with renderer
        if is_editing && state.line_input.cursor_visible() {
            cursor_info = Some((line.id, state.line_input.cursor_pos, r.x, r.width, r.y, r.height));
        }

        // Handles (only on hover/editing)
        if is_hovered || is_editing {
            quads.push(QuadInstance {
                rect: [r.x, r.y, HANDLE_WIDTH, r.height],
                color: HANDLE_COLOR, color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4], border_width: 0.0, border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                _padding: [0.0; 3],
            });
            quads.push(QuadInstance {
                rect: [r.x + r.width - HANDLE_WIDTH, r.y, HANDLE_WIDTH, r.height],
                color: HANDLE_COLOR, color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4], border_width: 0.0, border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                _padding: [0.0; 3],
            });
        }

        // Character badge — colored rectangle above the line with name inside
        let br = badge_rect_for_line(line, current_frame, zone);
        let is_editing_char = state.editing_character == Some(line.id);
        let badge_border = if is_editing_char {
            [0.8, 0.8, 0.85, 0.8]
        } else {
            [0.0_f32; 4]
        };
        quads.push(QuadInstance {
            rect: [br.x, br.y, br.width, br.height],
            color: line.character_color, color_bottom: line.character_color,
            border_color: badge_border,
            border_width: if is_editing_char { 1.0 } else { 0.0 },
            border_radius: BADGE_RADIUS,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });

        // Character name text — black on bright backgrounds for contrast
        if !line.character_name.is_empty() {
            let [r, g, b, _] = line.character_color;
            let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
            let text_color = if luminance > 0.55 { Some([0, 0, 0]) } else { None };

            labels.push(LabelInfo {
                text: &line.character_name,
                bounds: br,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: BADGE_PADDING_H,
                font_size_override: Some(BADGE_FONT_SIZE),
                color_override: text_color,
            });
        }

        // Character name editing cursor
        if is_editing_char && state.char_input.cursor_visible() {
            let char_count = line.character_name.chars().count();
            let cursor_pos = state.char_input.cursor_pos;
            let _text_area_w = br.width - BADGE_PADDING_H * 2.0;
            // Approximate: center the text, then position cursor
            let total_text_w = char_count as f32 * BADGE_CHAR_W;
            let text_start_x = br.x + (br.width - total_text_w) / 2.0;
            let cx = text_start_x + cursor_pos as f32 * BADGE_CHAR_W;
            let margin = 3.0;
            quads.push(QuadInstance {
                rect: [cx, br.y + margin, CURSOR_WIDTH, br.height - margin * 2.0],
                color: CURSOR_COLOR, color_bottom: CURSOR_COLOR,
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                _padding: [0.0; 3],
            });
        }

    }

    cursor_info
}

/// Render a diagonal arrow for breath markers.
/// `up` = bottom-left → top-right (inspiration), `!up` = top-left → bottom-right (expiration).
fn render_breath_arrow(r: &Rect, up: bool, quads: &mut Vec<QuadInstance>) {
    let margin = 4.0;
    let x0 = r.x + margin;
    let x1 = r.x + r.width - margin;
    let y_top = r.y + margin;
    let y_bot = r.y + r.height - margin;

    let steps = ((x1 - x0) / 2.0).max(4.0) as usize;
    let step_w = (x1 - x0) / steps as f32;
    let step_h = (y_bot - y_top) / steps as f32;
    let thickness = 2.0;

    let color = [0.85, 0.85, 0.90, 0.9];

    for i in 0..steps {
        let px = x0 + i as f32 * step_w;
        let py = if up {
            y_bot - i as f32 * step_h - thickness
        } else {
            y_top + i as f32 * step_h
        };
        quads.push(QuadInstance {
            rect: [px, py, step_w + 1.0, thickness],
            color, color_bottom: color,
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });
    }

    // Arrowhead at the end
    let arrow_size = 6.0;
    let (ax, ay) = if up { (x1, y_top) } else { (x1, y_bot) };
    // Two small quads forming a ">" arrowhead
    let dy = if up { 1.0 } else { -1.0 };
    quads.push(QuadInstance {
        rect: [ax - arrow_size, ay + dy * arrow_size * 0.3, arrow_size, thickness],
        color, color_bottom: color,
        border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
        shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
        _padding: [0.0; 3],
    });
    quads.push(QuadInstance {
        rect: [ax - thickness, ay, thickness, arrow_size * dy.abs()],
        color, color_bottom: color,
        border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
        shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
        _padding: [0.0; 3],
    });
}

/// Render autocomplete dropdown AFTER all lines (so it's on top).
pub fn render_autocomplete<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: i64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let line_id = match state.editing_character {
        Some(id) => id,
        None => return,
    };
    let line = match project.get_line(line_id) {
        Some(l) => l,
        None => return,
    };
    if line.character_name.is_empty() { return; }

    let suggestions = project.autocomplete(&line.character_name);
    if suggestions.is_empty() { return; }

    let r = line_rect(line, current_frame, zone);
    let br = badge_rect_for_line(line, current_frame, zone);
    let dropdown_x = br.x;
    let mut dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;
    let dropdown_h = suggestions.len() as f32 * item_h;

    // Background
    quads.push(QuadInstance {
        rect: [dropdown_x, dropdown_y, dropdown_w, dropdown_h],
        color: [0.15, 0.15, 0.17, 0.95],
        color_bottom: [0.12, 0.12, 0.14, 0.95],
        border_color: [0.3, 0.3, 0.36, 0.6],
        border_width: 1.0, border_radius: 3.0,
        shadow_offset: [0.0, 2.0], shadow_color: [0.0, 0.0, 0.0, 0.4], shadow_blur: 6.0,
        _padding: [0.0; 3],
    });

    for (i, suggestion) in suggestions.iter().enumerate() {
        let is_selected = state.autocomplete_index == Some(i);
        let is_hovered = state.autocomplete_hover == Some(i);

        // Highlight
        if is_selected || is_hovered {
            let alpha = if is_selected { 0.15 } else { 0.08 };
            quads.push(QuadInstance {
                rect: [dropdown_x + 2.0, dropdown_y + 1.0, dropdown_w - 4.0, item_h - 2.0],
                color: [1.0, 1.0, 1.0, alpha], color_bottom: [1.0, 1.0, 1.0, alpha],
                border_color: [0.0; 4], border_width: 0.0, border_radius: 2.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                _padding: [0.0; 3],
            });
        }

        // Color swatch
        quads.push(QuadInstance {
            rect: [dropdown_x + 4.0, dropdown_y + 4.0, 12.0, item_h - 8.0],
            color: suggestion.color, color_bottom: suggestion.color,
            border_color: [0.0; 4], border_width: 0.0, border_radius: 2.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            _padding: [0.0; 3],
        });
        // Name label
        labels.push(LabelInfo {
            text: &suggestion.name,
            bounds: Rect { x: dropdown_x + 20.0, y: dropdown_y, width: dropdown_w - 24.0, height: item_h },
            h_align: HAlign::Left, v_align: VAlign::Center,
            overflow: Overflow::Ellipsis, padding: 2.0,
            font_size_override: Some(11.0), color_override: None,
        });
        dropdown_y += item_h;
    }
}

/// Returns the autocomplete suggestion rect for hit testing
/// Render markers (boucle, out, scene change, liaisons) on the bande rythmo.
pub fn render_markers<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: i64,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    liaison_icons: &mut Vec<IconInstance>,
    liaison_left_uv: [f32; 4],
    liaison_right_uv: [f32; 4],
) {
    for marker in &project.markers {
        let x = frame_to_x(marker.frame, current_frame, zone);
        if x < zone.x - 20.0 || x > zone.x + zone.width + 20.0 { continue; }

        match &marker.kind {
            MarkerKind::Boucle => {
                let red = [0.85, 0.15, 0.15, 0.9];
                // Red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: red, color_bottom: red,
                    border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                    shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                    _padding: [0.0; 3],
                });
                // Big "X" — two crossed bars rendered with stepped quads
                let cx = x;
                let cy = zone.y + zone.height / 2.0;
                let size = 12.0;
                let steps = 8;
                let thickness = 2.5;
                for i in 0..steps {
                    let t = i as f32 / steps as f32;
                    let px = cx - size + t * size * 2.0;
                    // "\" diagonal
                    let py1 = cy - size + t * size * 2.0;
                    quads.push(QuadInstance {
                        rect: [px, py1, thickness, thickness],
                        color: red, color_bottom: red,
                        border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                        shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                        _padding: [0.0; 3],
                    });
                    // "/" diagonal
                    let py2 = cy + size - t * size * 2.0;
                    quads.push(QuadInstance {
                        rect: [px, py2, thickness, thickness],
                        color: red, color_bottom: red,
                        border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                        shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                        _padding: [0.0; 3],
                    });
                }
            }
            MarkerKind::Out => {
                let col = [0.85, 0.45, 0.45, 0.7];
                // Light red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: col, color_bottom: col,
                    border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                    shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                    _padding: [0.0; 3],
                });
                // Two parallel oblique bars crossing the vertical bar (stepped quads)
                let bar_h = zone.height * 0.3;
                let cy = zone.y + zone.height / 2.0;
                let steps = 6;
                let thickness = 2.0;
                for offset in &[-5.0_f32, 5.0] {
                    for i in 0..steps {
                        let t = i as f32 / steps as f32;
                        let px = x + offset - bar_h * 0.15 + t * bar_h * 0.3;
                        let py = cy - bar_h / 2.0 + t * bar_h;
                        quads.push(QuadInstance {
                            rect: [px, py, thickness, thickness],
                            color: col, color_bottom: col,
                            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                            _padding: [0.0; 3],
                        });
                    }
                }
                // "out" text
                labels.push(LabelInfo {
                    text: "out",
                    bounds: Rect { x: x + 12.0, y: cy - 8.0, width: 30.0, height: 16.0 },
                    h_align: HAlign::Left, v_align: VAlign::Center,
                    overflow: Overflow::Clip, padding: 0.0,
                    font_size_override: Some(10.0), color_override: Some([220, 120, 120]),
                });
            }
            MarkerKind::SceneChange => {
                // White bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: [0.9, 0.9, 0.95, 0.8], color_bottom: [0.9, 0.9, 0.95, 0.8],
                    border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                    shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                    _padding: [0.0; 3],
                });
            }
            MarkerKind::LiaisonLeft => {
                let uv = liaison_left_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
            MarkerKind::LiaisonRight => {
                let uv = liaison_right_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
        }
    }
}

pub fn autocomplete_hit(
    zone: &Rect,
    project: &Project,
    current_frame: i64,
    state: &RythmoState,
    click_x: f32,
    click_y: f32,
) -> Option<(String, [f32; 4])> {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = project.lines.iter().find(|l| l.id == line_id) {
            let br = badge_rect_for_line(line, current_frame, zone);
            let lr = line_rect(line, current_frame, zone);
            let suggestions = project.autocomplete(&line.character_name);
            if !suggestions.is_empty() {
                let dropdown_x = br.x;
                let mut dropdown_y = lr.y + lr.height + 2.0;
                let item_h = 20.0;
                let dropdown_w = 140.0;

                for suggestion in &suggestions {
                    let item_rect = Rect { x: dropdown_x, y: dropdown_y, width: dropdown_w, height: item_h };
                    if item_rect.contains(click_x, click_y) {
                        return Some((suggestion.name.clone(), suggestion.color));
                    }
                    dropdown_y += item_h;
                }
            }
        }
    }
    None
}

/// Context passed to all rythmo sub-handlers.
struct RythmoCtx<'a> {
    zone: &'a Rect,
    project: &'a Project,
    current_frame: i64,
}

pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    current_frame: i64,
    _fps: f64,
    state: &mut RythmoState,
) -> EventResponse {
    let ctx = RythmoCtx { zone, project, current_frame };

    // Autocomplete click has highest priority (before color picker eats it)
    if let UiEvent::MousePress { x, y } = event {
        if let Some((name, color)) = autocomplete_hit(ctx.zone, ctx.project, ctx.current_frame, state, *x, *y) {
            if let Some(line_id) = state.editing_character {
                state.stop_char_editing();
                return EventResponse::Action(UiAction::SetCharacter { line_id, name, color });
            }
        }
    }

    // Color picker overlay
    if state.color_picker.handle_event(event) {
        if let Some(line_id) = state.editing_character {
            return EventResponse::Action(UiAction::SetCharacterColor {
                line_id, color: state.color_picker.current_color(),
            });
        }
        return EventResponse::Consumed;
    }

    match event {
        UiEvent::MouseMove { x, y } => handle_mouse_move(&ctx, state, *x, *y),
        UiEvent::MousePress { x, y } => handle_mouse_press(&ctx, state, *x, *y),
        UiEvent::MouseRelease { .. } => handle_mouse_release(state),
        UiEvent::CtrlClick { x, y } => handle_ctrl_click(&ctx, *x, *y),
        UiEvent::DoubleClick { x, y } => handle_double_click(&ctx, state, *x, *y),
        UiEvent::KeyInput { text } => handle_key_input(&ctx, state, text),
        UiEvent::CursorLeft => handle_cursor_move(&ctx, state, -1),
        UiEvent::CursorRight => handle_cursor_move(&ctx, state, 1),
        UiEvent::CursorUp => handle_autocomplete_nav(&ctx, state, -1),
        UiEvent::CursorDown => handle_autocomplete_nav(&ctx, state, 1),
        _ => EventResponse::Ignored,
    }
}

fn autocomplete_hover_index(ctx: &RythmoCtx, state: &RythmoState, x: f32, y: f32) -> Option<usize> {
    let line_id = state.editing_character?;
    let line = ctx.project.get_line(line_id)?;
    let suggestions = ctx.project.autocomplete(&line.character_name);
    if suggestions.is_empty() { return None; }

    let r = line_rect(line, ctx.current_frame, ctx.zone);
    let br = badge_rect_for_line(line, ctx.current_frame, ctx.zone);
    let dropdown_x = br.x;
    let dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;

    for (i, _) in suggestions.iter().enumerate() {
        let iy = dropdown_y + i as f32 * item_h;
        let item_rect = Rect { x: dropdown_x, y: iy, width: dropdown_w, height: item_h };
        if item_rect.contains(x, y) {
            return Some(i);
        }
    }
    None
}

fn handle_mouse_move(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // Autocomplete hover tracking
    if state.editing_character.is_some() {
        let new_hover = autocomplete_hover_index(ctx, state, x, y);
        if new_hover != state.autocomplete_hover {
            state.autocomplete_hover = new_hover;
            // Also set keyboard index to match mouse for Enter to work
            if new_hover.is_some() {
                state.autocomplete_index = new_hover;
            }
            return EventResponse::Consumed;
        }
    }

    if let Some(drag) = &state.dragging {
        let dx = ((x - drag.drag_start_x) / PIXELS_PER_FRAME) as i64;
        return match drag.handle {
            DragHandle::Left => {
                let end = drag.original_start + drag.original_duration;
                let ns = (drag.original_start + dx).min(end - 1);
                EventResponse::Action(UiAction::ResizeLine { id: drag.line_id, start_frame: ns, duration_frames: end - ns })
            }
            DragHandle::Right => {
                EventResponse::Action(UiAction::ResizeLine { id: drag.line_id, start_frame: drag.original_start, duration_frames: (drag.original_duration + dx).max(1) })
            }
            DragHandle::Body => {
                // Only change track if the target slot is different AND we're past the midpoint
                let candidate = y_to_slot(y, ctx.zone);
                let new_y_slot = if candidate != drag.original_y_slot {
                    let (total_slot_h, _) = slot_metrics(ctx.zone);
                    let orig_slot_idx = (drag.original_y_slot * NUM_SLOTS).round();
                    let orig_center = ctx.zone.y + RULER_HEIGHT + orig_slot_idx * total_slot_h + total_slot_h / 2.0;
                    if (y - orig_center).abs() > total_slot_h * 0.6 {
                        candidate
                    } else {
                        drag.original_y_slot
                    }
                } else {
                    drag.original_y_slot
                };
                EventResponse::Action(UiAction::MoveLine { id: drag.line_id, start_frame: drag.original_start + dx, y_slot: new_y_slot })
            }
        };
    }

    if !ctx.zone.contains(x, y) {
        if state.hovered_line.take().is_some() { return EventResponse::Consumed; }
        return EventResponse::Ignored;
    }

    let found = ctx.project.lines.iter()
        .find(|l| line_rect(l, ctx.current_frame, ctx.zone).contains(x, y))
        .map(|l| l.id);
    if found != state.hovered_line {
        state.hovered_line = found;
        EventResponse::Consumed
    } else {
        EventResponse::Ignored
    }
}

fn handle_mouse_press(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // (autocomplete click already handled before color picker in handle_rythmo_event)

    // Click outside zone while editing char → finalize
    if !ctx.zone.contains(x, y) {
        if let Some(line_id) = state.editing_character {
            state.stop_char_editing();
            return EventResponse::Action(UiAction::FinalizeCharacter { line_id });
        }
        return EventResponse::Ignored;
    }

    for line in &ctx.project.lines {
        let r = line_rect(line, ctx.current_frame, ctx.zone);
        if !r.contains(x, y) { continue; }

        let handle = if x < r.x + HANDLE_WIDTH { DragHandle::Left }
            else if x > r.x + r.width - HANDLE_WIDTH { DragHandle::Right }
            else { DragHandle::Body };

        state.dragging = Some(DragState {
            line_id: line.id, handle, drag_start_x: x,
            original_start: line.start_frame, original_duration: line.duration_frames,
            original_y_slot: line.y_slot, drag_start_y: y,
        });
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

fn handle_mouse_release(state: &mut RythmoState) -> EventResponse {
    if state.dragging.take().is_some() { EventResponse::Consumed } else { EventResponse::Ignored }
}

fn handle_ctrl_click(ctx: &RythmoCtx, x: f32, y: f32) -> EventResponse {
    if !ctx.zone.contains(x, y) { return EventResponse::Ignored; }
    EventResponse::Action(UiAction::CreateLine {
        frame: x_to_frame(x, ctx.current_frame, ctx.zone),
        y_slot: y_to_slot(y, ctx.zone),
    })
}

fn handle_double_click(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // Save current character edit before switching
    let finalize_line_id = state.editing_character;

    // Badge → character editing
    for line in &ctx.project.lines {
        let br = badge_rect_for_line(line, ctx.current_frame, ctx.zone);
        if br.contains(x, y) {
            if let Some(old_id) = finalize_line_id {
                if old_id != line.id {
                    state.stop_char_editing();
                    // Can't dispatch two actions, so finalize happens via FinalizeCharacter below
                }
            }
            state.editing_character = Some(line.id);
            state.char_input.activate(&line.character_name);
            let lr = line_rect(line, ctx.current_frame, ctx.zone);
            state.color_picker.open(lr.x + lr.width + 10.0, lr.y - 30.0, line.character_color);
            state.stop_line_editing();
            return if let Some(old_id) = finalize_line_id.filter(|&id| id != line.id) {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Line body → text editing
    for line in &ctx.project.lines {
        let r = line_rect(line, ctx.current_frame, ctx.zone);
        if r.contains(x, y) {
            state.editing_line = Some(line.id);
            state.line_input.activate(&line.text);
            state.stop_char_editing();
            return if let Some(old_id) = finalize_line_id {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Click empty → stop editing
    if let Some(old_id) = finalize_line_id {
        state.stop_char_editing();
        return EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id });
    }
    if state.editing_line.is_some() {
        state.stop_line_editing();
        return EventResponse::Action(UiAction::StopEditing);
    }
    EventResponse::Ignored
}

fn handle_key_input(ctx: &RythmoCtx, state: &mut RythmoState, text: &str) -> EventResponse {
    use super::text_input::TextInputAction;

    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            // Enter with autocomplete selected → confirm suggestion
            if (text == "\r" || text == "\n") && state.autocomplete_index.is_some() {
                let suggestions = ctx.project.autocomplete(&line.character_name);
                if let Some(idx) = state.autocomplete_index {
                    if let Some(suggestion) = suggestions.get(idx) {
                        let name = suggestion.name.clone();
                        let color = suggestion.color;
                        state.stop_char_editing();
                        return EventResponse::Action(UiAction::SetCharacter { line_id, name, color });
                    }
                }
            }

            match state.char_input.handle_key(text, &line.character_name) {
                Some(TextInputAction::Changed(name)) => {
                    state.autocomplete_index = None; // reset on text change
                    return EventResponse::Action(UiAction::UpdateCharacterName { line_id, name });
                }
                Some(TextInputAction::Finished) => {
                    let name = line.character_name.clone();
                    let color = state.color_picker.current_color();
                    state.stop_char_editing();
                    return if !name.is_empty() {
                        EventResponse::Action(UiAction::SetCharacter { line_id, name, color })
                    } else {
                        EventResponse::Action(UiAction::StopEditing)
                    };
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }

    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            match state.line_input.handle_key(text, &line.text) {
                Some(TextInputAction::Changed(new_text)) =>
                    return EventResponse::Action(UiAction::UpdateLineText { id: line_id, text: new_text }),
                Some(TextInputAction::Finished) => {
                    state.stop_line_editing();
                    return EventResponse::Action(UiAction::StopEditing);
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

fn handle_autocomplete_nav(ctx: &RythmoCtx, state: &mut RythmoState, dir: i32) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            let suggestions = ctx.project.autocomplete(&line.character_name);
            if suggestions.is_empty() { return EventResponse::Ignored; }

            let count = suggestions.len();
            let new_idx = match state.autocomplete_index {
                Some(idx) => {
                    let next = idx as i32 + dir;
                    if next < 0 { None } else { Some((next as usize).min(count - 1)) }
                }
                None => {
                    if dir > 0 { Some(0) } else { None }
                }
            };
            state.autocomplete_index = new_idx;
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}

fn handle_cursor_move(ctx: &RythmoCtx, state: &mut RythmoState, dir: i32) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 { state.char_input.move_left(); }
            else { state.char_input.move_right(&line.character_name); }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 { state.line_input.move_left(); }
            else { state.line_input.move_right(&line.text); }
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}
