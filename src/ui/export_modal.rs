use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};

use crate::i18n::t;

const CARD_W: f32 = 400.0;
const CARD_H: f32 = 190.0;

pub struct ExportModal {
    pub fps: u32,
    pub fps_text: String,
    pub br_scale: f32,
    pub br_scale_text: String,
}

pub enum ExportModalResult {
    Consumed,
    Close,
    Export { fps: f64, br_scale: f32 },
}

impl ExportModal {
    pub fn new() -> Self {
        Self {
            fps: 60,
            fps_text: "60".to_string(),
            br_scale: 1.0,
            br_scale_text: "100%".to_string(),
        }
    }

    fn update_scale_text(&mut self) {
        self.br_scale_text = format!("{}%", (self.br_scale * 100.0).round() as u32);
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ExportModalResult {
        let card = Self::card_rect(screen_w, screen_h);

        // All events outside the card close the modal
        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ExportModalResult::Close;
                }
            }
            _ => {}
        }

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return ExportModalResult::Close;
                }
                if text == "\r" || text == "\n" {
                    return ExportModalResult::Export {
                        fps: self.fps as f64,
                        br_scale: self.br_scale,
                    };
                }
                ExportModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                // FPS minus button
                let fps_y = card.y + 46.0;
                let btn_sz = 36.0;
                let val_w = 80.0;
                let minus_rect = Rect {
                    x: card.x + 20.0,
                    y: fps_y,
                    width: btn_sz,
                    height: 30.0,
                };
                let plus_rect = Rect {
                    x: card.x + 20.0 + btn_sz + val_w,
                    y: fps_y,
                    width: btn_sz,
                    height: 30.0,
                };
                if minus_rect.contains(*x, *y) {
                    self.fps = (self.fps.saturating_sub(30)).max(30);
                    self.fps_text = self.fps.to_string();
                    return ExportModalResult::Consumed;
                }
                if plus_rect.contains(*x, *y) {
                    self.fps = (self.fps + 30).min(480);
                    self.fps_text = self.fps.to_string();
                    return ExportModalResult::Consumed;
                }

                let scale_y = card.y + 96.0;
                let scale_minus = Rect {
                    x: card.x + 20.0,
                    y: scale_y,
                    width: btn_sz,
                    height: 30.0,
                };
                let scale_plus = Rect {
                    x: card.x + 20.0 + btn_sz + val_w,
                    y: scale_y,
                    width: btn_sz,
                    height: 30.0,
                };
                if scale_minus.contains(*x, *y) {
                    self.br_scale = (self.br_scale - 0.25).max(0.5);
                    self.update_scale_text();
                    return ExportModalResult::Consumed;
                }
                if scale_plus.contains(*x, *y) {
                    self.br_scale = (self.br_scale + 0.25).min(2.0);
                    self.update_scale_text();
                    return ExportModalResult::Consumed;
                }

                // Export button (bigger hitbox)
                let btn_w = 160.0;
                let btn_h = 40.0;
                let btn_x = card.x + (card.width - btn_w) / 2.0;
                let btn_y = card.y + CARD_H - 48.0;
                let btn_rect = Rect {
                    x: btn_x,
                    y: btn_y,
                    width: btn_w,
                    height: btn_h,
                };
                if btn_rect.contains(*x, *y) {
                    return ExportModalResult::Export {
                        fps: self.fps as f64,
                        br_scale: self.br_scale,
                    };
                }

                ExportModalResult::Consumed
            }
            _ => ExportModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card_rect(screen_w, screen_h);

        // Dim background
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
            rect: [card.x, card.y, card.width, card.height],
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
        labels.push(LabelInfo {
            text: t("export_modal.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 8.0,
                width: card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: None,
            font_family_override: None,
        });

        // --- FPS section ---
        let fx = card.x + 20.0;
        let fw = card.width - 40.0;

        labels.push(LabelInfo {
            text: t("export_modal.fps"),
            bounds: Rect {
                x: fx,
                y: card.y + 26.0,
                width: fw,
                height: 18.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([180, 180, 195]),
            font_family_override: None,
        });

        let fps_y = card.y + 46.0;
        let btn_size = 36.0;
        let value_w = 80.0;

        // Minus button
        overlay_quads.push(QuadInstance {
            rect: [fx, fps_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0],
            color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "\u{2212}",
            bounds: Rect {
                x: fx,
                y: fps_y,
                width: btn_size,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });

        // Value display
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size, fps_y, value_w, 30.0],
            color: [0.08, 0.08, 0.10, 1.0],
            color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.3],
            border_width: 1.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: &self.fps_text,
            bounds: Rect {
                x: fx + btn_size,
                y: fps_y,
                width: value_w,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });

        // Plus button
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size + value_w, fps_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0],
            color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "+",
            bounds: Rect {
                x: fx + btn_size + value_w,
                y: fps_y,
                width: btn_size,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });

        labels.push(LabelInfo {
            text: t("export_modal.br_scale"),
            bounds: Rect {
                x: fx,
                y: card.y + 76.0,
                width: fw,
                height: 18.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([180, 180, 195]),
            font_family_override: None,
        });

        let scale_y = card.y + 96.0;
        overlay_quads.push(QuadInstance {
            rect: [fx, scale_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0],
            color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "\u{2212}",
            bounds: Rect {
                x: fx,
                y: scale_y,
                width: btn_size,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size, scale_y, value_w, 30.0],
            color: [0.08, 0.08, 0.10, 1.0],
            color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.3],
            border_width: 1.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: &self.br_scale_text,
            bounds: Rect {
                x: fx + btn_size,
                y: scale_y,
                width: value_w,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size + value_w, scale_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0],
            color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "+",
            bounds: Rect {
                x: fx + btn_size + value_w,
                y: scale_y,
                width: btn_size,
                height: 30.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });

        // --- Export button ---
        let btn_w = 160.0;
        let btn_h = 40.0;
        let btn_x = card.x + (card.width - btn_w) / 2.0;
        let btn_y = card.y + CARD_H - 48.0;
        overlay_quads.push(QuadInstance {
            rect: [btn_x, btn_y, btn_w, btn_h],
            color: [0.30, 0.55, 0.30, 1.0],
            color_bottom: [0.22, 0.45, 0.22, 1.0],
            border_color: [0.40, 0.65, 0.40, 0.8],
            border_width: 1.0,
            border_radius: 8.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            shadow_blur: 4.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("export_modal.export"),
            bounds: Rect {
                x: btn_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });
    }
}
