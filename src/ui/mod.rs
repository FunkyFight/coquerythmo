pub mod actor_icon_cache;
pub mod color_picker;
pub mod connect_modal;
pub mod dropdown;
pub mod export_modal;
pub mod file_explorer_modal;
pub mod icon_button;
pub mod icons;
pub mod interactive;
pub mod layout;
pub mod project_settings_modal;
pub mod proxy_error_modal;
pub mod proxy_modal;
pub mod rename_character_modal;
pub mod renderer;
pub mod rythmo;
pub mod save_prompt_modal;
pub mod server_browser;
pub mod settings_modal;
pub mod slider;
pub mod studio_warning_modal;
pub mod text_input;
pub mod theme;
pub mod toast;
pub mod tooltip;
pub mod voice_actor_modal;
pub mod whats_new_modal;
pub mod widget;

use layout::{Layout, PROPS_DEFAULT_W, PROPS_DRAG_ZONE, PROPS_MAX_W, PROPS_MIN_W};
use renderer::StretchedText;
use tooltip::TooltipState;
use widget::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign, Widget,
};

use crate::i18n::t;
use crate::project::Project;

use self::actor_icon_cache::ActorIconCache;
use self::dropdown::Dropdown;
use self::icon_button::IconButton;
use self::icons::IconAtlas;
use self::renderer::UiRenderer;
use self::slider::Slider;

use theme::*;

pub(crate) fn scroll_delta_to_frames(delta: f32, multiplier: f32) -> i32 {
    scroll_delta_to_frames_impl(delta, multiplier)
}

#[cfg(target_os = "macos")]
fn scroll_delta_to_frames_impl(delta: f32, multiplier: f32) -> i32 {
    let frames = (delta * multiplier).round() as i32;
    if frames == 0 && delta.abs() > f32::EPSILON {
        if delta > 0.0 {
            1
        } else {
            -1
        }
    } else {
        frames
    }
}

#[cfg(not(target_os = "macos"))]
fn scroll_delta_to_frames_impl(delta: f32, multiplier: f32) -> i32 {
    (delta * multiplier) as i32
}

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
    pub export_render_backend: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    export_label: String,
    pub progress_prefix: String,
    connect_modal: Option<connect_modal::ConnectModal>,
    settings_modal: Option<settings_modal::SettingsModal>,
    project_settings_modal: Option<project_settings_modal::ProjectSettingsModal>,
    export_modal: Option<export_modal::ExportModal>,
    file_explorer_modal: Option<file_explorer_modal::FileExplorerModal>,
    proxy_modal: Option<proxy_modal::ProxyModal>,
    rename_character_modal: Option<rename_character_modal::RenameCharacterModal>,
    proxy_error_modal: Option<proxy_error_modal::ProxyErrorModal>,
    server_browser: Option<server_browser::ServerBrowserModal>,
    add_server_modal: Option<server_browser::AddServerModal>,
    save_prompt_modal: Option<save_prompt_modal::SavePromptModal>,
    studio_warning_modal: Option<studio_warning_modal::StudioWarningModal>,
    voice_actor_modal: Option<voice_actor_modal::VoiceActorModal>,
    whats_new_modal: Option<whats_new_modal::WhatsNewModal>,
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
    pub toasts: toast::ToastManager,
}

impl Ui {
    pub fn new(screen_width: u32, screen_height: u32, icon_atlas: &IconAtlas) -> Self {
        let sw = screen_width as f32;
        let sh = screen_height as f32;
        let layout = Layout::compute(sw, sh, false, PROPS_DEFAULT_W);

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
            topbar_widgets: Self::build_topbar(false, false, sw, settings_uv, project_uv),
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
            export_render_backend: None,
            export_label: String::new(),
            progress_prefix: String::new(),
            connect_modal: None,
            settings_modal: None,
            project_settings_modal: None,
            export_modal: None,
            file_explorer_modal: None,
            proxy_modal: None,
            rename_character_modal: None,
            proxy_error_modal: None,
            server_browser: None,
            add_server_modal: None,
            save_prompt_modal: None,
            studio_warning_modal: None,
            voice_actor_modal: None,
            whats_new_modal: None,
            actor_icon_cache: ActorIconCache::new(),
            network_in_room: false,
            network_status: "Déconnecté".into(),
            network_room_label: String::new(),
            sync_overlay: None,
            sync_progress: 0.0,
            has_video: false,
            current_frame: 0,
            total_frames: 0,
            scrubbing: false,
            toasts: toast::ToastManager::new(),
        };
        ui.toolbar_widgets = ui.build_toolbar();
        ui
    }

    fn rebuild_layout(&mut self) {
        self.layout = Layout::compute(
            self.screen_w,
            self.screen_h,
            self.props_visible,
            self.props_width,
        );
        self.toolbar_widgets = self.build_toolbar();
    }

    fn build_topbar(
        in_room: bool,
        has_video: bool,
        screen_w: f32,
        settings_uv: [f32; 4],
        project_uv: [f32; 4],
    ) -> Vec<Box<dyn Widget>> {
        // Build project menu with "Récent" submenu
        let recents = crate::config::recent_projects();
        let recent_labels: Vec<String> = recents
            .iter()
            .map(|r| {
                let video = r
                    .video_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let br = r
                    .br_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                format!("{} + {}", video, br)
            })
            .collect();

        let recents_clone = recents.clone();
        let mut project_menu = Dropdown::new(
            Rect {
                x: 4.0,
                y: 2.0,
                width: 80.0,
                height: 28.0,
            },
            vec![
                t("menu.project.add_video").into(),
                format!("{} ▸", t("menu.project.import")),
                t("menu.project.export").into(),
                t("menu.project.restore_backup").into(),
                format!("{} ▸", t("menu.project.recent")),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::AddVideo),
                2 => EventResponse::Action(UiAction::ExportProject),
                3 => EventResponse::Action(UiAction::RestoreBackup),
                _ => EventResponse::Consumed, // "Importer" et "Récents" ne font rien au clic
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.project"))
        .with_panel_width(340.0)
        .with_disabled_items(vec![false, false, !has_video, false, false]);

        project_menu = project_menu.with_submenu(
            1,
            vec![
                t("menu.project.import.coquerythmo").into(),
                t("menu.project.import.cappela").into(),
                t("menu.project.import.srt").into(),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::ImportProject),
                1 => EventResponse::Action(UiAction::ImportCappelaProject),
                2 => EventResponse::Action(UiAction::ImportSrtProject),
                _ => EventResponse::Consumed,
            },
        );

        // Attach submenu to item index 4 ("Récent ▸")
        if !recent_labels.is_empty() {
            project_menu = project_menu.with_submenu(4, recent_labels, move |index, _label| {
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
            Rect {
                x: 88.0,
                y: 2.0,
                width: 80.0,
                height: 28.0,
            },
            vec![
                t("menu.export.mp4").into(),
                format!("{} (Alpha)", t("menu.export.studio_mode")),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::OpenExportModal),
                1 => EventResponse::Action(UiAction::ShowStudioWarning),
                _ => EventResponse::Consumed,
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.export"))
        .with_panel_width(260.0);

        let tools_menu = Dropdown::new(
            Rect {
                x: 172.0,
                y: 2.0,
                width: 80.0,
                height: 28.0,
            },
            vec![
                t("menu.tools.create_proxy").into(),
                t("menu.tools.secondary_display").into(),
                t("menu.tools.rename_character").into(),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::OpenProxyModal),
                1 => EventResponse::Action(UiAction::OpenSecondaryDisplay),
                2 => EventResponse::Action(UiAction::OpenRenameCharacterModal),
                _ => EventResponse::Consumed,
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.tools"))
        .with_panel_width(280.0)
        .with_disabled_items(vec![!has_video, !has_video, false]);

        let connect_menu = Dropdown::new(
            Rect {
                x: 256.0,
                y: 2.0,
                width: 120.0,
                height: 28.0,
            },
            vec![
                t("menu.connect.servers").into(),
                t("menu.connect.disconnect").into(),
            ],
            |index, _label| match index {
                0 => EventResponse::Action(UiAction::OpenServerBrowser),
                1 => EventResponse::Action(UiAction::NetworkDisconnect),
                _ => EventResponse::Consumed,
            },
        )
        .with_arrow(false)
        .with_trigger_bg(false)
        .with_trigger_label(t("menu.connect"))
        .with_panel_width(250.0)
        .with_disabled_items(vec![false, !in_room]);

        let settings_size = 24.0;
        let settings_x = screen_w - settings_size - 8.0;
        let settings_y = (TOPBAR_HEIGHT - settings_size) / 2.0;
        let project_x = settings_x - settings_size - 8.0;
        let project_btn = IconButton::new(
            Rect {
                x: project_x,
                y: settings_y,
                width: settings_size,
                height: settings_size,
            },
            "",
            project_uv,
            || EventResponse::Action(UiAction::OpenProjectSettings),
        )
        .with_tooltip(t("project_settings.tooltip"));
        let settings_btn = IconButton::new(
            Rect {
                x: settings_x,
                y: settings_y,
                width: settings_size,
                height: settings_size,
            },
            "",
            settings_uv,
            || EventResponse::Action(UiAction::OpenSettings),
        )
        .with_tooltip(t("settings.tooltip"));

        vec![
            Box::new(project_menu),
            Box::new(export_menu),
            Box::new(tools_menu),
            Box::new(connect_menu),
            Box::new(project_btn),
            Box::new(settings_btn),
        ]
    }

    pub fn rebuild_topbar(&mut self, in_room: bool) {
        self.network_in_room = in_room;
        self.topbar_widgets = Self::build_topbar(
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

    fn progress_bar_rect(&self) -> Rect {
        let tb = &self.layout.toolbar;
        let s = TOOLBAR_BTN_SIZE;
        let gap = 4.0;
        // 13 buttons + 4 double-gaps + 1 trailing gap
        let buttons_end = tb.x + 8.0 + 13.0 * (s + gap) + 4.0 * gap * 2.0 + gap;
        let slider_start = tb.x + tb.width - SLIDER_W - 8.0;
        let mute_start = slider_start - s - gap;
        let left = buttons_end + 8.0;
        let right = mute_start - 8.0;
        let w = (right - left).max(40.0);
        let h = 6.0;
        Rect {
            x: left,
            y: tb.y + (TOOLBAR_HEIGHT - h) / 2.0,
            width: w,
            height: h,
        }
    }

    fn progress_bar_hit_rect(&self) -> Rect {
        let r = self.progress_bar_rect();
        Rect {
            x: r.x,
            y: r.y - 8.0,
            width: r.width,
            height: r.height + 16.0,
        }
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
                    Rect {
                        x,
                        y,
                        width: s,
                        height: s,
                    },
                    "",
                    self.uv($icon),
                    $action,
                )
                .with_tooltip(t($tip));
                widgets.push(Box::new(b));
                x += s + gap;
            }};
        }

        // Transport: prev | play/pause | next
        btn!(
            "prev_frame",
            || EventResponse::Action(UiAction::PrevFrame),
            "toolbar.prev_frame"
        );
        let play_uv = if self.playing {
            self.uv("pause")
        } else {
            self.uv("resume")
        };
        let play_tip = if self.playing {
            "toolbar.stop"
        } else {
            "toolbar.play"
        };
        let play = IconButton::new(
            Rect {
                x,
                y,
                width: s,
                height: s,
            },
            "",
            play_uv,
            || EventResponse::Action(UiAction::TogglePlayPause),
        )
        .with_tooltip(t(play_tip));
        widgets.push(Box::new(play));
        x += s + gap;
        btn!(
            "next_frame",
            || EventResponse::Action(UiAction::NextFrame),
            "toolbar.next_frame"
        );

        x += gap * 2.0; // separator

        // Markers: boucle | out | scene
        btn!(
            "boucle",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::Boucle)),
            "toolbar.boucle"
        );
        btn!(
            "out",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::Out)),
            "toolbar.out"
        );
        btn!(
            "scene",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::SceneChange)),
            "toolbar.scene"
        );

        x += gap * 2.0; // separator

        // Quick-insert dropdowns: respirations | reactions
        btn!(
            "respirations",
            || EventResponse::Action(UiAction::OpenDropdown(
                widget::ToolbarDropdown::Respirations
            )),
            "toolbar.respirations"
        );
        btn!(
            "reactions",
            || EventResponse::Action(UiAction::OpenDropdown(widget::ToolbarDropdown::Reactions)),
            "toolbar.reactions"
        );

        x += gap * 2.0; // separator

        // Note
        btn!(
            "note",
            || EventResponse::Action(UiAction::AddNote),
            "toolbar.note"
        );

        x += gap * 2.0; // separator

        // Liaisons: left | right
        btn!(
            "liaison_left",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonLeft)),
            "toolbar.liaison_left"
        );
        btn!(
            "liaison_right",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonRight)),
            "toolbar.liaison_right"
        );

        x += gap * 2.0; // separator

        btn!(
            "karaoke",
            || EventResponse::Action(UiAction::ToggleKaraokeForSelection),
            "toolbar.karaoke"
        );
        let _ = x;

        // Right side: mute button + volume slider
        let slider_w = SLIDER_W;
        let slider_h = 24.0;
        let slider_x = tb.x + tb.width - slider_w - 8.0;
        let slider_y = tb.y + (TOOLBAR_HEIGHT - slider_h) / 2.0;
        let mute_x = slider_x - s - gap;
        let mute_icon = if self.volume <= 0.001 {
            "mute"
        } else {
            "sound"
        };
        let mute_tip = if self.volume <= 0.001 {
            "toolbar.unmute"
        } else {
            "toolbar.mute"
        };
        let mute = IconButton::new(
            Rect {
                x: mute_x,
                y,
                width: s,
                height: s,
            },
            "",
            self.uv(mute_icon),
            || EventResponse::Action(UiAction::ToggleMute),
        )
        .with_tooltip(t(mute_tip));
        widgets.push(Box::new(mute));
        let volume = Slider::new(
            Rect {
                x: slider_x,
                y: slider_y,
                width: slider_w,
                height: slider_h,
            },
            self.volume,
            |val| EventResponse::Action(UiAction::SetVolume(val)),
        )
        .with_tooltip(t("toolbar.volume"));
        widgets.push(Box::new(volume));

        widgets
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &Project,
        current_frame: i64,
        fps: f64,
    ) -> EventResponse {
        if let UiEvent::MouseMove { x, y } = event {
            self.cursor_pos = (*x, *y);
        }

        // File explorer is topmost and keeps parent modals alive underneath.
        if self.file_explorer_modal.is_some() {
            return self.handle_file_explorer_event(event);
        }

        // Proxy error modal blocks all input, including toast dismissal.
        if self.proxy_error_modal.is_some() {
            return self.handle_proxy_error_modal_event(event);
        }

        // Whats-new modal blocks all regular input while release notes are shown.
        if self.whats_new_modal.is_some() {
            return self.handle_whats_new_modal_event(event);
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

        // Settings modal intercepts all input
        if self.settings_modal.is_some() {
            return self.handle_settings_modal_event(event);
        }

        if self.project_settings_modal.is_some() {
            return self.handle_project_settings_modal_event(event);
        }

        // Studio warning modal
        if self.studio_warning_modal.is_some() {
            return self.handle_studio_warning_event(event);
        }

        // Save prompt modal (new project)
        if self.save_prompt_modal.is_some() {
            return self.handle_save_prompt_event(event);
        }

        // Export modal intercepts all input
        if self.export_modal.is_some() {
            return self.handle_export_modal_event(event);
        }

        // Voice actor creation modal intercepts all input
        if self.voice_actor_modal.is_some() {
            return self.handle_voice_actor_modal_event(event);
        }

        // Character rename modal intercepts all input
        if self.rename_character_modal.is_some() {
            return self.handle_rename_character_modal_event(event);
        }

        // Proxy modal intercepts all input
        if self.proxy_modal.is_some() {
            return self.handle_proxy_modal_event(event);
        }

        // Add server modal intercepts all input
        if self.add_server_modal.is_some() {
            return self.handle_add_server_event(event);
        }

        // Server browser intercepts all input
        if self.server_browser.is_some() {
            return self.handle_server_browser_event(event);
        }

        // Connect modal intercepts all input
        if self.connect_modal.is_some() {
            return self.handle_connect_modal_event(event);
        }

        if self.rythmo_state.context_menu.is_some() || matches!(event, UiEvent::ContextMenu { .. })
        {
            let response = rythmo::handle_context_menu_event(
                event,
                project,
                current_frame,
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

        // Progress bar scrubbing
        if self.total_frames > 0 {
            let hit = self.progress_bar_hit_rect();
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
                    return EventResponse::Consumed;
                }
                _ => {}
            }
        }

        // Rythmo zone events (lines, scroll, ctrl+click, etc.)
        let rythmo_response = rythmo::handle_rythmo_event(
            event,
            &self.layout.rythmo,
            project,
            current_frame,
            self.playing,
            fps,
            &mut self.rythmo_state,
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
                let frames = scroll_delta_to_frames(*delta, multiplier);
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

    pub fn toggle_play_pause(&mut self) {
        self.playing = !self.playing;
        self.toolbar_widgets = self.build_toolbar();
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
            || self.scrubbing
            || self.toasts.has_active()
            || self.rythmo_state.needs_animation_or_interaction()
    }

    pub fn needs_background_poll(&self) -> bool {
        self.server_browser.is_some()
            || self
                .file_explorer_modal
                .as_ref()
                .is_some_and(|modal| modal.needs_background_poll())
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        let mut deadline = self.rythmo_state.next_cursor_blink_deadline();
        if let Some(modal_deadline) = self
            .file_explorer_modal
            .as_ref()
            .and_then(|modal| modal.next_cursor_blink_deadline())
        {
            deadline = Some(deadline.map_or(modal_deadline, |current| current.min(modal_deadline)));
        }
        if let Some(modal_deadline) = self
            .rename_character_modal
            .as_ref()
            .and_then(|modal| modal.next_cursor_blink_deadline())
        {
            deadline = Some(deadline.map_or(modal_deadline, |current| current.min(modal_deadline)));
        }
        deadline
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
                ("↑", "resp.up"),
                ("↓", "resp.down"),
                ("(H)", "resp.h"),
                ("(HH)", "resp.hh"),
                ("(mH)", "resp.mh"),
                ("(mHH)", "resp.mhh"),
            ],
            widget::ToolbarDropdown::Reactions => vec![
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
                text: *text,
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
        self.rythmo_state.is_editing()
            || self
                .file_explorer_modal
                .as_ref()
                .is_some_and(|modal| modal.is_editing_text())
            || self.connect_modal.is_some()
            || self.export_modal.is_some()
            || self.proxy_modal.is_some()
            || self.proxy_error_modal.is_some()
            || self.voice_actor_modal.is_some()
            || self.rename_character_modal.is_some()
            || self.whats_new_modal.is_some()
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
            connect_modal::ConnectModalResult::Connect {
                ip,
                port,
                password,
                username,
                room_code,
            } => {
                self.connect_modal = None;
                EventResponse::Action(UiAction::NetworkConnect {
                    ip,
                    port,
                    password,
                    username,
                    room_code,
                })
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
            settings_modal::SettingsModalResult::Save {
                lang,
                rythmo_font,
                scroll_speed,
            } => {
                self.settings_modal = None;
                EventResponse::Action(UiAction::SaveSettings {
                    lang,
                    rythmo_font,
                    scroll_speed,
                })
            }
        }
    }

    fn handle_project_settings_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.project_settings_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            project_settings_modal::ProjectSettingsModalResult::Consumed => EventResponse::Consumed,
            project_settings_modal::ProjectSettingsModalResult::Close => {
                self.project_settings_modal = None;
                EventResponse::Consumed
            }
            project_settings_modal::ProjectSettingsModalResult::PickInstrumentalAudio => {
                EventResponse::Action(UiAction::PickProjectInstrumentalAudio)
            }
            project_settings_modal::ProjectSettingsModalResult::Save {
                instrumental_audio_path,
            } => {
                self.project_settings_modal = None;
                EventResponse::Action(UiAction::SaveProjectSettings {
                    instrumental_audio_path,
                })
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
            export_modal::ExportModalResult::Export {
                fps,
                br_scale,
                karaoke_text_scale,
                export_width,
                export_height,
                instrumental_audio_path,
                double_export_instrumental,
            } => {
                self.export_modal = None;
                EventResponse::Action(UiAction::StartExport {
                    fps,
                    br_scale,
                    karaoke_text_scale,
                    export_width,
                    export_height,
                    instrumental_audio_path,
                    double_export_instrumental,
                })
            }
            export_modal::ExportModalResult::PickInstrumentalAudio => {
                EventResponse::Action(UiAction::PickExportInstrumentalAudio)
            }
        }
    }

    fn handle_file_explorer_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.file_explorer_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            file_explorer_modal::FileExplorerResult::Consumed => EventResponse::Consumed,
            file_explorer_modal::FileExplorerResult::Close => {
                self.file_explorer_modal = None;
                EventResponse::Consumed
            }
            file_explorer_modal::FileExplorerResult::Clipboard(text) => {
                EventResponse::Action(UiAction::SetClipboard(text))
            }
            file_explorer_modal::FileExplorerResult::Selected { intent, path } => {
                self.file_explorer_modal = None;
                EventResponse::Action(UiAction::FilePickerSelected { intent, path })
            }
        }
    }

    fn handle_voice_actor_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.voice_actor_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            voice_actor_modal::VoiceActorModalResult::Consumed => EventResponse::Consumed,
            voice_actor_modal::VoiceActorModalResult::Close => {
                self.voice_actor_modal = None;
                EventResponse::Consumed
            }
            voice_actor_modal::VoiceActorModalResult::PickIcon => {
                EventResponse::Action(UiAction::PickVoiceActorIcon)
            }
            voice_actor_modal::VoiceActorModalResult::Clipboard(text) => {
                EventResponse::Action(UiAction::SetClipboard(text))
            }
            voice_actor_modal::VoiceActorModalResult::Create { name, icon_path } => {
                self.voice_actor_modal = None;
                EventResponse::Action(UiAction::CreateVoiceActor { name, icon_path })
            }
        }
    }

    fn handle_rename_character_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.rename_character_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            rename_character_modal::RenameCharacterModalResult::Consumed => EventResponse::Consumed,
            rename_character_modal::RenameCharacterModalResult::Close => {
                self.rename_character_modal = None;
                EventResponse::Consumed
            }
            rename_character_modal::RenameCharacterModalResult::Clipboard(text) => {
                EventResponse::Action(UiAction::SetClipboard(text))
            }
            rename_character_modal::RenameCharacterModalResult::Rename { old_name, new_name } => {
                self.rename_character_modal = None;
                EventResponse::Action(UiAction::RenameCharacter { old_name, new_name })
            }
        }
    }

    fn handle_proxy_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.proxy_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            proxy_modal::ProxyModalResult::Consumed => EventResponse::Consumed,
            proxy_modal::ProxyModalResult::Close => {
                self.proxy_modal = None;
                EventResponse::Consumed
            }
            proxy_modal::ProxyModalResult::Create { width, height, crf } => {
                self.proxy_modal = None;
                EventResponse::Action(UiAction::CreateProxy { width, height, crf })
            }
        }
    }

    fn handle_proxy_error_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.proxy_error_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            proxy_error_modal::ProxyErrorResult::Consumed => EventResponse::Consumed,
            proxy_error_modal::ProxyErrorResult::Close => {
                self.proxy_error_modal = None;
                EventResponse::Consumed
            }
        }
    }

    pub fn open_export_modal(&mut self, video_width: u32, video_height: u32) {
        self.export_modal = Some(export_modal::ExportModal::new(video_width, video_height));
    }

    pub fn open_file_explorer(&mut self, request: file_explorer_modal::FileExplorerRequest) {
        self.file_explorer_modal = Some(file_explorer_modal::FileExplorerModal::new(request));
    }

    pub fn poll_file_explorer(&mut self) -> bool {
        self.file_explorer_modal
            .as_mut()
            .is_some_and(|modal| modal.poll_background())
    }

    pub fn open_voice_actor_modal(&mut self) {
        self.voice_actor_modal = Some(voice_actor_modal::VoiceActorModal::new());
    }

    pub fn open_rename_character_modal(&mut self, characters: Vec<String>) {
        self.rename_character_modal = Some(rename_character_modal::RenameCharacterModal::new(
            characters,
        ));
    }

    pub fn set_voice_actor_modal_icon_path(&mut self, path: impl Into<String>) {
        if let Some(modal) = &mut self.voice_actor_modal {
            modal.set_icon_path(path);
        }
    }

    pub fn set_export_instrumental_audio_path(&mut self, path: impl Into<String>) {
        if let Some(modal) = &mut self.export_modal {
            modal.set_instrumental_audio_path(path);
        }
    }

    pub fn open_proxy_modal(&mut self, video_width: u32, video_height: u32) {
        self.proxy_modal = Some(proxy_modal::ProxyModal::new(video_width, video_height));
    }

    pub fn open_proxy_error_modal(&mut self, detail: impl Into<String>) {
        self.proxy_error_modal = Some(proxy_error_modal::ProxyErrorModal::new(detail));
    }

    pub fn open_whats_new_modal(&mut self, version: impl Into<String>, body: impl Into<String>) {
        self.whats_new_modal = Some(whats_new_modal::WhatsNewModal::new(version, body));
    }

    fn handle_whats_new_modal_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.whats_new_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            whats_new_modal::WhatsNewResult::Consumed => EventResponse::Consumed,
            whats_new_modal::WhatsNewResult::Close => {
                self.whats_new_modal = None;
                EventResponse::Consumed
            }
        }
    }

    fn handle_server_browser_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.server_browser {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            server_browser::BrowserResult::Consumed => EventResponse::Consumed,
            server_browser::BrowserResult::Close => {
                self.server_browser = None;
                EventResponse::Consumed
            }
            server_browser::BrowserResult::CreateRoom { ip, port } => {
                self.server_browser = None;
                EventResponse::Action(UiAction::OpenConnectModal {
                    ip,
                    port,
                    join: false,
                })
            }
            server_browser::BrowserResult::JoinRoom { ip, port } => {
                self.server_browser = None;
                EventResponse::Action(UiAction::OpenConnectModal {
                    ip,
                    port,
                    join: true,
                })
            }
            server_browser::BrowserResult::AddServer => {
                EventResponse::Action(UiAction::OpenAddServerModal)
            }
            server_browser::BrowserResult::RemoveServer(i) => {
                EventResponse::Action(UiAction::RemoveServer(i))
            }
            server_browser::BrowserResult::Refresh => {
                EventResponse::Action(UiAction::RefreshServers)
            }
        }
    }

    fn handle_add_server_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.add_server_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            server_browser::AddServerResult::Consumed => EventResponse::Consumed,
            server_browser::AddServerResult::Close => {
                self.add_server_modal = None;
                EventResponse::Consumed
            }
            server_browser::AddServerResult::Add { ip, port } => {
                self.add_server_modal = None;
                EventResponse::Action(UiAction::AddServer { ip, port })
            }
        }
    }

    fn handle_save_prompt_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.save_prompt_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            save_prompt_modal::SavePromptResult::Consumed => EventResponse::Consumed,
            save_prompt_modal::SavePromptResult::Save => {
                self.save_prompt_modal = None;
                EventResponse::Action(UiAction::NewProjectSave)
            }
            save_prompt_modal::SavePromptResult::Discard => {
                self.save_prompt_modal = None;
                EventResponse::Action(UiAction::NewProjectDiscard)
            }
            save_prompt_modal::SavePromptResult::Cancel => {
                self.save_prompt_modal = None;
                EventResponse::Consumed
            }
        }
    }

    pub fn open_save_prompt(&mut self) {
        self.save_prompt_modal = Some(save_prompt_modal::SavePromptModal::new());
    }

    fn handle_studio_warning_event(&mut self, event: &UiEvent) -> EventResponse {
        let modal = match &mut self.studio_warning_modal {
            Some(m) => m,
            None => return EventResponse::Ignored,
        };
        match modal.handle_event(event, self.screen_w, self.screen_h) {
            studio_warning_modal::StudioWarningResult::Consumed => EventResponse::Consumed,
            studio_warning_modal::StudioWarningResult::Confirm => {
                self.studio_warning_modal = None;
                EventResponse::Action(UiAction::EnterStudioMode)
            }
            studio_warning_modal::StudioWarningResult::Cancel => {
                self.studio_warning_modal = None;
                EventResponse::Consumed
            }
        }
    }

    pub fn open_studio_warning(&mut self) {
        self.studio_warning_modal = Some(studio_warning_modal::StudioWarningModal::new());
    }

    pub fn open_server_browser(&mut self) {
        self.server_browser = Some(server_browser::ServerBrowserModal::new());
    }

    pub fn open_add_server_modal(&mut self) {
        self.add_server_modal = Some(server_browser::AddServerModal::new());
    }

    pub fn server_browser_mut(&mut self) -> Option<&mut server_browser::ServerBrowserModal> {
        self.server_browser.as_mut()
    }

    pub fn open_connect_modal(&mut self, ip: &str, port: u16, join: bool) {
        self.connect_modal = Some(connect_modal::ConnectModal::new_with_server(ip, port, join));
    }

    pub fn open_settings_modal(&mut self, fonts: Vec<String>) {
        self.settings_modal = Some(settings_modal::SettingsModal::new(fonts));
    }

    pub fn open_project_settings_modal(&mut self, instrumental_audio_path: Option<String>) {
        self.project_settings_modal = Some(project_settings_modal::ProjectSettingsModal::new(
            instrumental_audio_path,
        ));
    }

    pub fn set_project_instrumental_audio_path(&mut self, path: impl Into<String>) {
        if let Some(modal) = &mut self.project_settings_modal {
            modal.set_instrumental_audio_path(path);
        }
    }

    pub fn close_project_settings_modal(&mut self) {
        self.project_settings_modal = None;
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
        self.topbar_widgets = Self::build_topbar(
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
        current_frame: i64,
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
                    )
                    .and_then(|segments| renderer.cursor_pos_from_segments(&segments, ratio))
                    .or_else(|| {
                        rythmo::segmented_cursor_index_for_line_at_ratio(
                            line,
                            self.rythmo_state.syllable_drag.as_ref(),
                            &lang,
                            self.playing,
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
            current_frame,
            fps,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
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
            current_frame,
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

        // Markers
        let mut liaison_icons: Vec<IconInstance> = Vec::new();
        rythmo::render_markers(
            &self.layout.rythmo,
            project,
            current_frame,
            &mut quads,
            &mut labels,
            &mut liaison_icons,
            self.uv("liaison_left"),
            self.uv("liaison_right"),
        );
        icons.extend(liaison_icons);

        // Prepare stretched text textures
        let stretched_quads = renderer.prepare_stretched_texts(device, queue, &stretched_texts);

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
            current_frame,
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

        // Settings modal
        if let Some(modal) = &self.settings_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        if let Some(modal) = &self.project_settings_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Connect modal
        if let Some(modal) = &self.connect_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Server browser
        if let Some(modal) = &self.server_browser {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Add server modal (on top of browser)
        if let Some(modal) = &self.add_server_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Export modal
        if let Some(modal) = &self.export_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        if let Some(modal) = &self.voice_actor_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        if let Some(modal) = &self.rename_character_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Proxy modal
        if let Some(modal) = &self.proxy_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Save prompt modal
        if let Some(modal) = &self.save_prompt_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Studio warning modal
        if let Some(modal) = &self.studio_warning_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
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
        }

        // Toasts
        self.toasts.render(
            &mut overlay_quads,
            &mut labels,
            self.screen_w,
            self.screen_h,
        );

        // Whats-new modal is rendered above toasts so the release notes stay readable.
        if let Some(modal) = &self.whats_new_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Proxy error modal is rendered last so it stays above toasts and progress.
        if let Some(modal) = &self.proxy_error_modal {
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // File explorer is rendered last so it stays above parent modals.
        if let Some(modal) = &self.file_explorer_modal {
            // Text is rendered in one final pass, so clear underlying labels here.
            // Otherwise parent-modal text can appear above the picker card.
            labels.clear();
            modal.render(
                &mut overlay_quads,
                &mut labels,
                self.screen_w,
                self.screen_h,
            );
        }

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
        );
    }

    fn render_zones<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        project: &Project,
        current_frame: i64,
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
            let pb = self.progress_bar_rect();
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
        quads.extend(rythmo::render_rythmo_base(
            &l.rythmo,
            project,
            current_frame,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
            self.playing,
            fps,
            &self.rythmo_state,
        ));

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
        current_frame: i64,
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
            current_frame,
            fps,
            &self.rythmo_state,
            &mut quads,
            &mut labels,
            &mut stretched_texts,
            &mut actor_icon_draws,
        );

        // Prepare stretched text textures
        let stretched_quads = renderer.prepare_stretched_texts(device, queue, &stretched_texts);
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
            &[],
            &base_textured,
            &[],
            &[], // no post_texture_quads
        );
    }
}
