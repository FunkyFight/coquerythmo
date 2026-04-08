pub mod connect_modal;
pub mod dropdown;
pub mod color_picker;
pub mod export_modal;
pub mod icon_button;
pub mod icons;
pub mod interactive;
pub mod layout;
pub mod rythmo;
pub mod settings_modal;
pub mod slider;
pub mod text_input;
pub mod theme;
pub mod renderer;
pub mod tooltip;
pub mod widget;

use layout::{Layout, PROPS_DEFAULT_W, PROPS_DRAG_ZONE, PROPS_MAX_W, PROPS_MIN_W};
use renderer::StretchedText;
use tooltip::TooltipState;
use widget::{EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign, Widget};

use crate::i18n::t;
use crate::project::Project;

use self::dropdown::Dropdown;
use self::icon_button::IconButton;
use self::icons::IconAtlas;
use self::renderer::UiRenderer;
use self::slider::Slider;

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
    tooltip: Option<TooltipState>,
    cursor_pos: (f32, f32),
    playing: bool,
    volume: f32,
    pub rythmo_state: rythmo::RythmoState,
    icon_uvs: std::collections::HashMap<String, [f32; 4]>,
    active_dropdown: Option<widget::ToolbarDropdown>,
    pub export_progress: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    export_label: String,
    connect_modal: Option<connect_modal::ConnectModal>,
    settings_modal: Option<settings_modal::SettingsModal>,
    export_modal: Option<export_modal::ExportModal>,
    network_in_room: bool,
    pub network_status: String,
    pub has_video: bool,
    pub sync_overlay: Option<String>,
    pub sync_progress: f32, // 0.0 to 1.0
}

impl Ui {
    pub fn new(screen_width: u32, screen_height: u32, icon_atlas: &IconAtlas) -> Self {
        let sw = screen_width as f32;
        let sh = screen_height as f32;
        let layout = Layout::compute(sw, sh, false, PROPS_DEFAULT_W);

        let icon_names = ["resume", "pause", "prev_frame", "next_frame",
            "boucle", "out", "scene", "respirations", "reactions", "liaison_left", "liaison_right", "settings", "stretcher", "br-edit"];
        let icon_uvs: std::collections::HashMap<String, [f32; 4]> = icon_names.iter()
            .map(|&name| (name.to_string(), icon_atlas.get_uv(name).unwrap_or([0.0; 4])))
            .collect();

        let settings_uv = icon_uvs.get("settings").copied().unwrap_or([0.0; 4]);
        let mut ui = Self {
            topbar_widgets: Self::build_topbar(false, false, sw, settings_uv),
            toolbar_widgets: vec![],
            layout,
            screen_w: sw,
            screen_h: sh,
            props_visible: false,
            props_width: PROPS_DEFAULT_W,
            dragging_props: false,
            tooltip: None,
            cursor_pos: (0.0, 0.0),
            playing: false,
            volume: 0.75,
            rythmo_state: rythmo::RythmoState::new(),
            icon_uvs,
            active_dropdown: None,
            export_progress: None,
            export_label: String::new(),
            connect_modal: None,
            settings_modal: None,
            export_modal: None,
            network_in_room: false,
            network_status: String::new(),
            sync_overlay: None,
            sync_progress: 0.0,
            has_video: false,
        };
        ui.toolbar_widgets = ui.build_toolbar();
        ui
    }

    fn rebuild_layout(&mut self) {
        self.layout = Layout::compute(self.screen_w, self.screen_h, self.props_visible, self.props_width);
        self.toolbar_widgets = self.build_toolbar();
    }

    fn build_topbar(in_room: bool, has_video: bool, screen_w: f32, settings_uv: [f32; 4]) -> Vec<Box<dyn Widget>> {
        // Build project menu with "Récent" submenu
        let recents = crate::config::recent_projects();
        let recent_labels: Vec<String> = recents.iter().map(|r| {
            let video = r.video_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let br = r.br_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            format!("{} + {}", video, br)
        }).collect();

        let recents_clone = recents.clone();
        let mut project_menu = Dropdown::new(
            Rect { x: 4.0, y: 2.0, width: 80.0, height: 28.0 },
            vec![
                t("menu.project.add_video").into(),
                t("menu.project.import").into(),
                t("menu.project.export").into(),
                format!("{} ▸", t("menu.project.recent")),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::AddVideo),
                1 => EventResponse::Action(UiAction::ImportProject),
                2 => EventResponse::Action(UiAction::ExportProject),
                _ => EventResponse::Consumed, // "Récent" item does nothing on click
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.project"))
        .with_panel_width(250.0)
        .with_disabled_items(vec![false, !has_video, !has_video, false]);

        // Attach submenu to item index 3 ("Récent ▸")
        if !recent_labels.is_empty() {
            project_menu = project_menu.with_submenu(3, recent_labels, move |index, _label| {
                if let Some(r) = recents_clone.get(index) {
                    EventResponse::Action(UiAction::OpenRecentProject {
                        video_path: r.video_path.clone(),
                        br_path: r.br_path.clone(),
                    })
                } else {
                    EventResponse::Consumed
                }
            });
        }

        let export_menu = Dropdown::new(
            Rect { x: 88.0, y: 2.0, width: 80.0, height: 28.0 },
            vec![
                t("menu.export.mp4").into(),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::OpenExportModal),
                _ => EventResponse::Consumed,
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.export"))
        .with_panel_width(260.0);

        let connect_menu = Dropdown::new(
            Rect { x: 172.0, y: 2.0, width: 120.0, height: 28.0 },
            vec![
                t("menu.connect.create_room").into(),
                t("menu.connect.join_room").into(),
                t("menu.connect.disconnect").into(),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::OpenConnectModal { join: false }),
                1 => EventResponse::Action(UiAction::OpenConnectModal { join: true }),
                2 => EventResponse::Action(UiAction::NetworkDisconnect),
                _ => EventResponse::Consumed,
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.connect"))
        .with_panel_width(280.0)
        .with_disabled_items(vec![
            in_room,   // Create: disabled if already in room
            in_room,   // Join: disabled if already in room
            !in_room,  // Disconnect: disabled if not in room
        ]);

        let settings_size = 24.0;
        let settings_x = screen_w - settings_size - 8.0;
        let settings_y = (TOPBAR_HEIGHT - settings_size) / 2.0;
        let settings_btn = IconButton::new(
            Rect { x: settings_x, y: settings_y, width: settings_size, height: settings_size },
            "", settings_uv,
            || EventResponse::Action(UiAction::OpenSettings),
        ).with_tooltip(t("settings.tooltip"));

        vec![Box::new(project_menu), Box::new(export_menu), Box::new(connect_menu), Box::new(settings_btn)]
    }

    pub fn rebuild_topbar(&mut self, in_room: bool) {
        self.network_in_room = in_room;
        self.topbar_widgets = Self::build_topbar(in_room, self.has_video, self.screen_w, self.uv("settings"));
    }

    pub fn rebuild_toolbar(&mut self) {
        self.toolbar_widgets = self.build_toolbar();
    }

    fn uv(&self, name: &str) -> [f32; 4] {
        self.icon_uvs.get(name).copied().unwrap_or([0.0; 4])
    }

    fn build_toolbar(&self) -> Vec<Box<dyn Widget>> {
        use crate::rythmo_line::MarkerKind;

        let tb = &self.layout.toolbar;
        let s = TOOLBAR_BTN_SIZE;
        let y = tb.y + (TOOLBAR_HEIGHT - s) / 2.0;
        let gap = 4.0;
        let mut x = tb.x + 8.0;

        let mut widgets: Vec<Box<dyn Widget>> = Vec::new();

        // Helper macro to keep it DRY
        macro_rules! btn {
            ($icon:expr, $action:expr, $tip:expr) => {{
                let b = IconButton::new(
                    Rect { x, y, width: s, height: s }, "", self.uv($icon), $action,
                ).with_tooltip(t($tip));
                widgets.push(Box::new(b));
                x += s + gap;
            }};
        }

        // Transport: prev | play/pause | next
        btn!("prev_frame", || EventResponse::Action(UiAction::PrevFrame), "toolbar.prev_frame");
        let play_uv = if self.playing { self.uv("pause") } else { self.uv("resume") };
        let play_tip = if self.playing { "toolbar.stop" } else { "toolbar.play" };
        let play = IconButton::new(
            Rect { x, y, width: s, height: s }, "", play_uv,
            || EventResponse::Action(UiAction::TogglePlayPause),
        ).with_tooltip(t(play_tip));
        widgets.push(Box::new(play));
        x += s + gap;
        btn!("next_frame", || EventResponse::Action(UiAction::NextFrame), "toolbar.next_frame");

        x += gap * 2.0; // separator

        // Markers: boucle | out | scene
        btn!("boucle", || EventResponse::Action(UiAction::AddMarker(MarkerKind::Boucle)), "toolbar.boucle");
        btn!("out", || EventResponse::Action(UiAction::AddMarker(MarkerKind::Out)), "toolbar.out");
        btn!("scene", || EventResponse::Action(UiAction::AddMarker(MarkerKind::SceneChange)), "toolbar.scene");

        x += gap * 2.0; // separator

        // Quick-insert dropdowns: respirations | reactions
        btn!("respirations", || EventResponse::Action(UiAction::OpenDropdown(widget::ToolbarDropdown::Respirations)), "toolbar.respirations");
        btn!("reactions", || EventResponse::Action(UiAction::OpenDropdown(widget::ToolbarDropdown::Reactions)), "toolbar.reactions");

        x += gap * 2.0; // separator

        // Liaisons: left | right
        btn!("liaison_left", || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonLeft)), "toolbar.liaison_left");
        btn!("liaison_right", || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonRight)), "toolbar.liaison_right");

        x += gap * 2.0; // separator

        // Syllable stretcher (toggles between stretcher and br-edit icons)
        let stretcher_icon = if self.rythmo_state.syllable_mode { "br-edit" } else { "stretcher" };
        let stretcher_tip = if self.rythmo_state.syllable_mode { "toolbar.back_to_edit" } else { "toolbar.stretcher" };
        btn!(stretcher_icon, || EventResponse::Action(UiAction::ToggleSyllableMode), stretcher_tip);

        // Right side: volume slider
        let slider_w = SLIDER_W;
        let slider_h = 24.0;
        let slider_x = tb.x + tb.width - slider_w - 8.0;
        let slider_y = tb.y + (TOOLBAR_HEIGHT - slider_h) / 2.0;
        let volume = Slider::new(
            Rect { x: slider_x, y: slider_y, width: slider_w, height: slider_h },
            self.volume, |val| EventResponse::Action(UiAction::SetVolume(val)),
        ).with_tooltip(t("toolbar.volume"));
        widgets.push(Box::new(volume));

        widgets
    }

    pub fn handle_event(&mut self, event: &UiEvent, project: &Project, current_frame: i64, fps: f64) -> EventResponse {
        if let UiEvent::MouseMove { x, y } = event {
            self.cursor_pos = (*x, *y);
        }

        // Sync overlay blocks all input
        if self.sync_overlay.is_some() {
            return EventResponse::Consumed;
        }

        // Settings modal intercepts all input
        if self.settings_modal.is_some() {
            return self.handle_settings_modal_event(event);
        }

        // Export modal intercepts all input
        if self.export_modal.is_some() {
            return self.handle_export_modal_event(event);
        }

        // Connect modal intercepts all input
        if self.connect_modal.is_some() {
            return self.handle_connect_modal_event(event);
        }

        // Toolbar dropdown overlay
        if self.active_dropdown.is_some() {
            if let UiEvent::MousePress { x, y } = event {
                let resp = self.handle_dropdown_click(*x, *y);
                if resp != EventResponse::Ignored { return resp; }
            }
        }

        if let Some(response) = self.handle_props_drag(event) {
            return response;
        }

        // Rythmo zone events (lines, scroll, ctrl+click, etc.)
        let rythmo_response = rythmo::handle_rythmo_event(
            event, &self.layout.rythmo, project, current_frame, fps, &mut self.rythmo_state,
        );
        if rythmo_response != EventResponse::Ignored {
            return rythmo_response;
        }

        // Scroll in rythmo zone → seek (shift = fast)
        if let UiEvent::Scroll { x, y, delta, fast } = event {
            if self.layout.rythmo.contains(*x, *y) {
                let multiplier = if *fast { 60.0 } else { 15.0 };
                let frames = (delta * multiplier) as i32;
                if frames != 0 {
                    return EventResponse::Action(UiAction::SeekRelative(frames));
                }
            }
        }

        // Capturing widgets first
        for widget in self.topbar_widgets.iter_mut().chain(self.toolbar_widgets.iter_mut()) {
            if widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        // Normal widgets
        for widget in self.topbar_widgets.iter_mut().chain(self.toolbar_widgets.iter_mut()) {
            if !widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
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

    fn update_tooltip(&mut self) {
        let (cx, cy) = self.cursor_pos;
        for widget in self.topbar_widgets.iter().chain(self.toolbar_widgets.iter()) {
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

    pub fn toggle_play_pause(&mut self) {
        self.playing = !self.playing;
        self.toolbar_widgets = self.build_toolbar();
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn toggle_toolbar_dropdown(&mut self, dd: widget::ToolbarDropdown) {
        if self.active_dropdown == Some(dd.clone()) {
            self.active_dropdown = None;
        } else {
            self.active_dropdown = Some(dd);
        }
    }

    fn dropdown_items(dd: &widget::ToolbarDropdown) -> Vec<(&'static str, &'static str)> {
        match dd {
            widget::ToolbarDropdown::Respirations => vec![
                ("↑", "resp.up"), ("↓", "resp.down"),
                ("(H)", "resp.h"), ("(HH)", "resp.hh"),
                ("(mH)", "resp.mh"), ("(mHH)", "resp.mhh"),
            ],
            widget::ToolbarDropdown::Reactions => vec![
                ("(X)", "react.x"), ("(mts)", "react.mts"), ("(tsc)", "react.tsc"),
                ("(ah)", "react.ah"), ("(oh)", "react.oh"), ("(ih)", "react.ih"),
                ("(mhm)", "react.mhm"), ("(hm)", "react.hm"), ("(ptt)", "react.ptt"),
                ("(pff)", "react.pff"), ("(unh)", "react.unh"), ("(hun)", "react.hun"),
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
            return EventResponse::Action(UiAction::AddQuickLine { text: text.to_string() });
        }
        EventResponse::Consumed
    }

    fn toolbar_dropdown_rect(&self, dd: &widget::ToolbarDropdown, count: usize) -> Rect {
        let items = Self::dropdown_items(dd);
        let _ = items; // use count param
        let item_h = 26.0;
        let w = 220.0;
        let h = count as f32 * item_h;
        // Position below the button that opened it
        let btn_index = match dd {
            widget::ToolbarDropdown::Respirations => 6, // 7th button (0-indexed)
            widget::ToolbarDropdown::Reactions => 7,
        };
        let btn_x = self.layout.toolbar.x + 8.0 + btn_index as f32 * (TOOLBAR_BTN_SIZE + 4.0)
            + if btn_index >= 3 { 8.0 } else { 0.0 }  // separator after transport
            + if btn_index >= 6 { 8.0 } else { 0.0 }; // separator after markers
        Rect { x: btn_x, y: self.layout.toolbar.y - h - 2.0, width: w, height: h }
    }

    fn render_toolbar_dropdown(&self, quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'_>>) {
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
            color: DROPDOWN_PANEL_TOP, color_bottom: DROPDOWN_PANEL_BOT,
            border_color: DROPDOWN_PANEL_BORDER,
            border_width: 1.0, border_radius: 4.0,
            shadow_offset: [0.0, -2.0], shadow_color: [0.0, 0.0, 0.0, 0.4], shadow_blur: 8.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        let mut iy = rect.y;
        for (text, tooltip_key) in &items {
            // Item label
            labels.push(LabelInfo {
                text,
                bounds: Rect { x: rect.x + 8.0, y: iy, width: rect.width - 16.0, height: item_h },
                h_align: HAlign::Left, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(13.0), color_override: None, font_family_override: None,
            });
            // Tooltip text on the right
            labels.push(LabelInfo {
                text: t(tooltip_key),
                bounds: Rect { x: rect.x + 40.0, y: iy, width: rect.width - 48.0, height: item_h },
                h_align: HAlign::Right, v_align: VAlign::Center,
                overflow: Overflow::Ellipsis, padding: 0.0,
                font_size_override: Some(9.0), color_override: Some([150, 150, 160]), font_family_override: None,
            });
            iy += item_h;
        }
    }

    pub fn is_editing_text(&self) -> bool {
        self.rythmo_state.is_editing() || self.connect_modal.is_some() || self.export_modal.is_some()
    }

    fn handle_connect_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.connect_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            connect_modal::ConnectModalResult::Consumed => EventResponse::Consumed,
            connect_modal::ConnectModalResult::Close => {
                self.connect_modal = None;
                EventResponse::Consumed
            }
            connect_modal::ConnectModalResult::Connect { ip, port, password, username, room_code } => {
                self.connect_modal = None;
                EventResponse::Action(UiAction::NetworkConnect { ip, port, password, username, room_code })
            }
        }
    }

    fn handle_settings_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.settings_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            settings_modal::SettingsModalResult::Consumed => EventResponse::Consumed,
            settings_modal::SettingsModalResult::Close => {
                self.settings_modal = None;
                EventResponse::Consumed
            }
            settings_modal::SettingsModalResult::Save { lang, rythmo_font, scroll_speed } => {
                self.settings_modal = None;
                EventResponse::Action(UiAction::SaveSettings { lang, rythmo_font, scroll_speed })
            }
        }
    }

    fn handle_export_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.export_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            export_modal::ExportModalResult::Consumed => EventResponse::Consumed,
            export_modal::ExportModalResult::Close => {
                self.export_modal = None;
                EventResponse::Consumed
            }
            export_modal::ExportModalResult::Export { filename, fps } => {
                self.export_modal = None;
                EventResponse::Action(UiAction::StartExport { filename, fps })
            }
        }
    }

    pub fn open_export_modal(&mut self) {
        self.export_modal = Some(export_modal::ExportModal::new());
    }

    pub fn open_connect_modal(&mut self, join: bool) {
        self.connect_modal = Some(connect_modal::ConnectModal::new(join));
    }

    pub fn open_settings_modal(&mut self, fonts: Vec<String>) {
        self.settings_modal = Some(settings_modal::SettingsModal::new(fonts));
    }

    pub fn close_settings_modal(&mut self) {
        self.settings_modal = None;
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
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn resize(&mut self, screen_width: u32, screen_height: u32) {
        self.screen_w = screen_width as f32;
        self.screen_h = screen_height as f32;
        self.topbar_widgets = Self::build_topbar(self.network_in_room, self.has_video, self.screen_w, self.uv("settings"));
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
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        project: &Project,
        current_frame: i64,
    ) {
        // Prepare color picker textures first (needs &mut self, before labels borrow self)
        self.rythmo_state.color_picker.ensure_textures(
            device, queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );
        let mut color_picker_bg_quads: Vec<QuadInstance> = Vec::new();
        let mut extra_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        let mut color_picker_fg_quads: Vec<QuadInstance> = Vec::new();
        self.rythmo_state.color_picker.render(&mut color_picker_bg_quads, &mut extra_textured, &mut color_picker_fg_quads);

        // Update export label BEFORE borrowing self via labels
        if let Some(progress_atomic) = &self.export_progress {
            use std::sync::atomic::Ordering;
            let progress = f32::from_bits(progress_atomic.load(Ordering::Relaxed));
            let pct = (progress.clamp(0.0, 1.0) * 100.0) as u32;
            self.export_label = format!("Export en cours... {}%", pct);
        }

        let mut quads = Vec::new();         // base layer (behind video)
        let mut overlay_quads = Vec::new(); // overlay layer (on top of video)
        let mut icons: Vec<IconInstance> = Vec::new();
        let mut labels: Vec<LabelInfo> = Vec::new();

        // Zone backgrounds
        self.render_zones(&mut quads, &mut labels, current_frame);

        // Network status in topbar (right-aligned)
        if !self.network_status.is_empty() {
            labels.push(LabelInfo {
                text: &self.network_status,
                bounds: Rect {
                    x: self.screen_w - 400.0,
                    y: 2.0,
                    width: 350.0,
                    height: TOPBAR_HEIGHT - 4.0,
                },
                h_align: HAlign::Right,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 8.0,
                font_size_override: Some(12.0),
                color_override: Some([160, 200, 160]),
                font_family_override: None,
            });
        }

        // Rythmo lines
        let mut stretched_texts: Vec<StretchedText> = Vec::new();
        let cursor_info = rythmo::render_lines(
            &self.layout.rythmo, project, current_frame,
            &self.rythmo_state, &mut quads, &mut labels, &mut stretched_texts,
        );

        // Markers
        let mut liaison_icons: Vec<IconInstance> = Vec::new();
        rythmo::render_markers(
            &self.layout.rythmo, project, current_frame,
            &mut quads, &mut labels, &mut liaison_icons,
            self.uv("liaison_left"), self.uv("liaison_right"),
        );
        icons.extend(liaison_icons);

        // Prepare stretched text textures
        let stretched_quads = renderer.prepare_stretched_texts(device, queue, &stretched_texts);

        // Render cursor using real glyph positions from the renderer cache
        if let Some((line_id, cursor_pos, text_x, text_w, ry, rh)) = cursor_info {
            let ratio = renderer.cursor_x_ratio(line_id, cursor_pos);
            let cx = text_x + ratio * text_w;
            let margin = rh * 0.25;
            quads.push(QuadInstance {
                rect: [cx, ry + margin, 1.5, rh - margin * 2.0],
                color: [0.9, 0.9, 0.95, 1.0], color_bottom: [0.9, 0.9, 0.95, 1.0],
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
        }

        // Non-capturing widgets
        for widget in self.topbar_widgets.iter().chain(self.toolbar_widgets.iter()) {
            if !widget.captures_all() {
                quads.extend(widget.render_quads());
                icons.extend(widget.render_icons());
                labels.extend(widget.labels());
            }
        }

        // Capturing widgets → overlay (on top of video)
        for widget in self.topbar_widgets.iter().chain(self.toolbar_widgets.iter()) {
            if widget.captures_all() {
                overlay_quads.extend(widget.render_quads());
                icons.extend(widget.render_icons());
                labels.extend(widget.labels());
            }
        }

        // Autocomplete dropdown (on top of all lines)
        rythmo::render_autocomplete(
            &self.layout.rythmo, project, current_frame,
            &self.rythmo_state, &mut quads, &mut labels,
        );

        // Color picker quads → overlay
        overlay_quads.extend(color_picker_bg_quads);

        // Toolbar dropdown → overlay
        self.render_toolbar_dropdown(&mut overlay_quads, &mut labels);

        // Tooltip → overlay
        if let Some(tooltip) = &self.tooltip {
            overlay_quads.extend(tooltip.render_quads(self.screen_w));
            labels.extend(tooltip.render_labels(self.screen_w));
        }

        // Export progress modal (only shown once export actually started, progress > 0)
        let export_progress_val = self.export_progress.as_ref().map(|p| {
            f32::from_bits(p.load(std::sync::atomic::Ordering::Relaxed))
        }).unwrap_or(0.0);
        if self.export_progress.is_some() && export_progress_val > 0.0 {
            let progress = export_progress_val;

            let dw = 420.0;
            let dh = 120.0;
            let dx = (self.screen_w - dw) / 2.0;
            let dy = (self.screen_h - dh) / 2.0;

            // Dim
            overlay_quads.push(QuadInstance {
                rect: [0.0, 0.0, self.screen_w, self.screen_h],
                color: [0.0, 0.0, 0.0, 0.75], color_bottom: [0.0, 0.0, 0.0, 0.75],
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Card
            overlay_quads.push(QuadInstance {
                rect: [dx, dy, dw, dh],
                color: [0.22, 0.22, 0.26, 1.0], color_bottom: [0.16, 0.16, 0.19, 1.0],
                border_color: [0.45, 0.45, 0.52, 0.8],
                border_width: 1.5, border_radius: 14.0,
                shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Bar track
            let bx = dx + 30.0;
            let by = dy + 65.0;
            let bw = dw - 60.0;
            let bh = 14.0;
            overlay_quads.push(QuadInstance {
                rect: [bx, by, bw, bh],
                color: [0.10, 0.10, 0.13, 1.0], color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.8], border_width: 1.0, border_radius: 7.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Bar fill
            let fill = (bw - 4.0) * progress.clamp(0.0, 1.0);
            if fill > 0.5 {
                overlay_quads.push(QuadInstance {
                    rect: [bx + 2.0, by + 2.0, fill, bh - 4.0],
                    color: [0.35, 0.60, 1.0, 1.0], color_bottom: [0.25, 0.45, 0.85, 1.0],
                    border_color: [0.0; 4], border_width: 0.0, border_radius: 5.0,
                    shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                    rotation: 0.0, _padding: [0.0; 2],
                });
            }
            // Labels
            labels.push(LabelInfo {
                text: &self.export_label,
                bounds: Rect { x: dx, y: dy + 18.0, width: dw, height: 28.0 },
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(17.0), color_override: None, font_family_override: None,
            });
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
                color: [0.0, 0.0, 0.0, 0.85], color_bottom: [0.0, 0.0, 0.0, 0.85],
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Card
            overlay_quads.push(QuadInstance {
                rect: [dx, dy, dw, dh],
                color: [0.22, 0.22, 0.26, 1.0], color_bottom: [0.16, 0.16, 0.19, 1.0],
                border_color: [0.45, 0.45, 0.52, 0.8],
                border_width: 1.5, border_radius: 14.0,
                shadow_offset: [0.0, 4.0], shadow_color: [0.0, 0.0, 0.0, 0.5], shadow_blur: 10.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Label
            labels.push(LabelInfo {
                text: msg,
                bounds: Rect { x: dx, y: dy + 16.0, width: dw, height: 28.0 },
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 0.0,
                font_size_override: Some(16.0), color_override: Some([200, 200, 220]), font_family_override: None,
            });
            // Progress bar track
            let bx = dx + 30.0;
            let by = dy + 58.0;
            let bw = dw - 60.0;
            let bh = 14.0;
            overlay_quads.push(QuadInstance {
                rect: [bx, by, bw, bh],
                color: [0.10, 0.10, 0.13, 1.0], color_bottom: [0.10, 0.10, 0.13, 1.0],
                border_color: [0.30, 0.30, 0.38, 0.8], border_width: 1.0, border_radius: 7.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            // Progress bar fill
            let fill = (bw - 4.0) * self.sync_progress.clamp(0.0, 1.0);
            if fill > 0.5 {
                overlay_quads.push(QuadInstance {
                    rect: [bx + 2.0, by + 2.0, fill, bh - 4.0],
                    color: [0.35, 0.60, 1.0, 1.0], color_bottom: [0.25, 0.45, 0.85, 1.0],
                    border_color: [0.0; 4], border_width: 0.0, border_radius: 5.0,
                    shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                    rotation: 0.0, _padding: [0.0; 2],
                });
            }
        }

        // Settings modal
        if let Some(modal) = &self.settings_modal {
            modal.render(&mut overlay_quads, &mut labels, self.screen_w, self.screen_h);
        }

        // Connect modal
        if let Some(modal) = &self.connect_modal {
            modal.render(&mut overlay_quads, &mut labels, self.screen_w, self.screen_h);
        }

        // Export modal
        if let Some(modal) = &self.export_modal {
            modal.render(&mut overlay_quads, &mut labels, self.screen_w, self.screen_h);
        }

        renderer.render(
            device, queue, encoder, view,
            screen_width, screen_height,
            &quads, &overlay_quads, &icons, &labels,
            video_quad,
            &stretched_quads,
            &extra_textured,
            &color_picker_fg_quads,
        );
    }

    fn render_zones<'a>(&'a self, quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>, current_frame: i64) {
        let l = &self.layout;

        // Topbar
        quads.push(QuadInstance {
            rect: [l.topbar.x, l.topbar.y, l.topbar.width, l.topbar.height],
            color: TOPBAR_BG, color_bottom: TOPBAR_BG,
            border_color: TOPBAR_SHADOW, border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0, 1.0], shadow_color: [0.0, 0.0, 0.0, 0.3], shadow_blur: 4.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Video preview
        quads.push(QuadInstance {
            rect: [l.video_preview.x, l.video_preview.y, l.video_preview.width, l.video_preview.height],
            color: VIDEO_BG, color_bottom: VIDEO_BG,
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Toolbar
        quads.push(QuadInstance {
            rect: [l.toolbar.x, l.toolbar.y, l.toolbar.width, l.toolbar.height],
            color: TOOLBAR_BG, color_bottom: TOOLBAR_BG,
            border_color: TOOLBAR_BORDER, border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });

        // Bande rythmo — fond noir + perforations + playhead
        quads.push(QuadInstance {
            rect: [l.rythmo.x, l.rythmo.y, l.rythmo.width, l.rythmo.height],
            color: [0.02, 0.02, 0.03, 1.0], color_bottom: [0.02, 0.02, 0.03, 1.0],
            border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
            shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
            rotation: 0.0, _padding: [0.0; 2],
        });
        quads.extend(rythmo::render_rythmo_base(&l.rythmo, current_frame));

        // Properties panel
        if let Some(props) = &l.properties {
            quads.push(QuadInstance {
                rect: [props.x, props.y, props.width, props.height],
                color: PROPS_BG, color_bottom: PROPS_BG,
                border_color: PROPS_BORDER, border_width: 0.0, border_radius: 0.0,
                shadow_offset: [-2.0, 0.0], shadow_color: [0.0, 0.0, 0.0, 0.3], shadow_blur: 6.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            quads.push(QuadInstance {
                rect: [props.x, props.y, 1.0, props.height],
                color: PROPS_BORDER, color_bottom: PROPS_BORDER,
                border_color: [0.0; 4], border_width: 0.0, border_radius: 0.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0,
                rotation: 0.0, _padding: [0.0; 2],
            });
            let header_rect = Rect { x: props.x, y: props.y, width: props.width, height: 32.0 };
            labels.push(LabelInfo {
                text: t("zone.properties"), bounds: header_rect,
                h_align: HAlign::Center, v_align: VAlign::Center,
                overflow: Overflow::Clip, padding: 8.0, font_size_override: None, color_override: None, font_family_override: None,
            });
        }
    }
}
