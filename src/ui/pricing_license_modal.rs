use super::text_input;
use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub struct PricingLicenseModal {
    key: String,
    input: text_input::TextInputState,
    error: Option<String>,
    field_rect: Rect,
    activate_rect: Rect,
    close_rect: Rect,
}

pub enum PricingLicenseModalResult {
    Consumed,
    Close,
    Activate(String),
}

impl PricingLicenseModal {
    pub fn new() -> Self {
        let mut input = text_input::TextInputState::new();
        input.activate("");
        Self {
            key: String::new(),
            input,
            error: None,
            field_rect: Rect::default(),
            activate_rect: Rect::default(),
            close_rect: Rect::default(),
        }
    }

    pub fn next_cursor_blink_deadline(&self) -> std::time::Instant {
        self.input.next_cursor_blink_deadline()
            .unwrap_or_else(|| std::time::Instant::now())
    }

    fn layout(sw: f32, sh: f32) -> (Rect, Rect, Rect, Rect) {
        let dw = 460.0;
        let dh = 270.0;
        let dx = (sw - dw) / 2.0;
        let dy = (sh - dh) / 2.0;
        let card = Rect {
            x: dx,
            y: dy,
            width: dw,
            height: dh,
        };
        let fw = dw - 48.0;
        let field_rect = Rect {
            x: dx + 24.0,
            y: dy + 96.0,
            width: fw,
            height: 32.0,
        };
        let btn_w = 160.0;
        let btn_h = 38.0;
        let gap = 16.0;
        let total = btn_w * 2.0 + gap;
        let start_x = dx + (dw - total) / 2.0;
        let by = dy + dh - 52.0;
        let activate_rect = Rect {
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
        (card, field_rect, activate_rect, close_rect)
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> PricingLicenseModalResult {
        let (_card, field_rect, activate_rect, close_rect) = Self::layout(sw, sh);
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return PricingLicenseModalResult::Close;
                }
                if text == "\r" || text == "\n" {
                    if self.key.trim().is_empty() {
                        self.error = Some(t("pricing.license_modal.error_required").to_string());
                        return PricingLicenseModalResult::Consumed;
                    }
                    return PricingLicenseModalResult::Activate(self.key.trim().to_string());
                }
                if let Some(action) = self.input.handle_key(text, &self.key) {
                    if let text_input::TextInputAction::Changed(new_text) = action {
                        self.key = new_text;
                        self.error = None;
                    }
                }
                PricingLicenseModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                PricingLicenseModalResult::Consumed
            }
            UiEvent::CursorRight => {
                self.input.move_right(&self.key);
                PricingLicenseModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if field_rect.contains(*x, *y) {
                    self.input.activate(&self.key);
                    let pos = text_input::cursor_pos_from_x(
                        &self.key,
                        field_rect,
                        *x,
                        text_input::TextInputMetrics::left(14.0, 10.0),
                    );
                    self.input.set_cursor_pos(pos);
                    return PricingLicenseModalResult::Consumed;
                }
                if activate_rect.contains(*x, *y) {
                    if self.key.trim().is_empty() {
                        self.error = Some(t("pricing.license_modal.error_required").to_string());
                        return PricingLicenseModalResult::Consumed;
                    }
                    return PricingLicenseModalResult::Activate(self.key.trim().to_string());
                }
                if close_rect.contains(*x, *y) {
                    return PricingLicenseModalResult::Close;
                }
                PricingLicenseModalResult::Consumed
            }
            _ => PricingLicenseModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let (card, field_rect, activate_rect, close_rect) = Self::layout(sw, sh);

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
            text: t("pricing.license_modal.title"),
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

        let desc_lines = super::pricing_page::wrap_text(t("pricing.license_modal.desc"), card.width - 48.0, 12.0);
        let mut dy = card.y + 48.0;
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

        labels.push(LabelInfo {
            text: t("pricing.license_modal.key"),
            bounds: Rect {
                x: field_rect.x,
                y: field_rect.y - 20.0,
                width: field_rect.width,
                height: 18.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([150, 152, 168]),
            font_family_override: None,
        });

        let focused = true;
        overlay.push(QuadInstance {
            rect: [field_rect.x, field_rect.y, field_rect.width, field_rect.height],
            color: [0.07, 0.07, 0.10, 1.0],
            color_bottom: [0.07, 0.07, 0.10, 1.0],
            border_color: if self.error.is_some() {
                [0.80, 0.35, 0.35, 0.9]
            } else {
                [0.40, 0.37, 0.80, 0.8]
            },
            border_width: 1.0,
            border_radius: 5.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        if !self.key.is_empty() {
            labels.push(LabelInfo {
                text: self.key.as_str(),
                bounds: field_rect,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 10.0,
                font_size_override: Some(14.0),
                color_override: Some([235, 235, 240]),
                font_family_override: None,
            });
        }
        text_input::render_selection_and_cursor(
            overlay,
            field_rect,
            &self.key,
            &self.input,
            focused,
            text_input::TextInputMetrics::left(14.0, 10.0),
            4.0,
            4.0,
            [0.25, 0.45, 0.95, 0.42],
            [0.88, 0.88, 0.96, 1.0],
        );

        if let Some(err) = &self.error {
            labels.push(LabelInfo {
                text: err.as_str(),
                bounds: Rect {
                    x: field_rect.x,
                    y: field_rect.y + field_rect.height + 4.0,
                    width: field_rect.width,
                    height: 16.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([210, 120, 120]),
                font_family_override: None,
            });
        }

        overlay.push(QuadInstance {
            rect: [activate_rect.x, activate_rect.y, activate_rect.width, activate_rect.height],
            color: [0.22, 0.50, 0.72, 1.0],
            color_bottom: [0.22, 0.50, 0.72, 1.0],
            border_color: [0.5, 0.55, 0.7, 0.5],
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("pricing.license_modal.activate"),
            bounds: activate_rect,
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
            text: t("pricing.license_modal.close"),
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
