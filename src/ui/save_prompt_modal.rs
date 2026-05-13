use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 420.0;
const CARD_H: f32 = 150.0;

pub struct SavePromptModal;

pub enum SavePromptResult {
    Consumed,
    Save,
    Discard,
    Cancel,
}

impl SavePromptModal {
    pub fn new() -> Self {
        Self
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect {
            x: (sw - CARD_W) / 2.0,
            y: (sh - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> SavePromptResult {
        let card = Self::card_rect(sw, sh);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => SavePromptResult::Cancel,
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return SavePromptResult::Cancel;
                }
                let btn_y = card.y + CARD_H - 44.0;
                let btn_w = 110.0;
                let gap = 10.0;
                let total_w = btn_w * 3.0 + gap * 2.0;
                let start_x = card.x + (CARD_W - total_w) / 2.0;

                let save_btn = Rect {
                    x: start_x,
                    y: btn_y,
                    width: btn_w,
                    height: 30.0,
                };
                let discard_btn = Rect {
                    x: start_x + btn_w + gap,
                    y: btn_y,
                    width: btn_w,
                    height: 30.0,
                };
                let cancel_btn = Rect {
                    x: start_x + 2.0 * (btn_w + gap),
                    y: btn_y,
                    width: btn_w,
                    height: 30.0,
                };

                if save_btn.contains(*x, *y) {
                    return SavePromptResult::Save;
                }
                if discard_btn.contains(*x, *y) {
                    return SavePromptResult::Discard;
                }
                if cancel_btn.contains(*x, *y) {
                    return SavePromptResult::Cancel;
                }
                SavePromptResult::Consumed
            }
            _ => SavePromptResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let card = Self::card_rect(sw, sh);

        // Dim background
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
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
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0],
            color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.55, 0.45, 0.20, 0.8],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Title
        labels.push(LabelInfo {
            text: t("save_prompt.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 14.0,
                width: card.width,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: Some([230, 190, 80]),
            font_family_override: None,
        });

        // Message
        labels.push(LabelInfo {
            text: t("save_prompt.message"),
            bounds: Rect {
                x: card.x + 20.0,
                y: card.y + 46.0,
                width: card.width - 40.0,
                height: 20.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([200, 200, 210]),
            font_family_override: None,
        });

        // Buttons
        let btn_y = card.y + CARD_H - 44.0;
        let btn_w = 110.0;
        let gap = 10.0;
        let total_w = btn_w * 3.0 + gap * 2.0;
        let start_x = card.x + (CARD_W - total_w) / 2.0;

        let buttons = [
            (start_x, t("save_prompt.save"), [0.20, 0.50, 0.30, 1.0]),
            (
                start_x + btn_w + gap,
                t("save_prompt.discard"),
                [0.55, 0.25, 0.20, 1.0],
            ),
            (
                start_x + 2.0 * (btn_w + gap),
                t("save_prompt.cancel"),
                [0.25, 0.25, 0.30, 1.0],
            ),
        ];

        for (bx, label, color) in buttons {
            quads.push(QuadInstance {
                rect: [bx, btn_y, btn_w, 30.0],
                color,
                color_bottom: color,
                border_color: [0.5, 0.5, 0.55, 0.5],
                border_width: 1.0,
                border_radius: 4.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text: label,
                bounds: Rect {
                    x: bx,
                    y: btn_y,
                    width: btn_w,
                    height: 30.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([230, 230, 235]),
                font_family_override: None,
            });
        }
    }
}
