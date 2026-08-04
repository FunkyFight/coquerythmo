use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 720.0;
const CARD_H: f32 = 360.0;
const PADDING: f32 = 32.0;
const BUTTON_H: f32 = 44.0;

pub struct InvitationModal {
    pub code: String,
    pub link: String,
    focused: usize,
}

pub enum InvitationModalResult {
    Consumed,
    Close,
    CopyLink,
    CopyCode,
}

impl InvitationModal {
    pub fn new(code: impl Into<String>, link: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            link: link.into(),
            focused: 0,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focused {
            0 => t("invite.copy_link").to_string(),
            1 => t("invite.copy_code").to_string(),
            _ => t("connect.cancel").to_string(),
        }
    }

    fn card(&self, screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn buttons(&self, card: Rect) -> [Rect; 3] {
        let y = card.y + card.height - PADDING - BUTTON_H;
        [
            Rect {
                x: card.x + PADDING,
                y,
                width: 300.0,
                height: BUTTON_H,
            },
            Rect {
                x: card.x + PADDING + 312.0,
                y,
                width: 150.0,
                height: BUTTON_H,
            },
            Rect {
                x: card.x + card.width - PADDING - 120.0,
                y,
                width: 120.0,
                height: BUTTON_H,
            },
        ]
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> InvitationModalResult {
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => InvitationModalResult::Close,
            UiEvent::KeyInput { text } if text == "\t" => {
                self.focused = (self.focused + 1) % 3;
                InvitationModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\u{b}" => {
                self.focused = (self.focused + 2) % 3;
                InvitationModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                match self.focused {
                    0 => InvitationModalResult::CopyLink,
                    1 => InvitationModalResult::CopyCode,
                    _ => InvitationModalResult::Close,
                }
            }
            UiEvent::FocusNext | UiEvent::CursorDown => {
                self.focused = (self.focused + 1) % 3;
                InvitationModalResult::Consumed
            }
            UiEvent::FocusPrevious | UiEvent::CursorUp => {
                self.focused = (self.focused + 2) % 3;
                InvitationModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let card = self.card(screen_w, screen_h);
                if !card.contains(*x, *y) {
                    return InvitationModalResult::Close;
                }
                for (index, button) in self.buttons(card).into_iter().enumerate() {
                    if button.contains(*x, *y) {
                        self.focused = index;
                        return match index {
                            0 => InvitationModalResult::CopyLink,
                            1 => InvitationModalResult::CopyCode,
                            _ => InvitationModalResult::Close,
                        };
                    }
                }
                InvitationModalResult::Consumed
            }
            _ => InvitationModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = self.card(screen_w, screen_h);
        quads.push(quad(
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
        quads.push(card_quad(card));
        labels.push(label(
            t("invite.title"),
            Rect {
                x: card.x + PADDING,
                y: card.y + 20.0,
                width: card.width - 2.0 * PADDING,
                height: 32.0,
            },
            HAlign::Left,
            22.0,
            None,
        ));
        labels.push(label(
            t("invite.description"),
            Rect {
                x: card.x + PADDING,
                y: card.y + 58.0,
                width: card.width - 2.0 * PADDING,
                height: 24.0,
            },
            HAlign::Left,
            13.0,
            Some([174, 174, 190]),
        ));
        let code_rect = Rect {
            x: card.x + PADDING,
            y: card.y + 100.0,
            width: card.width - 2.0 * PADDING,
            height: 58.0,
        };
        quads.push(quad(
            code_rect,
            [0.08, 0.08, 0.12, 1.0],
            [0.40, 0.45, 0.70, 0.9],
            1.0,
            8.0,
        ));
        labels.push(label(
            t("invite.room_code"),
            Rect {
                x: code_rect.x + 16.0,
                y: code_rect.y + 5.0,
                width: 140.0,
                height: 18.0,
            },
            HAlign::Left,
            11.0,
            Some([150, 150, 168]),
        ));
        labels.push(label(
            &self.code,
            Rect {
                x: code_rect.x + 16.0,
                y: code_rect.y + 23.0,
                width: code_rect.width - 32.0,
                height: 28.0,
            },
            HAlign::Left,
            20.0,
            Some([238, 240, 255]),
        ));
        let link_rect = Rect {
            x: card.x + PADDING,
            y: card.y + 174.0,
            width: card.width - 2.0 * PADDING,
            height: 54.0,
        };
        labels.push(label(
            &self.link,
            link_rect,
            HAlign::Left,
            12.0,
            Some([195, 195, 212]),
        ));
        let buttons = self.buttons(card);
        render_button(
            quads,
            labels,
            buttons[0],
            t("invite.copy_link"),
            true,
            self.focused == 0,
        );
        render_button(
            quads,
            labels,
            buttons[1],
            t("invite.copy_code"),
            false,
            self.focused == 1,
        );
        render_button(
            quads,
            labels,
            buttons[2],
            t("connect.cancel"),
            false,
            self.focused == 2,
        );
    }
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

fn render_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    primary: bool,
    focused: bool,
) {
    let color = if primary {
        [0.36, 0.49, 0.86, 1.0]
    } else {
        [0.28, 0.28, 0.34, 1.0]
    };
    quads.push(quad(
        rect,
        color,
        if focused {
            [0.75, 0.82, 1.0, 1.0]
        } else {
            [0.50, 0.50, 0.60, 0.7]
        },
        if focused { 2.0 } else { 1.0 },
        9.0,
    ));
    labels.push(label(
        text,
        rect,
        HAlign::Center,
        13.0,
        Some([238, 238, 246]),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_exposes_both_copy_actions() {
        let mut modal = InvitationModal::new("ABCD", "coquerythmo://link/example");
        assert!(matches!(
            modal.handle_event(&UiEvent::KeyInput { text: "\r".into() }, 1280.0, 720.0),
            InvitationModalResult::CopyLink
        ));
        modal.handle_event(&UiEvent::KeyInput { text: "\t".into() }, 1280.0, 720.0);
        assert!(matches!(
            modal.handle_event(&UiEvent::KeyInput { text: "\r".into() }, 1280.0, 720.0),
            InvitationModalResult::CopyCode
        ));
    }
}
