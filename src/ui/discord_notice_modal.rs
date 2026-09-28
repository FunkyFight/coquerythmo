use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 560.0;
const PADDING_X: f32 = 30.0;
const BODY_TOP: f32 = 72.0;
const BODY_FONT: f32 = 14.0;
const LINE_H: f32 = 20.0;
const PARAGRAPH_GAP: f32 = 10.0;
const BUTTON_W: f32 = 200.0;
const BUTTON_H: f32 = 42.0;
const BUTTON_GAP: f32 = 12.0;
const FOOTER_H: f32 = 20.0 + BUTTON_H + 24.0;

/// Discord brand "blurple", used for the join button.
const DISCORD_COLOR: [f32; 4] = [0.345, 0.396, 0.949, 1.0];

const PARAGRAPHS: [(&str, [u8; 3]); 3] = [
    ("discord_notice.creator", [222, 222, 232]),
    ("discord_notice.community", [222, 222, 232]),
    ("discord_notice.priority", [248, 211, 99]),
];

/// Shown before opening the Discord invite: sets expectations about support.
pub struct DiscordNoticeModal {
    focused: usize,
}

pub enum DiscordNoticeResult {
    Consumed,
    Join,
    Cancel,
}

impl Default for DiscordNoticeModal {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordNoticeModal {
    pub fn new() -> Self {
        Self { focused: 0 }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focused {
            0 => t("discord_notice.join").to_string(),
            _ => t("discord_notice.cancel").to_string(),
        }
    }

    pub fn accessibility_label() -> String {
        PARAGRAPHS
            .iter()
            .map(|&(key, _)| t(key))
            .chain([t("discord_notice.join")])
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn wrapped_paragraphs() -> Vec<(Vec<&'static str>, [u8; 3])> {
        PARAGRAPHS
            .iter()
            .map(|&(key, color)| {
                (
                    super::pricing_page::wrap_text(t(key), CARD_W - PADDING_X * 2.0, BODY_FONT),
                    color,
                )
            })
            .collect()
    }

    fn body_height(paragraphs: &[(Vec<&str>, [u8; 3])]) -> f32 {
        let lines: usize = paragraphs.iter().map(|(lines, _)| lines.len()).sum();
        lines as f32 * LINE_H + paragraphs.len().saturating_sub(1) as f32 * PARAGRAPH_GAP
    }

    fn card_rect(sw: f32, sh: f32, body_h: f32) -> Rect {
        let height = BODY_TOP + body_h + FOOTER_H;
        Rect {
            x: (sw - CARD_W) / 2.0,
            y: (sh - height) / 2.0,
            width: CARD_W,
            height,
        }
    }

    fn button_rects(card: Rect) -> [Rect; 2] {
        let total_w = BUTTON_W * 2.0 + BUTTON_GAP;
        let start_x = card.x + (card.width - total_w) / 2.0;
        let y = card.y + card.height - BUTTON_H - 24.0;
        [0.0, 1.0].map(|index| Rect {
            x: start_x + index * (BUTTON_W + BUTTON_GAP),
            y,
            width: BUTTON_W,
            height: BUTTON_H,
        })
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> DiscordNoticeResult {
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => DiscordNoticeResult::Cancel,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.focused = 1 - self.focused;
                DiscordNoticeResult::Consumed
            }
            UiEvent::CursorLeft
            | UiEvent::CursorRight
            | UiEvent::CursorUp
            | UiEvent::CursorDown => {
                self.focused = 1 - self.focused;
                DiscordNoticeResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => match self.focused {
                0 => DiscordNoticeResult::Join,
                _ => DiscordNoticeResult::Cancel,
            },
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let card = Self::card_rect(sw, sh, Self::body_height(&Self::wrapped_paragraphs()));
                if !card.contains(*x, *y) {
                    return DiscordNoticeResult::Cancel;
                }
                let [join_btn, cancel_btn] = Self::button_rects(card);
                if join_btn.contains(*x, *y) {
                    return DiscordNoticeResult::Join;
                }
                if cancel_btn.contains(*x, *y) {
                    return DiscordNoticeResult::Cancel;
                }
                DiscordNoticeResult::Consumed
            }
            _ => DiscordNoticeResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let paragraphs = Self::wrapped_paragraphs();
        let card = Self::card_rect(sw, sh, Self::body_height(&paragraphs));

        // Dim background
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.015, 0.015, 0.025, 0.82],
            color_bottom: [0.015, 0.015, 0.025, 0.82],
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
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.16, 0.16, 0.20, 1.0],
            color_bottom: [0.105, 0.105, 0.14, 1.0],
            border_color: [0.38, 0.38, 0.48, 0.9],
            border_width: 1.0,
            border_radius: 16.0,
            shadow_offset: [0.0, 8.0],
            shadow_color: [0.0, 0.0, 0.0, 0.58],
            shadow_blur: 18.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Title
        labels.push(LabelInfo {
            text: t("discord_notice.title"),
            bounds: Rect {
                x: card.x + PADDING_X,
                y: card.y + 24.0,
                width: card.width - PADDING_X * 2.0,
                height: 32.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(20.0),
            color_override: Some([140, 152, 255]),
            font_family_override: None,
        });

        // Body
        let mut y = card.y + BODY_TOP;
        for (lines, color) in paragraphs {
            for line in lines {
                labels.push(LabelInfo {
                    text: line,
                    bounds: Rect {
                        x: card.x + PADDING_X,
                        y,
                        width: card.width - PADDING_X * 2.0,
                        height: LINE_H,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(BODY_FONT),
                    color_override: Some(color),
                    font_family_override: None,
                });
                y += LINE_H;
            }
            y += PARAGRAPH_GAP;
        }

        // Buttons
        let button_rects = Self::button_rects(card);
        let buttons = [
            (t("discord_notice.join"), DISCORD_COLOR),
            (t("discord_notice.cancel"), [0.25, 0.25, 0.32, 1.0]),
        ];

        for (index, (button, (label, color))) in button_rects.into_iter().zip(buttons).enumerate() {
            let focused = self.focused == index;
            quads.push(QuadInstance {
                rect: [button.x, button.y, button.width, button.height],
                color,
                color_bottom: [color[0] * 0.82, color[1] * 0.82, color[2] * 0.82, color[3]],
                border_color: if focused {
                    [0.30, 0.62, 1.0, 1.0]
                } else {
                    [0.62, 0.62, 0.70, 0.45]
                },
                border_width: if focused { 2.5 } else { 1.0 },
                border_radius: 8.0,
                shadow_offset: [0.0, 2.0],
                shadow_color: [0.0, 0.0, 0.0, 0.28],
                shadow_blur: 4.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text: label,
                bounds: button,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(14.0),
                color_override: Some([248, 248, 252]),
                font_family_override: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_joins_by_default_and_escape_cancels() {
        let mut modal = DiscordNoticeModal::new();
        let enter = UiEvent::KeyInput {
            text: "\r".to_string(),
        };
        assert!(matches!(
            modal.handle_event(&enter, 800.0, 600.0),
            DiscordNoticeResult::Join
        ));
        assert!(matches!(
            modal.handle_event(
                &UiEvent::KeyInput {
                    text: "\x1b".to_string(),
                },
                800.0,
                600.0,
            ),
            DiscordNoticeResult::Cancel
        ));
    }

    #[test]
    fn arrows_move_focus_to_cancel() {
        let mut modal = DiscordNoticeModal::new();
        modal.handle_event(&UiEvent::CursorRight, 800.0, 600.0);
        assert!(matches!(
            modal.handle_event(
                &UiEvent::KeyInput {
                    text: "\r".to_string(),
                },
                800.0,
                600.0,
            ),
            DiscordNoticeResult::Cancel
        ));
    }
}
