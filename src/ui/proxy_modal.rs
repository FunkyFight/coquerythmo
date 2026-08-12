#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 430.0;
const CARD_H: f32 = 338.0;
const PRESETS: [u32; 5] = [360, 480, 720, 1080, 1440];
const ENCODER_ITEM_H: f32 = 30.0;

pub struct ProxyModal {
    source_width: u32,
    source_height: u32,
    selected_max_height: u32,
    target_width: u32,
    target_height: u32,
    target_text: String,
    crf: u8,
    crf_text: String,
    encoder: crate::video_proxy::ProxyEncoder,
    encoder_menu_open: bool,
    keyboard_focus: usize,
}

pub enum ProxyModalResult {
    Consumed,
    Close,
    Create {
        width: u32,
        height: u32,
        crf: u8,
        encoder: crate::video_proxy::ProxyEncoder,
    },
}

impl ProxyModal {
    pub fn new(source_width: u32, source_height: u32) -> Self {
        let (target_width, target_height) =
            crate::video_proxy::default_proxy_size(source_width, source_height);
        let mut modal = Self {
            source_width,
            source_height,
            selected_max_height: 1080,
            target_width,
            target_height,
            target_text: String::new(),
            crf: 24,
            crf_text: String::new(),
            encoder: crate::video_proxy::ProxyEncoder::default(),
            encoder_menu_open: false,
            keyboard_focus: 0,
        };
        modal.update_texts();
        modal
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn update_target(&mut self) {
        let (width, height) = crate::video_proxy::fit_to_max_height(
            self.source_width,
            self.source_height,
            self.selected_max_height,
        );
        self.target_width = width;
        self.target_height = height;
        self.update_texts();
    }

    fn update_texts(&mut self) {
        self.target_text = format!("{} x {}", self.target_width, self.target_height);
        self.crf_text = format!("CRF {}", self.crf);
    }

    fn create_result(&self) -> ProxyModalResult {
        ProxyModalResult::Create {
            width: self.target_width,
            height: self.target_height,
            crf: self.crf,
            encoder: self.encoder,
        }
    }

    fn move_focus(&mut self, direction: i32) {
        let h264 = [0, 1, 2, 3];
        let fixed_quality = [0, 1, 3];
        let order: &[usize] = if self.encoder == crate::video_proxy::ProxyEncoder::H264 {
            &h264
        } else {
            &fixed_quality
        };
        let current = order
            .iter()
            .position(|focus| *focus == self.keyboard_focus)
            .unwrap_or(0) as i32;
        self.keyboard_focus = order[(current + direction).rem_euclid(order.len() as i32) as usize];
    }

    fn encoder_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 140.0,
            y: card.y + 174.0,
            width: card.width - 160.0,
            height: 30.0,
        }
    }

    fn encoder_option_rect(card: Rect, index: usize) -> Rect {
        let trigger = Self::encoder_rect(card);
        Rect {
            x: trigger.x,
            y: trigger.y + trigger.height + 4.0 + index as f32 * ENCODER_ITEM_H,
            width: trigger.width,
            height: ENCODER_ITEM_H,
        }
    }

    fn adjust_encoder(&mut self, direction: i32) {
        let current = crate::video_proxy::ProxyEncoder::ALL
            .iter()
            .position(|encoder| *encoder == self.encoder)
            .unwrap_or(0) as i32;
        let index = (current + direction)
            .rem_euclid(crate::video_proxy::ProxyEncoder::ALL.len() as i32)
            as usize;
        self.encoder = crate::video_proxy::ProxyEncoder::ALL[index];
        if self.encoder != crate::video_proxy::ProxyEncoder::H264 && self.keyboard_focus == 2 {
            self.keyboard_focus = 1;
        }
    }

    fn adjust_resolution(&mut self, direction: i32) {
        let current = PRESETS
            .iter()
            .position(|preset| *preset == self.selected_max_height)
            .unwrap_or(1);
        let index = if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(PRESETS.len() - 1)
        };
        self.selected_max_height = PRESETS[index];
        self.update_target();
    }

    fn adjust_quality(&mut self, direction: i32) {
        self.crf = if direction < 0 {
            self.crf.saturating_sub(1).max(18)
        } else {
            (self.crf + 1).min(32)
        };
        self.update_texts();
    }

    fn activate_focused(&mut self) -> ProxyModalResult {
        match self.keyboard_focus {
            0 => {
                self.adjust_resolution(1);
                ProxyModalResult::Consumed
            }
            1 => {
                self.encoder_menu_open = !self.encoder_menu_open;
                ProxyModalResult::Consumed
            }
            2 => {
                self.adjust_quality(1);
                ProxyModalResult::Consumed
            }
            3 => self.create_result(),
            _ => ProxyModalResult::Consumed,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => format!(
                "{} : {}p, {} : {}",
                t("proxy_modal.resolution"),
                self.selected_max_height,
                t("proxy_modal.target"),
                self.target_text
            ),
            1 => format!("{} : {}", t("proxy_modal.encoder"), self.encoder.label()),
            2 => format!("{} : {}", t("proxy_modal.quality"), self.crf_text),
            3 => t("proxy_modal.create").to_string(),
            _ => t("proxy_modal.title").to_string(),
        }
    }

    pub fn encoder_accessibility_state(&self) -> Option<(crate::video_proxy::ProxyEncoder, bool)> {
        (self.keyboard_focus == 1).then_some((self.encoder, self.encoder_menu_open))
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ProxyModalResult {
        let card = Self::card_rect(screen_w, screen_h);

        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ProxyModalResult::Close;
                }
            }
            _ => {}
        }

        if self.encoder_menu_open {
            match event {
                UiEvent::KeyInput { text } if text == "\x1b" => {
                    self.encoder_menu_open = false;
                    return ProxyModalResult::Consumed;
                }
                UiEvent::CursorUp => {
                    self.adjust_encoder(-1);
                    return ProxyModalResult::Consumed;
                }
                UiEvent::CursorDown => {
                    self.adjust_encoder(1);
                    return ProxyModalResult::Consumed;
                }
                UiEvent::FocusNext | UiEvent::FocusPrevious => {
                    self.encoder_menu_open = false;
                }
                UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                    self.encoder_menu_open = false;
                }
                UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                    for (index, encoder) in crate::video_proxy::ProxyEncoder::ALL.iter().enumerate()
                    {
                        if Self::encoder_option_rect(card, index).contains(*x, *y) {
                            self.encoder = *encoder;
                            self.encoder_menu_open = false;
                            return ProxyModalResult::Consumed;
                        }
                    }
                    self.encoder_menu_open = false;
                    return ProxyModalResult::Consumed;
                }
                _ => {}
            }
        }

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return ProxyModalResult::Close;
                }
                if text == "\t" || text == "\u{b}" {
                    self.move_focus(if text == "\t" { 1 } else { -1 });
                    return ProxyModalResult::Consumed;
                }
                if text == "\r" || text == "\n" || text == " " {
                    return self.activate_focused();
                }
                ProxyModalResult::Consumed
            }
            UiEvent::FocusNext => {
                self.move_focus(1);
                ProxyModalResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.move_focus(-1);
                ProxyModalResult::Consumed
            }
            UiEvent::Activate => self.activate_focused(),
            UiEvent::CursorLeft | UiEvent::CursorUp => {
                match self.keyboard_focus {
                    0 => self.adjust_resolution(-1),
                    1 => self.adjust_encoder(-1),
                    2 => self.adjust_quality(-1),
                    _ => self.move_focus(-1),
                }
                ProxyModalResult::Consumed
            }
            UiEvent::CursorRight | UiEvent::CursorDown => {
                match self.keyboard_focus {
                    0 => self.adjust_resolution(1),
                    1 => self.adjust_encoder(1),
                    2 => self.adjust_quality(1),
                    _ => self.move_focus(1),
                }
                ProxyModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let preset_y = card.y + 92.0;
                let preset_w = 70.0;
                let preset_h = 34.0;
                for (index, preset) in PRESETS.iter().enumerate() {
                    let rect = Rect {
                        x: card.x + 20.0 + index as f32 * (preset_w + 10.0),
                        y: preset_y,
                        width: preset_w,
                        height: preset_h,
                    };
                    if rect.contains(*x, *y) {
                        self.keyboard_focus = 0;
                        self.selected_max_height = *preset;
                        self.update_target();
                        return ProxyModalResult::Consumed;
                    }
                }

                let encoder_rect = Self::encoder_rect(card);
                if encoder_rect.contains(*x, *y) {
                    self.keyboard_focus = 1;
                    self.encoder_menu_open = true;
                    return ProxyModalResult::Consumed;
                }

                let quality_y = card.y + 230.0;
                let btn_size = 36.0;
                let value_w = 100.0;
                let minus_rect = Rect {
                    x: card.x + 20.0,
                    y: quality_y,
                    width: btn_size,
                    height: 30.0,
                };
                let plus_rect = Rect {
                    x: card.x + 20.0 + btn_size + value_w,
                    y: quality_y,
                    width: btn_size,
                    height: 30.0,
                };
                if self.encoder == crate::video_proxy::ProxyEncoder::H264
                    && minus_rect.contains(*x, *y)
                {
                    self.keyboard_focus = 2;
                    self.crf = self.crf.saturating_sub(1).max(18);
                    self.update_texts();
                    return ProxyModalResult::Consumed;
                }
                if self.encoder == crate::video_proxy::ProxyEncoder::H264
                    && plus_rect.contains(*x, *y)
                {
                    self.keyboard_focus = 2;
                    self.crf = (self.crf + 1).min(32);
                    self.update_texts();
                    return ProxyModalResult::Consumed;
                }

                let create_rect = Rect {
                    x: card.x + (card.width - 180.0) / 2.0,
                    y: card.y + CARD_H - 52.0,
                    width: 180.0,
                    height: 40.0,
                };
                if create_rect.contains(*x, *y) {
                    self.keyboard_focus = 3;
                    return self.create_result();
                }

                ProxyModalResult::Consumed
            }
            _ => ProxyModalResult::Consumed,
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

        push_label(
            labels,
            t("proxy_modal.title"),
            Rect {
                x: card.x,
                y: card.y + 8.0,
                width: card.width,
                height: 28.0,
            },
            HAlign::Center,
            Some(16.0),
            None,
        );
        push_label(
            labels,
            t("proxy_modal.description_1"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 42.0,
                width: card.width - 40.0,
                height: 16.0,
            },
            HAlign::Left,
            Some(11.0),
            Some([180, 180, 195]),
        );
        push_label(
            labels,
            t("proxy_modal.description_2"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 58.0,
                width: card.width - 40.0,
                height: 16.0,
            },
            HAlign::Left,
            Some(11.0),
            Some([180, 180, 195]),
        );

        push_label(
            labels,
            t("proxy_modal.resolution"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 74.0,
                width: card.width - 40.0,
                height: 18.0,
            },
            HAlign::Left,
            Some(12.0),
            Some([180, 180, 195]),
        );

        let preset_y = card.y + 92.0;
        let preset_w = 70.0;
        let preset_h = 34.0;
        for (index, preset) in PRESETS.iter().enumerate() {
            let rect = Rect {
                x: card.x + 20.0 + index as f32 * (preset_w + 10.0),
                y: preset_y,
                width: preset_w,
                height: preset_h,
            };
            let label = match preset {
                360 => "360p",
                480 => "480p",
                720 => "720p",
                1080 => "1080p",
                1440 => "1440p",
                _ => "Proxy",
            };
            push_button(
                overlay_quads,
                labels,
                rect,
                label,
                self.selected_max_height == *preset,
                false,
                self.keyboard_focus == 0 && self.selected_max_height == *preset,
            );
        }

        push_label(
            labels,
            t("proxy_modal.target"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 132.0,
                width: 120.0,
                height: 20.0,
            },
            HAlign::Left,
            Some(12.0),
            Some([180, 180, 195]),
        );
        push_label(
            labels,
            &self.target_text,
            Rect {
                x: card.x + 132.0,
                y: card.y + 132.0,
                width: 180.0,
                height: 20.0,
            },
            HAlign::Left,
            Some(12.0),
            None,
        );

        push_label(
            labels,
            t("proxy_modal.encoder"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 174.0,
                width: 110.0,
                height: 30.0,
            },
            HAlign::Left,
            Some(12.0),
            Some([180, 180, 195]),
        );
        push_button(
            overlay_quads,
            labels,
            Self::encoder_rect(card),
            self.encoder.label(),
            self.encoder_menu_open,
            false,
            self.keyboard_focus == 1,
        );
        push_label(
            labels,
            if self.encoder_menu_open { "▲" } else { "▼" },
            Rect {
                x: Self::encoder_rect(card).x + Self::encoder_rect(card).width - 28.0,
                y: Self::encoder_rect(card).y,
                width: 24.0,
                height: Self::encoder_rect(card).height,
            },
            HAlign::Center,
            Some(10.0),
            None,
        );

        push_label(
            labels,
            t("proxy_modal.quality"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 212.0,
                width: card.width - 40.0,
                height: 18.0,
            },
            HAlign::Left,
            Some(12.0),
            Some([180, 180, 195]),
        );

        let quality_y = card.y + 230.0;
        let btn_size = 36.0;
        let value_w = 100.0;
        let adjustable_quality = self.encoder == crate::video_proxy::ProxyEncoder::H264;
        if adjustable_quality {
            push_button(
                overlay_quads,
                labels,
                Rect {
                    x: card.x + 20.0,
                    y: quality_y,
                    width: btn_size,
                    height: 30.0,
                },
                "-",
                false,
                false,
                self.keyboard_focus == 2,
            );
        }
        push_value_box(
            overlay_quads,
            labels,
            Rect {
                x: card.x + 20.0 + if adjustable_quality { btn_size } else { 0.0 },
                y: quality_y,
                width: if adjustable_quality {
                    value_w
                } else {
                    value_w + btn_size * 2.0
                },
                height: 30.0,
            },
            if adjustable_quality {
                &self.crf_text
            } else {
                self.encoder.label()
            },
        );
        if adjustable_quality {
            push_button(
                overlay_quads,
                labels,
                Rect {
                    x: card.x + 20.0 + btn_size + value_w,
                    y: quality_y,
                    width: btn_size,
                    height: 30.0,
                },
                "+",
                false,
                false,
                self.keyboard_focus == 2,
            );
        }
        push_label(
            labels,
            if adjustable_quality {
                t("proxy_modal.quality_hint")
            } else {
                t("proxy_modal.quality_fixed_hint")
            },
            Rect {
                x: card.x + 20.0,
                y: quality_y + 34.0,
                width: card.width - 40.0,
                height: 18.0,
            },
            HAlign::Left,
            Some(10.0),
            Some([150, 150, 165]),
        );

        push_button(
            overlay_quads,
            labels,
            Rect {
                x: card.x + (card.width - 180.0) / 2.0,
                y: card.y + CARD_H - 52.0,
                width: 180.0,
                height: 40.0,
            },
            t("proxy_modal.create"),
            false,
            true,
            self.keyboard_focus == 3,
        );

        if self.encoder_menu_open {
            for (index, encoder) in crate::video_proxy::ProxyEncoder::ALL.iter().enumerate() {
                push_button(
                    overlay_quads,
                    labels,
                    Self::encoder_option_rect(card, index),
                    encoder.label(),
                    self.encoder == *encoder,
                    false,
                    false,
                );
            }
        }
    }
}

fn push_button<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    selected: bool,
    primary: bool,
    focused: bool,
) {
    let (color, color_bottom, border_color) = if primary {
        (
            [0.30, 0.55, 0.30, 1.0],
            [0.22, 0.45, 0.22, 1.0],
            [0.40, 0.65, 0.40, 0.8],
        )
    } else if selected {
        (
            [0.30, 0.28, 0.60, 1.0],
            [0.24, 0.22, 0.50, 1.0],
            [0.50, 0.45, 0.85, 0.9],
        )
    } else {
        (
            [0.15, 0.15, 0.18, 1.0],
            [0.13, 0.13, 0.16, 1.0],
            [0.30, 0.30, 0.36, 0.5],
        )
    };

    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom,
        border_color: if focused {
            [1.0, 0.84, 0.28, 1.0]
        } else {
            border_color
        },
        border_width: if focused { 2.5 } else { 1.0 },
        border_radius: 7.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, if primary { 0.3 } else { 0.0 }],
        shadow_blur: if primary { 4.0 } else { 0.0 },
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    push_label(labels, text, rect, HAlign::Center, Some(13.0), None);
}

fn push_value_box<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
) {
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
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
    push_label(labels, text, rect, HAlign::Center, Some(12.0), None);
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    font_size: Option<f32>,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: font_size,
        color_override: color,
        font_family_override: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_resolution_proxy_presets_are_selectable() {
        let mut modal = ProxyModal::new(1920, 1080);
        modal.adjust_resolution(-1);
        modal.adjust_resolution(-1);
        assert_eq!(modal.selected_max_height, 480);
        assert_eq!((modal.target_width, modal.target_height), (854, 480));
        modal.adjust_resolution(-1);
        assert_eq!(modal.selected_max_height, 360);
        assert_eq!((modal.target_width, modal.target_height), (640, 360));
    }

    #[test]
    fn encoder_dropdown_defaults_to_prores_and_selects_mjpeg() {
        let mut modal = ProxyModal::new(1920, 1080);
        assert_eq!(modal.encoder, crate::video_proxy::ProxyEncoder::ProResProxy);

        let card = ProxyModal::card_rect(800.0, 600.0);
        let trigger = ProxyModal::encoder_rect(card);
        let _ = modal.handle_event(
            &UiEvent::MousePress {
                x: trigger.x + 1.0,
                y: trigger.y + 1.0,
            },
            800.0,
            600.0,
        );
        assert_eq!(
            modal.encoder_accessibility_state(),
            Some((crate::video_proxy::ProxyEncoder::ProResProxy, true))
        );
        let mjpeg = ProxyModal::encoder_option_rect(card, 1);
        let _ = modal.handle_event(
            &UiEvent::MousePress {
                x: mjpeg.x + 1.0,
                y: mjpeg.y + 1.0,
            },
            800.0,
            600.0,
        );

        assert_eq!(modal.encoder, crate::video_proxy::ProxyEncoder::Mjpeg);
        assert!(!modal.encoder_menu_open);
        assert_eq!(
            modal.encoder_accessibility_state(),
            Some((crate::video_proxy::ProxyEncoder::Mjpeg, false))
        );
    }
}
