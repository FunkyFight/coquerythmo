#![allow(clippy::items_after_test_module)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 820.0;
const CARD_H: f32 = 660.0;
const PADDING: f32 = 24.0;
const LIST_W: f32 = 320.0;
const ROW_H: f32 = 46.0;
const LIST_TOP: f32 = 70.0;
const LIST_BOTTOM: f32 = 70.0;
const CONTROL_COUNT: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageListItem {
    pub id: u64,
    pub name: String,
    pub instrumental_audio_path: Option<String>,
    pub syllable_language: crate::project::SyllableLanguage,
}

#[derive(Debug, PartialEq)]
pub enum LanguageModalResult {
    Consumed,
    Close,
    Create {
        name: String,
    },
    Rename {
        id: u64,
        name: String,
    },
    Delete {
        id: u64,
    },
    Select {
        id: u64,
    },
    SetSyllableLanguage {
        id: u64,
        language: crate::project::SyllableLanguage,
    },
    PickInstrumental {
        id: u64,
    },
    ClearInstrumental {
        id: u64,
    },
}

pub struct LanguageModal {
    languages: Vec<LanguageListItem>,
    active_language_id: u64,
    selected_id: u64,
    scroll_offset: f32,
    name_input: String,
    editing_name: bool,
    replace_name: bool,
    keyboard_focus: usize,
}

impl LanguageModal {
    pub fn new(languages: Vec<LanguageListItem>, active_language_id: u64) -> Self {
        let selected_id = languages
            .iter()
            .find(|language| language.id == active_language_id)
            .or_else(|| languages.first())
            .map_or(active_language_id, |language| language.id);
        let name_input = languages
            .iter()
            .find(|language| language.id == selected_id)
            .map(|language| language.name.clone())
            .unwrap_or_default();
        Self {
            languages,
            active_language_id,
            selected_id,
            scroll_offset: 0.0,
            name_input,
            editing_name: false,
            replace_name: false,
            keyboard_focus: 0,
        }
    }

    pub fn refresh(&mut self, languages: Vec<LanguageListItem>, active_language_id: u64) {
        self.languages = languages;
        self.active_language_id = active_language_id;
        if !self
            .languages
            .iter()
            .any(|language| language.id == self.selected_id)
        {
            self.selected_id = self
                .languages
                .iter()
                .find(|language| language.id == active_language_id)
                .or_else(|| self.languages.first())
                .map_or(active_language_id, |language| language.id);
        }
        if !self.editing_name {
            self.sync_name_from_selection();
        }
        self.clamp_scroll(Self::list_height());
    }

    fn card(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W.min(screen_w - 24.0).max(360.0)) / 2.0,
            y: (screen_h - CARD_H.min(screen_h - 24.0).max(360.0)) / 2.0,
            width: CARD_W.min(screen_w - 24.0).max(360.0),
            height: CARD_H.min(screen_h - 24.0).max(360.0),
        }
    }

    fn list_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + PADDING,
            y: card.y + LIST_TOP,
            width: LIST_W.min(card.width * 0.44),
            height: (card.height - LIST_TOP - LIST_BOTTOM).max(120.0),
        }
    }

    fn list_height() -> f32 {
        CARD_H - LIST_TOP - LIST_BOTTOM
    }

    fn details_rect(card: Rect) -> Rect {
        let list = Self::list_rect(card);
        Rect {
            x: list.x + list.width + 24.0,
            y: list.y,
            width: (card.x + card.width - PADDING - (list.x + list.width + 24.0)).max(180.0),
            height: list.height,
        }
    }

    fn selected(&self) -> Option<&LanguageListItem> {
        self.languages
            .iter()
            .find(|language| language.id == self.selected_id)
    }

    pub fn keyboard_selection_label(&self) -> Option<String> {
        self.selected().map(|language| language.name.clone())
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.keyboard_focus {
            0 => self
                .selected()
                .map(|language| language.name.clone())
                .unwrap_or_else(|| t("languages.title").to_string()),
            1 => t("languages.name").to_string(),
            2 => t("languages.add").to_string(),
            3 => t("languages.rename").to_string(),
            4 => t("languages.select").to_string(),
            5 => t("languages.instrumental").to_string(),
            6 => t("languages.delete").to_string(),
            7 => format!(
                "{}: {}",
                t("languages.syllables"),
                self.selected()
                    .map(|language| syllable_language_label(language.syllable_language))
                    .unwrap_or(t("languages.syllables.french"))
            ),
            8 => t("languages.clear_instrumental").to_string(),
            _ => t("project_settings.close").to_string(),
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        match self.keyboard_focus {
            0 => "list box",
            1 => "text field",
            7 => "radio group",
            _ => "button",
        }
    }

    fn sync_name_from_selection(&mut self) {
        self.name_input = self
            .languages
            .iter()
            .find(|language| language.id == self.selected_id)
            .map(|language| language.name.clone())
            .unwrap_or_default();
    }

    fn max_scroll(&self, viewport_h: f32) -> f32 {
        (self.languages.len() as f32 * ROW_H - viewport_h).max(0.0)
    }

    fn clamp_scroll(&mut self, viewport_h: f32) {
        self.scroll_offset = self.scroll_offset.clamp(0.0, self.max_scroll(viewport_h));
    }

    fn name_field(details: Rect) -> Rect {
        Rect {
            x: details.x,
            y: details.y + 42.0,
            width: details.width,
            height: 40.0,
        }
    }

    fn action_rect(details: Rect, index: usize) -> Rect {
        Rect {
            x: details.x,
            y: details.y + 96.0 + index as f32 * 48.0,
            width: details.width,
            height: 38.0,
        }
    }

    fn syllable_option_rect(details: Rect, index: usize) -> Rect {
        let gap = 8.0;
        let width = (details.width - gap) / 2.0;
        Rect {
            x: details.x + index.min(1) as f32 * (width + gap),
            y: details.y + 362.0,
            width,
            height: 36.0,
        }
    }

    fn clear_audio_rect(details: Rect) -> Rect {
        Rect {
            x: details.x,
            y: details.y + details.height - 42.0,
            width: details.width,
            height: 34.0,
        }
    }

    fn handle_name_key(&mut self, text: &str) {
        if text == "\x08" || text == "\x7f" {
            if self.replace_name {
                self.name_input.clear();
                self.replace_name = false;
            } else {
                self.name_input.pop();
            }
            return;
        }
        if text == "\r" || text == "\n" || text == "\t" || text == "\x1b" {
            return;
        }
        if self.replace_name {
            self.name_input.clear();
            self.replace_name = false;
        }
        for ch in text.chars().filter(|ch| !ch.is_control()) {
            if self.name_input.chars().count() < 80 {
                self.name_input.push(ch);
            }
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> LanguageModalResult {
        let card = Self::card(screen_w, screen_h);
        let list = Self::list_rect(card);
        let details = Self::details_rect(card);

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return LanguageModalResult::Close;
                }
                if text == "\t" || text == "\u{b}" {
                    self.keyboard_focus = if text == "\t" {
                        (self.keyboard_focus + 1) % CONTROL_COUNT
                    } else {
                        (self.keyboard_focus + CONTROL_COUNT - 1) % CONTROL_COUNT
                    };
                    self.editing_name = self.keyboard_focus == 1;
                    self.replace_name = self.editing_name;
                    return LanguageModalResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    let trimmed = self.name_input.trim().to_string();
                    return match self.keyboard_focus {
                        0 | 1 => LanguageModalResult::Consumed,
                        2 if !trimmed.is_empty() => LanguageModalResult::Create { name: trimmed },
                        3 if !trimmed.is_empty() && self.selected().is_some() => {
                            LanguageModalResult::Rename {
                                id: self.selected_id,
                                name: trimmed,
                            }
                        }
                        4 if self.selected().is_some() => LanguageModalResult::Select {
                            id: self.selected_id,
                        },
                        5 if self.selected().is_some() => LanguageModalResult::PickInstrumental {
                            id: self.selected_id,
                        },
                        6 if self.languages.len() > 1 && self.selected().is_some() => {
                            LanguageModalResult::Delete {
                                id: self.selected_id,
                            }
                        }
                        7 if self.selected().is_some() => {
                            LanguageModalResult::SetSyllableLanguage {
                                id: self.selected_id,
                                language: self
                                    .selected()
                                    .map(|language| language.syllable_language.toggled())
                                    .unwrap_or_default(),
                            }
                        }
                        8 if self
                            .selected()
                            .and_then(|language| language.instrumental_audio_path.as_ref())
                            .is_some() =>
                        {
                            LanguageModalResult::ClearInstrumental {
                                id: self.selected_id,
                            }
                        }
                        9 => LanguageModalResult::Close,
                        _ => LanguageModalResult::Consumed,
                    };
                }
                if self.editing_name {
                    self.handle_name_key(text);
                }
                LanguageModalResult::Consumed
            }
            UiEvent::CursorUp if self.keyboard_focus == 0 => {
                if let Some(index) = self
                    .languages
                    .iter()
                    .position(|language| language.id == self.selected_id)
                {
                    let next = index.saturating_sub(1);
                    self.selected_id = self.languages[next].id;
                    self.sync_name_from_selection();
                }
                LanguageModalResult::Consumed
            }
            UiEvent::CursorDown if self.keyboard_focus == 0 => {
                if let Some(index) = self
                    .languages
                    .iter()
                    .position(|language| language.id == self.selected_id)
                {
                    let next = (index + 1).min(self.languages.len().saturating_sub(1));
                    self.selected_id = self.languages[next].id;
                    self.sync_name_from_selection();
                }
                LanguageModalResult::Consumed
            }
            UiEvent::CursorLeft | UiEvent::CursorRight if self.keyboard_focus == 7 => {
                let language = if matches!(event, UiEvent::CursorRight) {
                    crate::project::SyllableLanguage::English
                } else {
                    crate::project::SyllableLanguage::French
                };
                if self
                    .selected()
                    .is_some_and(|selected| selected.syllable_language != language)
                {
                    LanguageModalResult::SetSyllableLanguage {
                        id: self.selected_id,
                        language,
                    }
                } else {
                    LanguageModalResult::Consumed
                }
            }
            UiEvent::Scroll { x, y, delta, .. } if list.contains(*x, *y) => {
                self.scroll_offset -= *delta * 32.0;
                self.clamp_scroll(list.height);
                LanguageModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return LanguageModalResult::Close;
                }
                if list.contains(*x, *y) {
                    let relative_y = *y - list.y + self.scroll_offset;
                    let index = (relative_y / ROW_H).floor().max(0.0) as usize;
                    if let Some(language) = self.languages.get(index) {
                        self.selected_id = language.id;
                        self.editing_name = false;
                        self.replace_name = false;
                        self.sync_name_from_selection();
                    }
                    return LanguageModalResult::Consumed;
                }

                if Self::name_field(details).contains(*x, *y) {
                    self.editing_name = true;
                    self.replace_name = true;
                    return LanguageModalResult::Consumed;
                }
                self.editing_name = false;
                self.replace_name = false;

                let trimmed = self.name_input.trim().to_string();
                if Self::action_rect(details, 0).contains(*x, *y) {
                    if !trimmed.is_empty() {
                        return LanguageModalResult::Create { name: trimmed };
                    }
                } else if Self::action_rect(details, 1).contains(*x, *y) {
                    if !trimmed.is_empty() && self.selected().is_some() {
                        return LanguageModalResult::Rename {
                            id: self.selected_id,
                            name: trimmed,
                        };
                    }
                } else if Self::action_rect(details, 2).contains(*x, *y) {
                    if self.selected().is_some() {
                        return LanguageModalResult::Select {
                            id: self.selected_id,
                        };
                    }
                } else if Self::action_rect(details, 3).contains(*x, *y) {
                    if self.selected().is_some() {
                        return LanguageModalResult::PickInstrumental {
                            id: self.selected_id,
                        };
                    }
                } else if Self::action_rect(details, 4).contains(*x, *y) {
                    if self.languages.len() > 1 && self.selected().is_some() {
                        return LanguageModalResult::Delete {
                            id: self.selected_id,
                        };
                    }
                } else if Self::syllable_option_rect(details, 0).contains(*x, *y) {
                    self.keyboard_focus = 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::French
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::French,
                        };
                    }
                } else if Self::syllable_option_rect(details, 1).contains(*x, *y) {
                    self.keyboard_focus = 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::English
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::English,
                        };
                    }
                } else if Self::clear_audio_rect(details).contains(*x, *y)
                    && self
                        .selected()
                        .and_then(|language| language.instrumental_audio_path.as_ref())
                        .is_some()
                {
                    return LanguageModalResult::ClearInstrumental {
                        id: self.selected_id,
                    };
                }
                LanguageModalResult::Consumed
            }
            _ => LanguageModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card(screen_w, screen_h);
        let list = Self::list_rect(card);
        let details = Self::details_rect(card);

        push_panel(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.78],
            0.0,
            [0.0; 4],
        );
        push_panel(
            quads,
            card,
            [0.105, 0.11, 0.14, 1.0],
            16.0,
            [0.30, 0.34, 0.43, 0.9],
        );
        labels.push(label(
            t("languages.title"),
            Rect {
                x: card.x + PADDING,
                y: card.y + 16.0,
                width: card.width - PADDING * 2.0,
                height: 34.0,
            },
            26.0,
            HAlign::Left,
            Some([238, 240, 248]),
        ));
        labels.push(label(
            t("languages.subtitle"),
            Rect {
                x: card.x + PADDING,
                y: card.y + 44.0,
                width: card.width - PADDING * 2.0,
                height: 20.0,
            },
            13.0,
            HAlign::Left,
            Some([145, 151, 169]),
        ));

        push_panel(
            quads,
            list,
            [0.065, 0.07, 0.09, 1.0],
            10.0,
            [0.21, 0.24, 0.31, 1.0],
        );
        let first = (self.scroll_offset / ROW_H).floor().max(0.0) as usize;
        let visible = (list.height / ROW_H).ceil() as usize + 2;
        for (index, language) in self.languages.iter().enumerate().skip(first).take(visible) {
            let y = list.y + index as f32 * ROW_H - self.scroll_offset;
            let row = Rect {
                x: list.x + 6.0,
                y: y + 4.0,
                width: list.width - 12.0,
                height: ROW_H - 8.0,
            };
            if row.y + row.height < list.y || row.y > list.y + list.height {
                continue;
            }
            let selected = language.id == self.selected_id;
            push_panel(
                quads,
                row,
                if selected {
                    [0.16, 0.25, 0.43, 1.0]
                } else {
                    [0.09, 0.095, 0.12, 1.0]
                },
                7.0,
                if selected {
                    [0.31, 0.52, 0.90, 1.0]
                } else {
                    [0.0; 4]
                },
            );
            labels.push(label(
                &language.name,
                Rect {
                    x: row.x + 12.0,
                    y: row.y,
                    width: row.width - 74.0,
                    height: row.height,
                },
                15.0,
                HAlign::Left,
                None,
            ));
            if language.instrumental_audio_path.is_some() {
                labels.push(label(
                    "♫",
                    Rect {
                        x: row.x + row.width - 48.0,
                        y: row.y,
                        width: 20.0,
                        height: row.height,
                    },
                    18.0,
                    HAlign::Center,
                    Some([128, 202, 164]),
                ));
            }
            if language.id == self.active_language_id {
                labels.push(label(
                    "✓",
                    Rect {
                        x: row.x + row.width - 26.0,
                        y: row.y,
                        width: 18.0,
                        height: row.height,
                    },
                    16.0,
                    HAlign::Center,
                    Some([126, 168, 255]),
                ));
            }
        }

        if self.max_scroll(list.height) > 0.0 {
            let track = Rect {
                x: list.x + list.width - 5.0,
                y: list.y + 5.0,
                width: 2.0,
                height: list.height - 10.0,
            };
            push_panel(quads, track, [0.18, 0.19, 0.24, 1.0], 1.0, [0.0; 4]);
            let thumb_h = (list.height / (self.languages.len() as f32 * ROW_H) * track.height)
                .clamp(28.0, track.height);
            let thumb_y = track.y
                + self.scroll_offset / self.max_scroll(list.height) * (track.height - thumb_h);
            push_panel(
                quads,
                Rect {
                    x: track.x - 1.0,
                    y: thumb_y,
                    width: 4.0,
                    height: thumb_h,
                },
                [0.36, 0.43, 0.58, 1.0],
                2.0,
                [0.0; 4],
            );
        }

        labels.push(label(
            t("languages.name"),
            Rect {
                x: details.x,
                y: details.y + 10.0,
                width: details.width,
                height: 22.0,
            },
            14.0,
            HAlign::Left,
            Some([165, 171, 190]),
        ));
        let name_field = Self::name_field(details);
        push_panel(
            quads,
            name_field,
            if self.editing_name {
                [0.09, 0.12, 0.18, 1.0]
            } else {
                [0.07, 0.075, 0.095, 1.0]
            },
            8.0,
            if self.editing_name {
                [0.30, 0.52, 0.95, 1.0]
            } else {
                [0.22, 0.25, 0.32, 1.0]
            },
        );
        labels.push(label(
            if self.name_input.is_empty() {
                t("languages.name_placeholder")
            } else {
                &self.name_input
            },
            Rect {
                x: name_field.x + 12.0,
                y: name_field.y,
                width: name_field.width - 24.0,
                height: name_field.height,
            },
            15.0,
            HAlign::Left,
            if self.name_input.is_empty() {
                Some([105, 111, 129])
            } else {
                None
            },
        ));

        push_action(
            quads,
            labels,
            Self::action_rect(details, 0),
            t("languages.add"),
            [0.18, 0.38, 0.72, 1.0],
            true,
        );
        push_action(
            quads,
            labels,
            Self::action_rect(details, 1),
            t("languages.rename"),
            [0.15, 0.18, 0.24, 1.0],
            self.selected().is_some(),
        );
        push_action(
            quads,
            labels,
            Self::action_rect(details, 2),
            if self.selected_id == self.active_language_id {
                t("languages.active")
            } else {
                t("languages.select")
            },
            [0.12, 0.38, 0.27, 1.0],
            self.selected_id != self.active_language_id,
        );
        push_action(
            quads,
            labels,
            Self::action_rect(details, 3),
            t("languages.instrumental"),
            [0.25, 0.20, 0.40, 1.0],
            self.selected().is_some(),
        );
        push_action(
            quads,
            labels,
            Self::action_rect(details, 4),
            t("languages.delete"),
            [0.42, 0.14, 0.17, 1.0],
            self.languages.len() > 1,
        );

        labels.push(label(
            t("languages.syllables"),
            Rect {
                x: details.x,
                y: details.y + 338.0,
                width: details.width,
                height: 18.0,
            },
            12.0,
            HAlign::Left,
            Some([137, 143, 160]),
        ));
        let selected_syllable_language = self
            .selected()
            .map(|language| language.syllable_language)
            .unwrap_or_default();
        for (index, language) in [
            crate::project::SyllableLanguage::French,
            crate::project::SyllableLanguage::English,
        ]
        .into_iter()
        .enumerate()
        {
            let selected = language == selected_syllable_language;
            let focused = self.keyboard_focus == 7;
            let rect = Self::syllable_option_rect(details, index);
            push_panel(
                quads,
                rect,
                if selected {
                    [0.16, 0.31, 0.55, 1.0]
                } else {
                    [0.10, 0.11, 0.15, 1.0]
                },
                8.0,
                if selected || focused {
                    [0.31, 0.52, 0.90, 1.0]
                } else {
                    [0.22, 0.25, 0.32, 1.0]
                },
            );
            labels.push(label(
                syllable_language_label(language),
                rect,
                13.0,
                HAlign::Center,
                None,
            ));
        }

        let audio_path = self
            .selected()
            .and_then(|language| language.instrumental_audio_path.as_deref())
            .unwrap_or(t("languages.no_instrumental"));
        labels.push(label(
            t("languages.instrumental_path"),
            Rect {
                x: details.x,
                y: details.y + details.height - 104.0,
                width: details.width,
                height: 18.0,
            },
            12.0,
            HAlign::Left,
            Some([137, 143, 160]),
        ));
        labels.push(LabelInfo {
            text: audio_path,
            bounds: Rect {
                x: details.x,
                y: details.y + details.height - 84.0,
                width: details.width,
                height: 32.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 8.0,
            font_size_override: Some(12.0),
            color_override: Some([180, 184, 198]),
            font_family_override: None,
        });
        push_action(
            quads,
            labels,
            Self::clear_audio_rect(details),
            t("languages.clear_instrumental"),
            [0.14, 0.15, 0.19, 1.0],
            self.selected()
                .and_then(|language| language.instrumental_audio_path.as_ref())
                .is_some(),
        );
    }
}

pub(super) fn syllable_language_label(language: crate::project::SyllableLanguage) -> &'static str {
    match language {
        crate::project::SyllableLanguage::French => t("languages.syllables.french"),
        crate::project::SyllableLanguage::English => t("languages.syllables.english"),
    }
}

fn push_action<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    color: [f32; 4],
    enabled: bool,
) {
    let disabled = [0.095, 0.10, 0.12, 1.0];
    push_panel(
        quads,
        rect,
        if enabled { color } else { disabled },
        8.0,
        if enabled {
            [0.34, 0.38, 0.48, 0.7]
        } else {
            [0.0; 4]
        },
    );
    labels.push(label(
        text,
        rect,
        14.0,
        HAlign::Center,
        if enabled { None } else { Some([94, 99, 113]) },
    ));
}

fn push_panel(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    radius: f32,
    border: [f32; 4],
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(language: crate::project::SyllableLanguage) -> LanguageListItem {
        LanguageListItem {
            id: 1,
            name: "English".into(),
            instrumental_audio_path: None,
            syllable_language: language,
        }
    }

    #[test]
    fn syllable_language_control_has_keyboard_and_pointer_parity() {
        let mut modal = LanguageModal::new(vec![item(crate::project::SyllableLanguage::French)], 1);
        modal.keyboard_focus = 7;
        assert_eq!(
            modal.handle_event(&UiEvent::KeyInput { text: "\r".into() }, 1280.0, 720.0,),
            LanguageModalResult::SetSyllableLanguage {
                id: 1,
                language: crate::project::SyllableLanguage::English,
            }
        );

        modal.refresh(vec![item(crate::project::SyllableLanguage::English)], 1);
        let details = LanguageModal::details_rect(LanguageModal::card(1280.0, 720.0));
        let french = LanguageModal::syllable_option_rect(details, 0);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: french.x + 2.0,
                    y: french.y + 2.0,
                },
                1280.0,
                720.0,
            ),
            LanguageModalResult::SetSyllableLanguage {
                id: 1,
                language: crate::project::SyllableLanguage::French,
            }
        );
        assert_eq!(modal.keyboard_focus, 7);
    }

    #[test]
    fn tab_and_shift_tab_reach_the_syllable_control_deterministically() {
        let mut modal = LanguageModal::new(vec![item(crate::project::SyllableLanguage::French)], 1);
        modal.handle_event(
            &UiEvent::KeyInput {
                text: "\u{b}".into(),
            },
            1280.0,
            720.0,
        );
        assert_eq!(modal.keyboard_focus, CONTROL_COUNT - 1);
        for _ in 0..8 {
            modal.handle_event(&UiEvent::KeyInput { text: "\t".into() }, 1280.0, 720.0);
        }
        assert_eq!(modal.keyboard_focus, 7);
        assert_eq!(modal.keyboard_focus_role(), "radio group");
        assert!(modal
            .keyboard_focus_label()
            .contains(t("languages.syllables")));
    }

    #[test]
    fn instrumental_details_start_below_last_language_action() {
        let card = LanguageModal::card(1280.0, 720.0);
        let details = LanguageModal::details_rect(card);
        let delete_button = LanguageModal::action_rect(details, 4);
        let instrumental_label_y = details.y + details.height - 104.0;

        let syllable_options_bottom = LanguageModal::syllable_option_rect(details, 0).y + 36.0;
        assert!(syllable_options_bottom + 18.0 <= instrumental_label_y);
        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);
        assert!(LanguageModal::clear_audio_rect(details).y + 34.0 <= details.y + details.height);
    }
}

fn label<'a>(
    text: &'a str,
    bounds: Rect,
    size: f32,
    h_align: HAlign,
    color: Option<[u8; 3]>,
) -> LabelInfo<'a> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(size),
        color_override: color,
        font_family_override: None,
    }
}
