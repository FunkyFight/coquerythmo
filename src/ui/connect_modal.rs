#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;

use crate::i18n::t;

const FIELD_FONT_SIZE: f32 = 13.0;
const FIELD_PADDING_X: f32 = 8.0;

pub struct ConnectModal {
    pub join: bool,
    pub ip: String,
    pub port: u16,
    pub fields: [String; 3], // password, username, room_code
    pub input: text_input::TextInputState,
    pub focused: usize,
}

impl ConnectModal {
    pub const PASSWORD: usize = 0;
    pub const USERNAME: usize = 1;
    pub const ROOM_CODE: usize = 2;

    pub fn new_with_server(ip: &str, port: u16, join: bool) -> Self {
        let cfg = crate::config::get();
        let net = &cfg.network;
        let mut modal = Self {
            join,
            ip: ip.to_string(),
            port,
            fields: [net.password.clone(), net.username.clone(), String::new()],
            input: text_input::TextInputState::new(),
            focused: 0,
        };
        modal.input.activate(&modal.fields[0]);
        modal
    }

    pub fn field_count(&self) -> usize {
        if self.join {
            3
        } else {
            2
        }
    }

    pub fn field_label(&self, i: usize) -> &str {
        match i {
            0 => "Mot de passe",
            1 => "Pseudo",
            2 => "Code du salon",
            _ => "",
        }
    }

    pub fn focus_next(&mut self) {
        self.focused = (self.focused + 1) % self.field_count();
        self.input.activate(&self.fields[self.focused]);
    }

    pub fn focus_prev(&mut self) {
        self.focused = if self.focused == 0 {
            self.field_count() - 1
        } else {
            self.focused - 1
        };
        self.input.activate(&self.fields[self.focused]);
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ConnectModalResult {
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\u{b}" {
                    self.focus_prev();
                    return ConnectModalResult::Consumed;
                }
                if text == "\x1b" {
                    return ConnectModalResult::Close;
                }
                if text == "\t" {
                    self.focus_next();
                    return ConnectModalResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    let password = self.fields[Self::PASSWORD].clone();
                    let username = self.fields[Self::USERNAME].trim().to_string();
                    let room_code = if self.join {
                        let c = self.fields[Self::ROOM_CODE].trim().to_uppercase();
                        if c.is_empty() {
                            return ConnectModalResult::Consumed;
                        }
                        Some(c)
                    } else {
                        None
                    };
                    if username.is_empty() {
                        return ConnectModalResult::Consumed;
                    }
                    return ConnectModalResult::Connect {
                        ip: self.ip.clone(),
                        port: self.port,
                        password,
                        username,
                        room_code,
                    };
                }
                let focused = self.focused;
                if let Some(action) = self.input.handle_key(text, &self.fields[focused]) {
                    if let text_input::TextInputAction::Changed(new_text) = action {
                        self.fields[focused] = new_text;
                    }
                }
                ConnectModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                ConnectModalResult::Consumed
            }
            UiEvent::CursorRight => {
                let f = self.focused;
                self.input.move_right(&self.fields[f]);
                ConnectModalResult::Consumed
            }
            UiEvent::CursorUp => {
                self.focus_prev();
                ConnectModalResult::Consumed
            }
            UiEvent::CursorDown => {
                self.focus_next();
                ConnectModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let double = matches!(event, UiEvent::DoubleClick { .. });
                let field_count = self.field_count();
                let label_h = 16.0;
                let field_h = 28.0;
                let field_gap = 8.0;
                let row_h = label_h + field_h + field_gap;
                let dw = 380.0;
                let dh = 60.0 + row_h * field_count as f32 + 10.0;
                let dx = (screen_w - dw) / 2.0;
                let dy = (screen_h - dh) / 2.0;
                let fx = dx + 24.0;
                let fw = dw - 48.0;
                let base_y = dy + 56.0;

                let mut hit = false;
                for i in 0..field_count {
                    let fy = base_y + i as f32 * row_h + label_h;
                    let field_rect = Rect {
                        x: fx,
                        y: fy,
                        width: fw,
                        height: field_h,
                    };
                    if field_rect.contains(*x, *y) {
                        self.focused = i;
                        self.input.activate(&self.fields[i]);
                        if double {
                            self.input.select_all(&self.fields[i]);
                        } else {
                            let pos = text_input::cursor_pos_from_x(
                                &self.fields[i],
                                field_rect,
                                *x,
                                field_metrics(),
                            );
                            self.input.set_cursor_pos(pos);
                        }
                        hit = true;
                        break;
                    }
                }
                if !hit {
                    let card = Rect {
                        x: dx,
                        y: dy,
                        width: dw,
                        height: dh,
                    };
                    if !card.contains(*x, *y) {
                        return ConnectModalResult::Close;
                    }
                }
                ConnectModalResult::Consumed
            }
            _ => ConnectModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let field_count = self.field_count();
        let field_h = 28.0;
        let field_gap = 8.0;
        let label_h = 16.0;
        let row_h = label_h + field_h + field_gap;
        let dw = 380.0;
        let dh = 60.0 + row_h * field_count as f32 + 10.0;
        let dx = (screen_w - dw) / 2.0;
        let dy = (screen_h - dh) / 2.0;

        // Dim
        overlay_quads.push(QuadInstance {
            rect: [0.0, 0.0, screen_w, screen_h],
            color: [0.0, 0.0, 0.0, 0.75],
            color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Card
        overlay_quads.push(QuadInstance {
            rect: [dx, dy, dw, dh],
            color: [0.22, 0.22, 0.26, 1.0],
            color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Title
        let title = if self.join {
            t("menu.connect.join_room")
        } else {
            t("menu.connect.create_room")
        };
        labels.push(LabelInfo {
            text: title,
            bounds: Rect {
                x: dx,
                y: dy + 8.0,
                width: dw,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(15.0),
            color_override: None,
            font_family_override: None,
        });
        // Server info subtitle
        labels.push(LabelInfo {
            text: &self.ip,
            bounds: Rect {
                x: dx,
                y: dy + 30.0,
                width: dw,
                height: 18.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(10.0),
            color_override: Some([130, 130, 145]),
            font_family_override: None,
        });

        // Fields
        let fx = dx + 24.0;
        let fw = dw - 48.0;
        let base_y = dy + 56.0;
        for i in 0..field_count {
            let fy = base_y + i as f32 * row_h;
            let is_focused = self.focused == i;

            labels.push(LabelInfo {
                text: self.field_label(i),
                bounds: Rect {
                    x: fx,
                    y: fy,
                    width: fw,
                    height: label_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some(if is_focused {
                    [200, 200, 220]
                } else {
                    [140, 140, 155]
                }),
                font_family_override: None,
            });

            let iy = fy + label_h;
            let border = if is_focused {
                [0.40, 0.37, 0.80, 0.8]
            } else {
                [0.30, 0.30, 0.36, 0.5]
            };
            overlay_quads.push(QuadInstance {
                rect: [fx, iy, fw, field_h],
                color: [0.08, 0.08, 0.10, 1.0],
                color_bottom: [0.08, 0.08, 0.10, 1.0],
                border_color: border,
                border_width: 1.0,
                border_radius: 4.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            if !self.fields[i].is_empty() {
                labels.push(LabelInfo {
                    text: &self.fields[i],
                    bounds: Rect {
                        x: fx,
                        y: iy,
                        width: fw,
                        height: field_h,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: FIELD_PADDING_X,
                    font_size_override: Some(FIELD_FONT_SIZE),
                    color_override: None,
                    font_family_override: None,
                });
            }

            text_input::render_selection_and_cursor(
                overlay_quads,
                Rect {
                    x: fx,
                    y: iy,
                    width: fw,
                    height: field_h,
                },
                &self.fields[i],
                &self.input,
                is_focused,
                field_metrics(),
                4.0,
                4.0,
                [0.25, 0.45, 0.95, 0.42],
                [0.90, 0.90, 0.96, 1.0],
            );
        }
    }
}

fn field_metrics() -> text_input::TextInputMetrics {
    text_input::TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

pub enum ConnectModalResult {
    Consumed,
    Close,
    Connect {
        ip: String,
        port: u16,
        password: String,
        username: String,
        room_code: Option<String>,
    },
}
