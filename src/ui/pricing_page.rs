use super::text_input;
use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PricingPlan {
    Independent,
    Studio,
    School,
    Enterprise,
}

pub const PLANS: [PricingPlan; 4] = [
    PricingPlan::Independent,
    PricingPlan::Studio,
    PricingPlan::School,
    PricingPlan::Enterprise,
];

impl PricingPlan {
    pub fn name(self) -> &'static str {
        match self {
            PricingPlan::Independent => t("pricing.plan_independent"),
            PricingPlan::Studio => t("pricing.plan_studio"),
            PricingPlan::School => t("pricing.plan_school"),
            PricingPlan::Enterprise => t("pricing.plan_enterprise"),
        }
    }
    pub fn price(self) -> &'static str {
        match self {
            PricingPlan::Independent => t("pricing.plan_independent_price"),
            PricingPlan::Studio => t("pricing.plan_studio_price"),
            PricingPlan::School => t("pricing.plan_school_price"),
            PricingPlan::Enterprise => t("pricing.plan_enterprise_price"),
        }
    }
    pub fn is_enterprise(self) -> bool {
        matches!(self, PricingPlan::Enterprise)
    }
}

pub enum PricingResult {
    Consumed,
    Close,
    SelectPlan(PricingPlan),
    ActivateLicense,
}

/// Word-wrap `text` to `max_w` pixels at the given font size.
/// Returns slices borrowed from `text` (so the labels stay valid for the render lifetime).
pub fn wrap_text<'a>(text: &'a str, max_w: f32, font_size: f32) -> Vec<&'a str> {
    let space_w = text_input::text_width(" ", font_size);
    let mut out: Vec<&'a str> = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            out.push("");
            continue;
        }
        let bytes = paragraph.as_bytes();
        let mut words: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i > start {
                words.push((start, i));
            }
        }
        let mut line_start: Option<usize> = None;
        let mut current_w = 0.0f32;
        let mut idx = 0;
        while idx < words.len() {
            let (ws, we) = words[idx];
            let word = &paragraph[ws..we];
            let w = text_input::text_width(word, font_size);
            let add = if line_start.is_some() { space_w + w } else { w };
            if let Some(ls) = line_start {
                if current_w + add > max_w {
                    out.push(&paragraph[ls..words[idx - 1].1]);
                    line_start = Some(ws);
                    current_w = w;
                    idx += 1;
                } else {
                    current_w += add;
                    idx += 1;
                }
            } else {
                line_start = Some(ws);
                current_w = w;
                idx += 1;
            }
        }
        if let Some(ls) = line_start {
            out.push(&paragraph[ls..words[words.len() - 1].1]);
        }
    }
    out
}

struct PricingLayout {
    content_w: f32,
    cx: f32,
    close_rect: Rect,
    cards: [Rect; 4],
    buttons: [Rect; 4],
    activate_button: Rect,
}

fn compute_layout(sw: f32, sh: f32) -> (f32, PricingLayout) {
    let content_w = (980.0f32).min(sw - 48.0).max(320.0);
    let cx = (sw - content_w) / 2.0;
    let y0 = ((sh - 660.0).max(0.0) / 2.0).max(36.0);

    let close_size = 30.0;
    let close_rect = Rect {
        x: sw - close_size - 18.0,
        y: 18.0,
        width: close_size,
        height: close_size,
    };

    let gap = 16.0;
    let card_w = (content_w - 3.0 * gap) / 4.0;
    let card_h = 220.0;
    let cards_y = y0 + 262.0;
    let mut cards = [Rect::default(); 4];
    let mut buttons = [Rect::default(); 4];
    for i in 0..4 {
        let x = cx + i as f32 * (card_w + gap);
        cards[i] = Rect {
            x,
            y: cards_y,
            width: card_w,
            height: card_h,
        };
        let btn_w = card_w - 32.0;
        let btn_h = 38.0;
        buttons[i] = Rect {
            x: x + 16.0,
            y: cards_y + card_h - btn_h - 16.0,
            width: btn_w,
            height: btn_h,
        };
    }

    let btn_w = 220.0;
    let btn_h = 40.0;
    let activate_button = Rect {
        x: (sw - btn_w) / 2.0,
        y: y0 + 550.0,
        width: btn_w,
        height: btn_h,
    };

    (
        y0,
        PricingLayout {
            content_w,
            cx,
            close_rect,
            cards,
            buttons,
            activate_button,
        },
    )
}

pub struct PricingPage {
    hover_close: bool,
    hover_plan: Option<usize>,
    hover_activate: bool,
}

impl Default for PricingPage {
    fn default() -> Self {
        Self::new()
    }
}

impl PricingPage {
    pub fn new() -> Self {
        Self {
            hover_close: false,
            hover_plan: None,
            hover_activate: false,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> PricingResult {
        let (_y0, lay) = compute_layout(sw, sh);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => PricingResult::Close,
            UiEvent::MouseMove { x, y } => {
                self.hover_close = lay.close_rect.contains(*x, *y);
                self.hover_plan = lay.buttons.iter().position(|r| r.contains(*x, *y));
                self.hover_activate = lay.activate_button.contains(*x, *y);
                PricingResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if lay.close_rect.contains(*x, *y) {
                    return PricingResult::Close;
                }
                for (i, r) in lay.buttons.iter().enumerate() {
                    if r.contains(*x, *y) {
                        return PricingResult::SelectPlan(PLANS[i]);
                    }
                }
                if lay.activate_button.contains(*x, *y) {
                    return PricingResult::ActivateLicense;
                }
                PricingResult::Consumed
            }
            _ => PricingResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        _overlay: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let (y0, lay) = compute_layout(sw, sh);

        // Page background
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.06, 0.06, 0.08, 1.0],
            color_bottom: [0.08, 0.08, 0.11, 1.0],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Close button (top right)
        let close_color = if self.hover_close {
            [0.22, 0.22, 0.28, 1.0]
        } else {
            [0.16, 0.16, 0.20, 1.0]
        };
        quads.push(QuadInstance {
            rect: [
                lay.close_rect.x,
                lay.close_rect.y,
                lay.close_rect.width,
                lay.close_rect.height,
            ],
            color: close_color,
            color_bottom: close_color,
            border_color: [0.35, 0.35, 0.42, 0.6],
            border_width: 1.0,
            border_radius: 8.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "✕",
            bounds: lay.close_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: Some(if self.hover_close {
                [225, 225, 235]
            } else {
                [165, 165, 178]
            }),
            font_family_override: None,
        });

        // Title
        labels.push(LabelInfo {
            text: t("pricing.title"),
            bounds: Rect {
                x: lay.cx,
                y: y0 + 14.0,
                width: lay.content_w,
                height: 40.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(30.0),
            color_override: Some([236, 236, 242]),
            font_family_override: None,
        });
        // Title accent underline
        quads.push(QuadInstance {
            rect: [sw / 2.0 - 36.0, y0 + 58.0, 72.0, 3.0],
            color: [0.55, 0.50, 1.0, 1.0],
            color_bottom: [0.55, 0.50, 1.0, 1.0],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Paragraph
        let para = t("pricing.paragraph");
        let max_w = (720.0f32).min(lay.content_w);
        let lines = wrap_text(para, max_w, 14.0);
        let mut py = y0 + 74.0;
        for line in lines {
            if line.trim().is_empty() {
                py += 10.0;
                continue;
            }
            labels.push(LabelInfo {
                text: line,
                bounds: Rect {
                    x: lay.cx,
                    y: py,
                    width: lay.content_w,
                    height: 21.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(14.0),
                color_override: Some([178, 180, 195]),
                font_family_override: None,
            });
            py += 21.0;
        }

        // Plan cards
        for (i, card) in lay.cards.iter().enumerate() {
            let plan = PLANS[i];
            let accent = [0.55, 0.50, 1.0, 1.0];
            let hovered = self.hover_plan == Some(i);

            quads.push(QuadInstance {
                rect: [card.x, card.y, card.width, card.height],
                color: [0.11, 0.11, 0.14, 1.0],
                color_bottom: [0.08, 0.08, 0.10, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.7],
                border_width: 1.0,
                border_radius: 12.0,
                shadow_offset: [0.0, 3.0],
                shadow_color: [0.0, 0.0, 0.0, 0.35],
                shadow_blur: 12.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            // Top accent stripe
            quads.push(QuadInstance {
                rect: [card.x + 16.0, card.y + 14.0, card.width - 32.0, 3.0],
                color: accent,
                color_bottom: accent,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            // Plan name
            labels.push(LabelInfo {
                text: plan.name(),
                bounds: Rect {
                    x: card.x + 12.0,
                    y: card.y + 28.0,
                    width: card.width - 24.0,
                    height: 28.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(15.0),
                color_override: Some([228, 228, 235]),
                font_family_override: None,
            });
            // Price
            labels.push(LabelInfo {
                text: plan.price(),
                bounds: Rect {
                    x: card.x + 12.0,
                    y: card.y + 60.0,
                    width: card.width - 24.0,
                    height: 38.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(28.0),
                color_override: Some([255, 255, 255]),
                font_family_override: None,
            });
            // Sub line
            let sub = if plan.is_enterprise() {
                t("pricing.plan_enterprise_note")
            } else {
                t("pricing.yearly")
            };
            labels.push(LabelInfo {
                text: sub,
                bounds: Rect {
                    x: card.x + 12.0,
                    y: card.y + 104.0,
                    width: card.width - 24.0,
                    height: 20.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([150, 152, 168]),
                font_family_override: None,
            });

            // Select button
            let btn = lay.buttons[i];
            let btn_color = if hovered {
                if plan.is_enterprise() {
                    [0.26, 0.48, 0.78, 1.0]
                } else {
                    [0.28, 0.56, 0.78, 1.0]
                }
            } else if plan.is_enterprise() {
                [0.20, 0.42, 0.70, 1.0]
            } else {
                [0.22, 0.50, 0.72, 1.0]
            };
            quads.push(QuadInstance {
                rect: [btn.x, btn.y, btn.width, btn.height],
                color: btn_color,
                color_bottom: btn_color,
                border_color: [0.5, 0.55, 0.7, 0.5],
                border_width: 1.0,
                border_radius: 6.0,
                shadow_offset: [0.0, 1.0],
                shadow_color: [0.0, 0.0, 0.0, 0.3],
                shadow_blur: 4.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            let btn_label = if plan.is_enterprise() {
                t("pricing.request_quote")
            } else {
                t("pricing.select")
            };
            labels.push(LabelInfo {
                text: btn_label,
                bounds: btn,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(13.0),
                color_override: Some([245, 245, 250]),
                font_family_override: None,
            });
        }

        // Activate licence section
        labels.push(LabelInfo {
            text: t("pricing.activate_license"),
            bounds: Rect {
                x: lay.cx,
                y: y0 + 518.0,
                width: lay.content_w,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: Some([172, 174, 188]),
            font_family_override: None,
        });
        let ab = lay.activate_button;
        let ab_color = if self.hover_activate {
            [0.20, 0.20, 0.26, 1.0]
        } else {
            [0.15, 0.15, 0.19, 1.0]
        };
        quads.push(QuadInstance {
            rect: [ab.x, ab.y, ab.width, ab.height],
            color: ab_color,
            color_bottom: ab_color,
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5,
            border_radius: 8.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            shadow_blur: 6.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("pricing.activate_button"),
            bounds: ab,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: Some([220, 220, 230]),
            font_family_override: None,
        });
    }
}
