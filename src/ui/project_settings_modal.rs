use super::font_dropdown::FontDropdown;
use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

pub const PROJECT_SETTINGS_W: f32 = 520.0;
pub const PROJECT_SETTINGS_H: f32 = 470.0;
const CONTROL_COUNT: usize = 10;

pub struct ProjectSettingsModal {
    font: FontDropdown,
    pub scroll_speed: f32,
    scroll_speed_text: String,
    pub reading_bar_offset_percent: f32,
    reading_bar_offset_text: String,
    pub instrumental_audio_path: String,
    pub highlight_read_word: bool,
    pub scrolling_text_uses_character_color: bool,
    pub show_text_emotion_lanes: bool,
    keyboard_focus: usize,
}

pub enum ProjectSettingsModalResult {
    Consumed,
    Close,
    PickInstrumentalAudio,
    Save {
        rythmo_font: Option<String>,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
        show_text_emotion_lanes: bool,
    },
}

impl ProjectSettingsModal {
    pub fn new(
        fonts: Vec<String>,
        rythmo_font: Option<String>,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
        show_text_emotion_lanes: bool,
    ) -> Self {
        let scroll_speed = scroll_speed.clamp(0.25, 4.0);
        let reading_bar_offset_percent = reading_bar_offset_percent.clamp(-50.0, 50.0);
        Self {
            font: FontDropdown::new(fonts, rythmo_font),
            scroll_speed,
            scroll_speed_text: format!("×{scroll_speed:.2}"),
            reading_bar_offset_percent,
            reading_bar_offset_text: format!("{reading_bar_offset_percent:+.0} %"),
            instrumental_audio_path: path.unwrap_or_default(),
            highlight_read_word,
            scrolling_text_uses_character_color,
            show_text_emotion_lanes,
            keyboard_focus: 0,
        }
    }

    pub fn set_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.instrumental_audio_path = path.into();
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => format!(
                "{}, {}",
                t("settings.rythmo_font"),
                self.font
                    .selected()
                    .unwrap_or_else(|| t("settings.default_font"))
            ),
            1 => format!("{} {}", t("settings.scroll_speed"), self.scroll_speed_text),
            2 => format!(
                "{} {}",
                t("settings.reading_bar_offset"),
                self.reading_bar_offset_text
            ),
            3 => format!(
                "{}, {}",
                t("project_settings.instrumental_version"),
                if self.instrumental_audio_path.trim().is_empty() {
                    t("accessibility.unchecked")
                } else {
                    self.instrumental_audio_path.as_str()
                }
            ),
            4 => t("project_settings.clear").to_string(),
            5 => toggle_label(
                "project_settings.highlight_read_word",
                self.highlight_read_word,
            ),
            6 => toggle_label(
                "project_settings.scrolling_text_character_color",
                self.scrolling_text_uses_character_color,
            ),
            7 => toggle_label(
                "project_settings.show_text_emotion_lanes",
                self.show_text_emotion_lanes,
            ),
            8 => t("settings.save").to_string(),
            _ => t("project_settings.close").to_string(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ProjectSettingsModalResult {
        let card = self.card_rect(screen_w, screen_h);
        let font = font_rect(card);
        if self.keyboard_focus == 0
            && matches!(
                event,
                UiEvent::Activate
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::KeyInput { .. }
            )
            && self.font.handle_event(event, font)
        {
            return ProjectSettingsModalResult::Consumed;
        }
        if matches!(
            event,
            UiEvent::MousePress { .. }
                | UiEvent::DoubleClick { .. }
                | UiEvent::MouseMove { .. }
                | UiEvent::Scroll { .. }
        ) && self.font.handle_event(event, font)
        {
            self.keyboard_focus = 0;
            return ProjectSettingsModalResult::Consumed;
        }
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => ProjectSettingsModalResult::Close,
            UiEvent::FocusNext => self.move_focus(1),
            UiEvent::FocusPrevious => self.move_focus(-1),
            UiEvent::KeyInput { text } if text == "\t" => self.move_focus(1),
            UiEvent::KeyInput { text } if text == "\u{b}" => self.move_focus(-1),
            UiEvent::CursorUp => self.move_focus(-1),
            UiEvent::CursorDown => self.move_focus(1),
            UiEvent::CursorLeft if matches!(self.keyboard_focus, 1 | 2) => {
                self.adjust(self.keyboard_focus, -1);
                ProjectSettingsModalResult::Consumed
            }
            UiEvent::CursorRight if matches!(self.keyboard_focus, 1 | 2) => {
                self.adjust(self.keyboard_focus, 1);
                ProjectSettingsModalResult::Consumed
            }
            UiEvent::CursorLeft => self.move_focus(-1),
            UiEvent::CursorRight => self.move_focus(1),
            UiEvent::Activate => self.activate(),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                self.activate()
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ProjectSettingsModalResult::Close;
                }
                for control in 1..=2 {
                    if minus_rect(card, control, self.font.expanded_height()).contains(*x, *y) {
                        self.adjust(control, -1);
                        return ProjectSettingsModalResult::Consumed;
                    }
                    if plus_rect(card, control, self.font.expanded_height()).contains(*x, *y) {
                        self.adjust(control, 1);
                        return ProjectSettingsModalResult::Consumed;
                    }
                }
                if browse_rect(card, self.extra()).contains(*x, *y) {
                    return ProjectSettingsModalResult::PickInstrumentalAudio;
                }
                if clear_rect(card, self.extra()).contains(*x, *y) {
                    self.instrumental_audio_path.clear();
                    return ProjectSettingsModalResult::Consumed;
                }
                if highlight_rect(card, self.extra()).contains(*x, *y) {
                    self.highlight_read_word = !self.highlight_read_word;
                    return ProjectSettingsModalResult::Consumed;
                }
                if color_rect(card, self.extra()).contains(*x, *y) {
                    self.scrolling_text_uses_character_color =
                        !self.scrolling_text_uses_character_color;
                    return ProjectSettingsModalResult::Consumed;
                }
                if emotion_rect(card, self.extra()).contains(*x, *y) {
                    self.show_text_emotion_lanes = !self.show_text_emotion_lanes;
                    return ProjectSettingsModalResult::Consumed;
                }
                if save_rect(card).contains(*x, *y) {
                    return self.save();
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
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = self.card_rect(screen_w, screen_h);
        let extra = self.extra();
        push_quad(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.75],
            [0.0; 4],
            0.0,
        );
        push_quad(
            quads,
            card,
            [0.22, 0.22, 0.26, 1.0],
            [0.45, 0.45, 0.52, 0.8],
            14.0,
        );
        push_label(
            labels,
            t("project_settings.title"),
            Rect {
                y: card.y + 10.0,
                height: 28.0,
                ..card
            },
            HAlign::Center,
            16.0,
            None,
        );
        push_label(
            labels,
            t("settings.rythmo_font"),
            Rect {
                x: card.x + 22.0,
                y: card.y + 44.0,
                width: card.width - 44.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );

        for (control, name, value) in [
            (
                1,
                t("settings.scroll_speed"),
                self.scroll_speed_text.as_str(),
            ),
            (
                2,
                t("settings.reading_bar_offset"),
                self.reading_bar_offset_text.as_str(),
            ),
        ] {
            let row = value_row_rect(card, control, extra);
            push_label(
                labels,
                name,
                Rect {
                    width: 220.0,
                    ..row
                },
                HAlign::Left,
                12.0,
                None,
            );
            push_button(quads, labels, minus_rect(card, control, extra), "-");
            push_label(
                labels,
                value,
                Rect {
                    x: card.x + 302.0,
                    width: 130.0,
                    ..row
                },
                HAlign::Center,
                12.0,
                None,
            );
            push_button(quads, labels, plus_rect(card, control, extra), "+");
        }

        push_label(
            labels,
            t("project_settings.instrumental_version"),
            Rect {
                x: card.x + 22.0,
                y: card.y + 208.0 + extra,
                width: card.width - 44.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        let path = path_rect(card, extra);
        push_quad(
            quads,
            path,
            [0.08, 0.08, 0.10, 1.0],
            [0.30, 0.30, 0.36, 0.5],
            4.0,
        );
        push_label(
            labels,
            if self.instrumental_audio_path.is_empty() {
                t("project_settings.no_file")
            } else {
                &self.instrumental_audio_path
            },
            path,
            HAlign::Left,
            12.0,
            None,
        );
        push_button(
            quads,
            labels,
            browse_rect(card, extra),
            t("project_settings.browse"),
        );
        push_button(
            quads,
            labels,
            clear_rect(card, extra),
            t("project_settings.clear"),
        );
        push_toggle(
            quads,
            labels,
            highlight_rect(card, extra),
            t("project_settings.highlight_read_word"),
            self.highlight_read_word,
        );
        push_toggle(
            quads,
            labels,
            color_rect(card, extra),
            t("project_settings.scrolling_text_character_color"),
            self.scrolling_text_uses_character_color,
        );
        push_toggle(
            quads,
            labels,
            emotion_rect(card, extra),
            t("project_settings.show_text_emotion_lanes"),
            self.show_text_emotion_lanes,
        );
        self.font
            .render(quads, labels, font_rect(card), t("settings.default_font"));
        push_button(quads, labels, save_rect(card), t("settings.save"));
        push_button(quads, labels, close_rect(card), t("project_settings.close"));
        let focus = match self.keyboard_focus {
            0 => font_rect(card),
            1 | 2 => value_row_rect(card, self.keyboard_focus, extra),
            3 => browse_rect(card, extra),
            4 => clear_rect(card, extra),
            5 => highlight_rect(card, extra),
            6 => color_rect(card, extra),
            7 => emotion_rect(card, extra),
            8 => save_rect(card),
            _ => close_rect(card),
        };
        push_outline(quads, focus);
    }

    fn card_rect(&self, screen_w: f32, screen_h: f32) -> Rect {
        let height = PROJECT_SETTINGS_H + self.extra();
        Rect {
            x: (screen_w - PROJECT_SETTINGS_W) / 2.0,
            y: (screen_h - height) / 2.0,
            width: PROJECT_SETTINGS_W,
            height,
        }
    }

    fn extra(&self) -> f32 {
        self.font.expanded_height()
    }

    fn move_focus(&mut self, delta: isize) -> ProjectSettingsModalResult {
        self.keyboard_focus =
            (self.keyboard_focus as isize + delta).rem_euclid(CONTROL_COUNT as isize) as usize;
        ProjectSettingsModalResult::Consumed
    }

    fn adjust(&mut self, control: usize, delta: isize) {
        if control == 1 {
            self.scroll_speed = (self.scroll_speed + delta as f32 * 0.25).clamp(0.25, 4.0);
            self.scroll_speed_text = format!("×{:.2}", self.scroll_speed);
        } else if control == 2 {
            self.reading_bar_offset_percent =
                (self.reading_bar_offset_percent + delta as f32).clamp(-50.0, 50.0);
            self.reading_bar_offset_text = format!("{:+.0} %", self.reading_bar_offset_percent);
        }
    }

    fn activate(&mut self) -> ProjectSettingsModalResult {
        match self.keyboard_focus {
            3 => ProjectSettingsModalResult::PickInstrumentalAudio,
            4 => {
                self.instrumental_audio_path.clear();
                ProjectSettingsModalResult::Consumed
            }
            5 => {
                self.highlight_read_word = !self.highlight_read_word;
                ProjectSettingsModalResult::Consumed
            }
            6 => {
                self.scrolling_text_uses_character_color =
                    !self.scrolling_text_uses_character_color;
                ProjectSettingsModalResult::Consumed
            }
            7 => {
                self.show_text_emotion_lanes = !self.show_text_emotion_lanes;
                ProjectSettingsModalResult::Consumed
            }
            8 => self.save(),
            9 => ProjectSettingsModalResult::Close,
            _ => ProjectSettingsModalResult::Consumed,
        }
    }

    fn save(&self) -> ProjectSettingsModalResult {
        let path = self.instrumental_audio_path.trim();
        ProjectSettingsModalResult::Save {
            rythmo_font: self.font.selected_owned(),
            scroll_speed: self.scroll_speed,
            reading_bar_offset_percent: self.reading_bar_offset_percent,
            instrumental_audio_path: (!path.is_empty()).then(|| path.to_string()),
            highlight_read_word: self.highlight_read_word,
            scrolling_text_uses_character_color: self.scrolling_text_uses_character_color,
            show_text_emotion_lanes: self.show_text_emotion_lanes,
        }
    }
}

pub fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: (screen_w - PROJECT_SETTINGS_W) / 2.0,
        y: (screen_h - PROJECT_SETTINGS_H) / 2.0,
        width: PROJECT_SETTINGS_W,
        height: PROJECT_SETTINGS_H,
    }
}

fn font_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 68.0,
        width: card.width - 44.0,
        height: 34.0,
    }
}
fn value_row_rect(card: Rect, control: usize, extra: f32) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 114.0 + (control - 1) as f32 * 46.0 + extra,
        width: card.width - 44.0,
        height: 32.0,
    }
}
fn minus_rect(card: Rect, control: usize, extra: f32) -> Rect {
    Rect {
        x: card.x + 270.0,
        width: 32.0,
        ..value_row_rect(card, control, extra)
    }
}
fn plus_rect(card: Rect, control: usize, extra: f32) -> Rect {
    Rect {
        x: card.x + card.width - 54.0,
        width: 32.0,
        ..value_row_rect(card, control, extra)
    }
}
fn path_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 232.0 + extra,
        width: card.width - 44.0,
        height: 32.0,
    }
}
fn browse_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 272.0 + extra,
        width: 130.0,
        height: 30.0,
    }
}
fn clear_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        x: card.x + 162.0,
        y: card.y + 272.0 + extra,
        width: 110.0,
        height: 30.0,
    }
}
fn highlight_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        x: card.x + 22.0,
        y: card.y + 314.0 + extra,
        width: card.width - 44.0,
        height: 20.0,
    }
}
fn color_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        y: card.y + 344.0 + extra,
        ..highlight_rect(card, extra)
    }
}
fn emotion_rect(card: Rect, extra: f32) -> Rect {
    Rect {
        y: card.y + 374.0 + extra,
        ..highlight_rect(card, extra)
    }
}
fn save_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 100.0,
        y: card.y + card.height - 48.0,
        width: 140.0,
        height: 34.0,
    }
}
fn close_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + card.width - 210.0,
        y: card.y + card.height - 48.0,
        width: 110.0,
        height: 34.0,
    }
}

fn toggle_label(key: &str, checked: bool) -> String {
    format!(
        "{}, {}",
        t(key),
        t(if checked {
            "accessibility.checked"
        } else {
            "accessibility.unchecked"
        })
    )
}

fn push_toggle<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    checked: bool,
) {
    push_quad(
        quads,
        Rect {
            width: 20.0,
            ..rect
        },
        if checked {
            [0.90, 0.72, 0.12, 1.0]
        } else {
            [0.08, 0.08, 0.10, 1.0]
        },
        [0.45, 0.45, 0.52, 0.8],
        4.0,
    );
    push_label(
        labels,
        text,
        Rect {
            x: rect.x + 30.0,
            width: rect.width - 30.0,
            ..rect
        },
        HAlign::Left,
        12.0,
        None,
    );
}

fn push_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
) {
    push_quad(
        quads,
        rect,
        [0.15, 0.15, 0.18, 1.0],
        [0.35, 0.35, 0.42, 0.8],
        6.0,
    );
    push_label(labels, text, rect, HAlign::Center, 12.0, None);
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    align: HAlign,
    size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align: align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(size),
        color_override: color,
        font_family_override: None,
    });
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_outline(quads: &mut Vec<QuadInstance>, rect: Rect) {
    push_quad(quads, rect, [0.0; 4], [0.38, 0.65, 1.0, 1.0], 8.0);
    quads.last_mut().unwrap().border_width = 2.5;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rythmo_values_are_clamped_at_the_modal_boundary() {
        let modal = ProjectSettingsModal::new(vec![], None, 99.0, -99.0, None, true, false, true);
        assert_eq!(modal.scroll_speed, 4.0);
        assert_eq!(modal.reading_bar_offset_percent, -50.0);
    }
}
