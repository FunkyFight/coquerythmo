//! « Ajouter et conformer » : pick the project video, the band and the new
//! cut on the disk, follow the analysis, then read what moved.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::conform::{ConformReport, SpanStatus};
use crate::i18n::t;
use crate::project::MediaId;

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};

const CARD_W: f32 = 860.0;
const CARD_H: f32 = 660.0;
const MARGIN: f32 = 28.0;
/// Height of one row of the video and band lists.
const ROW_H: f32 = 36.0;
const LIST_ROWS: usize = 5;
/// Height of one row of the report.
const REPORT_ROW_H: f32 = 28.0;

const FONT_TITLE: f32 = 22.0;
const FONT_SECTION: f32 = 16.0;
const FONT_BODY: f32 = 15.0;
const FONT_HINT: f32 = 14.0;
const FONT_BUTTON: f32 = 16.0;
const FONT_COUNT: f32 = 24.0;

const TEXT: Option<[u8; 3]> = None;
const TEXT_SOFT: Option<[u8; 3]> = Some([205, 205, 220]);
const TEXT_HINT: Option<[u8; 3]> = Some([170, 170, 188]);

const COLOR_UNCHANGED: [u8; 3] = [170, 220, 170];
const COLOR_MOVED: [u8; 3] = [160, 200, 255];
const COLOR_MODIFIED: [u8; 3] = [255, 210, 120];
const COLOR_TRUNCATED: [u8; 3] = [255, 180, 120];
const COLOR_DELETED: [u8; 3] = [255, 140, 140];
const COLOR_RERECORD: [u8; 3] = [255, 130, 180];

#[derive(Clone, Debug, PartialEq)]
pub struct ConformChoice {
    pub id: u64,
    pub name: String,
}

pub enum ConformStage {
    Setup,
    Running {
        progress: Arc<AtomicU32>,
    },
    Report(Box<ReportView>),
    Failed(String),
}

/// A report with its rows already rendered as text, so drawing borrows them.
pub struct ReportView {
    pub report: ConformReport,
    /// One line per notable entry, with its colour and re-record flag.
    pub rows: Vec<(String, [u8; 3], bool)>,
    pub counts: [String; 6],
    pub scroll_text: String,
    pub header: String,
}

impl ReportView {
    fn new(report: ConformReport) -> Self {
        let rows = report
            .notable()
            .map(|entry| {
                (
                    entry.line(report.fps, report.new_fps),
                    status_color(entry.status),
                    entry.needs_rerecord(),
                )
            })
            .collect();
        let counts = [
            report.count(SpanStatus::Moved).to_string(),
            report.count(SpanStatus::Modified).to_string(),
            report.count(SpanStatus::Truncated).to_string(),
            report.count(SpanStatus::Deleted).to_string(),
            report.rerecord_count().to_string(),
            report.count(SpanStatus::Unchanged).to_string(),
        ];
        let header = format!(
            "{} → {}   ·   {}   ·   {}: {:.0} %",
            report.old_video,
            report.new_video,
            report.band,
            t("conform.report.coverage"),
            report.coverage * 100.0
        );
        Self {
            report,
            rows,
            counts,
            scroll_text: String::new(),
            header,
        }
    }
}

fn status_color(status: SpanStatus) -> [u8; 3] {
    match status {
        SpanStatus::Unchanged => COLOR_UNCHANGED,
        SpanStatus::Moved => COLOR_MOVED,
        SpanStatus::Modified => COLOR_MODIFIED,
        SpanStatus::Truncated => COLOR_TRUNCATED,
        SpanStatus::Deleted => COLOR_DELETED,
    }
}

pub struct ConformModal {
    videos: Vec<ConformChoice>,
    bands: Vec<ConformChoice>,
    selected_video: usize,
    selected_band: usize,
    /// Whether the takes of the Recording tab follow the pictures too.
    move_takes: bool,
    stage: ConformStage,
    /// 0 video list, 1 band list, 2 takes checkbox, 3 pick button, 4 close.
    keyboard_focus: usize,
    report_scroll: usize,
    video_scroll: usize,
    band_scroll: usize,
    progress_text: String,
}

pub enum ConformModalResult {
    Consumed,
    Close,
    /// Cancel the running analysis.
    Cancel,
    /// Open the file picker for the new cut.
    PickNewVideo {
        video_id: MediaId,
        band_id: u64,
        move_takes: bool,
    },
}

impl ConformModal {
    pub fn new(
        videos: Vec<ConformChoice>,
        bands: Vec<ConformChoice>,
        selected_video: Option<MediaId>,
        selected_band: Option<u64>,
        move_takes: bool,
    ) -> Self {
        let selected_video = selected_video
            .and_then(|id| videos.iter().position(|video| video.id == id))
            .unwrap_or(0);
        let selected_band = selected_band
            .and_then(|id| bands.iter().position(|band| band.id == id))
            .unwrap_or(0);
        let mut modal = Self {
            videos,
            bands,
            selected_video,
            selected_band,
            move_takes,
            stage: ConformStage::Setup,
            keyboard_focus: 0,
            report_scroll: 0,
            video_scroll: 0,
            band_scroll: 0,
            progress_text: String::new(),
        };
        modal.keep_visible();
        modal
    }

    pub fn stage(&self) -> &ConformStage {
        &self.stage
    }

    pub fn is_running(&self) -> bool {
        matches!(self.stage, ConformStage::Running { .. })
    }

    pub fn selected_video(&self) -> Option<MediaId> {
        self.videos.get(self.selected_video).map(|video| video.id)
    }

    pub fn selected_band(&self) -> Option<u64> {
        self.bands.get(self.selected_band).map(|band| band.id)
    }

    pub fn move_takes(&self) -> bool {
        self.move_takes
    }

    pub fn select_video(&mut self, id: MediaId) -> bool {
        match self.videos.iter().position(|video| video.id == id) {
            Some(index) => {
                self.selected_video = index;
                self.keep_visible();
                true
            }
            None => false,
        }
    }

    pub fn select_band(&mut self, id: u64) -> bool {
        match self.bands.iter().position(|band| band.id == id) {
            Some(index) => {
                self.selected_band = index;
                self.keep_visible();
                true
            }
            None => false,
        }
    }

    pub fn set_move_takes(&mut self, move_takes: bool) {
        self.move_takes = move_takes;
    }

    pub fn start(&mut self, progress: Arc<AtomicU32>) {
        self.stage = ConformStage::Running { progress };
        self.keyboard_focus = 4;
    }

    pub fn show_report(&mut self, report: ConformReport) {
        self.stage = ConformStage::Report(Box::new(ReportView::new(report)));
        self.report_scroll = 0;
        self.keyboard_focus = 4;
        self.update_scroll_text();
    }

    pub fn report(&self) -> Option<&ConformReport> {
        match &self.stage {
            ConformStage::Report(view) => Some(&view.report),
            _ => None,
        }
    }

    pub fn fail(&mut self, error: String) {
        self.stage = ConformStage::Failed(error);
        self.keyboard_focus = 4;
    }

    pub fn back_to_setup(&mut self) {
        self.stage = ConformStage::Setup;
        self.keyboard_focus = 3;
    }

    /// Refreshes the percentage shown while the analysis runs.
    pub fn refresh_progress(&mut self) -> bool {
        let ConformStage::Running { progress } = &self.stage else {
            return false;
        };
        let value = f32::from_bits(progress.load(Ordering::Relaxed)).clamp(0.0, 1.0);
        let text = format!("{} {:.0} %", t("conform.running"), value * 100.0);
        if text != self.progress_text {
            self.progress_text = text;
            true
        } else {
            false
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match &self.stage {
            ConformStage::Setup => match self.keyboard_focus {
                0 => format!(
                    "{}: {}",
                    t("conform.project_video"),
                    self.videos
                        .get(self.selected_video)
                        .map(|video| video.name.as_str())
                        .unwrap_or("—")
                ),
                1 => format!(
                    "{}: {}",
                    t("conform.band"),
                    self.bands
                        .get(self.selected_band)
                        .map(|band| band.name.as_str())
                        .unwrap_or("—")
                ),
                2 => format!(
                    "{}: {}",
                    t("conform.move_takes"),
                    t(if self.move_takes {
                        "accessibility.on"
                    } else {
                        "accessibility.off"
                    })
                ),
                3 => t("conform.pick_new_video").to_string(),
                _ => t("conform.cancel").to_string(),
            },
            ConformStage::Running { .. } => self.progress_text.clone(),
            // The summary, then the rows on screen: arrows read the next ones.
            ConformStage::Report(view) => {
                let rows = Self::report_rows(Self::default_card());
                let mut text = format!(
                    "{}. {}: {} · {}: {} · {}: {} · {}: {} · {}: {} · {}: {}.",
                    view.header,
                    t("conform.status.moved"),
                    view.counts[0],
                    t("conform.status.modified"),
                    view.counts[1],
                    t("conform.status.truncated"),
                    view.counts[2],
                    t("conform.status.deleted"),
                    view.counts[3],
                    t("conform.status.rerecord"),
                    view.counts[4],
                    t("conform.status.unchanged"),
                    view.counts[5]
                );
                if view.rows.is_empty() {
                    text.push(' ');
                    text.push_str(t("conform.report.nothing"));
                }
                for (row, _, _) in view.rows.iter().skip(self.report_scroll).take(rows) {
                    text.push('\n');
                    text.push_str(row);
                }
                if !view.scroll_text.is_empty() {
                    text.push('\n');
                    text.push_str(&view.scroll_text);
                }
                text
            }
            ConformStage::Failed(error) => error.clone(),
        }
    }

    // ---- Geometry ----------------------------------------------------------

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        let width = CARD_W.min(screen_w - 40.0).max(320.0);
        let height = CARD_H.min(screen_h - 40.0).max(320.0);
        Rect {
            x: ((screen_w - width) / 2.0).round(),
            y: ((screen_h - height) / 2.0).round(),
            width,
            height,
        }
    }

    fn inner(card: Rect) -> Rect {
        Rect {
            x: card.x + MARGIN,
            y: card.y + MARGIN,
            width: card.width - 2.0 * MARGIN,
            height: card.height - 2.0 * MARGIN,
        }
    }

    fn title_rect(card: Rect) -> Rect {
        let inner = Self::inner(card);
        Rect {
            height: 32.0,
            ..inner
        }
    }

    fn description_rect(card: Rect) -> Rect {
        let title = Self::title_rect(card);
        Rect {
            x: title.x,
            y: title.y + title.height + 8.0,
            width: title.width,
            height: 64.0,
        }
    }

    fn video_list_rect(card: Rect) -> Rect {
        let description = Self::description_rect(card);
        let inner = Self::inner(card);
        Rect {
            x: inner.x,
            y: description.y + description.height + 40.0,
            width: (inner.width - MARGIN) / 2.0,
            height: ROW_H * LIST_ROWS as f32 + 4.0,
        }
    }

    fn band_list_rect(card: Rect) -> Rect {
        let videos = Self::video_list_rect(card);
        Rect {
            x: videos.x + videos.width + MARGIN,
            ..videos
        }
    }

    fn takes_rect(card: Rect) -> Rect {
        let videos = Self::video_list_rect(card);
        let inner = Self::inner(card);
        Rect {
            x: inner.x,
            y: videos.y + videos.height + 24.0,
            width: inner.width,
            height: 32.0,
        }
    }

    fn pick_rect(card: Rect) -> Rect {
        let takes = Self::takes_rect(card);
        let width = 460.0_f32.min(card.width - 2.0 * MARGIN);
        Rect {
            x: card.x + ((card.width - width) / 2.0).round(),
            y: takes.y + takes.height + 28.0,
            width,
            height: 50.0,
        }
    }

    fn hint_rect(card: Rect) -> Rect {
        let pick = Self::pick_rect(card);
        let inner = Self::inner(card);
        Rect {
            x: inner.x,
            y: pick.y + pick.height + 16.0,
            width: inner.width,
            height: 66.0,
        }
    }

    fn close_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + ((card.width - 220.0) / 2.0).round(),
            y: card.y + card.height - MARGIN - 46.0,
            width: 220.0,
            height: 46.0,
        }
    }

    fn report_header_rect(card: Rect) -> Rect {
        let title = Self::title_rect(card);
        Rect {
            x: title.x,
            y: title.y + title.height + 6.0,
            width: title.width,
            height: 24.0,
        }
    }

    fn report_counts_rect(card: Rect) -> Rect {
        let header = Self::report_header_rect(card);
        Rect {
            x: header.x,
            y: header.y + header.height + 12.0,
            width: header.width,
            height: 2.0 * 34.0,
        }
    }

    fn report_hint_rect(card: Rect) -> Rect {
        let counts = Self::report_counts_rect(card);
        Rect {
            x: counts.x,
            y: counts.y + counts.height + 8.0,
            width: counts.width,
            height: 24.0,
        }
    }

    fn report_rect(card: Rect) -> Rect {
        let hint = Self::report_hint_rect(card);
        let close = Self::close_rect(card);
        let top = hint.y + hint.height + 8.0;
        Rect {
            x: hint.x,
            y: top,
            width: hint.width,
            height: (close.y - 24.0 - top).max(REPORT_ROW_H),
        }
    }

    fn report_rows(card: Rect) -> usize {
        ((Self::report_rect(card).height - 8.0) / REPORT_ROW_H).max(1.0) as usize
    }

    fn default_card() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    // ---- Behaviour ---------------------------------------------------------

    fn keep_visible(&mut self) {
        if self.selected_video < self.video_scroll {
            self.video_scroll = self.selected_video;
        } else if self.selected_video >= self.video_scroll + LIST_ROWS {
            self.video_scroll = self.selected_video + 1 - LIST_ROWS;
        }
        if self.selected_band < self.band_scroll {
            self.band_scroll = self.selected_band;
        } else if self.selected_band >= self.band_scroll + LIST_ROWS {
            self.band_scroll = self.selected_band + 1 - LIST_ROWS;
        }
    }

    fn update_scroll_text(&mut self) {
        let rows = Self::report_rows(Self::default_card());
        let scroll = self.report_scroll;
        if let ConformStage::Report(view) = &mut self.stage {
            view.scroll_text = if view.rows.len() > rows {
                format!(
                    "{}–{} / {}",
                    scroll + 1,
                    (scroll + rows).min(view.rows.len()),
                    view.rows.len()
                )
            } else {
                String::new()
            };
        }
    }

    fn move_focus(&mut self, delta: i32) {
        let count = 5;
        self.keyboard_focus = (self.keyboard_focus as i32 + delta).rem_euclid(count) as usize;
    }

    fn adjust_selection(&mut self, delta: i32) {
        match self.keyboard_focus {
            0 if !self.videos.is_empty() => {
                self.selected_video = (self.selected_video as i32 + delta)
                    .clamp(0, self.videos.len() as i32 - 1)
                    as usize;
            }
            1 if !self.bands.is_empty() => {
                self.selected_band = (self.selected_band as i32 + delta)
                    .clamp(0, self.bands.len() as i32 - 1)
                    as usize;
            }
            2 => self.move_takes = !self.move_takes,
            _ => {}
        }
        self.keep_visible();
    }

    fn pick_result(&self) -> ConformModalResult {
        match (self.selected_video(), self.selected_band()) {
            (Some(video_id), Some(band_id)) => ConformModalResult::PickNewVideo {
                video_id,
                band_id,
                move_takes: self.move_takes,
            },
            _ => ConformModalResult::Consumed,
        }
    }

    fn activate_focused(&mut self) -> ConformModalResult {
        match self.stage {
            ConformStage::Setup => match self.keyboard_focus {
                2 => {
                    self.move_takes = !self.move_takes;
                    ConformModalResult::Consumed
                }
                3 => self.pick_result(),
                4 => ConformModalResult::Close,
                _ => ConformModalResult::Consumed,
            },
            ConformStage::Running { .. } => ConformModalResult::Cancel,
            ConformStage::Report(_) | ConformStage::Failed(_) => ConformModalResult::Close,
        }
    }

    fn report_len(&self) -> usize {
        match &self.stage {
            ConformStage::Report(view) => view.rows.len(),
            _ => 0,
        }
    }

    fn scroll_report(&mut self, delta: i32) {
        let rows = Self::report_rows(Self::default_card());
        let max = self.report_len().saturating_sub(rows);
        self.report_scroll = (self.report_scroll as i32 + delta).clamp(0, max as i32) as usize;
        self.update_scroll_text();
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ConformModalResult {
        let card = Self::card_rect(screen_w, screen_h);
        let running = self.is_running();

        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return if running {
                        ConformModalResult::Consumed
                    } else {
                        ConformModalResult::Close
                    };
                }
            }
            _ => {}
        }

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return if running {
                        ConformModalResult::Cancel
                    } else {
                        ConformModalResult::Close
                    };
                }
                if text == "\t" || text == "\u{b}" {
                    if !running {
                        self.move_focus(if text == "\t" { 1 } else { -1 });
                    }
                    return ConformModalResult::Consumed;
                }
                if text == "\r" || text == "\n" || text == " " {
                    return self.activate_focused();
                }
                ConformModalResult::Consumed
            }
            UiEvent::FocusNext => {
                if !running {
                    self.move_focus(1);
                }
                ConformModalResult::Consumed
            }
            UiEvent::FocusPrevious => {
                if !running {
                    self.move_focus(-1);
                }
                ConformModalResult::Consumed
            }
            UiEvent::Activate => self.activate_focused(),
            UiEvent::CursorUp => {
                match self.stage {
                    ConformStage::Setup => self.adjust_selection(-1),
                    ConformStage::Report(_) => self.scroll_report(-1),
                    _ => {}
                }
                ConformModalResult::Consumed
            }
            UiEvent::CursorDown => {
                match self.stage {
                    ConformStage::Setup => self.adjust_selection(1),
                    ConformStage::Report(_) => self.scroll_report(1),
                    _ => {}
                }
                ConformModalResult::Consumed
            }
            UiEvent::PageUp => {
                self.scroll_report(-(Self::report_rows(card) as i32));
                ConformModalResult::Consumed
            }
            UiEvent::PageDown => {
                self.scroll_report(Self::report_rows(card) as i32);
                ConformModalResult::Consumed
            }
            UiEvent::CursorLeft | UiEvent::CursorRight => {
                if matches!(self.stage, ConformStage::Setup) {
                    self.move_focus(if matches!(event, UiEvent::CursorLeft) {
                        -1
                    } else {
                        1
                    });
                }
                ConformModalResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } => {
                if matches!(self.stage, ConformStage::Report(_))
                    && Self::report_rect(card).contains(*x, *y)
                {
                    self.scroll_report(if *delta > 0.0 { -3 } else { 3 });
                } else if matches!(self.stage, ConformStage::Setup) {
                    let step = if *delta > 0.0 { -1 } else { 1 };
                    if Self::video_list_rect(card).contains(*x, *y) {
                        let max = self.videos.len().saturating_sub(LIST_ROWS);
                        self.video_scroll =
                            (self.video_scroll as i32 + step).clamp(0, max as i32) as usize;
                    } else if Self::band_list_rect(card).contains(*x, *y) {
                        let max = self.bands.len().saturating_sub(LIST_ROWS);
                        self.band_scroll =
                            (self.band_scroll as i32 + step).clamp(0, max as i32) as usize;
                    }
                }
                ConformModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                match self.stage {
                    ConformStage::Setup => {
                        let videos = Self::video_list_rect(card);
                        if videos.contains(*x, *y) {
                            let row = ((*y - videos.y - 2.0) / ROW_H).max(0.0) as usize
                                + self.video_scroll;
                            if row < self.videos.len() {
                                self.selected_video = row;
                                self.keyboard_focus = 0;
                            }
                            return ConformModalResult::Consumed;
                        }
                        let bands = Self::band_list_rect(card);
                        if bands.contains(*x, *y) {
                            let row = ((*y - bands.y - 2.0) / ROW_H).max(0.0) as usize
                                + self.band_scroll;
                            if row < self.bands.len() {
                                self.selected_band = row;
                                self.keyboard_focus = 1;
                            }
                            return ConformModalResult::Consumed;
                        }
                        if Self::takes_rect(card).contains(*x, *y) {
                            self.keyboard_focus = 2;
                            self.move_takes = !self.move_takes;
                            return ConformModalResult::Consumed;
                        }
                        if Self::pick_rect(card).contains(*x, *y) {
                            self.keyboard_focus = 3;
                            return self.pick_result();
                        }
                        if Self::close_rect(card).contains(*x, *y) {
                            return ConformModalResult::Close;
                        }
                    }
                    ConformStage::Running { .. } => {
                        if Self::close_rect(card).contains(*x, *y) {
                            return ConformModalResult::Cancel;
                        }
                    }
                    ConformStage::Report(_) | ConformStage::Failed(_) => {
                        if Self::close_rect(card).contains(*x, *y) {
                            return ConformModalResult::Close;
                        }
                    }
                }
                ConformModalResult::Consumed
            }
            _ => ConformModalResult::Consumed,
        }
    }

    // ---- Drawing -----------------------------------------------------------

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card_rect(screen_w, screen_h);

        push_quad(
            overlay_quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.72],
            [0.0, 0.0, 0.0, 0.72],
            [0.0; 4],
            0.0,
            0.0,
        );
        overlay_quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.20, 0.20, 0.24, 1.0],
            color_bottom: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.45, 0.45, 0.52, 0.8],
            border_width: 1.5,
            border_radius: 16.0,
            shadow_offset: [0.0, 6.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 14.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        push_label(
            labels,
            t("conform.title"),
            Self::title_rect(card),
            HAlign::Left,
            FONT_TITLE,
            TEXT,
        );

        match &self.stage {
            ConformStage::Setup => self.render_setup(card, overlay_quads, labels),
            ConformStage::Running { progress } => {
                push_paragraph(
                    labels,
                    t("conform.running_description"),
                    Self::description_rect(card),
                    FONT_BODY,
                    TEXT_SOFT,
                );
                let inner = Self::inner(card);
                push_label(
                    labels,
                    &self.progress_text,
                    Rect {
                        x: inner.x,
                        y: card.y + card.height * 0.42,
                        width: inner.width,
                        height: 36.0,
                    },
                    HAlign::Center,
                    20.0,
                    TEXT,
                );
                let value = f32::from_bits(progress.load(Ordering::Relaxed)).clamp(0.0, 1.0);
                push_bar(
                    overlay_quads,
                    Rect {
                        x: inner.x + 40.0,
                        y: card.y + card.height * 0.42 + 52.0,
                        width: inner.width - 80.0,
                        height: 18.0,
                    },
                    value,
                );
                push_button(
                    overlay_quads,
                    labels,
                    Self::close_rect(card),
                    t("conform.cancel"),
                    false,
                    false,
                    true,
                );
            }
            ConformStage::Report(view) => {
                self.render_report(card, view, overlay_quads, labels);
            }
            ConformStage::Failed(error) => {
                let description = Self::description_rect(card);
                push_label(
                    labels,
                    t("conform.failed"),
                    Rect {
                        height: 28.0,
                        ..description
                    },
                    HAlign::Left,
                    FONT_SECTION,
                    Some(COLOR_TRUNCATED),
                );
                push_paragraph(
                    labels,
                    error,
                    Rect {
                        y: description.y + 36.0,
                        height: card.height * 0.5,
                        ..description
                    },
                    FONT_BODY,
                    TEXT_SOFT,
                );
                push_button(
                    overlay_quads,
                    labels,
                    Self::close_rect(card),
                    t("conform.close"),
                    false,
                    true,
                    true,
                );
            }
        }
    }

    fn render_setup<'a>(
        &'a self,
        card: Rect,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        push_paragraph(
            labels,
            t("conform.description"),
            Self::description_rect(card),
            FONT_BODY,
            TEXT_SOFT,
        );
        let videos = Self::video_list_rect(card);
        let bands = Self::band_list_rect(card);
        push_label(
            labels,
            t("conform.project_video"),
            Rect {
                x: videos.x,
                y: videos.y - 30.0,
                width: videos.width,
                height: 26.0,
            },
            HAlign::Left,
            FONT_SECTION,
            TEXT,
        );
        push_label(
            labels,
            t("conform.band"),
            Rect {
                x: bands.x,
                y: bands.y - 30.0,
                width: bands.width,
                height: 26.0,
            },
            HAlign::Left,
            FONT_SECTION,
            TEXT,
        );
        push_list(
            overlay_quads,
            labels,
            videos,
            &self.videos,
            self.selected_video,
            self.video_scroll,
            self.keyboard_focus == 0,
            t("conform.no_video"),
        );
        push_list(
            overlay_quads,
            labels,
            bands,
            &self.bands,
            self.selected_band,
            self.band_scroll,
            self.keyboard_focus == 1,
            t("conform.no_band"),
        );
        push_checkbox(
            overlay_quads,
            labels,
            Self::takes_rect(card),
            t("conform.move_takes"),
            self.move_takes,
            self.keyboard_focus == 2,
        );
        push_button(
            overlay_quads,
            labels,
            Self::pick_rect(card),
            t("conform.pick_new_video"),
            false,
            true,
            self.keyboard_focus == 3,
        );
        push_paragraph(
            labels,
            t("conform.pick_hint"),
            Self::hint_rect(card),
            FONT_HINT,
            TEXT_HINT,
        );
        push_button(
            overlay_quads,
            labels,
            Self::close_rect(card),
            t("conform.cancel"),
            false,
            false,
            self.keyboard_focus == 4,
        );
    }

    fn render_report<'a>(
        &'a self,
        card: Rect,
        view: &'a ReportView,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        labels.push(LabelInfo {
            text: &view.header,
            bounds: Self::report_header_rect(card),
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(FONT_BODY),
            color_override: TEXT_SOFT,
            font_family_override: None,
        });

        let counts = Self::report_counts_rect(card);
        let keys = [
            ("conform.status.moved", COLOR_MOVED),
            ("conform.status.modified", COLOR_MODIFIED),
            ("conform.status.truncated", COLOR_TRUNCATED),
            ("conform.status.deleted", COLOR_DELETED),
            ("conform.status.rerecord", COLOR_RERECORD),
            ("conform.status.unchanged", COLOR_UNCHANGED),
        ];
        let column = counts.width / 3.0;
        for (index, (key, color)) in keys.iter().enumerate() {
            let rect = Rect {
                x: counts.x + (index % 3) as f32 * column,
                y: counts.y + (index / 3) as f32 * 34.0,
                width: column - 12.0,
                height: 32.0,
            };
            push_label(
                labels,
                &view.counts[index],
                Rect {
                    width: 64.0,
                    ..rect
                },
                HAlign::Right,
                FONT_COUNT,
                Some(*color),
            );
            push_label(
                labels,
                t(key),
                Rect {
                    x: rect.x + 76.0,
                    width: rect.width - 76.0,
                    ..rect
                },
                HAlign::Left,
                FONT_BODY,
                Some(*color),
            );
        }
        push_label(
            labels,
            t("conform.report.hint"),
            Self::report_hint_rect(card),
            HAlign::Left,
            FONT_HINT,
            TEXT_HINT,
        );

        let list = Self::report_rect(card);
        push_quad(
            overlay_quads,
            list,
            [0.10, 0.10, 0.12, 1.0],
            [0.10, 0.10, 0.12, 1.0],
            [0.30, 0.30, 0.36, 0.5],
            1.0,
            8.0,
        );
        let rows = Self::report_rows(card);
        if view.rows.is_empty() {
            push_label(
                labels,
                t("conform.report.nothing"),
                list,
                HAlign::Center,
                FONT_SECTION,
                Some(COLOR_UNCHANGED),
            );
        }
        for (row, (text, color, rerecord)) in view
            .rows
            .iter()
            .skip(self.report_scroll)
            .take(rows)
            .enumerate()
        {
            let bounds = Rect {
                x: list.x + 12.0,
                y: list.y + 4.0 + row as f32 * REPORT_ROW_H,
                width: list.width - 24.0,
                height: REPORT_ROW_H,
            };
            if row % 2 == 1 {
                push_quad(
                    overlay_quads,
                    Rect {
                        x: list.x + 2.0,
                        width: list.width - 4.0,
                        ..bounds
                    },
                    [0.13, 0.13, 0.16, 1.0],
                    [0.13, 0.13, 0.16, 1.0],
                    [0.0; 4],
                    0.0,
                    4.0,
                );
            }
            labels.push(LabelInfo {
                text,
                bounds,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(FONT_BODY),
                color_override: Some(if *rerecord { COLOR_RERECORD } else { *color }),
                font_family_override: None,
            });
        }
        if !view.scroll_text.is_empty() {
            push_label(
                labels,
                &view.scroll_text,
                Rect {
                    x: list.x,
                    y: list.y + list.height + 2.0,
                    width: list.width,
                    height: 20.0,
                },
                HAlign::Right,
                FONT_HINT,
                TEXT_HINT,
            );
        }
        push_button(
            overlay_quads,
            labels,
            Self::close_rect(card),
            t("conform.close"),
            false,
            true,
            true,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    overlay_quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    color_bottom: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    border_radius: f32,
) {
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom,
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

#[allow(clippy::too_many_arguments)]
fn push_list<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    items: &'a [ConformChoice],
    selected: usize,
    scroll: usize,
    focused: bool,
    empty_text: &'a str,
) {
    push_quad(
        overlay_quads,
        rect,
        [0.10, 0.10, 0.12, 1.0],
        [0.10, 0.10, 0.12, 1.0],
        if focused {
            [1.0, 0.84, 0.28, 1.0]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        },
        if focused { 2.0 } else { 1.0 },
        8.0,
    );
    if items.is_empty() {
        push_label(labels, empty_text, rect, HAlign::Center, FONT_BODY, TEXT_HINT);
        return;
    }
    for (row, item) in items.iter().skip(scroll).take(LIST_ROWS).enumerate() {
        let index = row + scroll;
        let row_rect = Rect {
            x: rect.x + 2.0,
            y: rect.y + 2.0 + row as f32 * ROW_H,
            width: rect.width - 4.0,
            height: ROW_H,
        };
        if index == selected {
            push_quad(
                overlay_quads,
                row_rect,
                [0.30, 0.28, 0.60, 1.0],
                [0.24, 0.22, 0.50, 1.0],
                [0.0; 4],
                0.0,
                6.0,
            );
        }
        labels.push(LabelInfo {
            text: &item.name,
            bounds: Rect {
                x: row_rect.x + 12.0,
                width: row_rect.width - 24.0,
                ..row_rect
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(FONT_BODY),
            color_override: TEXT,
            font_family_override: None,
        });
    }
}

fn push_checkbox<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    checked: bool,
    focused: bool,
) {
    let size = 24.0;
    let box_rect = Rect {
        x: rect.x,
        y: rect.y + (rect.height - size) / 2.0,
        width: size,
        height: size,
    };
    push_quad(
        overlay_quads,
        box_rect,
        if checked {
            [0.30, 0.55, 0.30, 1.0]
        } else {
            [0.12, 0.12, 0.15, 1.0]
        },
        if checked {
            [0.22, 0.45, 0.22, 1.0]
        } else {
            [0.10, 0.10, 0.13, 1.0]
        },
        if focused {
            [1.0, 0.84, 0.28, 1.0]
        } else {
            [0.40, 0.40, 0.46, 0.8]
        },
        if focused { 2.0 } else { 1.0 },
        5.0,
    );
    if checked {
        push_label(labels, "✓", box_rect, HAlign::Center, FONT_BODY, TEXT);
    }
    push_label(
        labels,
        text,
        Rect {
            x: rect.x + size + 12.0,
            width: rect.width - size - 12.0,
            ..rect
        },
        HAlign::Left,
        FONT_BODY,
        TEXT,
    );
}

fn push_bar(overlay_quads: &mut Vec<QuadInstance>, track: Rect, value: f32) {
    push_quad(
        overlay_quads,
        track,
        [0.10, 0.10, 0.12, 1.0],
        [0.10, 0.10, 0.12, 1.0],
        [0.30, 0.30, 0.36, 0.5],
        1.0,
        9.0,
    );
    if value > 0.0 {
        push_quad(
            overlay_quads,
            Rect {
                width: (track.width * value).max(track.height),
                ..track
            },
            [0.30, 0.55, 0.30, 1.0],
            [0.22, 0.45, 0.22, 1.0],
            [0.0; 4],
            0.0,
            9.0,
        );
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
            [0.17, 0.17, 0.20, 1.0],
            [0.14, 0.14, 0.17, 1.0],
            [0.34, 0.34, 0.40, 0.7],
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
        border_radius: 9.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, if primary { 0.3 } else { 0.0 }],
        shadow_blur: if primary { 4.0 } else { 0.0 },
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    push_label(labels, text, rect, HAlign::Center, FONT_BUTTON, TEXT);
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: color,
        font_family_override: None,
    });
}

/// A wrapped block of text, anchored at the top of its box.
fn push_paragraph<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    font_size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align: HAlign::Left,
        v_align: VAlign::Top,
        overflow: Overflow::Wrap,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: color,
        font_family_override: None,
    });
}

/// The new cut's name as shown in the report header.
pub fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modal() -> ConformModal {
        ConformModal::new(
            vec![
                ConformChoice {
                    id: 1,
                    name: "V1".into(),
                },
                ConformChoice {
                    id: 2,
                    name: "Trailer".into(),
                },
            ],
            vec![ConformChoice {
                id: 7,
                name: "Français".into(),
            }],
            Some(2),
            Some(7),
            true,
        )
    }

    #[test]
    fn keyboard_walks_the_three_steps_and_asks_for_the_file() {
        let mut modal = modal();
        assert_eq!(modal.selected_video(), Some(2));
        let _ = modal.handle_event(&UiEvent::CursorUp, 800.0, 600.0);
        assert_eq!(modal.selected_video(), Some(1));
        let _ = modal.handle_event(&UiEvent::FocusNext, 800.0, 600.0);
        let _ = modal.handle_event(&UiEvent::FocusNext, 800.0, 600.0);
        let _ = modal.handle_event(&UiEvent::Activate, 800.0, 600.0);
        assert!(!modal.move_takes());
        let _ = modal.handle_event(&UiEvent::FocusNext, 800.0, 600.0);
        match modal.handle_event(&UiEvent::Activate, 800.0, 600.0) {
            ConformModalResult::PickNewVideo {
                video_id,
                band_id,
                move_takes,
            } => {
                assert_eq!((video_id, band_id, move_takes), (1, 7, false));
            }
            _ => panic!("the fourth control picks the new video"),
        }
    }

    #[test]
    fn mouse_picks_a_row_and_the_file_button() {
        let mut modal = modal();
        let (w, h) = (1280.0, 800.0);
        let card = ConformModal::card_rect(w, h);
        let videos = ConformModal::video_list_rect(card);
        let first_row = UiEvent::MousePress {
            x: videos.x + 20.0,
            y: videos.y + 2.0 + ROW_H * 0.5,
        };
        let _ = modal.handle_event(&first_row, w, h);
        assert_eq!(modal.selected_video(), Some(1));
        let pick = ConformModal::pick_rect(card);
        assert!(matches!(
            modal.handle_event(
                &UiEvent::MousePress {
                    x: pick.x + pick.width / 2.0,
                    y: pick.y + pick.height / 2.0,
                },
                w,
                h,
            ),
            ConformModalResult::PickNewVideo { video_id: 1, .. }
        ));
    }

    #[test]
    fn layout_fits_the_card_in_every_stage() {
        let card = ConformModal::card_rect(1920.0, 1080.0);
        let bottom = card.y + card.height;
        let hint = ConformModal::hint_rect(card);
        assert!(hint.y + hint.height <= ConformModal::close_rect(card).y);
        let report = ConformModal::report_rect(card);
        assert!(report.y + report.height + 20.0 <= ConformModal::close_rect(card).y);
        assert!(ConformModal::close_rect(card).y + 46.0 <= bottom);
        assert!(ConformModal::report_rows(card) >= 8);
    }

    #[test]
    fn escape_cancels_a_running_analysis_and_closes_otherwise() {
        let mut modal = modal();
        let escape = UiEvent::KeyInput { text: "\x1b".into() };
        assert!(matches!(
            modal.handle_event(&escape, 800.0, 600.0),
            ConformModalResult::Close
        ));
        modal.start(Arc::new(AtomicU32::new(0.5f32.to_bits())));
        assert!(modal.refresh_progress());
        assert!(modal.progress_text.contains("50"));
        assert!(matches!(
            modal.handle_event(&escape, 800.0, 600.0),
            ConformModalResult::Cancel
        ));
        modal.show_report(ConformReport::default());
        assert!(matches!(
            modal.handle_event(&escape, 800.0, 600.0),
            ConformModalResult::Close
        ));
    }

    #[test]
    fn report_renders_without_rows_when_nothing_moved() {
        let mut modal = modal();
        modal.show_report(ConformReport::default());
        let mut quads = Vec::new();
        let mut labels = Vec::new();
        modal.render(&mut quads, &mut labels, 1280.0, 800.0);
        assert!(labels
            .iter()
            .any(|label| label.text == t("conform.report.nothing")));
        assert!(labels
            .iter()
            .all(|label| label.font_size_override.unwrap_or(FONT_BODY) >= FONT_HINT));
    }
}
