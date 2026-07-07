use std::sync::OnceLock;
use std::time::Instant;

use super::theme;
use super::widget::{EventResponse, LabelInfo, QuadInstance, Rect, UiEvent, Widget};

static ANIM_START: OnceLock<Instant> = OnceLock::new();

fn anim_secs() -> f32 {
    let start = ANIM_START.get_or_init(Instant::now);
    start.elapsed().as_secs_f32()
}

pub struct LicenseBadge {
    bounds: Rect,
    label: String,
}

impl LicenseBadge {
    pub fn new(bounds: Rect, label: impl Into<String>) -> Self {
        Self {
            bounds,
            label: label.into(),
        }
    }
}

impl Widget for LicenseBadge {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn handle_event(&mut self, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    fn render_quads(&self) -> Vec<QuadInstance> {
        let t = anim_secs();
        let pulse = (t * 1.5).sin(); // slow pulse
        let p = (pulse + 1.0) / 2.0; // 0.0 .. 1.0

        // Top: brighter red, Bottom: darker red, animated vertically
        let r_top = 0.75 + 0.15 * p;
        let g_top = 0.08 + 0.06 * p;
        let b_top = 0.06;

        let r_bot = 0.40 + 0.12 * p;
        let g_bot = 0.02;
        let b_bot = 0.02;

        // Border pulses too
        let br = 0.80 + 0.20 * p;

        vec![QuadInstance {
            rect: [
                self.bounds.x,
                self.bounds.y,
                self.bounds.width,
                self.bounds.height,
            ],
            color: [r_top, g_top, b_top, 1.0],
            color_bottom: [r_bot, g_bot, b_bot, 1.0],
            border_color: [br, 0.15, 0.15, 0.9],
            border_width: 1.0,
            border_radius: theme::BORDER_RADIUS_SMALL,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        vec![LabelInfo {
            text: &self.label,
            bounds: self.bounds,
            h_align: super::widget::HAlign::Center,
            v_align: super::widget::VAlign::Center,
            overflow: super::widget::Overflow::Clip,
            padding: 4.0,
            font_size_override: Some(12.0),
            color_override: Some([255, 230, 230]),
            font_family_override: None,
        }]
    }
}
