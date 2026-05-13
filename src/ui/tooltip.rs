use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

const TOOLTIP_PADDING_H: f32 = 12.0;
const TOOLTIP_PADDING_V: f32 = 6.0;
const TOOLTIP_OFFSET_Y: f32 = 20.0;
const TOOLTIP_RADIUS: f32 = 4.0;

pub struct TooltipState {
    pub text: String,
    pub cursor_x: f32,
    pub cursor_y: f32,
}

impl TooltipState {
    fn rect(&self, screen_w: f32, font_size: f32) -> Rect {
        let char_count = self.text.chars().count() as f32;
        // Approximate char width as ~60% of font size for sans-serif
        let char_w = font_size * 0.6;
        let text_w = char_count * char_w;
        let w = (text_w + TOOLTIP_PADDING_H * 2.0).clamp(40.0, screen_w - 8.0);
        let h = font_size + TOOLTIP_PADDING_V * 2.0;
        let x = (self.cursor_x - w / 2.0).clamp(4.0, screen_w - w - 4.0);
        let y = self.cursor_y + TOOLTIP_OFFSET_Y;
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn render_quads(&self, screen_w: f32) -> Vec<QuadInstance> {
        let font_size = crate::config::get().ui.font_size;
        let r = self.rect(screen_w, font_size);
        vec![QuadInstance {
            rect: [r.x, r.y, r.width, r.height],
            color: [0.18, 0.18, 0.20, 0.95],
            color_bottom: [0.14, 0.14, 0.16, 0.95],
            border_color: [0.30, 0.30, 0.36, 0.6],
            border_width: 1.0,
            border_radius: TOOLTIP_RADIUS,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.4],
            shadow_blur: 6.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    pub fn render_labels(&self, screen_w: f32) -> Vec<LabelInfo<'_>> {
        let font_size = crate::config::get().ui.font_size;
        let r = self.rect(screen_w, font_size);
        vec![LabelInfo {
            text: &self.text,
            bounds: r,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Visible,
            padding: TOOLTIP_PADDING_H,
            font_size_override: None,
            color_override: None,
            font_family_override: None,
        }]
    }
}
