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
const FOOTER_H: f32 = 20.0 + BUTTON_H + 24.0;

const BUTTON_COLOR: [f32; 4] = [0.18, 0.52, 0.32, 1.0];

const PARAGRAPHS: [(&str, [u8; 3]); 3] = [
    ("export_done.message", [222, 222, 232]),
    ("export_done.free", [222, 222, 232]),
    ("export_done.credit", [248, 211, 99]),
];

/// Shown when an export finishes: confirms success and asks users to credit
/// the software in the media it helped produce.
pub struct ExportDoneModal;

pub enum ExportDoneResult {
    Consumed,
    Close,
}

impl Default for ExportDoneModal {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportDoneModal {
    pub fn new() -> Self {
        Self
    }

    pub fn keyboard_focus_label(&self) -> String {
        t("export_done.close").to_string()
    }

    pub fn accessibility_label() -> String {
        PARAGRAPHS
            .iter()
            .map(|&(key, _)| t(key))
            .chain([t("export_done.close")])
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

    fn button_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + (card.width - BUTTON_W) / 2.0,
            y: card.y + card.height - BUTTON_H - 24.0,
            width: BUTTON_W,
            height: BUTTON_H,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> ExportDoneResult {
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" || text == "\r" || text == "\n" => {
                ExportDoneResult::Close
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let card = Self::card_rect(sw, sh, Self::body_height(&Self::wrapped_paragraphs()));
                if !card.contains(*x, *y) || Self::button_rect(card).contains(*x, *y) {
                    return ExportDoneResult::Close;
                }
                ExportDoneResult::Consumed
            }
            _ => ExportDoneResult::Consumed,
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
            text: t("export_done.title"),
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
            color_override: Some([120, 218, 150]),
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

        // Button (always focused: it is the only one)
        let button = Self::button_rect(card);
        quads.push(QuadInstance {
            rect: [button.x, button.y, button.width, button.height],
            color: BUTTON_COLOR,
            color_bottom: [
                BUTTON_COLOR[0] * 0.82,
                BUTTON_COLOR[1] * 0.82,
                BUTTON_COLOR[2] * 0.82,
                BUTTON_COLOR[3],
            ],
            border_color: [0.30, 0.62, 1.0, 1.0],
            border_width: 2.5,
            border_radius: 8.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.28],
            shadow_blur: 4.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("export_done.close"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> UiEvent {
        UiEvent::KeyInput {
            text: text.to_string(),
        }
    }

    #[test]
    fn enter_and_escape_close() {
        let mut modal = ExportDoneModal::new();
        assert!(matches!(
            modal.handle_event(&key("\r"), 800.0, 600.0),
            ExportDoneResult::Close
        ));
        assert!(matches!(
            modal.handle_event(&key("\x1b"), 800.0, 600.0),
            ExportDoneResult::Close
        ));
    }

    #[test]
    fn click_inside_card_keeps_it_open_and_outside_closes() {
        let mut modal = ExportDoneModal::new();
        assert!(matches!(
            modal.handle_event(&UiEvent::MousePress { x: 400.0, y: 300.0 }, 800.0, 600.0),
            ExportDoneResult::Consumed
        ));
        assert!(matches!(
            modal.handle_event(&UiEvent::MousePress { x: 5.0, y: 5.0 }, 800.0, 600.0),
            ExportDoneResult::Close
        ));
    }
}
