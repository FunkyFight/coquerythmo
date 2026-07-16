use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 560.0;
const CARD_H: f32 = 230.0;
const BUTTON_W: f32 = 154.0;
const BUTTON_H: f32 = 42.0;
const BUTTON_GAP: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePromptKind {
    NewProject,
    CloseProject,
    ExitApplication,
}

pub struct SavePromptModal {
    kind: SavePromptKind,
    focused: usize,
}

pub enum SavePromptResult {
    Consumed,
    Save,
    Discard,
    Cancel,
}

impl Default for SavePromptModal {
    fn default() -> Self {
        Self::new(SavePromptKind::NewProject)
    }
}

impl SavePromptModal {
    pub fn new(kind: SavePromptKind) -> Self {
        Self { kind, focused: 2 }
    }

    pub fn kind(&self) -> SavePromptKind {
        self.kind
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect {
            x: (sw - CARD_W) / 2.0,
            y: (sh - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn button_rects(card: Rect) -> [Rect; 3] {
        let total_w = BUTTON_W * 3.0 + BUTTON_GAP * 2.0;
        let start_x = card.x + (card.width - total_w) / 2.0;
        let y = card.y + card.height - BUTTON_H - 24.0;
        [0.0, 1.0, 2.0].map(|index| Rect {
            x: start_x + index * (BUTTON_W + BUTTON_GAP),
            y,
            width: BUTTON_W,
            height: BUTTON_H,
        })
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> SavePromptResult {
        let card = Self::card_rect(sw, sh);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => SavePromptResult::Cancel,
            UiEvent::KeyInput { text } if text == "\t" => {
                self.focused = (self.focused + 1) % 3;
                SavePromptResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\u{b}" => {
                self.focused = (self.focused + 2) % 3;
                SavePromptResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => match self.focused {
                0 => SavePromptResult::Save,
                1 => SavePromptResult::Discard,
                _ => SavePromptResult::Cancel,
            },
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return SavePromptResult::Cancel;
                }
                let [save_btn, discard_btn, cancel_btn] = Self::button_rects(card);

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
            text: t("save_prompt.title"),
            bounds: Rect {
                x: card.x + 30.0,
                y: card.y + 24.0,
                width: card.width - 60.0,
                height: 32.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(20.0),
            color_override: Some([248, 211, 99]),
            font_family_override: None,
        });

        // Message
        labels.push(LabelInfo {
            text: match self.kind {
                SavePromptKind::NewProject => t("save_prompt.message.new_project"),
                SavePromptKind::CloseProject => t("save_prompt.message.close_project"),
                SavePromptKind::ExitApplication => t("save_prompt.message.exit_application"),
            },
            bounds: Rect {
                x: card.x + 30.0,
                y: card.y + 70.0,
                width: card.width - 60.0,
                height: 54.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(15.0),
            color_override: Some([222, 222, 232]),
            font_family_override: None,
        });

        // Buttons
        let button_rects = Self::button_rects(card);
        let buttons = [
            (t("save_prompt.save"), [0.18, 0.52, 0.32, 1.0]),
            (t("save_prompt.discard"), [0.58, 0.24, 0.22, 1.0]),
            (t("save_prompt.cancel"), [0.25, 0.25, 0.32, 1.0]),
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
                bounds: Rect {
                    x: button.x,
                    y: button.y,
                    width: button.width,
                    height: button.height,
                },
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
