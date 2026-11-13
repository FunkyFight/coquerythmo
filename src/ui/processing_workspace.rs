//! Backend-neutral scene and pointer interaction for the Processing workspace.
//!
//! The workspace shows the audio being processed on the first lane and one
//! lane per track a model produced. The left column lists the models that are
//! installed or being downloaded. The engine (with its hardware warning and
//! its install folder) and the model catalog each live in a modal window.
//!
//! Everything is reachable from the keyboard and spoken with its state: every
//! control exists in the scene even when it is scrolled out of view, and comes
//! back on screen when it gets the focus.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accessibility::AccessibilityEvent;
use crate::i18n::t;
use crate::processing::ProcessingAction;
use crate::recording::WaveformData;
use crate::ui::focus::AccessibleRole;
use crate::ui::primitives::{
    EventResponse, HAlign, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::ui::recording_workspace::spoken_duration;
pub use crate::ui::voicelines_workspace::SceneLabel;

const SIDEBAR_W: f32 = 330.0;
const HEADER_H: f32 = 54.0;
const TOOLBAR_H: f32 = 42.0;
const STATUS_H: f32 = 52.0;
const MODEL_ROW_H: f32 = 56.0;
const ITEM_ROW_H: f32 = 44.0;
const FOOTER_H: f32 = 88.0;
const LANE_INFO_W: f32 = 262.0;
const LANE_MIN_H: f32 = 80.0;
const LANE_MAX_H: f32 = 170.0;
const LANE_GAP: f32 = 6.0;
/// Width of one level of descent: a lane is pushed right by this much per
/// ancestor, behind as many vertical bars.
const SCOPE_STEP: f32 = 14.0;
const SCOPE_MAX: usize = 4;
const RULER_H: f32 = 22.0;
const CONFIRM_DELAY: Duration = Duration::from_secs(5);

const ENGINE_CARD_W: f32 = 660.0;
const CATALOG_CARD_W: f32 = 800.0;
const CARD_PAD: f32 = 28.0;
const CATALOG_ROW_H: f32 = 86.0;

/// Smallest text of the tab: readable with low vision.
const SMALL: f32 = 12.0;

const BG: [f32; 4] = [0.055, 0.058, 0.072, 1.0];
const PANEL: [f32; 4] = [0.085, 0.09, 0.11, 1.0];
const PANEL_ALT: [f32; 4] = [0.115, 0.12, 0.15, 1.0];
const SELECTED: [f32; 4] = [0.16, 0.14, 0.30, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.9];
/// Thick light border of what is selected or listened to: the state does not
/// rest on colour alone.
const SELECTED_BORDER: [f32; 4] = [0.84, 0.80, 1.0, 1.0];
const ACCENT: [f32; 4] = [0.38, 0.31, 0.88, 1.0];
const DANGER: [f32; 4] = [0.62, 0.20, 0.24, 1.0];
const FOCUS: [f32; 4] = [0.25, 0.52, 1.0, 1.0];
/// Bars that tie a produced track to the track it descends from.
const SCOPE: [f32; 4] = [0.70, 0.64, 1.0, 1.0];
const TEXT: [u8; 3] = [232, 234, 242];
const MUTED: [u8; 3] = [176, 180, 196];
const GOOD: [u8; 3] = [130, 220, 160];
const WARN: [u8; 3] = [246, 200, 110];

const LANE_COLORS: [[f32; 4]; 6] = [
    [0.38, 0.62, 0.96, 0.9],
    [0.96, 0.60, 0.38, 0.9],
    [0.48, 0.84, 0.56, 0.9],
    [0.90, 0.48, 0.74, 0.9],
    [0.92, 0.82, 0.40, 0.9],
    [0.50, 0.82, 0.88, 0.9],
];

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessingLayout {
    pub content: Rect,
    pub sidebar: Rect,
    pub header: Rect,
    pub toolbar: Rect,
    pub tracks: Rect,
    pub status: Rect,
    /// Height kept free at the bottom of the left column for the shortcut
    /// panel of the application.
    pub panel_height: f32,
}

impl ProcessingLayout {
    /// Leaves `height` free at the bottom of the left column.
    pub fn above_panel(mut self, height: f32) -> Self {
        self.panel_height = height.max(0.0);
        self
    }

    pub fn compute(content: Rect) -> Self {
        let sidebar_width = SIDEBAR_W.min((content.width * 0.36).max(220.0));
        let sidebar = Rect {
            x: content.x,
            y: content.y,
            width: sidebar_width,
            height: content.height,
        };
        let main_x = sidebar.x + sidebar.width;
        let main_width = (content.x + content.width - main_x).max(0.0);
        let header = Rect {
            x: main_x,
            y: content.y,
            width: main_width,
            height: HEADER_H,
        };
        let toolbar = Rect {
            x: main_x,
            y: header.y + header.height,
            width: main_width,
            height: TOOLBAR_H,
        };
        let status = Rect {
            x: main_x,
            y: content.y + content.height - STATUS_H,
            width: main_width,
            height: STATUS_H,
        };
        let tracks_top = toolbar.y + toolbar.height;
        let tracks = Rect {
            x: main_x + 12.0,
            y: tracks_top + 8.0,
            width: (main_width - 24.0).max(0.0),
            height: (status.y - tracks_top - 16.0).max(0.0),
        };
        Self {
            content,
            sidebar,
            header,
            toolbar,
            tracks,
            status,
            panel_height: 0.0,
        }
    }
}

/// What the graphics card means for the user, already put into words.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuLine {
    pub text: String,
    pub warning: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgressView {
    pub title: String,
    pub detail: String,
    /// `None` while the amount of work is unknown.
    pub fraction: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ModelState {
    /// In the catalog, not downloaded.
    #[default]
    Available,
    Downloading {
        fraction: Option<f32>,
        detail: String,
    },
    Installed,
    Removing,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelView {
    pub title: String,
    pub purpose: String,
    pub size: String,
    /// Names of the tracks the model produces, for the catalog.
    pub outputs: String,
    pub state: ModelState,
}

/// The engine as the engine window shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineView {
    pub supported: bool,
    pub installed: bool,
    pub uses_gpu: bool,
    pub download_size: String,
    pub gpu: GpuLine,
    /// Folder everything is installed in; empty while none is chosen.
    pub directory: String,
    /// Something is installed there: the folder cannot change.
    pub directory_locked: bool,
    pub disk_usage: String,
    /// Installation or removal in progress.
    pub task: Option<ProgressView>,
    /// The task is the installation: it can be cancelled.
    pub installing: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemView {
    pub id: u64,
    pub name: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TrackView {
    pub title: String,
    pub subtitle: String,
    pub waveform: Arc<WaveformData>,
    pub waveform_sample_rate: u32,
    pub duration_ms: u64,
    /// How many tracks it descends from: 0 for the original audio, 1 for a
    /// track made from it, 2 for a track made from that one.
    pub depth: usize,
}

/// Everything the workspace shows, prepared by the application state.
#[derive(Debug, Clone, Default)]
pub struct ProcessingView {
    pub engine: EngineView,
    pub models: Vec<ModelView>,
    pub selected_model: usize,
    pub best_quality: bool,
    /// Model pass in progress.
    pub job: Option<ProgressView>,
    pub items: Vec<ItemView>,
    pub active_item: Option<u64>,
    pub tracks: Vec<TrackView>,
    pub listening: usize,
    pub has_project_video: bool,
}

/// A focusable element of the tab, as the keyboard and the screen reader see
/// it. `visible` is false for one scrolled out of view: it keeps its place in
/// the `Tab` order and is revealed when it gets the focus.
#[derive(Debug, Clone)]
pub struct SceneControl {
    pub id: String,
    pub label: String,
    /// State spoken after the name (« installé », a duration, a progress).
    pub value: Option<String>,
    /// i18n key of its keyboard shortcut, spoken last.
    pub shortcut: Option<&'static str>,
    pub bounds: Rect,
    pub role: AccessibleRole,
    pub selected: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessingScene {
    pub quads: Vec<QuadInstance>,
    pub labels: Vec<SceneLabel>,
    pub controls: Vec<SceneControl>,
}

impl ProcessingScene {
    fn translate(&mut self, dx: f32, dy: f32) {
        for quad in &mut self.quads {
            quad.rect[0] += dx;
            quad.rect[1] += dy;
        }
        for label in &mut self.labels {
            label.bounds.x += dx;
            label.bounds.y += dy;
        }
        for control in &mut self.controls {
            control.bounds.x += dx;
            control.bounds.y += dy;
        }
    }

    /// Adds a part of the scene. One that does not fit on screen only keeps
    /// its controls, marked as out of view.
    fn absorb(&mut self, mut part: ProcessingScene, visible: bool) {
        if visible {
            self.quads.append(&mut part.quads);
            self.labels.append(&mut part.labels);
        } else {
            for control in &mut part.controls {
                control.visible = false;
            }
        }
        self.controls.append(&mut part.controls);
    }

    fn set_value(&mut self, id: &str, value: impl Into<String>) {
        if let Some(control) = self
            .controls
            .iter_mut()
            .rev()
            .find(|control| control.id == id)
        {
            control.value = Some(value.into());
        }
    }

    fn set_shortcut(&mut self, id: &str, shortcut: &'static str) {
        if let Some(control) = self
            .controls
            .iter_mut()
            .rev()
            .find(|control| control.id == id)
        {
            control.shortcut = Some(shortcut);
        }
    }

    fn set_name(&mut self, id: &str, name: impl Into<String>) {
        if let Some(control) = self
            .controls
            .iter_mut()
            .rev()
            .find(|control| control.id == id)
        {
            control.label = name.into();
        }
    }
}

/// The two windows of the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingModal {
    /// Engine: hardware warning, install folder, installation, removal.
    Engine,
    /// Models and features to download.
    Catalog,
}

/// What a control does: an application action, or a window of this tab.
enum Target {
    App(ProcessingAction),
    Open(ProcessingModal),
    Close,
}

/// A control of the open window: where it is, what it is called.
#[derive(Debug, Clone)]
struct ModalHit {
    bounds: Rect,
    id: String,
    spoken: String,
    visible: bool,
}

#[derive(Debug, Default)]
pub struct ProcessingWorkspaceUi {
    view: ProcessingView,
    view_start_ms: u64,
    view_duration_ms: u64,
    viewed_item: Option<u64>,
    lane_scroll: f32,
    item_scroll: usize,
    items_visible: usize,
    /// On-screen controls of the last scene, for pointer hit-testing.
    hits: Vec<(Rect, String)>,
    /// Every control of the last scene, in `Tab` order.
    control_ids: Vec<String>,
    /// Waveform rectangles of the last scene, by track index.
    waveforms: Vec<(Rect, usize)>,
    /// A destructive button waits for a second activation.
    confirm: Option<(String, Instant)>,
    scrubbing: bool,
    modal: Option<ProcessingModal>,
    /// Controls of the open window, in keyboard order.
    modal_hits: Vec<ModalHit>,
    modal_card: Rect,
    modal_focus: usize,
    catalog_scroll: usize,
    catalog_visible: usize,
}

impl ProcessingWorkspaceUi {
    pub fn set_view(&mut self, view: ProcessingView) {
        self.view = view;
    }

    pub fn view(&self) -> &ProcessingView {
        &self.view
    }

    pub fn open_modal(&mut self, modal: ProcessingModal) {
        if self.modal != Some(modal) {
            self.modal = Some(modal);
            self.modal_focus = 0;
            self.modal_hits.clear();
            self.catalog_scroll = 0;
            self.confirm = None;
        }
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
        self.modal_hits.clear();
        self.confirm = None;
    }

    pub fn modal(&self) -> Option<ProcessingModal> {
        self.modal
    }

    fn modal_title(modal: ProcessingModal) -> &'static str {
        match modal {
            ProcessingModal::Engine => t("processing.engine.title"),
            ProcessingModal::Catalog => t("processing.catalog.title"),
        }
    }

    /// What a screen reader says when the window opens: its title and
    /// everything written in it, which no button carries.
    pub fn modal_announcement(&self) -> Option<String> {
        let modal = self.modal?;
        Some(format!(
            "{}. {}",
            Self::modal_title(modal),
            self.modal_description(modal)
        ))
    }

    fn modal_description(&self, modal: ProcessingModal) -> String {
        let engine = &self.view.engine;
        let parts: Vec<String> = match modal {
            ProcessingModal::Catalog => {
                let mut parts = vec![t("processing.catalog.subtitle").to_string()];
                if !engine.installed {
                    parts.push(t("processing.catalog.engine_missing").to_string());
                }
                parts
            }
            ProcessingModal::Engine if !engine.supported => {
                vec![t("processing.empty.unsupported").to_string()]
            }
            ProcessingModal::Engine => match &engine.task {
                Some(task) => vec![task.title.clone(), task.detail.clone()],
                None if engine.installed => vec![
                    t(if engine.uses_gpu {
                        "processing.engine.installed_gpu"
                    } else {
                        "processing.engine.installed_cpu"
                    })
                    .to_string(),
                    engine.gpu.text.clone(),
                    format!(
                        "{} : {}",
                        t("processing.engine.directory"),
                        engine.directory
                    ),
                    engine.disk_usage.clone(),
                ],
                None => vec![
                    t("processing.engine.intro").to_string(),
                    t("processing.engine.requirement").to_string(),
                    format!("{} {}", t("processing.engine.your_card"), engine.gpu.text),
                    format!(
                        "{} {}",
                        t("processing.engine.download"),
                        engine.download_size
                    ),
                    t("processing.engine.space").to_string(),
                    format!(
                        "{} : {}",
                        t("processing.engine.directory"),
                        if engine.directory.is_empty() {
                            t("processing.engine.no_directory")
                        } else {
                            engine.directory.as_str()
                        }
                    ),
                ],
            },
        };
        parts
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn duration_ms(&self) -> u64 {
        self.view
            .tracks
            .iter()
            .map(|track| track.duration_ms)
            .max()
            .unwrap_or(0)
            .max(1)
    }

    fn sync(&mut self) {
        let duration = self.duration_ms();
        if self.viewed_item != self.view.active_item {
            self.viewed_item = self.view.active_item;
            self.view_start_ms = 0;
            self.view_duration_ms = duration;
            self.lane_scroll = 0.0;
        }
        if self.view_duration_ms == 0 {
            self.view_duration_ms = duration;
        }
        self.view_duration_ms = self.view_duration_ms.clamp(200.min(duration), duration);
        self.view_start_ms = self
            .view_start_ms
            .min(duration.saturating_sub(self.view_duration_ms));
        if self
            .confirm
            .as_ref()
            .is_some_and(|(_, deadline)| Instant::now() > *deadline)
        {
            self.confirm = None;
        }
    }

    /// True while a destructive button shows its confirmation label: the
    /// scene must be redrawn when the delay runs out.
    pub fn is_waiting_confirmation(&self) -> bool {
        self.confirm.is_some()
    }

    fn lane_height(&self, layout: ProcessingLayout) -> f32 {
        let count = self.view.tracks.len().max(1) as f32;
        let available = (layout.tracks.height - RULER_H).max(0.0);
        ((available - (count - 1.0) * LANE_GAP) / count).clamp(LANE_MIN_H, LANE_MAX_H)
    }

    fn max_lane_scroll(&self, layout: ProcessingLayout) -> f32 {
        let count = self.view.tracks.len() as f32;
        let total = count * self.lane_height(layout) + (count - 1.0).max(0.0) * LANE_GAP;
        (total - (layout.tracks.height - RULER_H)).max(0.0)
    }

    /// Scrolls so the control that just got the focus is on screen.
    pub fn reveal(&mut self, id: &str, layout: ProcessingLayout) {
        let Some((family, index, _)) = split_id(id) else {
            return;
        };
        match family {
            "track" => {
                let lane = index;
                let height = self.lane_height(layout);
                let top = lane as f32 * (height + LANE_GAP);
                let available = (layout.tracks.height - RULER_H).max(0.0);
                if top < self.lane_scroll {
                    self.lane_scroll = top;
                } else if top + height > self.lane_scroll + available {
                    self.lane_scroll = top + height - available;
                }
                self.lane_scroll = self.lane_scroll.clamp(0.0, self.max_lane_scroll(layout));
            }
            "item" => {
                let Some(row) = self
                    .view
                    .items
                    .iter()
                    .position(|item| item.id == index as u64)
                else {
                    return;
                };
                let visible = self.items_visible.max(1);
                if row < self.item_scroll {
                    self.item_scroll = row;
                } else if row >= self.item_scroll + visible {
                    self.item_scroll = row + 1 - visible;
                }
            }
            _ => {}
        }
    }

    /// The control above or below `id` in its list: the same button on the
    /// neighbouring track, the next model, the next audio.
    pub fn neighbour(&self, id: &str, forward: bool) -> Option<String> {
        let (family, index, suffix) = split_id(id)?;
        // A model row and the cancel button of a download are the same level.
        let level = |suffix: &str| -> String {
            if family == "model" && suffix == "cancel" {
                String::new()
            } else {
                suffix.to_owned()
            }
        };
        let wanted = level(suffix);
        let siblings: Vec<&String> = self
            .control_ids
            .iter()
            .filter(|candidate| {
                split_id(candidate).is_some_and(|(other, _, other_suffix)| {
                    other == family && level(other_suffix) == wanted
                })
            })
            .collect();
        let position = siblings.iter().position(|candidate| *candidate == id)?;
        let next = if forward {
            position
                .checked_add(1)
                .filter(|next| *next < siblings.len())
        } else {
            position.checked_sub(1)
        };
        if let Some(next) = next {
            return Some(siblings[next].clone());
        }
        // The original audio has neither « Exporter » nor « + Projet »: going
        // up from those lands on its « Écouter ».
        if family == "track" && suffix != "listen" {
            let lane = if forward {
                index.checked_add(1)?
            } else {
                index.checked_sub(1)?
            };
            let fallback = format!("processing.track.{lane}.listen");
            return self.control_ids.contains(&fallback).then_some(fallback);
        }
        None
    }

    /// `Suppr` on a focused audio, model or produced track removes it, with
    /// the same confirmation as its cross.
    pub fn delete_action(&mut self, id: &str) -> Option<UiAction> {
        let (family, index, suffix) = split_id(id)?;
        let target = match family {
            "item" => format!("processing.item.{index}.remove"),
            "model" if suffix != "cancel" => format!("processing.model.{index}.remove"),
            "track" => format!("processing.track.{index}.remove"),
            _ => return None,
        };
        if !self.control_ids.contains(&target) {
            return None;
        }
        self.control_action(&target)
    }

    pub fn handle_event(&mut self, event: &UiEvent, layout: ProcessingLayout) -> EventResponse {
        self.sync();
        match event {
            UiEvent::MousePress { x, y }
            | UiEvent::DoubleClick { x, y }
            | UiEvent::CtrlClick { x, y }
            | UiEvent::ShiftMousePress { x, y } => {
                let hit = self
                    .hits
                    .iter()
                    .rev()
                    .find(|(rect, _)| rect.contains(*x, *y))
                    .map(|(_, id)| id.clone());
                if let Some(id) = hit {
                    return match self.control_action(&id) {
                        Some(action) => EventResponse::Action(action),
                        None => EventResponse::Consumed,
                    };
                }
                self.confirm = None;
                let lane = self
                    .waveforms
                    .iter()
                    .find(|(rect, _)| rect.contains(*x, *y))
                    .copied();
                if let Some((rect, track)) = lane {
                    self.scrubbing = true;
                    let at_ms = self.ms_at_x(rect, *x);
                    return EventResponse::Action(UiAction::Processing(ProcessingAction::Listen {
                        track,
                        at_ms: Some(at_ms),
                    }));
                }
                // The empty work area is one large button for the next step.
                if self.view.tracks.is_empty() && layout.tracks.contains(*x, *y) {
                    return match self.control_action("processing.primary") {
                        Some(action) => EventResponse::Action(action),
                        None => EventResponse::Consumed,
                    };
                }
                if layout.content.contains(*x, *y) && !layout.toolbar.contains(*x, *y) {
                    return EventResponse::Consumed;
                }
            }
            UiEvent::MouseMove { x, .. } if self.scrubbing => {
                if let Some((rect, _)) = self.waveforms.first().copied() {
                    let frame = (self.ms_at_x(rect, *x) / 10) as i64;
                    return EventResponse::Action(UiAction::SeekAbsolute(frame));
                }
            }
            UiEvent::MouseRelease { .. } if self.scrubbing => {
                self.scrubbing = false;
                return EventResponse::Action(UiAction::FinishSeek);
            }
            UiEvent::Scroll {
                x, y, delta, ctrl, ..
            } if layout.tracks.contains(*x, *y) && !self.view.tracks.is_empty() => {
                let duration = self.duration_ms();
                let over_waveform = *x >= layout.tracks.x + LANE_INFO_W;
                let zoomed = self.view_duration_ms < duration;
                if *ctrl && over_waveform {
                    let wave = waveform_span(layout);
                    let anchor = self.ms_at_x(wave, *x);
                    let factor = if *delta > 0.0 { 0.75 } else { 1.35 };
                    let next = ((self.view_duration_ms as f64 * factor).round() as u64)
                        .clamp(200.min(duration), duration);
                    let ratio = ((*x - wave.x) / wave.width.max(1.0)).clamp(0.0, 1.0) as f64;
                    self.view_duration_ms = next;
                    self.view_start_ms = anchor
                        .saturating_sub((next as f64 * ratio).round() as u64)
                        .min(duration.saturating_sub(next));
                } else if over_waveform && zoomed {
                    let amount = (self.view_duration_ms / 8).max(1);
                    self.view_start_ms = if *delta > 0.0 {
                        self.view_start_ms.saturating_sub(amount)
                    } else {
                        self.view_start_ms
                            .saturating_add(amount)
                            .min(duration.saturating_sub(self.view_duration_ms))
                    };
                } else {
                    let step = self.lane_height(layout) * 0.5;
                    self.lane_scroll = if *delta > 0.0 {
                        (self.lane_scroll - step).max(0.0)
                    } else {
                        (self.lane_scroll + step).min(self.max_lane_scroll(layout))
                    };
                }
                return EventResponse::Consumed;
            }
            UiEvent::Scroll { x, y, delta, .. } if layout.sidebar.contains(*x, *y) => {
                let max_scroll = self.view.items.len().saturating_sub(1);
                self.item_scroll = if *delta > 0.0 {
                    self.item_scroll.saturating_sub(1)
                } else {
                    self.item_scroll.saturating_add(1).min(max_scroll)
                };
                return EventResponse::Consumed;
            }
            _ => {}
        }
        EventResponse::Ignored
    }

    /// Input while a window of the tab is open: it takes everything.
    pub fn handle_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let Some(modal) = self.modal else {
            return EventResponse::Ignored;
        };
        self.sync();
        let count = self.modal_hits.len();
        let backwards = count.saturating_sub(1);
        let step = match event {
            UiEvent::FocusNext | UiEvent::CursorDown | UiEvent::CursorRight => Some(1),
            UiEvent::FocusPrevious | UiEvent::CursorUp | UiEvent::CursorLeft => Some(backwards),
            UiEvent::KeyInput { text } if text == "\t" => Some(1),
            UiEvent::KeyInput { text } if text == "\u{b}" => Some(backwards),
            _ => None,
        };
        if let Some(step) = step {
            if count == 0 {
                return EventResponse::Consumed;
            }
            self.modal_focus = (self.modal_focus + step) % count;
            let hit = self.modal_hits[self.modal_focus].clone();
            self.reveal_in_catalog(&hit.id);
            return EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Focus {
                label: hit.spoken,
                role: "button".to_string(),
            }));
        }
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => self.close_with_announcement(modal),
            UiEvent::Activate => self.activate_modal_focus(),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                self.activate_modal_focus()
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                let hit = self
                    .modal_hits
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, hit)| hit.visible && hit.bounds.contains(*x, *y))
                    .map(|(index, hit)| (index, hit.id.clone()));
                if let Some((index, id)) = hit {
                    self.modal_focus = index;
                    return match self.control_action(&id) {
                        Some(action) => EventResponse::Action(action),
                        None => EventResponse::Consumed,
                    };
                }
                self.confirm = None;
                if !self.modal_card.contains(*x, *y) {
                    return self.close_with_announcement(modal);
                }
                EventResponse::Consumed
            }
            UiEvent::Scroll { delta, .. } if modal == ProcessingModal::Catalog => {
                let max_scroll = self
                    .view
                    .models
                    .len()
                    .saturating_sub(self.catalog_visible.max(1));
                self.catalog_scroll = if *delta > 0.0 {
                    self.catalog_scroll.saturating_sub(1)
                } else {
                    self.catalog_scroll.saturating_add(1).min(max_scroll)
                };
                EventResponse::Consumed
            }
            _ => EventResponse::Consumed,
        }
    }

    /// Scrolls the catalog so the model whose button has the focus shows.
    fn reveal_in_catalog(&mut self, id: &str) {
        if self.modal != Some(ProcessingModal::Catalog) {
            return;
        }
        let Some(("model", row, _)) = split_id(id) else {
            return;
        };
        let visible = self.catalog_visible.max(1);
        if row < self.catalog_scroll {
            self.catalog_scroll = row;
        } else if row >= self.catalog_scroll + visible {
            self.catalog_scroll = row + 1 - visible;
        }
    }

    fn activate_modal_focus(&mut self) -> EventResponse {
        let Some(id) = self
            .modal_hits
            .get(self.modal_focus)
            .map(|hit| hit.id.clone())
        else {
            return EventResponse::Consumed;
        };
        match self.control_action(&id) {
            Some(action) => EventResponse::Action(action),
            None => EventResponse::Consumed,
        }
    }

    fn close_with_announcement(&mut self, modal: ProcessingModal) -> EventResponse {
        self.close_modal();
        EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Activation {
            label: format!(
                "{} : {}",
                Self::modal_title(modal),
                t("accessibility.closed")
            ),
        }))
    }

    fn ms_at_x(&self, wave: Rect, x: f32) -> u64 {
        let ratio = ((x - wave.x) / wave.width.max(1.0)).clamp(0.0, 1.0) as f64;
        self.view_start_ms
            .saturating_add((self.view_duration_ms as f64 * ratio).round() as u64)
            .min(self.duration_ms())
    }

    /// Action of a control, from a click or from the keyboard. A destructive
    /// control asks for a second activation, and says so; a control that only
    /// opens or closes a window of the tab answers with what to announce.
    /// `None`: the control does nothing (a text to read).
    pub fn control_action(&mut self, id: &str) -> Option<UiAction> {
        let target = self.target_for(id)?;
        let action = match target {
            Target::Open(modal) => {
                self.open_modal(modal);
                return Some(UiAction::Accessibility(AccessibilityEvent::Activation {
                    label: self.modal_announcement().unwrap_or_default(),
                }));
            }
            Target::Close => {
                let modal = self.modal?;
                return match self.close_with_announcement(modal) {
                    EventResponse::Action(action) => Some(action),
                    _ => None,
                };
            }
            Target::App(action) => action,
        };
        if is_destructive(&action) && !self.confirming(id) {
            self.confirm = Some((id.to_owned(), Instant::now() + CONFIRM_DELAY));
            return Some(UiAction::Accessibility(AccessibilityEvent::Activation {
                label: t("processing.confirm").to_string(),
            }));
        }
        self.confirm = None;
        Some(UiAction::Processing(action))
    }

    fn target_for(&self, id: &str) -> Option<Target> {
        let rest = id.strip_prefix("processing.")?;
        let action = match rest {
            "engine.open" => return Some(Target::Open(ProcessingModal::Engine)),
            "catalog.open" => return Some(Target::Open(ProcessingModal::Catalog)),
            "modal.close" => return Some(Target::Close),
            "primary" => return self.primary_action().map(|(_, target)| target),
            "import" => ProcessingAction::ImportAudio,
            "project_video" => ProcessingAction::UseProjectVideo,
            "quality" => ProcessingAction::ToggleQuality,
            "cancel" => ProcessingAction::Cancel,
            "export_all" => ProcessingAction::ExportAll,
            "engine.choose" => ProcessingAction::ChooseDirectory,
            "runtime.install" => ProcessingAction::InstallRuntime,
            "runtime.cancel" => ProcessingAction::CancelEngine,
            "runtime.uninstall" => ProcessingAction::UninstallRuntime,
            "uninstall_all" => ProcessingAction::UninstallAll,
            "open_folder" => ProcessingAction::OpenFolder,
            _ => match split_id(id)? {
                ("model", index, "download") => ProcessingAction::DownloadModel(index),
                ("model", index, "cancel") => ProcessingAction::CancelDownload(index),
                ("model", index, "remove") => ProcessingAction::RemoveModel(index),
                ("model", index, "") => ProcessingAction::SelectModel(index),
                ("item", id, "remove") => ProcessingAction::RemoveItem(id as u64),
                ("item", id, "") => ProcessingAction::SelectItem(id as u64),
                ("track", track, "listen") => ProcessingAction::Listen { track, at_ms: None },
                ("track", track, "export") => ProcessingAction::ExportTrack(track),
                ("track", track, "project") => ProcessingAction::AddTrackToProject(track),
                ("track", track, "remove") => ProcessingAction::RemoveTrack(track),
                _ => return None,
            },
        };
        Some(Target::App(action))
    }

    /// The one button that always names the next step towards a result:
    /// the engine, then a model, then an audio, then the run.
    fn primary_action(&self) -> Option<(String, Target)> {
        let view = &self.view;
        if view.job.is_some() {
            return Some((
                t("processing.cancel_job").into(),
                Target::App(ProcessingAction::Cancel),
            ));
        }
        if !view.engine.supported {
            return None;
        }
        if !view.engine.installed {
            let text = if view.engine.installing {
                t("processing.engine.installing")
            } else {
                t("processing.engine.install_open")
            };
            return Some((text.into(), Target::Open(ProcessingModal::Engine)));
        }
        let model = view
            .models
            .get(view.selected_model)
            .filter(|model| model.state == ModelState::Installed);
        let Some(model) = model else {
            return Some((
                t("processing.catalog.open").into(),
                Target::Open(ProcessingModal::Catalog),
            ));
        };
        if view.active_item.is_none() {
            return Some((
                t("processing.import").into(),
                Target::App(ProcessingAction::ImportAudio),
            ));
        }
        Some((
            format!("{} : {}", t("processing.run"), model.purpose),
            Target::App(ProcessingAction::Run),
        ))
    }

    fn confirming(&self, id: &str) -> bool {
        self.confirm
            .as_ref()
            .is_some_and(|(pending, deadline)| pending == id && Instant::now() <= *deadline)
    }

    fn button_fill(&self, id: &str) -> [f32; 4] {
        if self.confirming(id) {
            DANGER
        } else {
            PANEL_ALT
        }
    }

    /// A destructive button: once armed, its label asks for the confirmation.
    fn confirm_button(&self, scene: &mut ProcessingScene, id: &str, text: &str, bounds: Rect) {
        let shown = if self.confirming(id) {
            t("processing.confirm")
        } else {
            text
        };
        button(scene, id, shown, bounds, self.button_fill(id), true);
        scene.set_name(id, text);
    }

    /// The small cross of a row, with the same confirmation.
    fn cross_button(&self, scene: &mut ProcessingScene, id: &str, name: &str, bounds: Rect) {
        let text = if self.confirming(id) { "?" } else { "×" };
        button(scene, id, text, bounds, self.button_fill(id), true);
        scene.set_name(id, name);
    }

    pub fn scene(&mut self, current_ms: u64, layout: ProcessingLayout) -> ProcessingScene {
        self.sync();
        let mut scene = ProcessingScene::default();
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.sidebar, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.header, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.status, PANEL, BORDER, 0.0));

        let mut waveforms = Vec::new();
        self.header(&mut scene, layout);
        self.sidebar(&mut scene, layout);
        self.lanes(&mut scene, &mut waveforms, current_ms, layout);
        self.status(&mut scene, layout);

        self.hits = scene
            .controls
            .iter()
            .filter(|control| control.visible)
            .map(|control| (control.bounds, control.id.clone()))
            .collect();
        self.control_ids = scene
            .controls
            .iter()
            .map(|control| control.id.clone())
            .collect();
        self.waveforms = waveforms;
        scene
    }

    /// Scene of the open window, drawn above everything else.
    pub fn modal_scene(&mut self, screen_w: f32, screen_h: f32) -> Option<ProcessingScene> {
        let modal = self.modal?;
        self.sync();
        let mut content = ProcessingScene::default();
        let (width, height) = match modal {
            ProcessingModal::Engine => (ENGINE_CARD_W, self.engine_window(&mut content)),
            ProcessingModal::Catalog => {
                let height = self.catalog_window(&mut content, screen_h);
                (CATALOG_CARD_W, height)
            }
        };
        let width = width.min(screen_w - 24.0);
        let card = Rect {
            x: ((screen_w - width) / 2.0).max(0.0),
            y: ((screen_h - height) / 2.0).max(0.0),
            width,
            height,
        };
        content.translate(card.x, card.y);

        let mut scene = ProcessingScene::default();
        scene.quads.push(quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.015, 0.015, 0.025, 0.86],
            [0.0; 4],
            0.0,
        ));
        scene.quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.16, 0.16, 0.20, 1.0],
            color_bottom: [0.105, 0.105, 0.14, 1.0],
            border_color: [0.38, 0.38, 0.48, 0.9],
            border_width: 1.0,
            border_radius: 16.0,
            shadow_offset: [0.0, 8.0],
            shadow_color: [0.0, 0.0, 0.0, 0.58],
            shadow_blur: 18.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        scene.quads.append(&mut content.quads);
        scene.labels.append(&mut content.labels);
        scene.controls.append(&mut content.controls);

        self.modal_card = card;
        self.modal_hits = scene
            .controls
            .iter()
            .map(|control| ModalHit {
                bounds: control.bounds,
                id: control.id.clone(),
                spoken: spoken(control),
                visible: control.visible,
            })
            .collect();
        self.modal_focus = self
            .modal_focus
            .min(self.modal_hits.len().saturating_sub(1));
        let focused = self
            .modal_hits
            .get(self.modal_focus)
            .filter(|hit| hit.visible)
            .map(|hit| hit.bounds);
        if let Some(bounds) = focused {
            // Thick double ring, blue then white: visible with low vision on
            // any button colour.
            let ring = |inset: f32, color: [f32; 4], width: f32| QuadInstance {
                border_width: width,
                ..quad(
                    Rect {
                        x: bounds.x - inset,
                        y: bounds.y - inset,
                        width: bounds.width + inset * 2.0,
                        height: bounds.height + inset * 2.0,
                    },
                    [0.0; 4],
                    color,
                    8.0,
                )
            };
            scene.quads.push(ring(3.0, FOCUS, 3.0));
            scene.quads.push(ring(0.5, [1.0, 1.0, 1.0, 1.0], 1.5));
        }
        Some(scene)
    }

    /// Title of a window. It is also its first control: the screen reader
    /// reads the whole text of the window there, and again on demand.
    fn window_title(&self, scene: &mut ProcessingScene, modal: ProcessingModal, width: f32) {
        let bounds = row(CARD_PAD, 22.0, width, 32.0);
        label(
            scene,
            Self::modal_title(modal),
            bounds,
            HAlign::Left,
            20.0,
            [186, 178, 255],
        );
        scene.controls.push(SceneControl {
            id: "processing.modal.info".into(),
            label: Self::modal_title(modal).to_string(),
            value: Some(self.modal_description(modal)),
            shortcut: None,
            bounds,
            role: AccessibleRole::Region,
            selected: false,
            visible: true,
        });
    }

    /// The engine window, drawn from the top-left corner of its card.
    /// Returns the height of the card.
    fn engine_window(&self, scene: &mut ProcessingScene) -> f32 {
        let engine = &self.view.engine;
        let x = CARD_PAD;
        let width = ENGINE_CARD_W - CARD_PAD * 2.0;
        self.window_title(scene, ProcessingModal::Engine, width);
        let mut y = 68.0;

        let button_h = 40.0;
        let close = |scene: &mut ProcessingScene, y: f32| {
            button(
                scene,
                "processing.modal.close",
                t("processing.close"),
                row(x + width - 150.0, y, 150.0, button_h),
                PANEL_ALT,
                true,
            );
        };

        if !engine.supported {
            y += paragraph(
                scene,
                t("processing.empty.unsupported"),
                x,
                y,
                width,
                14.0,
                TEXT,
            ) + 18.0;
            close(scene, y);
            return y + button_h + 24.0;
        }

        if let Some(task) = &engine.task {
            label(
                scene,
                &task.title,
                row(x, y, width, 22.0),
                HAlign::Left,
                15.0,
                TEXT,
            );
            y += 30.0;
            progress_bar(scene, row(x, y, width, 12.0), task.fraction);
            y += 22.0;
            label(
                scene,
                &task.detail,
                row(x, y, width, 18.0),
                HAlign::Left,
                SMALL,
                MUTED,
            );
            y += 30.0;
            if engine.installing {
                y += paragraph(
                    scene,
                    t("processing.engine.background_note"),
                    x,
                    y,
                    width,
                    13.0,
                    MUTED,
                ) + 18.0;
                button(
                    scene,
                    "processing.runtime.cancel",
                    t("processing.engine.cancel_install"),
                    row(x, y, 220.0, button_h),
                    PANEL_ALT,
                    true,
                );
            }
            close(scene, y);
            return y + button_h + 24.0;
        }

        if engine.installed {
            let (status, color) = if engine.uses_gpu {
                (t("processing.engine.installed_gpu"), GOOD)
            } else {
                (t("processing.engine.installed_cpu"), WARN)
            };
            label(
                scene,
                status,
                row(x, y, width, 22.0),
                HAlign::Left,
                15.0,
                color,
            );
            y += 30.0;
            if !engine.gpu.text.is_empty() {
                y += paragraph(
                    scene,
                    &engine.gpu.text,
                    x,
                    y,
                    width,
                    13.0,
                    if engine.gpu.warning { WARN } else { TEXT },
                ) + 8.0;
            }
            label(
                scene,
                t("processing.engine.directory"),
                row(x, y, width, 18.0),
                HAlign::Left,
                SMALL,
                MUTED,
            );
            y += 20.0;
            y += paragraph(scene, &engine.directory, x, y, width, 13.0, TEXT) + 6.0;
            label(
                scene,
                &engine.disk_usage,
                row(x, y, width, 18.0),
                HAlign::Left,
                SMALL,
                MUTED,
            );
            y += 34.0;

            let third = (width - 16.0) / 3.0;
            button(
                scene,
                "processing.open_folder",
                t("processing.open_folder"),
                row(x, y, third, button_h),
                PANEL_ALT,
                true,
            );
            self.confirm_button(
                scene,
                "processing.runtime.uninstall",
                t("processing.engine.uninstall"),
                row(x + third + 8.0, y, third, button_h),
            );
            self.confirm_button(
                scene,
                "processing.uninstall_all",
                t("processing.uninstall_all"),
                row(x + (third + 8.0) * 2.0, y, third, button_h),
            );
            y += button_h + 14.0;
            close(scene, y);
            return y + button_h + 24.0;
        }

        // Not installed: what it is, what it needs, where it goes.
        y += paragraph(scene, t("processing.engine.intro"), x, y, width, 14.0, TEXT) + 14.0;

        let warning_lines = wrap(t("processing.engine.requirement"), width - 28.0, 14.0);
        let warning_h = warning_lines.len() as f32 * line_height(14.0) + 22.0;
        scene.quads.push(QuadInstance {
            border_width: 2.0,
            ..quad(
                row(x, y, width, warning_h),
                [0.20, 0.15, 0.06, 1.0],
                [0.96, 0.78, 0.43, 1.0],
                8.0,
            )
        });
        paragraph(
            scene,
            t("processing.engine.requirement"),
            x + 14.0,
            y + 11.0,
            width - 28.0,
            14.0,
            WARN,
        );
        y += warning_h + 12.0;

        if !engine.gpu.text.is_empty() {
            y += paragraph(
                scene,
                &format!("{} {}", t("processing.engine.your_card"), engine.gpu.text),
                x,
                y,
                width,
                13.0,
                if engine.gpu.warning { WARN } else { GOOD },
            ) + 8.0;
        }
        y += paragraph(
            scene,
            &format!(
                "{} {}. {}",
                t("processing.engine.download"),
                engine.download_size,
                t("processing.engine.space")
            ),
            x,
            y,
            width,
            13.0,
            MUTED,
        ) + 16.0;

        label(
            scene,
            t("processing.engine.directory"),
            row(x, y, width, 20.0),
            HAlign::Left,
            13.0,
            TEXT,
        );
        y += 24.0;
        let choose_w = 130.0;
        let path_box = row(x, y, width - choose_w - 8.0, 36.0);
        scene
            .quads
            .push(quad(path_box, [0.07, 0.075, 0.095, 1.0], BORDER, 7.0));
        let (path_text, path_color) = if engine.directory.is_empty() {
            (t("processing.engine.no_directory"), MUTED)
        } else {
            (engine.directory.as_str(), TEXT)
        };
        label(
            scene,
            path_text,
            row(
                path_box.x + 10.0,
                path_box.y,
                path_box.width - 20.0,
                path_box.height,
            ),
            HAlign::Left,
            13.0,
            path_color,
        );
        // Always reachable: when the folder cannot change or is missing, the
        // application says why.
        button(
            scene,
            "processing.engine.choose",
            t("processing.engine.choose"),
            row(x + width - choose_w, y, choose_w, 36.0),
            PANEL_ALT,
            true,
        );
        scene.set_name(
            "processing.engine.choose",
            format!(
                "{} : {}",
                t("processing.engine.directory"),
                t("processing.engine.choose")
            ),
        );
        scene.set_value("processing.engine.choose", path_text);
        y += 44.0;
        let note = if engine.directory_locked {
            t("processing.engine.directory_locked")
        } else {
            t("processing.engine.directory_note")
        };
        y += paragraph(scene, note, x, y, width, SMALL, MUTED) + 18.0;

        let install_w = 240.0;
        button(
            scene,
            "processing.runtime.install",
            t("processing.install_engine"),
            row(x + width - install_w, y, install_w, button_h),
            ACCENT,
            true,
        );
        button(
            scene,
            "processing.modal.close",
            t("processing.cancel"),
            row(x + width - install_w - 158.0, y, 150.0, button_h),
            PANEL_ALT,
            true,
        );
        y + button_h + 24.0
    }

    /// The catalog window. Returns the height of the card.
    fn catalog_window(&mut self, scene: &mut ProcessingScene, screen_h: f32) -> f32 {
        let x = CARD_PAD;
        let width = CATALOG_CARD_W - CARD_PAD * 2.0;
        self.window_title(scene, ProcessingModal::Catalog, width);
        let mut y = 62.0;
        y += paragraph(
            scene,
            t("processing.catalog.subtitle"),
            x,
            y,
            width,
            13.0,
            MUTED,
        ) + 10.0;

        if !self.view.engine.installed {
            let open_w = 220.0;
            let note_w = width - open_w - 12.0;
            let lines = wrap(t("processing.catalog.engine_missing"), note_w, 13.0);
            let block_h = (lines.len() as f32 * line_height(13.0)).max(36.0);
            paragraph(
                scene,
                t("processing.catalog.engine_missing"),
                x,
                y,
                note_w,
                13.0,
                WARN,
            );
            button(
                scene,
                "processing.engine.open",
                t("processing.engine.install_open"),
                row(x + width - open_w, y, open_w, 36.0),
                ACCENT,
                self.view.engine.supported,
            );
            y += block_h + 12.0;
        }

        let count = self.view.models.len();
        let footer = 40.0 + 24.0 + 14.0;
        let available = (screen_h - 48.0 - y - footer).max(CATALOG_ROW_H);
        let visible = ((available / CATALOG_ROW_H).floor() as usize).clamp(1, count.max(1));
        self.catalog_visible = visible;
        self.catalog_scroll = self.catalog_scroll.min(count.saturating_sub(visible));
        let window = self.catalog_scroll..self.catalog_scroll + visible;
        for index in 0..count {
            let shown = window.contains(&index);
            let part = self.catalog_row(index, row(x, y, width, CATALOG_ROW_H - 8.0));
            scene.absorb(part, shown);
            if shown {
                y += CATALOG_ROW_H;
            }
        }
        if count > visible {
            label(
                scene,
                t("processing.catalog.scroll"),
                row(x, y - 4.0, width, 18.0),
                HAlign::Center,
                SMALL,
                MUTED,
            );
            y += 16.0;
        }
        y += 6.0;
        button(
            scene,
            "processing.modal.close",
            t("processing.close"),
            row(x + width - 150.0, y, 150.0, 40.0),
            PANEL_ALT,
            true,
        );
        y + 40.0 + 24.0
    }

    fn catalog_row(&self, index: usize, rect: Rect) -> ProcessingScene {
        let mut scene = ProcessingScene::default();
        let model = &self.view.models[index];
        scene.quads.push(quad(rect, PANEL_ALT, BORDER, 9.0));
        let action_w = 220.0;
        let text_w = (rect.width - action_w - 36.0).max(0.0);
        label(
            &mut scene,
            &model.purpose,
            row(rect.x + 14.0, rect.y + 8.0, text_w, 22.0),
            HAlign::Left,
            15.0,
            TEXT,
        );
        label(
            &mut scene,
            &format!("{}, {}", model.title, model.size),
            row(rect.x + 14.0, rect.y + 31.0, text_w, 18.0),
            HAlign::Left,
            SMALL,
            MUTED,
        );
        let outputs = format!("{} {}", t("processing.catalog.outputs"), model.outputs);
        label(
            &mut scene,
            &outputs,
            row(rect.x + 14.0, rect.y + 51.0, text_w, 18.0),
            HAlign::Left,
            SMALL,
            MUTED,
        );
        let action = Rect {
            x: rect.x + rect.width - action_w - 12.0,
            y: rect.y + 19.0,
            width: action_w,
            height: 40.0,
        };
        // Spoken with the button: what the model is, which a sighted user
        // reads on the row.
        let described = format!(
            "{}, {}, {}. {outputs}",
            model.purpose, model.title, model.size
        );
        match &model.state {
            ModelState::Available => {
                let id = format!("processing.model.{index}.download");
                button(
                    &mut scene,
                    &id,
                    &format!("{} ({})", t("processing.catalog.download"), model.size),
                    action,
                    ACCENT,
                    self.view.engine.supported,
                );
                scene.set_name(
                    &id,
                    format!("{} : {described}", t("processing.catalog.download")),
                );
            }
            ModelState::Downloading { fraction, detail } => {
                let cancel_w = 92.0;
                let info = row(action.x, action.y, action.width - cancel_w - 8.0, 20.0);
                label(&mut scene, detail, info, HAlign::Left, SMALL, TEXT);
                progress_bar(
                    &mut scene,
                    row(info.x, action.y + 27.0, info.width, 10.0),
                    *fraction,
                );
                let id = format!("processing.model.{index}.cancel");
                button(
                    &mut scene,
                    &id,
                    t("processing.cancel"),
                    row(
                        action.x + action.width - cancel_w,
                        action.y,
                        cancel_w,
                        action.height,
                    ),
                    PANEL,
                    true,
                );
                scene.set_name(
                    &id,
                    format!("{} : {}", t("processing.cancel"), model.purpose),
                );
                scene.set_value(&id, detail.clone());
            }
            ModelState::Installed => {
                let remove_w = 110.0;
                label(
                    &mut scene,
                    t("processing.catalog.installed"),
                    row(
                        action.x,
                        action.y,
                        action.width - remove_w - 8.0,
                        action.height,
                    ),
                    HAlign::Left,
                    13.0,
                    GOOD,
                );
                let id = format!("processing.model.{index}.remove");
                self.confirm_button(
                    &mut scene,
                    &id,
                    t("processing.catalog.remove"),
                    row(
                        action.x + action.width - remove_w,
                        action.y,
                        remove_w,
                        action.height,
                    ),
                );
                scene.set_name(
                    &id,
                    format!("{} : {described}", t("processing.catalog.remove")),
                );
                scene.set_value(&id, t("processing.catalog.installed"));
            }
            ModelState::Removing => {
                label(
                    &mut scene,
                    t("processing.step.removing"),
                    action,
                    HAlign::Left,
                    SMALL,
                    MUTED,
                );
            }
        }
        scene
    }

    fn sidebar(&mut self, scene: &mut ProcessingScene, layout: ProcessingLayout) {
        let x = layout.sidebar.x + 14.0;
        let width = (layout.sidebar.width - 28.0).max(0.0);
        let mut y = layout.sidebar.y + 10.0;

        label(
            scene,
            t("processing.models.title"),
            row(x, y, width, 24.0),
            HAlign::Left,
            16.0,
            TEXT,
        );
        y += 30.0;
        let listed: Vec<usize> = (0..self.view.models.len())
            .filter(|index| self.view.models[*index].state != ModelState::Available)
            .collect();
        if listed.is_empty() {
            y += paragraph(
                scene,
                t("processing.models.empty"),
                x,
                y,
                width,
                SMALL,
                MUTED,
            ) + 10.0;
        }
        for index in listed {
            let part = self.model_row(index, row(x, y, width, MODEL_ROW_H - 4.0));
            scene.absorb(part, true);
            y += MODEL_ROW_H;
        }

        y += 4.0;
        label(
            scene,
            t("processing.items.title"),
            row(x, y, width, 24.0),
            HAlign::Left,
            16.0,
            TEXT,
        );
        y += 28.0;
        let footer_y = layout.sidebar.y + layout.sidebar.height - FOOTER_H - layout.panel_height;
        let visible = (((footer_y - y) / ITEM_ROW_H).floor().max(0.0)) as usize;
        self.items_visible = visible;
        let count = self.view.items.len();
        if count == 0 && visible > 0 {
            label(
                scene,
                t("processing.items.empty"),
                row(x, y, width, 20.0),
                HAlign::Left,
                SMALL,
                MUTED,
            );
        }
        self.item_scroll = self.item_scroll.min(count.saturating_sub(visible.max(1)));
        let window = self.item_scroll..self.item_scroll + visible;
        for index in 0..count {
            let shown = window.contains(&index);
            let part = self.item_row(index, row(x, y, width, ITEM_ROW_H - 4.0));
            scene.absorb(part, shown);
            if shown {
                y += ITEM_ROW_H;
            }
        }

        // Engine: one status line and the door to its window.
        let engine = &self.view.engine;
        let (status, color) = if !engine.supported {
            (t("processing.engine.unsupported"), WARN)
        } else if engine.task.is_some() && engine.installing {
            (t("processing.engine.installing"), TEXT)
        } else if engine.installed && engine.uses_gpu {
            (t("processing.engine.installed_gpu"), GOOD)
        } else if engine.installed {
            (t("processing.engine.installed_cpu"), WARN)
        } else {
            (t("processing.engine.missing"), MUTED)
        };
        label(
            scene,
            t("processing.engine.title"),
            row(x, footer_y + 2.0, width, 18.0),
            HAlign::Left,
            13.0,
            TEXT,
        );
        label(
            scene,
            status,
            row(x, footer_y + 21.0, width, 18.0),
            HAlign::Left,
            SMALL,
            color,
        );
        button(
            scene,
            "processing.engine.open",
            t("processing.engine.manage"),
            row(x, footer_y + 46.0, width, 30.0),
            PANEL_ALT,
            true,
        );
        scene.set_value("processing.engine.open", status);
        scene.set_shortcut("processing.engine.open", "processing.key.engine");
    }

    fn model_row(&self, index: usize, rect: Rect) -> ProcessingScene {
        let mut scene = ProcessingScene::default();
        let model = &self.view.models[index];
        let installed = model.state == ModelState::Installed;
        let selected = installed && index == self.view.selected_model;
        scene.quads.push(row_quad(rect, selected));
        let name = format!("{}, {}", model.purpose, model.title);
        if installed {
            scene.controls.push(SceneControl {
                id: format!("processing.model.{index}"),
                label: name.clone(),
                value: Some(t("processing.catalog.installed").to_string()),
                shortcut: Some("processing.key.model"),
                bounds: rect,
                role: AccessibleRole::ListItem,
                selected,
                visible: true,
            });
        }
        let text_w = (rect.width - 54.0).max(0.0);
        label(
            &mut scene,
            &model.purpose,
            row(rect.x + 10.0, rect.y + 5.0, text_w, 20.0),
            HAlign::Left,
            14.0,
            TEXT,
        );
        let side_button = Rect {
            x: rect.x + rect.width - 38.0,
            y: rect.y + 10.0,
            width: 30.0,
            height: rect.height - 20.0,
        };
        let detail = row(rect.x + 10.0, rect.y + 27.0, text_w, 18.0);
        match &model.state {
            ModelState::Installed => {
                label(
                    &mut scene,
                    &format!("✓ {}", model.title),
                    detail,
                    HAlign::Left,
                    SMALL,
                    GOOD,
                );
                self.cross_button(
                    &mut scene,
                    &format!("processing.model.{index}.remove"),
                    &format!("{} : {}", t("processing.catalog.remove"), model.purpose),
                    side_button,
                );
            }
            ModelState::Downloading {
                fraction,
                detail: text,
            } => {
                label(&mut scene, text, detail, HAlign::Left, SMALL, TEXT);
                progress_bar(
                    &mut scene,
                    row(rect.x + 10.0, rect.y + rect.height - 8.0, text_w, 5.0),
                    *fraction,
                );
                let id = format!("processing.model.{index}.cancel");
                button(&mut scene, &id, "×", side_button, PANEL, true);
                scene.set_name(&id, format!("{} : {name}", t("processing.cancel")));
                scene.set_value(&id, text.clone());
            }
            ModelState::Removing => {
                label(
                    &mut scene,
                    t("processing.step.removing"),
                    detail,
                    HAlign::Left,
                    SMALL,
                    MUTED,
                );
            }
            ModelState::Available => {}
        }
        scene
    }

    fn item_row(&self, index: usize, rect: Rect) -> ProcessingScene {
        let mut scene = ProcessingScene::default();
        let item = &self.view.items[index];
        let selected = self.view.active_item == Some(item.id);
        scene.quads.push(row_quad(rect, selected));
        scene.controls.push(SceneControl {
            id: format!("processing.item.{}", item.id),
            label: item.name.clone(),
            value: (item.duration_ms > 0)
                .then(|| spoken_duration(item.duration_ms as f64 / 1_000.0)),
            shortcut: None,
            bounds: rect,
            role: AccessibleRole::ListItem,
            selected,
            visible: true,
        });
        label(
            &mut scene,
            &item.name,
            row(rect.x + 10.0, rect.y + 2.0, rect.width - 52.0, 19.0),
            HAlign::Left,
            13.0,
            TEXT,
        );
        label(
            &mut scene,
            &format_duration(item.duration_ms),
            row(rect.x + 10.0, rect.y + 20.0, rect.width - 52.0, 17.0),
            HAlign::Left,
            SMALL,
            MUTED,
        );
        let remove = Rect {
            x: rect.x + rect.width - 36.0,
            y: rect.y,
            width: 36.0,
            height: rect.height,
        };
        label(&mut scene, "×", remove, HAlign::Center, 18.0, TEXT);
        scene.controls.push(SceneControl {
            id: format!("processing.item.{}.remove", item.id),
            label: format!("{} {}", t("processing.items.remove"), item.name),
            value: None,
            shortcut: None,
            bounds: remove,
            role: AccessibleRole::Button,
            selected: false,
            visible: true,
        });
        scene
    }

    fn header(&self, scene: &mut ProcessingScene, layout: ProcessingLayout) {
        let view = &self.view;
        let y = layout.header.y + 9.0;
        let height = layout.header.height - 18.0;
        let gap = 6.0;
        let left = layout.header.x + 10.0;
        let right = layout.header.x + layout.header.width - 10.0;
        let primary = self.primary_action();
        let primary_text = match &primary {
            Some((text, _)) => text.clone(),
            None => t("processing.engine.unsupported").to_string(),
        };
        // import, project video, catalog, quality, primary
        let widths = [150.0, 150.0, 180.0, 150.0, 280.0];
        let total: f32 = widths.iter().sum::<f32>() + gap * 4.0;
        let scale = ((right - left) / total).min(1.0);
        let supported = view.engine.supported;
        let primary_rect = row(right - widths[4] * scale, y, widths[4] * scale, height);
        let quality_rect = row(
            primary_rect.x - gap - widths[3] * scale,
            y,
            widths[3] * scale,
            height,
        );
        let catalog_rect = row(
            quality_rect.x - gap - widths[2] * scale,
            y,
            widths[2] * scale,
            height,
        );
        // The main button comes first in the `Tab` order: it always names the
        // next step.
        let fill = if view.job.is_some() { DANGER } else { ACCENT };
        button(
            scene,
            "processing.primary",
            &primary_text,
            primary_rect,
            fill,
            primary.is_some(),
        );
        button(
            scene,
            "processing.import",
            t("processing.import"),
            row(left, y, widths[0] * scale, height),
            PANEL_ALT,
            supported,
        );
        // Without a video the button stays reachable and says so.
        button(
            scene,
            "processing.project_video",
            t("processing.project_video"),
            row(left + widths[0] * scale + gap, y, widths[1] * scale, height),
            PANEL_ALT,
            supported,
        );
        button(
            scene,
            "processing.catalog.open",
            t("processing.catalog.open"),
            catalog_rect,
            PANEL_ALT,
            supported,
        );
        let quality = if view.best_quality {
            t("processing.quality.best")
        } else {
            t("processing.quality.fast")
        };
        button(
            scene,
            "processing.quality",
            quality,
            quality_rect,
            PANEL_ALT,
            true,
        );
        for (id, shortcut) in [
            ("processing.primary", "processing.key.run"),
            ("processing.import", "processing.key.import"),
            ("processing.project_video", "processing.key.project_video"),
            ("processing.catalog.open", "processing.key.catalog"),
            ("processing.quality", "processing.key.quality"),
        ] {
            scene.set_shortcut(id, shortcut);
        }
    }

    fn lanes(
        &self,
        scene: &mut ProcessingScene,
        waveforms: &mut Vec<(Rect, usize)>,
        current_ms: u64,
        layout: ProcessingLayout,
    ) {
        let view = &self.view;
        let area = layout.tracks;
        if view.tracks.is_empty() {
            let any_model = view
                .models
                .iter()
                .any(|model| model.state == ModelState::Installed);
            let text = if !view.engine.supported {
                t("processing.empty.unsupported")
            } else if !view.engine.installed {
                t("processing.empty.first_time")
            } else if !any_model {
                t("processing.empty.no_model")
            } else {
                t("processing.empty.ready")
            };
            label(scene, text, area, HAlign::Center, 18.0, MUTED);
            return;
        }

        let wave_span = waveform_span(layout);
        for tick in 0..=6 {
            let ms = self.view_start_ms + self.view_duration_ms * tick / 6;
            let x = wave_span.x + wave_span.width * tick as f32 / 6.0;
            label(
                scene,
                &format_duration(ms),
                row(x - 40.0, area.y, 80.0, RULER_H - 3.0),
                HAlign::Center,
                SMALL,
                MUTED,
            );
        }

        let lane_h = self.lane_height(layout);
        let top = area.y + RULER_H;
        let bottom = area.y + area.height;
        let scroll = self.lane_scroll.min(self.max_lane_scroll(layout));
        for index in 0..view.tracks.len() {
            let y = top + index as f32 * (lane_h + LANE_GAP) - scroll;
            // Whole lanes only: the renderer has no clipping rectangle here.
            let shown = y >= top - 0.5 && y + lane_h <= bottom + 0.5;
            let lane = row(area.x, y, area.width, lane_h);
            let lane = indented(lane, view.tracks[index].depth);
            let wave = Rect {
                x: wave_span.x,
                y: y + 6.0,
                width: wave_span.width,
                height: lane_h - 12.0,
            };
            let part = self.lane(index, lane, wave, current_ms);
            scene.absorb(part, shown);
            if shown {
                waveforms.push((wave, index));
            }
        }
    }

    fn lane(&self, index: usize, lane: Rect, wave: Rect, current_ms: u64) -> ProcessingScene {
        let mut scene = ProcessingScene::default();
        let view = &self.view;
        let track = &view.tracks[index];
        let listening = index == view.listening;
        scene
            .quads
            .push(row_quad_filled(lane, PANEL, listening, 8.0));
        // One bar per ancestor, joined to the lane above: a produced track
        // visibly sits in the scope of the track it comes from.
        let levels = track.depth.min(SCOPE_MAX);
        for level in 0..levels {
            scene.quads.push(quad(
                row(
                    lane.x - (levels - level) as f32 * SCOPE_STEP + 4.0,
                    lane.y - LANE_GAP,
                    3.0,
                    lane.height + LANE_GAP,
                ),
                SCOPE,
                [0.0; 4],
                1.5,
            ));
        }

        let info_x = lane.x + 10.0;
        let info_w = (wave.x - lane.x - 20.0).max(0.0);
        label(
            &mut scene,
            &track.title,
            row(info_x, lane.y + 6.0, info_w - 28.0, 20.0),
            HAlign::Left,
            14.0,
            TEXT,
        );
        label(
            &mut scene,
            &track.subtitle,
            row(info_x, lane.y + 26.0, info_w, 17.0),
            HAlign::Left,
            SMALL,
            MUTED,
        );
        let button_y = lane.y + lane.height - 34.0;
        // Listen, export, add to the project: three equal buttons.
        let small = (info_w - 12.0) / 3.0;
        let listen_text = if listening {
            t("processing.track.listening")
        } else {
            t("processing.track.listen")
        };
        let described = format!("{}, {}", track.title, track.subtitle);
        let listen_id = format!("processing.track.{index}.listen");
        button(
            &mut scene,
            &listen_id,
            listen_text,
            row(info_x, button_y, small, 28.0),
            if listening { SELECTED } else { PANEL_ALT },
            true,
        );
        if listening {
            // The button of the track being heard is framed too.
            if let Some(quad) = scene.quads.last_mut() {
                quad.border_color = SELECTED_BORDER;
                quad.border_width = 2.0;
            }
        }
        scene.set_name(
            &listen_id,
            format!("{} : {described}", t("processing.track.listen")),
        );
        scene.set_value(
            &listen_id,
            spoken_duration(track.duration_ms as f64 / 1_000.0),
        );
        if let Some(control) = scene.controls.last_mut() {
            control.selected = listening;
        }
        scene.set_shortcut(&listen_id, "processing.key.listen");
        if track.depth > 0 {
            let id = format!("processing.track.{index}.export");
            button(
                &mut scene,
                &id,
                t("processing.track.export"),
                row(info_x + small + 6.0, button_y, small, 28.0),
                PANEL_ALT,
                true,
            );
            scene.set_name(
                &id,
                format!("{} : {described}", t("processing.track.export")),
            );
            // The chords act on the listened track: only its buttons say them.
            if listening {
                scene.set_shortcut(&id, "processing.key.export");
            }
            let id = format!("processing.track.{index}.project");
            button(
                &mut scene,
                &id,
                t("processing.track.add_to_project"),
                row(info_x + (small + 6.0) * 2.0, button_y, small, 28.0),
                PANEL_ALT,
                true,
            );
            scene.set_name(
                &id,
                format!(
                    "{} : {described}",
                    t("processing.track.add_to_project_long")
                ),
            );
            if listening {
                scene.set_shortcut(&id, "processing.key.add_to_project");
            }
            // Each produced track goes on its own, with what descends from it.
            self.cross_button(
                &mut scene,
                &format!("processing.track.{index}.remove"),
                &format!("{} : {described}", t("processing.track.remove")),
                Rect {
                    x: wave.x - 34.0,
                    y: lane.y + 6.0,
                    width: 24.0,
                    height: 24.0,
                },
            );
        }
        self.waveform(
            &mut scene,
            track,
            wave,
            LANE_COLORS[index % LANE_COLORS.len()],
            current_ms,
        );
        scene
    }

    fn waveform(
        &self,
        scene: &mut ProcessingScene,
        track: &TrackView,
        wave: Rect,
        color: [f32; 4],
        current_ms: u64,
    ) {
        scene
            .quads
            .push(quad(wave, [0.035, 0.038, 0.05, 1.0], BORDER, 6.0));
        let center = wave.y + wave.height * 0.5;
        scene.quads.push(quad(
            Rect {
                x: wave.x + 1.0,
                y: center,
                width: (wave.width - 2.0).max(0.0),
                height: 1.0,
            },
            [0.28, 0.29, 0.34, 0.8],
            [0.0; 4],
            0.0,
        ));
        let view_end = self.view_start_ms.saturating_add(self.view_duration_ms);
        let peaks = &track.waveform.peaks;
        let peak_ms = f64::from(track.waveform.samples_per_peak.max(1)) * 1_000.0
            / f64::from(track.waveform_sample_rate.max(1));
        if !peaks.is_empty() && peak_ms > 0.0 {
            let first = ((self.view_start_ms as f64 / peak_ms).floor() as usize).min(peaks.len());
            let last = ((view_end as f64 / peak_ms).ceil() as usize).min(peaks.len());
            let columns = wave.width.max(1.0) as usize;
            let span = last.saturating_sub(first);
            for column in 0..columns {
                let a = first + span * column / columns;
                let b = (first + span * (column + 1) / columns)
                    .max(a + 1)
                    .min(peaks.len());
                if a >= b {
                    continue;
                }
                let peak = peaks[a..b].iter().copied().fold(0.0_f32, f32::max);
                let height = (peak.min(1.0) * (wave.height - 8.0) * 0.5).max(0.5);
                scene.quads.push(quad(
                    Rect {
                        x: wave.x + column as f32,
                        y: center - height,
                        width: 1.2,
                        height: height * 2.0,
                    },
                    color,
                    [0.0; 4],
                    0.0,
                ));
            }
        }
        if current_ms >= self.view_start_ms && current_ms <= view_end {
            let ratio =
                (current_ms - self.view_start_ms) as f32 / self.view_duration_ms.max(1) as f32;
            // White core in a red line: the playhead shows on every waveform
            // colour.
            let x = wave.x + wave.width * ratio.clamp(0.0, 1.0);
            scene.quads.push(quad(
                row(x - 2.0, wave.y + 1.0, 4.0, wave.height - 2.0),
                [0.95, 0.25, 0.30, 1.0],
                [0.0; 4],
                0.0,
            ));
            scene.quads.push(quad(
                row(x - 0.5, wave.y + 1.0, 1.0, wave.height - 2.0),
                [1.0, 1.0, 1.0, 1.0],
                [0.0; 4],
                0.0,
            ));
        }
    }

    fn status(&self, scene: &mut ProcessingScene, layout: ProcessingLayout) {
        let view = &self.view;
        let bar = layout.status;
        let x = bar.x + 12.0;
        // A model pass first; otherwise the engine being installed, so the
        // user sees it move with the engine window closed.
        let (progress, button_id, button_text) = match (&view.job, &view.engine.task) {
            (Some(job), _) => (job, "processing.cancel", t("processing.cancel")),
            (None, Some(task)) => (task, "processing.engine.open", t("processing.details")),
            (None, None) => {
                let has_results = view.tracks.iter().any(|track| track.depth > 0);
                let export_w = if has_results { 150.0 } else { 0.0 };
                label(
                    scene,
                    t("processing.hint"),
                    row(x, bar.y, (bar.width - 36.0 - export_w).max(0.0), bar.height),
                    HAlign::Left,
                    SMALL,
                    MUTED,
                );
                if has_results {
                    button(
                        scene,
                        "processing.export_all",
                        t("processing.export_all"),
                        row(
                            bar.x + bar.width - export_w - 12.0,
                            bar.y + 10.0,
                            export_w,
                            bar.height - 20.0,
                        ),
                        PANEL_ALT,
                        true,
                    );
                    scene.set_shortcut("processing.export_all", "processing.key.export_all");
                }
                return;
            }
        };
        let button_w = 110.0;
        let text_w = (bar.width - button_w - 48.0).max(0.0);
        let text = row(x, bar.y + 4.0, text_w, 22.0);
        label(
            scene,
            &progress.title,
            row(x, text.y, text_w * 0.45, 22.0),
            HAlign::Left,
            13.0,
            TEXT,
        );
        label(
            scene,
            &progress.detail,
            row(x + text_w * 0.45 + 8.0, text.y, text_w * 0.55 - 8.0, 22.0),
            HAlign::Right,
            SMALL,
            TEXT,
        );
        // Read on demand: where the work stands.
        scene.controls.push(SceneControl {
            id: "processing.status".into(),
            label: progress.title.clone(),
            value: Some(progress.detail.clone()),
            shortcut: None,
            bounds: text,
            role: AccessibleRole::Region,
            selected: false,
            visible: true,
        });
        progress_bar(scene, row(x, bar.y + 31.0, text_w, 10.0), progress.fraction);
        button(
            scene,
            button_id,
            button_text,
            row(
                bar.x + bar.width - button_w - 12.0,
                bar.y + 10.0,
                button_w,
                bar.height - 20.0,
            ),
            PANEL_ALT,
            true,
        );
        if button_id == "processing.cancel" {
            scene.set_shortcut(button_id, "processing.key.cancel");
        }
    }
}

/// State of a control and its keyboard shortcut, as spoken after its name:
/// « installé, raccourci Contrôle Maj flèche bas ».
pub fn spoken_state(control: &SceneControl) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(value) = control
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(value.to_owned());
    }
    if let Some(shortcut) = control.shortcut {
        parts.push(format!("{} {}", t("processing.shortcut"), t(shortcut)));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// What a screen reader says of a control: its name, its state, its keyboard
/// shortcut, whether it is selected.
pub fn spoken(control: &SceneControl) -> String {
    let mut text = control.label.clone();
    if let Some(state) = spoken_state(control) {
        text.push_str(", ");
        text.push_str(&state);
    }
    if control.selected {
        text.push_str(", ");
        text.push_str(t("accessibility.selected"));
    }
    text
}

/// `processing.track.3.export` → `("track", 3, "export")`. `None` for the
/// controls that are not part of a list.
fn split_id(id: &str) -> Option<(&str, usize, &str)> {
    let mut parts = id.strip_prefix("processing.")?.splitn(3, '.');
    let family = parts.next()?;
    let index = parts.next()?.parse().ok()?;
    Some((family, index, parts.next().unwrap_or("")))
}

fn is_destructive(action: &ProcessingAction) -> bool {
    matches!(
        action,
        ProcessingAction::UninstallRuntime
            | ProcessingAction::UninstallAll
            | ProcessingAction::RemoveModel(_)
            | ProcessingAction::RemoveTrack(_)
    )
}

/// A lane pushed right by its depth in the tree of tracks.
fn indented(lane: Rect, depth: usize) -> Rect {
    let indent = depth.min(SCOPE_MAX) as f32 * SCOPE_STEP;
    row(lane.x + indent, lane.y, lane.width - indent, lane.height)
}

/// Horizontal extent shared by every waveform lane.
fn waveform_span(layout: ProcessingLayout) -> Rect {
    Rect {
        x: layout.tracks.x + LANE_INFO_W,
        y: layout.tracks.y,
        width: (layout.tracks.width - LANE_INFO_W - 8.0).max(1.0),
        height: layout.tracks.height,
    }
}

fn row(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width: width.max(0.0),
        height: height.max(0.0),
    }
}

fn format_duration(ms: u64) -> String {
    let seconds = ms / 1_000;
    if seconds >= 3_600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3_600,
            seconds % 3_600 / 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn quad(rect: Rect, color: [f32; 4], border_color: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width: if border_color == [0.0; 4] { 0.0 } else { 1.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

/// Background of a list row. A selected one has a thick light border on top
/// of its colour.
fn row_quad(rect: Rect, selected: bool) -> QuadInstance {
    row_quad_filled(
        rect,
        if selected { SELECTED } else { PANEL_ALT },
        selected,
        7.0,
    )
}

fn row_quad_filled(rect: Rect, fill: [f32; 4], selected: bool, radius: f32) -> QuadInstance {
    if selected {
        QuadInstance {
            border_width: 2.5,
            ..quad(rect, fill, SELECTED_BORDER, radius)
        }
    } else {
        quad(rect, fill, BORDER, radius)
    }
}

fn label(
    scene: &mut ProcessingScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
    });
}

/// A clickable button. A disabled one is drawn dimmed and takes no click:
/// only what the platform cannot do at all is disabled. Everything else stays
/// reachable, and the application says why it cannot act.
fn button(
    scene: &mut ProcessingScene,
    id: &str,
    text: &str,
    bounds: Rect,
    fill: [f32; 4],
    enabled: bool,
) {
    let fill = if enabled {
        fill
    } else {
        [fill[0] * 0.6, fill[1] * 0.6, fill[2] * 0.6, fill[3]]
    };
    scene.quads.push(quad(bounds, fill, BORDER, 7.0));
    label(
        scene,
        text,
        Rect {
            x: bounds.x + 6.0,
            width: (bounds.width - 12.0).max(0.0),
            ..bounds
        },
        HAlign::Center,
        13.0,
        if enabled { TEXT } else { MUTED },
    );
    if enabled {
        scene.controls.push(SceneControl {
            id: id.to_owned(),
            label: text.to_owned(),
            value: None,
            shortcut: None,
            bounds,
            role: AccessibleRole::Button,
            selected: false,
            visible: true,
        });
    }
}

/// A progress bar. Without a known fraction, a short block travels along it.
fn progress_bar(scene: &mut ProcessingScene, track: Rect, fraction: Option<f32>) {
    let radius = track.height / 2.0;
    scene
        .quads
        .push(quad(track, [0.05, 0.05, 0.07, 1.0], BORDER, radius));
    let (offset, filled) = match fraction {
        Some(fraction) => (0.0, track.width * fraction.clamp(0.0, 1.0)),
        None => {
            let phase = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() % 1_600)
                .unwrap_or(0)) as f32
                / 1_600.0;
            let block = track.width * 0.18;
            ((track.width - block) * phase, block)
        }
    };
    scene.quads.push(quad(
        Rect {
            x: track.x + offset,
            width: filled,
            ..track
        },
        [0.62, 0.56, 1.0, 1.0],
        [0.0; 4],
        radius,
    ));
}

fn line_height(font_size: f32) -> f32 {
    (font_size * 1.4).ceil()
}

/// Draws `text` on as many lines as `width` needs and returns the height used.
fn paragraph(
    scene: &mut ProcessingScene,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    font_size: f32,
    color: [u8; 3],
) -> f32 {
    let lines = wrap(text, width, font_size);
    let line_height = line_height(font_size);
    for (index, line) in lines.iter().enumerate() {
        label(
            scene,
            line,
            row(x, y + index as f32 * line_height, width, line_height),
            HAlign::Left,
            font_size,
            color,
        );
    }
    lines.len() as f32 * line_height
}

/// Greedy word wrap on an estimated glyph width: labels have no wrapping.
/// Paths have no spaces, so an overlong word is cut to fit.
fn wrap(text: &str, width: f32, font_size: f32) -> Vec<String> {
    let max_chars = ((width / (font_size * 0.52)).floor() as usize).max(12);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while word.len() > max_chars {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(word.drain(..max_chars).collect());
        }
        let needed = current.chars().count() + word.len() + usize::from(!current.is_empty());
        if needed > max_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.extend(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn append_scene<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<crate::ui::primitives::LabelInfo<'a>>,
    scene: &'a ProcessingScene,
) {
    quads.extend_from_slice(&scene.quads);
    labels.extend(
        scene
            .labels
            .iter()
            .map(|label| crate::ui::primitives::LabelInfo {
                text: label.text.as_str(),
                bounds: label.bounds,
                h_align: label.h_align,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(label.font_size),
                color_override: Some(label.color),
                font_family_override: None,
            }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f32, f32) = (1_600.0, 900.0);

    fn layout() -> ProcessingLayout {
        layout_of(SCREEN.1)
    }

    fn layout_of(screen_h: f32) -> ProcessingLayout {
        ProcessingLayout::compute(Rect {
            x: 0.0,
            y: 68.0,
            width: SCREEN.0,
            height: screen_h - 68.0,
        })
    }

    fn track(depth: usize, duration_ms: u64) -> TrackView {
        TrackView {
            title: "Piste".into(),
            subtitle: "Modèle".into(),
            waveform: Arc::new(WaveformData {
                samples_per_peak: 480,
                peaks: vec![0.5; (duration_ms / 10) as usize],
            }),
            waveform_sample_rate: 48_000,
            duration_ms,
            depth,
        }
    }

    fn model(purpose: &str, state: ModelState) -> ModelView {
        ModelView {
            title: format!("{purpose} (modèle)"),
            purpose: purpose.into(),
            size: "913 Mo".into(),
            outputs: "Instrumental, Voix".into(),
            state,
        }
    }

    fn ready_view() -> ProcessingView {
        ProcessingView {
            engine: EngineView {
                supported: true,
                installed: true,
                uses_gpu: true,
                download_size: "≈ 3,5 Go".into(),
                directory: r"D:\IA\coquerythmo-processing".into(),
                directory_locked: true,
                ..EngineView::default()
            },
            models: vec![
                model("Retirer la voix", ModelState::Installed),
                model("Séparer", ModelState::Available),
                model(
                    "Débruiter",
                    ModelState::Downloading {
                        fraction: Some(0.4),
                        detail: "40 %".into(),
                    },
                ),
            ],
            items: vec![ItemView {
                id: 7,
                name: "scene.wav".into(),
                duration_ms: 60_000,
            }],
            active_item: Some(7),
            tracks: vec![track(0, 60_000), track(1, 60_000), track(1, 60_000)],
            ..ProcessingView::default()
        }
    }

    fn fresh_view() -> ProcessingView {
        let mut view = ready_view();
        view.engine.installed = false;
        view.engine.directory_locked = false;
        view.models = vec![model("Retirer la voix", ModelState::Available)];
        view.items.clear();
        view.active_item = None;
        view.tracks.clear();
        view
    }

    fn center(bounds: Rect) -> UiEvent {
        UiEvent::MousePress {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    }

    fn press(ui: &mut ProcessingWorkspaceUi, id: &str) -> EventResponse {
        let layout = layout();
        let scene = ui.scene(0, layout);
        let bounds = scene
            .controls
            .iter()
            .rev()
            .find(|control| control.id == id)
            .unwrap_or_else(|| panic!("control {id} missing"))
            .bounds;
        ui.handle_event(&center(bounds), layout)
    }

    fn press_in_modal(ui: &mut ProcessingWorkspaceUi, id: &str) -> EventResponse {
        let scene = ui
            .modal_scene(SCREEN.0, SCREEN.1)
            .expect("a window is open");
        let bounds = scene
            .controls
            .iter()
            .rev()
            .find(|control| control.id == id)
            .unwrap_or_else(|| panic!("control {id} missing in the window"))
            .bounds;
        ui.handle_modal_event(&center(bounds))
    }

    fn app(action: ProcessingAction) -> EventResponse {
        EventResponse::Action(UiAction::Processing(action))
    }

    fn asks_confirmation(response: &EventResponse) -> bool {
        matches!(
            response,
            EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Activation { label }))
                if label == t("processing.confirm")
        )
    }

    #[test]
    fn primary_button_walks_engine_then_model_then_audio_then_run() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = fresh_view();
        ui.set_view(view.clone());
        press(&mut ui, "processing.primary");
        assert_eq!(ui.modal(), Some(ProcessingModal::Engine));
        ui.close_modal();

        view.engine.installed = true;
        ui.set_view(view.clone());
        press(&mut ui, "processing.primary");
        assert_eq!(ui.modal(), Some(ProcessingModal::Catalog));
        ui.close_modal();

        view.models[0].state = ModelState::Installed;
        ui.set_view(view.clone());
        assert_eq!(
            press(&mut ui, "processing.primary"),
            app(ProcessingAction::ImportAudio)
        );

        let ready = ready_view();
        ui.set_view(ready.clone());
        assert_eq!(
            press(&mut ui, "processing.primary"),
            app(ProcessingAction::Run)
        );

        let mut running = ready;
        running.job = Some(ProgressView::default());
        ui.set_view(running);
        assert_eq!(
            press(&mut ui, "processing.primary"),
            app(ProcessingAction::Cancel)
        );
    }

    #[test]
    fn engine_window_shows_the_warning_and_asks_for_the_folder() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = fresh_view();
        view.engine.directory.clear();
        view.engine.gpu.text = "RTX 4060, 8 Go".into();
        ui.set_view(view);
        ui.open_modal(ProcessingModal::Engine);

        let scene = ui.modal_scene(SCREEN.0, SCREEN.1).unwrap();
        assert!(scene
            .labels
            .iter()
            .any(|label| label.color == WARN && !label.text.is_empty()));
        // Opening the window reads everything written in it.
        let announcement = ui.modal_announcement().unwrap();
        for text in [
            t("processing.engine.requirement"),
            "RTX 4060, 8 Go",
            t("processing.engine.no_directory"),
        ] {
            assert!(announcement.contains(text), "{text} is not announced");
        }
        assert_eq!(
            press_in_modal(&mut ui, "processing.engine.choose"),
            app(ProcessingAction::ChooseDirectory)
        );
        // Installing stays reachable without a folder: the application then
        // says what is missing.
        assert_eq!(
            press_in_modal(&mut ui, "processing.runtime.install"),
            app(ProcessingAction::InstallRuntime)
        );
        assert_eq!(ui.modal(), Some(ProcessingModal::Engine));
    }

    #[test]
    fn installed_engine_is_removed_after_a_spoken_confirmation() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        ui.open_modal(ProcessingModal::Engine);
        let first = press_in_modal(&mut ui, "processing.runtime.uninstall");
        assert!(asks_confirmation(&first));
        assert!(ui.is_waiting_confirmation());
        assert_eq!(
            press_in_modal(&mut ui, "processing.runtime.uninstall"),
            app(ProcessingAction::UninstallRuntime)
        );
    }

    #[test]
    fn catalog_downloads_cancels_and_removes_models() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        ui.open_modal(ProcessingModal::Catalog);
        assert_eq!(
            press_in_modal(&mut ui, "processing.model.1.download"),
            app(ProcessingAction::DownloadModel(1))
        );
        assert_eq!(
            press_in_modal(&mut ui, "processing.model.2.cancel"),
            app(ProcessingAction::CancelDownload(2))
        );
        let first = press_in_modal(&mut ui, "processing.model.0.remove");
        assert!(asks_confirmation(&first));
        assert_eq!(
            press_in_modal(&mut ui, "processing.model.0.remove"),
            app(ProcessingAction::RemoveModel(0))
        );
        assert_eq!(ui.modal(), Some(ProcessingModal::Catalog));
    }

    #[test]
    fn a_window_takes_the_keyboard_and_closes_on_escape_or_outside_click() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        ui.open_modal(ProcessingModal::Catalog);
        ui.modal_scene(SCREEN.0, SCREEN.1);
        // The first stop is the text of the window.
        assert_eq!(ui.modal_hits[0].id, "processing.modal.info");
        let focus = |response: EventResponse| match response {
            EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Focus {
                label,
                ..
            })) => label,
            other => panic!("not a focus announcement: {other:?}"),
        };
        // The button says what the model is and its state, not only « Supprimer ».
        let said = focus(ui.handle_modal_event(&UiEvent::FocusNext));
        assert_eq!(
            ui.modal_hits[ui.modal_focus].id,
            "processing.model.0.remove"
        );
        assert!(said.contains("Retirer la voix"));
        assert!(said.contains("Instrumental, Voix"));
        assert!(said.contains(t("processing.catalog.installed")));

        ui.handle_modal_event(&UiEvent::FocusNext);
        assert_eq!(
            ui.handle_modal_event(&UiEvent::Activate),
            app(ProcessingAction::DownloadModel(1))
        );
        ui.handle_modal_event(&UiEvent::FocusPrevious);
        ui.handle_modal_event(&UiEvent::FocusPrevious);
        assert_eq!(ui.modal_focus, 0);

        ui.handle_modal_event(&UiEvent::KeyInput {
            text: "\x1b".into(),
        });
        assert_eq!(ui.modal(), None);

        ui.open_modal(ProcessingModal::Engine);
        ui.modal_scene(SCREEN.0, SCREEN.1);
        ui.handle_modal_event(&UiEvent::MousePress { x: 4.0, y: 4.0 });
        assert_eq!(ui.modal(), None);
    }

    #[test]
    fn catalog_rows_off_screen_stay_reachable_and_scroll_into_view() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        view.models = (0..6)
            .map(|index| model(&format!("Modèle {index}"), ModelState::Available))
            .collect();
        ui.set_view(view);
        ui.open_modal(ProcessingModal::Catalog);
        // A small window: not every row fits.
        let scene = ui.modal_scene(SCREEN.0, 520.0).unwrap();
        let downloads: Vec<&SceneControl> = scene
            .controls
            .iter()
            .filter(|control| control.id.ends_with(".download"))
            .collect();
        assert_eq!(downloads.len(), 6);
        assert!(downloads.iter().any(|control| !control.visible));
        assert!(ui.catalog_visible < 6);

        // Tab to the last model: it is announced, then shown.
        for _ in 0..6 {
            ui.handle_modal_event(&UiEvent::FocusNext);
        }
        assert_eq!(
            ui.modal_hits[ui.modal_focus].id,
            "processing.model.5.download"
        );
        let scene = ui.modal_scene(SCREEN.0, 520.0).unwrap();
        let last = scene
            .controls
            .iter()
            .find(|control| control.id == "processing.model.5.download")
            .unwrap();
        assert!(last.visible);
        assert_eq!(
            ui.handle_modal_event(&UiEvent::Activate),
            app(ProcessingAction::DownloadModel(5))
        );
    }

    #[test]
    fn left_column_lists_installed_and_downloading_models_only() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        let scene = ui.scene(0, layout());
        let ids: Vec<&str> = scene
            .controls
            .iter()
            .map(|control| control.id.as_str())
            .collect();
        assert!(ids.contains(&"processing.model.0"));
        assert!(ids.contains(&"processing.model.0.remove"));
        assert!(ids.contains(&"processing.model.2.cancel"));
        // The model that is only in the catalog is not listed.
        assert!(!ids.iter().any(|id| id.starts_with("processing.model.1")));
        assert!(ids.contains(&"processing.catalog.open"));
        assert!(ids.contains(&"processing.engine.open"));
    }

    #[test]
    fn a_row_button_wins_over_the_row_it_sits_on() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        assert_eq!(
            press(&mut ui, "processing.model.0"),
            app(ProcessingAction::SelectModel(0))
        );
        assert_eq!(
            press(&mut ui, "processing.model.2.cancel"),
            app(ProcessingAction::CancelDownload(2))
        );
        assert_eq!(
            press(&mut ui, "processing.item.7.remove"),
            app(ProcessingAction::RemoveItem(7))
        );
    }

    #[test]
    fn clicking_a_lane_listens_to_it_at_that_time() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        let layout = layout();
        ui.scene(0, layout);
        let (wave, track) = ui.waveforms[1];
        let response = ui.handle_event(
            &UiEvent::MousePress {
                x: wave.x + wave.width / 2.0,
                y: wave.y + 4.0,
            },
            layout,
        );
        assert_eq!(
            response,
            app(ProcessingAction::Listen {
                track,
                at_ms: Some(30_000),
            })
        );
        assert_eq!(track, 1);
    }

    #[test]
    fn each_produced_track_has_its_own_remove_button() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        let scene = ui.scene(0, layout());
        let removes: Vec<&str> = scene
            .controls
            .iter()
            .filter(|control| control.id.starts_with("processing.track."))
            .filter(|control| control.id.ends_with(".remove"))
            .map(|control| control.id.as_str())
            .collect();
        // Instrumental and voice go separately; the original audio has none.
        assert_eq!(
            removes,
            ["processing.track.1.remove", "processing.track.2.remove"]
        );
        let first = press(&mut ui, "processing.track.2.remove");
        assert!(asks_confirmation(&first));
        assert_eq!(
            press(&mut ui, "processing.track.2.remove"),
            app(ProcessingAction::RemoveTrack(2))
        );
    }

    #[test]
    fn a_track_sits_in_the_scope_of_the_track_it_descends_from() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        // original > instrumental > (bass, drums), then voice
        view.tracks = vec![
            track(0, 60_000),
            track(1, 60_000),
            track(2, 60_000),
            track(2, 60_000),
            track(1, 60_000),
        ];
        ui.set_view(view);
        let scene = ui.scene(0, layout());
        let left = |index: usize| {
            scene
                .controls
                .iter()
                .find(|control| control.id == format!("processing.track.{index}.listen"))
                .unwrap()
                .bounds
                .x
        };
        // Deeper tracks are pushed right, siblings line up.
        assert!(left(1) > left(0));
        assert!(left(2) > left(1));
        assert_eq!(left(2), left(3));
        assert_eq!(left(4), left(1));
        // One bar per ancestor: 1 + 2 + 2 + 1.
        let bars = scene
            .quads
            .iter()
            .filter(|quad| quad.color == SCOPE)
            .count();
        assert_eq!(bars, 6);
        // Waveforms stay aligned whatever the depth.
        let waves: Vec<f32> = ui.waveforms.iter().map(|(rect, _)| rect.x).collect();
        assert!(waves.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn a_produced_track_can_be_added_to_the_project_but_not_the_source() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        let scene = ui.scene(0, layout());
        assert!(!scene
            .controls
            .iter()
            .any(|control| control.id == "processing.track.0.project"));
        assert_eq!(
            press(&mut ui, "processing.track.1.project"),
            app(ProcessingAction::AddTrackToProject(1))
        );
        // The three lane buttons sit side by side without overlapping.
        let scene = ui.scene(0, layout());
        let bounds = |suffix: &str| {
            scene
                .controls
                .iter()
                .find(|control| control.id == format!("processing.track.1.{suffix}"))
                .unwrap()
                .bounds
        };
        let (listen, export, project) = (bounds("listen"), bounds("export"), bounds("project"));
        assert!(listen.x + listen.width <= export.x);
        assert!(export.x + export.width <= project.x);
    }

    #[test]
    fn every_control_is_spoken_with_its_state() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        view.job = Some(ProgressView {
            title: "Retirer la voix 40 %".into(),
            detail: "reste 0:42".into(),
            fraction: Some(0.4),
        });
        ui.set_view(view);
        let scene = ui.scene(0, layout());
        let said = |id: &str| {
            spoken(
                scene
                    .controls
                    .iter()
                    .find(|control| control.id == id)
                    .unwrap_or_else(|| panic!("control {id} missing")),
            )
        };
        let model = said("processing.model.0");
        assert!(model.contains("Retirer la voix"));
        assert!(model.contains(t("processing.catalog.installed")));
        assert!(model.ends_with(t("accessibility.selected")));
        assert!(said("processing.model.2.cancel").contains("40 %"));
        assert!(said("processing.item.7").contains(&spoken_duration(60.0)));
        let listen = said("processing.track.0.listen");
        assert!(listen.contains("Piste"));
        assert!(listen.contains(&spoken_duration(60.0)));
        assert!(listen.ends_with(t("accessibility.selected")));
        // The progress of the work can be read on demand.
        assert_eq!(
            said("processing.status"),
            "Retirer la voix 40 %, reste 0:42"
        );
        assert!(said("processing.cancel").contains(t("processing.key.cancel")));
        assert!(said("processing.quality").contains(t("processing.quality.fast")));
        // Buttons say their keyboard shortcut after their state.
        assert!(said("processing.primary").ends_with(&format!(
            "{} {}",
            t("processing.shortcut"),
            t("processing.key.run")
        )));
        assert!(said("processing.import").contains(t("processing.key.import")));
        assert!(model.contains(t("processing.key.model")));
        assert!(listen.contains(t("processing.key.listen")));
        assert!(said("processing.status").starts_with("Retirer la voix 40 %"));
        // No text of the tab is smaller than the low-vision minimum.
        assert!(scene.labels.iter().all(|label| label.font_size >= SMALL));
    }

    #[test]
    fn tracks_and_audios_off_screen_stay_in_tab_order_and_come_back_on_focus() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        view.tracks = (0..8)
            .map(|index| track(usize::from(index > 0), 60_000))
            .collect();
        view.items = (1..=12)
            .map(|id| ItemView {
                id,
                name: format!("audio{id}.wav"),
                duration_ms: 1_000,
            })
            .collect();
        view.active_item = Some(1);
        ui.set_view(view);
        // A short window: neither the 8 lanes nor the 12 audios fit.
        let layout = layout_of(560.0);
        let scene = ui.scene(0, layout);
        let visible = |scene: &ProcessingScene, id: &str| {
            scene
                .controls
                .iter()
                .find(|control| control.id == id)
                .unwrap_or_else(|| panic!("control {id} is not in the Tab order"))
                .visible
        };
        assert!(!visible(&scene, "processing.track.7.listen"));
        assert!(!visible(&scene, "processing.item.12"));
        // A control out of view takes no click.
        assert!(!ui
            .hits
            .iter()
            .any(|(_, id)| id == "processing.track.7.listen"));

        ui.reveal("processing.track.7.listen", layout);
        ui.reveal("processing.item.12", layout);
        let scene = ui.scene(0, layout);
        assert!(visible(&scene, "processing.track.7.listen"));
        assert!(visible(&scene, "processing.item.12"));
        assert!(!visible(&scene, "processing.track.0.listen"));
    }

    #[test]
    fn arrows_move_between_tracks_models_and_audios() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        view.items.push(ItemView {
            id: 9,
            name: "autre.wav".into(),
            duration_ms: 1_000,
        });
        ui.set_view(view);
        ui.scene(0, layout());
        let down = |ui: &ProcessingWorkspaceUi, id: &str| ui.neighbour(id, true);
        let up = |ui: &ProcessingWorkspaceUi, id: &str| ui.neighbour(id, false);
        assert_eq!(
            down(&ui, "processing.track.0.listen").as_deref(),
            Some("processing.track.1.listen")
        );
        assert_eq!(
            down(&ui, "processing.track.1.export").as_deref(),
            Some("processing.track.2.export")
        );
        // The original audio has no « Exporter »: up lands on its « Écouter ».
        assert_eq!(
            up(&ui, "processing.track.1.export").as_deref(),
            Some("processing.track.0.listen")
        );
        assert_eq!(down(&ui, "processing.track.2.listen"), None);
        // An installed model and one being downloaded are the same list.
        assert_eq!(
            down(&ui, "processing.model.0").as_deref(),
            Some("processing.model.2.cancel")
        );
        assert_eq!(
            down(&ui, "processing.item.7").as_deref(),
            Some("processing.item.9")
        );
        assert_eq!(down(&ui, "processing.primary"), None);
    }

    #[test]
    fn delete_removes_the_focused_audio_model_or_result() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        ui.scene(0, layout());
        assert_eq!(
            ui.delete_action("processing.item.7"),
            Some(UiAction::Processing(ProcessingAction::RemoveItem(7)))
        );
        // A model and a result ask for the confirmation first.
        assert!(matches!(
            ui.delete_action("processing.model.0"),
            Some(UiAction::Accessibility(_))
        ));
        assert_eq!(
            ui.delete_action("processing.model.0"),
            Some(UiAction::Processing(ProcessingAction::RemoveModel(0)))
        );
        assert!(matches!(
            ui.delete_action("processing.track.2.listen"),
            Some(UiAction::Accessibility(_))
        ));
        assert_eq!(
            ui.delete_action("processing.track.2.listen"),
            Some(UiAction::Processing(ProcessingAction::RemoveTrack(2)))
        );
        // The original audio is not a result.
        assert_eq!(ui.delete_action("processing.track.0.listen"), None);
    }

    #[test]
    fn export_chords_are_said_on_the_listened_track_only() {
        let mut ui = ProcessingWorkspaceUi::default();
        let mut view = ready_view();
        view.listening = 1;
        ui.set_view(view);
        let scene = ui.scene(0, layout());
        let shortcut = |id: &str| {
            scene
                .controls
                .iter()
                .find(|control| control.id == id)
                .unwrap()
                .shortcut
        };
        assert_eq!(
            shortcut("processing.track.1.export"),
            Some("processing.key.export")
        );
        assert_eq!(
            shortcut("processing.track.1.project"),
            Some("processing.key.add_to_project")
        );
        assert_eq!(shortcut("processing.track.2.export"), None);
        assert_eq!(shortcut("processing.track.2.project"), None);
    }

    #[test]
    fn the_left_column_ends_above_the_shortcut_panel() {
        let mut ui = ProcessingWorkspaceUi::default();
        ui.set_view(ready_view());
        let bottom = |layout: ProcessingLayout, ui: &mut ProcessingWorkspaceUi| {
            let scene = ui.scene(0, layout);
            let engine = scene
                .controls
                .iter()
                .find(|control| control.id == "processing.engine.open")
                .unwrap()
                .bounds;
            engine.y + engine.height
        };
        let free = bottom(layout(), &mut ui);
        let with_panel = bottom(layout().above_panel(140.0), &mut ui);
        assert_eq!(free - with_panel, 140.0);
    }

    #[test]
    fn long_notes_and_paths_wrap_inside_their_column() {
        let lines = wrap(
            "Demande un PC puissant : carte NVIDIA RTX avec 8 Go de mémoire vidéo conseillée.",
            300.0,
            10.5,
        );
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|line| line.chars().count() <= 55));

        let path = format!(r"D:\{}\coquerythmo-processing", "dossier".repeat(20));
        let lines = wrap(&path, 300.0, 13.0);
        assert!(lines.len() >= 2);
        assert_eq!(lines.concat(), path);
    }
}
