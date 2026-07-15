//! Main application UI shell.
//!
//! A few handlers retain explicit context parameters while the workspace
//! migration is completed; the signatures keep event flow visible.
#![allow(clippy::too_many_arguments)]

pub use crate::application::command::ToolMode;
pub mod actor_icon_cache;
pub mod automation;
pub mod color_picker;
pub mod connect_modal;
pub mod context_menu;
pub mod dropdown;
pub mod export_modal;
pub mod file_explorer;
pub mod icon_button;
pub mod icons;
pub mod interactive;
pub mod language_modal;
pub mod layout;
pub mod license_badge;
pub mod modal_host;
pub mod pricing_license_modal;
pub mod pricing_page;
pub mod pricing_plan_modal;
pub mod primitives;
pub mod project_settings_modal;
pub mod proxy_error_modal;
pub mod proxy_modal;
pub mod rename_character_modal;
pub mod renderer;
pub mod save_prompt_modal;
pub mod server_browser;
pub mod settings_modal;
pub mod shell;
pub mod slider;
pub mod studio_warning_modal;
pub mod text_button;
pub mod text_input;
pub mod theme;
pub mod toast;
pub mod tooltip;
pub mod voice_actor_modal;
pub mod whats_new_modal;

use layout::{
    Layout, PROPS_DEFAULT_W, PROPS_DRAG_ZONE, PROPS_MAX_W, PROPS_MIN_W, RYTHMO_MIN_H, TOOLBAR_H,
    TOPBAR_H, VIDEO_MIN_H,
};
use primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign, Widget,
};
use renderer::StretchedText;
use tooltip::TooltipState;

use crate::i18n::t;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rendering::rythmo::scene::{FrameWindow, RythmoScene, SceneOptions};

use self::actor_icon_cache::ActorIconCache;
use self::icons::IconAtlas;
use self::modal_host::ModalHost;
use self::renderer::UiRenderer;
use crate::workspaces::rythmo::view as rythmo;

use theme::*;

pub struct Ui {
    topbar_widgets: Vec<Box<dyn Widget>>,
    toolbar_widgets: Vec<Box<dyn Widget>>,
    layout: Layout,
    screen_w: f32,
    screen_h: f32,
    props_visible: bool,
    props_width: f32,
    dragging_props: bool,
    /// Fraction of the free area given to the video preview (rest goes to bande rythmo).
    video_split: f32,
    dragging_split: Option<shell::SplitHandle>,
    tooltip: Option<TooltipState>,
    pub cursor_pos: (f32, f32),
    playing: bool,
    volume: f32,
    pub rythmo_state: rythmo::RythmoState,
    icon_uvs: std::collections::HashMap<String, [f32; 4]>,
    active_dropdown: Option<primitives::ToolbarDropdown>,
    pub export_progress: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    pub export_render_backend: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    export_label: String,
    pub progress_prefix: String,
    pub modal_host: ModalHost,
    actor_icon_cache: ActorIconCache,
    network_in_room: bool,
    pub network_status: String,
    network_room_label: String,
    pub has_video: bool,
    pub current_frame: i64,
    pub total_frames: i64,
    pub scrubbing: bool,
    pub sync_overlay: Option<String>,
    pub sync_progress: f32,
    pub active_mode: Option<ToolMode>,
    pub brush_color: [f32; 4],
    pub brush_radius_index: usize,
    pub erasing: bool,
    pub brush_picking: bool,
    pub(crate) drawing_overlay_cache: Option<DrawingOverlayCache>,
    pub brush_color_presets: [[f32; 4]; 8],
    pub brush_color_preset_index: usize,
    pub toasts: toast::ToastManager,
    /// Active bande rythmo import (label, start instant). Set by State while a
    /// background parse runs; the modal blocks input + shows a spinner.
    pub loading_project: Option<(String, std::time::Instant)>,
    automation_editor: automation::AutomationEditor,
}

impl Ui {
    pub fn new(screen_width: u32, screen_height: u32, icon_atlas: &IconAtlas) -> Self {
        let sw = screen_width as f32;
        let sh = screen_height as f32;
        let video_split = crate::config::video_split();
        let layout = Layout::compute(sw, sh, false, PROPS_DEFAULT_W, video_split);

        let icon_names = [
            "resume",
            "pause",
            "prev_frame",
            "next_frame",
            "boucle",
            "out",
            "scene",
            "respirations",
            "reactions",
            "liaison_left",
            "liaison_right",
            "settings",
            "project",
            "stretcher",
            "br-edit",
            "note",
            "karaoke",
            "sound",
            "mute",
            "select-mode",
            "draw-mode",
            "eraser",
        ];
        let icon_uvs: std::collections::HashMap<String, [f32; 4]> = icon_names
            .iter()
            .map(|&name| {
                (
                    name.to_string(),
                    icon_atlas.get_uv(name).unwrap_or([0.0; 4]),
                )
            })
            .collect();

        let settings_uv = icon_uvs.get("settings").copied().unwrap_or([0.0; 4]);
        let project_uv = icon_uvs.get("project").copied().unwrap_or([0.0; 4]);
        let mut ui = Self {
            topbar_widgets: shell::build_topbar(false, false, sw, settings_uv, project_uv),
            toolbar_widgets: vec![],
            layout,
            screen_w: sw,
            screen_h: sh,
            props_visible: false,
            props_width: PROPS_DEFAULT_W,
            dragging_props: false,
            video_split,
            dragging_split: None,
            tooltip: None,
            cursor_pos: (0.0, 0.0),
            playing: false,
            volume: 0.75,
            rythmo_state: rythmo::RythmoState::new(),
            icon_uvs,
            active_dropdown: None,
            export_progress: None,
            export_render_backend: None,
            export_label: String::new(),
            progress_prefix: String::new(),
            modal_host: ModalHost::new(),
            actor_icon_cache: ActorIconCache::new(),
            network_in_room: false,
            network_status: "".into(),
            network_room_label: String::new(),
            sync_overlay: None,
            sync_progress: 0.0,
            has_video: false,
            current_frame: 0,
            total_frames: 0,
            scrubbing: false,
            toasts: toast::ToastManager::new(),
            loading_project: None,
            automation_editor: automation::AutomationEditor::default(),
            active_mode: Some(ToolMode::Select),
            brush_color: [1.0, 1.0, 1.0, 1.0],
            brush_radius_index: 0,
            erasing: false,
            brush_picking: false,
            drawing_overlay_cache: None,
            // Color palette for drawing
            brush_color_presets: [
                [1.0, 1.0, 1.0, 1.0], // White
                [1.0, 0.3, 0.3, 1.0], // Red
                [0.3, 1.0, 0.3, 1.0], // Green
                [0.3, 0.5, 1.0, 1.0], // Blue
                [1.0, 1.0, 0.3, 1.0], // Yellow
                [1.0, 0.5, 0.2, 1.0], // Orange
                [0.8, 0.3, 1.0, 1.0], // Purple
                [0.2, 0.8, 0.8, 1.0], // Cyan
            ],
            brush_color_preset_index: 0,
        };
        ui.toolbar_widgets = shell::build_toolbar(ui.toolbar_build_context());
        ui
    }

    fn rebuild_layout(&mut self) {
        self.layout = Layout::compute(
            self.screen_w,
            self.screen_h,
            self.props_visible,
            self.props_width,
            self.video_split,
        );
        self.toolbar_widgets = shell::build_toolbar(self.toolbar_build_context());
    }

    pub fn rebuild_topbar(&mut self, in_room: bool) {
        self.network_in_room = in_room;
        self.topbar_widgets = shell::build_topbar(
            in_room,
            self.has_video,
            self.screen_w,
            self.uv("settings"),
            self.uv("project"),
        );
    }

    pub fn set_network_room_code(&mut self, code: Option<&str>) {
        self.network_room_label = code
            .map(|code| format!("Code salon : {code}"))
            .unwrap_or_default();
    }

    pub fn rebuild_toolbar(&mut self) {
        self.toolbar_widgets = shell::build_toolbar(self.toolbar_build_context());
    }

    fn toolbar_build_context(&self) -> shell::ToolbarBuildContext<'_> {
        shell::ToolbarBuildContext {
            layout: &self.layout,
            icon_uvs: &self.icon_uvs,
            playing: self.playing,
            volume: self.volume,
            active_mode: self.active_mode,
            brush_color: self.brush_color,
            brush_radius_index: self.brush_radius_index,
            brush_color_preset_index: self.brush_color_preset_index,
            erasing: self.erasing,
            brush_color_presets: &self.brush_color_presets,
            ctrl_held: self.rythmo_state.ctrl_held,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &Project,
        render_index: &ProjectRenderIndex,
        render_frame: f64,
        fps: f64,
    ) -> EventResponse {
        if let UiEvent::MouseMove { x, y } = event {
            self.cursor_pos = (*x, *y);
        }

        // Project loading modal blocks all input while a BR is being parsed.
        if self.loading_project.is_some() {
            return EventResponse::Consumed;
        }

        // Export/proxy workers may read media extracted from a portable
        // project. Keep the document immutable until they finish; Escape is a
        // deliberate cancellation path handled by the worker.
        if self.export_progress.is_some() {
            if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
                return EventResponse::Action(UiAction::CancelExport);
            }
            return EventResponse::Consumed;
        }

        // ModalHost owns modal priority, lifecycle and command conversion.
        if let Some(outcome) =
            self.modal_host
                .handle_topmost_event(event, self.screen_w, self.screen_h)
        {
            return outcome.into_event_response();
        }

        // Toast click to dismiss
        if self
            .toasts
            .handle_event(event, self.screen_w, self.screen_h)
        {
            return EventResponse::Consumed;
        }

        // Sync overlay blocks all input
        if self.sync_overlay.is_some() {
            return EventResponse::Consumed;
        }

        if let Some(outcome) = self
            .modal_host
            .handle_event(event, self.screen_w, self.screen_h)
        {
            return outcome.into_event_response();
        }

        if self.rythmo_state.context_menu.is_some() || matches!(event, UiEvent::ContextMenu { .. })
        {
            let response = rythmo::handle_context_menu_event(
                event,
                project,
                render_frame,
                &self.layout.rythmo,
                self.screen_w,
                self.screen_h,
                &mut self.rythmo_state,
            );
            if response != EventResponse::Ignored {
                return response;
            }
        }

        // Toolbar dropdown overlay
        if self.active_dropdown.is_some() {
            if let UiEvent::MousePress { x, y } = event {
                let resp = self.handle_dropdown_click(*x, *y);
                if resp != EventResponse::Ignored {
                    return resp;
                }
            }
        }

        if let Some(response) = self.handle_split_drag(event) {
            return response;
        }

        if let Some(response) = self.handle_props_drag(event) {
            return response;
        }

        // Toolbar widgets must receive clicks before seek scrubbing because they live in the same bar.
        if let UiEvent::MousePress { x, y } = event {
            for widget in self.toolbar_widgets.iter() {
                if widget.bounds().contains(*x, *y)
                    && widget
                        .tooltip()
                        .is_some_and(|tip| tip == t("toolbar.mute") || tip == t("toolbar.unmute"))
                {
                    return EventResponse::Action(UiAction::ToggleMute);
                }
            }
        }
        for widget in self
            .topbar_widgets
            .iter_mut()
            .chain(self.toolbar_widgets.iter_mut())
        {
            if widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        if let Some(response) = self.automation_editor.handle_event(
            event,
            &self.layout.video_preview,
            &project.settings().automation,
            project,
        ) {
            return response;
        }
        for widget in self
            .topbar_widgets
            .iter_mut()
            .chain(self.toolbar_widgets.iter_mut())
        {
            if !widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        // Intercept UI actions for tool mode / brush settings (handled locally in Ui)
        // Check if any widget returned an action we handle locally
        // We need to check the responses from the widget loops above
        // Since we returned early on any non-Ignored, we handle them here by re-checking
        // Actually, the widget loops already return the action. We need to handle specific actions here.
        // Let's re-process by checking if the event was a click on our toolbar buttons
        // But the action has already bubbled up. Instead, we handle in State/main.rs.
        // For SetToolMode, CycleBrushSize, ToggleEraser, OpenBrushColorPicker, we handle in State.

        // Handle brush color picker sync
        if self.brush_picking {
            // Handle color picker events when picking brush color
            if self.rythmo_state.color_picker.handle_event(event) {
                if !self.rythmo_state.color_picker.active {
                    self.brush_picking = false;
                } else {
                    self.brush_color = self.rythmo_state.color_picker.current_color();
                }
                return EventResponse::Consumed;
            }
            // If picker closed without selection
            if !self.rythmo_state.color_picker.active {
                self.brush_picking = false;
            }
        }

        // Rythmo zone events (lines, scroll, ctrl+click, etc.)
        if self.total_frames > 0 {
            let hit = shell::progress_bar_hit_rect(&self.layout);
            match event {
                UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                    if hit.contains(*x, *y) =>
                {
                    self.scrubbing = true;
                    let t = ((*x - hit.x) / hit.width).clamp(0.0, 1.0);
                    let frame = (t * self.total_frames as f32) as i64;
                    return EventResponse::Action(UiAction::SeekAbsolute(frame));
                }
                UiEvent::MouseMove { x, .. } if self.scrubbing => {
                    let t = ((*x - hit.x) / hit.width).clamp(0.0, 1.0);
                    let frame = (t * self.total_frames as f32) as i64;
                    return EventResponse::Action(UiAction::SeekAbsolute(frame));
                }
                UiEvent::MouseRelease { .. } if self.scrubbing => {
                    self.scrubbing = false;
                    return EventResponse::Action(UiAction::FinishSeek);
                }
                _ => {}
            }
        }

        // Rythmo zone events (lines, scroll, ctrl+click, etc.)
        let brush_radius_frac = match self.brush_radius_index {
            0 => 0.006,
            1 => 0.012,
            2 => 0.024,
            _ => 0.012,
        };
        let rythmo_response = rythmo::handle_rythmo_event(
            event,
            &self.layout.rythmo,
            project,
            render_index,
            render_frame,
            self.playing,
            fps,
            &mut self.rythmo_state,
            self.active_mode.unwrap_or(ToolMode::Select),
            self.brush_color,
            brush_radius_frac,
            self.erasing,
        );
        if rythmo_response != EventResponse::Ignored {
            return rythmo_response;
        }

        // Scroll in rythmo zone
        if let UiEvent::Scroll {
            x,
            y,
            delta,
            fast,
            ctrl,
        } = event
        {
            if self.layout.rythmo.contains(*x, *y) {
                if *ctrl && *fast {
                    // CTRL+SHIFT+scroll: jump between boucle markers
                    let direction: i32 = if *delta > 0.0 { 1 } else { -1 };
                    return EventResponse::Action(UiAction::SeekToNextBoucle { direction });
                }
                let multiplier = if *fast { 60.0 } else { 15.0 };
                let frames = shell::scroll_delta_to_frames(*delta, multiplier);
                if frames != 0 {
                    return EventResponse::Action(UiAction::SeekRelative(frames));
                }
            }
        }

        self.update_tooltip();
        EventResponse::Ignored
    }

    fn handle_props_drag(&mut self, event: &UiEvent) -> Option<EventResponse> {
        if !self.props_visible {
            return None;
        }
        match event {
            UiEvent::MousePress { x, y } => {
                if let Some(props) = &self.layout.properties {
                    let drag_zone = Rect {
                        x: props.x - PROPS_DRAG_ZONE,
                        y: props.y,
                        width: PROPS_DRAG_ZONE * 2.0,
                        height: props.height,
                    };
                    if drag_zone.contains(*x, *y) {
                        self.dragging_props = true;
                        return Some(EventResponse::Consumed);
                    }
                }
                None
            }
            UiEvent::MouseMove { x, .. } => {
                if self.dragging_props {
                    self.props_width = (self.screen_w - x).clamp(PROPS_MIN_W, PROPS_MAX_W);
                    self.rebuild_layout();
                    return Some(EventResponse::Consumed);
                }
                None
            }
            UiEvent::MouseRelease { .. } => {
                if self.dragging_props {
                    self.dragging_props = false;
                    return Some(EventResponse::Consumed);
                }
                None
            }
            _ => None,
        }
    }

    fn handle_split_drag(&mut self, event: &UiEvent) -> Option<EventResponse> {
        let content_h = self.screen_h - TOPBAR_H;
        let free_h = (content_h - TOOLBAR_H).max(0.0);
        match event {
            UiEvent::MousePress { x, y } => {
                if self.layout.video_split_handle_rect().contains(*x, *y) {
                    self.dragging_split = Some(shell::SplitHandle::Video);
                    return Some(EventResponse::Consumed);
                }
                if self.layout.rythmo_split_handle_rect().contains(*x, *y) {
                    self.dragging_split = Some(shell::SplitHandle::Rythmo);
                    return Some(EventResponse::Consumed);
                }
                None
            }
            UiEvent::MouseMove { y, .. } => {
                if let Some(handle) = self.dragging_split {
                    let requested = match handle {
                        shell::SplitHandle::Video => (*y) - TOPBAR_H,
                        shell::SplitHandle::Rythmo => free_h - (self.screen_h - *y),
                    };
                    let min_video = VIDEO_MIN_H.min(free_h);
                    let max_video = (free_h - RYTHMO_MIN_H).max(min_video);
                    let video_h = requested.clamp(min_video, max_video);
                    let split = if free_h > 0.0 {
                        (video_h / free_h).clamp(0.0, 1.0)
                    } else {
                        self.video_split
                    };
                    self.video_split = split;
                    self.rebuild_layout();
                    return Some(EventResponse::Consumed);
                }
                None
            }
            UiEvent::MouseRelease { .. } => {
                if self.dragging_split.is_some() {
                    self.dragging_split = None;
                    crate::config::set_video_split(self.video_split);
                    return Some(EventResponse::Consumed);
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn hovering_split_handle(&self) -> bool {
        let (cx, cy) = self.cursor_pos;
        self.layout.video_split_handle_rect().contains(cx, cy)
            || self.layout.rythmo_split_handle_rect().contains(cx, cy)
    }

    pub(crate) fn dragging_split_handle(&self) -> bool {
        self.dragging_split.is_some()
    }

    pub fn open_automation(&mut self) {
        self.rythmo_state.stop_line_editing();
        self.rythmo_state.stop_char_editing();
        self.rythmo_state.stop_note_editing();
        self.rythmo_state.selected = None;
        self.rythmo_state.context_menu = None;
        self.automation_editor.open();
        self.tooltip = None;
    }

    pub fn close_automation(&mut self) {
        self.automation_editor.close();
    }

    pub fn automation_open(&self) -> bool {
        self.automation_editor.is_open()
    }

    pub fn take_selected_automation_node(&mut self) -> Option<u64> {
        self.automation_editor.take_selected_node_for_deletion()
    }

    fn update_tooltip(&mut self) {
        let (cx, cy) = self.cursor_pos;
        for widget in self
            .topbar_widgets
            .iter()
            .chain(self.toolbar_widgets.iter())
        {
            if widget.bounds().contains(cx, cy) {
                if let Some(text) = widget.tooltip() {
                    self.tooltip = Some(TooltipState {
                        text: text.to_string(),
                        cursor_x: cx,
                        cursor_y: cy,
                    });
                    return;
                }
            }
        }
        self.tooltip = None;
    }

    fn uv(&self, name: &str) -> [f32; 4] {
        self.icon_uvs.get(name).copied().unwrap_or([0.0; 4])
    }

    pub fn toggle_play_pause(&mut self) {
        self.playing = !self.playing;
        self.toolbar_widgets = shell::build_toolbar(self.toolbar_build_context());
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn has_active_progress(&self) -> bool {
        self.export_progress.is_some()
    }

    pub fn needs_animation_or_interaction(&self) -> bool {
        self.playing
            || self.dragging_props
            || self.dragging_split.is_some()
            || self.scrubbing
            || self.toasts.has_active()
            || self.rythmo_state.needs_animation_or_interaction()
    }

    pub fn needs_background_poll(&self) -> bool {
        self.modal_host.server_browser.is_some()
            || self
                .modal_host
                .file_explorer
                .as_ref()
                .is_some_and(|modal| modal.needs_background_poll())
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        let mut deadline = self.rythmo_state.next_cursor_blink_deadline();
        if let Some(modal_deadline) = self
            .modal_host
            .file_explorer
            .as_ref()
            .and_then(|modal| modal.next_cursor_blink_deadline())
        {
            deadline = Some(deadline.map_or(modal_deadline, |current| current.min(modal_deadline)));
        }
        if let Some(modal_deadline) = self
            .modal_host
            .rename_character
            .as_ref()
            .and_then(|modal| modal.next_cursor_blink_deadline())
        {
            deadline = Some(deadline.map_or(modal_deadline, |current| current.min(modal_deadline)));
        }
        if let Some(modal_deadline) = self
            .modal_host
            .pricing_license
            .as_ref()
            .map(|modal| modal.next_cursor_blink_deadline())
        {
            deadline = Some(deadline.map_or(modal_deadline, |current| current.min(modal_deadline)));
        }
        deadline
    }

    pub fn toggle_toolbar_dropdown(&mut self, dd: primitives::ToolbarDropdown) {
        if self.active_dropdown == Some(dd.clone()) {
            self.active_dropdown = None;
        } else {
            self.active_dropdown = Some(dd);
        }
    }

    fn dropdown_items(dd: &primitives::ToolbarDropdown) -> Vec<(&'static str, &'static str)> {
        match dd {
            primitives::ToolbarDropdown::Respirations => vec![
                ("↑", "resp.up"),
                ("↓", "resp.down"),
                ("(H)", "resp.h"),
                ("(HH)", "resp.hh"),
                ("(mH)", "resp.mh"),
                ("(mHH)", "resp.mhh"),
            ],
            primitives::ToolbarDropdown::Reactions => vec![
                ("(X)", "react.x"),
                ("(mts)", "react.mts"),
                ("(tsc)", "react.tsc"),
                ("(ah)", "react.ah"),
                ("(oh)", "react.oh"),
                ("(ih)", "react.ih"),
                ("(mhm)", "react.mhm"),
                ("(hm)", "react.hm"),
                ("(ptt)", "react.ptt"),
                ("(pff)", "react.pff"),
                ("(unh)", "react.unh"),
                ("(hun)", "react.hun"),
                ("(psst)", "react.psst"),
            ],
        }
    }

    fn handle_dropdown_click(&mut self, x: f32, y: f32) -> EventResponse {
        let dd = match &self.active_dropdown {
            Some(dd) => dd.clone(),
            None => return EventResponse::Ignored,
        };
        let items = Self::dropdown_items(&dd);
        let dropdown_rect = self.toolbar_dropdown_rect(&dd, items.len());
        if !dropdown_rect.contains(x, y) {
            self.active_dropdown = None;
            return EventResponse::Consumed;
        }
        let item_h = 26.0;
        let idx = ((y - dropdown_rect.y) / item_h) as usize;
        if let Some((text, _)) = items.get(idx) {
            self.active_dropdown = None;
            return EventResponse::Action(UiAction::AddQuickLine {
                text: text.to_string(),
            });
        }
        EventResponse::Consumed
    }

    fn toolbar_dropdown_rect(&self, dd: &primitives::ToolbarDropdown, count: usize) -> Rect {
        let items = Self::dropdown_items(dd);
        let _ = items; // use count param
        let item_h = 26.0;
        let w = 220.0;
        let h = count as f32 * item_h;
        // Position below the button that opened it
        let btn_index = match dd {
            primitives::ToolbarDropdown::Respirations => 6, // 7th button (0-indexed)
            primitives::ToolbarDropdown::Reactions => 7,
        };
        let btn_x = self.layout.toolbar.x + 8.0 + btn_index as f32 * (TOOLBAR_BTN_SIZE + 4.0)
            + if btn_index >= 3 { 8.0 } else { 0.0 }  // separator after transport
            + if btn_index >= 6 { 8.0 } else { 0.0 }; // separator after markers
        Rect {
            x: btn_x,
            y: self.layout.toolbar.y - h - 2.0,
            width: w,
            height: h,
        }
    }

    fn render_toolbar_dropdown(
        &self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'_>>,
    ) {
        let dd = match &self.active_dropdown {
            Some(dd) => dd,
            None => return,
        };
        let items = Self::dropdown_items(dd);
        let rect = self.toolbar_dropdown_rect(dd, items.len());
        let item_h = 26.0;

        // Background
        quads.push(QuadInstance {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: DROPDOWN_PANEL_TOP,
            color_bottom: DROPDOWN_PANEL_BOT,
            border_color: DROPDOWN_PANEL_BORDER,
            border_width: 1.0,
            border_radius: 4.0,
            shadow_offset: [0.0, -2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.4],
            shadow_blur: 8.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        let mut iy = rect.y;
        for (text, tooltip_key) in &items {
            // Item label
            labels.push(LabelInfo {
                text,
                bounds: Rect {
                    x: rect.x + 8.0,
                    y: iy,
                    width: rect.width - 16.0,
                    height: item_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(13.0),
                color_override: None,
                font_family_override: None,
            });
            // Tooltip text on the right
            labels.push(LabelInfo {
                text: t(tooltip_key),
                bounds: Rect {
                    x: rect.x + 40.0,
                    y: iy,
                    width: rect.width - 48.0,
                    height: item_h,
                },
                h_align: HAlign::Right,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(9.0),
                color_override: Some([150, 150, 160]),
                font_family_override: None,
            });
            iy += item_h;
        }
    }

    pub fn is_editing_text(&self) -> bool {
        self.rythmo_state.is_editing() || self.modal_host.is_editing_text()
    }

    pub fn open_export_modal(
        &mut self,
        video_width: u32,
        video_height: u32,
        languages: Vec<export_modal::ExportLanguageOption>,
        configuration: crate::project::ExportConfiguration,
    ) {
        self.modal_host
            .open_export(video_width, video_height, languages, configuration);
    }

    pub fn open_languages_modal(
        &mut self,
        languages: Vec<language_modal::LanguageListItem>,
        active_language_id: u64,
    ) {
        self.modal_host
            .open_languages(languages, active_language_id);
    }

    pub fn refresh_languages_modal(
        &mut self,
        languages: Vec<language_modal::LanguageListItem>,
        active_language_id: u64,
    ) {
        self.modal_host
            .refresh_languages(languages, active_language_id);
    }

    pub fn open_file_explorer(&mut self, request: file_explorer::FileExplorerRequest) {
        self.modal_host.open_file_explorer(request);
    }

    pub fn poll_file_explorer(&mut self) -> bool {
        self.modal_host.poll_file_explorer()
    }

    pub fn open_voice_actor_modal(&mut self) {
        self.modal_host.open_voice_actor();
    }

    pub fn open_rename_character_modal(&mut self, characters: Vec<String>) {
        self.modal_host.open_rename_character(characters);
    }

    pub fn set_voice_actor_modal_icon_path(&mut self, path: impl Into<String>) {
        self.modal_host.set_voice_actor_icon_path(path);
    }

    pub fn open_proxy_modal(&mut self, video_width: u32, video_height: u32) {
        self.modal_host.open_proxy(video_width, video_height);
    }

    pub fn open_proxy_error_modal(&mut self, detail: impl Into<String>) {
        self.modal_host.open_proxy_error(detail);
    }

    pub fn open_whats_new_modal(&mut self, version: impl Into<String>, body: impl Into<String>) {
        self.modal_host.open_whats_new(version, body);
    }

    pub fn open_save_prompt(&mut self) {
        self.modal_host.open_save_prompt();
    }

    pub fn open_studio_warning(&mut self) {
        self.modal_host.open_studio_warning();
    }

    pub fn open_pricing_page(&mut self) {
        self.modal_host.open_pricing_page();
    }

    pub fn close_pricing_page(&mut self) {
        self.modal_host.close_pricing_page();
    }

    pub fn open_server_browser(&mut self) {
        self.modal_host.open_server_browser();
    }

    pub fn open_add_server_modal(&mut self) {
        self.modal_host.open_add_server();
    }

    pub fn server_browser_mut(&mut self) -> Option<&mut server_browser::ServerBrowserModal> {
        self.modal_host.server_browser_mut()
    }

    pub fn open_connect_modal(&mut self, ip: &str, port: u16, join: bool) {
        self.modal_host.open_connect(ip, port, join);
    }

    pub fn open_settings_modal(&mut self, fonts: Vec<String>) {
        self.modal_host.open_settings(fonts);
    }

    pub fn open_project_settings_modal(&mut self, instrumental_audio_path: Option<String>) {
        self.modal_host
            .open_project_settings(instrumental_audio_path);
    }

    pub fn set_project_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.modal_host.set_project_instrumental_audio_path(path);
    }

    pub fn close_project_settings_modal(&mut self) {
        self.modal_host.close_project_settings();
    }

    pub fn close_settings_modal(&mut self) {
        self.modal_host.close_settings();
    }

    pub fn rythmo_state(&self) -> &rythmo::RythmoState {
        &self.rythmo_state
    }

    pub fn clear_selection(&mut self) {
        self.rythmo_state.selected = None;
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol;
        self.rebuild_toolbar();
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn screen_w(&self) -> f32 {
        self.screen_w
    }
    pub fn screen_h(&self) -> f32 {
        self.screen_h
    }

    pub fn resize(&mut self, screen_width: u32, screen_height: u32) {
        self.screen_w = screen_width as f32;
        self.screen_h = screen_height as f32;
        self.topbar_widgets = shell::build_topbar(
            self.network_in_room,
            self.has_video,
            self.screen_w,
            self.uv("settings"),
            self.uv("project"),
        );
        self.rebuild_layout();
    }

    pub fn render(
        &mut self,
        renderer: &mut UiRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        ui_scale: f32,
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        project: &Project,
        render_index: &ProjectRenderIndex,
        current_frame: i64,
        render_frame: f64,
        fps: f64,
        waveform: &[f32],
        waveform_offset_frames: i64,
        waveform_is_instrumental: bool,
    ) {
        // Update frame info for progress bar
        self.current_frame = current_frame;

        // Tick toasts (needs &mut self, before labels borrow self)
        self.toasts.tick();

        // Prepare color picker textures first (needs &mut self, before labels borrow self)
        self.rythmo_state.color_picker.ensure_textures(
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );
        self.actor_icon_cache.sync(
            project,
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );

        // Update drawing overlay texture if needed
        self.update_drawing_overlay(device, queue, renderer, project, render_frame, fps);

        let mut color_picker_bg_quads: Vec<QuadInstance> = Vec::new();
        let mut base_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        let mut extra_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        let mut color_picker_fg_quads: Vec<QuadInstance> = Vec::new();

        // Update export label BEFORE borrowing self via labels
        if let Some(progress_atomic) = &self.export_progress {
            use std::sync::atomic::Ordering;
            let progress = f32::from_bits(progress_atomic.load(Ordering::Relaxed));
            let pct = (progress.clamp(0.0, 1.0) * 100.0) as u32;
            let prefix = if self.progress_prefix.is_empty() {
                self.export_render_backend
                    .as_ref()
                    .map(|status| status.load(Ordering::Relaxed))
                    .and_then(|status| match status {
                        crate::video_export::EXPORT_RENDER_BACKEND_GPU => {
                            Some(crate::i18n::t("progress.export_gpu"))
                        }
                        crate::video_export::EXPORT_RENDER_BACKEND_CPU => {
                            Some(crate::i18n::t("progress.export_cpu"))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| crate::i18n::t("progress.exporting"))
            } else {
                &self.progress_prefix
            };
            self.export_label = format!("{} {}%", prefix, pct);
        }
        // Consume pending cursor click if any before starting to collect labels referencing self
        let pending_click = self.rythmo_state.pending_cursor_click.take();

        let mut quads = Vec::new(); // base layer (behind video)
        let mut overlay_quads = Vec::new(); // overlay layer (on top of video)
        let mut icons: Vec<IconInstance> = Vec::new();
        let mut labels: Vec<LabelInfo> = Vec::new();
        let mut modal_quads: Vec<QuadInstance> = Vec::new(); // modal backgrounds (above normal text)
        let mut modal_labels: Vec<LabelInfo> = Vec::new(); // modal text (above modal backgrounds)

        // Pricing / support page replaces the entire layout while active.
        if self.modal_host.pricing_page.is_some() {
            // Page content is the "normal" layer; modals render into the modal
            // layer so their backgrounds and text always sit above the page text.
            self.modal_host.render_pricing(
                &mut quads,
                &mut overlay_quads,
                &mut labels,
                &mut modal_quads,
                &mut modal_labels,
                self.screen_w,
                self.screen_h,
            );
            // Toasts (e.g. confirmation after subscribing / activating)
            self.toasts.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
            let stretched_quads: Vec<(IconInstance, u64)> = Vec::new();
            let syllable_quads: Vec<QuadInstance> = Vec::new();
            let base_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
            let extra_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
            let color_picker_fg_quads: Vec<QuadInstance> = Vec::new();
            renderer.render(
                device,
                queue,
                encoder,
                view,
                screen_width,
                screen_height,
                ui_scale,
                &quads,
                &overlay_quads,
                &icons,
                &labels,
                None,
                &stretched_quads,
                &syllable_quads,
                &base_textured,
                &extra_textured,
                &color_picker_fg_quads,
                &modal_quads,
                &modal_labels,
            );
            return;
        }

        // We can't mutate self after borrowing labels. So process click before ANY render stuff borrowing self.
        if let Some((ratio, is_shift)) = pending_click {
            if let Some(line_id) = self.rythmo_state.editing_line {
                let segmented_idx = project.get_line(line_id).and_then(|line| {
                    let lang = crate::config::get().lang.clone();
                    rythmo::cursor_segments_for_line(
                        line,
                        self.rythmo_state.syllable_drag.as_ref(),
                        &lang,
                        self.playing,
                        &self.rythmo_state,
                    )
                    .and_then(|segments| renderer.cursor_pos_from_segments(&segments, ratio))
                    .or_else(|| {
                        rythmo::segmented_cursor_index_for_line_at_ratio(
                            line,
                            self.rythmo_state.syllable_drag.as_ref(),
                            &lang,
                            self.playing,
                            &self.rythmo_state,
                            ratio,
                        )
                    })
                });
                if let Some(closest_idx) =
                    segmented_idx.or_else(|| renderer.cursor_pos_from_x_ratio(line_id, ratio))
                {
                    if is_shift {
                        self.rythmo_state.line_input.update_selection(closest_idx);
                    } else {
                        self.rythmo_state.line_input.set_cursor_pos(closest_idx);
                    }
                }
            }
        }

        // Zone backgrounds
        self.render_zones(
            &mut quads,
            &mut labels,
            project,
            render_frame,
            render_index,
            fps,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
        );

        self.automation_editor.render(
            &self.layout.video_preview,
            &project.settings().automation,
            project,
            self.cursor_pos,
            &mut quads,
            &mut labels,
        );

        // Rythmo lines
        let mut stretched_texts: Vec<StretchedText> = Vec::new();
        let mut syllable_quads: Vec<QuadInstance> = Vec::new();
        let mut note_icons: Vec<IconInstance> = Vec::new();
        let mut actor_icon_draws: Vec<rythmo::VoiceActorIconDraw> = Vec::new();
        let note_uv = self.uv("note");
        let cursor_info = rythmo::render_lines(
            &self.layout.rythmo,
            project,
            render_index,
            render_frame,
            self.playing,
            fps,
            &self.rythmo_state,
            &mut quads,
            &mut syllable_quads,
            &mut labels,
            &mut stretched_texts,
            &mut note_icons,
            &mut actor_icon_draws,
            note_uv,
        );
        icons.extend(note_icons);
        for draw in actor_icon_draws {
            if let Some(actor) = project.find_voice_actor(&draw.actor_name) {
                if let Some(bind_group) = self.actor_icon_cache.bind_group_for(actor) {
                    base_textured.push((
                        IconInstance {
                            rect: [draw.rect.x, draw.rect.y, draw.rect.width, draw.rect.height],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [1.0, 1.0, 1.0, 1.0],
                        },
                        bind_group,
                    ));
                }
            }
        }

        // Drawing overlay
        if let Some(cache) = &self.drawing_overlay_cache {
            let zone = &self.layout.rythmo;
            base_textured.push((
                IconInstance {
                    rect: [zone.x, zone.y, zone.width, zone.height],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                },
                &cache.bind_group,
            ));
        }

        // Selection overlay (marquee + selected-strokes bbox & handles).
        // Drawn into overlay_quads so it composites above the drawing overlay.
        {
            let zone = &self.layout.rythmo;
            rythmo::render_selection_overlay(
                zone,
                render_frame,
                project,
                &self.rythmo_state,
                &mut overlay_quads,
            );
        }

        // Eraser cursor ring (visible like the pencil preview)
        if self.erasing && self.active_mode == Some(ToolMode::Draw) {
            let zone = &self.layout.rythmo;
            let (cx, cy) = self.cursor_pos;
            if zone.contains(cx, cy) {
                let brush_radius_frac = match self.brush_radius_index {
                    0 => 0.006,
                    1 => 0.012,
                    2 => 0.024,
                    _ => 0.012,
                };
                let r_px = (brush_radius_frac * zone.height).max(2.0);
                quads.push(QuadInstance {
                    rect: [cx - r_px, cy - r_px, r_px * 2.0, r_px * 2.0],
                    color: [0.0, 0.0, 0.0, 0.0],
                    color_bottom: [0.0, 0.0, 0.0, 0.0],
                    border_color: [0.95, 0.95, 0.98, 0.95],
                    border_width: 1.5,
                    border_radius: r_px,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
        }

        // Markers
        let mut liaison_icons: Vec<IconInstance> = Vec::new();
        rythmo::render_markers(
            &self.layout.rythmo,
            project,
            render_index,
            render_frame,
            &mut quads,
            &mut labels,
            &mut liaison_icons,
            self.uv("liaison_left"),
            self.uv("liaison_right"),
        );
        icons.extend(liaison_icons);

        // Prepare stretched text textures
        let stretched_quads = renderer.prepare_stretched_texts(
            device,
            queue,
            ui_scale,
            &stretched_texts,
            self.playing,
        );

        // Render cursor and selection using real glyph positions from the renderer cache
        if let Some((line_id, cursor_pos, selection, text_x, text_w, ry, rh, cursor_segments)) =
            cursor_info
        {
            let margin = rh * 0.25;
            let cursor_ratio = |pos: usize| {
                cursor_segments
                    .as_ref()
                    .and_then(|segments| {
                        segments
                            .iter()
                            .find(|segment| pos >= segment.start_char && pos <= segment.end_char)
                            .map(|segment| {
                                let local_pos = pos.saturating_sub(segment.start_char);
                                let local_ratio =
                                    renderer.cursor_x_ratio(segment.cache_id, local_pos);
                                (segment.start_ratio + local_ratio * segment.width_ratio)
                                    .clamp(0.0, 1.0)
                            })
                    })
                    .unwrap_or_else(|| renderer.cursor_x_ratio(line_id, pos))
            };

            // Draw selection highlight if any (blueish rect)
            if let Some((start_idx, end_idx)) = selection {
                let start_ratio = cursor_ratio(start_idx);
                let end_ratio = cursor_ratio(end_idx);
                let sx = text_x + start_ratio * text_w;
                let sw = (end_ratio - start_ratio) * text_w;
                if sw > 0.0 {
                    quads.push(QuadInstance {
                        rect: [sx, ry, sw, rh],
                        color: [0.2, 0.4, 0.8, 0.5],
                        color_bottom: [0.2, 0.4, 0.8, 0.5],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 2.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                }
            }

            // Draw blinking cursor (only if it should be visible based on timer, handled by rythmo)
            if self.rythmo_state.line_input.cursor_visible() {
                let ratio = cursor_ratio(cursor_pos);
                let cx = text_x + ratio * text_w;
                quads.push(QuadInstance {
                    rect: [cx, ry + margin, 1.5, rh - margin * 2.0],
                    color: [0.9, 0.9, 0.95, 1.0],
                    color_bottom: [0.9, 0.9, 0.95, 1.0],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
        }

        // Non-capturing widgets
        for widget in self
            .topbar_widgets
            .iter()
            .chain(self.toolbar_widgets.iter())
        {
            if !widget.captures_all() {
                quads.extend(widget.render_quads());
                icons.extend(widget.render_icons());
                labels.extend(widget.labels());
            }
        }

        // Capturing widgets → overlay (on top of video)
        for widget in self
            .topbar_widgets
            .iter()
            .chain(self.toolbar_widgets.iter())
        {
            if widget.captures_all() {
                overlay_quads.extend(widget.render_quads());
                icons.extend(widget.render_icons());
                labels.extend(widget.labels());
            }
        }

        // Autocomplete dropdown (on top of all lines)
        rythmo::render_autocomplete(
            &self.layout.rythmo,
            project,
            render_frame,
            &self.rythmo_state,
            &mut quads,
            &mut labels,
        );

        self.rythmo_state.color_picker.render(
            &mut color_picker_bg_quads,
            &mut extra_textured,
            &mut color_picker_fg_quads,
        );

        // Color picker quads → overlay
        overlay_quads.extend(color_picker_bg_quads);

        // Toolbar dropdown → overlay
        self.render_toolbar_dropdown(&mut overlay_quads, &mut labels);

        rythmo::render_context_menu(
            project,
            self.screen_w,
            self.screen_h,
            &self.rythmo_state,
            &mut overlay_quads,
            &mut labels,
        );

        // Tooltip → overlay
        if let Some(tooltip) = &self.tooltip {
            overlay_quads.extend(tooltip.render_quads(self.screen_w));
            labels.extend(tooltip.render_labels(self.screen_w));
        }

        // Sync overlay (blocks UI during video transfer)
        if let Some(msg) = &self.sync_overlay {
            let dw = 420.0;
            let dh = 100.0;
            let dx = (self.screen_w - dw) / 2.0;
            let dy = (self.screen_h - dh) / 2.0;

            // Dim
            overlay_quads.push(QuadInstance {
                rect: [0.0, 0.0, self.screen_w, self.screen_h],
                color: [0.0, 0.0, 0.0, 0.85],
                color_bottom: [0.0, 0.0, 0.0, 0.85],
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
                rect: [dx, dy, dw, dh],
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
            // Label
            labels.push(LabelInfo {
                text: msg,
                bounds: Rect {
                    x: dx,
                    y: dy + 16.0,
                    width: dw,
                    height: 28.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(16.0),
                color_override: Some([200, 200, 220]),
                font_family_override: None,
            });
            // Progress bar track
            let bx = dx + 30.0;
            let by = dy + 58.0;
            let bw = dw - 60.0;
            let bh = 14.0;
            overlay_quads.push(QuadInstance {
                rect: [bx, by, bw, bh],
                color: [0.10, 0.10, 0.13, 1.0],
                color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.8],
                border_width: 1.0,
                border_radius: 7.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            // Progress bar fill
            let fill = (bw - 4.0) * self.sync_progress.clamp(0.0, 1.0);
            if fill > 0.5 {
                overlay_quads.push(QuadInstance {
                    rect: [bx + 2.0, by + 2.0, fill, bh - 4.0],
                    color: [0.35, 0.60, 1.0, 1.0],
                    color_bottom: [0.25, 0.45, 0.85, 1.0],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 5.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
        }

        self.modal_host.render_base(
            &mut modal_quads,
            &mut modal_labels,
            self.screen_w,
            self.screen_h,
        );

        // Bande rythmo import loading modal (on top while a background parse runs)
        if let Some((label, started)) = &self.loading_project {
            let dw = 420.0;
            let dh = 130.0;
            let dx = (self.screen_w - dw) / 2.0;
            let dy = (self.screen_h - dh) / 2.0;

            // Dim
            overlay_quads.push(QuadInstance {
                rect: [0.0, 0.0, self.screen_w, self.screen_h],
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
                rect: [dx, dy, dw, dh],
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
                text: t("loading_project.title"),
                bounds: Rect {
                    x: dx,
                    y: dy + 22.0,
                    width: dw,
                    height: 26.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(17.0),
                color_override: None,
                font_family_override: None,
            });
            // File name
            labels.push(LabelInfo {
                text: label.as_str(),
                bounds: Rect {
                    x: dx + 20.0,
                    y: dy + 52.0,
                    width: dw - 40.0,
                    height: 18.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([180, 180, 190]),
                font_family_override: None,
            });
            // Indeterminate sliding bar
            let bx = dx + 30.0;
            let by = dy + 88.0;
            let bw = dw - 60.0;
            let bh = 10.0;
            overlay_quads.push(QuadInstance {
                rect: [bx, by, bw, bh],
                color: [0.10, 0.10, 0.13, 1.0],
                color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.8],
                border_width: 1.0,
                border_radius: 5.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            let track = bw - 4.0;
            let fill_w = track * 0.35;
            let p = (started.elapsed().as_secs_f32() * 1.2).sin() * 0.5 + 0.5;
            let fill_x = bx + 2.0 + (track - fill_w) * p;
            overlay_quads.push(QuadInstance {
                rect: [fill_x, by + 2.0, fill_w, bh - 4.0],
                color: [0.35, 0.60, 1.0, 1.0],
                color_bottom: [0.25, 0.45, 0.85, 1.0],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 3.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Export progress modal (rendered last so it's always on top)
        let export_progress_val = self
            .export_progress
            .as_ref()
            .map(|p| f32::from_bits(p.load(std::sync::atomic::Ordering::Relaxed)))
            .unwrap_or(0.0);
        if self.export_progress.is_some() && export_progress_val > 0.0 {
            let progress = export_progress_val;

            let dw = 420.0;
            let dh = 120.0;
            let dx = (self.screen_w - dw) / 2.0;
            let dy = (self.screen_h - dh) / 2.0;

            // Dim
            overlay_quads.push(QuadInstance {
                rect: [0.0, 0.0, self.screen_w, self.screen_h],
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
                rect: [dx, dy, dw, dh],
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
            // Bar track
            let bx = dx + 30.0;
            let by = dy + 65.0;
            let bw = dw - 60.0;
            let bh = 14.0;
            overlay_quads.push(QuadInstance {
                rect: [bx, by, bw, bh],
                color: [0.10, 0.10, 0.13, 1.0],
                color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.8],
                border_width: 1.0,
                border_radius: 7.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            // Bar fill
            let fill = (bw - 4.0) * progress.clamp(0.0, 1.0);
            if fill > 0.5 {
                overlay_quads.push(QuadInstance {
                    rect: [bx + 2.0, by + 2.0, fill, bh - 4.0],
                    color: [0.35, 0.60, 1.0, 1.0],
                    color_bottom: [0.25, 0.45, 0.85, 1.0],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 5.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
            // Labels
            labels.push(LabelInfo {
                text: &self.export_label,
                bounds: Rect {
                    x: dx,
                    y: dy + 18.0,
                    width: dw,
                    height: 28.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(17.0),
                color_override: None,
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: crate::i18n::t("progress.cancel_hint"),
                bounds: Rect {
                    x: dx,
                    y: dy + 86.0,
                    width: dw,
                    height: 20.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([160, 164, 176]),
                font_family_override: None,
            });
        }

        // Toasts
        self.toasts.render(
            &mut overlay_quads,
            &mut labels,
            self.screen_w,
            self.screen_h,
        );

        self.modal_host.render_top(
            &mut labels,
            &mut modal_quads,
            &mut modal_labels,
            self.screen_w,
            self.screen_h,
        );

        renderer.render(
            device,
            queue,
            encoder,
            view,
            screen_width,
            screen_height,
            ui_scale,
            &quads,
            &overlay_quads,
            &icons,
            &labels,
            video_quad,
            &stretched_quads,
            &syllable_quads,
            &base_textured,
            &extra_textured,
            &color_picker_fg_quads,
            &modal_quads,
            &modal_labels,
        );
    }

    fn render_zones<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        project: &Project,
        render_frame: f64,
        render_index: &ProjectRenderIndex,
        fps: f64,
        waveform: &[f32],
        waveform_offset_frames: i64,
        waveform_is_instrumental: bool,
    ) {
        let l = &self.layout;

        // Topbar
        quads.push(QuadInstance {
            rect: [l.topbar.x, l.topbar.y, l.topbar.width, l.topbar.height],
            color: TOPBAR_BG,
            color_bottom: TOPBAR_BG,
            border_color: TOPBAR_SHADOW,
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0, 1.0],
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            shadow_blur: 4.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        let status_text = self.network_status.trim();
        let room_text = self.network_room_label.trim();
        if !status_text.is_empty() || !room_text.is_empty() {
            let left = 388.0;
            let right = self.screen_w - 42.0;
            let available = right - left;
            if available >= 140.0 {
                let y = 0.0;
                let h = TOPBAR_HEIGHT;
                let dot_color =
                    if status_text.starts_with("Erreur") || status_text.starts_with("Échec") {
                        [0.90, 0.28, 0.28, 1.0]
                    } else if status_text == "Connexion..." {
                        [0.95, 0.68, 0.30, 1.0]
                    } else if self.network_in_room {
                        [0.38, 0.78, 0.48, 1.0]
                    } else {
                        [0.46, 0.48, 0.55, 1.0]
                    };

                let has_status = !status_text.is_empty();
                let has_room = !room_text.is_empty();
                let status_w = if has_room {
                    (available * 0.42).clamp(92.0, 190.0).min(available - 96.0)
                } else {
                    available.min(240.0)
                };
                let status_x = if has_room { left } else { right - status_w };

                if has_status {
                    quads.push(QuadInstance {
                        rect: [status_x + 4.0, 13.0, 6.0, 6.0],
                        color: dot_color,
                        color_bottom: dot_color,
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 3.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                    labels.push(LabelInfo {
                        text: status_text,
                        bounds: Rect {
                            x: status_x + 14.0,
                            y,
                            width: status_w - 14.0,
                            height: h,
                        },
                        h_align: HAlign::Left,
                        v_align: VAlign::Center,
                        overflow: Overflow::Ellipsis,
                        padding: 0.0,
                        font_size_override: Some(11.0),
                        color_override: Some([165, 168, 178]),
                        font_family_override: None,
                    });
                }

                if has_room {
                    let room_x = if has_status {
                        status_x + status_w + 12.0
                    } else {
                        left
                    };
                    let room_w = (right - room_x).max(80.0);
                    if has_status && room_w >= 90.0 {
                        quads.push(QuadInstance {
                            rect: [room_x - 7.0, 8.0, 1.0, 16.0],
                            color: [0.28, 0.28, 0.34, 0.9],
                            color_bottom: [0.28, 0.28, 0.34, 0.9],
                            border_color: [0.0; 4],
                            border_width: 0.0,
                            border_radius: 0.0,
                            shadow_offset: [0.0; 2],
                            shadow_color: [0.0; 4],
                            shadow_blur: 0.0,
                            rotation: 0.0,
                            _padding: [0.0; 2],
                        });
                    }
                    labels.push(LabelInfo {
                        text: room_text,
                        bounds: Rect {
                            x: room_x,
                            y,
                            width: room_w,
                            height: h,
                        },
                        h_align: HAlign::Left,
                        v_align: VAlign::Center,
                        overflow: Overflow::Ellipsis,
                        padding: 0.0,
                        font_size_override: Some(11.0),
                        color_override: Some([210, 212, 222]),
                        font_family_override: None,
                    });
                }
            }
        }

        // Video preview
        quads.push(QuadInstance {
            rect: [
                l.video_preview.x,
                l.video_preview.y,
                l.video_preview.width,
                l.video_preview.height,
            ],
            color: VIDEO_BG,
            color_bottom: VIDEO_BG,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Toolbar
        quads.push(QuadInstance {
            rect: [l.toolbar.x, l.toolbar.y, l.toolbar.width, l.toolbar.height],
            color: TOOLBAR_BG,
            color_bottom: TOOLBAR_BG,
            border_color: TOOLBAR_BORDER,
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Progress bar (in toolbar)
        if self.total_frames > 0 {
            let pb = shell::progress_bar_rect(&self.layout);
            let progress = (self.current_frame as f32 / self.total_frames as f32).clamp(0.0, 1.0);
            // Track
            quads.push(QuadInstance {
                rect: [pb.x, pb.y, pb.width, pb.height],
                color: [0.10, 0.10, 0.13, 1.0],
                color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.25, 0.25, 0.30, 0.5],
                border_width: 0.5,
                border_radius: 3.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            // Fill
            let fill_w = (pb.width - 2.0) * progress;
            if fill_w > 1.0 {
                quads.push(QuadInstance {
                    rect: [pb.x + 1.0, pb.y + 1.0, fill_w, pb.height - 2.0],
                    color: [0.35, 0.45, 0.85, 0.9],
                    color_bottom: [0.25, 0.35, 0.70, 0.9],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 2.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
            // Knob
            let knob_r = 5.0;
            let knob_x = pb.x + fill_w + 1.0;
            let knob_y = pb.y + pb.height / 2.0 - knob_r;
            quads.push(QuadInstance {
                rect: [knob_x - knob_r, knob_y, knob_r * 2.0, knob_r * 2.0],
                color: [0.85, 0.85, 0.92, 1.0],
                color_bottom: [0.70, 0.70, 0.78, 1.0],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: knob_r,
                shadow_offset: [0.0, 1.0],
                shadow_color: [0.0, 0.0, 0.0, 0.3],
                shadow_blur: 3.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Bande rythmo — fond noir + perforations + playhead
        quads.push(QuadInstance {
            rect: [l.rythmo.x, l.rythmo.y, l.rythmo.width, l.rythmo.height],
            color: [0.02, 0.02, 0.03, 1.0],
            color_bottom: [0.02, 0.02, 0.03, 1.0],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        let ppf = crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed();
        let visible_frames = (l.rythmo.width / ppf.max(0.001)) as i64 + 4;
        let scene_center = render_frame.clamp(i64::MIN as f64, i64::MAX as f64) as i64;
        let scene = RythmoScene::build(
            project,
            render_index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: scene_center.saturating_sub(visible_frames / 2 + 2),
                    last: scene_center.saturating_add(visible_frames / 2 + 2),
                },
                current_frame: render_frame,
                source_fps: fps,
                ..SceneOptions::default()
            },
        );
        quads.extend(rythmo::render_rythmo_base(
            &l.rythmo,
            project,
            render_frame,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
            self.playing,
            fps,
            &self.rythmo_state,
            &scene,
        ));

        // Resize handles between video/toolbar and toolbar/rythmo.
        let (hover_video, hover_rythmo) = (
            self.layout
                .video_split_handle_rect()
                .contains(self.cursor_pos.0, self.cursor_pos.1),
            self.layout
                .rythmo_split_handle_rect()
                .contains(self.cursor_pos.0, self.cursor_pos.1),
        );
        let handles: [(Rect, bool); 2] = [
            (
                l.video_split_handle_rect(),
                self.dragging_split == Some(shell::SplitHandle::Video) || hover_video,
            ),
            (
                l.rythmo_split_handle_rect(),
                self.dragging_split == Some(shell::SplitHandle::Rythmo) || hover_rythmo,
            ),
        ];
        for (rect, active) in handles {
            let color: [f32; 4] = if active {
                [0.45, 0.55, 0.95, 0.95]
            } else {
                [0.20, 0.20, 0.24, 0.55]
            };
            let y = rect.y + rect.height / 2.0 - 1.0;
            quads.push(QuadInstance {
                rect: [rect.x, y, rect.width, 2.0],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Properties panel
        if let Some(props) = &l.properties {
            quads.push(QuadInstance {
                rect: [props.x, props.y, props.width, props.height],
                color: PROPS_BG,
                color_bottom: PROPS_BG,
                border_color: PROPS_BORDER,
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [-2.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.3],
                shadow_blur: 6.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            quads.push(QuadInstance {
                rect: [props.x, props.y, 1.0, props.height],
                color: PROPS_BORDER,
                color_bottom: PROPS_BORDER,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            let header_rect = Rect {
                x: props.x,
                y: props.y,
                width: props.width,
                height: 32.0,
            };
            labels.push(LabelInfo {
                text: t("zone.properties"),
                bounds: header_rect,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 8.0,
                font_size_override: None,
                color_override: None,
                font_family_override: None,
            });
        }
    }

    /// Studio Mode render: black BG + video + export-style rythmo band only.
    pub fn render_studio(
        &mut self,
        renderer: &mut UiRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        ui_scale: f32,
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        project: &Project,
        render_index: &ProjectRenderIndex,
        render_frame: f64,
        fps: f64,
    ) {
        let mut quads: Vec<QuadInstance> = Vec::new();
        let mut labels: Vec<LabelInfo> = Vec::new();
        let mut stretched_texts: Vec<StretchedText> = Vec::new();
        let mut actor_icon_draws: Vec<rythmo::VoiceActorIconDraw> = Vec::new();
        self.actor_icon_cache.sync(
            project,
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );

        // Compute studio rythmo zone: full width, bottom portion
        let rythmo_h = rythmo::studio_br_height(project, self.screen_w);
        let rythmo_zone = Rect {
            x: 0.0,
            y: self.screen_h - rythmo_h,
            width: self.screen_w,
            height: rythmo_h,
        };

        // Black background for rythmo zone
        let bg = [0.02, 0.02, 0.03, 1.0];
        quads.push(QuadInstance {
            rect: [
                rythmo_zone.x,
                rythmo_zone.y,
                rythmo_zone.width,
                rythmo_zone.height,
            ],
            color: bg,
            color_bottom: bg,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Export-style rythmo: ticks, playhead, lines, markers — NO waveform
        rythmo::render_studio_rythmo(
            &rythmo_zone,
            project,
            render_index,
            render_frame,
            fps,
            &self.rythmo_state,
            &mut quads,
            &mut labels,
            &mut stretched_texts,
            &mut actor_icon_draws,
        );

        // Prepare stretched text textures
        let stretched_quads = renderer.prepare_stretched_texts(
            device,
            queue,
            ui_scale,
            &stretched_texts,
            self.playing,
        );
        let mut base_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        for draw in actor_icon_draws {
            if let Some(actor) = project.find_voice_actor(&draw.actor_name) {
                if let Some(bind_group) = self.actor_icon_cache.bind_group_for(actor) {
                    base_textured.push((
                        IconInstance {
                            rect: [draw.rect.x, draw.rect.y, draw.rect.width, draw.rect.height],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [1.0, 1.0, 1.0, 1.0],
                        },
                        bind_group,
                    ));
                }
            }
        }

        // Render through existing UiRenderer
        renderer.render(
            device,
            queue,
            encoder,
            view,
            screen_width,
            screen_height,
            ui_scale,
            &quads, // base layer
            &[],    // no overlay quads
            &[],    // no icons (markers use quads)
            &labels,
            video_quad,
            &stretched_quads,
            &[], // post_stretched_quads
            &base_textured,
            &[], // extra_textured
            &[], // post_texture_quads
            &[], // modal_quads
            &[], // modal_labels
        );
    }

    fn update_drawing_overlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut UiRenderer,
        project: &Project,
        current_frame: f64,
        _fps: f64,
    ) {
        use crate::rythmo_drawing::{rasterize_window, visible_frame_window, DrawingStroke};

        let zone = &self.layout.rythmo;
        let zw = zone.width.max(1.0) as u32;
        let zh = zone.height.max(1.0) as u32;
        let cf = current_frame;
        let ppf = crate::rythmo_drawing::ppf_for_scale(1.0);

        // Compute cache key
        let active_stroke_len = self
            .rythmo_state
            .active_stroke
            .as_ref()
            .map_or(0, |s| s.points.len());
        // Use frame * 1000 as key to include sub-frame precision for smooth scrolling
        let frame_key = (cf * 1000.0).round() as i64;
        let key = (
            frame_key,
            zw,
            zh,
            project.revision(),
            active_stroke_len,
            self.rythmo_state.drawing_dirty,
        );

        // Check if we need to re-rasterize. A live transform drag mutates the
        // actual stroke points without changing the revision, so force an
        // update while a transform handle is active to keep strokes in sync.
        let transform_active = self.rythmo_state.transform_handle.is_some();
        let needs_update = transform_active
            || self
                .drawing_overlay_cache
                .as_ref()
                .is_none_or(|c| c.key != key);

        if needs_update {
            // Collect visible strokes
            let (first_frame, last_frame) = visible_frame_window(zone.width, cf, ppf, 4);
            let mut strokes: Vec<&DrawingStroke> =
                project.drawing().query_window(first_frame, last_frame);

            // Add active stroke for live preview
            if let Some(ref active) = self.rythmo_state.active_stroke {
                if active.points.len() > 1 {
                    strokes.push(active);
                }
            }

            if !strokes.is_empty() {
                let rgba = rasterize_window(&strokes, zw, zh, cf, ppf);

                // Reuse the existing GPU texture when the zone size is unchanged so
                // scrolling/playback doesn't reallocate a texture every frame (which
                // caused stutter). Only recreate when the zone is resized.
                let mut reused = false;
                if let Some(c) = self.drawing_overlay_cache.as_mut() {
                    if c.zw == zw && c.zh == zh {
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &c._texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            &rgba,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(4 * zw),
                                rows_per_image: Some(zh),
                            },
                            wgpu::Extent3d {
                                width: zw,
                                height: zh,
                                depth_or_array_layers: 1,
                            },
                        );
                        c.key = key;
                        reused = true;
                    }
                }

                if !reused {
                    // Create texture
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Drawing Overlay"),
                        size: wgpu::Extent3d {
                            width: zw,
                            height: zh,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &rgba,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * zw),
                            rows_per_image: Some(zh),
                        },
                        wgpu::Extent3d {
                            width: zw,
                            height: zh,
                            depth_or_array_layers: 1,
                        },
                    );

                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Drawing Overlay BG"),
                        layout: renderer.texture_bind_group_layout(),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(
                                    renderer.texture_sampler(),
                                ),
                            },
                        ],
                    });

                    self.drawing_overlay_cache = Some(DrawingOverlayCache {
                        _texture: texture,
                        bind_group,
                        key,
                        zw,
                        zh,
                    });
                }
            } else {
                self.drawing_overlay_cache = None;
            }
        }

        self.rythmo_state.drawing_dirty = false;
    }
}

// Drawing overlay cache for GPU texture
pub(crate) struct DrawingOverlayCache {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    key: (i64, u32, u32, u64, usize, bool),
    zw: u32,
    zh: u32,
}
