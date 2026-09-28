use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;
use std::path::PathBuf;

pub const SETTINGS_W: f32 = 450.0;
pub const SETTINGS_H: f32 = 334.0;
const CONTROL_COUNT: usize = 6;
const CONTROLS_TOGGLE_FOCUS: usize = 2;
const HINTS_TOGGLE_FOCUS: usize = 3;
const SAVE_FOCUS: usize = 4;

pub struct SettingsModal {
    pub lang: String,
    pub temporary_directory: PathBuf,
    pub temporary_directory_text: String,
    pub show_controls_hint: bool,
    pub hide_formatting_hints: bool,
    keyboard_focus: usize,
}

pub enum SettingsModalResult {
    Consumed,
    Close,
    Save {
        lang: String,
        temporary_directory: PathBuf,
        show_controls_hint: bool,
        hide_formatting_hints: bool,
    },
    BrowseTemporaryDirectory,
}

impl SettingsModal {
    pub fn new(temporary_directory: PathBuf) -> Self {
        Self {
            lang: crate::config::get().lang.clone(),
            temporary_directory_text: temporary_directory.display().to_string(),
            temporary_directory,
            show_controls_hint: crate::config::get().ui.show_controls_hint,
            hide_formatting_hints: crate::config::get().ui.hide_formatting_hints,
            keyboard_focus: 0,
        }
    }

    pub fn set_temporary_directory(&mut self, path: PathBuf) {
        self.temporary_directory_text = path.display().to_string();
        self.temporary_directory = path;
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => format!("{}, {}", t("settings.language"), language_label(&self.lang)),
            1 => format!(
                "{} {}",
                t("settings.temporary_directory"),
                self.temporary_directory_text
            ),
            CONTROLS_TOGGLE_FOCUS => {
                checkbox_label(t("settings.show_controls"), self.show_controls_hint)
            }
            HINTS_TOGGLE_FOCUS => checkbox_label(
                t("settings.hide_formatting_hints"),
                self.hide_formatting_hints,
            ),
            SAVE_FOCUS => t("settings.save").to_string(),
            _ => t("project_settings.close").to_string(),
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
            UiEvent::FocusNext => {
                self.keyboard_focus = (self.keyboard_focus + 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\t" => {
                self.keyboard_focus = (self.keyboard_focus + 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.keyboard_focus = (self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\u{b}" => {
                self.keyboard_focus = (self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::CursorLeft | UiEvent::CursorUp if self.keyboard_focus == 0 => {
                self.cycle_language(-1);
                SettingsModalResult::Consumed
            }
            UiEvent::CursorRight | UiEvent::CursorDown if self.keyboard_focus == 0 => {
                self.cycle_language(1);
                SettingsModalResult::Consumed
            }
            UiEvent::CursorUp | UiEvent::CursorLeft => {
                self.keyboard_focus = (self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::CursorDown | UiEvent::CursorRight => {
                self.keyboard_focus = (self.keyboard_focus + 1) % CONTROL_COUNT;
                SettingsModalResult::Consumed
            }
            UiEvent::Activate => self.activate(),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                self.activate()
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return SettingsModalResult::Close;
                }
                for (lang, rect) in language_buttons(card) {
                    if rect.contains(*x, *y) {
                        self.lang = lang.to_string();
                        self.keyboard_focus = 0;
                        return SettingsModalResult::Consumed;
                    }
                }
                if browse_rect(card).contains(*x, *y) {
                    self.keyboard_focus = 1;
                    return SettingsModalResult::BrowseTemporaryDirectory;
                }
                if controls_toggle_rect(card).contains(*x, *y) {
                    self.keyboard_focus = CONTROLS_TOGGLE_FOCUS;
                    self.show_controls_hint = !self.show_controls_hint;
                    return SettingsModalResult::Consumed;
                }
                if hints_toggle_rect(card).contains(*x, *y) {
                    self.keyboard_focus = HINTS_TOGGLE_FOCUS;
                    self.hide_formatting_hints = !self.hide_formatting_hints;
                    return SettingsModalResult::Consumed;
                }
                if save_rect(card).contains(*x, *y) {
                    return self.save();
                }
                if close_rect(card).contains(*x, *y) {
                    return SettingsModalResult::Close;
                }
                SettingsModalResult::Consumed
            }
            _ => SettingsModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = card_rect(screen_w, screen_h);
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
            t("settings.title"),
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
            t("settings.language"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 44.0,
                width: card.width - 40.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        for (lang, rect) in language_buttons(card) {
            push_button(quads, labels, rect, language_label(lang), self.lang == lang);
        }
        push_label(
            labels,
            t("settings.temporary_directory"),
            Rect {
                x: card.x + 20.0,
                y: card.y + 112.0,
                width: card.width - 40.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        let path = path_rect(card);
        push_quad(
            quads,
            path,
            [0.08, 0.08, 0.10, 1.0],
            [0.30, 0.30, 0.36, 0.5],
            4.0,
        );
        push_label(
            labels,
            &self.temporary_directory_text,
            path,
            HAlign::Left,
            11.0,
            None,
        );
        push_button(
            quads,
            labels,
            browse_rect(card),
            t("settings.browse"),
            false,
        );
        push_checkbox_row(
            quads,
            labels,
            controls_toggle_rect(card),
            t("settings.show_controls"),
            self.show_controls_hint,
        );
        push_checkbox_row(
            quads,
            labels,
            hints_toggle_rect(card),
            t("settings.hide_formatting_hints"),
            self.hide_formatting_hints,
        );
        push_button(quads, labels, save_rect(card), t("settings.save"), true);
        push_button(
            quads,
            labels,
            close_rect(card),
            t("project_settings.close"),
            false,
        );
        let focus = match self.keyboard_focus {
            0 => language_rect(card, &self.lang),
            1 => browse_rect(card),
            CONTROLS_TOGGLE_FOCUS => controls_toggle_rect(card),
            HINTS_TOGGLE_FOCUS => hints_toggle_rect(card),
            SAVE_FOCUS => save_rect(card),
            _ => close_rect(card),
        };
        push_outline(quads, focus);
    }

    fn activate(&mut self) -> SettingsModalResult {
        match self.keyboard_focus {
            0 => {
                self.cycle_language(1);
                SettingsModalResult::Consumed
            }
            1 => SettingsModalResult::BrowseTemporaryDirectory,
            CONTROLS_TOGGLE_FOCUS => {
                self.show_controls_hint = !self.show_controls_hint;
                SettingsModalResult::Consumed
            }
            HINTS_TOGGLE_FOCUS => {
                self.hide_formatting_hints = !self.hide_formatting_hints;
                SettingsModalResult::Consumed
            }
            SAVE_FOCUS => self.save(),
            _ => SettingsModalResult::Close,
        }
    }

    fn cycle_language(&mut self, delta: isize) {
        const LANGS: [&str; 3] = ["fr-fr", "en-us", "es-es"];
        let index = LANGS
            .iter()
            .position(|lang| *lang == self.lang)
            .unwrap_or(0);
        self.lang =
            LANGS[(index as isize + delta).rem_euclid(LANGS.len() as isize) as usize].into();
    }

    fn save(&self) -> SettingsModalResult {
        SettingsModalResult::Save {
            lang: self.lang.clone(),
            temporary_directory: self.temporary_directory.clone(),
            show_controls_hint: self.show_controls_hint,
            hide_formatting_hints: self.hide_formatting_hints,
        }
    }
}

fn checkbox_label(name: &str, checked: bool) -> String {
    format!(
        "{name}, {}",
        if checked {
            t("accessibility.checked")
        } else {
            t("accessibility.unchecked")
        }
    )
}

fn controls_toggle_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 20.0,
        y: card.y + 180.0,
        width: card.width - 40.0,
        height: 30.0,
    }
}

fn hints_toggle_rect(card: Rect) -> Rect {
    Rect {
        y: card.y + 214.0,
        ..controls_toggle_rect(card)
    }
}

pub fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: (screen_w - SETTINGS_W) / 2.0,
        y: (screen_h - SETTINGS_H) / 2.0,
        width: SETTINGS_W,
        height: SETTINGS_H,
    }
}

fn language_buttons(card: Rect) -> [(&'static str, Rect); 3] {
    ["fr-fr", "en-us", "es-es"].map(|lang| (lang, language_rect(card, lang)))
}

fn language_rect(card: Rect, lang: &str) -> Rect {
    let index = if lang.starts_with("en") {
        1.0
    } else if lang.starts_with("es") {
        2.0
    } else {
        0.0
    };
    Rect {
        x: card.x + 20.0 + index * 100.0,
        y: card.y + 68.0,
        width: 90.0,
        height: 30.0,
    }
}

fn path_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 20.0,
        y: card.y + 136.0,
        width: card.width - 148.0,
        height: 30.0,
    }
}

fn browse_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + card.width - 120.0,
        y: card.y + 136.0,
        width: 100.0,
        height: 30.0,
    }
}

fn save_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 90.0,
        y: card.y + SETTINGS_H - 50.0,
        width: 140.0,
        height: 36.0,
    }
}

fn close_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + card.width - 180.0,
        y: card.y + SETTINGS_H - 50.0,
        width: 100.0,
        height: 36.0,
    }
}

fn language_label(lang: &str) -> &'static str {
    if lang.starts_with("en") {
        "English"
    } else if lang.starts_with("es") {
        "Español"
    } else {
        "Français"
    }
}

fn push_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    selected: bool,
) {
    push_quad(
        quads,
        rect,
        if selected {
            [0.30, 0.55, 0.30, 1.0]
        } else {
            [0.15, 0.15, 0.18, 1.0]
        },
        [0.35, 0.35, 0.42, 0.8],
        6.0,
    );
    push_label(labels, text, rect, HAlign::Center, 12.0, None);
}

fn push_checkbox_row<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    checked: bool,
) {
    let tick = Rect {
        y: rect.y + (rect.height - 20.0) / 2.0,
        width: 20.0,
        height: 20.0,
        ..rect
    };
    push_quad(
        quads,
        tick,
        if checked {
            [0.20, 0.43, 0.82, 1.0]
        } else {
            [0.07, 0.075, 0.095, 1.0]
        },
        [0.28, 0.34, 0.46, 1.0],
        4.0,
    );
    if checked {
        push_label(labels, "✓", tick, HAlign::Center, 14.0, None);
        labels.last_mut().unwrap().padding = 0.0;
    }
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

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 8.0,
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
    fn general_settings_save_language_directory_and_controls_hint() {
        let modal = SettingsModal {
            lang: "fr-fr".into(),
            temporary_directory: PathBuf::from("tmp"),
            temporary_directory_text: "tmp".into(),
            show_controls_hint: true,
            hide_formatting_hints: true,
            keyboard_focus: 0,
        };
        assert!(matches!(
            modal.save(),
            SettingsModalResult::Save {
                show_controls_hint: true,
                hide_formatting_hints: true,
                ..
            }
        ));
    }

    #[test]
    fn formatting_hints_checkbox_toggles_from_the_keyboard() {
        let mut modal = SettingsModal {
            lang: "fr-fr".into(),
            temporary_directory: PathBuf::from("tmp"),
            temporary_directory_text: "tmp".into(),
            show_controls_hint: true,
            hide_formatting_hints: false,
            keyboard_focus: CONTROLS_TOGGLE_FOCUS,
        };
        modal.handle_event(&UiEvent::FocusNext, SETTINGS_W, SETTINGS_H);
        modal.handle_event(&UiEvent::Activate, SETTINGS_W, SETTINGS_H);

        assert!(modal.hide_formatting_hints);
        assert!(modal.show_controls_hint);
    }
}
