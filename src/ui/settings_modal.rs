use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub const SETTINGS_W: f32 = 450.0;
pub const SETTINGS_H: f32 = 540.0;
pub const FONT_ITEM_H: f32 = 26.0;
pub const FONT_LIST_H: f32 = 220.0;

pub struct SettingsModal {
    pub lang: String,
    pub rythmo_font: Option<String>,
    pub scroll_speed: f32,
    pub scroll_speed_text: String,
    pub available_fonts: Vec<String>,
    pub font_scroll_offset: f32,
    pub selected_font_index: Option<usize>,
    pub hovered_font_index: Option<usize>,
    keyboard_focus: usize,
}

pub fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: (screen_w - SETTINGS_W) / 2.0,
        y: (screen_h - SETTINGS_H) / 2.0,
        width: SETTINGS_W,
        height: SETTINGS_H,
    }
}

pub enum SettingsModalResult {
    Consumed,
    Close,
    Save {
        lang: String,
        rythmo_font: Option<String>,
        scroll_speed: f32,
    },
}

impl SettingsModal {
    pub fn new(fonts: Vec<String>) -> Self {
        let cfg = crate::config::get();
        let current_font = cfg.ui.rythmo_font.clone();
        let selected_font_index = current_font
            .as_ref()
            .and_then(|name| fonts.iter().position(|f| f == name));
        let scroll_speed = cfg.ui.scroll_speed;
        Self {
            lang: cfg.lang.clone(),
            rythmo_font: current_font,
            scroll_speed,
            scroll_speed_text: format!("×{:.2}", scroll_speed),
            available_fonts: fonts,
            font_scroll_offset: 0.0,
            selected_font_index,
            hovered_font_index: None,
            keyboard_focus: 0,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> SettingsModalResult {
        let card = card_rect(screen_w, screen_h);

        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => SettingsModalResult::Close,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.keyboard_focus = if text == "\t" {
                    (self.keyboard_focus + 1) % 6
                } else {
                    (self.keyboard_focus + 5) % 6
                };
                SettingsModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => match self.keyboard_focus
            {
                2 => {
                    self.selected_font_index = None;
                    self.rythmo_font = None;
                    SettingsModalResult::Consumed
                }
                4 => SettingsModalResult::Save {
                    lang: self.lang.clone(),
                    rythmo_font: self.rythmo_font.clone(),
                    scroll_speed: self.scroll_speed,
                },
                5 => SettingsModalResult::Close,
                _ => SettingsModalResult::Consumed,
            },
            UiEvent::CursorLeft | UiEvent::CursorUp if self.keyboard_focus == 0 => {
                self.lang = match self.lang.as_str() {
                    "es-es" => "en-us",
                    "en-us" => "fr-fr",
                    _ => "fr-fr",
                }
                .into();
                SettingsModalResult::Consumed
            }
            UiEvent::CursorRight | UiEvent::CursorDown if self.keyboard_focus == 0 => {
                self.lang = match self.lang.as_str() {
                    "fr-fr" => "en-us",
                    "en-us" => "es-es",
                    _ => "es-es",
                }
                .into();
                SettingsModalResult::Consumed
            }
            UiEvent::CursorUp if self.keyboard_focus == 1 => {
                let index = self.selected_font_index.unwrap_or(0).saturating_sub(1);
                self.selected_font_index = Some(index);
                self.rythmo_font = self.available_fonts.get(index).cloned();
                SettingsModalResult::Consumed
            }
            UiEvent::CursorDown if self.keyboard_focus == 1 => {
                let index = (self.selected_font_index.map_or(0, |index| index + 1))
                    .min(self.available_fonts.len().saturating_sub(1));
                self.selected_font_index = Some(index);
                self.rythmo_font = self.available_fonts.get(index).cloned();
                SettingsModalResult::Consumed
            }
            UiEvent::CursorLeft | UiEvent::CursorDown if self.keyboard_focus == 3 => {
                self.scroll_speed = (self.scroll_speed - 0.25).max(0.25);
                self.scroll_speed_text = format!("Ã—{:.2}", self.scroll_speed);
                SettingsModalResult::Consumed
            }
            UiEvent::CursorRight | UiEvent::CursorUp if self.keyboard_focus == 3 => {
                self.scroll_speed = (self.scroll_speed + 0.25).min(4.0);
                self.scroll_speed_text = format!("Ã—{:.2}", self.scroll_speed);
                SettingsModalResult::Consumed
            }
            UiEvent::MouseMove { x, y } => {
                let list_x = card.x + 20.0;
                let list_y = card.y + 126.0;
                let list_w = card.width - 40.0;
                let list_rect = Rect {
                    x: list_x,
                    y: list_y,
                    width: list_w,
                    height: FONT_LIST_H,
                };
                if list_rect.contains(*x, *y) {
                    let rel_y = *y - list_y + self.font_scroll_offset;
                    let idx = (rel_y / FONT_ITEM_H) as usize;
                    if idx < self.available_fonts.len() {
                        self.hovered_font_index = Some(idx);
                    } else {
                        self.hovered_font_index = None;
                    }
                } else {
                    self.hovered_font_index = None;
                }
                SettingsModalResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } => {
                let list_x = card.x + 20.0;
                let list_y = card.y + 126.0;
                let list_w = card.width - 40.0;
                let list_rect = Rect {
                    x: list_x,
                    y: list_y,
                    width: list_w,
                    height: FONT_LIST_H,
                };
                if list_rect.contains(*x, *y) {
                    let max_scroll =
                        (self.available_fonts.len() as f32 * FONT_ITEM_H - FONT_LIST_H).max(0.0);
                    self.font_scroll_offset =
                        (self.font_scroll_offset - delta * 30.0).clamp(0.0, max_scroll);
                }
                SettingsModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return SettingsModalResult::Close;
                }

                // Language buttons
                let lang_y = card.y + 62.0;
                let btn_w = 90.0;
                let btn_h = 30.0;
                let fr_rect = Rect {
                    x: card.x + 20.0,
                    y: lang_y,
                    width: btn_w,
                    height: btn_h,
                };
                let en_rect = Rect {
                    x: card.x + 20.0 + btn_w + 10.0,
                    y: lang_y,
                    width: btn_w,
                    height: btn_h,
                };
                let es_rect = Rect {
                    x: card.x + 20.0 + (btn_w + 10.0) * 2.0,
                    y: lang_y,
                    width: btn_w,
                    height: btn_h,
                };

                if fr_rect.contains(*x, *y) {
                    self.lang = "fr-fr".to_string();
                    return SettingsModalResult::Consumed;
                }
                if en_rect.contains(*x, *y) {
                    self.lang = "en-us".to_string();
                    return SettingsModalResult::Consumed;
                }
                if es_rect.contains(*x, *y) {
                    self.lang = "es-es".to_string();
                    return SettingsModalResult::Consumed;
                }

                // Font list click
                let list_x = card.x + 20.0;
                let list_y = card.y + 126.0;
                let list_w = card.width - 40.0;
                let list_rect = Rect {
                    x: list_x,
                    y: list_y,
                    width: list_w,
                    height: FONT_LIST_H,
                };
                if list_rect.contains(*x, *y) {
                    let rel_y = *y - list_y + self.font_scroll_offset;
                    let idx = (rel_y / FONT_ITEM_H) as usize;
                    if idx < self.available_fonts.len() {
                        self.selected_font_index = Some(idx);
                        self.rythmo_font = Some(self.available_fonts[idx].clone());
                    }
                    return SettingsModalResult::Consumed;
                }

                // "Default font" button (reset to None)
                let default_btn_y = card.y + 126.0 + FONT_LIST_H + 6.0;
                let default_btn_rect = Rect {
                    x: list_x,
                    y: default_btn_y,
                    width: 180.0,
                    height: 26.0,
                };
                if default_btn_rect.contains(*x, *y) {
                    self.selected_font_index = None;
                    self.rythmo_font = None;
                    return SettingsModalResult::Consumed;
                }

                // Scroll speed buttons
                // list_y=card.y+126, default_btn_y=list_y+FONT_LIST_H+6, preview_y=default_btn_y+32
                // speed_label_y=preview_y+36+8, speed_y=speed_label_y+20
                let speed_y = card.y + 126.0 + FONT_LIST_H + 6.0 + 32.0 + 36.0 + 8.0 + 20.0;
                let minus_rect = Rect {
                    x: card.x + 20.0,
                    y: speed_y,
                    width: 30.0,
                    height: 26.0,
                };
                let plus_rect = Rect {
                    x: card.x + 20.0 + 30.0 + 80.0,
                    y: speed_y,
                    width: 30.0,
                    height: 26.0,
                };
                if minus_rect.contains(*x, *y) {
                    self.scroll_speed = (self.scroll_speed - 0.25).max(0.25);
                    self.scroll_speed_text = format!("×{:.2}", self.scroll_speed);
                    return SettingsModalResult::Consumed;
                }
                if plus_rect.contains(*x, *y) {
                    self.scroll_speed = (self.scroll_speed + 0.25).min(4.0);
                    self.scroll_speed_text = format!("×{:.2}", self.scroll_speed);
                    return SettingsModalResult::Consumed;
                }

                // Save button
                let save_y = card.y + SETTINGS_H - 50.0;
                let save_w = 140.0;
                let save_x = card.x + (card.width - save_w) / 2.0;
                let save_rect = Rect {
                    x: save_x,
                    y: save_y,
                    width: save_w,
                    height: 36.0,
                };
                if save_rect.contains(*x, *y) {
                    let lang = self.lang.clone();
                    let rythmo_font = self.rythmo_font.clone();
                    let scroll_speed = self.scroll_speed;
                    return SettingsModalResult::Save {
                        lang,
                        rythmo_font,
                        scroll_speed,
                    };
                }

                SettingsModalResult::Consumed
            }
            _ => SettingsModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = card_rect(screen_w, screen_h);

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
            text: t("settings.title"),
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

        // --- Language section ---
        labels.push(LabelInfo {
            text: t("settings.language"),
            bounds: Rect {
                x: card.x + 20.0,
                y: card.y + 42.0,
                width: 200.0,
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

        let lang_y = card.y + 62.0;
        let btn_w = 90.0;
        let btn_h = 30.0;
        let is_fr = self.lang.starts_with("fr");
        let is_en = self.lang.starts_with("en");
        let is_es = self.lang.starts_with("es");

        // Français button
        let fr_bg = if is_fr {
            [0.30, 0.28, 0.60, 1.0]
        } else {
            [0.15, 0.15, 0.18, 1.0]
        };
        let fr_border = if is_fr {
            [0.50, 0.45, 0.85, 0.9]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        };
        overlay_quads.push(QuadInstance {
            rect: [card.x + 20.0, lang_y, btn_w, btn_h],
            color: fr_bg,
            color_bottom: fr_bg,
            border_color: fr_border,
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "Français",
            bounds: Rect {
                x: card.x + 20.0,
                y: lang_y,
                width: btn_w,
                height: btn_h,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: None,
            font_family_override: None,
        });

        // English button
        let en_bg = if is_en {
            [0.30, 0.28, 0.60, 1.0]
        } else {
            [0.15, 0.15, 0.18, 1.0]
        };
        let en_border = if is_en {
            [0.50, 0.45, 0.85, 0.9]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        };
        overlay_quads.push(QuadInstance {
            rect: [card.x + 20.0 + btn_w + 10.0, lang_y, btn_w, btn_h],
            color: en_bg,
            color_bottom: en_bg,
            border_color: en_border,
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "English",
            bounds: Rect {
                x: card.x + 20.0 + btn_w + 10.0,
                y: lang_y,
                width: btn_w,
                height: btn_h,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: None,
            font_family_override: None,
        });

        // Spanish button
        let es_bg = if is_es {
            [0.30, 0.28, 0.60, 1.0]
        } else {
            [0.15, 0.15, 0.18, 1.0]
        };
        let es_border = if is_es {
            [0.50, 0.45, 0.85, 0.9]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        };
        overlay_quads.push(QuadInstance {
            rect: [card.x + 20.0 + (btn_w + 10.0) * 2.0, lang_y, btn_w, btn_h],
            color: es_bg,
            color_bottom: es_bg,
            border_color: es_border,
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "Español",
            bounds: Rect {
                x: card.x + 20.0 + (btn_w + 10.0) * 2.0,
                y: lang_y,
                width: btn_w,
                height: btn_h,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: None,
            font_family_override: None,
        });

        // Restart required note
        labels.push(LabelInfo {
            text: t("settings.restart_required"),
            bounds: Rect {
                x: card.x + 20.0 + (btn_w + 10.0) * 3.0,
                y: lang_y,
                width: card.width - 40.0 - (btn_w + 10.0) * 3.0,
                height: btn_h,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(10.0),
            color_override: Some([130, 130, 145]),
            font_family_override: None,
        });

        // --- Font section ---
        labels.push(LabelInfo {
            text: t("settings.rythmo_font"),
            bounds: Rect {
                x: card.x + 20.0,
                y: card.y + 102.0,
                width: 300.0,
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

        // Font list background
        let list_x = card.x + 20.0;
        let list_y = card.y + 126.0;
        let list_w = card.width - 40.0;
        let list_h = FONT_LIST_H;
        overlay_quads.push(QuadInstance {
            rect: [list_x, list_y, list_w, list_h],
            color: [0.08, 0.08, 0.10, 1.0],
            color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Font list items (virtual scroll)
        let item_h = FONT_ITEM_H;
        let first_visible = (self.font_scroll_offset / item_h) as usize;
        let visible_count = (list_h / item_h) as usize + 2;
        for i in first_visible
            ..self
                .available_fonts
                .len()
                .min(first_visible + visible_count)
        {
            let iy = list_y + (i as f32 * item_h) - self.font_scroll_offset;
            if iy + item_h < list_y || iy > list_y + list_h {
                continue;
            }

            // Highlight selected or hovered
            if self.selected_font_index == Some(i) {
                overlay_quads.push(QuadInstance {
                    rect: [
                        list_x + 2.0,
                        iy.max(list_y),
                        list_w - 4.0,
                        item_h.min(list_y + list_h - iy),
                    ],
                    color: [0.30, 0.28, 0.55, 0.8],
                    color_bottom: [0.30, 0.28, 0.55, 0.8],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 3.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            } else if self.hovered_font_index == Some(i) {
                overlay_quads.push(QuadInstance {
                    rect: [
                        list_x + 2.0,
                        iy.max(list_y),
                        list_w - 4.0,
                        item_h.min(list_y + list_h - iy),
                    ],
                    color: [1.0, 1.0, 1.0, 0.06],
                    color_bottom: [1.0, 1.0, 1.0, 0.06],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 3.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }

            // Clip: only show label if it's within the list bounds
            if iy >= list_y - item_h && iy < list_y + list_h {
                labels.push(LabelInfo {
                    text: &self.available_fonts[i],
                    bounds: Rect {
                        x: list_x + 8.0,
                        y: iy,
                        width: list_w - 16.0,
                        height: item_h,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 0.0,
                    font_size_override: Some(12.0),
                    color_override: None,
                    font_family_override: None,
                });
            }
        }

        // Default font button
        let default_btn_y = list_y + list_h + 6.0;
        let default_selected = self.rythmo_font.is_none();
        let default_bg = if default_selected {
            [0.30, 0.28, 0.60, 1.0]
        } else {
            [0.15, 0.15, 0.18, 1.0]
        };
        let default_border = if default_selected {
            [0.50, 0.45, 0.85, 0.9]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        };
        overlay_quads.push(QuadInstance {
            rect: [list_x, default_btn_y, 180.0, 26.0],
            color: default_bg,
            color_bottom: default_bg,
            border_color: default_border,
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: t("settings.default_font"),
            bounds: Rect {
                x: list_x,
                y: default_btn_y,
                width: 180.0,
                height: 26.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: None,
            font_family_override: None,
        });

        // Preview area
        let preview_y = default_btn_y + 32.0;
        let preview_h = 36.0;
        overlay_quads.push(QuadInstance {
            rect: [list_x, preview_y, list_w, preview_h],
            color: [0.12, 0.12, 0.15, 1.0],
            color_bottom: [0.12, 0.12, 0.15, 1.0],
            border_color: [0.30, 0.30, 0.36, 0.3],
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: "Abc 123 Àéîôù — The quick brown fox",
            bounds: Rect {
                x: list_x + 8.0,
                y: preview_y,
                width: list_w - 16.0,
                height: preview_h,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: None,
            font_family_override: self.rythmo_font.as_deref(),
        });

        // --- Scroll speed section ---
        let speed_label_y = preview_y + preview_h + 8.0;
        labels.push(LabelInfo {
            text: t("settings.scroll_speed"),
            bounds: Rect {
                x: card.x + 20.0,
                y: speed_label_y,
                width: 300.0,
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

        let speed_y = speed_label_y + 20.0;
        let btn_size = 30.0;
        let value_w = 80.0;

        // Minus button
        overlay_quads.push(QuadInstance {
            rect: [card.x + 20.0, speed_y, btn_size, 26.0],
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
            text: "−",
            bounds: Rect {
                x: card.x + 20.0,
                y: speed_y,
                width: btn_size,
                height: 26.0,
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
            rect: [card.x + 20.0 + btn_size, speed_y, value_w, 26.0],
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
            text: &self.scroll_speed_text,
            bounds: Rect {
                x: card.x + 20.0 + btn_size,
                y: speed_y,
                width: value_w,
                height: 26.0,
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
            rect: [card.x + 20.0 + btn_size + value_w, speed_y, btn_size, 26.0],
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
                x: card.x + 20.0 + btn_size + value_w,
                y: speed_y,
                width: btn_size,
                height: 26.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });

        // Save button
        let save_w = 140.0;
        let save_h = 36.0;
        let save_x = card.x + (card.width - save_w) / 2.0;
        let save_y = card.y + SETTINGS_H - 50.0;
        overlay_quads.push(QuadInstance {
            rect: [save_x, save_y, save_w, save_h],
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
            text: t("settings.save"),
            bounds: Rect {
                x: save_x,
                y: save_y,
                width: save_w,
                height: save_h,
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
