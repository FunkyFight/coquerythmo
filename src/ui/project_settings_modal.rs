use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub const PROJECT_SETTINGS_W: f32 = 520.0;
pub const PROJECT_SETTINGS_H: f32 = 220.0;

pub struct ProjectSettingsModal {
    pub instrumental_audio_path: String,
}

pub enum ProjectSettingsModalResult {
    Consumed,
    Close,
    PickInstrumentalAudio,
    Save {
        instrumental_audio_path: Option<String>,
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
    pub fn new(path: Option<String>) -> Self {
        Self {
            instrumental_audio_path: path.unwrap_or_default(),
        }
    }

    pub fn set_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.instrumental_audio_path = path.into();
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

                let save_rect = save_rect(card);
                if save_rect.contains(*x, *y) {
                    let path = self.instrumental_audio_path.trim();
                    return ProjectSettingsModalResult::Save {
                        instrumental_audio_path: (!path.is_empty()).then(|| path.to_string()),
                    };
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

fn save_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + (card.width - 140.0) / 2.0,
        y: card.y + PROJECT_SETTINGS_H - 48.0,
        width: 140.0,
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
