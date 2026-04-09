use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign, VocalRemovalParams};
use crate::i18n::t;

const CARD_W: f32 = 440.0;
const CARD_H: f32 = 370.0;
const ROW_H: f32 = 22.0;
const LABEL_W: f32 = 220.0;
const VALUE_W: f32 = 60.0;
const BTN_W: f32 = 30.0;

pub struct VocalRemoverModal {
    pub params: [f32; 11],
    pub param_texts: [String; 11],
}

const PARAM_NAMES: [&str; 11] = [
    "Reverb Room Size",
    "Reverb Damping",
    "Reverb Dry Level",
    "Reverb Wet Level",
    "Delay Seconds",
    "Delay Mix",
    "Compressor Threshold",
    "Compressor Ratio",
    "Compressor Attack",
    "Compressor Release",
    "Vocal Gain",
];

const PARAM_DEFAULTS: [f32; 11] = [
    0.15, 0.7, 0.8, 0.2, 0.0, 0.0, -15.0, 4.0, 1.0, 100.0, 0.0,
];

const PARAM_STEPS: [f32; 11] = [
    0.05, 0.05, 0.05, 0.05, 0.1, 0.05, 1.0, 0.5, 0.5, 10.0, 1.0,
];

const PARAM_MINS: [f32; 11] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -60.0, 1.0, 0.1, 10.0, -20.0,
];

const PARAM_MAXS: [f32; 11] = [
    1.0, 1.0, 1.0, 1.0, 5.0, 1.0, 0.0, 20.0, 50.0, 1000.0, 20.0,
];

pub enum VocalRemoverResult {
    Consumed,
    Close,
    Start(VocalRemovalParams),
}

impl VocalRemoverModal {
    pub fn new() -> Self {
        let params = PARAM_DEFAULTS;
        let param_texts = std::array::from_fn(|i| format_param(params[i], i));
        Self { params, param_texts }
    }

    fn card_rect(sw: f32, sh: f32) -> Rect {
        Rect { x: (sw - CARD_W) / 2.0, y: (sh - CARD_H) / 2.0, width: CARD_W, height: CARD_H }
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> VocalRemoverResult {
        let card = Self::card_rect(sw, sh);

        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => VocalRemoverResult::Close,

            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) { return VocalRemoverResult::Close; }

                let base_y = card.y + 50.0;
                let px = card.x + 20.0;

                // Check +/- buttons for each param
                for i in 0..11 {
                    let ry = base_y + i as f32 * ROW_H;
                    let minus = Rect { x: px + LABEL_W, y: ry, width: BTN_W, height: ROW_H };
                    let plus = Rect { x: px + LABEL_W + BTN_W + VALUE_W, y: ry, width: BTN_W, height: ROW_H };
                    if minus.contains(*x, *y) {
                        self.params[i] = (self.params[i] - PARAM_STEPS[i]).max(PARAM_MINS[i]);
                        self.param_texts[i] = format_param(self.params[i], i);
                        return VocalRemoverResult::Consumed;
                    }
                    if plus.contains(*x, *y) {
                        self.params[i] = (self.params[i] + PARAM_STEPS[i]).min(PARAM_MAXS[i]);
                        self.param_texts[i] = format_param(self.params[i], i);
                        return VocalRemoverResult::Consumed;
                    }
                }

                // Start button
                let btn_y = base_y + 11.0 * ROW_H + 16.0;
                let btn = Rect { x: card.x + (CARD_W - 180.0) / 2.0, y: btn_y, width: 180.0, height: 30.0 };
                if btn.contains(*x, *y) {
                    return VocalRemoverResult::Start(self.to_params());
                }

                VocalRemoverResult::Consumed
            }
            _ => VocalRemoverResult::Consumed,
        }
    }

    fn to_params(&self) -> VocalRemovalParams {
        VocalRemovalParams {
            reverb_room_size: self.params[0],
            reverb_damping: self.params[1],
            reverb_dry: self.params[2],
            reverb_wet: self.params[3],
            delay_seconds: self.params[4],
            delay_mix: self.params[5],
            compressor_threshold: self.params[6],
            compressor_ratio: self.params[7],
            compressor_attack: self.params[8],
            compressor_release: self.params[9],
            vocal_gain: self.params[10],
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
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5, border_radius: 14.0,
            shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Title
        labels.push(LabelInfo {
            text: t("tools.vocal_remover"),
            bounds: Rect { x: card.x, y: card.y + 8.0, width: card.width, height: 28.0 },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(16.0), color_override: None, font_family_override: None,
        });

        // Section header
        labels.push(LabelInfo {
            text: t("tools.vocal_params"),
            bounds: Rect { x: card.x + 20.0, y: card.y + 34.0, width: 300.0, height: 16.0 },
            h_align: HAlign::Left, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(10.0), color_override: Some([140, 140, 155]), font_family_override: None,
        });

        let base_y = card.y + 50.0;
        let px = card.x + 20.0;

        for i in 0..11 {
            let ry = base_y + i as f32 * ROW_H;

            // Label
            labels.push(LabelInfo {
                text: PARAM_NAMES[i],
                bounds: Rect { x: px, y: ry, width: LABEL_W, height: ROW_H },
                h_align: HAlign::Left, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(11.0), color_override: None, font_family_override: None,
            });

            // Minus
            quads.push(QuadInstance {
                rect: [px + LABEL_W, ry + 2.0, BTN_W, ROW_H - 4.0],
                color: [0.15, 0.15, 0.18, 1.0], color_bottom: [0.15, 0.15, 0.18, 1.0],
                border_color: [0.30, 0.30, 0.36, 0.5], border_width: 1.0, border_radius: 3.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text: "\u{2212}",
                bounds: Rect { x: px + LABEL_W, y: ry, width: BTN_W, height: ROW_H },
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(12.0), color_override: None, font_family_override: None,
            });

            // Value
            quads.push(QuadInstance {
                rect: [px + LABEL_W + BTN_W, ry + 2.0, VALUE_W, ROW_H - 4.0],
                color: [0.08, 0.08, 0.10, 1.0], color_bottom: [0.08, 0.08, 0.10, 1.0],
                border_color: [0.25, 0.25, 0.30, 0.3], border_width: 1.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text: &self.param_texts[i],
                bounds: Rect { x: px + LABEL_W + BTN_W, y: ry, width: VALUE_W, height: ROW_H },
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(10.0), color_override: None, font_family_override: None,
            });

            // Plus
            quads.push(QuadInstance {
                rect: [px + LABEL_W + BTN_W + VALUE_W, ry + 2.0, BTN_W, ROW_H - 4.0],
                color: [0.15, 0.15, 0.18, 1.0], color_bottom: [0.15, 0.15, 0.18, 1.0],
                border_color: [0.30, 0.30, 0.36, 0.5], border_width: 1.0, border_radius: 3.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            labels.push(LabelInfo {
                text: "+",
                bounds: Rect { x: px + LABEL_W + BTN_W + VALUE_W, y: ry, width: BTN_W, height: ROW_H },
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(12.0), color_override: None, font_family_override: None,
            });
        }

        // Start button
        let btn_y = base_y + 11.0 * ROW_H + 16.0;
        let btn = Rect { x: card.x + (CARD_W - 180.0) / 2.0, y: btn_y, width: 180.0, height: 30.0 };
        quads.push(QuadInstance {
            rect: [btn.x, btn.y, btn.width, btn.height],
            color: [0.18, 0.18, 0.22, 1.0], color_bottom: [0.18, 0.18, 0.22, 1.0],
            border_color: [0.35, 0.35, 0.42, 0.6], border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("tools.start"),
            bounds: Rect { x: btn.x, y: btn.y, width: btn.width, height: btn.height },
            h_align: HAlign::Center, v_align: VAlign::Center,
            overflow: Overflow::Clip, padding: 0.0,
            font_size_override: Some(11.0), color_override: None, font_family_override: None,
        });
    }
}

fn format_param(v: f32, _i: usize) -> String {
    if v.fract().abs() < 0.001 { format!("{:.0}", v) } else { format!("{:.2}", v) }
}
