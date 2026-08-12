#![allow(clippy::items_after_test_module)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 820.0;
const CARD_H: f32 = 660.0;
const PADDING: f32 = 24.0;
const LIST_W: f32 = 320.0;
const ROW_H: f32 = 46.0;
const LIST_TOP: f32 = 132.0;
const LIST_BOTTOM: f32 = 70.0;
const MEDIA_CONTENT_FOCUS: usize = 3;
const MEDIA_CLOSE_FOCUS: usize = usize::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaExplorerTab {
    #[default]
    Videos,
    Audios,
    RythmoBands,
}

impl MediaExplorerTab {
    fn index(self) -> usize {
        match self {
            Self::Videos => 0,
            Self::Audios => 1,
            Self::RythmoBands => 2,
        }
    }

    fn from_index(index: usize) -> Self {
        [Self::Videos, Self::Audios, Self::RythmoBands][index.min(2)]
    }

    fn label(self) -> &'static str {
        t(match self {
            Self::Videos => "media_explorer.tab.videos",
            Self::Audios => "media_explorer.tab.audios",
            Self::RythmoBands => "media_explorer.tab.rythmo_bands",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaVideoItem {
    pub name: String,
    pub path: String,
    pub summary: String,
    pub audio_summary: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaExplorerData {
    pub source: Option<MediaVideoItem>,
    pub proxy: Option<MediaVideoItem>,
    pub active_proxy: bool,
    pub default_proxy: bool,
    pub can_persist_default: bool,
}

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
    AddVideo,
    CreateProxy,
    SwitchVideo {
        use_proxy: bool,
    },
    SetDefaultVideo {
        use_proxy: bool,
    },
    DeleteVideo {
        use_proxy: bool,
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
    tab: MediaExplorerTab,
    media: MediaExplorerData,
    audio_scroll_offset: f32,
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
            tab: MediaExplorerTab::default(),
            media: MediaExplorerData::default(),
            audio_scroll_offset: 0.0,
        }
    }

    pub fn with_media(
        languages: Vec<LanguageListItem>,
        active_language_id: u64,
        media: MediaExplorerData,
    ) -> Self {
        Self {
            media,
            ..Self::new(languages, active_language_id)
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
        if !self.focus_order().contains(&self.keyboard_focus) {
            self.keyboard_focus = self.tab.index();
        }
        self.clamp_scroll(Self::list_height());
    }

    pub fn refresh_media(&mut self, media: MediaExplorerData) {
        self.media = media;
        if !self.focus_order().contains(&self.keyboard_focus) {
            self.keyboard_focus = self.tab.index();
        }
    }

    fn card(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W.min(screen_w - 24.0).max(360.0)) / 2.0,
            y: (screen_h - CARD_H.min(screen_h - 24.0).max(360.0)) / 2.0,
            width: CARD_W.min(screen_w - 24.0).max(360.0),
            height: CARD_H.min(screen_h - 24.0).max(360.0),
        }
    }

    fn tab_rect(card: Rect, index: usize) -> Rect {
        let gap = 8.0;
        let width = (card.width - PADDING * 2.0 - gap * 2.0) / 3.0;
        Rect {
            x: card.x + PADDING + index.min(2) as f32 * (width + gap),
            y: card.y + 82.0,
            width,
            height: 36.0,
        }
    }

    fn close_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - 52.0,
            y: card.y + 16.0,
            width: 32.0,
            height: 32.0,
        }
    }

    fn content_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + PADDING,
            y: card.y + LIST_TOP,
            width: card.width - PADDING * 2.0,
            height: card.height - LIST_TOP - LIST_BOTTOM,
        }
    }

    fn video_row(card: Rect, index: usize) -> Rect {
        let content = Self::content_rect(card);
        Rect {
            x: content.x,
            y: content.y + index.min(1) as f32 * 174.0,
            width: content.width,
            height: 158.0,
        }
    }

    fn video_action(row: Rect, index: usize) -> Rect {
        let gap = 8.0;
        let width = (row.width - 32.0 - gap * 2.0) / 3.0;
        Rect {
            x: row.x + 16.0 + index.min(2) as f32 * (width + gap),
            y: row.y + row.height - 46.0,
            width,
            height: 32.0,
        }
    }

    fn media_footer_action(card: Rect, index: usize) -> Rect {
        let content = Self::content_rect(card);
        let gap = 10.0;
        let width = (content.width - gap) / 2.0;
        Rect {
            x: content.x + index.min(1) as f32 * (width + gap),
            y: content.y + content.height - 42.0,
            width,
            height: 38.0,
        }
    }

    fn audio_viewport(card: Rect) -> Rect {
        Self::content_rect(card)
    }

    fn audio_row(card: Rect, index: usize, scroll: f32) -> Rect {
        let viewport = Self::audio_viewport(card);
        Rect {
            x: viewport.x,
            y: viewport.y + index as f32 * 72.0 - scroll,
            width: viewport.width,
            height: 62.0,
        }
    }

    fn audio_action(row: Rect, index: usize) -> Rect {
        Rect {
            x: row.x + row.width - 250.0 + index.min(1) as f32 * 126.0,
            y: row.y + 15.0,
            width: 118.0,
            height: 32.0,
        }
    }

    fn keyboard_focus_rect(&self, card: Rect) -> Option<Rect> {
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS {
            return Some(Self::tab_rect(card, self.keyboard_focus));
        }
        if self.keyboard_focus == MEDIA_CLOSE_FOCUS {
            return Some(Self::close_rect(card));
        }
        let focus = self.content_focus()?;
        match self.tab {
            MediaExplorerTab::Videos => Some(if focus < 6 {
                Self::video_action(Self::video_row(card, focus / 3), focus % 3)
            } else {
                Self::media_footer_action(card, focus - 6)
            }),
            MediaExplorerTab::Audios => Some(Self::audio_action(
                Self::audio_row(card, focus / 2 + 1, self.audio_scroll_offset),
                focus % 2,
            )),
            MediaExplorerTab::RythmoBands => {
                let details = Self::details_rect(card);
                Some(match focus {
                    0 => Self::list_rect(card),
                    1 => Self::name_field(details),
                    2..=6 => Self::action_rect(details, focus - 2),
                    7 => Rect {
                        x: details.x,
                        y: Self::syllable_option_rect(details, 0).y,
                        width: details.width,
                        height: Self::syllable_option_rect(details, 0).height,
                    },
                    8 => Self::clear_audio_rect(details),
                    _ => return None,
                })
            }
        }
    }

    fn render_keyboard_focus(&self, quads: &mut Vec<QuadInstance>, card: Rect) {
        if let Some(rect) = self.keyboard_focus_rect(card) {
            quads.push(QuadInstance {
                rect: [
                    rect.x - 2.0,
                    rect.y - 2.0,
                    rect.width + 4.0,
                    rect.height + 4.0,
                ],
                color: [0.0; 4],
                color_bottom: [0.0; 4],
                border_color: [0.45, 0.78, 1.0, 1.0],
                border_width: 2.0,
                border_radius: 9.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
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
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS {
            let tab = MediaExplorerTab::from_index(self.keyboard_focus);
            return if tab == self.tab {
                format!("{}, {}", tab.label(), t("accessibility.selected"))
            } else {
                tab.label().to_string()
            };
        }
        if self.keyboard_focus == MEDIA_CLOSE_FOCUS {
            return t("project_settings.close").to_string();
        }
        let focus = self.keyboard_focus - MEDIA_CONTENT_FOCUS;
        if self.tab == MediaExplorerTab::Videos {
            let (kind, item, action) = match focus {
                0 => (
                    t("media_explorer.video.original"),
                    self.media.source.as_ref(),
                    t("media_explorer.use"),
                ),
                1 => (
                    t("media_explorer.video.original"),
                    self.media.source.as_ref(),
                    t("media_explorer.make_default"),
                ),
                2 => (
                    t("media_explorer.video.original"),
                    self.media.source.as_ref(),
                    t("media_explorer.unlink"),
                ),
                3 => (
                    t("media_explorer.video.proxy"),
                    self.media.proxy.as_ref(),
                    t("media_explorer.use"),
                ),
                4 => (
                    t("media_explorer.video.proxy"),
                    self.media.proxy.as_ref(),
                    t("media_explorer.make_default"),
                ),
                5 => (
                    t("media_explorer.video.proxy"),
                    self.media.proxy.as_ref(),
                    t("media_explorer.delete"),
                ),
                6 => return t("media_explorer.video.add").to_string(),
                _ => return t("media_explorer.video.create_proxy").to_string(),
            };
            let status = [
                (
                    self.media.active_proxy == (focus >= 3),
                    t("media_explorer.active"),
                ),
                (
                    self.media.default_proxy == (focus >= 3),
                    t("media_explorer.default"),
                ),
            ]
            .into_iter()
            .filter_map(|(enabled, label)| enabled.then_some(label))
            .collect::<Vec<_>>()
            .join(", ");
            return format!(
                "{} {}, {}{}{}",
                kind,
                item.map_or("", |item| item.name.as_str()),
                status,
                (!status.is_empty()).then_some(": ").unwrap_or_default(),
                action
            );
        }
        if self.tab == MediaExplorerTab::Audios {
            return self
                .languages
                .get(focus / 2)
                .map(|language| {
                    format!(
                        "{}: {}",
                        language.name,
                        if focus.is_multiple_of(2) {
                            t("media_explorer.audio.assign")
                        } else {
                            t("media_explorer.audio.remove")
                        }
                    )
                })
                .unwrap_or_else(|| t("media_explorer.tab.audios").to_string());
        }
        match focus {
            0 => self
                .selected()
                .map(|language| language.name.clone())
                .unwrap_or_else(|| t("media_explorer.title").to_string()),
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
            _ => t("languages.clear_instrumental").to_string(),
        }
    }

    pub fn keyboard_focus_role(&self) -> &'static str {
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS {
            return "tab";
        }
        if self.keyboard_focus == MEDIA_CLOSE_FOCUS || self.tab != MediaExplorerTab::RythmoBands {
            return "button";
        }
        match self.keyboard_focus - MEDIA_CONTENT_FOCUS {
            0 => "list box",
            1 => "text field",
            7 => "radio group",
            _ => "button",
        }
    }

    fn focus_order(&self) -> Vec<usize> {
        let mut controls = vec![0, 1, 2];
        match self.tab {
            MediaExplorerTab::Videos => {
                if self.media.source.is_some() {
                    if self.media.active_proxy {
                        controls.push(MEDIA_CONTENT_FOCUS);
                    }
                    if self.media.can_persist_default && self.media.default_proxy {
                        controls.push(MEDIA_CONTENT_FOCUS + 1);
                    }
                    controls.push(MEDIA_CONTENT_FOCUS + 2);
                }
                if self.media.proxy.is_some() {
                    if !self.media.active_proxy {
                        controls.push(MEDIA_CONTENT_FOCUS + 3);
                    }
                    if self.media.can_persist_default && !self.media.default_proxy {
                        controls.push(MEDIA_CONTENT_FOCUS + 4);
                    }
                    controls.push(MEDIA_CONTENT_FOCUS + 5);
                }
                controls.push(MEDIA_CONTENT_FOCUS + 6);
                if self.media.source.is_some() {
                    controls.push(MEDIA_CONTENT_FOCUS + 7);
                }
            }
            MediaExplorerTab::Audios => {
                for (index, language) in self.languages.iter().enumerate() {
                    controls.push(MEDIA_CONTENT_FOCUS + index * 2);
                    if language.instrumental_audio_path.is_some() {
                        controls.push(MEDIA_CONTENT_FOCUS + index * 2 + 1);
                    }
                }
            }
            MediaExplorerTab::RythmoBands => {
                if !self.languages.is_empty() {
                    controls.push(MEDIA_CONTENT_FOCUS);
                }
                controls.push(MEDIA_CONTENT_FOCUS + 1);
                if !self.name_input.trim().is_empty() {
                    controls.push(MEDIA_CONTENT_FOCUS + 2);
                }
                if self.selected().is_some() {
                    if !self.name_input.trim().is_empty() {
                        controls.push(MEDIA_CONTENT_FOCUS + 3);
                    }
                    if self.selected_id != self.active_language_id {
                        controls.push(MEDIA_CONTENT_FOCUS + 4);
                    }
                    controls.push(MEDIA_CONTENT_FOCUS + 5);
                    if self.languages.len() > 1 {
                        controls.push(MEDIA_CONTENT_FOCUS + 6);
                    }
                    controls.push(MEDIA_CONTENT_FOCUS + 7);
                    if self
                        .selected()
                        .and_then(|language| language.instrumental_audio_path.as_ref())
                        .is_some()
                    {
                        controls.push(MEDIA_CONTENT_FOCUS + 8);
                    }
                }
            }
        }
        controls.push(MEDIA_CLOSE_FOCUS);
        controls
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
            y: details.y + 26.0,
            width: details.width,
            height: 32.0,
        }
    }

    fn action_rect(details: Rect, index: usize) -> Rect {
        Rect {
            x: details.x,
            y: details.y + 68.0 + index as f32 * 34.0,
            width: details.width,
            height: 29.0,
        }
    }

    fn syllable_option_rect(details: Rect, index: usize) -> Rect {
        let gap = 8.0;
        let width = (details.width - gap * 2.0) / 3.0;
        Rect {
            x: details.x + index.min(2) as f32 * (width + gap),
            y: details.y + 262.0,
            width,
            height: 30.0,
        }
    }

    fn clear_audio_rect(details: Rect) -> Rect {
        Rect {
            x: details.x,
            y: details.y + 344.0,
            width: details.width,
            height: 28.0,
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

    fn set_tab(&mut self, tab: MediaExplorerTab) {
        self.tab = tab;
        self.keyboard_focus = tab.index();
        self.editing_name = false;
        self.replace_name = false;
    }

    fn content_focus(&self) -> Option<usize> {
        self.keyboard_focus
            .checked_sub(MEDIA_CONTENT_FOCUS)
            .filter(|_| self.keyboard_focus != MEDIA_CLOSE_FOCUS)
    }

    fn move_focus(&mut self, forward: bool, card: Rect) {
        let controls = self.focus_order();
        let current = controls
            .iter()
            .position(|control| *control == self.keyboard_focus)
            .unwrap_or(if forward { controls.len() - 1 } else { 0 });
        self.keyboard_focus = if forward {
            controls[(current + 1) % controls.len()]
        } else {
            controls[(current + controls.len() - 1) % controls.len()]
        };
        self.editing_name =
            self.tab == MediaExplorerTab::RythmoBands && self.content_focus() == Some(1);
        self.replace_name = self.editing_name;
        self.ensure_audio_focus_visible(card);
    }

    fn ensure_audio_focus_visible(&mut self, card: Rect) {
        if self.tab != MediaExplorerTab::Audios {
            return;
        }
        let Some(focus) = self.content_focus() else {
            return;
        };
        let viewport = Self::audio_viewport(card);
        let row = focus / 2 + 1;
        let top = row as f32 * 72.0;
        if top < self.audio_scroll_offset {
            self.audio_scroll_offset = top;
        } else if top + 62.0 > self.audio_scroll_offset + viewport.height {
            self.audio_scroll_offset = top + 62.0 - viewport.height;
        }
        let max_scroll = ((self.languages.len() + 1) as f32 * 72.0 - viewport.height).max(0.0);
        self.audio_scroll_offset = self.audio_scroll_offset.clamp(0.0, max_scroll);
    }

    fn activate_focused(&mut self) -> LanguageModalResult {
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS {
            self.set_tab(MediaExplorerTab::from_index(self.keyboard_focus));
            return LanguageModalResult::Consumed;
        }
        if self.keyboard_focus == MEDIA_CLOSE_FOCUS {
            return LanguageModalResult::Close;
        }
        match self.tab {
            MediaExplorerTab::Videos => self.activate_video_control(),
            MediaExplorerTab::Audios => self.activate_audio_control(),
            MediaExplorerTab::RythmoBands => LanguageModalResult::Consumed,
        }
    }

    fn activate_video_control(&self) -> LanguageModalResult {
        match self.content_focus() {
            Some(0) if self.media.source.is_some() && self.media.active_proxy => {
                LanguageModalResult::SwitchVideo { use_proxy: false }
            }
            Some(1)
                if self.media.source.is_some()
                    && self.media.can_persist_default
                    && self.media.default_proxy =>
            {
                LanguageModalResult::SetDefaultVideo { use_proxy: false }
            }
            Some(2) if self.media.source.is_some() => {
                LanguageModalResult::DeleteVideo { use_proxy: false }
            }
            Some(3) if self.media.proxy.is_some() && !self.media.active_proxy => {
                LanguageModalResult::SwitchVideo { use_proxy: true }
            }
            Some(4)
                if self.media.proxy.is_some()
                    && self.media.can_persist_default
                    && !self.media.default_proxy =>
            {
                LanguageModalResult::SetDefaultVideo { use_proxy: true }
            }
            Some(5) if self.media.proxy.is_some() => {
                LanguageModalResult::DeleteVideo { use_proxy: true }
            }
            Some(6) => LanguageModalResult::AddVideo,
            Some(7) if self.media.source.is_some() => LanguageModalResult::CreateProxy,
            _ => LanguageModalResult::Consumed,
        }
    }

    fn handle_video_event(&mut self, event: &UiEvent, card: Rect) -> LanguageModalResult {
        match event {
            UiEvent::Activate => self.activate_focused(),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                self.activate_focused()
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                for (row_index, use_proxy) in [(0, false), (1, true)] {
                    let row = Self::video_row(card, row_index);
                    for action_index in 0..3 {
                        if Self::video_action(row, action_index).contains(*x, *y) {
                            self.keyboard_focus =
                                MEDIA_CONTENT_FOCUS + row_index * 3 + action_index;
                            return self.activate_video_control();
                        }
                    }
                    let available = if use_proxy {
                        self.media.proxy.is_some()
                    } else {
                        self.media.source.is_some()
                    };
                    if available && row.contains(*x, *y) {
                        return LanguageModalResult::SwitchVideo { use_proxy };
                    }
                }
                for index in 0..2 {
                    if Self::media_footer_action(card, index).contains(*x, *y) {
                        self.keyboard_focus = MEDIA_CONTENT_FOCUS + 6 + index;
                        return self.activate_video_control();
                    }
                }
                LanguageModalResult::Consumed
            }
            _ => LanguageModalResult::Consumed,
        }
    }

    fn handle_audio_event(&mut self, event: &UiEvent, card: Rect) -> LanguageModalResult {
        let max_scroll =
            ((self.languages.len() + 1) as f32 * 72.0 - Self::audio_viewport(card).height).max(0.0);
        match event {
            UiEvent::Activate => self.activate_focused(),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                self.activate_audio_control()
            }
            UiEvent::Scroll { x, y, delta, .. } if Self::audio_viewport(card).contains(*x, *y) => {
                self.audio_scroll_offset =
                    (self.audio_scroll_offset - *delta * 32.0).clamp(0.0, max_scroll);
                LanguageModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                for (index, language) in self.languages.iter().enumerate() {
                    let row = Self::audio_row(card, index + 1, self.audio_scroll_offset);
                    for action in 0..2 {
                        if Self::audio_action(row, action).contains(*x, *y) {
                            self.keyboard_focus = MEDIA_CONTENT_FOCUS + index * 2 + action;
                            return if action == 0 {
                                LanguageModalResult::PickInstrumental { id: language.id }
                            } else if language.instrumental_audio_path.is_some() {
                                LanguageModalResult::ClearInstrumental { id: language.id }
                            } else {
                                LanguageModalResult::Consumed
                            };
                        }
                    }
                }
                LanguageModalResult::Consumed
            }
            _ => LanguageModalResult::Consumed,
        }
    }

    fn activate_audio_control(&self) -> LanguageModalResult {
        let Some(focus) = self.content_focus() else {
            return LanguageModalResult::Consumed;
        };
        let Some(language) = self.languages.get(focus / 2) else {
            return LanguageModalResult::Consumed;
        };
        if focus.is_multiple_of(2) {
            LanguageModalResult::PickInstrumental { id: language.id }
        } else if language.instrumental_audio_path.is_some() {
            LanguageModalResult::ClearInstrumental { id: language.id }
        } else {
            LanguageModalResult::Consumed
        }
    }

    fn render_video_row<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        row: Rect,
        item: Option<&'a MediaVideoItem>,
        use_proxy: bool,
    ) {
        let active = self.media.active_proxy == use_proxy && item.is_some();
        let default = self.media.default_proxy == use_proxy && item.is_some();
        push_panel(
            quads,
            row,
            if active {
                [0.095, 0.15, 0.24, 1.0]
            } else {
                [0.065, 0.07, 0.09, 1.0]
            },
            10.0,
            if active {
                [0.31, 0.52, 0.90, 1.0]
            } else {
                [0.21, 0.24, 0.31, 1.0]
            },
        );
        labels.push(label(
            if use_proxy {
                t("media_explorer.video.proxy")
            } else {
                t("media_explorer.video.original")
            },
            Rect {
                x: row.x + 16.0,
                y: row.y + 10.0,
                width: row.width - 32.0,
                height: 24.0,
            },
            17.0,
            HAlign::Left,
            Some([224, 228, 240]),
        ));
        if active {
            labels.push(label(
                t("media_explorer.active"),
                Rect {
                    x: row.x + row.width - 200.0,
                    y: row.y + 10.0,
                    width: 86.0,
                    height: 22.0,
                },
                11.0,
                HAlign::Center,
                Some([126, 190, 255]),
            ));
        }
        if default {
            labels.push(label(
                t("media_explorer.default"),
                Rect {
                    x: row.x + row.width - 108.0,
                    y: row.y + 10.0,
                    width: 92.0,
                    height: 22.0,
                },
                11.0,
                HAlign::Center,
                Some([135, 218, 172]),
            ));
        }
        if let Some(item) = item {
            labels.push(LabelInfo {
                text: &item.name,
                bounds: Rect {
                    x: row.x + 16.0,
                    y: row.y + 38.0,
                    width: row.width - 32.0,
                    height: 22.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(14.0),
                color_override: None,
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &item.summary,
                bounds: Rect {
                    x: row.x + 16.0,
                    y: row.y + 61.0,
                    width: row.width - 32.0,
                    height: 20.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([169, 177, 197]),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &item.path,
                bounds: Rect {
                    x: row.x + 16.0,
                    y: row.y + 82.0,
                    width: row.width - 32.0,
                    height: 18.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(10.0),
                color_override: Some([116, 122, 139]),
                font_family_override: None,
            });
        } else {
            labels.push(label(
                t("media_explorer.missing"),
                Rect {
                    x: row.x + 16.0,
                    y: row.y + 48.0,
                    width: row.width - 32.0,
                    height: 30.0,
                },
                14.0,
                HAlign::Left,
                Some([125, 131, 148]),
            ));
        }
        for (index, (text, enabled, color)) in [
            (
                if active {
                    t("media_explorer.active")
                } else {
                    t("media_explorer.use")
                },
                item.is_some() && !active,
                [0.16, 0.31, 0.55, 1.0],
            ),
            (
                if default {
                    t("media_explorer.default")
                } else {
                    t("media_explorer.make_default")
                },
                item.is_some() && self.media.can_persist_default && !default,
                [0.12, 0.38, 0.27, 1.0],
            ),
            (
                if use_proxy {
                    t("media_explorer.delete")
                } else {
                    t("media_explorer.unlink")
                },
                item.is_some(),
                [0.42, 0.14, 0.17, 1.0],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            push_action(
                quads,
                labels,
                Self::video_action(row, index),
                text,
                color,
                enabled,
            );
        }
    }

    fn render_videos<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        card: Rect,
    ) {
        self.render_video_row(
            quads,
            labels,
            Self::video_row(card, 0),
            self.media.source.as_ref(),
            false,
        );
        self.render_video_row(
            quads,
            labels,
            Self::video_row(card, 1),
            self.media.proxy.as_ref(),
            true,
        );
        push_action(
            quads,
            labels,
            Self::media_footer_action(card, 0),
            t("media_explorer.video.add"),
            [0.18, 0.38, 0.72, 1.0],
            true,
        );
        push_action(
            quads,
            labels,
            Self::media_footer_action(card, 1),
            if self.media.proxy.is_some() {
                t("media_explorer.video.recreate_proxy")
            } else {
                t("media_explorer.video.create_proxy")
            },
            [0.25, 0.20, 0.40, 1.0],
            self.media.source.is_some(),
        );
    }

    fn render_audios<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        card: Rect,
    ) {
        let viewport = Self::audio_viewport(card);
        let source_row = Self::audio_row(card, 0, self.audio_scroll_offset);
        push_panel(
            quads,
            source_row,
            [0.065, 0.07, 0.09, 1.0],
            10.0,
            [0.21, 0.24, 0.31, 1.0],
        );
        labels.push(label(
            t("media_explorer.audio.original"),
            Rect {
                x: source_row.x + 16.0,
                y: source_row.y + 8.0,
                width: source_row.width - 32.0,
                height: 22.0,
            },
            15.0,
            HAlign::Left,
            None,
        ));
        labels.push(label(
            self.media
                .source
                .as_ref()
                .and_then(|item| item.audio_summary.as_deref())
                .unwrap_or(t("media_explorer.audio.none")),
            Rect {
                x: source_row.x + 16.0,
                y: source_row.y + 31.0,
                width: source_row.width - 32.0,
                height: 20.0,
            },
            12.0,
            HAlign::Left,
            Some([155, 162, 181]),
        ));
        for (index, language) in self.languages.iter().enumerate() {
            let row = Self::audio_row(card, index + 1, self.audio_scroll_offset);
            if row.y + row.height < viewport.y || row.y > viewport.y + viewport.height {
                continue;
            }
            push_panel(
                quads,
                row,
                [0.065, 0.07, 0.09, 1.0],
                10.0,
                [0.21, 0.24, 0.31, 1.0],
            );
            labels.push(label(
                &language.name,
                Rect {
                    x: row.x + 16.0,
                    y: row.y + 7.0,
                    width: row.width - 282.0,
                    height: 22.0,
                },
                14.0,
                HAlign::Left,
                None,
            ));
            labels.push(LabelInfo {
                text: language
                    .instrumental_audio_path
                    .as_deref()
                    .unwrap_or(t("languages.no_instrumental")),
                bounds: Rect {
                    x: row.x + 16.0,
                    y: row.y + 30.0,
                    width: row.width - 282.0,
                    height: 20.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([137, 143, 160]),
                font_family_override: None,
            });
            push_action(
                quads,
                labels,
                Self::audio_action(row, 0),
                t("media_explorer.audio.assign"),
                [0.18, 0.38, 0.72, 1.0],
                true,
            );
            push_action(
                quads,
                labels,
                Self::audio_action(row, 1),
                t("media_explorer.audio.remove"),
                [0.42, 0.14, 0.17, 1.0],
                language.instrumental_audio_path.is_some(),
            );
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

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
            return LanguageModalResult::Close;
        }
        if matches!(event, UiEvent::FocusNext | UiEvent::FocusPrevious) {
            self.move_focus(matches!(event, UiEvent::FocusNext), card);
            return LanguageModalResult::Consumed;
        }
        if let UiEvent::KeyInput { text } = event {
            if text == "\t" || text == "\u{b}" {
                self.move_focus(text == "\t", card);
                return LanguageModalResult::Consumed;
            }
        }
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS
            && matches!(event, UiEvent::CursorLeft | UiEvent::CursorRight)
        {
            let index = self.keyboard_focus;
            let next = if matches!(event, UiEvent::CursorRight) {
                (index + 1) % MEDIA_CONTENT_FOCUS
            } else {
                (index + MEDIA_CONTENT_FOCUS - 1) % MEDIA_CONTENT_FOCUS
            };
            self.set_tab(MediaExplorerTab::from_index(next));
            return LanguageModalResult::Consumed;
        }
        if self.keyboard_focus < MEDIA_CONTENT_FOCUS
            && (matches!(event, UiEvent::Activate)
                || matches!(event, UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " "))
        {
            return self.activate_focused();
        }
        if self.keyboard_focus == MEDIA_CLOSE_FOCUS
            && (matches!(event, UiEvent::Activate)
                || matches!(event, UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " "))
        {
            return LanguageModalResult::Close;
        }
        if let UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } = event {
            if !card.contains(*x, *y) {
                return LanguageModalResult::Close;
            }
            if Self::close_rect(card).contains(*x, *y) {
                return LanguageModalResult::Close;
            }
            for (index, tab) in [
                MediaExplorerTab::Videos,
                MediaExplorerTab::Audios,
                MediaExplorerTab::RythmoBands,
            ]
            .into_iter()
            .enumerate()
            {
                if Self::tab_rect(card, index).contains(*x, *y) {
                    self.set_tab(tab);
                    return LanguageModalResult::Consumed;
                }
            }
        }
        match self.tab {
            MediaExplorerTab::Videos => return self.handle_video_event(event, card),
            MediaExplorerTab::Audios => return self.handle_audio_event(event, card),
            MediaExplorerTab::RythmoBands => {}
        }

        match event {
            UiEvent::KeyInput { text } => {
                let focus = self.content_focus();
                if text == "\r" || text == "\n" || (text == " " && focus != Some(1)) {
                    let trimmed = self.name_input.trim().to_string();
                    return match focus {
                        Some(0 | 1) => LanguageModalResult::Consumed,
                        Some(2) if !trimmed.is_empty() => {
                            LanguageModalResult::Create { name: trimmed }
                        }
                        Some(3) if !trimmed.is_empty() && self.selected().is_some() => {
                            LanguageModalResult::Rename {
                                id: self.selected_id,
                                name: trimmed,
                            }
                        }
                        Some(4) if self.selected().is_some() => LanguageModalResult::Select {
                            id: self.selected_id,
                        },
                        Some(5) if self.selected().is_some() => {
                            LanguageModalResult::PickInstrumental {
                                id: self.selected_id,
                            }
                        }
                        Some(6) if self.languages.len() > 1 && self.selected().is_some() => {
                            LanguageModalResult::Delete {
                                id: self.selected_id,
                            }
                        }
                        Some(7) if self.selected().is_some() => {
                            LanguageModalResult::SetSyllableLanguage {
                                id: self.selected_id,
                                language: self
                                    .selected()
                                    .map(|language| language.syllable_language.toggled())
                                    .unwrap_or_default(),
                            }
                        }
                        Some(8)
                            if self
                                .selected()
                                .and_then(|language| language.instrumental_audio_path.as_ref())
                                .is_some() =>
                        {
                            LanguageModalResult::ClearInstrumental {
                                id: self.selected_id,
                            }
                        }
                        _ => LanguageModalResult::Consumed,
                    };
                }
                if self.editing_name {
                    self.handle_name_key(text);
                }
                LanguageModalResult::Consumed
            }
            UiEvent::CursorUp if self.content_focus() == Some(0) => {
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
            UiEvent::CursorDown if self.content_focus() == Some(0) => {
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
            UiEvent::CursorLeft | UiEvent::CursorRight if self.content_focus() == Some(7) => {
                let options = [
                    crate::project::SyllableLanguage::French,
                    crate::project::SyllableLanguage::English,
                    crate::project::SyllableLanguage::Spanish,
                ];
                let current = self
                    .selected()
                    .map(|selected| selected.syllable_language)
                    .unwrap_or_default();
                let index = options
                    .iter()
                    .position(|option| *option == current)
                    .unwrap_or(0);
                let next = if matches!(event, UiEvent::CursorRight) {
                    (index + 1) % options.len()
                } else {
                    (index + options.len() - 1) % options.len()
                };
                let language = options[next];
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
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS;
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
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 1;
                    self.editing_name = true;
                    self.replace_name = true;
                    return LanguageModalResult::Consumed;
                }
                self.editing_name = false;
                self.replace_name = false;

                let trimmed = self.name_input.trim().to_string();
                if Self::action_rect(details, 0).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 2;
                    if !trimmed.is_empty() {
                        return LanguageModalResult::Create { name: trimmed };
                    }
                } else if Self::action_rect(details, 1).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 3;
                    if !trimmed.is_empty() && self.selected().is_some() {
                        return LanguageModalResult::Rename {
                            id: self.selected_id,
                            name: trimmed,
                        };
                    }
                } else if Self::action_rect(details, 2).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 4;
                    if self.selected().is_some() {
                        return LanguageModalResult::Select {
                            id: self.selected_id,
                        };
                    }
                } else if Self::action_rect(details, 3).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 5;
                    if self.selected().is_some() {
                        return LanguageModalResult::PickInstrumental {
                            id: self.selected_id,
                        };
                    }
                } else if Self::action_rect(details, 4).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 6;
                    if self.languages.len() > 1 && self.selected().is_some() {
                        return LanguageModalResult::Delete {
                            id: self.selected_id,
                        };
                    }
                } else if Self::syllable_option_rect(details, 0).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::French
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::French,
                        };
                    }
                } else if Self::syllable_option_rect(details, 1).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::English
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::English,
                        };
                    }
                } else if Self::syllable_option_rect(details, 2).contains(*x, *y) {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 7;
                    if self.selected().is_some_and(|selected| {
                        selected.syllable_language != crate::project::SyllableLanguage::Spanish
                    }) {
                        return LanguageModalResult::SetSyllableLanguage {
                            id: self.selected_id,
                            language: crate::project::SyllableLanguage::Spanish,
                        };
                    }
                } else if Self::clear_audio_rect(details).contains(*x, *y)
                    && self
                        .selected()
                        .and_then(|language| language.instrumental_audio_path.as_ref())
                        .is_some()
                {
                    self.keyboard_focus = MEDIA_CONTENT_FOCUS + 8;
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
            t("media_explorer.title"),
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
            t("media_explorer.subtitle"),
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
        push_action(
            quads,
            labels,
            Self::close_rect(card),
            "×",
            [0.14, 0.15, 0.19, 1.0],
            true,
        );

        for (index, (tab, text)) in [
            (MediaExplorerTab::Videos, t("media_explorer.tab.videos")),
            (MediaExplorerTab::Audios, t("media_explorer.tab.audios")),
            (
                MediaExplorerTab::RythmoBands,
                t("media_explorer.tab.rythmo_bands"),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let selected = self.tab == tab;
            push_panel(
                quads,
                Self::tab_rect(card, index),
                if selected {
                    [0.16, 0.25, 0.43, 1.0]
                } else {
                    [0.075, 0.08, 0.105, 1.0]
                },
                8.0,
                if selected {
                    [0.31, 0.52, 0.90, 1.0]
                } else {
                    [0.20, 0.23, 0.30, 1.0]
                },
            );
            labels.push(label(
                text,
                Self::tab_rect(card, index),
                14.0,
                HAlign::Center,
                None,
            ));
        }

        match self.tab {
            MediaExplorerTab::Videos => {
                self.render_videos(quads, labels, card);
                self.render_keyboard_focus(quads, card);
                return;
            }
            MediaExplorerTab::Audios => {
                self.render_audios(quads, labels, card);
                self.render_keyboard_focus(quads, card);
                return;
            }
            MediaExplorerTab::RythmoBands => {}
        }

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
                y: details.y,
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
            !self.name_input.trim().is_empty(),
        );
        push_action(
            quads,
            labels,
            Self::action_rect(details, 1),
            t("languages.rename"),
            [0.15, 0.18, 0.24, 1.0],
            self.selected().is_some() && !self.name_input.trim().is_empty(),
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
                y: details.y + 242.0,
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
            crate::project::SyllableLanguage::Spanish,
        ]
        .into_iter()
        .enumerate()
        {
            let selected = language == selected_syllable_language;
            let focused = self.content_focus() == Some(7);
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
                y: details.y + 299.0,
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
                y: details.y + 315.0,
                width: details.width,
                height: 26.0,
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
        self.render_keyboard_focus(quads, card);
    }
}

pub(super) fn syllable_language_label(language: crate::project::SyllableLanguage) -> &'static str {
    match language {
        crate::project::SyllableLanguage::French => t("languages.syllables.french"),
        crate::project::SyllableLanguage::English => t("languages.syllables.english"),
        crate::project::SyllableLanguage::Spanish => t("languages.syllables.spanish"),
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

    fn language_modal(language: crate::project::SyllableLanguage) -> LanguageModal {
        let mut modal = LanguageModal::new(vec![item(language)], 1);
        modal.set_tab(MediaExplorerTab::RythmoBands);
        modal
    }

    #[test]
    fn syllable_language_control_has_keyboard_and_pointer_parity() {
        let mut modal = language_modal(crate::project::SyllableLanguage::French);
        modal.keyboard_focus = MEDIA_CONTENT_FOCUS + 7;
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
        assert_eq!(modal.keyboard_focus, MEDIA_CONTENT_FOCUS + 7);
    }

    #[test]
    fn tab_and_shift_tab_reach_the_syllable_control_deterministically() {
        let mut modal = language_modal(crate::project::SyllableLanguage::French);
        modal.handle_event(
            &UiEvent::KeyInput {
                text: "\u{b}".into(),
            },
            1280.0,
            720.0,
        );
        assert_eq!(modal.keyboard_focus, MediaExplorerTab::Audios.index());
        modal.set_tab(MediaExplorerTab::RythmoBands);
        for _ in 0..6 {
            modal.handle_event(&UiEvent::KeyInput { text: "\t".into() }, 1280.0, 720.0);
        }
        assert_eq!(modal.keyboard_focus, MEDIA_CONTENT_FOCUS + 7);
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
        let instrumental_label_y = details.y + 299.0;

        let syllable_options_bottom = LanguageModal::syllable_option_rect(details, 0).y + 30.0;
        assert!(syllable_options_bottom + 7.0 <= instrumental_label_y);
        assert!(instrumental_label_y >= delete_button.y + delete_button.height + 24.0);
        assert!(LanguageModal::clear_audio_rect(details).y + 28.0 <= details.y + details.height);
    }

    #[test]
    fn video_actions_separate_temporary_switch_from_persisted_default() {
        let video = MediaVideoItem {
            name: "movie.mov".into(),
            path: "movie.mov".into(),
            summary: "1920 × 1080".into(),
            audio_summary: None,
        };
        let mut modal = LanguageModal::with_media(
            vec![item(crate::project::SyllableLanguage::French)],
            1,
            MediaExplorerData {
                source: Some(video.clone()),
                proxy: Some(video),
                active_proxy: false,
                default_proxy: false,
                can_persist_default: true,
            },
        );
        let proxy_row = LanguageModal::video_row(LanguageModal::card(1280.0, 720.0), 1);

        let use_proxy = LanguageModal::video_action(proxy_row, 0);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: use_proxy.x + 2.0,
                    y: use_proxy.y + 2.0,
                },
                1280.0,
                720.0,
            ),
            LanguageModalResult::SwitchVideo { use_proxy: true }
        );

        let default_proxy = LanguageModal::video_action(proxy_row, 1);
        assert_eq!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: default_proxy.x + 2.0,
                    y: default_proxy.y + 2.0,
                },
                1280.0,
                720.0,
            ),
            LanguageModalResult::SetDefaultVideo { use_proxy: true }
        );

        modal.keyboard_focus = 0;
        assert_eq!(
            modal.handle_event(&UiEvent::CursorRight, 1280.0, 720.0),
            LanguageModalResult::Consumed
        );
        assert_eq!(modal.tab, MediaExplorerTab::Audios);
    }

    #[test]
    fn media_tab_order_includes_tabs_skips_disabled_actions_and_wraps() {
        let video = MediaVideoItem {
            name: "movie.mov".into(),
            path: "movie.mov".into(),
            summary: "1920 × 1080".into(),
            audio_summary: None,
        };
        let mut modal = LanguageModal::with_media(
            vec![item(crate::project::SyllableLanguage::French)],
            1,
            MediaExplorerData {
                source: Some(video.clone()),
                proxy: Some(video),
                active_proxy: false,
                default_proxy: false,
                can_persist_default: true,
            },
        );

        for expected in [1, 2, MEDIA_CONTENT_FOCUS + 2] {
            modal.handle_event(&UiEvent::FocusNext, 1280.0, 720.0);
            assert_eq!(modal.keyboard_focus, expected);
        }
        assert!(modal
            .keyboard_focus_label()
            .contains(t("media_explorer.video.original")));
        assert!(modal
            .keyboard_focus_label()
            .contains(t("media_explorer.unlink")));
        assert_eq!(
            modal.handle_event(&UiEvent::KeyInput { text: " ".into() }, 1280.0, 720.0),
            LanguageModalResult::DeleteVideo { use_proxy: false }
        );

        modal.set_tab(MediaExplorerTab::Videos);
        assert_eq!(
            modal.keyboard_focus_rect(LanguageModal::card(1280.0, 720.0)),
            Some(LanguageModal::tab_rect(
                LanguageModal::card(1280.0, 720.0),
                0
            ))
        );
        modal.handle_event(&UiEvent::FocusPrevious, 1280.0, 720.0);
        assert_eq!(modal.keyboard_focus, MEDIA_CLOSE_FOCUS);
        assert_eq!(modal.keyboard_focus_role(), "button");
    }

    #[test]
    fn audio_tab_keeps_keyboard_focus_visible_and_skips_unavailable_remove() {
        let mut languages = Vec::new();
        for id in 1..=10 {
            languages.push(LanguageListItem {
                id,
                name: format!("Language {id}"),
                instrumental_audio_path: None,
                syllable_language: crate::project::SyllableLanguage::French,
            });
        }
        let mut modal = LanguageModal::with_media(languages, 1, MediaExplorerData::default());
        modal.set_tab(MediaExplorerTab::Audios);

        for _ in 0..11 {
            modal.handle_event(&UiEvent::FocusNext, 1280.0, 720.0);
        }

        assert_eq!(modal.content_focus(), Some(9 * 2));
        assert!(modal.audio_scroll_offset > 0.0);
        assert!(modal.keyboard_focus_label().contains("Language 10"));
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
