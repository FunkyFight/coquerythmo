use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub const PROJECT_SETTINGS_W: f32 = 520.0;
pub const PROJECT_SETTINGS_H: f32 = 270.0;

pub struct ProjectSettingsModal {
    pub instrumental_audio_path: String,
    pub highlight_read_word: bool,
    keyboard_focus: usize,
}

pub enum ProjectSettingsModalResult {
    Consumed,
    Close,
    PickInstrumentalAudio,
    Save {
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
    },
}

pub fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: (screen_w - PROJECT_SETTINGS_W) / 2.0,
        y: (screen_h - PROJECT_SETTINGS_H) / 2.0,
        width: PROJECT_SETTINGS_W,
        height: PROJECT_SETTINGS_H,
    }
}

impl ProjectSettingsModal {
    pub fn new(path: Option<String>, highlight_read_word: bool) -> Self {
        Self {
            instrumental_audio_path: path.unwrap_or_default(),
            highlight_read_word,
            keyboard_focus: 0,
        }
    }

    pub fn set_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.instrumental_audio_path = path.into();
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => t("project_settings.browse").to_string(),
            1 => t("project_settings.clear").to_string(),
            2 => format!(
                "{}, {}",
                t("project_settings.highlight_read_word"),
                if self.highlight_read_word {
                    t("accessibility.checked")
                } else {
                    t("accessibility.unchecked")
                }
            ),
            3 => t("settings.save").to_string(),
            _ => t("project_settings.close").to_string(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ProjectSettingsModalResult {
        let card = card_rect(screen_w, screen_h);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => ProjectSettingsModalResult::Close,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.keyboard_focus = if text == "\t" {
                    (self.keyboard_focus + 1) % 5
                } else {
                    (self.keyboard_focus + 4) % 5
                };
                ProjectSettingsModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                match self.keyboard_focus {
                    0 => ProjectSettingsModalResult::PickInstrumentalAudio,
                    1 => {
                        self.instrumental_audio_path.clear();
                        ProjectSettingsModalResult::Consumed
                    }
                    2 => {
                        self.highlight_read_word = !self.highlight_read_word;
                        ProjectSettingsModalResult::Consumed
                    }
                    3 => {
                        let path = self.instrumental_audio_path.trim();
                        ProjectSettingsModalResult::Save {
                            instrumental_audio_path: (!path.is_empty()).then(|| path.to_string()),
                            highlight_read_word: self.highlight_read_word,
                        }
                    }
                    _ => ProjectSettingsModalResult::Close,
                }
            }
            UiEvent::CursorUp | UiEvent::CursorLeft => {
                self.keyboard_focus = (self.keyboard_focus + 4) % 5;
                ProjectSettingsModalResult::Consumed
            }
            UiEvent::CursorDown | UiEvent::CursorRight => {
                self.keyboard_focus = (self.keyboard_focus + 1) % 5;
                ProjectSettingsModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ProjectSettingsModalResult::Close;
                }

                let browse_rect = browse_rect(card);
                if browse_rect.contains(*x, *y) {
                    return ProjectSettingsModalResult::PickInstrumentalAudio;
                }

                let clear_rect = clear_rect(card);
                if clear_rect.contains(*x, *y) {
                    self.instrumental_audio_path.clear();
                    return ProjectSettingsModalResult::Consumed;
                }

                if highlight_word_rect(card).contains(*x, *y) {
                    self.highlight_read_word = !self.highlight_read_word;
                    return ProjectSettingsModalResult::Consumed;
                }

                let save_rect = save_rect(card);
                if save_rect.contains(*x, *y) {
                    let path = self.instrumental_audio_path.trim();
                    return ProjectSettingsModalResult::Save {
                        instrumental_audio_path: (!path.is_empty()).then(|| path.to_string()),
                        highlight_read_word: self.highlight_read_word,
                    };
                }

                if close_rect(card).contains(*x, *y) {
                    return ProjectSettingsModalResult::Close;
                }
                ProjectSettingsModalResult::Consumed
            }
            _ => ProjectSettingsModalResult::Consumed,
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
        push_quad(
            overlay_quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.75],
            [0.0; 4],
            0.0,
            0.0,
        );
        push_quad(
            overlay_quads,
            card,
            [0.22, 0.22, 0.26, 1.0],
            [0.45, 0.45, 0.52, 0.8],
            1.5,
            14.0,
        );

        labels.push(LabelInfo {
            text: t("project_settings.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 10.0,
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

        labels.push(LabelInfo {
            text: t("project_settings.instrumental_version"),
            bounds: Rect {
                x: card.x + 22.0,
                y: card.y + 58.0,
                width: 300.0,
                height: 20.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([180, 180, 195]),
            font_family_override: None,
        });

        let field = path_field_rect(card);
        push_quad(
            overlay_quads,
            field,
            [0.08, 0.08, 0.10, 1.0],
            [0.30, 0.30, 0.36, 0.5],
            1.0,
            4.0,
        );
        let display = if self.instrumental_audio_path.is_empty() {
            t("project_settings.no_file")
        } else {
            &self.instrumental_audio_path
        };
        labels.push(LabelInfo {
            text: display,
            bounds: Rect {
                x: field.x + 8.0,
                y: field.y,
                width: field.width - 16.0,
                height: field.height,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });

        let browse = browse_rect(card);
        push_button(overlay_quads, labels, browse, t("project_settings.browse"));
        let clear = clear_rect(card);
        push_button(overlay_quads, labels, clear, t("project_settings.clear"));
        let highlight = highlight_word_rect(card);
        push_quad(
            overlay_quads,
            Rect {
                width: 20.0,
                height: 20.0,
                ..highlight
            },
            if self.highlight_read_word {
                [0.90, 0.72, 0.12, 1.0]
            } else {
                [0.08, 0.08, 0.10, 1.0]
            },
            [0.45, 0.45, 0.52, 0.8],
            1.0,
            4.0,
        );
        labels.push(LabelInfo {
            text: t("project_settings.highlight_read_word"),
            bounds: Rect {
                x: highlight.x + 30.0,
                width: highlight.width - 30.0,
                ..highlight
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: None,
            font_family_override: None,
        });
        let save = save_rect(card);
        push_quad(
            overlay_quads,
            save,
            [0.30, 0.55, 0.30, 1.0],
            [0.40, 0.65, 0.40, 0.8],
            1.0,
            8.0,
        );
        labels.push(LabelInfo {
            text: t("settings.save"),
            bounds: save,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(14.0),
            color_override: None,
            font_family_override: None,
        });

        let close = close_rect(card);
        push_button(overlay_quads, labels, close, t("project_settings.close"));

        let focus_rect = match self.keyboard_focus {
            0 => browse,
            1 => clear,
            2 => highlight,
            3 => save,
            _ => close,
        };
        focus_outline(overlay_quads, focus_rect);
    }
}

fn path_field_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 84.0,
        width: card.width - 44.0,
        height: 32.0,
    }
}

fn browse_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 126.0,
        width: 130.0,
        height: 30.0,
    }
}

fn clear_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 162.0,
        y: card.y + 126.0,
        width: 110.0,
        height: 30.0,
    }
}

fn highlight_word_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 170.0,
        width: card.width - 44.0,
        height: 20.0,
    }
}

fn save_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + (card.width - 140.0) / 2.0,
        y: card.y + PROJECT_SETTINGS_H - 48.0,
        width: 140.0,
        height: 34.0,
    }
}

fn close_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + card.width - 132.0,
        y: card.y + PROJECT_SETTINGS_H - 48.0,
        width: 110.0,
        height: 34.0,
    }
}

fn push_button<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
) {
    push_quad(
        overlay_quads,
        rect,
        [0.15, 0.15, 0.18, 1.0],
        [0.30, 0.30, 0.36, 0.5],
        1.0,
        5.0,
    );
    labels.push(LabelInfo {
        text,
        bounds: rect,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: None,
        font_family_override: None,
    });
}

fn push_quad(
    overlay_quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    border_radius: f32,
) {
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width,
        border_radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn focus_outline(overlay_quads: &mut Vec<QuadInstance>, rect: Rect) {
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.0, 0.0, 0.0, 0.0],
        color_bottom: [0.0, 0.0, 0.0, 0.0],
        border_color: [0.38, 0.65, 1.0, 1.0],
        border_width: 2.5,
        border_radius: 8.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}
