#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;

use crate::i18n::t;

const CARD_W: f32 = 620.0;
const CARD_PADDING: f32 = 32.0;
const FIELD_FONT_SIZE: f32 = 15.0;
const FIELD_PADDING_X: f32 = 12.0;
const FIELD_H: f32 = 44.0;
const FIELD_ROW_H: f32 = 76.0;
const FIELDS_Y: f32 = 164.0;
const BUTTON_H: f32 = 44.0;

pub struct ConnectModal {
    pub join: bool,
    pub ip: String,
    pub port: u16,
    pub fields: [String; 3], // password, username, room_code
    pub input: text_input::TextInputState,
    pub focused: usize,
    endpoint: String,
    masked_password: String,
    username_only: bool,
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
            endpoint: format!("{ip}:{port}"),
            masked_password: "•".repeat(net.password.chars().count()),
            username_only: false,
        };
        modal.input.activate(&modal.fields[0]);
        modal
    }

    /// Variant for `coquerythmo://` quick-join links: joins `ip` directly and
    /// pre-fills the room code so the user only has to type a username. The
    /// password and room code remain in the modal state but are not exposed.
    pub fn new_with_room(ip: &str, port: u16, room_code: &str, password: &str) -> Self {
        let mut modal = Self {
            join: true,
            ip: ip.to_string(),
            port,
            fields: [
                password.to_string(),
                String::new(),
                room_code.trim().to_uppercase(),
            ],
            input: text_input::TextInputState::new(),
            focused: 0,
            endpoint: format!("{ip}:{port}"),
            masked_password: "•".repeat(password.chars().count()),
            username_only: true,
        };
        modal.input.activate(&modal.fields[Self::USERNAME]);
        modal
    }

    pub fn field_count(&self) -> usize {
        if self.username_only {
            1
        } else if self.join {
            3
        } else {
            2
        }
    }

    fn field_index(&self, visible_index: usize) -> usize {
        if self.username_only {
            Self::USERNAME
        } else {
            visible_index
        }
    }

    fn cancel_focus(&self) -> usize {
        self.field_count()
    }

    fn submit_focus(&self) -> usize {
        self.field_count() + 1
    }

    fn focus_count(&self) -> usize {
        self.field_count() + 2
    }

    pub fn field_label(&self, i: usize) -> &str {
        match self.field_index(i) {
            Self::PASSWORD => t("connect.password"),
            Self::USERNAME => t("connect.username"),
            Self::ROOM_CODE => t("connect.room_code"),
            _ => "",
        }
    }

    fn field_placeholder(&self, i: usize) -> &str {
        match self.field_index(i) {
            Self::PASSWORD => t("connect.password_placeholder"),
            Self::USERNAME => t("connect.username_placeholder"),
            Self::ROOM_CODE => t("connect.room_code_placeholder"),
            _ => "",
        }
    }

    fn submit_label(&self) -> &str {
        if self.join {
            t("connect.join")
        } else {
            t("connect.create")
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        if self.focused < self.field_count() {
            self.field_label(self.focused).to_string()
        } else if self.focused == self.cancel_focus() {
            t("connect.cancel").to_string()
        } else {
            self.submit_label().to_string()
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        if self.focused < self.field_count() {
            "text field"
        } else {
            "button"
        }
    }

    fn set_focus(&mut self, focus: usize) {
        self.focused = focus % self.focus_count();
        if self.focused < self.field_count() {
            let field = self.field_index(self.focused);
            self.input.activate(&self.fields[field]);
        } else {
            self.input.deactivate();
        }
    }

    pub fn focus_next(&mut self) {
        self.set_focus(self.focused + 1);
    }

    pub fn focus_prev(&mut self) {
        self.set_focus(
            self.focused
                .checked_sub(1)
                .unwrap_or(self.focus_count() - 1),
        );
    }

    fn can_submit(&self) -> bool {
        !self.fields[Self::USERNAME].trim().is_empty()
            && (!self.join || !self.fields[Self::ROOM_CODE].trim().is_empty())
    }

    fn connect_result(&self) -> ConnectModalResult {
        if !self.can_submit() {
            return ConnectModalResult::Consumed;
        }
        ConnectModalResult::Connect {
            ip: self.ip.clone(),
            port: self.port,
            password: self.fields[Self::PASSWORD].clone(),
            username: self.fields[Self::USERNAME].trim().to_string(),
            room_code: self
                .join
                .then(|| self.fields[Self::ROOM_CODE].trim().to_uppercase()),
        }
    }

    fn card_height(&self) -> f32 {
        270.0 + self.field_count() as f32 * FIELD_ROW_H
    }

    fn card_rect(&self, screen_w: f32, screen_h: f32) -> Rect {
        let height = self.card_height();
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - height) / 2.0,
            width: CARD_W,
            height,
        }
    }

    fn field_rect(&self, card: Rect, index: usize) -> Rect {
        Rect {
            x: card.x + CARD_PADDING,
            y: card.y + FIELDS_Y + index as f32 * FIELD_ROW_H + 22.0,
            width: card.width - CARD_PADDING * 2.0,
            height: FIELD_H,
        }
    }

    fn button_rects(&self, card: Rect) -> (Rect, Rect) {
        let submit = Rect {
            x: card.x + card.width - CARD_PADDING - 210.0,
            y: card.y + card.height - 70.0,
            width: 210.0,
            height: BUTTON_H,
        };
        let cancel = Rect {
            x: submit.x - 132.0,
            y: submit.y,
            width: 120.0,
            height: BUTTON_H,
        };
        (cancel, submit)
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ConnectModalResult {
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return ConnectModalResult::Close;
                }
                if text == "\t" {
                    self.focus_next();
                    return ConnectModalResult::Consumed;
                }
                if text == "\u{b}" {
                    self.focus_prev();
                    return ConnectModalResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    return if self.focused == self.cancel_focus() {
                        ConnectModalResult::Close
                    } else {
                        self.connect_result()
                    };
                }
                if text == " " && self.focused >= self.field_count() {
                    return if self.focused == self.cancel_focus() {
                        ConnectModalResult::Close
                    } else {
                        self.connect_result()
                    };
                }
                if self.focused < self.field_count() {
                    let focused = self.focused;
                    let field = self.field_index(focused);
                    if let Some(text_input::TextInputAction::Changed(new_text)) =
                        self.input.handle_key(text, &self.fields[field])
                    {
                        if field == Self::PASSWORD {
                            self.masked_password = "•".repeat(new_text.chars().count());
                        }
                        self.fields[field] = new_text;
                    }
                }
                ConnectModalResult::Consumed
            }
            UiEvent::FocusNext => {
                self.focus_next();
                ConnectModalResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.focus_prev();
                ConnectModalResult::Consumed
            }
            UiEvent::CursorLeft if self.focused < self.field_count() => {
                self.input.move_left();
                ConnectModalResult::Consumed
            }
            UiEvent::CursorRight if self.focused < self.field_count() => {
                let field = self.field_index(self.focused);
                self.input.move_right(&self.fields[field]);
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
                let card = self.card_rect(screen_w, screen_h);
                if !card.contains(*x, *y) {
                    return ConnectModalResult::Close;
                }

                for i in 0..self.field_count() {
                    let field = self.field_rect(card, i);
                    if field.contains(*x, *y) {
                        let field_index = self.field_index(i);
                        self.set_focus(i);
                        if double {
                            self.input.select_all(&self.fields[field_index]);
                        } else {
                            let display_value = if field_index == Self::PASSWORD {
                                &self.masked_password
                            } else {
                                &self.fields[field_index]
                            };
                            self.input.set_cursor_pos(text_input::cursor_pos_from_x(
                                display_value,
                                field,
                                *x,
                                field_metrics(),
                            ));
                        }
                        return ConnectModalResult::Consumed;
                    }
                }

                let (cancel, submit) = self.button_rects(card);
                if cancel.contains(*x, *y) {
                    self.set_focus(self.cancel_focus());
                    return ConnectModalResult::Close;
                }
                if submit.contains(*x, *y) {
                    self.set_focus(self.submit_focus());
                    return self.connect_result();
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
        let card = self.card_rect(screen_w, screen_h);
        overlay_quads.push(quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.78],
            [0.0; 4],
            0.0,
            0.0,
        ));
        overlay_quads.push(card_quad(card));

        labels.push(label(
            if self.join {
                t("menu.connect.join_room")
            } else {
                t("menu.connect.create_room")
            },
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 18.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 32.0,
            },
            HAlign::Left,
            22.0,
            None,
        ));
        labels.push(label(
            if self.join {
                t("connect.join_description")
            } else {
                t("connect.create_description")
            },
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + 52.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 22.0,
            },
            HAlign::Left,
            12.0,
            Some([174, 174, 190]),
        ));

        let server = Rect {
            x: card.x + CARD_PADDING,
            y: card.y + 88.0,
            width: card.width - CARD_PADDING * 2.0,
            height: 54.0,
        };
        overlay_quads.push(quad(
            server,
            [0.11, 0.11, 0.15, 1.0],
            [0.31, 0.31, 0.40, 0.9],
            1.0,
            8.0,
        ));
        labels.push(label(
            t("connect.server"),
            Rect {
                x: server.x + 16.0,
                y: server.y + 6.0,
                width: server.width - 32.0,
                height: 18.0,
            },
            HAlign::Left,
            11.0,
            Some([150, 150, 168]),
        ));
        labels.push(label(
            &self.endpoint,
            Rect {
                x: server.x + 16.0,
                y: server.y + 24.0,
                width: server.width - 32.0,
                height: 22.0,
            },
            HAlign::Left,
            14.0,
            Some([230, 230, 239]),
        ));

        for i in 0..self.field_count() {
            let field = self.field_rect(card, i);
            let focused = self.focused == i;
            let field_index = self.field_index(i);
            labels.push(label(
                self.field_label(i),
                Rect {
                    x: field.x,
                    y: field.y - 22.0,
                    width: field.width,
                    height: 20.0,
                },
                HAlign::Left,
                13.0,
                Some(if focused {
                    [210, 218, 244]
                } else {
                    [180, 180, 198]
                }),
            ));
            overlay_quads.push(input_quad(field, focused));

            let display_value = if field_index == Self::PASSWORD {
                &self.masked_password
            } else {
                &self.fields[field_index]
            };
            labels.push(LabelInfo {
                text: if display_value.is_empty() {
                    self.field_placeholder(i)
                } else {
                    display_value
                },
                bounds: field,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: FIELD_PADDING_X,
                font_size_override: Some(FIELD_FONT_SIZE),
                color_override: Some(if display_value.is_empty() {
                    [105, 105, 122]
                } else {
                    [232, 232, 240]
                }),
                font_family_override: None,
            });
            text_input::render_selection_and_cursor(
                overlay_quads,
                field,
                display_value,
                &self.input,
                focused,
                field_metrics(),
                6.0,
                6.0,
                [0.25, 0.45, 0.95, 0.42],
                [0.90, 0.90, 0.96, 1.0],
            );
        }

        labels.push(label(
            t("connect.required"),
            Rect {
                x: card.x + CARD_PADDING,
                y: card.y + FIELDS_Y + self.field_count() as f32 * FIELD_ROW_H - 4.0,
                width: card.width - CARD_PADDING * 2.0,
                height: 18.0,
            },
            HAlign::Left,
            11.0,
            Some([140, 140, 158]),
        ));

        let (cancel, submit) = self.button_rects(card);
        render_button(
            overlay_quads,
            labels,
            cancel,
            t("connect.cancel"),
            false,
            true,
            self.focused == self.cancel_focus(),
        );
        render_button(
            overlay_quads,
            labels,
            submit,
            self.submit_label(),
            true,
            self.can_submit(),
            self.focused == self.submit_focus(),
        );
    }
}

fn field_metrics() -> text_input::TextInputMetrics {
    text_input::TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

fn label(
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: Option<[u8; 3]>,
) -> LabelInfo<'_> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: color,
        font_family_override: None,
    }
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    radius: f32,
) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn card_quad(card: Rect) -> QuadInstance {
    QuadInstance {
        rect: [card.x, card.y, card.width, card.height],
        color: [0.19, 0.19, 0.24, 1.0],
        color_bottom: [0.13, 0.13, 0.17, 1.0],
        border_color: [0.43, 0.43, 0.53, 0.9],
        border_width: 1.5,
        border_radius: 16.0,
        shadow_offset: [0.0, 8.0],
        shadow_color: [0.0, 0.0, 0.0, 0.58],
        shadow_blur: 18.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn input_quad(rect: Rect, focused: bool) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.09, 0.09, 0.12, 1.0],
        color_bottom: [0.07, 0.07, 0.10, 1.0],
        border_color: if focused {
            [0.45, 0.62, 0.95, 1.0]
        } else {
            [0.31, 0.31, 0.39, 0.9]
        },
        border_width: if focused { 2.0 } else { 1.0 },
        border_radius: 8.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn render_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    primary: bool,
    enabled: bool,
    focused: bool,
) {
    let (top, bottom) = if !enabled {
        ([0.16, 0.16, 0.19, 1.0], [0.13, 0.13, 0.16, 1.0])
    } else if primary {
        ([0.36, 0.49, 0.86, 1.0], [0.27, 0.37, 0.73, 1.0])
    } else {
        ([0.28, 0.28, 0.34, 1.0], [0.21, 0.21, 0.26, 1.0])
    };
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: top,
        color_bottom: bottom,
        border_color: if focused {
            [0.55, 0.70, 1.0, 1.0]
        } else {
            [0.50, 0.50, 0.60, if enabled { 0.7 } else { 0.25 }]
        },
        border_width: if focused { 2.0 } else { 1.0 },
        border_radius: 9.0,
        shadow_offset: [0.0, 3.0],
        shadow_color: [0.0, 0.0, 0.0, if enabled { 0.28 } else { 0.0 }],
        shadow_blur: 6.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    labels.push(label(
        text,
        rect,
        HAlign::Center,
        13.0,
        Some(if enabled {
            [238, 238, 246]
        } else {
            [105, 105, 120]
        }),
    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn modal(join: bool) -> ConnectModal {
        ConnectModal {
            join,
            ip: "127.0.0.1".into(),
            port: 9050,
            fields: [String::new(), String::new(), String::new()],
            input: text_input::TextInputState::new(),
            focused: 0,
            endpoint: "127.0.0.1:9050".into(),
            masked_password: String::new(),
            username_only: false,
        }
    }

    #[test]
    fn required_connection_fields_control_submission() {
        let mut create = modal(false);
        assert!(!create.can_submit());
        create.fields[ConnectModal::USERNAME] = "Alex".into();
        assert!(create.can_submit());

        let mut join = modal(true);
        join.fields[ConnectModal::USERNAME] = "Alex".into();
        assert!(!join.can_submit());
        join.fields[ConnectModal::ROOM_CODE] = "abcd".into();
        assert!(join.can_submit());
    }

    #[test]
    fn invitation_mode_exposes_only_the_username_field() {
        let mut invite = modal(true);
        invite.username_only = true;
        invite.fields[ConnectModal::ROOM_CODE] = "ABCD".into();

        assert_eq!(invite.field_count(), 1);
        assert_eq!(invite.field_index(0), ConnectModal::USERNAME);
        assert!(!invite.can_submit());

        invite.fields[ConnectModal::USERNAME] = "Invitee".into();
        assert!(invite.can_submit());
    }
}
