use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;

use crate::i18n::t;

const CARD_W: f32 = 400.0;
const CARD_H: f32 = 230.0;

pub struct ExportModal {
    pub filename: String,
    pub fps: u32,
    pub fps_text: String,
    pub input: text_input::TextInputState,
}

pub enum ExportModalResult {
    Consumed,
    Close,
    Export { filename: String, fps: f64 },
}

impl ExportModal {
    pub fn new() -> Self {
        let mut input = text_input::TextInputState::new();
        let filename = "export".to_string();
        input.activate(&filename);
        Self {
            filename,
            fps: 60,
            fps_text: "60".to_string(),
            input,
        }
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, screen_w: f32, screen_h: f32) -> ExportModalResult {
        let card = Self::card_rect(screen_w, screen_h);

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return ExportModalResult::Close;
                }
                if text == "\r" || text == "\n" {
                    if !self.filename.trim().is_empty() {
                        return ExportModalResult::Export {
                            filename: self.filename.clone(),
                            fps: self.fps as f64,
                        };
                    }
                    return ExportModalResult::Consumed;
                }
                if let Some(action) = self.input.handle_key(text, &self.filename) {
                    if let text_input::TextInputAction::Changed(new_text) = action {
                        self.filename = new_text;
                    }
                }
                ExportModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                ExportModalResult::Consumed
            }
            UiEvent::CursorRight => {
                self.input.move_right(&self.filename);
                ExportModalResult::Consumed
            }
            UiEvent::MousePress { x, y } => {
                if !card.contains(*x, *y) {
                    return ExportModalResult::Close;
                }

                // FPS minus button
                let fps_y = card.y + 120.0;
                let btn_sz = 36.0;
                let val_w = 80.0;
                let minus_rect = Rect { x: card.x + 20.0, y: fps_y, width: btn_sz, height: 30.0 };
                let plus_rect = Rect { x: card.x + 20.0 + btn_sz + val_w, y: fps_y, width: btn_sz, height: 30.0 };
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

                // Export button
                let btn_w = 160.0;
                let btn_h = 36.0;
                let btn_x = card.x + (card.width - btn_w) / 2.0;
                let btn_y = card.y + CARD_H - 50.0;
                let btn_rect = Rect { x: btn_x, y: btn_y, width: btn_w, height: btn_h };
                if btn_rect.contains(*x, *y) && !self.filename.trim().is_empty() {
                    return ExportModalResult::Export {
                        filename: self.filename.clone(),
                        fps: self.fps as f64,
                    };
                }

                ExportModalResult::Consumed
            }
            _ => ExportModalResult::Consumed,
        }
    }

    pub fn render<'a>(&'a self, overlay_quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>, screen_w: f32, screen_h: f32) {
        let card = Self::card_rect(screen_w, screen_h);

        // Dim background
        overlay_quads.push(QuadInstance {
            rect: [0.0, 0.0, screen_w, screen_h],
            color: [0.0, 0.0, 0.0, 0.75], color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        // Card
        overlay_quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0], color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5, border_radius: 14.0,
            shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Title
        labels.push(LabelInfo {
            text: t("export_modal.title"),
            bounds: Rect { x: card.x, y: card.y + 8.0, width: card.width, height: 28.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(16.0), color_override: None, font_family_override: None,
        });

        // --- Filename section ---
        let fx = card.x + 20.0;
        let fw = card.width - 40.0;

        labels.push(LabelInfo {
            text: t("export_modal.filename"),
            bounds: Rect { x: fx, y: card.y + 42.0, width: fw, height: 18.0 },
            h_align: HAlign::Left, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(12.0), color_override: Some([180, 180, 195]), font_family_override: None,
        });

        let field_y = card.y + 62.0;
        let field_h = 28.0;
        // Input background
        overlay_quads.push(QuadInstance {
            rect: [fx, field_y, fw, field_h],
            color: [0.08, 0.08, 0.10, 1.0], color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: [0.40, 0.37, 0.80, 0.8], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        // Filename text
        if !self.filename.is_empty() {
            labels.push(LabelInfo {
                text: &self.filename,
                bounds: Rect { x: fx, y: field_y, width: fw, height: field_h },
                h_align: HAlign::Left, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 8.0,
                font_size_override: Some(13.0), color_override: None, font_family_override: None,
            });
        }
        // Cursor
        if self.input.cursor_visible() {
            let cursor_x = fx + 8.0 + self.input.cursor_pos as f32 * 7.8;
            overlay_quads.push(QuadInstance {
                rect: [cursor_x, field_y + 4.0, 1.5, field_h - 8.0],
                color: [0.9, 0.9, 0.95, 1.0], color_bottom: [0.9, 0.9, 0.95, 1.0],
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
        }

        // --- FPS section ---
        labels.push(LabelInfo {
            text: t("export_modal.fps"),
            bounds: Rect { x: fx, y: card.y + 98.0, width: fw, height: 18.0 },
            h_align: HAlign::Left, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(12.0), color_override: Some([180, 180, 195]), font_family_override: None,
        });

        let fps_y = card.y + 120.0;
        let btn_size = 36.0;
        let value_w = 80.0;

        // Minus button
        overlay_quads.push(QuadInstance {
            rect: [fx, fps_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0], color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "\u{2212}",
            bounds: Rect { x: fx, y: fps_y, width: btn_size, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(14.0), color_override: None, font_family_override: None,
        });

        // Value display
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size, fps_y, value_w, 30.0],
            color: [0.08, 0.08, 0.10, 1.0], color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.3], border_width: 1.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: &self.fps_text,
            bounds: Rect { x: fx + btn_size, y: fps_y, width: value_w, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(12.0), color_override: None, font_family_override: None,
        });

        // Plus button
        overlay_quads.push(QuadInstance {
            rect: [fx + btn_size + value_w, fps_y, btn_size, 30.0],
            color: [0.15, 0.15, 0.18, 1.0], color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "+",
            bounds: Rect { x: fx + btn_size + value_w, y: fps_y, width: btn_size, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(14.0), color_override: None, font_family_override: None,
        });

        // --- Export button ---
        let btn_w = 160.0;
        let btn_h = 36.0;
        let btn_x = card.x + (card.width - btn_w) / 2.0;
        let btn_y = card.y + CARD_H - 50.0;
        overlay_quads.push(QuadInstance {
            rect: [btn_x, btn_y, btn_w, btn_h],
            color: [0.30, 0.55, 0.30, 1.0], color_bottom: [0.22, 0.45, 0.22, 1.0],
            border_color: [0.40, 0.65, 0.40, 0.8],
            border_width: 1.0, border_radius: 8.0,
            shadow_offset: [0.0, 2.0], shadow_color: [0.0, 0.0, 0.0, 0.3], shadow_blur: 4.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("export_modal.export"),
            bounds: Rect { x: btn_x, y: btn_y, width: btn_w, height: btn_h },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(14.0), color_override: None, font_family_override: None,
        });
    }
}
