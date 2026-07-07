use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub struct PricingPlanModal {
    plan_name: String,
    plan_price: String,
    is_enterprise: bool,
    confirm_rect: Rect,
    close_rect: Rect,
}

pub enum PricingPlanModalResult {
    Consumed,
    Close,
    Confirm(String),
}

impl PricingPlanModal {
    pub fn new(plan_name: String, plan_price: String, is_enterprise: bool) -> Self {
        Self {
            plan_name,
            plan_price,
            is_enterprise,
            confirm_rect: Rect::default(),
            close_rect: Rect::default(),
        }
    }

    fn layout(sw: f32, sh: f32) -> (Rect, Rect, Rect) {
        let dw = 460.0;
        let dh = 290.0;
        let dx = (sw - dw) / 2.0;
        let dy = (sh - dh) / 2.0;
        let card = Rect {
            x: dx,
            y: dy,
            width: dw,
            height: dh,
        };
        let btn_w = 160.0;
        let btn_h = 38.0;
        let gap = 16.0;
        let total = btn_w * 2.0 + gap;
        let start_x = dx + (dw - total) / 2.0;
        let by = dy + dh - 56.0;
        let confirm_rect = Rect {
            x: start_x,
            y: by,
            width: btn_w,
            height: btn_h,
        };
        let close_rect = Rect {
            x: start_x + btn_w + gap,
            y: by,
            width: btn_w,
            height: btn_h,
        };
        (card, confirm_rect, close_rect)
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> PricingPlanModalResult {
        let (_card, confirm_rect, close_rect) = Self::layout(sw, sh);
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return PricingPlanModalResult::Close;
                }
                if text == "\r" || text == "\n" {
                    return PricingPlanModalResult::Confirm(self.plan_name.clone());
                }
                PricingPlanModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if confirm_rect.contains(*x, *y) {
                    return PricingPlanModalResult::Confirm(self.plan_name.clone());
                }
                if close_rect.contains(*x, *y) {
                    return PricingPlanModalResult::Close;
                }
                PricingPlanModalResult::Consumed
            }
            _ => PricingPlanModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let (card, confirm_rect, close_rect) = Self::layout(sw, sh);

        overlay.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.72],
            color_bottom: [0.0, 0.0, 0.0, 0.72],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        overlay.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.16, 0.16, 0.20, 1.0],
            color_bottom: [0.12, 0.12, 0.15, 1.0],
            border_color: [0.35, 0.35, 0.45, 0.8],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 6.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 16.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        labels.push(LabelInfo {
            text: t("pricing.plan_modal.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 16.0,
                width: card.width,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: Some([232, 232, 240]),
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: &self.plan_name,
            bounds: Rect {
                x: card.x,
                y: card.y + 52.0,
                width: card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(20.0),
            color_override: Some([255, 255, 255]),
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: &self.plan_price,
            bounds: Rect {
                x: card.x,
                y: card.y + 84.0,
                width: card.width,
                height: 34.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(26.0),
            color_override: Some([150, 140, 255]),
            font_family_override: None,
        });

        let desc_lines = super::pricing_page::wrap_text(t("pricing.plan_modal.desc"), card.width - 48.0, 12.0);
        let mut dy = card.y + 128.0;
        for line in desc_lines {
            labels.push(LabelInfo {
                text: line,
                bounds: Rect {
                    x: card.x + 24.0,
                    y: dy,
                    width: card.width - 48.0,
                    height: 18.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([170, 172, 186]),
                font_family_override: None,
            });
            dy += 18.0;
        }

        let confirm_color = if self.is_enterprise {
            [0.20, 0.42, 0.70, 1.0]
        } else {
            [0.22, 0.50, 0.72, 1.0]
        };
        overlay.push(QuadInstance {
            rect: [confirm_rect.x, confirm_rect.y, confirm_rect.width, confirm_rect.height],
            color: confirm_color,
            color_bottom: confirm_color,
            border_color: [0.5, 0.55, 0.7, 0.5],
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        let confirm_label = if self.is_enterprise {
            t("pricing.plan_modal.quote")
        } else {
            t("pricing.plan_modal.confirm")
        };
        labels.push(LabelInfo {
            text: confirm_label,
            bounds: confirm_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: Some([245, 245, 250]),
            font_family_override: None,
        });

        overlay.push(QuadInstance {
            rect: [close_rect.x, close_rect.y, close_rect.width, close_rect.height],
            color: [0.20, 0.20, 0.25, 1.0],
            color_bottom: [0.16, 0.16, 0.20, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6],
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("pricing.plan_modal.close"),
            bounds: close_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: Some([215, 215, 225]),
            font_family_override: None,
        });
    }
}
