use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;
use crate::project::{AudioSelection, ExportConfiguration, VideoExportAspect, VideoExportQuality};

const CARD_W: f32 = 1040.0;
const CARD_H: f32 = 700.0;
const NAV_W: f32 = 170.0;
const LANGUAGE_W: f32 = 300.0;
const ROW_H: f32 = 52.0;

#[derive(Clone, Debug)]
pub struct ExportLanguageOption {
    pub id: u64,
    pub name: String,
    pub has_instrumental: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExportPage {
    Video,
    Subtitles,
    Audio,
    Reports,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericField {
    Width,
    Height,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExportFocus {
    Page(usize),
    VideoToggle,
    VideoAspect(usize),
    VideoQuality(usize),
    VideoWidth,
    VideoHeight,
    VideoFps,
    VideoBrScale,
    VideoKaraokeScale,
    VideoPreroll,
    VideoCountdown,
    VideoCountdownStart,
    Format(usize),
    Language(usize),
    LanguageOriginal(usize),
    LanguageInstrumental(usize),
    Close,
    Export,
}

pub enum ExportModalResult {
    Consumed,
    Close { configuration: ExportConfiguration },
    Export { configuration: ExportConfiguration },
}

pub struct ExportModal {
    configuration: ExportConfiguration,
    languages: Vec<ExportLanguageOption>,
    page: ExportPage,
    language_scroll: f32,
    source_width: u32,
    source_height: u32,
    numeric_field: Option<NumericField>,
    numeric_text: String,
    replace_numeric: bool,
    fps_display: String,
    scale_display: String,
    karaoke_scale_display: String,
    preroll_display: String,
    countdown_display: String,
    width_display: String,
    height_display: String,
    resolution_display: String,
    keyboard_focus: usize,
}

impl ExportModal {
    pub fn new(
        video_width: u32,
        video_height: u32,
        languages: Vec<ExportLanguageOption>,
        mut configuration: ExportConfiguration,
    ) -> Self {
        let ids: Vec<u64> = languages.iter().map(|language| language.id).collect();
        configuration
            .selected_language_ids
            .retain(|id| ids.contains(id));
        if configuration.selected_language_ids.is_empty() {
            configuration
                .selected_language_ids
                .extend(ids.iter().copied());
        }
        for id in ids {
            configuration.audio_by_language.entry(id).or_default();
        }
        let mut modal = Self {
            configuration,
            languages,
            page: ExportPage::Video,
            language_scroll: 0.0,
            source_width: video_width.max(16),
            source_height: video_height.max(16),
            numeric_field: None,
            numeric_text: String::new(),
            replace_numeric: false,
            fps_display: String::new(),
            scale_display: String::new(),
            karaoke_scale_display: String::new(),
            preroll_display: String::new(),
            countdown_display: String::new(),
            width_display: String::new(),
            height_display: String::new(),
            resolution_display: String::new(),
            keyboard_focus: 0,
        };
        modal.refresh_display_strings();
        modal
    }

    fn focus_order(&self) -> Vec<ExportFocus> {
        let page_index = match self.page {
            ExportPage::Video => 0,
            ExportPage::Subtitles => 1,
            ExportPage::Audio => 2,
            ExportPage::Reports => 3,
        };
        let mut order = vec![ExportFocus::Page(page_index)];
        match self.page {
            ExportPage::Video => {
                order.push(ExportFocus::VideoToggle);
                order.extend((0..3).map(ExportFocus::VideoAspect));
                order.extend((0..5).map(ExportFocus::VideoQuality));
                if self.configuration.video_quality == VideoExportQuality::Custom {
                    order.push(ExportFocus::VideoWidth);
                    order.push(ExportFocus::VideoHeight);
                }
                order.extend([
                    ExportFocus::VideoFps,
                    ExportFocus::VideoBrScale,
                    ExportFocus::VideoKaraokeScale,
                    ExportFocus::VideoPreroll,
                    ExportFocus::VideoCountdown,
                ]);
                if self.configuration.countdown_enabled {
                    order.push(ExportFocus::VideoCountdownStart);
                }
            }
            ExportPage::Subtitles => order.extend((0..4).map(ExportFocus::Format)),
            ExportPage::Audio => order.extend((0..3).map(ExportFocus::Format)),
            ExportPage::Reports => order.extend((0..3).map(ExportFocus::Format)),
        }
        for (index, language) in self.languages.iter().enumerate() {
            order.push(ExportFocus::Language(index));
            order.push(ExportFocus::LanguageOriginal(index));
            if language.has_instrumental {
                order.push(ExportFocus::LanguageInstrumental(index));
            }
        }
        order.extend([ExportFocus::Close, ExportFocus::Export]);
        order
    }

    fn current_focus(&self) -> ExportFocus {
        self.focus_order()
            .get(self.keyboard_focus)
            .copied()
            .unwrap_or(ExportFocus::Page(0))
    }

    fn set_focus(&mut self, focus: ExportFocus) {
        if let Some(index) = self.focus_order().iter().position(|item| *item == focus) {
            self.keyboard_focus = index;
            self.ensure_language_visible();
        }
    }

    fn move_keyboard_focus(&mut self, direction: i32) {
        self.finish_numeric();
        let count = self.focus_order().len();
        if count == 0 {
            return;
        }
        self.keyboard_focus =
            (self.keyboard_focus as i32 + direction).rem_euclid(count as i32) as usize;
        self.ensure_language_visible();
    }

    fn move_page(&mut self, direction: i32) {
        self.finish_numeric();
        let current = match self.page {
            ExportPage::Video => 0,
            ExportPage::Subtitles => 1,
            ExportPage::Audio => 2,
            ExportPage::Reports => 3,
        };
        let next = (current as i32 + direction).rem_euclid(4) as usize;
        self.page = Self::page_for_index(next);
        self.keyboard_focus = 0;
    }

    fn ensure_language_visible(&mut self) {
        let index = match self.current_focus() {
            ExportFocus::Language(index)
            | ExportFocus::LanguageOriginal(index)
            | ExportFocus::LanguageInstrumental(index) => index,
            _ => return,
        };
        let viewport = Self::language_list_viewport(Self::card(1280.0, 720.0));
        self.language_scroll = (index as f32 * ROW_H - viewport.height + ROW_H)
            .max(0.0)
            .min(self.max_language_scroll(viewport));
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.current_focus() {
            ExportFocus::Page(index) => match index {
                0 => t("export_hub.video").to_string(),
                1 => t("export_hub.subtitles").to_string(),
                2 => t("export_hub.audio").to_string(),
                _ => t("export_hub.reports").to_string(),
            },
            ExportFocus::VideoToggle => t("export_hub.video_mp4").to_string(),
            ExportFocus::VideoAspect(_) => t("export_hub.aspect").to_string(),
            ExportFocus::VideoQuality(_) => t("export_hub.quality").to_string(),
            ExportFocus::VideoWidth => t("export_modal.width").to_string(),
            ExportFocus::VideoHeight => t("export_modal.height").to_string(),
            ExportFocus::VideoFps => t("export_modal.fps").to_string(),
            ExportFocus::VideoBrScale => t("export_modal.br_scale").to_string(),
            ExportFocus::VideoKaraokeScale => t("export_modal.karaoke_text_scale").to_string(),
            ExportFocus::VideoPreroll => t("export_hub.preroll").to_string(),
            ExportFocus::VideoCountdown => t("export_hub.countdown").to_string(),
            ExportFocus::VideoCountdownStart => t("export_hub.countdown_from").to_string(),
            ExportFocus::Format(index) => self.format_name(index).to_string(),
            ExportFocus::Language(index) => self
                .languages
                .get(index)
                .map(|language| language.name.clone())
                .unwrap_or_else(|| t("export_hub.languages").to_string()),
            ExportFocus::LanguageOriginal(index) => self
                .languages
                .get(index)
                .map(|language| {
                    format!(
                        "{}, {}",
                        language.name,
                        t("export_modal.export_original_audio")
                    )
                })
                .unwrap_or_else(|| t("export_hub.languages").to_string()),
            ExportFocus::LanguageInstrumental(index) => self
                .languages
                .get(index)
                .map(|language| {
                    format!(
                        "{}, {}",
                        language.name,
                        t("export_modal.export_instrumental_audio")
                    )
                })
                .unwrap_or_else(|| t("export_hub.languages").to_string()),
            ExportFocus::Close => t("export_hub.close").to_string(),
            ExportFocus::Export => t("export_modal.export").to_string(),
        }
    }

    pub fn keyboard_selection_label(&self) -> Option<String> {
        let state = |checked: bool| {
            if checked {
                t("accessibility.checked")
            } else {
                t("accessibility.unchecked")
            }
        };
        match self.current_focus() {
            ExportFocus::VideoToggle => Some(format!(
                "{}, {}",
                self.keyboard_focus_label(),
                state(self.configuration.video_enabled)
            )),
            ExportFocus::VideoAspect(index) => Some(format!(
                "{} {}",
                t("export_hub.aspect"),
                [t("export_hub.source_aspect"), "16:9", "9:16"]
                    .get(index)
                    .copied()
                    .unwrap_or_default()
            )),
            ExportFocus::VideoQuality(index) => Some(format!(
                "{} {}",
                t("export_hub.quality"),
                ["720p", "1080p", "1440p", "8K", t("export_hub.custom")]
                    .get(index)
                    .copied()
                    .unwrap_or_default()
            )),
            ExportFocus::VideoWidth => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.width_display
            )),
            ExportFocus::VideoHeight => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.height_display
            )),
            ExportFocus::VideoFps => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.fps_display
            )),
            ExportFocus::VideoBrScale => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.scale_display
            )),
            ExportFocus::VideoKaraokeScale => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.karaoke_scale_display
            )),
            ExportFocus::VideoPreroll => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.preroll_display
            )),
            ExportFocus::VideoCountdown => Some(format!(
                "{}, {}",
                self.keyboard_focus_label(),
                state(self.configuration.countdown_enabled)
            )),
            ExportFocus::VideoCountdownStart => Some(format!(
                "{} {}",
                self.keyboard_focus_label(),
                self.countdown_display
            )),
            ExportFocus::Format(index) => Some(format!(
                "{}, {}",
                self.format_name(index),
                state(self.format_checked(index))
            )),
            ExportFocus::Language(index) => self.languages.get(index).map(|language| {
                format!("{}, {}", language.name, state(self.selected(language.id)))
            }),
            ExportFocus::LanguageOriginal(index) => self.languages.get(index).map(|language| {
                let selection = self
                    .configuration
                    .audio_by_language
                    .get(&language.id)
                    .copied()
                    .unwrap_or_default();
                format!(
                    "{}, {}",
                    self.keyboard_focus_label(),
                    state(selection.original)
                )
            }),
            ExportFocus::LanguageInstrumental(index) => self.languages.get(index).map(|language| {
                let selection = self
                    .configuration
                    .audio_by_language
                    .get(&language.id)
                    .copied()
                    .unwrap_or_default();
                format!(
                    "{}, {}",
                    self.keyboard_focus_label(),
                    state(selection.instrumental)
                )
            }),
            ExportFocus::Page(_) | ExportFocus::Close | ExportFocus::Export => None,
        }
    }

    fn refresh_display_strings(&mut self) {
        self.fps_display = format!("{:.0}", self.configuration.fps);
        self.scale_display = format!("{:.0}%", self.configuration.br_scale * 100.0);
        self.karaoke_scale_display =
            format!("{:.0}%", self.configuration.karaoke_text_scale * 100.0);
        self.preroll_display = format!("{:.1} s", self.configuration.pre_roll_seconds);
        self.countdown_display = self.configuration.countdown_start.to_string();
        self.width_display = self.configuration.custom_width.to_string();
        self.height_display = self.configuration.custom_height.to_string();
        let (width, height) =
            resolve_video_dimensions(&self.configuration, self.source_width, self.source_height);
        self.resolution_display = format!("{width} × {height} px");
    }

    fn card(screen_w: f32, screen_h: f32) -> Rect {
        let width = CARD_W.min((screen_w - 24.0).max(620.0));
        let height = CARD_H.min((screen_h - 24.0).max(500.0));
        Rect {
            x: (screen_w - width) / 2.0,
            y: (screen_h - height) / 2.0,
            width,
            height,
        }
    }

    fn nav_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 18.0,
            y: card.y + 68.0,
            width: NAV_W,
            height: card.height - 132.0,
        }
    }

    fn language_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - LANGUAGE_W - 18.0,
            y: card.y + 68.0,
            width: LANGUAGE_W,
            height: card.height - 132.0,
        }
    }

    fn content_rect(card: Rect) -> Rect {
        let nav = Self::nav_rect(card);
        let languages = Self::language_rect(card);
        Rect {
            x: nav.x + nav.width + 18.0,
            y: nav.y,
            width: (languages.x - 18.0 - (nav.x + nav.width + 18.0)).max(300.0),
            height: nav.height,
        }
    }

    fn nav_item(card: Rect, index: usize) -> Rect {
        let nav = Self::nav_rect(card);
        Rect {
            x: nav.x,
            y: nav.y + index as f32 * 54.0,
            width: nav.width,
            height: 44.0,
        }
    }

    fn export_button(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - 174.0,
            y: card.y + card.height - 48.0,
            width: 154.0,
            height: 36.0,
        }
    }

    fn close_button(card: Rect) -> Rect {
        Rect {
            x: card.x + 20.0,
            y: card.y + card.height - 48.0,
            width: 108.0,
            height: 36.0,
        }
    }

    fn language_list_viewport(card: Rect) -> Rect {
        let panel = Self::language_rect(card);
        Rect {
            x: panel.x + 8.0,
            y: panel.y + 42.0,
            width: panel.width - 16.0,
            height: panel.height - 50.0,
        }
    }

    fn max_language_scroll(&self, viewport: Rect) -> f32 {
        (self.languages.len() as f32 * ROW_H - viewport.height).max(0.0)
    }

    fn page_for_index(index: usize) -> ExportPage {
        match index {
            0 => ExportPage::Video,
            1 => ExportPage::Subtitles,
            2 => ExportPage::Audio,
            _ => ExportPage::Reports,
        }
    }

    fn selected(&self, id: u64) -> bool {
        self.configuration.selected_language_ids.contains(&id)
    }

    fn toggle_language(&mut self, id: u64) {
        if let Some(index) = self
            .configuration
            .selected_language_ids
            .iter()
            .position(|selected| *selected == id)
        {
            if self.configuration.selected_language_ids.len() > 1 {
                self.configuration.selected_language_ids.remove(index);
            }
        } else {
            self.configuration.selected_language_ids.push(id);
        }
    }

    fn toggle_language_audio(&mut self, id: u64, instrumental: bool, available: bool) {
        let selection = self.configuration.audio_by_language.entry(id).or_default();
        if instrumental {
            if !available {
                return;
            }
            if selection.original || !selection.instrumental {
                selection.instrumental = !selection.instrumental;
            }
        } else if selection.instrumental || !selection.original {
            selection.original = !selection.original;
        }
    }

    fn adjust_focus_value(&mut self, direction: i32) {
        let direction = direction.signum();
        if direction == 0 {
            return;
        }
        match self.current_focus() {
            ExportFocus::VideoToggle => {
                self.configuration.video_enabled = !self.configuration.video_enabled
            }
            ExportFocus::VideoAspect(index) => {
                let next = (index as i32 + direction).rem_euclid(3) as usize;
                self.configuration.video_aspect = match next {
                    0 => VideoExportAspect::Source,
                    1 => VideoExportAspect::Landscape16x9,
                    _ => VideoExportAspect::Portrait9x16,
                };
            }
            ExportFocus::VideoQuality(index) => {
                let next = (index as i32 + direction).rem_euclid(5) as usize;
                self.configuration.video_quality = match next {
                    0 => VideoExportQuality::P720,
                    1 => VideoExportQuality::P1080,
                    2 => VideoExportQuality::P1440,
                    3 => VideoExportQuality::P8k,
                    _ => VideoExportQuality::Custom,
                };
            }
            ExportFocus::VideoWidth => {
                self.configuration.custom_width =
                    (self.configuration.custom_width as i32 + direction * 2).clamp(16, 8192) as u32;
            }
            ExportFocus::VideoHeight => {
                self.configuration.custom_height = (self.configuration.custom_height as i32
                    + direction * 2)
                    .clamp(16, 8192) as u32;
            }
            ExportFocus::VideoFps => {
                self.configuration.fps =
                    (self.configuration.fps + direction as f64).clamp(1.0, 480.0);
            }
            ExportFocus::VideoBrScale => {
                self.configuration.br_scale =
                    (self.configuration.br_scale + direction as f32 * 0.1).clamp(0.5, 2.0);
            }
            ExportFocus::VideoKaraokeScale => {
                self.configuration.karaoke_text_scale = (self.configuration.karaoke_text_scale
                    + direction as f32 * 0.1)
                    .clamp(0.5, 2.0);
            }
            ExportFocus::VideoPreroll => {
                self.configuration.pre_roll_seconds = (self.configuration.pre_roll_seconds
                    + direction as f64 * 0.5)
                    .clamp(0.0, 120.0);
            }
            ExportFocus::VideoCountdown => {
                self.configuration.countdown_enabled = !self.configuration.countdown_enabled;
            }
            ExportFocus::VideoCountdownStart => {
                let value = self.configuration.countdown_start as i32 + direction;
                self.configuration.countdown_start = value.clamp(1, 30) as u32;
            }
            ExportFocus::Format(_)
            | ExportFocus::Language(_)
            | ExportFocus::LanguageOriginal(_)
            | ExportFocus::LanguageInstrumental(_)
            | ExportFocus::Page(_)
            | ExportFocus::Close
            | ExportFocus::Export => {
                self.move_keyboard_focus(direction);
                return;
            }
        }
        self.refresh_display_strings();
    }

    fn focus_value_adjustable(&self) -> bool {
        matches!(
            self.current_focus(),
            ExportFocus::VideoToggle
                | ExportFocus::VideoAspect(_)
                | ExportFocus::VideoQuality(_)
                | ExportFocus::VideoWidth
                | ExportFocus::VideoHeight
                | ExportFocus::VideoFps
                | ExportFocus::VideoBrScale
                | ExportFocus::VideoKaraokeScale
                | ExportFocus::VideoPreroll
                | ExportFocus::VideoCountdown
                | ExportFocus::VideoCountdownStart
        )
    }

    fn activate_focus(&mut self) -> Option<ExportModalResult> {
        match self.current_focus() {
            ExportFocus::Page(index) => {
                self.finish_numeric();
                self.page = Self::page_for_index(index);
                let first_content = match self.page {
                    ExportPage::Video => ExportFocus::VideoToggle,
                    ExportPage::Subtitles | ExportPage::Audio | ExportPage::Reports => {
                        ExportFocus::Format(0)
                    }
                };
                self.set_focus(first_content);
            }
            ExportFocus::VideoToggle => {
                self.configuration.video_enabled = !self.configuration.video_enabled
            }
            ExportFocus::VideoAspect(index) => {
                self.configuration.video_aspect = match index {
                    0 => VideoExportAspect::Source,
                    1 => VideoExportAspect::Landscape16x9,
                    _ => VideoExportAspect::Portrait9x16,
                };
            }
            ExportFocus::VideoQuality(index) => {
                self.configuration.video_quality = match index {
                    0 => VideoExportQuality::P720,
                    1 => VideoExportQuality::P1080,
                    2 => VideoExportQuality::P1440,
                    3 => VideoExportQuality::P8k,
                    _ => VideoExportQuality::Custom,
                };
            }
            ExportFocus::VideoWidth => self.begin_numeric(NumericField::Width),
            ExportFocus::VideoHeight => self.begin_numeric(NumericField::Height),
            ExportFocus::VideoCountdown => {
                self.configuration.countdown_enabled = !self.configuration.countdown_enabled;
            }
            ExportFocus::Format(index) => self.toggle_format(index),
            ExportFocus::Language(index) => {
                if let Some(language) = self.languages.get(index).cloned() {
                    self.toggle_language(language.id);
                }
            }
            ExportFocus::LanguageOriginal(index) => {
                if let Some(language) = self.languages.get(index).cloned() {
                    self.toggle_language_audio(language.id, false, true);
                }
            }
            ExportFocus::LanguageInstrumental(index) => {
                if let Some(language) = self.languages.get(index).cloned() {
                    self.toggle_language_audio(language.id, true, language.has_instrumental);
                }
            }
            ExportFocus::Close => {
                self.finish_numeric();
                return Some(ExportModalResult::Close {
                    configuration: self.configuration.clone(),
                });
            }
            ExportFocus::Export => {
                if self.any_format_selected()
                    && !self.configuration.selected_language_ids.is_empty()
                {
                    self.finish_numeric();
                    return Some(ExportModalResult::Export {
                        configuration: self.configuration.clone(),
                    });
                }
            }
            ExportFocus::VideoFps
            | ExportFocus::VideoBrScale
            | ExportFocus::VideoKaraokeScale
            | ExportFocus::VideoPreroll
            | ExportFocus::VideoCountdownStart => {}
        }
        self.refresh_display_strings();
        None
    }

    fn any_format_selected(&self) -> bool {
        let cfg = &self.configuration;
        cfg.video_enabled
            || cfg.subtitle_formats.json
            || cfg.subtitle_formats.srt
            || cfg.subtitle_formats.ass
            || cfg.subtitle_formats.detx
            || cfg.audio_formats.mp3
            || cfg.audio_formats.wav
            || cfg.audio_formats.bwf_stems
            || cfg.cross_reference_formats.csv
            || cfg.cross_reference_formats.pdf
            || cfg.presence_grid_pdf
    }

    fn format_name(&self, index: usize) -> &'static str {
        match self.page {
            ExportPage::Subtitles => ["JSON", "SRT", "ASS", "DETX"]
                .get(index)
                .copied()
                .unwrap_or("Format"),
            ExportPage::Audio => ["MP3", "WAV", "BWF stems"]
                .get(index)
                .copied()
                .unwrap_or("Format"),
            ExportPage::Reports => match index {
                0 => "CSV",
                1 => "PDF",
                2 => "Grille PDF",
                _ => "Format",
            },
            ExportPage::Video => "Format",
        }
    }

    fn format_checked(&self, index: usize) -> bool {
        match self.page {
            ExportPage::Subtitles => match index {
                0 => self.configuration.subtitle_formats.json,
                1 => self.configuration.subtitle_formats.srt,
                2 => self.configuration.subtitle_formats.ass,
                _ => self.configuration.subtitle_formats.detx,
            },
            ExportPage::Audio => match index {
                0 => self.configuration.audio_formats.mp3,
                1 => self.configuration.audio_formats.wav,
                _ => self.configuration.audio_formats.bwf_stems,
            },
            ExportPage::Reports => match index {
                0 => self.configuration.cross_reference_formats.csv,
                1 => self.configuration.cross_reference_formats.pdf,
                _ => self.configuration.presence_grid_pdf,
            },
            ExportPage::Video => false,
        }
    }

    fn toggle_format(&mut self, index: usize) {
        match self.page {
            ExportPage::Subtitles => match index {
                0 => self.configuration.subtitle_formats.json ^= true,
                1 => self.configuration.subtitle_formats.srt ^= true,
                2 => self.configuration.subtitle_formats.ass ^= true,
                _ => self.configuration.subtitle_formats.detx ^= true,
            },
            ExportPage::Audio => match index {
                0 => self.configuration.audio_formats.mp3 ^= true,
                1 => self.configuration.audio_formats.wav ^= true,
                _ => self.configuration.audio_formats.bwf_stems ^= true,
            },
            ExportPage::Reports => match index {
                0 => self.configuration.cross_reference_formats.csv ^= true,
                1 => self.configuration.cross_reference_formats.pdf ^= true,
                _ => self.configuration.presence_grid_pdf ^= true,
            },
            ExportPage::Video => {}
        }
    }

    fn begin_numeric(&mut self, field: NumericField) {
        self.numeric_field = Some(field);
        self.numeric_text = match field {
            NumericField::Width => self.configuration.custom_width,
            NumericField::Height => self.configuration.custom_height,
        }
        .to_string();
        self.replace_numeric = true;
    }

    fn finish_numeric(&mut self) {
        let value = self
            .numeric_text
            .parse::<u32>()
            .unwrap_or(match self.numeric_field {
                Some(NumericField::Height) => self.configuration.custom_height,
                _ => self.configuration.custom_width,
            })
            .clamp(16, 8192);
        let value = if value % 2 == 0 {
            value
        } else {
            (value + 1).min(8192)
        };
        match self.numeric_field.take() {
            Some(NumericField::Width) => self.configuration.custom_width = value,
            Some(NumericField::Height) => self.configuration.custom_height = value,
            None => {}
        }
        self.numeric_text.clear();
        self.replace_numeric = false;
        self.refresh_display_strings();
    }

    fn handle_numeric_key(&mut self, text: &str) -> bool {
        if self.numeric_field.is_none() {
            return false;
        }
        if text == "\r" || text == "\n" || text == "\t" {
            self.finish_numeric();
            return true;
        }
        if text == "\x08" || text == "\x7f" {
            if self.replace_numeric {
                self.numeric_text.clear();
                self.replace_numeric = false;
            } else {
                self.numeric_text.pop();
            }
            return true;
        }
        if text.chars().all(|character| character.is_ascii_digit()) {
            if self.replace_numeric {
                self.numeric_text.clear();
                self.replace_numeric = false;
            }
            if self.numeric_text.len() < 5 {
                self.numeric_text.push_str(text);
            }
        }
        true
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ExportModalResult {
        let card = Self::card(screen_w, screen_h);
        let content = Self::content_rect(card);
        let language_viewport = Self::language_list_viewport(card);

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return ExportModalResult::Close {
                        configuration: self.configuration.clone(),
                    };
                }
                if text == "\t" || text == "\u{b}" {
                    if self.numeric_field.is_some() {
                        self.finish_numeric();
                    }
                    self.move_keyboard_focus(if text == "\t" { 1 } else { -1 });
                    return ExportModalResult::Consumed;
                }
                if self.handle_numeric_key(text) {
                    return ExportModalResult::Consumed;
                }
                if text == "\r" || text == "\n" || text == " " {
                    if let Some(result) = self.activate_focus() {
                        return result;
                    }
                }
                ExportModalResult::Consumed
            }
            UiEvent::FocusNext => {
                self.move_keyboard_focus(1);
                ExportModalResult::Consumed
            }
            UiEvent::FocusPrevious => {
                self.move_keyboard_focus(-1);
                ExportModalResult::Consumed
            }
            UiEvent::CursorUp => {
                // The export pages are presented as a vertical navigation
                // rail.  While that rail has focus, Up/Down must move between
                // Video, Subtitles, Audio and References instead of jumping
                // into the page's first control.
                if matches!(self.current_focus(), ExportFocus::Page(_)) {
                    self.move_page(-1);
                } else if self.focus_value_adjustable() {
                    self.adjust_focus_value(1);
                } else {
                    self.move_keyboard_focus(-1);
                }
                ExportModalResult::Consumed
            }
            UiEvent::CursorDown => {
                if matches!(self.current_focus(), ExportFocus::Page(_)) {
                    self.move_page(1);
                } else if self.focus_value_adjustable() {
                    self.adjust_focus_value(-1);
                } else {
                    self.move_keyboard_focus(1);
                }
                ExportModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                if matches!(self.current_focus(), ExportFocus::Page(_)) {
                    self.move_page(-1);
                } else if self.focus_value_adjustable() {
                    self.adjust_focus_value(-1);
                } else {
                    self.move_keyboard_focus(-1);
                }
                ExportModalResult::Consumed
            }
            UiEvent::CursorRight => {
                if matches!(self.current_focus(), ExportFocus::Page(_)) {
                    self.move_page(1);
                } else if self.focus_value_adjustable() {
                    self.adjust_focus_value(1);
                } else {
                    self.move_keyboard_focus(1);
                }
                ExportModalResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } if language_viewport.contains(*x, *y) => {
                let max = self.max_language_scroll(language_viewport);
                self.language_scroll = (self.language_scroll - *delta * 34.0).clamp(0.0, max);
                ExportModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) || Self::close_button(card).contains(*x, *y) {
                    self.set_focus(ExportFocus::Close);
                    return ExportModalResult::Close {
                        configuration: self.configuration.clone(),
                    };
                }
                if Self::export_button(card).contains(*x, *y) {
                    self.set_focus(ExportFocus::Export);
                    if self.any_format_selected()
                        && !self.configuration.selected_language_ids.is_empty()
                    {
                        self.finish_numeric();
                        return ExportModalResult::Export {
                            configuration: self.configuration.clone(),
                        };
                    }
                    return ExportModalResult::Consumed;
                }

                for index in 0..4 {
                    if Self::nav_item(card, index).contains(*x, *y) {
                        self.finish_numeric();
                        self.page = Self::page_for_index(index);
                        self.set_focus(ExportFocus::Page(index));
                        return ExportModalResult::Consumed;
                    }
                }

                if language_viewport.contains(*x, *y) {
                    let row_index = ((*y - language_viewport.y + self.language_scroll) / ROW_H)
                        .floor()
                        .max(0.0) as usize;
                    if let Some(language) = self.languages.get(row_index).cloned() {
                        let row_y =
                            language_viewport.y + row_index as f32 * ROW_H - self.language_scroll;
                        let instrumental = Rect {
                            x: language_viewport.x + language_viewport.width - 35.0,
                            y: row_y + 15.0,
                            width: 27.0,
                            height: 24.0,
                        };
                        let original = Rect {
                            x: instrumental.x - 31.0,
                            y: instrumental.y,
                            width: 27.0,
                            height: 24.0,
                        };
                        if instrumental.contains(*x, *y) {
                            self.set_focus(ExportFocus::LanguageInstrumental(row_index));
                            self.toggle_language_audio(
                                language.id,
                                true,
                                language.has_instrumental,
                            );
                        } else if original.contains(*x, *y) {
                            self.set_focus(ExportFocus::LanguageOriginal(row_index));
                            self.toggle_language_audio(language.id, false, true);
                        } else {
                            self.set_focus(ExportFocus::Language(row_index));
                            self.toggle_language(language.id);
                        }
                    }
                    return ExportModalResult::Consumed;
                }

                self.finish_numeric();
                match self.page {
                    ExportPage::Video => self.handle_video_click(content, *x, *y),
                    ExportPage::Subtitles => self.handle_subtitle_click(content, *x, *y),
                    ExportPage::Audio => self.handle_audio_click(content, *x, *y),
                    ExportPage::Reports => self.handle_report_click(content, *x, *y),
                }
                self.refresh_display_strings();
                ExportModalResult::Consumed
            }
            _ => ExportModalResult::Consumed,
        }
    }

    fn handle_video_click(&mut self, content: Rect, x: f32, y: f32) {
        if option_rect(content, 0).contains(x, y) {
            self.set_focus(ExportFocus::VideoToggle);
            self.configuration.video_enabled = !self.configuration.video_enabled;
            return;
        }
        for (index, aspect) in [
            VideoExportAspect::Source,
            VideoExportAspect::Landscape16x9,
            VideoExportAspect::Portrait9x16,
        ]
        .into_iter()
        .enumerate()
        {
            if segment_rect(content, 112.0, 3, index).contains(x, y) {
                self.set_focus(ExportFocus::VideoAspect(index));
                self.configuration.video_aspect = aspect;
                return;
            }
        }
        for (index, quality) in [
            VideoExportQuality::P720,
            VideoExportQuality::P1080,
            VideoExportQuality::P1440,
            VideoExportQuality::P8k,
            VideoExportQuality::Custom,
        ]
        .into_iter()
        .enumerate()
        {
            if segment_rect(content, 180.0, 5, index).contains(x, y) {
                self.set_focus(ExportFocus::VideoQuality(index));
                self.configuration.video_quality = quality;
                return;
            }
        }
        if self.configuration.video_quality == VideoExportQuality::Custom {
            let (width, height) = dimension_rects(content);
            if width.contains(x, y) {
                self.set_focus(ExportFocus::VideoWidth);
                self.begin_numeric(NumericField::Width);
                return;
            }
            if height.contains(x, y) {
                self.set_focus(ExportFocus::VideoHeight);
                self.begin_numeric(NumericField::Height);
                return;
            }
        }
        if stepper_minus(content, 276.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoFps);
            self.configuration.fps = (self.configuration.fps - 1.0).max(1.0);
        } else if stepper_plus(content, 276.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoFps);
            self.configuration.fps = (self.configuration.fps + 1.0).min(480.0);
        } else if stepper_minus(content, 326.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoBrScale);
            self.configuration.br_scale = (self.configuration.br_scale - 0.1).max(0.5);
        } else if stepper_plus(content, 326.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoBrScale);
            self.configuration.br_scale = (self.configuration.br_scale + 0.1).min(2.0);
        } else if stepper_minus(content, 376.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoKaraokeScale);
            self.configuration.karaoke_text_scale =
                (self.configuration.karaoke_text_scale - 0.1).max(0.5);
        } else if stepper_plus(content, 376.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoKaraokeScale);
            self.configuration.karaoke_text_scale =
                (self.configuration.karaoke_text_scale + 0.1).min(2.0);
        } else if stepper_minus(content, 426.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoPreroll);
            self.configuration.pre_roll_seconds =
                (self.configuration.pre_roll_seconds - 0.5).max(0.0);
        } else if stepper_plus(content, 426.0).contains(x, y) {
            self.set_focus(ExportFocus::VideoPreroll);
            self.configuration.pre_roll_seconds =
                (self.configuration.pre_roll_seconds + 0.5).min(120.0);
        } else if (Rect {
            x: content.x,
            y: content.y + 466.0,
            width: content.width,
            height: 34.0,
        })
        .contains(x, y)
        {
            self.set_focus(ExportFocus::VideoCountdown);
            self.configuration.countdown_enabled = !self.configuration.countdown_enabled;
        } else if self.configuration.countdown_enabled
            && stepper_minus(content, 506.0).contains(x, y)
        {
            self.set_focus(ExportFocus::VideoCountdownStart);
            self.configuration.countdown_start =
                self.configuration.countdown_start.saturating_sub(1).max(1);
        } else if self.configuration.countdown_enabled
            && stepper_plus(content, 506.0).contains(x, y)
        {
            self.set_focus(ExportFocus::VideoCountdownStart);
            self.configuration.countdown_start =
                self.configuration.countdown_start.saturating_add(1).min(30);
        }
    }

    fn handle_subtitle_click(&mut self, content: Rect, x: f32, y: f32) {
        for index in 0..4 {
            if format_card(content, index).contains(x, y) {
                self.set_focus(ExportFocus::Format(index));
                self.toggle_format(index);
            }
        }
    }

    fn handle_audio_click(&mut self, content: Rect, x: f32, y: f32) {
        for index in 0..3 {
            if format_card(content, index).contains(x, y) {
                self.set_focus(ExportFocus::Format(index));
                self.toggle_format(index);
            }
        }
    }

    fn handle_report_click(&mut self, content: Rect, x: f32, y: f32) {
        for index in 0..3 {
            if format_card(content, index).contains(x, y) {
                self.set_focus(ExportFocus::Format(index));
                self.toggle_format(index);
            }
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
        let nav = Self::nav_rect(card);
        let content = Self::content_rect(card);
        let language_panel = Self::language_rect(card);

        panel(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.80],
            0.0,
            [0.0; 4],
        );
        panel(
            quads,
            card,
            [0.085, 0.09, 0.115, 1.0],
            16.0,
            [0.28, 0.32, 0.41, 1.0],
        );
        labels.push(text(
            t("export_modal.title"),
            Rect {
                x: card.x + 22.0,
                y: card.y + 13.0,
                width: card.width - 44.0,
                height: 32.0,
            },
            26.0,
            HAlign::Left,
            Some([240, 242, 249]),
        ));
        labels.push(text(
            t("export_hub.subtitle"),
            Rect {
                x: card.x + 22.0,
                y: card.y + 42.0,
                width: card.width - 44.0,
                height: 18.0,
            },
            12.0,
            HAlign::Left,
            Some([141, 148, 168]),
        ));
        panel(
            quads,
            nav,
            [0.06, 0.065, 0.085, 1.0],
            10.0,
            [0.17, 0.19, 0.25, 1.0],
        );
        panel(
            quads,
            content,
            [0.065, 0.07, 0.09, 1.0],
            10.0,
            [0.17, 0.19, 0.25, 1.0],
        );
        panel(
            quads,
            language_panel,
            [0.06, 0.065, 0.085, 1.0],
            10.0,
            [0.17, 0.19, 0.25, 1.0],
        );

        let pages = [
            (ExportPage::Video, t("export_hub.video")),
            (ExportPage::Subtitles, t("export_hub.subtitles")),
            (ExportPage::Audio, t("export_hub.audio")),
            (ExportPage::Reports, t("export_hub.reports")),
        ];
        for (index, (page, name)) in pages.iter().enumerate() {
            let rect = Self::nav_item(card, index);
            if *page == self.page {
                panel(
                    quads,
                    rect,
                    [0.14, 0.24, 0.43, 1.0],
                    8.0,
                    [0.28, 0.50, 0.92, 1.0],
                );
            }
            if self.current_focus() == ExportFocus::Page(index) {
                panel(
                    quads,
                    rect,
                    [0.0, 0.0, 0.0, 0.0],
                    8.0,
                    [0.38, 0.65, 1.0, 1.0],
                );
            }
            labels.push(text(name, rect, 14.0, HAlign::Left, None));
        }

        match self.page {
            ExportPage::Video => self.render_video(quads, labels, content),
            ExportPage::Subtitles => self.render_formats(
                quads,
                labels,
                content,
                &[
                    (
                        "JSON",
                        t("export_hub.json_hint"),
                        self.configuration.subtitle_formats.json,
                    ),
                    (
                        "SRT",
                        t("export_hub.srt_hint"),
                        self.configuration.subtitle_formats.srt,
                    ),
                    (
                        "ASS",
                        t("export_hub.ass_hint"),
                        self.configuration.subtitle_formats.ass,
                    ),
                    (
                        "DETX",
                        t("export_hub.detx_hint"),
                        self.configuration.subtitle_formats.detx,
                    ),
                ],
            ),
            ExportPage::Audio => self.render_formats(
                quads,
                labels,
                content,
                &[
                    (
                        "MP3",
                        t("export_hub.mp3_hint"),
                        self.configuration.audio_formats.mp3,
                    ),
                    (
                        "WAV",
                        t("export_hub.wav_hint"),
                        self.configuration.audio_formats.wav,
                    ),
                    (
                        "BWF stems",
                        t("export_hub.bwf_hint"),
                        self.configuration.audio_formats.bwf_stems,
                    ),
                ],
            ),
            ExportPage::Reports => self.render_formats(
                quads,
                labels,
                content,
                &[
                    (
                        "CSV",
                        t("export_hub.csv_hint"),
                        self.configuration.cross_reference_formats.csv,
                    ),
                    (
                        "PDF",
                        t("export_hub.cross_pdf_hint"),
                        self.configuration.cross_reference_formats.pdf,
                    ),
                    (
                        t("export_hub.grid_pdf"),
                        t("export_hub.grid_pdf_hint"),
                        self.configuration.presence_grid_pdf,
                    ),
                ],
            ),
        }
        self.render_languages(quads, labels, card);

        let close = Self::close_button(card);
        button(
            quads,
            labels,
            close,
            t("export_hub.close"),
            [0.12, 0.13, 0.17, 1.0],
            true,
        );
        let export = Self::export_button(card);
        button(
            quads,
            labels,
            export,
            t("export_modal.export"),
            [0.13, 0.42, 0.28, 1.0],
            self.any_format_selected() && !self.configuration.selected_language_ids.is_empty(),
        );
        if let Some(rect) = self.focus_rect(card) {
            focus_outline(quads, rect);
        }
    }

    fn focus_rect(&self, card: Rect) -> Option<Rect> {
        let content = Self::content_rect(card);
        let rect = match self.current_focus() {
            ExportFocus::Page(_) => return None,
            ExportFocus::VideoToggle => option_rect(content, 0),
            ExportFocus::VideoAspect(index) => segment_rect(content, 112.0, 3, index),
            ExportFocus::VideoQuality(index) => segment_rect(content, 180.0, 5, index),
            ExportFocus::VideoWidth => dimension_rects(content).0,
            ExportFocus::VideoHeight => dimension_rects(content).1,
            ExportFocus::VideoFps => stepper_focus_rect(content, 276.0),
            ExportFocus::VideoBrScale => stepper_focus_rect(content, 326.0),
            ExportFocus::VideoKaraokeScale => stepper_focus_rect(content, 376.0),
            ExportFocus::VideoPreroll => stepper_focus_rect(content, 426.0),
            ExportFocus::VideoCountdown => Rect {
                x: content.x,
                y: content.y + 466.0,
                width: content.width,
                height: 34.0,
            },
            ExportFocus::VideoCountdownStart => stepper_focus_rect(content, 506.0),
            ExportFocus::Format(index) => format_card(content, index),
            ExportFocus::Language(index)
            | ExportFocus::LanguageOriginal(index)
            | ExportFocus::LanguageInstrumental(index) => {
                let viewport = Self::language_list_viewport(card);
                let y = viewport.y + index as f32 * ROW_H - self.language_scroll;
                let row = Rect {
                    x: viewport.x,
                    y: y + 3.0,
                    width: viewport.width,
                    height: ROW_H - 6.0,
                };
                match self.current_focus() {
                    ExportFocus::LanguageOriginal(_) => Rect {
                        x: row.x + row.width - 66.0,
                        y: row.y + 8.0,
                        width: 27.0,
                        height: 28.0,
                    },
                    ExportFocus::LanguageInstrumental(_) => Rect {
                        x: row.x + row.width - 35.0,
                        y: row.y + 8.0,
                        width: 27.0,
                        height: 28.0,
                    },
                    _ => row,
                }
            }
            ExportFocus::Close => Self::close_button(card),
            ExportFocus::Export => Self::export_button(card),
        };
        Some(rect)
    }

    fn render_video<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        content: Rect,
    ) {
        checkbox_row(
            quads,
            labels,
            option_rect(content, 0),
            self.configuration.video_enabled,
            t("export_hub.video_mp4"),
            true,
        );
        section_label(labels, content, 84.0, t("export_hub.aspect"));
        let aspects = [
            (VideoExportAspect::Source, t("export_hub.source_aspect")),
            (VideoExportAspect::Landscape16x9, "16:9"),
            (VideoExportAspect::Portrait9x16, "9:16"),
        ];
        for (index, (value, label)) in aspects.iter().enumerate() {
            segment(
                quads,
                labels,
                segment_rect(content, 112.0, 3, index),
                label,
                *value == self.configuration.video_aspect,
            );
        }
        section_label(labels, content, 152.0, t("export_hub.quality"));
        let qualities = [
            (VideoExportQuality::P720, "720p"),
            (VideoExportQuality::P1080, "1080p"),
            (VideoExportQuality::P1440, "1440p"),
            (VideoExportQuality::P8k, "8K"),
            (VideoExportQuality::Custom, t("export_hub.custom")),
        ];
        for (index, (value, label)) in qualities.iter().enumerate() {
            segment(
                quads,
                labels,
                segment_rect(content, 180.0, 5, index),
                label,
                *value == self.configuration.video_quality,
            );
        }
        let dimension_y = 226.0;
        if self.configuration.video_quality == VideoExportQuality::Custom {
            let (width, height) = dimension_rects(content);
            field(
                quads,
                labels,
                width,
                if self.numeric_field == Some(NumericField::Width) {
                    &self.numeric_text
                } else {
                    &self.width_display
                },
                self.numeric_field == Some(NumericField::Width),
            );
            labels.push(text(
                "×",
                Rect {
                    x: width.x + width.width,
                    y: width.y,
                    width: 24.0,
                    height: width.height,
                },
                13.0,
                HAlign::Center,
                Some([135, 141, 159]),
            ));
            field(
                quads,
                labels,
                height,
                if self.numeric_field == Some(NumericField::Height) {
                    &self.numeric_text
                } else {
                    &self.height_display
                },
                self.numeric_field == Some(NumericField::Height),
            );
        } else {
            labels.push(text(
                &self.resolution_display,
                Rect {
                    x: content.x + 16.0,
                    y: content.y + dimension_y,
                    width: content.width - 32.0,
                    height: 34.0,
                },
                13.0,
                HAlign::Left,
                Some([132, 181, 255]),
            ));
        }
        stepper(
            quads,
            labels,
            content,
            276.0,
            t("export_modal.fps"),
            &self.fps_display,
        );
        stepper(
            quads,
            labels,
            content,
            326.0,
            t("export_modal.br_scale"),
            &self.scale_display,
        );
        stepper(
            quads,
            labels,
            content,
            376.0,
            t("export_modal.karaoke_text_scale"),
            &self.karaoke_scale_display,
        );
        stepper(
            quads,
            labels,
            content,
            426.0,
            t("export_hub.preroll"),
            &self.preroll_display,
        );
        checkbox_row(
            quads,
            labels,
            Rect {
                x: content.x + 16.0,
                y: content.y + 466.0,
                width: content.width - 32.0,
                height: 34.0,
            },
            self.configuration.countdown_enabled,
            t("export_hub.countdown"),
            true,
        );
        if self.configuration.countdown_enabled {
            stepper(
                quads,
                labels,
                content,
                506.0,
                t("export_hub.countdown_from"),
                &self.countdown_display,
            );
        }
    }

    fn render_formats<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        content: Rect,
        formats: &[(&'a str, &'a str, bool)],
    ) {
        labels.push(text(
            t("export_hub.multiple_formats"),
            Rect {
                x: content.x + 18.0,
                y: content.y + 12.0,
                width: content.width - 36.0,
                height: 28.0,
            },
            18.0,
            HAlign::Left,
            None,
        ));
        for (index, (name, hint, checked)) in formats.iter().enumerate() {
            let rect = format_card(content, index);
            panel(
                quads,
                rect,
                if *checked {
                    [0.12, 0.20, 0.34, 1.0]
                } else {
                    [0.085, 0.09, 0.115, 1.0]
                },
                9.0,
                if *checked {
                    [0.25, 0.48, 0.88, 1.0]
                } else {
                    [0.18, 0.20, 0.26, 1.0]
                },
            );
            checkbox(
                quads,
                labels,
                Rect {
                    x: rect.x + 12.0,
                    y: rect.y + 15.0,
                    width: 20.0,
                    height: 20.0,
                },
                *checked,
                true,
            );
            labels.push(text(
                name,
                Rect {
                    x: rect.x + 44.0,
                    y: rect.y + 6.0,
                    width: rect.width - 56.0,
                    height: 24.0,
                },
                16.0,
                HAlign::Left,
                None,
            ));
            labels.push(text(
                hint,
                Rect {
                    x: rect.x + 44.0,
                    y: rect.y + 29.0,
                    width: rect.width - 56.0,
                    height: 24.0,
                },
                12.0,
                HAlign::Left,
                Some([139, 145, 163]),
            ));
        }
    }

    fn render_languages<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        card: Rect,
    ) {
        let panel_rect = Self::language_rect(card);
        let viewport = Self::language_list_viewport(card);
        labels.push(text(
            t("export_hub.languages"),
            Rect {
                x: panel_rect.x + 12.0,
                y: panel_rect.y + 8.0,
                width: panel_rect.width - 24.0,
                height: 24.0,
            },
            15.0,
            HAlign::Left,
            None,
        ));
        labels.push(text(
            t("export_hub.audio_short_hint"),
            Rect {
                x: panel_rect.x + panel_rect.width - 80.0,
                y: panel_rect.y + 9.0,
                width: 66.0,
                height: 22.0,
            },
            11.0,
            HAlign::Right,
            Some([126, 133, 152]),
        ));
        let first = (self.language_scroll / ROW_H).floor().max(0.0) as usize;
        let count = (viewport.height / ROW_H).ceil() as usize + 2;
        for (index, language) in self.languages.iter().enumerate().skip(first).take(count) {
            let y = viewport.y + index as f32 * ROW_H - self.language_scroll;
            let row = Rect {
                x: viewport.x,
                y: y + 3.0,
                width: viewport.width,
                height: ROW_H - 6.0,
            };
            if row.y + row.height < viewport.y || row.y > viewport.y + viewport.height {
                continue;
            }
            let selected = self.selected(language.id);
            panel(
                quads,
                row,
                if selected {
                    [0.105, 0.15, 0.25, 1.0]
                } else {
                    [0.075, 0.08, 0.10, 1.0]
                },
                7.0,
                if selected {
                    [0.22, 0.39, 0.70, 1.0]
                } else {
                    [0.0; 4]
                },
            );
            checkbox(
                quads,
                labels,
                Rect {
                    x: row.x + 8.0,
                    y: row.y + 13.0,
                    width: 18.0,
                    height: 18.0,
                },
                selected,
                true,
            );
            labels.push(text(
                &language.name,
                Rect {
                    x: row.x + 34.0,
                    y: row.y,
                    width: row.width - 108.0,
                    height: row.height,
                },
                13.0,
                HAlign::Left,
                None,
            ));
            let selection = self
                .configuration
                .audio_by_language
                .get(&language.id)
                .copied()
                .unwrap_or_else(AudioSelection::default);
            mini_toggle(
                quads,
                labels,
                Rect {
                    x: row.x + row.width - 66.0,
                    y: row.y + 11.0,
                    width: 27.0,
                    height: 24.0,
                },
                "O",
                selection.original,
                true,
            );
            mini_toggle(
                quads,
                labels,
                Rect {
                    x: row.x + row.width - 35.0,
                    y: row.y + 11.0,
                    width: 27.0,
                    height: 24.0,
                },
                "I",
                selection.instrumental,
                language.has_instrumental,
            );
        }
        let max = self.max_language_scroll(viewport);
        if max > 0.0 {
            let thumb_h = (viewport.height / (self.languages.len() as f32 * ROW_H)
                * viewport.height)
                .clamp(24.0, viewport.height);
            let thumb_y = viewport.y + self.language_scroll / max * (viewport.height - thumb_h);
            panel(
                quads,
                Rect {
                    x: viewport.x + viewport.width - 3.0,
                    y: thumb_y,
                    width: 3.0,
                    height: thumb_h,
                },
                [0.33, 0.40, 0.55, 1.0],
                2.0,
                [0.0; 4],
            );
        }
    }
}

pub fn resolve_video_dimensions(
    configuration: &ExportConfiguration,
    source_width: u32,
    source_height: u32,
) -> (u32, u32) {
    crate::configured_export::resolve_video_dimensions(configuration, source_width, source_height)
}
fn option_rect(content: Rect, index: usize) -> Rect {
    Rect {
        x: content.x + 16.0,
        y: content.y + 12.0 + index as f32 * 40.0,
        width: content.width - 32.0,
        height: 34.0,
    }
}
fn segment_rect(content: Rect, y: f32, count: usize, index: usize) -> Rect {
    let width = (content.width - 32.0) / count as f32;
    Rect {
        x: content.x + 16.0 + index as f32 * width,
        y: content.y + y,
        width: width - 4.0,
        height: 32.0,
    }
}
fn dimension_rects(content: Rect) -> (Rect, Rect) {
    let width = (content.width - 58.0) / 2.0;
    (
        Rect {
            x: content.x + 16.0,
            y: content.y + 226.0,
            width,
            height: 34.0,
        },
        Rect {
            x: content.x + 42.0 + width,
            y: content.y + 226.0,
            width,
            height: 34.0,
        },
    )
}
fn stepper_minus(content: Rect, y: f32) -> Rect {
    Rect {
        x: content.x + content.width - 130.0,
        y: content.y + y,
        width: 32.0,
        height: 32.0,
    }
}
fn stepper_plus(content: Rect, y: f32) -> Rect {
    Rect {
        x: content.x + content.width - 34.0,
        y: content.y + y,
        width: 32.0,
        height: 32.0,
    }
}
fn stepper_focus_rect(content: Rect, y: f32) -> Rect {
    Rect {
        x: content.x + 8.0,
        y: content.y + y - 4.0,
        width: content.width - 16.0,
        height: 40.0,
    }
}
fn format_card(content: Rect, index: usize) -> Rect {
    Rect {
        x: content.x + 16.0,
        y: content.y + 54.0 + index as f32 * 72.0,
        width: content.width - 32.0,
        height: 60.0,
    }
}

fn panel(
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
fn focus_outline(quads: &mut Vec<QuadInstance>, rect: Rect) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.0, 0.0, 0.0, 0.0],
        color_bottom: [0.0, 0.0, 0.0, 0.0],
        border_color: [0.38, 0.65, 1.0, 1.0],
        border_width: 2.0,
        border_radius: 8.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}
fn text<'a>(
    value: &'a str,
    bounds: Rect,
    size: f32,
    align: HAlign,
    color: Option<[u8; 3]>,
) -> LabelInfo<'a> {
    LabelInfo {
        text: value,
        bounds,
        h_align: align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: if align == HAlign::Left { 8.0 } else { 0.0 },
        font_size_override: Some(size),
        color_override: color,
        font_family_override: None,
    }
}
fn button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    value: &'a str,
    color: [f32; 4],
    enabled: bool,
) {
    panel(
        quads,
        rect,
        if enabled {
            color
        } else {
            [0.10, 0.105, 0.125, 1.0]
        },
        8.0,
        if enabled {
            [0.30, 0.37, 0.46, 0.8]
        } else {
            [0.0; 4]
        },
    );
    labels.push(text(
        value,
        rect,
        14.0,
        HAlign::Center,
        if enabled { None } else { Some([94, 99, 113]) },
    ));
}
fn checkbox(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'_>>,
    rect: Rect,
    checked: bool,
    enabled: bool,
) {
    panel(
        quads,
        rect,
        if checked && enabled {
            [0.20, 0.43, 0.82, 1.0]
        } else {
            [0.07, 0.075, 0.095, 1.0]
        },
        4.0,
        if enabled {
            [0.28, 0.34, 0.46, 1.0]
        } else {
            [0.16, 0.17, 0.21, 1.0]
        },
    );
    if checked && enabled {
        labels.push(text("✓", rect, 14.0, HAlign::Center, None));
    }
}
fn checkbox_row<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    checked: bool,
    value: &'a str,
    enabled: bool,
) {
    checkbox(
        quads,
        labels,
        Rect {
            x: rect.x,
            y: rect.y + 7.0,
            width: 20.0,
            height: 20.0,
        },
        checked,
        enabled,
    );
    labels.push(text(
        value,
        Rect {
            x: rect.x + 30.0,
            y: rect.y,
            width: rect.width - 30.0,
            height: rect.height,
        },
        14.0,
        HAlign::Left,
        if enabled { None } else { Some([96, 101, 116]) },
    ));
}
fn mini_toggle<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    value: &'a str,
    checked: bool,
    enabled: bool,
) {
    panel(
        quads,
        rect,
        if checked && enabled {
            [0.20, 0.39, 0.69, 1.0]
        } else {
            [0.08, 0.085, 0.105, 1.0]
        },
        5.0,
        if enabled {
            [0.22, 0.28, 0.39, 1.0]
        } else {
            [0.0; 4]
        },
    );
    labels.push(text(
        value,
        rect,
        11.0,
        HAlign::Center,
        if enabled { None } else { Some([74, 78, 89]) },
    ));
}
fn section_label<'a>(labels: &mut Vec<LabelInfo<'a>>, content: Rect, y: f32, value: &'a str) {
    labels.push(text(
        value,
        Rect {
            x: content.x + 16.0,
            y: content.y + y,
            width: content.width - 32.0,
            height: 22.0,
        },
        12.0,
        HAlign::Left,
        Some([139, 145, 164]),
    ));
}
fn segment<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    value: &'a str,
    selected: bool,
) {
    panel(
        quads,
        rect,
        if selected {
            [0.16, 0.31, 0.56, 1.0]
        } else {
            [0.085, 0.09, 0.115, 1.0]
        },
        6.0,
        if selected {
            [0.30, 0.54, 0.96, 1.0]
        } else {
            [0.18, 0.20, 0.26, 1.0]
        },
    );
    labels.push(text(value, rect, 12.0, HAlign::Center, None));
}
fn field<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    value: &'a str,
    active: bool,
) {
    panel(
        quads,
        rect,
        [0.055, 0.06, 0.08, 1.0],
        6.0,
        if active {
            [0.30, 0.55, 1.0, 1.0]
        } else {
            [0.19, 0.22, 0.29, 1.0]
        },
    );
    labels.push(text(value, rect, 13.0, HAlign::Center, None));
}
fn stepper<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    content: Rect,
    y: f32,
    name: &'a str,
    value: &'a str,
) {
    labels.push(text(
        name,
        Rect {
            x: content.x + 16.0,
            y: content.y + y,
            width: content.width - 156.0,
            height: 32.0,
        },
        13.0,
        HAlign::Left,
        None,
    ));
    let minus = stepper_minus(content, y);
    let plus = stepper_plus(content, y);
    panel(
        quads,
        minus,
        [0.10, 0.11, 0.14, 1.0],
        6.0,
        [0.20, 0.23, 0.30, 1.0],
    );
    panel(
        quads,
        Rect {
            x: minus.x + minus.width + 2.0,
            y: minus.y,
            width: 60.0,
            height: 32.0,
        },
        [0.055, 0.06, 0.08, 1.0],
        4.0,
        [0.16, 0.18, 0.23, 1.0],
    );
    panel(
        quads,
        plus,
        [0.10, 0.11, 0.14, 1.0],
        6.0,
        [0.20, 0.23, 0.30, 1.0],
    );
    labels.push(text("−", minus, 17.0, HAlign::Center, None));
    labels.push(text(
        value,
        Rect {
            x: minus.x + minus.width + 2.0,
            y: minus.y,
            width: 60.0,
            height: 32.0,
        },
        13.0,
        HAlign::Center,
        None,
    ));
    labels.push(text("+", plus, 17.0, HAlign::Center, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_follow_aspect_and_quality() {
        let mut config = ExportConfiguration {
            video_quality: VideoExportQuality::P720,
            video_aspect: VideoExportAspect::Landscape16x9,
            ..ExportConfiguration::default()
        };
        assert_eq!(resolve_video_dimensions(&config, 1920, 1080), (1280, 720));
        config.video_aspect = VideoExportAspect::Portrait9x16;
        assert_eq!(resolve_video_dimensions(&config, 1920, 1080), (720, 1280));
    }

    #[test]
    fn source_aspect_is_preserved() {
        let config = ExportConfiguration {
            video_quality: VideoExportQuality::P1080,
            ..ExportConfiguration::default()
        };
        assert_eq!(resolve_video_dimensions(&config, 2048, 858), (2578, 1080));
    }
}
