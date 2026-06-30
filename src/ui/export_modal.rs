use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};

use crate::i18n::t;

const CARD_W: f32 = 430.0;
const CARD_H: f32 = 424.0;

#[derive(Clone, Copy, PartialEq)]
enum ActiveField {
    Width,
    Height,
    InstrumentalAudio,
}

#[derive(Clone, Copy)]
enum DimensionField {
    Width,
    Height,
}

fn sanitize_dimension(value: u32) -> u32 {
    let clamped = value.clamp(16, 8192);
    if clamped % 2 == 0 {
        clamped
    } else {
        (clamped + 1).min(8192)
    }
}

pub struct ExportModal {
    pub fps: u32,
    pub fps_text: String,
    pub br_scale: f32,
    pub br_scale_text: String,
    pub karaoke_text_scale: f32,
    pub karaoke_text_scale_text: String,
    pub export_width: u32,
    pub export_width_text: String,
    pub export_height: u32,
    pub export_height_text: String,
    pub instrumental_audio_path: String,
    pub double_export_instrumental: bool,
    active_field: Option<ActiveField>,
    replace_active_field: bool,
}

pub enum ExportModalResult {
    Consumed,
    Close,
    Export {
        fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        export_width: u32,
        export_height: u32,
        instrumental_audio_path: Option<std::path::PathBuf>,
        double_export_instrumental: bool,
    },
    PickInstrumentalAudio,
}

impl ExportModal {
    pub fn new(video_width: u32, video_height: u32) -> Self {
        let export_width = sanitize_dimension(video_width);
        let export_height = sanitize_dimension(video_height);
        let fps = crate::constants::DEFAULT_EXPORT_FPS;
        Self {
            fps,
            fps_text: fps.to_string(),
            br_scale: 1.0,
            br_scale_text: "100%".to_string(),
            karaoke_text_scale: 1.0,
            karaoke_text_scale_text: "100%".to_string(),
            export_width,
            export_width_text: export_width.to_string(),
            export_height,
            export_height_text: export_height.to_string(),
            instrumental_audio_path: String::new(),
            double_export_instrumental: false,
            active_field: None,
            replace_active_field: false,
        }
    }

    pub fn set_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.instrumental_audio_path = path.into();
        self.active_field = Some(ActiveField::InstrumentalAudio);
        self.replace_active_field = false;
    }

    fn update_scale_text(&mut self) {
        self.br_scale_text = format!("{}%", (self.br_scale * 100.0).round() as u32);
    }

    fn update_karaoke_text_scale_text(&mut self) {
        self.karaoke_text_scale_text =
            format!("{}%", (self.karaoke_text_scale * 100.0).round() as u32);
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn instrumental_audio_rects(card: Rect) -> (Rect, Rect) {
        let audio_y = card.y + 266.0;
        let browse_w = 88.0;
        let audio_gap = 8.0;
        let audio_field_w = card.width - 40.0 - browse_w - audio_gap;
        let audio_rect = Rect {
            x: card.x + 20.0,
            y: audio_y,
            width: audio_field_w,
            height: 30.0,
        };
        let browse_rect = Rect {
            x: audio_rect.x + audio_rect.width + audio_gap,
            y: audio_y,
            width: browse_w,
            height: 30.0,
        };
        (audio_rect, browse_rect)
    }

    fn double_export_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 20.0,
            y: card.y + 318.0,
            width: card.width - 40.0,
            height: 24.0,
        }
    }

    fn export_width(&self) -> u32 {
        let value = self
            .export_width_text
            .parse::<u32>()
            .ok()
            .filter(|&v| v >= 16)
            .unwrap_or(self.export_width);
        sanitize_dimension(value)
    }

    fn export_height(&self) -> u32 {
        let value = self
            .export_height_text
            .parse::<u32>()
            .ok()
            .filter(|&v| v >= 16)
            .unwrap_or(self.export_height);
        sanitize_dimension(value)
    }

    fn instrumental_audio_path(&self) -> Option<std::path::PathBuf> {
        let path = self
            .instrumental_audio_path
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim();
        if path.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(path))
        }
    }

    fn export_result(&mut self) -> ExportModalResult {
        self.export_width = self.export_width();
        self.export_height = self.export_height();
        self.export_width_text = self.export_width.to_string();
        self.export_height_text = self.export_height.to_string();
        let instrumental_audio_path = self.instrumental_audio_path();
        ExportModalResult::Export {
            fps: self.fps as f64,
            br_scale: self.br_scale,
            karaoke_text_scale: self.karaoke_text_scale,
            export_width: self.export_width,
            export_height: self.export_height,
            instrumental_audio_path,
            double_export_instrumental: self.double_export_instrumental,
        }
    }

    fn finish_active_field(&mut self) {
        self.active_field = None;
        self.replace_active_field = false;
        self.export_width = self.export_width();
        self.export_height = self.export_height();
        self.export_width_text = self.export_width.to_string();
        self.export_height_text = self.export_height.to_string();
    }

    fn handle_dimension_field_key(&mut self, field: DimensionField, text: &str) -> bool {
        let target = match field {
            DimensionField::Width => &mut self.export_width_text,
            DimensionField::Height => &mut self.export_height_text,
        };

        if text == "\x08" || text == "\x7f" {
            if self.replace_active_field {
                target.clear();
                self.replace_active_field = false;
                return true;
            }
            target.pop();
            return true;
        }

        if text.chars().all(|c| c.is_ascii_digit())
            && (self.replace_active_field || target.len() < 5)
        {
            if self.replace_active_field {
                target.clear();
                self.replace_active_field = false;
            }
            if target.as_str() == "0" {
                target.clear();
            }
            target.push_str(text);
            return true;
        }

        true
    }

    fn handle_instrumental_audio_key(&mut self, text: &str) -> bool {
        if text == "\x08" || text == "\x7f" {
            if self.replace_active_field {
                self.instrumental_audio_path.clear();
                self.replace_active_field = false;
                return true;
            }
            self.instrumental_audio_path.pop();
            return true;
        }

        if text.chars().all(|c| !c.is_control())
            && self.instrumental_audio_path.len() + text.len() <= 1024
        {
            if self.replace_active_field {
                self.instrumental_audio_path.clear();
                self.replace_active_field = false;
            }
            self.instrumental_audio_path.push_str(text);
            return true;
        }

        true
    }

    fn handle_active_field_key(&mut self, text: &str) -> bool {
        let Some(active_field) = self.active_field else {
            return false;
        };

        if text == "\r" || text == "\n" {
            self.finish_active_field();
            return true;
        }

        match active_field {
            ActiveField::Width => self.handle_dimension_field_key(DimensionField::Width, text),
            ActiveField::Height => self.handle_dimension_field_key(DimensionField::Height, text),
            ActiveField::InstrumentalAudio => self.handle_instrumental_audio_key(text),
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
                if self.handle_active_field_key(text) {
                    return ExportModalResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    return self.export_result();
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
                    self.fps = self.fps.saturating_sub(1).max(1);
                    self.fps_text = self.fps.to_string();
                    return ExportModalResult::Consumed;
                }
                if plus_rect.contains(*x, *y) {
                    self.fps = self.fps.saturating_add(1).min(480);
                    self.fps_text = self.fps.to_string();
                    return ExportModalResult::Consumed;
                }

                let resolution_y = card.y + 98.0;
                let field_w = 118.0;
                let field_h = 30.0;
                let width_rect = Rect {
                    x: card.x + 20.0,
                    y: resolution_y,
                    width: field_w,
                    height: field_h,
                };
                let height_rect = Rect {
                    x: card.x + 170.0,
                    y: resolution_y,
                    width: field_w,
                    height: field_h,
                };
                if width_rect.contains(*x, *y) {
                    self.active_field = Some(ActiveField::Width);
                    self.replace_active_field = true;
                    return ExportModalResult::Consumed;
                }
                if height_rect.contains(*x, *y) {
                    self.active_field = Some(ActiveField::Height);
                    self.replace_active_field = true;
                    return ExportModalResult::Consumed;
                }

                self.active_field = None;
                self.replace_active_field = false;

                let scale_y = card.y + 154.0;
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

                let karaoke_scale_y = card.y + 210.0;
                let karaoke_scale_minus = Rect {
                    x: card.x + 20.0,
                    y: karaoke_scale_y,
                    width: btn_sz,
                    height: 30.0,
                };
                let karaoke_scale_plus = Rect {
                    x: card.x + 20.0 + btn_sz + val_w,
                    y: karaoke_scale_y,
                    width: btn_sz,
                    height: 30.0,
                };
                if karaoke_scale_minus.contains(*x, *y) {
                    self.karaoke_text_scale = (self.karaoke_text_scale - 0.10).max(0.5);
                    self.update_karaoke_text_scale_text();
                    return ExportModalResult::Consumed;
                }
                if karaoke_scale_plus.contains(*x, *y) {
                    self.karaoke_text_scale = (self.karaoke_text_scale + 0.10).min(2.0);
                    self.update_karaoke_text_scale_text();
                    return ExportModalResult::Consumed;
                }

                let (audio_rect, browse_rect) = Self::instrumental_audio_rects(card);
                if audio_rect.contains(*x, *y) {
                    self.active_field = Some(ActiveField::InstrumentalAudio);
                    self.replace_active_field = true;
                    return ExportModalResult::Consumed;
                }
                if browse_rect.contains(*x, *y) {
                    self.active_field = None;
                    self.replace_active_field = false;
                    return ExportModalResult::PickInstrumentalAudio;
                }

                if Self::double_export_rect(card).contains(*x, *y) {
                    self.double_export_instrumental = !self.double_export_instrumental;
                    self.active_field = None;
                    self.replace_active_field = false;
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
                    return self.export_result();
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

        // --- Resolution section ---
        labels.push(LabelInfo {
            text: t("export_modal.resolution"),
            bounds: Rect {
                x: fx,
                y: card.y + 78.0,
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

        let resolution_y = card.y + 98.0;
        let field_w = 118.0;
        let field_h = 30.0;
        let width_rect = Rect {
            x: fx,
            y: resolution_y,
            width: field_w,
            height: field_h,
        };
        let height_rect = Rect {
            x: card.x + 170.0,
            y: resolution_y,
            width: field_w,
            height: field_h,
        };
        for (rect, active) in [
            (width_rect, self.active_field == Some(ActiveField::Width)),
            (height_rect, self.active_field == Some(ActiveField::Height)),
        ] {
            overlay_quads.push(QuadInstance {
                rect: [rect.x, rect.y, rect.width, rect.height],
                color: [0.08, 0.08, 0.10, 1.0],
                color_bottom: [0.08, 0.08, 0.10, 1.0],
                border_color: if active {
                    [0.50, 0.65, 0.95, 0.9]
                } else {
                    [0.30, 0.30, 0.36, 0.5]
                },
                border_width: if active { 1.5 } else { 1.0 },
                border_radius: 4.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        labels.push(LabelInfo {
            text: &self.export_width_text,
            bounds: width_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: "x",
            bounds: Rect {
                x: fx + field_w,
                y: resolution_y,
                width: 32.0,
                height: field_h,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([180, 180, 195]),
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: &self.export_height_text,
            bounds: height_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });

        labels.push(LabelInfo {
            text: t("export_modal.br_scale"),
            bounds: Rect {
                x: fx,
                y: card.y + 134.0,
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

        let scale_y = card.y + 154.0;
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

        labels.push(LabelInfo {
            text: t("export_modal.karaoke_text_scale"),
            bounds: Rect {
                x: fx,
                y: card.y + 190.0,
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

        let karaoke_scale_y = card.y + 210.0;
        overlay_quads.push(QuadInstance {
            rect: [fx, karaoke_scale_y, btn_size, 30.0],
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
                y: karaoke_scale_y,
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
            rect: [fx + btn_size, karaoke_scale_y, value_w, 30.0],
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
            text: &self.karaoke_text_scale_text,
            bounds: Rect {
                x: fx + btn_size,
                y: karaoke_scale_y,
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
            rect: [fx + btn_size + value_w, karaoke_scale_y, btn_size, 30.0],
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
                y: karaoke_scale_y,
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

        // --- Optional instrumental audio ---
        labels.push(LabelInfo {
            text: t("export_modal.instrumental_audio"),
            bounds: Rect {
                x: fx,
                y: card.y + 246.0,
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

        let (audio_rect, browse_rect) = Self::instrumental_audio_rects(card);
        let audio_active = self.active_field == Some(ActiveField::InstrumentalAudio);
        let audio_text = if self.instrumental_audio_path.is_empty() {
            t("export_modal.instrumental_audio_placeholder")
        } else {
            &self.instrumental_audio_path
        };
        overlay_quads.push(QuadInstance {
            rect: [
                audio_rect.x,
                audio_rect.y,
                audio_rect.width,
                audio_rect.height,
            ],
            color: [0.08, 0.08, 0.10, 1.0],
            color_bottom: [0.08, 0.08, 0.10, 1.0],
            border_color: if audio_active {
                [0.50, 0.65, 0.95, 0.9]
            } else {
                [0.30, 0.30, 0.36, 0.5]
            },
            border_width: if audio_active { 1.5 } else { 1.0 },
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        labels.push(LabelInfo {
            text: audio_text,
            bounds: audio_rect,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 8.0,
            font_size_override: Some(11.0),
            color_override: if self.instrumental_audio_path.is_empty() {
                Some([130, 130, 145])
            } else {
                None
            },
            font_family_override: None,
        });
        overlay_quads.push(QuadInstance {
            rect: [
                browse_rect.x,
                browse_rect.y,
                browse_rect.width,
                browse_rect.height,
            ],
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
            text: t("export_modal.browse"),
            bounds: browse_rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: None,
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: t("export_modal.instrumental_audio_hint"),
            bounds: Rect {
                x: fx,
                y: card.y + 298.0,
                width: fw,
                height: 18.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(10.0),
            color_override: Some([145, 145, 160]),
            font_family_override: None,
        });

        let checkbox = Self::double_export_rect(card);
        let box_size = 16.0;
        let box_rect = Rect {
            x: checkbox.x,
            y: checkbox.y + (checkbox.height - box_size) / 2.0,
            width: box_size,
            height: box_size,
        };
        overlay_quads.push(QuadInstance {
            rect: [box_rect.x, box_rect.y, box_rect.width, box_rect.height],
            color: if self.double_export_instrumental {
                [0.30, 0.45, 0.85, 1.0]
            } else {
                [0.08, 0.08, 0.10, 1.0]
            },
            color_bottom: if self.double_export_instrumental {
                [0.22, 0.34, 0.70, 1.0]
            } else {
                [0.08, 0.08, 0.10, 1.0]
            },
            border_color: if self.double_export_instrumental {
                [0.55, 0.68, 1.0, 0.9]
            } else {
                [0.30, 0.30, 0.36, 0.6]
            },
            border_width: 1.0,
            border_radius: 3.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        if self.double_export_instrumental {
            labels.push(LabelInfo {
                text: "✓",
                bounds: box_rect,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(13.0),
                color_override: Some([245, 245, 255]),
                font_family_override: None,
            });
        }
        labels.push(LabelInfo {
            text: t("export_modal.double_export_instrumental"),
            bounds: Rect {
                x: checkbox.x + box_size + 8.0,
                y: checkbox.y,
                width: checkbox.width - box_size - 8.0,
                height: checkbox.height,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([190, 190, 205]),
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
