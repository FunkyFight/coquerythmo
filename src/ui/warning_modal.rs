use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 460.0;
const CARD_H: f32 = 180.0;

pub struct WarningModal {
    pub warning_type: String, // "ai", "heavy", or "download"
    pub message_lines: Vec<String>,
}

pub enum WarningResult {
    Consumed,
    Ok { warning_type: String },
    NeverAgain { warning_type: String },
}

impl WarningModal {
    pub fn new(warning_type: &str) -> Self {
        let msg = match warning_type {
            "ai" => t("tools.warning_ai"),
            "heavy" => t("tools.warning_heavy"),
            "download" => t("tools.warning_download"),
            _ => "",
        };
        let lines = word_wrap(msg, CARD_W - 40.0);
        Self {
            warning_type: warning_type.to_string(),
            message_lines: lines,
        }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect { x: (sw - CARD_W) / 2.0, y: (sh - CARD_H) / 2.0, width: CARD_W, height: CARD_H }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> WarningResult {
        let card = Self::card_rect(sw, sh);
        match event {
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                WarningResult::Ok { warning_type: self.warning_type.clone() }
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return WarningResult::Ok { warning_type: self.warning_type.clone() };
                }
                let btn_y = card.y + CARD_H - 44.0;
                let ok_btn = Rect { x: card.x + CARD_W / 2.0 - 130.0, y: btn_y, width: 120.0, height: 30.0 };
                let dismiss_btn = Rect { x: card.x + CARD_W / 2.0 + 10.0, y: btn_y, width: 120.0, height: 30.0 };
                if ok_btn.contains(*x, *y) {
                    return WarningResult::Ok { warning_type: self.warning_type.clone() };
                }
                if dismiss_btn.contains(*x, *y) {
                    return WarningResult::NeverAgain { warning_type: self.warning_type.clone() };
                }
                WarningResult::Consumed
            }
            _ => WarningResult::Consumed,
        }
    }

    pub fn render<'a>(&'a self, quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>, sw: f32, sh: f32) {
        let card = Self::card_rect(sw, sh);

        // Dim
        quads.push(QuadInstance {
            rect: [0.0, 0.0, sw, sh],
            color: [0.0, 0.0, 0.0, 0.75], color_bottom: [0.0, 0.0, 0.0, 0.75],
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        // Card
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.22, 0.22, 0.26, 1.0], color_bottom: [0.16, 0.16, 0.19, 1.0],
            border_color: [0.55, 0.45, 0.20, 0.8],
            border_width: 1.5, border_radius: 14.0,
            shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Warning icon + title
        let title = match self.warning_type.as_str() {
            "ai" => "Avertissement IA",
            _ => "Avertissement",
        };
        labels.push(LabelInfo {
            text: title,
            bounds: Rect { x: card.x, y: card.y + 8.0, width: card.width, height: 24.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(14.0), color_override: Some([230, 190, 80]), font_family_override: None,
        });

        // Message lines
        let mut ly = card.y + 36.0;
        for line in &self.message_lines {
            labels.push(LabelInfo {
                text: line,
                bounds: Rect { x: card.x + 20.0, y: ly, width: card.width - 40.0, height: 16.0 },
                h_align: HAlign::Left, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(11.0), color_override: Some([200, 200, 210]), font_family_override: None,
            });
            ly += 16.0;
        }

        // Buttons
        let btn_y = card.y + CARD_H - 44.0;
        // OK button
        let ok_x = card.x + CARD_W / 2.0 - 130.0;
        quads.push(QuadInstance {
            rect: [ok_x, btn_y, 120.0, 30.0],
            color: [0.18, 0.18, 0.22, 1.0], color_bottom: [0.18, 0.18, 0.22, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("tools.warning_ok"),
            bounds: Rect { x: ok_x, y: btn_y, width: 120.0, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(11.0), color_override: None, font_family_override: None,
        });

        // Never again button
        let na_x = card.x + CARD_W / 2.0 + 10.0;
        quads.push(QuadInstance {
            rect: [na_x, btn_y, 120.0, 30.0],
            color: [0.18, 0.18, 0.22, 1.0], color_bottom: [0.18, 0.18, 0.22, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("tools.warning_dismiss"),
            bounds: Rect { x: na_x, y: btn_y, width: 120.0, height: 30.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(11.0), color_override: None, font_family_override: None,
        });
    }
}

fn word_wrap(text: &str, max_width: f32) -> Vec<String> {
    let char_w = 6.0;
    let max_chars = (max_width / char_w).floor() as usize;
    if max_chars == 0 { return vec![text.to_string()]; }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() { lines.push(current); }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}
