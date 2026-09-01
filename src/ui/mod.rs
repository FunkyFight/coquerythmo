//! Main application UI shell.
//!
//! A few handlers retain explicit context parameters while the workspace
//! migration is completed; the signatures keep event flow visible.
#![allow(clippy::too_many_arguments)]

pub use crate::application::command::ToolMode;
pub mod actor_icon_cache;
pub mod automation;
pub mod color_picker;
pub mod comic_dubs_settings_modal;
pub mod comic_dubs_workspace;
pub mod connect_modal;
pub mod context_menu;
pub mod dropdown;
pub mod export_modal;
pub mod focus;
pub mod font_dropdown;
pub mod icon_button;
pub mod icons;
pub mod interactive;
pub mod invitation_modal;
pub mod language_modal;
pub mod layout;
pub mod license_badge;
pub mod microphone_modal;
pub mod modal_host;
pub mod pricing_license_modal;
pub mod pricing_page;
pub mod pricing_plan_modal;
pub mod primitives;
pub mod project_settings_modal;
pub mod project_transfer_modal;
pub mod proxy_error_modal;
pub mod proxy_modal;
pub mod recording_workspace;
pub mod rename_character_modal;
pub mod renderer;
pub mod save_prompt_modal;
pub mod server_browser;
pub mod settings_modal;
pub mod shell;
pub mod shortcut_panel;
pub mod side_panel;
pub mod slider;
pub mod tab_button;
pub mod task_row;
pub mod text_button;
pub mod text_input;
pub mod theme;
pub mod toast;
pub mod tooltip;
pub mod voice_actor_modal;
pub mod voicelines_workspace;
pub mod whats_new_modal;

use layout::{
    Layout, PROPS_DEFAULT_W, PROPS_DRAG_ZONE, PROPS_MAX_W, PROPS_MIN_W, RYTHMO_MIN_H, TABBAR_H,
    TOOLBAR_H, TOPBAR_H, VIDEO_MIN_H,
};
use primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign, Widget,
};
use renderer::{StretchedText, UiLayer, UiLayerBatch};
use tooltip::{LintTooltipState, TooltipState};

use crate::application::workspace_service::WorkspaceId;
use crate::i18n::t;
use crate::network::NetworkMember;
use crate::project::Project;
use crate::recording::{CaptureState, RecordingProject};
use crate::render_index::ProjectRenderIndex;

use self::actor_icon_cache::ActorIconCache;
use self::focus::{AccessibleNode, FocusId, FocusManager};
use self::icons::IconAtlas;
use self::modal_host::ModalHost;
use self::project_transfer_modal::{ProjectTransferAction, ProjectTransferModal};
use self::recording_workspace::{
    RecordingControl, RecordingLayout, RecordingPage, RecordingRole, RecordingScene,
    RecordingTextEditResult, RecordingWorkspaceUi, TRACK_ROW_H,
};
use self::renderer::UiRenderer;
use crate::workspaces::rythmo::view as rythmo;

use theme::*;

pub struct ProjectLoadUi {
    pub label: String,
    pub phase: String,
    pub progress: f32,
    /// Index into `task_row::LOADING_STEP_KEYS` for the expanded sub-steps.
    pub stage_index: usize,
}

pub struct Ui {
    topbar_widgets: Vec<Box<dyn Widget>>,
    tab_widgets: Vec<Box<dyn Widget>>,
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
    recording_video_split: f32,
    recording_rythmo_split: f32,
    recording_assets_split: f32,
    dragging_recording_split: Option<RecordingSplitHandle>,
    tooltip: Option<TooltipState>,
    /// Contextual shortcut hints rendered in the bottom-left corner.
    shortcut_panel: shortcut_panel::ShortcutPanelState,
    /// Bridged from State: the internal line clipboard holds an entry.
    pub line_clipboard_available: bool,
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
    pub has_video: bool,
    pub current_frame: i64,
    pub total_frames: i64,
    pub scrubbing: bool,
    pub sync_overlay: Option<String>,
    pub sync_progress: f32,
    pub project_transfer_modal: Option<ProjectTransferModal>,
    pub active_mode: Option<ToolMode>,
    pub brush_color: [f32; 4],
    pub brush_radius_index: usize,
    pub erasing: bool,
    pub brush_picking: bool,
    pub(crate) drawing_overlay_cache: Option<DrawingOverlayCache>,
    whats_new_thumbnail_texture: Option<WhatsNewThumbnailTexture>,
    pub brush_color_presets: [[f32; 4]; 8],
    pub brush_color_preset_index: usize,
    pub toasts: toast::ToastManager,
    /// Expanded state of the background task rows (top-center card).
    pub task_rows: task_row::TaskRowsState,
    /// Active bande rythmo import. Set by State while a background parse runs;
    /// surfaced as a task row with the current load stage.
    pub loading_project: Option<ProjectLoadUi>,
    automation_editor: automation::AutomationEditor,
    side_panel: side_panel::SidePanel,
    focus: FocusManager,
    active_workspace: WorkspaceId,
    recording_ui: RecordingWorkspaceUi,
    recording_layout: RecordingLayout,
    recording_scene: RecordingScene,
    recording_capture_active: bool,
    recording_capture_view: bool,
    recording_capture_rythmo_split: Option<f32>,
    recording_daw_detached: bool,
    recording_daw_layout: RecordingLayout,
    recording_daw_scene: RecordingScene,
    recording_daw_cursor: (f32, f32),
    recording_daw_toolbar_widgets: Vec<Box<dyn Widget>>,
    voicelines_ui: voicelines_workspace::VoicelinesWorkspaceUi,
    voicelines_layout: voicelines_workspace::VoicelinesLayout,
    voicelines_scene: voicelines_workspace::VoicelinesScene,
    comic_dubs_ui: comic_dubs_workspace::ComicDubsWorkspaceUi,
    comic_dubs_layout: comic_dubs_workspace::ComicDubsLayout,
    comic_dubs_scene: comic_dubs_workspace::ComicDubsScene,
    comic_dubs_texture: Option<ComicDubsTexture>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordingSplitHandle {
    Video,
    Rythmo,
    Assets,
}

fn uses_recording_capture_view(
    page: RecordingPage,
    role: RecordingRole,
    capture_active: bool,
) -> bool {
    capture_active || (page == RecordingPage::Timeline && matches!(role, RecordingRole::Actor))
}

fn shows_rythmo(workspace: WorkspaceId, recording_page: RecordingPage, zone: Rect) -> bool {
    workspace == WorkspaceId::Rythmo
        || (workspace == WorkspaceId::Recording
            && recording_page == RecordingPage::Timeline
            && zone.width > 0.0
            && zone.height > 0.0)
}

fn playback_progress(current_frame: i64, total_frames: i64) -> f32 {
    match total_frames {
        ..=0 => 0.0,
        1 => 1.0,
        _ => (current_frame as f32 / (total_frames - 1) as f32).clamp(0.0, 1.0),
    }
}

struct WhatsNewThumbnailTexture {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

struct ComicDubsTexture {
    page_id: crate::comic_dubs::PageId,
    path: std::path::PathBuf,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

fn recording_drop_target(
    layout: RecordingLayout,
    scene: &RecordingScene,
    ui: &RecordingWorkspaceUi,
    x: f32,
    y: f32,
) -> Option<(crate::recording::AudioTrackId, i64)> {
    let body = layout.track_body?;
    if !body.contains(x, y) {
        return None;
    }
    let row = ((y - body.y) / TRACK_ROW_H).floor() as usize;
    let track_id = scene
        .controls
        .iter()
        .filter_map(|control| match control.control {
            RecordingControl::TrackMute(track_id) => Some(track_id),
            _ => None,
        })
        .nth(row)?;
    let start_frame =
        (ui.view_start_frame + ((x - body.x) / ui.pixels_per_frame) as f64).round() as i64;
    Some((track_id, start_frame.max(0)))
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
            "detection/labial",
            "detection/semi_labial",
            "detection/mouth_open",
            "detection/mouth_closed",
            "detection/teeth_visible",
            "detection/breath",
            "detection/reaction",
            "detection/th",
            "detection/neutral",
            "detection/pucker",
            "detection/rhubarb_lips/AA",
            "detection/rhubarb_lips/AO_ER",
            "detection/rhubarb_lips/EH_AE",
            "detection/rhubarb_lips/F_V",
            "detection/rhubarb_lips/K_S_T_EE",
            "detection/rhubarb_lips/L",
            "detection/rhubarb_lips/P_B_M",
            "detection/rhubarb_lips/UW_OW_W",
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
            topbar_widgets: shell::build_topbar(
                false,
                false,
                sw,
                settings_uv,
                project_uv,
                WorkspaceId::Rythmo,
                true,
                false,
                Vec::new(),
                None,
            ),
            tab_widgets: vec![],
            toolbar_widgets: vec![],
            layout,
            screen_w: sw,
            screen_h: sh,
            props_visible: false,
            props_width: PROPS_DEFAULT_W,
            dragging_props: false,
            video_split,
            dragging_split: None,
            recording_video_split: 0.48,
            recording_rythmo_split: 0.34,
            recording_assets_split: 0.23,
            dragging_recording_split: None,
            tooltip: None,
            shortcut_panel: shortcut_panel::ShortcutPanelState::new(),
            line_clipboard_available: false,
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
            sync_overlay: None,
            sync_progress: 0.0,
            project_transfer_modal: None,
            has_video: false,
            current_frame: 0,
            total_frames: 0,
            scrubbing: false,
            toasts: toast::ToastManager::new(),
            task_rows: task_row::TaskRowsState::default(),
            loading_project: None,
            automation_editor: automation::AutomationEditor::default(),
            side_panel: side_panel::SidePanel::default(),
            focus: FocusManager::default(),
            active_workspace: WorkspaceId::Rythmo,
            recording_ui: RecordingWorkspaceUi::default(),
            recording_layout: RecordingLayout::choice(Rect {
                x: 0.0,
                y: TOPBAR_H + TABBAR_H,
                width: sw,
                height: (sh - TOPBAR_H - TABBAR_H).max(0.0),
            }),
            recording_scene: RecordingScene::default(),
            recording_capture_active: false,
            recording_capture_view: false,
            recording_capture_rythmo_split: None,
            recording_daw_detached: false,
            recording_daw_layout: RecordingLayout::choice(Rect::default()),
            recording_daw_scene: RecordingScene::default(),
            recording_daw_cursor: (0.0, 0.0),
            recording_daw_toolbar_widgets: Vec::new(),
            voicelines_ui: voicelines_workspace::VoicelinesWorkspaceUi::default(),
            voicelines_layout: voicelines_workspace::VoicelinesLayout::compute(Rect {
                x: 0.0,
                y: TOPBAR_H + TABBAR_H,
                width: sw,
                height: (sh - TOPBAR_H - TABBAR_H).max(0.0),
            }),
            voicelines_scene: voicelines_workspace::VoicelinesScene::default(),
            comic_dubs_ui: comic_dubs_workspace::ComicDubsWorkspaceUi::default(),
            comic_dubs_layout: comic_dubs_workspace::ComicDubsLayout::compute(Rect {
                x: 0.0,
                y: TOPBAR_H + TABBAR_H,
                width: sw,
                height: (sh - TOPBAR_H - TABBAR_H).max(0.0),
            }),
            comic_dubs_scene: comic_dubs_workspace::ComicDubsScene::default(),
            comic_dubs_texture: None,
            active_mode: Some(ToolMode::Select),
            brush_color: [1.0, 1.0, 1.0, 1.0],
            brush_radius_index: 0,
            erasing: false,
            brush_picking: false,
            drawing_overlay_cache: None,
            whats_new_thumbnail_texture: None,
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
        ui.tab_widgets = shell::build_workspace_tabs(&ui.layout, ui.active_workspace);
        ui.toolbar_widgets = shell::build_toolbar(ui.toolbar_build_context());
        ui.refresh_root_focus_nodes();
        ui
    }

    fn refresh_root_focus_nodes(&mut self) {
        let mut nodes = Vec::new();
        if !self.recording_capture_view {
            for (index, widget) in self.topbar_widgets.iter().enumerate() {
                let label = widget
                    .accessible_label()
                    .map(str::to_string)
                    .or_else(|| widget.labels().first().map(|label| label.text.to_string()))
                    .unwrap_or_else(|| format!("Contrôle {}", index + 1));
                nodes.push(
                    AccessibleNode::focusable(
                        format!("topbar.{index}"),
                        widget.accessible_role(),
                        label,
                    )
                    .with_selected(widget.accessible_selected()),
                );
            }
            for (index, widget) in self.tab_widgets.iter().enumerate() {
                let label = widget
                    .accessible_label()
                    .map(str::to_string)
                    .or_else(|| widget.labels().first().map(|label| label.text.to_string()))
                    .unwrap_or_else(|| t("accessibility.control").to_string());
                nodes.push(
                    AccessibleNode::focusable(
                        format!("tabs.{index}"),
                        widget.accessible_role(),
                        label,
                    )
                    .with_selected(widget.accessible_selected()),
                );
            }
            for (index, widget) in self.toolbar_widgets.iter().enumerate() {
                let label = widget
                    .accessible_label()
                    .map(str::to_string)
                    .or_else(|| widget.labels().first().map(|label| label.text.to_string()))
                    .unwrap_or_else(|| format!("Outil {}", index + 1));
                nodes.push(
                    AccessibleNode::focusable(
                        format!("toolbar.{index}"),
                        widget.accessible_role(),
                        label,
                    )
                    .with_selected(widget.accessible_selected()),
                );
            }
        }
        if self.active_workspace == WorkspaceId::Recording {
            nodes.extend(self.recording_scene.controls.iter().map(|control| {
                let mut node = AccessibleNode::focusable(
                    control.control.stable_id(),
                    control.role,
                    control.label.clone(),
                )
                .with_selected(Some(control.selected));
                node.value = control.value.clone();
                node.enabled = control.enabled;
                node
            }));
        } else if self.active_workspace == WorkspaceId::Voicelines {
            nodes.extend(self.voicelines_scene.controls.iter().map(|control| {
                AccessibleNode::focusable(control.id.clone(), control.role, control.label.clone())
                    .with_selected(Some(control.selected))
            }));
        } else if self.active_workspace == WorkspaceId::ComicDubs {
            nodes.extend(self.comic_dubs_scene.controls.iter().map(|control| {
                AccessibleNode::focusable(control.id.clone(), control.role, control.label.clone())
                    .with_selected(Some(control.selected))
            }));
        }
        self.focus.replace_root(nodes);
    }

    fn focused_widget_mut(&mut self) -> Option<&mut Box<dyn Widget>> {
        let id = self.focus.current_id()?.0.clone();
        let (group, index) = id.split_once('.')?;
        let index = index.parse::<usize>().ok()?;
        match group {
            "topbar" => self.topbar_widgets.get_mut(index),
            "tabs" => self.tab_widgets.get_mut(index),
            "toolbar" => self.toolbar_widgets.get_mut(index),
            _ => None,
        }
    }

    fn focused_widget(&self) -> Option<&dyn Widget> {
        let id = self.focus.current_id()?.0.as_str();
        let (group, index) = id.split_once('.')?;
        let index = index.parse::<usize>().ok()?;
        match group {
            "topbar" => self.topbar_widgets.get(index).map(|widget| widget.as_ref()),
            "tabs" => self.tab_widgets.get(index).map(|widget| widget.as_ref()),
            "toolbar" => self
                .toolbar_widgets
                .get(index)
                .map(|widget| widget.as_ref()),
            _ => None,
        }
    }

    pub fn has_keyboard_focus(&self) -> bool {
        self.focus.current_id().is_some()
    }

    /// Ordered shortcut contexts for the current UI state. This is the single
    /// source of truth shared by the event loop (to resolve a keystroke) and
    /// the bottom-left shortcut panel (to list what is available).
    pub fn shortcut_contexts(&self) -> Vec<crate::input::context::InputContext> {
        use crate::input::context::InputContext;
        let mut contexts = Vec::new();
        if self.modal_host.captures_input() {
            contexts.push(InputContext::Modal);
        } else if self.rythmo_state.is_editing() {
            contexts.push(InputContext::TextEditing);
        } else if self.has_keyboard_focus() {
            if self.active_workspace == WorkspaceId::Recording {
                contexts.push(InputContext::Recording);
            }
            contexts.push(InputContext::MainWindow);
            contexts.push(InputContext::Global);
        } else if !self.is_editing_text() {
            match self.active_workspace {
                WorkspaceId::Rythmo
                | WorkspaceId::Voicelines
                | WorkspaceId::ComicDubs => contexts.push(InputContext::Workspace),
                WorkspaceId::Recording => contexts.push(InputContext::Recording),
            }
            contexts.push(InputContext::Global);
        }
        contexts
    }

    pub fn focused_workspace_tab(&self) -> bool {
        self.focus
            .current_id()
            .is_some_and(|id| id.0.starts_with("tabs."))
    }

    pub fn is_sensitive_text_context(&self) -> bool {
        self.modal_host.is_sensitive_text_context()
    }

    fn focus_widget_at(&mut self, x: f32, y: f32) {
        if let Some((index, _)) = self
            .topbar_widgets
            .iter()
            .enumerate()
            .find(|(_, widget)| widget.bounds().contains(x, y))
        {
            self.focus.focus(&FocusId::new(format!("topbar.{index}")));
            return;
        }
        if let Some((index, _)) = self
            .tab_widgets
            .iter()
            .enumerate()
            .find(|(_, widget)| widget.bounds().contains(x, y))
        {
            self.focus.focus(&FocusId::new(format!("tabs.{index}")));
            return;
        }
        if let Some((index, _)) = self
            .toolbar_widgets
            .iter()
            .enumerate()
            .find(|(_, widget)| widget.bounds().contains(x, y))
        {
            self.focus.focus(&FocusId::new(format!("toolbar.{index}")));
            return;
        }
        if self.active_workspace == WorkspaceId::Recording {
            if let Some(control) = self
                .recording_scene
                .controls
                .iter()
                .find(|control| control.bounds.contains(x, y))
            {
                self.focus.focus(&FocusId::new(control.control.stable_id()));
                return;
            }
        } else if self.active_workspace == WorkspaceId::ComicDubs {
            if let Some(control) = self
                .comic_dubs_scene
                .controls
                .iter()
                .find(|control| control.bounds.contains(x, y))
            {
                self.focus.focus(&FocusId::new(control.id.clone()));
                return;
            }
        }
        self.focus.clear();
    }

    fn rebuild_layout(&mut self) {
        self.layout = Layout::compute(
            self.screen_w,
            self.screen_h,
            self.props_visible,
            self.props_width,
            self.video_split,
        );
        self.voicelines_layout =
            voicelines_workspace::VoicelinesLayout::compute(self.workspace_content_rect());
        self.comic_dubs_layout =
            comic_dubs_workspace::ComicDubsLayout::compute(self.workspace_content_rect());
        self.tab_widgets = shell::build_workspace_tabs(&self.layout, self.active_workspace);
        self.toolbar_widgets = if self.active_workspace == WorkspaceId::Recording
            && (self.recording_ui.page == RecordingPage::Choice
                || self.recording_capture_view
                || self.recording_daw_detached)
        {
            Vec::new()
        } else {
            shell::build_toolbar(self.toolbar_build_context())
        };
        self.refresh_root_focus_nodes();
    }

    pub fn rebuild_topbar(&mut self, in_room: bool) {
        self.network_in_room = in_room;
        self.voicelines_ui.set_recording_transfer_disabled(in_room);
        let selected_regions = self.voicelines_ui.selected_regions().to_vec();
        let selected_bubble = self.comic_dubs_ui.selected_bubble();
        self.topbar_widgets = shell::build_topbar(
            in_room,
            self.has_video,
            self.screen_w,
            self.uv("settings"),
            self.uv("project"),
            self.active_workspace,
            self.recording_daw_enabled(),
            self.recording_actor_requests_enabled(),
            selected_regions,
            selected_bubble,
        );
        self.refresh_root_focus_nodes();
    }

    pub fn rebuild_toolbar(&mut self) {
        self.toolbar_widgets = if self.active_workspace == WorkspaceId::Recording
            && (self.recording_ui.page == RecordingPage::Choice
                || self.recording_capture_view
                || self.recording_daw_detached)
        {
            Vec::new()
        } else {
            shell::build_toolbar(self.toolbar_build_context())
        };
        if self.recording_daw_detached {
            self.rebuild_recording_daw_toolbar();
        }
        self.refresh_root_focus_nodes();
    }

    pub fn active_workspace(&self) -> WorkspaceId {
        self.active_workspace
    }

    pub fn set_recording_daw_detached(&mut self, detached: bool) {
        self.recording_daw_detached = detached;
        if !detached {
            self.recording_daw_scene = RecordingScene::default();
            self.recording_daw_toolbar_widgets.clear();
            self.recording_ui.dragging_asset = None;
            self.recording_ui.dragging_clip = None;
            self.recording_ui.dragging_track_volume = None;
        }
        if self.active_workspace == WorkspaceId::Recording {
            self.rebuild_layout();
            self.rebuild_toolbar();
        }
    }

    pub fn set_active_workspace(&mut self, workspace: WorkspaceId) {
        self.active_workspace = workspace;
        if workspace != WorkspaceId::Rythmo {
            crate::detection_foreground::clear();
        }
        self.active_dropdown = None;
        self.tooltip = None;
        self.automation_editor.close();
        self.side_panel.close();
        self.props_visible = false;
        self.brush_picking = false;
        self.rythmo_state.cancel_active_interaction();
        let selected_regions = self.voicelines_ui.selected_regions().to_vec();
        let selected_bubble = self.comic_dubs_ui.selected_bubble();
        self.topbar_widgets = shell::build_topbar(
            self.network_in_room,
            self.has_video,
            self.screen_w,
            self.uv("settings"),
            self.uv("project"),
            self.active_workspace,
            self.recording_daw_enabled(),
            self.recording_actor_requests_enabled(),
            selected_regions,
            selected_bubble,
        );
        self.rebuild_layout();
    }

    fn toolbar_build_context(&self) -> shell::ToolbarBuildContext<'_> {
        shell::ToolbarBuildContext {
            toolbar: self.active_toolbar_rect(),
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
            editable: self.active_workspace == WorkspaceId::Rythmo,
            playback_enabled: self.active_workspace != WorkspaceId::Recording
                || self.recording_playback_controls_enabled(),
        }
    }

    fn workspace_content_rect(&self) -> Rect {
        let top = TOPBAR_H + TABBAR_H;
        Rect {
            x: 0.0,
            y: top,
            width: self.screen_w,
            height: (self.screen_h - top).max(0.0),
        }
    }

    fn active_toolbar_rect(&self) -> Rect {
        match self.active_workspace {
            WorkspaceId::Recording => self.recording_layout.toolbar.unwrap_or(self.layout.toolbar),
            WorkspaceId::Voicelines => self.voicelines_layout.toolbar,
            WorkspaceId::ComicDubs => self.comic_dubs_layout.toolbar,
            WorkspaceId::Rythmo => self.layout.toolbar,
        }
    }

    fn active_rythmo_rect(&self) -> Rect {
        if self.active_workspace == WorkspaceId::Recording {
            self.recording_layout.rythmo
        } else {
            self.layout.rythmo
        }
    }

    pub fn video_preview_rect(&self) -> Rect {
        if self.active_workspace == WorkspaceId::Recording
            && self.recording_ui.page == RecordingPage::Timeline
        {
            self.recording_layout.video
        } else {
            self.layout.video_preview
        }
    }

    fn recording_rythmo_min_height(render_index: &ProjectRenderIndex) -> f32 {
        let used = render_index.used_track_indices();
        let karaoke = render_index.karaoke_tracks();
        let emotion = render_index.text_emotion_tracks();
        let used_tracks = used.len();
        let karaoke_tracks = used
            .iter()
            .filter(|track| karaoke.get(**track).copied().unwrap_or(false))
            .count();
        let emotion_tracks = used
            .iter()
            .filter(|track| {
                !karaoke.get(**track).copied().unwrap_or(false)
                    && emotion.get(**track).copied().unwrap_or(false)
            })
            .count();
        let row_height = 58.0;
        let track_header = crate::constants::VOICE_ACTOR_DISPLAY_ICON_SIZE;
        let track_body = used_tracks as f32 * row_height
            + karaoke_tracks as f32
                * (row_height + crate::rythmo_layout::karaoke_stack_gap(row_height * 2.0, 1.0))
            + emotion_tracks as f32
                * (row_height + crate::rythmo_layout::karaoke_stack_gap(row_height, 1.0));
        (crate::constants::RULER_HEIGHT
            + used_tracks as f32 * (track_header + crate::constants::BADGE_GAP)
            + track_body)
            .max(100.0)
    }

    pub fn sync_recording_scene(
        &mut self,
        render_index: &ProjectRenderIndex,
        project: &RecordingProject,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        capture: Option<&CaptureState>,
        participants: &[NetworkMember],
        control_owner_id: Option<&str>,
        current_frame: f64,
        countdown_seconds: Option<u32>,
    ) {
        let capture_active = capture.is_some_and(|state| {
            matches!(
                state,
                CaptureState::Countdown { .. }
                    | CaptureState::Capturing { .. }
                    | CaptureState::Finalizing { .. }
            )
        });
        let capture_view = uses_recording_capture_view(
            self.recording_ui.page,
            self.recording_ui.role,
            capture_active,
        );
        let next_layout = if capture_view {
            RecordingLayout::capturing(
                self.screen_w,
                self.screen_h,
                Self::recording_rythmo_min_height(render_index),
                self.recording_capture_rythmo_split,
            )
        } else if self.recording_ui.page == RecordingPage::Choice {
            RecordingLayout::choice(self.workspace_content_rect())
        } else if self.recording_daw_detached {
            RecordingLayout::detached_main(
                self.workspace_content_rect(),
                self.recording_video_split,
            )
        } else {
            RecordingLayout::timeline_with_splits_and_rythmo_min(
                self.workspace_content_rect(),
                self.recording_ui.role.is_online(),
                self.recording_video_split,
                self.recording_rythmo_split,
                self.recording_assets_split,
                Self::recording_rythmo_min_height(render_index),
            )
        };
        let chrome_changed = self.recording_capture_view != capture_view
            || self.recording_capture_active != capture_active
            || self.recording_layout != next_layout;
        self.recording_capture_active = capture_active;
        self.recording_capture_view = capture_view;
        self.recording_layout = next_layout;
        self.recording_ui.sync_track_count(project.tracks().count());
        self.recording_ui.sync_asset_content(project);
        self.recording_ui.sync_view_to_playhead(
            self.recording_layout,
            current_frame,
            project.timeline_fps(),
            scroll_speed,
            reading_bar_offset_percent,
        );
        self.recording_scene = self.recording_ui.scene(
            self.recording_layout,
            project,
            capture,
            participants,
            control_owner_id,
            current_frame,
            countdown_seconds,
        );
        if chrome_changed {
            self.rebuild_toolbar();
        } else {
            self.refresh_root_focus_nodes();
        }
    }

    pub fn sync_recording_daw_scene(
        &mut self,
        width: f32,
        height: f32,
        project: &RecordingProject,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        capture: Option<&CaptureState>,
        participants: &[NetworkMember],
        control_owner_id: Option<&str>,
        current_frame: f64,
        countdown_seconds: Option<u32>,
    ) {
        let next_layout = RecordingLayout::daw(
            Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            self.recording_assets_split,
        );
        let toolbar_changed = self.recording_daw_layout.toolbar != next_layout.toolbar;
        self.recording_daw_layout = next_layout;
        if toolbar_changed || self.recording_daw_toolbar_widgets.is_empty() {
            self.rebuild_recording_daw_toolbar();
        }
        self.recording_ui.sync_track_count(project.tracks().count());
        self.recording_ui.sync_asset_content(project);
        self.recording_ui.sync_view_to_playhead(
            self.recording_daw_layout,
            current_frame,
            project.timeline_fps(),
            scroll_speed,
            reading_bar_offset_percent,
        );
        self.recording_daw_scene = self.recording_ui.scene(
            self.recording_daw_layout,
            project,
            capture,
            participants,
            control_owner_id,
            current_frame,
            countdown_seconds,
        );
    }

    fn rebuild_recording_daw_toolbar(&mut self) {
        let Some(toolbar) = self.recording_daw_layout.toolbar else {
            self.recording_daw_toolbar_widgets.clear();
            return;
        };
        self.recording_daw_toolbar_widgets = shell::build_toolbar(shell::ToolbarBuildContext {
            toolbar,
            icon_uvs: &self.icon_uvs,
            playing: self.playing,
            volume: self.volume,
            active_mode: self.active_mode,
            brush_color: self.brush_color,
            brush_radius_index: self.brush_radius_index,
            brush_color_preset_index: self.brush_color_preset_index,
            erasing: self.erasing,
            brush_color_presets: &self.brush_color_presets,
            ctrl_held: false,
            editable: false,
            playback_enabled: self.recording_playback_controls_enabled(),
        });
    }

    pub fn handle_recording_daw_event(&mut self, event: &UiEvent) -> EventResponse {
        if let Some(response) = self.recording_ui.handle_asset_context_menu(
            event,
            &self.recording_daw_scene,
            self.recording_daw_layout.content,
        ) {
            return response;
        }
        if self
            .recording_ui
            .handle_asset_scroll(event, self.recording_daw_layout)
        {
            return EventResponse::Consumed;
        }
        if self
            .recording_ui
            .handle_track_scroll(event, self.recording_daw_layout)
        {
            return EventResponse::Consumed;
        }
        if let Some(track_id) = self.recording_ui.dragging_track_volume {
            match event {
                UiEvent::MouseMove { x, .. } => {
                    if let Some(control) =
                        self.recording_daw_scene.controls.iter().find(|control| {
                            control.control == RecordingControl::TrackVolume(track_id)
                        })
                    {
                        let volume = ((*x - control.bounds.x) / control.bounds.width
                            * crate::recording_mix::TRACK_VOLUME_MAX)
                            .clamp(0.0, crate::recording_mix::TRACK_VOLUME_MAX);
                        return EventResponse::Action(UiAction::RecordingSetTrackVolume {
                            track_id,
                            volume,
                        });
                    }
                }
                UiEvent::MouseRelease { .. } => {
                    self.recording_ui.dragging_track_volume = None;
                    return EventResponse::Consumed;
                }
                _ => {}
            }
        }
        if !self.recording_playback_controls_enabled() {
            return EventResponse::Consumed;
        }
        if let Some(response) = self.handle_recording_text_edit(event) {
            return response;
        }

        match event {
            UiEvent::MouseMove { x, y }
            | UiEvent::MousePress { x, y }
            | UiEvent::MouseRelease { x, y }
            | UiEvent::DoubleClick { x, y }
            | UiEvent::ShiftMousePress { x, y }
            | UiEvent::CtrlClick { x, y } => self.recording_daw_cursor = (*x, *y),
            _ => {}
        }

        for widget in self.recording_daw_toolbar_widgets.iter_mut() {
            let response = widget.handle_event(event);
            if response != EventResponse::Ignored {
                return response;
            }
        }

        if self.total_frames > 0 && self.recording_playback_controls_enabled() {
            if let Some(toolbar) = self.recording_daw_layout.toolbar {
                let hit = shell::progress_bar_hit_rect(&toolbar, false);
                match event {
                    UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                        if hit.contains(*x, *y) =>
                    {
                        self.scrubbing = true;
                        let ratio = ((*x - hit.x) / hit.width).clamp(0.0, 1.0);
                        return EventResponse::Action(UiAction::SeekAbsolute(
                            (ratio * self.total_frames as f32) as i64,
                        ));
                    }
                    UiEvent::MouseMove { x, .. } if self.scrubbing => {
                        let ratio = ((*x - hit.x) / hit.width).clamp(0.0, 1.0);
                        return EventResponse::Action(UiAction::SeekAbsolute(
                            (ratio * self.total_frames as f32) as i64,
                        ));
                    }
                    UiEvent::MouseRelease { .. } if self.scrubbing => {
                        self.scrubbing = false;
                        return EventResponse::Action(UiAction::FinishSeek);
                    }
                    _ => {}
                }
            }
        }

        if (matches!(event, UiEvent::Delete)
            || matches!(event, UiEvent::KeyInput { text } if text == "\x7f"))
            && (self.recording_ui.selected_clips().next().is_some()
                || self.recording_ui.selected_asset.is_some())
        {
            return EventResponse::Action(if self.recording_ui.selected_clips().next().is_some() {
                UiAction::RecordingDeleteSelectedClips
            } else {
                UiAction::RecordingDeleteSelectedAsset
            });
        }

        if self.dragging_recording_split == Some(RecordingSplitHandle::Assets) {
            match event {
                UiEvent::MouseMove { x, .. } => {
                    let content = self.recording_daw_layout.content;
                    self.recording_assets_split = ((content.x + content.width - *x)
                        / content.width.max(1.0))
                    .clamp(0.15, 0.45);
                    return EventResponse::Consumed;
                }
                UiEvent::MouseRelease { .. } => {
                    self.dragging_recording_split = None;
                    return EventResponse::Consumed;
                }
                _ => return EventResponse::Consumed,
            }
        }

        if let UiEvent::MousePress { x, y } = event {
            if self
                .recording_daw_layout
                .assets_split_handle_rect()
                .is_some_and(|rect| rect.contains(*x, *y))
            {
                self.dragging_recording_split = Some(RecordingSplitHandle::Assets);
                return EventResponse::Consumed;
            }
        }

        if let Some(asset_id) = self.recording_ui.dragging_asset {
            match event {
                UiEvent::MouseMove { .. } => return EventResponse::Consumed,
                UiEvent::MouseRelease { x, y } => {
                    self.recording_ui.dragging_asset = None;
                    let Some((track_id, start_frame)) = recording_drop_target(
                        self.recording_daw_layout,
                        &self.recording_daw_scene,
                        &self.recording_ui,
                        *x,
                        *y,
                    ) else {
                        return EventResponse::Consumed;
                    };
                    return EventResponse::Action(UiAction::RecordingPlaceAsset {
                        asset_id,
                        track_id,
                        start_frame,
                    });
                }
                _ => {}
            }
        }

        if let Some(mut drag) = self.recording_ui.dragging_clip {
            match event {
                UiEvent::MouseMove { x, y } => {
                    drag.accum_px += *x - drag.last_x;
                    drag.last_x = *x;
                    let delta_frames =
                        (drag.accum_px / self.recording_ui.pixels_per_frame).round() as i64;
                    if delta_frames == 0 {
                        self.recording_ui.dragging_clip = Some(drag);
                        return EventResponse::Consumed;
                    }
                    drag.accum_px -= delta_frames as f32 * self.recording_ui.pixels_per_frame;
                    self.recording_ui.dragging_clip = Some(drag);
                    let Some(body) = self.recording_daw_layout.track_body else {
                        return EventResponse::Consumed;
                    };
                    if !body.contains(*x, *y) {
                        return EventResponse::Consumed;
                    }
                    let track_ids: Vec<_> = self
                        .recording_daw_scene
                        .controls
                        .iter()
                        .filter_map(|control| match control.control {
                            RecordingControl::TrackMute(track_id) => Some(track_id),
                            _ => None,
                        })
                        .collect();
                    let row = ((*y - body.y) / TRACK_ROW_H).floor() as usize;
                    let Some(track_id) = track_ids.get(row).copied() else {
                        return EventResponse::Consumed;
                    };
                    return EventResponse::Action(UiAction::RecordingMoveSelectedClips {
                        track_id,
                        delta_frames,
                    });
                }
                UiEvent::MouseRelease { .. } => {
                    self.recording_ui.dragging_clip = None;
                    return EventResponse::Consumed;
                }
                _ => {}
            }
        }

        let pointer = match event {
            UiEvent::MousePress { x, y }
            | UiEvent::DoubleClick { x, y }
            | UiEvent::ShiftMousePress { x, y } => Some((*x, *y, false)),
            UiEvent::CtrlClick { x, y } => Some((*x, *y, true)),
            _ => None,
        };
        let Some((x, y, additive)) = pointer else {
            return EventResponse::Ignored;
        };
        let Some(control) = self
            .recording_daw_scene
            .controls
            .iter()
            .find(|control| control.bounds.contains(x, y))
            .cloned()
        else {
            self.recording_ui.clear_selection();
            return EventResponse::Consumed;
        };
        if !control.enabled {
            return EventResponse::Consumed;
        }
        if let RecordingControl::AssetGroup(owner) = &control.control {
            self.recording_ui.toggle_asset_group(owner);
            return EventResponse::Consumed;
        }
        if matches!(event, UiEvent::DoubleClick { .. })
            && matches!(control.control, RecordingControl::TrackExport(_))
        {
            return EventResponse::Consumed;
        }
        if self.recording_ui.editor.tool == crate::recording::RecordingTool::Cut
            && matches!(
                event,
                UiEvent::MousePress { .. } | UiEvent::DoubleClick { .. }
            )
        {
            if let RecordingControl::Clip(clip_id) = control.control {
                if let Some(body) = self.recording_daw_layout.track_body {
                    return EventResponse::Action(self.recording_cut_action(clip_id, x, body));
                }
            }
        }
        match control.control {
            RecordingControl::Asset(asset_id) => self.recording_ui.dragging_asset = Some(asset_id),
            RecordingControl::Clip(_) => {
                self.recording_ui.dragging_clip = Some(recording_workspace::RecordingClipDrag {
                    last_x: x,
                    accum_px: 0.0,
                });
            }
            RecordingControl::TrackVolume(track_id) => {
                self.recording_ui.dragging_track_volume = Some(track_id);
                let volume = ((x - control.bounds.x) / control.bounds.width
                    * crate::recording_mix::TRACK_VOLUME_MAX)
                    .clamp(0.0, crate::recording_mix::TRACK_VOLUME_MAX);
                return EventResponse::Action(UiAction::RecordingSetTrackVolume {
                    track_id,
                    volume,
                });
            }
            _ => {}
        }
        Self::recording_control_action(&control.control, additive)
            .map(EventResponse::Action)
            .unwrap_or(EventResponse::Consumed)
    }

    pub fn recording_enter_solo(&mut self) {
        self.recording_ui.enter_solo();
    }

    pub fn recording_enter_online(&mut self, role: RecordingRole) {
        self.recording_ui.enter_online(role);
    }

    pub fn reset_recording_workspace(&mut self) {
        self.recording_ui.return_to_choice();
        self.recording_capture_active = false;
        self.recording_capture_view = false;
        self.recording_scene = RecordingScene::default();
        self.rebuild_layout();
    }

    pub fn recording_role(&self) -> RecordingRole {
        self.recording_ui.role
    }

    pub fn recording_can_edit_timeline(&self) -> bool {
        self.recording_ui.role.can_edit_timeline()
    }

    pub fn recording_drop_target(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(crate::recording::AudioTrackId, i64)> {
        recording_drop_target(
            self.recording_layout,
            &self.recording_scene,
            &self.recording_ui,
            x,
            y,
        )
    }

    pub fn recording_daw_drop_target(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(crate::recording::AudioTrackId, i64)> {
        recording_drop_target(
            self.recording_daw_layout,
            &self.recording_daw_scene,
            &self.recording_ui,
            x,
            y,
        )
    }

    pub fn recording_begin_audio_import(
        &mut self,
        path: std::path::PathBuf,
        placement: Option<(crate::recording::AudioTrackId, i64)>,
        username: String,
    ) {
        self.recording_ui
            .begin_audio_import(path, placement, username);
    }

    pub fn recording_reveal_asset(
        &mut self,
        file_name: &str,
        asset_id: crate::recording::AudioAssetId,
    ) {
        self.recording_ui.reveal_asset(file_name, asset_id);
    }

    fn handle_recording_text_edit(&mut self, event: &UiEvent) -> Option<EventResponse> {
        self.recording_ui
            .handle_text_edit(event)
            .map(|result| match result {
                RecordingTextEditResult::Consumed => EventResponse::Consumed,
                RecordingTextEditResult::RenameTrack { track_id, name } => {
                    EventResponse::Action(UiAction::RecordingRenameTrack { track_id, name })
                }
                RecordingTextEditResult::ImportAudio {
                    path,
                    username,
                    placement,
                } => EventResponse::Action(UiAction::RecordingConfirmAudioImport {
                    path,
                    username,
                    placement,
                }),
            })
    }

    pub fn recording_track_volume(&self, track_id: crate::recording::AudioTrackId) -> f32 {
        self.recording_ui.track_volume(track_id)
    }

    pub fn recording_set_track_volume(
        &mut self,
        track_id: crate::recording::AudioTrackId,
        volume: f32,
    ) {
        self.recording_ui.set_track_volume(track_id, volume);
    }

    pub fn recording_playback_controls_enabled(&self) -> bool {
        !self.network_in_room || self.recording_ui.role.can_control_playback()
    }

    pub fn recording_daw_enabled(&self) -> bool {
        !self.network_in_room || !matches!(self.recording_ui.role, RecordingRole::Actor)
    }

    fn recording_actor_requests_enabled(&self) -> bool {
        self.network_in_room && matches!(self.recording_ui.role, RecordingRole::Director)
    }

    pub fn recording_page(&self) -> RecordingPage {
        self.recording_ui.page
    }

    pub fn recording_is_editing_text(&self) -> bool {
        self.recording_ui.is_editing_text()
    }

    pub fn recording_set_tool(&mut self, tool: crate::recording::RecordingTool) {
        self.recording_ui.editor.tool = tool;
    }

    pub fn recording_begin_rename_track(
        &mut self,
        project: &RecordingProject,
        track_id: crate::recording::AudioTrackId,
    ) {
        self.recording_ui.begin_rename_track(project, track_id);
    }

    pub fn recording_select_clip(
        &mut self,
        project: &RecordingProject,
        clip_id: crate::recording::AudioClipId,
        additive: bool,
    ) -> Result<(), crate::recording::RecordingError> {
        self.recording_ui.select_clip(project, clip_id, additive)
    }

    pub fn recording_select_asset(&mut self, asset_id: crate::recording::AudioAssetId) {
        self.recording_ui.editor.clear_selection();
        self.recording_ui.selected_asset = Some(asset_id);
    }

    pub fn recording_selected_asset(&self) -> Option<crate::recording::AudioAssetId> {
        self.recording_ui.selected_asset
    }

    pub fn recording_clear_asset_selection(&mut self) {
        self.recording_ui.selected_asset = None;
    }

    pub fn recording_editor_mut(&mut self) -> &mut crate::recording::RecordingEditor {
        &mut self.recording_ui.editor
    }

    fn recording_control_action(control: &RecordingControl, additive: bool) -> Option<UiAction> {
        Some(match control {
            RecordingControl::ChooseSolo => UiAction::RecordingChooseSolo,
            RecordingControl::ChooseOnline => UiAction::RecordingChooseOnline,
            RecordingControl::Tool(tool) => UiAction::RecordingSetTool(*tool),
            RecordingControl::AddTrack => UiAction::RecordingAddTrack,
            RecordingControl::DeleteSelectedClips => UiAction::RecordingDeleteSelectedClips,
            RecordingControl::RemoveTrack(track_id) => UiAction::RecordingRemoveTrack(*track_id),
            RecordingControl::RenameTrack(track_id) => {
                UiAction::RecordingBeginRenameTrack(*track_id)
            }
            RecordingControl::TrackMute(track_id) => UiAction::RecordingToggleTrackMute(*track_id),
            RecordingControl::TrackSolo(track_id) => UiAction::RecordingToggleTrackSolo(*track_id),
            RecordingControl::TrackArm(track_id) => UiAction::RecordingArmTrack(*track_id),
            RecordingControl::TrackVolume(track_id) => UiAction::RecordingAdjustTrackVolume {
                track_id: *track_id,
                delta: 0.1,
            },
            RecordingControl::TrackExport(track_id) => UiAction::RecordingExportTrack(*track_id),
            RecordingControl::StartCapture => UiAction::RecordingStartCapture,
            RecordingControl::Clip(clip_id) => UiAction::RecordingSelectClip {
                clip_id: *clip_id,
                additive,
            },
            RecordingControl::Asset(asset_id) => UiAction::RecordingSelectAsset(*asset_id),
            RecordingControl::AssetGroup(_) | RecordingControl::ImportUsername => return None,
            RecordingControl::Participant(_) => return None,
        })
    }

    fn recording_cut_action(
        &self,
        clip_id: crate::recording::AudioClipId,
        x: f32,
        body: Rect,
    ) -> UiAction {
        UiAction::RecordingCutClip {
            clip_id,
            at_frame: (self.recording_ui.view_start_frame
                + f64::from((x - body.x) / self.recording_ui.pixels_per_frame))
            .round() as i64,
        }
    }

    fn focused_recording_control(&self) -> Option<&recording_workspace::RecordingControlInfo> {
        let id = self.focus.current_id()?.0.as_str();
        self.recording_scene
            .controls
            .iter()
            .find(|control| control.control.stable_id() == id)
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &Project,
        voicelines_project: &crate::voicelines::VoicelinesProject,
        comic_dubs_project: &crate::comic_dubs::ComicDubsProject,
        render_index: &ProjectRenderIndex,
        render_frame: f64,
        fps: f64,
    ) -> EventResponse {
        if let UiEvent::MouseMove { x, y } = event {
            self.cursor_pos = (*x, *y);
        }

        // A microphone picker is opened for the local actor while other
        // recording overlays may still be settling. It owns every event
        // until it closes, including clicks on its cancel button.
        if let Some(outcome) =
            self.modal_host
                .handle_topmost_event(event, self.screen_w, self.screen_h)
        {
            return outcome.into_event_response();
        }

        if let UiEvent::MousePress { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y } = event
        {
            self.focus_widget_at(*x, *y);
        }

        // Project loading runs as a background task row: the UI stays
        // interactive while the BR archive is parsed.

        if let Some(modal) = self.project_transfer_modal.as_mut() {
            if let Some(action) = modal.handle_event(event, self.screen_w, self.screen_h) {
                return EventResponse::Action(match action {
                    ProjectTransferAction::Accept => UiAction::ProjectTransferAccept,
                    ProjectTransferAction::SaveAndReplace => UiAction::ProjectTransferSaveAndAccept,
                    ProjectTransferAction::Replace => UiAction::ProjectTransferReplace,
                    ProjectTransferAction::Refuse => UiAction::ProjectTransferRefuse,
                });
            }
            return EventResponse::Consumed;
        }

        // Export/proxy workers run on document snapshots, so the UI stays
        // interactive while they progress in a task row. Escape remains a
        // deliberate cancellation path handled by the worker.
        if self.export_progress.is_some()
            && matches!(event, UiEvent::KeyInput { text } if text == "\x1b")
        {
            return EventResponse::Action(UiAction::CancelExport);
        }

        // Task rows: a click on a row header unfolds its sub-steps; clicks
        // anywhere else on the card are consumed so they cannot reach the
        // interface underneath.
        if let UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } = event {
            let rows = self.task_row_views();
            if let Some(kind) =
                task_row::row_header_at(&rows, *x, *y, self.screen_w, self.screen_h)
            {
                match kind {
                    task_row::TaskRowKind::Loading => {
                        self.task_rows.loading_expanded = !self.task_rows.loading_expanded;
                    }
                    task_row::TaskRowKind::Export => {
                        self.task_rows.export_expanded = !self.task_rows.export_expanded;
                    }
                }
                return EventResponse::Consumed;
            }
            if task_row::card_bounds(&rows, self.screen_w, self.screen_h)
                .is_some_and(|bounds| bounds.contains(*x, *y))
            {
                return EventResponse::Consumed;
            }
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

        if self.active_workspace == WorkspaceId::Recording
            && self.network_in_room
            && matches!(self.recording_ui.role, RecordingRole::Actor)
            && matches!(event, UiEvent::KeyInput { text } if text == "\x1b")
        {
            return EventResponse::Action(UiAction::OpenRecordingActorMenu);
        }

        if self.active_workspace == WorkspaceId::Recording && self.recording_capture_active {
            if let Some(response) = self.handle_recording_split_drag(event) {
                return response;
            }
            if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
                return EventResponse::Action(UiAction::RecordingStopCapture);
            }
            return EventResponse::Consumed;
        }

        if let Some(response) = self.handle_recording_text_edit(event) {
            return response;
        }

        // Handle recording deletion before the properties panel gets a chance
        // to consume the global Delete key.
        if self.active_workspace == WorkspaceId::Recording
            && (matches!(event, UiEvent::Delete)
                || matches!(event, UiEvent::KeyInput { text } if text == "\x7f"))
            && (self.recording_ui.selected_clips().next().is_some()
                || self.recording_ui.selected_asset.is_some())
        {
            return EventResponse::Action(if self.recording_ui.selected_clips().next().is_some() {
                UiAction::RecordingDeleteSelectedClips
            } else {
                UiAction::RecordingDeleteSelectedAsset
            });
        }

        if self.rythmo_state.context_menu.is_some()
            && (matches!(
                event,
                UiEvent::CursorLeft
                    | UiEvent::CursorRight
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::Activate
            ) || matches!(event, UiEvent::KeyInput { text } if text == "\x1b"))
        {
            return rythmo::handle_context_menu_event(
                event,
                project,
                render_frame,
                &self.layout.rythmo,
                self.screen_w,
                self.screen_h,
                fps,
                &mut self.rythmo_state,
            );
        }

        // A rythmo text editor owns cursor navigation before any toolbar
        // widget sees it. In particular, the volume slider also interprets
        // Up/Down as value changes; letting it inspect these events while a
        // line is being edited makes the caret appear to change the volume.
        if self.active_workspace == WorkspaceId::Rythmo
            && self.rythmo_state.is_editing()
            && matches!(
                event,
                UiEvent::CursorLeft
                    | UiEvent::CursorRight
                    | UiEvent::ShiftCursorLeft
                    | UiEvent::ShiftCursorRight
                    | UiEvent::SelectWordLeft
                    | UiEvent::SelectWordRight
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::Home
                    | UiEvent::End
            )
        {
            let response = rythmo::handle_rythmo_event(
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
                match self.brush_radius_index {
                    0 => 0.006,
                    1 => 0.012,
                    2 => 0.024,
                    _ => 0.012,
                },
                self.erasing,
                rythmo::RythmoInteractionMode::Editable,
            );
            return if response == EventResponse::Ignored {
                EventResponse::Consumed
            } else {
                response
            };
        }

        // An open top-bar dropdown can extend over the side panel. Give it
        // pointer and keyboard priority so the panel underneath cannot consume
        // clicks on its items or on the area outside the dropdown.
        for widget in self.topbar_widgets.iter_mut() {
            if widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        // An open side-panel table is a keyboard focus scope of its own.
        // Route its semantic navigation before the shell-wide focus ring so
        // Tab, arrows, Enter, Space and Escape reach every table cell.
        if let Some(panel) = self.layout.properties {
            if self.side_panel.captures_keyboard_event(event) {
                if let Some(response) = self.side_panel.handle_event(event, panel, project) {
                    return response;
                }
            }
        }

        match event {
            UiEvent::FocusNext => {
                if let Some(node) = self.focus.focus_next() {
                    let label = if node.selected == Some(true) {
                        format!("{}, {}", node.label, t("accessibility.selected"))
                    } else {
                        node.label.clone()
                    };
                    return EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Focus {
                            label,
                            role: format!("{:?}", node.role),
                        },
                    ));
                }
                return EventResponse::Consumed;
            }
            UiEvent::FocusPrevious => {
                if let Some(node) = self.focus.focus_previous() {
                    let label = if node.selected == Some(true) {
                        format!("{}, {}", node.label, t("accessibility.selected"))
                    } else {
                        node.label.clone()
                    };
                    return EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Focus {
                            label,
                            role: format!("{:?}", node.role),
                        },
                    ));
                }
                return EventResponse::Consumed;
            }
            UiEvent::Activate => {
                if let Some(widget) = self.focused_widget_mut() {
                    let response = widget.handle_event(event);
                    return if response == EventResponse::Ignored {
                        EventResponse::Consumed
                    } else {
                        response
                    };
                }
                if let Some(control) = self.focused_recording_control().cloned() {
                    if !control.enabled {
                        return EventResponse::Consumed;
                    }
                    if let RecordingControl::AssetGroup(owner) = &control.control {
                        self.recording_ui.toggle_asset_group(owner);
                        return EventResponse::Consumed;
                    }
                    if let Some(action) = Self::recording_control_action(&control.control, false) {
                        return EventResponse::Action(action);
                    }
                    return EventResponse::Consumed;
                }
                if self.active_workspace == WorkspaceId::Voicelines {
                    if let Some(id) = self.focus.current_id().map(|id| id.0.clone()) {
                        if let Some(action) =
                            self.voicelines_ui.control_action(&id, voicelines_project)
                        {
                            return EventResponse::Action(action);
                        }
                    }
                } else if self.active_workspace == WorkspaceId::ComicDubs {
                    if let Some(id) = self.focus.current_id().map(|id| id.0.clone()) {
                        if let Some(action) =
                            self.comic_dubs_ui.control_action(&id, comic_dubs_project)
                        {
                            return EventResponse::Action(action);
                        }
                    }
                }
            }
            UiEvent::CursorLeft | UiEvent::CursorRight if self.focused_workspace_tab() => {
                let current = self
                    .focus
                    .current_id()
                    .and_then(|id| id.0.strip_prefix("tabs."))
                    .and_then(|index| index.parse::<usize>().ok())
                    .unwrap_or(0);
                let index = if matches!(event, UiEvent::CursorRight) {
                    (current + 1) % 4
                } else {
                    (current + 3) % 4
                };
                self.focus.focus(&FocusId::new(format!("tabs.{index}")));
                let workspace = match index {
                    0 => WorkspaceId::Rythmo,
                    1 => WorkspaceId::Recording,
                    2 => WorkspaceId::Voicelines,
                    _ => WorkspaceId::ComicDubs,
                };
                return EventResponse::Action(UiAction::ActivateWorkspace(workspace));
            }
            UiEvent::KeyInput { text } if text == "\x1b" && self.focus.current_id().is_some() => {
                self.focus.clear();
                return EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: t("accessibility.focus_exited").to_string(),
                    },
                ));
            }
            _ => {}
        }

        // The resize handle straddles the panel boundary, so it owns pointer
        // events before the panel content does.
        if let Some(response) = self.handle_props_drag(event) {
            return response;
        }

        // The side panel owns its complete area and its overlays (context menu,
        // role picker and color picker) before the rythmo workspace sees them.
        if let Some(panel) = self.layout.properties {
            if let Some(response) = self.side_panel.handle_event(event, panel, project) {
                return response;
            }
        }

        if self.active_workspace == WorkspaceId::Recording
            && matches!(
                event,
                UiEvent::OpenContextMenu | UiEvent::ContextMenu { .. }
            )
        {
            return self
                .recording_ui
                .handle_asset_context_menu(
                    event,
                    &self.recording_scene,
                    self.recording_layout.content,
                )
                .unwrap_or(EventResponse::Consumed);
        }

        if matches!(event, UiEvent::OpenContextMenu) {
            let line_id = match self.rythmo_state.selected.as_ref() {
                Some(rythmo::Selection::Line(line_id)) => Some(*line_id),
                Some(rythmo::Selection::Lines(line_ids)) => line_ids.first().copied(),
                Some(rythmo::Selection::AllLines) => project.lines().next().map(|line| line.id),
                _ => None,
            };
            if let Some(line_id) = line_id {
                let zone = self.layout.rythmo;
                self.rythmo_state.context_menu = Some(rythmo::LineContextMenu {
                    line_id,
                    x: zone.x + zone.width * 0.5,
                    y: zone.y + zone.height * 0.5,
                    hover_main: true,
                    hover_change_character: false,
                    hover_text_emotion: false,
                    hover_generate_detection: false,
                    hover_emotion_index: None,
                    hover_emotion_variant: None,
                    text_range: (self.rythmo_state.editing_line == Some(line_id))
                        .then(|| self.rythmo_state.line_input.selection_range())
                        .flatten(),
                    hover_actor_index: None,
                    hover_action_index: None,
                    actor_scroll: 0.0,
                });
                return EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus {
                        label: rythmo::context_menu_accessibility_label(project, line_id),
                        role: "menu".to_string(),
                    },
                ));
            }
            return EventResponse::Consumed;
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
                fps,
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

        // Top-bar triggers sit above every workspace. Handle them before the
        // Comic Dubs canvas can clear its selection on an unrelated click.
        for widget in self.topbar_widgets.iter_mut() {
            if !widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        if self.active_workspace == WorkspaceId::Voicelines {
            let selection_before = self.voicelines_ui.selected_regions().to_vec();
            let response =
                self.voicelines_ui
                    .handle_event(event, voicelines_project, self.voicelines_layout);
            if self.voicelines_ui.selected_regions() != selection_before {
                self.rebuild_topbar(self.network_in_room);
            }
            if response != EventResponse::Ignored {
                return response;
            }
        }

        if self.active_workspace == WorkspaceId::ComicDubs {
            let selection_before = self.comic_dubs_ui.selected_bubble();
            let response =
                self.comic_dubs_ui
                    .handle_event(event, comic_dubs_project, self.comic_dubs_layout);
            if self.comic_dubs_ui.selected_bubble() != selection_before {
                self.rebuild_topbar(self.network_in_room);
            }
            if response != EventResponse::Ignored {
                return response;
            }
        }

        if let Some(response) = self.handle_recording_split_drag(event) {
            return response;
        }

        if self.active_workspace == WorkspaceId::Recording
            && self
                .recording_ui
                .handle_asset_scroll(event, self.recording_layout)
        {
            return EventResponse::Consumed;
        }

        if self.active_workspace == WorkspaceId::Recording
            && self
                .recording_ui
                .handle_track_scroll(event, self.recording_layout)
        {
            return EventResponse::Consumed;
        }

        if let Some(response) = self.handle_split_drag(event) {
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
        for widget in self.toolbar_widgets.iter_mut() {
            if widget.captures_all() {
                let response = widget.handle_event(event);
                if response != EventResponse::Ignored {
                    self.update_tooltip();
                    return response;
                }
            }
        }

        if self.active_workspace == WorkspaceId::Rythmo {
            if let Some(response) = self.automation_editor.handle_event(
                event,
                &self.layout.video_preview,
                &project.settings().automation,
                project,
            ) {
                return response;
            }
        }
        for widget in self
            .tab_widgets
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

        if matches!(
            self.active_workspace,
            WorkspaceId::Voicelines | WorkspaceId::ComicDubs
        ) && self.total_frames > 0
        {
            let hit = shell::progress_bar_hit_rect(&self.active_toolbar_rect(), false);
            match event {
                UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                    if hit.contains(*x, *y) =>
                {
                    self.scrubbing = true;
                    let frame = (((*x - hit.x) / hit.width).clamp(0.0, 1.0)
                        * self.total_frames as f32) as i64;
                    return EventResponse::Action(UiAction::SeekAbsolute(frame));
                }
                UiEvent::MouseMove { x, .. } if self.scrubbing => {
                    let frame = (((*x - hit.x) / hit.width).clamp(0.0, 1.0)
                        * self.total_frames as f32) as i64;
                    return EventResponse::Action(UiAction::SeekAbsolute(frame));
                }
                UiEvent::MouseRelease { .. } if self.scrubbing => {
                    self.scrubbing = false;
                    return EventResponse::Action(UiAction::FinishSeek);
                }
                _ => {}
            }
        }

        if self.active_workspace == WorkspaceId::Voicelines {
            return EventResponse::Ignored;
        }

        if self.active_workspace == WorkspaceId::Recording {
            if let Some(response) = self.recording_ui.handle_asset_context_menu(
                event,
                &self.recording_scene,
                self.recording_layout.content,
            ) {
                return response;
            }
            if (matches!(event, UiEvent::Delete)
                || matches!(event, UiEvent::KeyInput { text } if text == "\x7f"))
                && (self.recording_ui.selected_clips().next().is_some()
                    || self.recording_ui.selected_asset.is_some())
            {
                return EventResponse::Action(
                    if self.recording_ui.selected_clips().next().is_some() {
                        UiAction::RecordingDeleteSelectedClips
                    } else {
                        UiAction::RecordingDeleteSelectedAsset
                    },
                );
            }
            if let Some(track_id) = self.recording_ui.dragging_track_volume {
                match event {
                    UiEvent::MouseMove { x, .. } => {
                        if let Some(control) =
                            self.recording_scene.controls.iter().find(|control| {
                                control.control == RecordingControl::TrackVolume(track_id)
                            })
                        {
                            let volume = ((*x - control.bounds.x) / control.bounds.width
                                * crate::recording_mix::TRACK_VOLUME_MAX)
                                .clamp(0.0, crate::recording_mix::TRACK_VOLUME_MAX);
                            return EventResponse::Action(UiAction::RecordingSetTrackVolume {
                                track_id,
                                volume,
                            });
                        }
                    }
                    UiEvent::MouseRelease { .. } => {
                        self.recording_ui.dragging_track_volume = None;
                        return EventResponse::Consumed;
                    }
                    _ => {}
                }
            }
            if let Some(asset_id) = self.recording_ui.dragging_asset {
                match event {
                    UiEvent::MouseMove { .. } => return EventResponse::Consumed,
                    UiEvent::MouseRelease { x, y } => {
                        self.recording_ui.dragging_asset = None;
                        let Some((track_id, start_frame)) = recording_drop_target(
                            self.recording_layout,
                            &self.recording_scene,
                            &self.recording_ui,
                            *x,
                            *y,
                        ) else {
                            return EventResponse::Consumed;
                        };
                        return EventResponse::Action(UiAction::RecordingPlaceAsset {
                            asset_id,
                            track_id,
                            start_frame,
                        });
                    }
                    _ => {}
                }
            }
            if let Some(mut drag) = self.recording_ui.dragging_clip {
                match event {
                    UiEvent::MouseMove { x, y } => {
                        drag.accum_px += *x - drag.last_x;
                        drag.last_x = *x;
                        let delta_frames =
                            (drag.accum_px / self.recording_ui.pixels_per_frame).round() as i64;
                        if delta_frames == 0 {
                            self.recording_ui.dragging_clip = Some(drag);
                            return EventResponse::Consumed;
                        }
                        drag.accum_px -= delta_frames as f32 * self.recording_ui.pixels_per_frame;
                        self.recording_ui.dragging_clip = Some(drag);
                        let Some(body) = self.recording_layout.track_body else {
                            return EventResponse::Consumed;
                        };
                        if !body.contains(*x, *y) {
                            return EventResponse::Consumed;
                        }
                        let track_ids: Vec<_> = self
                            .recording_scene
                            .controls
                            .iter()
                            .filter_map(|control| match &control.control {
                                RecordingControl::TrackMute(track_id) => Some(*track_id),
                                _ => None,
                            })
                            .collect();
                        let row = ((*y - body.y) / TRACK_ROW_H).floor() as usize;
                        let Some(track_id) = track_ids.get(row).copied() else {
                            return EventResponse::Consumed;
                        };
                        return EventResponse::Action(UiAction::RecordingMoveSelectedClips {
                            track_id,
                            delta_frames,
                        });
                    }
                    UiEvent::MouseRelease { .. } => {
                        self.recording_ui.dragging_clip = None;
                        return EventResponse::Consumed;
                    }
                    _ => {}
                }
            }
            let pointer = match event {
                UiEvent::MousePress { x, y }
                | UiEvent::DoubleClick { x, y }
                | UiEvent::ShiftMousePress { x, y } => Some((*x, *y, false)),
                UiEvent::CtrlClick { x, y } => Some((*x, *y, true)),
                _ => None,
            };
            if let Some((x, y, additive)) = pointer {
                if let Some(owner) = self.recording_scene.controls.iter().find_map(|control| {
                    if !control.bounds.contains(x, y) {
                        return None;
                    }
                    match &control.control {
                        RecordingControl::AssetGroup(owner) => Some(owner.clone()),
                        _ => None,
                    }
                }) {
                    self.recording_ui.toggle_asset_group(&owner);
                    return EventResponse::Consumed;
                }
                if matches!(
                    event,
                    UiEvent::MousePress { .. } | UiEvent::DoubleClick { .. }
                ) {
                    if let Some(control) = self
                        .recording_scene
                        .controls
                        .iter()
                        .find(|control| control.bounds.contains(x, y))
                        .map(|control| &control.control)
                    {
                        match control {
                            RecordingControl::Asset(asset_id) => {
                                self.recording_ui.dragging_asset = Some(*asset_id);
                            }
                            RecordingControl::Clip(clip_id)
                                if self.recording_ui.editor.tool
                                    == crate::recording::RecordingTool::Cut =>
                            {
                                if let Some(body) = self.recording_layout.track_body {
                                    return EventResponse::Action(
                                        self.recording_cut_action(*clip_id, x, body),
                                    );
                                }
                            }
                            RecordingControl::Clip(_) => {
                                self.recording_ui.dragging_clip =
                                    Some(recording_workspace::RecordingClipDrag {
                                        last_x: x,
                                        accum_px: 0.0,
                                    });
                            }
                            RecordingControl::TrackVolume(track_id) => {
                                self.recording_ui.dragging_track_volume = Some(*track_id);
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(control) = self
                    .recording_scene
                    .controls
                    .iter()
                    .find(|control| control.bounds.contains(x, y))
                {
                    if !control.enabled {
                        return EventResponse::Consumed;
                    }
                    if let RecordingControl::TrackVolume(track_id) = &control.control {
                        let volume = ((x - control.bounds.x) / control.bounds.width
                            * crate::recording_mix::TRACK_VOLUME_MAX)
                            .clamp(0.0, crate::recording_mix::TRACK_VOLUME_MAX);
                        return EventResponse::Action(UiAction::RecordingSetTrackVolume {
                            track_id: *track_id,
                            volume,
                        });
                    }
                    if matches!(event, UiEvent::DoubleClick { .. })
                        && matches!(control.control, RecordingControl::TrackExport(_))
                    {
                        return EventResponse::Consumed;
                    }
                    if let Some(action) = Self::recording_control_action(&control.control, additive)
                    {
                        return EventResponse::Action(action);
                    }
                    return EventResponse::Consumed;
                }
            }
            if self.recording_ui.page == RecordingPage::Choice {
                return EventResponse::Ignored;
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
        if self.active_workspace == WorkspaceId::Rythmo && self.brush_picking {
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
        if self.total_frames > 0
            && (self.active_workspace != WorkspaceId::Recording
                || self.recording_playback_controls_enabled())
            && (self.active_workspace == WorkspaceId::Rythmo
                || self.recording_layout.toolbar.is_some())
        {
            let hit = shell::progress_bar_hit_rect(
                &self.active_toolbar_rect(),
                self.active_workspace == WorkspaceId::Rythmo,
            );
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
        let passive_playback_move = self.playing
            && matches!(event, UiEvent::MouseMove { .. })
            && !self.rythmo_state.needs_pointer_motion();
        self.rythmo_state.set_compact_empty_tracks(
            self.active_workspace == WorkspaceId::Recording
                && self.recording_ui.page == RecordingPage::Timeline,
        );
        let rythmo_response = if passive_playback_move {
            EventResponse::Ignored
        } else {
            let rythmo_zone = self.active_rythmo_rect();
            rythmo::handle_rythmo_event(
                event,
                &rythmo_zone,
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
                if self.active_workspace == WorkspaceId::Rythmo {
                    rythmo::RythmoInteractionMode::Editable
                } else {
                    rythmo::RythmoInteractionMode::ReadOnly
                },
            )
        };
        if rythmo_response != EventResponse::Ignored {
            return rythmo_response;
        }

        // The contextual shortcut panel owns the wheel over its own rect,
        // before the rythmo zone claims every scroll inside its bounds.
        if let UiEvent::Scroll { .. } = event {
            if crate::config::get().ui.show_controls_hint
                && self.shortcut_panel.handle_scroll(event, self.screen_h)
            {
                return EventResponse::Consumed;
            }
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
            let recording_timeline_contains = self.active_workspace == WorkspaceId::Recording
                && self
                    .recording_layout
                    .track_body
                    .is_some_and(|body| body.contains(*x, *y));
            if self.active_rythmo_rect().contains(*x, *y) || recording_timeline_contains {
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
                        x: props.x + props.width - PROPS_DRAG_ZONE,
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
                    self.props_width = x.clamp(PROPS_MIN_W, PROPS_MAX_W);
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
        if self.active_workspace != WorkspaceId::Rythmo {
            return None;
        }
        let content_top = TOPBAR_H + TABBAR_H;
        let content_h = self.screen_h - content_top;
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
                        shell::SplitHandle::Video => (*y) - content_top,
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

    fn handle_recording_split_drag(&mut self, event: &UiEvent) -> Option<EventResponse> {
        if self.active_workspace != WorkspaceId::Recording
            || self.recording_ui.page != RecordingPage::Timeline
        {
            return None;
        }
        let layout = self.recording_layout;
        match event {
            UiEvent::MousePress { x, y } => {
                let handle = if self.recording_capture_view {
                    layout
                        .rythmo_split_handle_rect()
                        .contains(*x, *y)
                        .then_some(RecordingSplitHandle::Rythmo)
                } else if layout.video_split_handle_rect().contains(*x, *y) {
                    Some(RecordingSplitHandle::Video)
                } else if !self.recording_daw_detached
                    && layout.rythmo_split_handle_rect().contains(*x, *y)
                {
                    Some(RecordingSplitHandle::Rythmo)
                } else if !self.recording_daw_detached
                    && layout
                        .assets_split_handle_rect()
                        .is_some_and(|rect| rect.contains(*x, *y))
                {
                    Some(RecordingSplitHandle::Assets)
                } else {
                    None
                };
                if let Some(handle) = handle {
                    self.dragging_recording_split = Some(handle);
                    return Some(EventResponse::Consumed);
                }
            }
            UiEvent::MouseMove { x, y } => {
                let Some(handle) = self.dragging_recording_split else {
                    return None;
                };
                let content = layout.content;
                match handle {
                    RecordingSplitHandle::Video => {
                        let available = if self.recording_daw_detached {
                            content.height.max(1.0)
                        } else {
                            (content.height - TOOLBAR_H).max(1.0)
                        };
                        self.recording_video_split =
                            ((*y - content.y) / available).clamp(0.2, 0.72);
                    }
                    RecordingSplitHandle::Rythmo => {
                        if self.recording_capture_view {
                            let waveform_h = layout
                                .source_waveform
                                .into_iter()
                                .chain(layout.microphone_waveform)
                                .map(|waveform| waveform.height)
                                .sum::<f32>();
                            let available = (content.height - waveform_h).max(1.0);
                            self.recording_capture_rythmo_split = Some(
                                ((content.y + content.height - *y) / available).clamp(0.0, 1.0),
                            );
                        } else {
                            let available =
                                (content.height - layout.video.height - TOOLBAR_H).max(1.0);
                            self.recording_rythmo_split =
                                ((*y - layout.rythmo.y) / available).clamp(0.15, 0.72);
                        }
                    }
                    RecordingSplitHandle::Assets => {
                        self.recording_assets_split = ((content.x + content.width - *x)
                            / content.width.max(1.0))
                        .clamp(0.15, 0.45);
                    }
                }
                self.rebuild_layout();
                return Some(EventResponse::Consumed);
            }
            UiEvent::MouseRelease { .. } => {
                if self.dragging_recording_split.take().is_some() {
                    return Some(EventResponse::Consumed);
                }
            }
            _ => {}
        }
        None
    }

    pub(crate) fn hovering_split_handle(&self) -> bool {
        let (cx, cy) = self.cursor_pos;
        if self.active_workspace == WorkspaceId::Recording
            && self.recording_ui.page == RecordingPage::Timeline
        {
            if self.recording_capture_view {
                return self
                    .recording_layout
                    .rythmo_split_handle_rect()
                    .contains(cx, cy);
            }
            return self
                .recording_layout
                .video_split_handle_rect()
                .contains(cx, cy)
                || (!self.recording_daw_detached
                    && (self
                        .recording_layout
                        .rythmo_split_handle_rect()
                        .contains(cx, cy)
                        || self
                            .recording_layout
                            .assets_split_handle_rect()
                            .is_some_and(|rect| rect.contains(cx, cy))));
        }
        self.layout.video_split_handle_rect().contains(cx, cy)
            || self.layout.rythmo_split_handle_rect().contains(cx, cy)
    }

    pub(crate) fn dragging_split_handle(&self) -> bool {
        self.dragging_split.is_some() || self.dragging_recording_split.is_some()
    }

    pub(crate) fn hovering_props_handle(&self) -> bool {
        let Some(props) = self.layout.properties else {
            return false;
        };
        let (cx, cy) = self.cursor_pos;
        Rect {
            x: props.x + props.width - PROPS_DRAG_ZONE,
            y: props.y,
            width: PROPS_DRAG_ZONE * 2.0,
            height: props.height,
        }
        .contains(cx, cy)
    }

    pub(crate) fn dragging_props_handle(&self) -> bool {
        self.dragging_props
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

    pub fn open_side_panel(&mut self, kind: side_panel::SidePanelKind) {
        self.close_automation();
        self.props_visible = true;
        self.side_panel.open(kind);
        self.rebuild_layout();
        self.tooltip = None;
    }

    pub fn open_side_panel_with_selection(
        &mut self,
        kind: side_panel::SidePanelKind,
        selected_line_ids: impl IntoIterator<Item = u64>,
    ) {
        self.close_automation();
        self.props_visible = true;
        self.side_panel.open_with_selection(kind, selected_line_ids);
        self.rebuild_layout();
        self.tooltip = None;
    }

    pub fn side_panel_first_accessibility_label(&self, project: &Project) -> String {
        self.side_panel.first_accessibility_label(project)
    }

    pub fn side_panel_accessibility_title(&self) -> Option<&'static str> {
        self.side_panel.accessibility_title()
    }

    pub fn close_side_panel(&mut self) {
        self.props_visible = false;
        self.side_panel.close();
        self.rebuild_layout();
    }

    pub fn side_panel_open(&self) -> bool {
        self.side_panel.is_open()
    }

    pub fn take_selected_automation_node(&mut self) -> Option<u64> {
        self.automation_editor.take_selected_node_for_deletion()
    }

    fn update_tooltip(&mut self) {
        let (cx, cy) = self.cursor_pos;
        for widget in self
            .topbar_widgets
            .iter()
            .chain(self.tab_widgets.iter())
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
        self.set_playing(!self.playing);
    }

    pub fn set_playing(&mut self, playing: bool) {
        if self.playing == playing {
            return;
        }
        self.playing = playing;
        if self.playing {
            self.rythmo_state.hovered_line = None;
            self.rythmo_state.hovered_track = None;
            self.rythmo_state.detection_hover = None;
            self.rythmo_state.ghost_preview = None;
        }
        self.rebuild_toolbar();
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn has_active_progress(&self) -> bool {
        self.export_progress.is_some()
    }

    /// Views of the currently running background tasks (project import,
    /// export, proxy), rendered as expandable task rows at the top center.
    fn task_row_views(&self) -> Vec<task_row::TaskRowView> {
        let mut rows = Vec::new();
        if let Some(load) = &self.loading_project {
            let steps = task_row::LOADING_STEP_KEYS
                .iter()
                .enumerate()
                .map(|(index, key)| task_row::TaskStepView {
                    label: t(key).to_string(),
                    state: match index.cmp(&load.stage_index) {
                        std::cmp::Ordering::Less => task_row::TaskStepState::Done,
                        std::cmp::Ordering::Equal => task_row::TaskStepState::Running,
                        std::cmp::Ordering::Greater => task_row::TaskStepState::Pending,
                    },
                    meta: None,
                })
                .collect();
            let detail = (!load.label.is_empty()).then(|| load.label.clone());
            rows.push(task_row::TaskRowView::new(
                task_row::TaskRowKind::Loading,
                t("loading_project.title"),
                detail,
                load.progress,
                false,
                steps,
                self.task_rows.loading_expanded,
            ));
        }
        if let Some(progress_atomic) = &self.export_progress {
            let progress =
                f32::from_bits(progress_atomic.load(std::sync::atomic::Ordering::Relaxed));
            let percent = format!("{:.0} %", progress.clamp(0.0, 1.0) * 100.0);
            let steps = vec![task_row::TaskStepView {
                label: self.export_label.clone(),
                state: task_row::TaskStepState::Running,
                meta: Some(percent),
            }];
            rows.push(task_row::TaskRowView::new(
                task_row::TaskRowKind::Export,
                self.export_label.clone(),
                None,
                progress,
                true,
                steps,
                self.task_rows.export_expanded,
            ));
        }
        rows
    }

    pub fn open_project_transfer_modal(
        &mut self,
        metadata: crate::network::ProjectTransferMetadata,
        is_director: bool,
        dirty: bool,
    ) {
        self.project_transfer_modal = Some(ProjectTransferModal::new(metadata, is_director, dirty));
    }

    pub fn start_project_load(&mut self, label: String) {
        self.task_rows.loading_expanded = false;
        self.loading_project = Some(ProjectLoadUi {
            label,
            phase: t("loading_project.reading_manifest").to_string(),
            progress: 0.0,
            stage_index: 0,
        });
    }

    pub fn set_project_load_progress(&mut self, phase_key: &str, progress: f32) {
        if let Some(load) = self.loading_project.as_mut() {
            load.phase = t(phase_key).to_string();
            load.progress = progress.clamp(0.0, 1.0);
            load.stage_index = task_row::LOADING_STEP_KEYS
                .iter()
                .position(|key| *key == phase_key)
                .unwrap_or(load.stage_index);
        }
    }

    pub fn finish_project_load(&mut self) {
        self.loading_project = None;
    }

    pub fn set_project_transfer_status(&mut self, status: crate::network::ProjectTransferStatus) {
        if let Some(modal) = self.project_transfer_modal.as_mut() {
            modal.set_status(status);
        }
    }

    pub fn set_project_transfer_result_path(&mut self, path: String) {
        if let Some(modal) = self.project_transfer_modal.as_mut() {
            modal.set_result_path(path);
        }
    }

    pub fn mark_project_transfer_responded(&mut self) {
        if let Some(modal) = self.project_transfer_modal.as_mut() {
            modal.mark_response_submitted();
        }
    }

    pub fn reset_project_transfer_response(&mut self) {
        if let Some(modal) = self.project_transfer_modal.as_mut() {
            modal.reset_response();
        }
    }

    pub fn close_project_transfer_modal(&mut self) {
        self.project_transfer_modal = None;
    }

    pub fn needs_animation_or_interaction(&self) -> bool {
        self.playing
            || self.dragging_props
            || self.dragging_split.is_some()
            || self.dragging_recording_split.is_some()
            || self.scrubbing
            || self.toasts.has_active()
            || self.project_transfer_modal.is_some()
            || self.comic_dubs_ui.vertex_editor_playing()
            || self.rythmo_state.needs_animation_or_interaction()
    }

    pub fn needs_background_poll(&self) -> bool {
        self.modal_host.server_browser.is_some()
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        let mut deadline = self.rythmo_state.next_cursor_blink_deadline();
        if let Some(side_panel_deadline) = self.side_panel.next_cursor_blink_deadline() {
            deadline = Some(deadline.map_or(side_panel_deadline, |current| {
                current.min(side_panel_deadline)
            }));
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

    pub fn toggle_toolbar_dropdown(&mut self, dd: primitives::ToolbarDropdown) -> bool {
        if self.active_dropdown == Some(dd.clone()) {
            self.active_dropdown = None;
            false
        } else {
            self.active_dropdown = Some(dd);
            true
        }
    }

    pub fn toolbar_dropdown_is_open(&self, dd: &primitives::ToolbarDropdown) -> bool {
        self.active_dropdown.as_ref() == Some(dd)
    }

    pub fn open_recent_projects(&mut self) {
        // Replaced by the keyboard focus/popup implementation below. Keeping
        // this semantic entry point prevents shortcuts from depending on the
        // topbar widget order.
        self.rebuild_topbar(self.network_in_room);
        if let Some(project_menu) = self.topbar_widgets.first_mut() {
            project_menu.open_submenu(4);
            self.focus.focus(&FocusId::new("topbar.0"));
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
        let label = match &dd {
            primitives::ToolbarDropdown::Respirations => t("toolbar.respirations").to_string(),
            primitives::ToolbarDropdown::Reactions => t("toolbar.reactions").to_string(),
        };
        let items = Self::dropdown_items(&dd);
        let dropdown_rect = self.toolbar_dropdown_rect(&dd, items.len());
        if !dropdown_rect.contains(x, y) {
            self.active_dropdown = None;
            return EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Collapsed { label },
            ));
        }
        let item_h = 26.0;
        let idx = ((y - dropdown_rect.y) / item_h) as usize;
        if let Some((text, _)) = items.get(idx) {
            self.active_dropdown = None;
            return EventResponse::Actions(vec![
                UiAction::AddQuickLine {
                    text: text.to_string(),
                },
                UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Collapsed {
                    label,
                }),
            ]);
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
        self.rythmo_state.is_editing()
            || self.modal_host.is_editing_text()
            || self.side_panel.is_editing_text()
            || self.recording_ui.is_editing_text()
            || self.voicelines_ui.is_editing_text()
            || self.comic_dubs_ui.is_editing_text()
    }

    pub fn voicelines_audio_selected(&mut self, duration_ms: u64, audio_index: usize) {
        self.voicelines_ui.audio_selected(duration_ms, audio_index);
        self.rebuild_topbar(self.network_in_room);
    }

    pub fn set_voicelines_selected_region(
        &mut self,
        selected: Option<crate::voicelines::RegionId>,
    ) {
        self.voicelines_ui.set_selected_region(selected);
        self.rebuild_topbar(self.network_in_room);
    }

    pub fn begin_voicelines_region_rename(
        &mut self,
        region_id: crate::voicelines::RegionId,
        name: String,
    ) {
        self.voicelines_ui.begin_rename(region_id, name);
        self.rebuild_topbar(self.network_in_room);
    }

    pub fn begin_voicelines_naming_pattern(&mut self, pattern: String) {
        self.voicelines_ui.begin_naming_pattern(pattern);
    }

    pub fn comic_dubs_drop_accepts(&self, x: f32, y: f32) -> bool {
        self.comic_dubs_ui
            .drop_accepts(self.comic_dubs_layout, x, y)
    }

    pub fn begin_comic_dubs_text_edit(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        text: String,
    ) {
        self.comic_dubs_ui.begin_text_edit(bubble_id, text);
        self.rebuild_topbar(self.network_in_room);
    }

    pub fn open_comic_dubs_vertex_editor(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
    ) {
        self.comic_dubs_ui.open_vertex_editor(bubble_id);
    }

    pub fn close_comic_dubs_vertex_editor(&mut self) -> bool {
        self.comic_dubs_ui.close_vertex_editor()
    }

    pub fn set_comic_dubs_vertex_editor_playhead(
        &mut self,
        at_ms: u64,
        project: &crate::comic_dubs::ComicDubsProject,
    ) {
        self.comic_dubs_ui
            .set_vertex_editor_playhead(at_ms, project);
    }

    pub fn toggle_comic_dubs_vertex_editor_preview(
        &mut self,
        project: &crate::comic_dubs::ComicDubsProject,
    ) -> bool {
        self.comic_dubs_ui.toggle_vertex_editor_preview(project)
    }

    pub fn nudge_comic_dubs_vertex_editor(
        &mut self,
        delta_ms: i64,
        project: &crate::comic_dubs::ComicDubsProject,
    ) -> bool {
        self.comic_dubs_ui.nudge_vertex_editor(delta_ms, project)
    }

    pub fn cancel_comic_dubs_draft(&mut self) -> bool {
        self.comic_dubs_ui.cancel_draft()
    }

    pub fn set_comic_dubs_playback(
        &mut self,
        page_id: Option<crate::comic_dubs::PageId>,
        visible_bubbles: usize,
        bubble_elapsed_ms: u64,
    ) {
        self.comic_dubs_ui
            .set_playback(page_id, visible_bubbles, bubble_elapsed_ms);
    }

    pub fn set_comic_dubs_pending_audio_imports(&mut self, count: usize) {
        self.comic_dubs_ui.set_pending_audio_imports(count);
    }

    pub fn reset_comic_dubs_workspace(&mut self) {
        self.comic_dubs_ui.clear_document_state();
        self.comic_dubs_texture = None;
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

    pub fn open_video_only_export_modal(
        &mut self,
        video_width: u32,
        video_height: u32,
        configuration: crate::project::ExportConfiguration,
    ) {
        self.modal_host
            .open_video_only_export(video_width, video_height, configuration);
    }

    pub fn open_media_explorer(
        &mut self,
        languages: Vec<language_modal::LanguageListItem>,
        active_language_id: u64,
        media: language_modal::MediaExplorerData,
    ) {
        self.modal_host
            .open_media_explorer(languages, active_language_id, media);
    }

    pub fn refresh_languages_modal(
        &mut self,
        languages: Vec<language_modal::LanguageListItem>,
        active_language_id: u64,
    ) {
        self.modal_host
            .refresh_languages(languages, active_language_id);
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

    pub fn open_recording_input_device_modal(
        &mut self,
        devices: Vec<crate::media_recording::InputDeviceInfo>,
        selected: Option<String>,
    ) {
        self.modal_host.open_microphone(devices, selected);
    }

    pub fn open_recording_actor_menu(&mut self) {
        self.modal_host.open_recording_actor_menu(self.volume);
    }

    pub fn refresh_media_explorer(&mut self, media: language_modal::MediaExplorerData) {
        self.modal_host.refresh_media_explorer(media);
    }

    pub fn open_room_invitation(&mut self, code: String, link: String) {
        self.modal_host.open_invitation(code, link);
    }

    pub fn open_whats_new_modal(
        &mut self,
        version: impl Into<String>,
        body: impl Into<String>,
        video_url: Option<String>,
        thumbnail: Option<Vec<u8>>,
    ) {
        self.whats_new_thumbnail_texture = None;
        self.modal_host
            .open_whats_new(version, body, video_url, thumbnail);
    }

    pub fn open_save_prompt(&mut self, kind: save_prompt_modal::SavePromptKind) {
        self.modal_host.open_save_prompt(kind);
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

    /// Open the connect modal pre-filled with a room code, for the
    /// `coquerythmo://<join>` quick-setup flow.
    pub fn open_connect_modal_with_room(
        &mut self,
        ip: &str,
        port: u16,
        room_code: &str,
        password: &str,
    ) {
        self.modal_host
            .open_connect_with_room(ip, port, room_code, password);
    }

    pub fn open_settings_modal(
        &mut self,
        temporary_directory: std::path::PathBuf,
    ) {
        self.modal_host.open_settings(temporary_directory);
    }

    pub fn open_project_settings_modal(
        &mut self,
        fonts: Vec<String>,
        rythmo_font: Option<String>,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
        show_text_emotion_lanes: bool,
    ) {
        self.modal_host.open_project_settings(
            fonts,
            rythmo_font,
            scroll_speed,
            reading_bar_offset_percent,
            instrumental_audio_path,
            highlight_read_word,
            scrolling_text_uses_character_color,
            show_text_emotion_lanes,
        );
    }

    pub fn open_comic_dubs_settings_modal(
        &mut self,
        fonts: Vec<String>,
        font_family: Option<String>,
        bubble_duration_ms: u64,
        page_duration_ms: u64,
        default_font_size: f32,
    ) {
        self.modal_host.open_comic_dubs_settings(
            fonts,
            font_family,
            bubble_duration_ms,
            page_duration_ms,
            default_font_size,
        );
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

    pub fn set_settings_temporary_directory(&mut self, path: std::path::PathBuf) {
        self.modal_host.set_settings_temporary_directory(path);
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
        let selected_regions = self.voicelines_ui.selected_regions().to_vec();
        let selected_bubble = self.comic_dubs_ui.selected_bubble();
        self.topbar_widgets = shell::build_topbar(
            self.network_in_room,
            self.has_video,
            self.screen_w,
            self.uv("settings"),
            self.uv("project"),
            self.active_workspace,
            self.recording_daw_enabled(),
            self.recording_actor_requests_enabled(),
            selected_regions,
            selected_bubble,
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
        voicelines_project: &crate::voicelines::VoicelinesProject,
        comic_dubs_project: &crate::comic_dubs::ComicDubsProject,
        render_index: &ProjectRenderIndex,
        current_frame: i64,
        render_frame: f64,
        fps: f64,
        waveform: &[f32],
        waveform_offset_frames: i64,
        waveform_is_instrumental: bool,
    ) {
        if self.active_workspace == WorkspaceId::Voicelines {
            let selection_before = self.voicelines_ui.selected_regions().to_vec();
            self.voicelines_ui
                .sync(voicelines_project, self.voicelines_layout);
            if self.voicelines_ui.selected_regions() != selection_before {
                self.rebuild_topbar(self.network_in_room);
            }
            self.voicelines_scene = self.voicelines_ui.scene(
                voicelines_project,
                (render_frame.max(0.0) * 10.0).round() as u64,
                self.voicelines_layout,
            );
            self.refresh_root_focus_nodes();
        } else if self.active_workspace == WorkspaceId::ComicDubs {
            self.comic_dubs_ui
                .sync(comic_dubs_project, self.comic_dubs_layout);
            self.comic_dubs_scene = self
                .comic_dubs_ui
                .scene(comic_dubs_project, self.comic_dubs_layout);
            self.refresh_root_focus_nodes();
        }
        if let Some(modal) = self.project_transfer_modal.as_mut() {
            modal.refresh_countdown();
        }
        let rythmo_zone = self.active_rythmo_rect();
        self.rythmo_state.set_compact_empty_tracks(
            self.active_workspace == WorkspaceId::Recording
                && self.recording_ui.page == RecordingPage::Timeline,
        );
        let show_rythmo = shows_rythmo(self.active_workspace, self.recording_ui.page, rythmo_zone);
        let rythmo_editable = self.active_workspace == WorkspaceId::Rythmo;
        let recording_scene =
            (self.active_workspace == WorkspaceId::Recording).then(|| self.recording_scene.clone());
        let recording_audio_import_prompt = (self.active_workspace == WorkspaceId::Recording)
            .then(|| {
                self.recording_ui
                    .audio_import_prompt_scene(self.recording_layout.content)
            })
            .flatten();
        self.ensure_whats_new_thumbnail_texture(device, queue, renderer);
        self.ensure_comic_dubs_texture(device, queue, renderer, comic_dubs_project);
        // Update frame info for progress bar
        self.current_frame = current_frame;

        // Tick toasts (needs &mut self, before labels borrow self)
        self.toasts.tick();

        // Refresh the contextual shortcut panel when the situation changed
        // (needs &mut self, before labels borrow self).
        {
            use crate::workspaces::rythmo::view::Selection;
            let (line_selected, any_selection, detection_selected) =
                match self.rythmo_state.selected.as_ref() {
                    Some(Selection::Line(_) | Selection::Lines(_) | Selection::AllLines) => {
                        (true, true, false)
                    }
                    Some(Selection::Detection(_)) => (false, true, true),
                    Some(_) => (false, true, false),
                    None => (false, false, false),
                };
            let situation = shortcut_panel::PanelSituation {
                contexts: self.shortcut_contexts(),
                workspace: self.active_workspace,
                has_video: self.has_video,
                has_instrumental: project.settings().instrumental_audio_path.is_some(),
                line_selected,
                any_selection,
                detection_selected,
                hovered_line: self.rythmo_state.hovered_line.is_some(),
                editing_line: self.rythmo_state.editing_line.is_some(),
                editing_any: self.rythmo_state.is_editing(),
                has_lines: project.line_count() > 0,
                line_at_playhead: project
                    .lines()
                    .any(|line| current_frame >= line.start_frame && current_frame < line.end_frame()),
                line_clipboard_available: self.line_clipboard_available,
            };
            self.shortcut_panel
                .sync(&situation, shortcut_panel::shortcut_router().bindings());
        }

        // Prepare color picker textures first (needs &mut self, before labels borrow self)
        self.rythmo_state.color_picker.ensure_textures(
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );
        self.side_panel.ensure_color_picker_textures(
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );
        if self.active_workspace == WorkspaceId::ComicDubs {
            self.comic_dubs_ui.ensure_color_picker_textures(
                device,
                queue,
                renderer.texture_bind_group_layout(),
                renderer.texture_sampler(),
            );
        }
        self.actor_icon_cache.sync(
            project,
            device,
            queue,
            renderer.texture_bind_group_layout(),
            renderer.texture_sampler(),
        );

        // Update drawing overlay texture if needed
        if show_rythmo {
            self.update_drawing_overlay(
                device,
                queue,
                renderer,
                project,
                render_frame,
                fps,
                rythmo_zone,
            );
        }

        let mut color_picker_bg_quads: Vec<QuadInstance> = Vec::new();
        let mut base_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        let mut extra_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
        let mut color_picker_fg_quads: Vec<QuadInstance> = Vec::new();

        // Update the export/proxy task row title BEFORE borrowing self via
        // labels. The percentage itself is rendered by the task row.
        if self.export_progress.is_some() {
            use std::sync::atomic::Ordering;
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
            self.export_label = prefix.to_string();
        }
        // Consume pending cursor click if any before starting to collect labels referencing self
        let pending_click = self.rythmo_state.pending_cursor_click.take();

        let mut quads = Vec::new(); // base layer (behind video)
        let mut overlay_quads = Vec::new(); // overlay layer (on top of video)
        let mut icons: Vec<IconInstance> = Vec::new();
        let mut labels: Vec<LabelInfo> = Vec::new();
        let mut overlay_labels: Vec<LabelInfo> = Vec::new();
        let mut popup_quads: Vec<QuadInstance> = Vec::new();
        let mut popup_labels: Vec<LabelInfo> = Vec::new();
        let mut popup_icons: Vec<IconInstance> = Vec::new();
        let mut tooltip_quads: Vec<QuadInstance> = Vec::new();
        let mut tooltip_labels: Vec<LabelInfo> = Vec::new();
        let mut toast_quads: Vec<QuadInstance> = Vec::new();
        let mut toast_labels: Vec<LabelInfo> = Vec::new();
        let mut modal_quads: Vec<QuadInstance> = Vec::new(); // modal backgrounds (above normal text)
        let mut modal_labels: Vec<LabelInfo> = Vec::new(); // modal text (above modal backgrounds)
        let mut modal_overlay_quads: Vec<QuadInstance> = Vec::new();
        let mut modal_overlay_labels: Vec<LabelInfo> = Vec::new();
        let mut system_quads: Vec<QuadInstance> = Vec::new();
        let mut system_labels: Vec<LabelInfo> = Vec::new();
        let mut topmost_quads: Vec<QuadInstance> = Vec::new();
        let mut topmost_labels: Vec<LabelInfo> = Vec::new();
        let mut modal_overlay_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();

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
                &mut toast_quads,
                &mut toast_labels,
                self.screen_w,
                self.screen_h,
            );
            let stretched_quads: Vec<(IconInstance, u64)> = Vec::new();
            let syllable_quads: Vec<QuadInstance> = Vec::new();
            let base_textured: Vec<(IconInstance, &wgpu::BindGroup)> = Vec::new();
            let layers = [
                UiLayerBatch::new(UiLayer::Overlay, &overlay_quads, &overlay_labels),
                UiLayerBatch::new(UiLayer::Toast, &toast_quads, &toast_labels),
                UiLayerBatch::new(UiLayer::Modal, &modal_quads, &modal_labels),
                UiLayerBatch::new(
                    UiLayer::ModalOverlay,
                    &modal_overlay_quads,
                    &modal_overlay_labels,
                ),
            ];
            renderer.render(
                device,
                queue,
                encoder,
                view,
                screen_width,
                screen_height,
                ui_scale,
                &quads,
                &icons,
                &labels,
                None,
                &stretched_quads,
                &syllable_quads,
                &base_textured,
                &layers,
            );
            return;
        }

        // We can't mutate self after borrowing labels. So process click before ANY render stuff borrowing self.
        if let Some((ratio, is_shift)) = pending_click {
            if let Some(line_id) = self.rythmo_state.editing_line {
                let segmented_idx = project.get_line(line_id).and_then(|line| {
                    let lang = project.syllable_language_code();
                    rythmo::sync_cursor_segments_for_line(
                        project,
                        line,
                        self.rythmo_state.syllable_drag.as_ref(),
                        lang,
                        &self.rythmo_state,
                    )
                    .and_then(|segments| renderer.cursor_pos_from_segments(&segments, ratio))
                    .or_else(|| {
                        rythmo::cursor_segments_for_line(
                            line,
                            self.rythmo_state.syllable_drag.as_ref(),
                            lang,
                            self.playing,
                            &self.rythmo_state,
                        )
                        .and_then(|segments| renderer.cursor_pos_from_segments(&segments, ratio))
                    })
                    .or_else(|| {
                        rythmo::segmented_cursor_index_for_line_at_ratio(
                            line,
                            self.rythmo_state.syllable_drag.as_ref(),
                            lang,
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
            rythmo_zone,
            recording_scene.as_ref(),
        );
        if self.active_workspace == WorkspaceId::ComicDubs {
            if let (Some(rect), Some(texture)) = (
                self.comic_dubs_scene.page_rect,
                self.comic_dubs_texture.as_ref(),
            ) {
                base_textured.push((
                    IconInstance {
                        rect: [rect.x, rect.y, rect.width, rect.height],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0; 4],
                        transform: [0.0, 0.0, 0.5, 0.5],
                    },
                    &texture.bind_group,
                ));
            }
            comic_dubs_workspace::append_overlay(
                &mut overlay_quads,
                &mut overlay_labels,
                &self.comic_dubs_scene,
            );
        }
        if self.active_workspace == WorkspaceId::Rythmo {
            self.automation_editor.render(
                &self.layout.video_preview,
                &project.settings().automation,
                project,
                self.cursor_pos,
                &mut quads,
                &mut labels,
                &mut system_quads,
                &mut system_labels,
            );
        }

        // Rythmo lines
        let mut stretched_texts: Vec<StretchedText> = Vec::new();
        let mut syllable_quads: Vec<QuadInstance> = Vec::new();
        let mut note_icons: Vec<IconInstance> = Vec::new();
        let mut actor_icon_draws: Vec<rythmo::VoiceActorIconDraw> = Vec::new();
        let note_uv = self.uv("note");
        let detection_uvs = [
            "detection/labial",
            "detection/semi_labial",
            "detection/mouth_open",
            "detection/mouth_closed",
            "detection/teeth_visible",
            "detection/breath",
            "detection/reaction",
            "detection/th",
            "detection/neutral",
            "detection/pucker",
            "detection/rhubarb_lips/AA",
            "detection/rhubarb_lips/AO_ER",
            "detection/rhubarb_lips/EH_AE",
            "detection/rhubarb_lips/F_V",
            "detection/rhubarb_lips/K_S_T_EE",
            "detection/rhubarb_lips/L",
            "detection/rhubarb_lips/P_B_M",
            "detection/rhubarb_lips/UW_OW_W",
        ]
        .map(|name| self.uv(name));
        let lint_diagnostics = self.rythmo_state.cached_lint_diagnostics(project, fps);
        let lint_severities = self.rythmo_state.cached_lint_severities();
        let lint_zones = self.rythmo_state.cached_lint_zones();
        let cursor_info = show_rythmo
            .then(|| {
                rythmo::render_lines(
                    &rythmo_zone,
                    project,
                    render_index,
                    render_frame,
                    self.playing,
                    rythmo_editable,
                    fps,
                    &self.rythmo_state,
                    &lint_severities,
                    &mut quads,
                    &mut syllable_quads,
                    &mut labels,
                    &mut stretched_texts,
                    &mut note_icons,
                    &mut actor_icon_draws,
                    note_uv,
                    detection_uvs,
                )
            })
            .flatten();
        let lint_tooltip = if rythmo_editable
            && show_rythmo
            && self.rythmo_state.context_menu.is_none()
            && rythmo_zone.contains(self.cursor_pos.0, self.cursor_pos.1)
        {
            self.rythmo_state
                .hovered_line
                .and_then(|line_id| {
                    let diagnostics = project
                        .get_line(line_id)
                        .map(|line| crate::lint::for_line_in(&lint_diagnostics, line))
                        .unwrap_or_default();
                    (!diagnostics.is_empty()).then(|| {
                        LintTooltipState::new(&diagnostics, self.cursor_pos.0, self.cursor_pos.1)
                    })
                })
                .or_else(|| {
                    let diagnostics = rythmo::lint_zone_diagnostics(
                        &rythmo_zone,
                        project,
                        render_frame,
                        fps,
                        &lint_zones,
                        self.cursor_pos.0,
                        self.cursor_pos.1,
                    );
                    (!diagnostics.is_empty()).then(|| {
                        LintTooltipState::new(&diagnostics, self.cursor_pos.0, self.cursor_pos.1)
                    })
                })
        } else {
            None
        };
        icons.extend(note_icons);
        for draw in actor_icon_draws {
            if let Some(actor) = project.find_voice_actor(&draw.actor_name) {
                if let Some(bind_group) = self.actor_icon_cache.bind_group_for(actor) {
                    base_textured.push((
                        IconInstance {
                            rect: [draw.rect.x, draw.rect.y, draw.rect.width, draw.rect.height],
                            uv_rect: [0.0, 0.0, 1.0, 1.0],
                            tint: [1.0, 1.0, 1.0, 1.0],
                            transform: [0.0, 0.0, 0.5, 0.5],
                        },
                        bind_group,
                    ));
                }
            }
        }

        // Drawing overlay
        if show_rythmo {
            if let Some(cache) = &self.drawing_overlay_cache {
                let zone = &rythmo_zone;
                base_textured.push((
                    IconInstance {
                        rect: [zone.x, zone.y, zone.width, zone.height],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0, 1.0, 1.0, 1.0],
                        transform: [0.0, 0.0, 0.5, 0.5],
                    },
                    &cache.bind_group,
                ));
            }
        }

        // Selection overlay (marquee + selected-strokes bbox & handles).
        // Drawn into overlay_quads so it composites above the drawing overlay.
        if show_rythmo {
            let zone = &rythmo_zone;
            rythmo::render_selection_overlay(
                zone,
                render_frame,
                project,
                &self.rythmo_state,
                &mut overlay_quads,
            );
        }

        // Eraser cursor ring (visible like the pencil preview)
        if self.active_workspace == WorkspaceId::Rythmo
            && self.erasing
            && self.active_mode == Some(ToolMode::Draw)
        {
            let zone = &rythmo_zone;
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
        if show_rythmo {
            rythmo::render_markers(
                &rythmo_zone,
                project,
                render_index,
                render_frame,
                fps,
                if rythmo_editable { &lint_zones } else { &[] },
                &mut quads,
                &mut labels,
                &mut liaison_icons,
                self.uv("liaison_left"),
                self.uv("liaison_right"),
            );
            rythmo::render_ambiance_liaison_icons(
                &rythmo_zone,
                project,
                render_index,
                render_frame,
                fps,
                &mut liaison_icons,
                self.uv("liaison_left"),
                self.uv("liaison_right"),
            );
        }
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
        if !self.recording_capture_view {
            for widget in self
                .topbar_widgets
                .iter()
                .chain(self.tab_widgets.iter())
                .chain(self.toolbar_widgets.iter())
            {
                if !widget.captures_all() {
                    quads.extend(widget.render_quads());
                    icons.extend(widget.render_icons());
                    labels.extend(widget.labels());
                }
            }
        }

        let focused_bounds = self
            .focused_widget()
            .map(Widget::bounds)
            .or_else(|| {
                self.focused_recording_control()
                    .map(|control| control.bounds)
            })
            .or_else(|| {
                let id = self.focus.current_id()?.0.as_str();
                self.voicelines_scene
                    .controls
                    .iter()
                    .find(|control| control.id == id)
                    .map(|control| control.bounds)
            })
            .or_else(|| {
                let id = self.focus.current_id()?.0.as_str();
                self.comic_dubs_scene
                    .controls
                    .iter()
                    .find(|control| control.id == id)
                    .map(|control| control.bounds)
            });
        if let Some(bounds) = focused_bounds {
            overlay_quads.push(QuadInstance {
                rect: [
                    bounds.x - 2.0,
                    bounds.y - 2.0,
                    bounds.width + 4.0,
                    bounds.height + 4.0,
                ],
                color: [0.0; 4],
                color_bottom: [0.0; 4],
                border_color: [0.25, 0.52, 1.0, 1.0],
                border_width: 2.0,
                border_radius: 6.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Transient workspace surfaces use the popup layer.
        if self.active_workspace == WorkspaceId::Rythmo {
            rythmo::render_autocomplete(
                &rythmo_zone,
                project,
                render_frame,
                &self.rythmo_state,
                &mut popup_quads,
                &mut popup_labels,
                fps,
            );
        }

        // Panels are true overlays: drawing them here keeps line textures,
        // actor icons and the drawing layer from ever bleeding into the panel.
        if self.active_workspace == WorkspaceId::Rythmo {
            if let Some(panel) = self.layout.properties {
                self.side_panel
                    .render(panel, project, &mut overlay_quads, &mut overlay_labels);
            }
        }

        if self.active_workspace == WorkspaceId::Rythmo {
            self.side_panel
                .render_menus(project, &mut system_quads, &mut system_labels);
        }

        // Capturing dropdowns belong above persistent overlays such as the
        // side panel, with their text in the same semantic layer.
        if !self.recording_capture_view {
            for widget in self
                .topbar_widgets
                .iter()
                .chain(self.tab_widgets.iter())
                .chain(self.toolbar_widgets.iter())
            {
                if widget.captures_all() {
                    popup_quads.extend(widget.render_quads());
                    popup_icons.extend(widget.render_icons());
                    popup_labels.extend(widget.labels());
                }
            }
        }

        if self.active_workspace == WorkspaceId::Rythmo {
            self.rythmo_state.color_picker.render(
                &mut color_picker_bg_quads,
                &mut extra_textured,
                &mut color_picker_fg_quads,
            );
            self.side_panel.render_color_picker(
                &mut color_picker_bg_quads,
                &mut extra_textured,
                &mut color_picker_fg_quads,
            );
        } else if self.active_workspace == WorkspaceId::ComicDubs {
            self.comic_dubs_ui.render_color_picker(
                &mut color_picker_bg_quads,
                &mut extra_textured,
                &mut color_picker_fg_quads,
            );
        }

        // Color picker quads → overlay
        popup_quads.extend(color_picker_bg_quads);

        // Toolbar dropdown → overlay
        if self.active_workspace == WorkspaceId::Rythmo {
            self.render_toolbar_dropdown(&mut popup_quads, &mut popup_labels);
        }

        if self.active_workspace == WorkspaceId::Rythmo {
            rythmo::render_context_menu(
                project,
                self.screen_w,
                self.screen_h,
                &self.rythmo_state,
                &mut system_quads,
                &mut system_labels,
            );
        }

        // Tooltip → overlay
        if let Some(tooltip) = lint_tooltip.as_ref() {
            tooltip_quads.extend(tooltip.render_quads(self.screen_w));
            tooltip_labels.extend(tooltip.render_labels(self.screen_w));
        } else if let Some(tooltip) = &self.tooltip {
            tooltip_quads.extend(tooltip.render_quads(self.screen_w));
            tooltip_labels.extend(tooltip.render_labels(self.screen_w));
        }

        // Contextual shortcut hints → overlay (bottom-left, below popups and
        // modal layers so it never covers interactive chrome). Controlled by
        // the "show controls" application setting.
        if crate::config::get().ui.show_controls_hint && !self.shortcut_panel.is_empty() {
            overlay_quads.extend(self.shortcut_panel.render_quads(self.screen_h));
            overlay_labels.extend(self.shortcut_panel.render_labels(self.screen_h));
        }

        // Sync overlay (blocks UI during video transfer)
        if let Some(msg) = &self.sync_overlay {
            let (overlay_quads, overlay_labels) = (&mut system_quads, &mut system_labels);
            let dw = 420.0;
            let has_progress = self.sync_progress > 0.0;
            let dh = if has_progress { 100.0 } else { 64.0 };
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
            overlay_labels.push(LabelInfo {
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
            // Waiting overlays do not need a progress bar; show it only once
            // bytes are actually moving.
            if has_progress {
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
        }

        self.modal_host.render_base(
            &mut modal_quads,
            &mut modal_labels,
            self.screen_w,
            self.screen_h,
        );

        if let Some(modal) = &self.project_transfer_modal {
            modal.render(
                &mut modal_quads,
                &mut modal_labels,
                self.screen_w,
                self.screen_h,
            );
        }

        // Background tasks (project import, export, proxy) surface as
        // non-blocking task rows at the top center of the screen.
        let task_rows = self.task_row_views();
        task_row::render_task_rows(
            &task_rows,
            &mut system_quads,
            &mut system_labels,
            self.screen_w,
            self.screen_h,
        );

        if self.active_workspace == WorkspaceId::Recording
            && self.recording_ui.page == RecordingPage::Timeline
        {
            if let (Some(scene), Some(assets)) =
                (recording_scene.as_ref(), self.recording_layout.assets)
            {
                Self::append_recording_assets_overlay(
                    &mut overlay_quads,
                    &mut overlay_labels,
                    scene,
                    assets,
                );
            }
        }

        // Toasts
        self.toasts.render(
            &mut toast_quads,
            &mut toast_labels,
            self.screen_w,
            self.screen_h,
        );

        if self.active_workspace == WorkspaceId::Recording {
            if let Some(scene) = recording_scene.as_ref() {
                Self::append_recording_system(&mut system_quads, &mut system_labels, scene);
            }
        } else if self.active_workspace == WorkspaceId::Voicelines {
            voicelines_workspace::append_system(
                &mut system_quads,
                &mut system_labels,
                &self.voicelines_scene,
            );
        }

        self.modal_host.render_top(
            &mut modal_quads,
            &mut modal_labels,
            &mut modal_overlay_quads,
            &mut modal_overlay_labels,
            &mut topmost_quads,
            &mut topmost_labels,
            self.screen_w,
            self.screen_h,
        );

        if self.modal_host.save_prompt.is_none() {
            if let Some(scene) = recording_audio_import_prompt.as_ref() {
                Self::append_recording_scene(
                    &mut modal_overlay_quads,
                    &mut modal_overlay_labels,
                    scene,
                );
            }
        }

        if let (Some(modal), Some(texture)) = (
            self.modal_host.whats_new.as_ref(),
            self.whats_new_thumbnail_texture.as_ref(),
        ) {
            if let Some(rect) = modal.attachment_rect(self.screen_w, self.screen_h) {
                modal_overlay_textured.push((
                    IconInstance {
                        rect: [rect.x, rect.y, rect.width, rect.height],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0; 4],
                        transform: [0.0, 0.0, 0.5, 0.5],
                    },
                    &texture.bind_group,
                ));
            }
        }

        let layers = [
            UiLayerBatch::new(UiLayer::Overlay, &overlay_quads, &overlay_labels),
            UiLayerBatch {
                layer: UiLayer::Popup,
                quads: &popup_quads,
                textured: &extra_textured,
                foreground_quads: &color_picker_fg_quads,
                icons: &popup_icons,
                labels: &popup_labels,
            },
            UiLayerBatch::new(UiLayer::Tooltip, &tooltip_quads, &tooltip_labels),
            UiLayerBatch::new(UiLayer::Toast, &toast_quads, &toast_labels),
            UiLayerBatch {
                layer: UiLayer::Modal,
                quads: &modal_quads,
                textured: &[],
                foreground_quads: &[],
                icons: &[],
                labels: &modal_labels,
            },
            UiLayerBatch {
                layer: UiLayer::ModalOverlay,
                quads: &modal_overlay_quads,
                textured: &modal_overlay_textured,
                foreground_quads: &[],
                icons: &[],
                labels: &modal_overlay_labels,
            },
            UiLayerBatch::new(UiLayer::System, &system_quads, &system_labels),
            UiLayerBatch::new(UiLayer::Topmost, &topmost_quads, &topmost_labels),
        ];

        renderer.render(
            device,
            queue,
            encoder,
            view,
            screen_width,
            screen_height,
            ui_scale,
            &quads,
            &icons,
            &labels,
            video_quad,
            &stretched_quads,
            &syllable_quads,
            &base_textured,
            &layers,
        );
    }

    pub fn render_recording_daw(
        &mut self,
        renderer: &mut UiRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let mut quads = vec![QuadInstance {
            rect: [0.0, 0.0, width as f32, height as f32],
            color: [0.055, 0.058, 0.073, 1.0],
            color_bottom: [0.055, 0.058, 0.073, 1.0],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }];
        if let Some(toolbar) = self.recording_daw_layout.toolbar {
            self.push_toolbar_zone(&mut quads, toolbar, false, true);
        }
        let scene = &self.recording_daw_scene;
        quads.extend(scene.quads.iter().copied());
        let mut labels = Vec::new();
        Self::append_recording_drag_preview(
            &mut quads,
            &mut labels,
            scene,
            self.recording_ui.dragging_asset_id(),
            self.recording_daw_layout,
            self.recording_ui.pixels_per_frame,
            self.recording_daw_cursor,
        );
        if let Some(handle) = self.recording_daw_layout.assets_split_handle_rect() {
            let active = handle.contains(self.recording_daw_cursor.0, self.recording_daw_cursor.1)
                || self.dragging_recording_split == Some(RecordingSplitHandle::Assets);
            let color = if active {
                [0.45, 0.55, 0.95, 0.95]
            } else {
                [0.20, 0.20, 0.24, 0.75]
            };
            quads.push(QuadInstance {
                rect: [
                    handle.x + handle.width * 0.5 - 1.0,
                    handle.y,
                    2.0,
                    handle.height,
                ],
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
        for label in &scene.labels {
            labels.push(LabelInfo {
                text: &label.text,
                bounds: label.bounds,
                h_align: label.h_align,
                v_align: label.v_align,
                overflow: label.overflow,
                padding: 0.0,
                font_size_override: Some(label.font_size),
                color_override: Some(label.color),
                font_family_override: None,
            });
        }
        let mut icons = Vec::new();
        for widget in &self.recording_daw_toolbar_widgets {
            quads.extend(widget.render_quads());
            icons.extend(widget.render_icons());
            labels.extend(widget.labels());
        }
        let audio_import_prompt = self
            .recording_ui
            .audio_import_prompt_scene(self.recording_daw_layout.content);
        let mut modal_quads = Vec::new();
        let mut modal_labels = Vec::new();
        if let Some(prompt) = audio_import_prompt.as_ref() {
            Self::append_recording_scene(&mut modal_quads, &mut modal_labels, prompt);
        }
        let mut system_quads = Vec::new();
        let mut system_labels = Vec::new();
        Self::append_recording_system(&mut system_quads, &mut system_labels, scene);
        let layers = [
            UiLayerBatch::new(UiLayer::Modal, &modal_quads, &modal_labels),
            UiLayerBatch::new(UiLayer::System, &system_quads, &system_labels),
        ];
        renderer.render(
            device,
            queue,
            encoder,
            view,
            width,
            height,
            1.0,
            &quads,
            &icons,
            &labels,
            None,
            &[],
            &[],
            &[],
            &layers,
        );
    }

    fn ensure_comic_dubs_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &UiRenderer,
        project: &crate::comic_dubs::ComicDubsProject,
    ) {
        if self.active_workspace != WorkspaceId::ComicDubs {
            return;
        }
        let Some(page) = project.active_page() else {
            self.comic_dubs_texture = None;
            return;
        };
        if self
            .comic_dubs_texture
            .as_ref()
            .is_some_and(|texture| texture.page_id == page.id && texture.path == page.image_path)
        {
            return;
        }
        let image = match image::open(&page.image_path) {
            Ok(image) => image.to_rgba8(),
            Err(error) => {
                log::warn!(
                    "Could not render Comic Dubs page {}: {error}",
                    page.image_path.display()
                );
                self.comic_dubs_texture = None;
                return;
            }
        };
        let (width, height) = image.dimensions();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Comic Dubs page"),
            size: wgpu::Extent3d {
                width,
                height,
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
            image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Comic Dubs page bind group"),
            layout: renderer.texture_bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(renderer.texture_sampler()),
                },
            ],
        });
        self.comic_dubs_texture = Some(ComicDubsTexture {
            page_id: page.id,
            path: page.image_path.clone(),
            _texture: texture,
            bind_group,
        });
    }

    fn ensure_whats_new_thumbnail_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &UiRenderer,
    ) {
        let Some(image) = self
            .modal_host
            .whats_new
            .as_ref()
            .and_then(|modal| modal.thumbnail.as_ref())
            .cloned()
        else {
            self.whats_new_thumbnail_texture = None;
            return;
        };

        if self
            .whats_new_thumbnail_texture
            .as_ref()
            .is_some_and(|texture| texture.width == image.width && texture.height == image.height)
        {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("What's New YouTube Thumbnail"),
            size: wgpu::Extent3d {
                width: image.width,
                height: image.height,
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
            &image.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("What's New YouTube Thumbnail BG"),
            layout: renderer.texture_bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(renderer.texture_sampler()),
                },
            ],
        });
        self.whats_new_thumbnail_texture = Some(WhatsNewThumbnailTexture {
            _texture: texture,
            bind_group,
            width: image.width,
            height: image.height,
        });
    }

    fn append_recording_scene<'a>(
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        scene: &'a RecordingScene,
    ) {
        quads.extend(scene.quads.iter().copied());
        labels.extend(scene.labels.iter().map(|label| LabelInfo {
            text: &label.text,
            bounds: label.bounds,
            h_align: label.h_align,
            v_align: label.v_align,
            overflow: label.overflow,
            padding: 0.0,
            font_size_override: Some(label.font_size),
            color_override: Some(label.color),
            font_family_override: None,
        }));
    }

    fn append_recording_system<'a>(
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        scene: &'a RecordingScene,
    ) {
        quads.extend(scene.system_quads.iter().copied());
        labels.extend(scene.system_labels.iter().map(|label| LabelInfo {
            text: &label.text,
            bounds: label.bounds,
            h_align: label.h_align,
            v_align: label.v_align,
            overflow: label.overflow,
            padding: 0.0,
            font_size_override: Some(label.font_size),
            color_override: Some(label.color),
            font_family_override: None,
        }));
    }

    fn append_recording_drag_preview<'a>(
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        scene: &'a RecordingScene,
        dragging_asset: Option<crate::recording::AudioAssetId>,
        layout: RecordingLayout,
        pixels_per_frame: f32,
        cursor: (f32, f32),
    ) {
        let Some(asset_id) = dragging_asset else {
            return;
        };
        let Some(body) = layout.track_body else {
            return;
        };
        let Some(asset) = scene.controls.iter().find(
            |control| matches!(control.control, RecordingControl::Asset(id) if id == asset_id),
        ) else {
            return;
        };

        let floating = Rect {
            x: cursor.0 + 14.0,
            y: cursor.1 + 14.0,
            width: 190.0,
            height: 34.0,
        };
        quads.push(Self::recording_preview_quad(
            floating,
            [0.24, 0.20, 0.52, 0.94],
            [0.65, 0.70, 1.0, 1.0],
        ));
        labels.push(LabelInfo {
            text: &asset.label,
            bounds: Rect {
                x: floating.x + 8.0,
                y: floating.y,
                width: floating.width - 16.0,
                height: floating.height,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([240, 242, 255]),
            font_family_override: None,
        });

        if body.contains(cursor.0, cursor.1) {
            let row = ((cursor.1 - body.y) / TRACK_ROW_H).floor().max(0.0);
            let width = 120.0_f32.min(body.width.max(0.0));
            let x = (body.x
                + ((cursor.0 - body.x) / pixels_per_frame.max(0.001)).round()
                    * pixels_per_frame.max(0.001))
            .clamp(body.x, (body.x + body.width - width).max(body.x));
            let target = Rect {
                x,
                y: body.y + row * TRACK_ROW_H + 6.0,
                width,
                height: 46.0,
            };
            quads.push(Self::recording_preview_quad(
                target,
                [0.28, 0.42, 0.82, 0.52],
                [0.65, 0.75, 1.0, 0.95],
            ));
            labels.push(LabelInfo {
                text: &asset.label,
                bounds: Rect {
                    x: target.x + 6.0,
                    y: target.y + 2.0,
                    width: target.width - 12.0,
                    height: 18.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(10.0),
                color_override: Some([235, 240, 255]),
                font_family_override: None,
            });
        }
    }

    fn recording_preview_quad(rect: Rect, color: [f32; 4], border_color: [f32; 4]) -> QuadInstance {
        QuadInstance {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color,
            color_bottom: color,
            border_color,
            border_width: 1.0,
            border_radius: 5.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }
    }

    fn append_recording_assets_overlay<'a>(
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        scene: &'a RecordingScene,
        assets: Rect,
    ) {
        for quad in &scene.quads {
            let [x, y, width, height] = quad.rect;
            if x >= assets.x
                && y >= assets.y
                && x + width <= assets.x + assets.width
                && y + height <= assets.y + assets.height
            {
                quads.push(*quad);
            }
        }
        labels.extend(scene.labels.iter().filter_map(|label| {
            let bounds = label.bounds;
            (bounds.x >= assets.x
                && bounds.y >= assets.y
                && bounds.x + bounds.width <= assets.x + assets.width
                && bounds.y + bounds.height <= assets.y + assets.height)
                .then_some(LabelInfo {
                    text: &label.text,
                    bounds,
                    h_align: label.h_align,
                    v_align: label.v_align,
                    overflow: label.overflow,
                    padding: 0.0,
                    font_size_override: Some(label.font_size),
                    color_override: Some(label.color),
                    font_family_override: None,
                })
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn push_rythmo_base(
        &self,
        quads: &mut Vec<QuadInstance>,
        zone: &Rect,
        project: &Project,
        render_index: &ProjectRenderIndex,
        render_frame: f64,
        fps: f64,
        waveform: &[f32],
        waveform_offset_frames: i64,
        waveform_is_instrumental: bool,
    ) {
        quads.push(QuadInstance {
            rect: [zone.x, zone.y, zone.width, zone.height],
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
            zone,
            project,
            render_index,
            render_frame,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
            self.playing,
            fps,
            &self.rythmo_state,
        ));
    }

    fn push_toolbar_zone(
        &self,
        quads: &mut Vec<QuadInstance>,
        toolbar: Rect,
        editable: bool,
        playback_enabled: bool,
    ) {
        quads.push(QuadInstance {
            rect: [toolbar.x, toolbar.y, toolbar.width, toolbar.height],
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
        if !playback_enabled {
            return;
        }
        let pb = shell::progress_bar_rect(&toolbar, editable);
        let progress = playback_progress(self.current_frame, self.total_frames);
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
        if self.total_frames <= 0 {
            return;
        }
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

    fn render_zones<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        project: &'a Project,
        render_frame: f64,
        render_index: &ProjectRenderIndex,
        fps: f64,
        waveform: &[f32],
        waveform_offset_frames: i64,
        waveform_is_instrumental: bool,
        rythmo_zone: Rect,
        recording_scene: Option<&'a RecordingScene>,
    ) {
        let l = &self.layout;

        if self.active_workspace == WorkspaceId::Recording && self.recording_capture_view {
            self.push_rythmo_base(
                quads,
                &rythmo_zone,
                project,
                render_index,
                render_frame,
                fps,
                waveform,
                waveform_offset_frames,
                waveform_is_instrumental,
            );
            if let Some(scene) = recording_scene {
                Self::append_recording_scene(quads, labels, scene);
                Self::append_recording_drag_preview(
                    quads,
                    labels,
                    scene,
                    self.recording_ui.dragging_asset_id(),
                    self.recording_layout,
                    self.recording_ui.pixels_per_frame,
                    self.cursor_pos,
                );
            }
            let handle = self.recording_layout.rythmo_split_handle_rect();
            let active = handle.contains(self.cursor_pos.0, self.cursor_pos.1)
                || self.dragging_recording_split == Some(RecordingSplitHandle::Rythmo);
            let color = if active {
                [0.45, 0.55, 0.95, 0.95]
            } else {
                [0.20, 0.20, 0.24, 0.75]
            };
            quads.push(QuadInstance {
                rect: [
                    handle.x,
                    handle.y + handle.height * 0.5 - 1.0,
                    handle.width,
                    2.0,
                ],
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
            return;
        }

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

        // Workspace tabs are part of the shared shell and always span the
        // complete window, independently of optional side panels.
        quads.push(QuadInstance {
            rect: [l.tabs.x, l.tabs.y, l.tabs.width, l.tabs.height],
            color: [0.09, 0.09, 0.11, 1.0],
            color_bottom: [0.09, 0.09, 0.11, 1.0],
            border_color: [0.18, 0.18, 0.22, 0.8],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0, 1.0],
            shadow_color: [0.0, 0.0, 0.0, 0.25],
            shadow_blur: 2.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        if self.active_workspace == WorkspaceId::Recording {
            if self.recording_daw_detached && self.recording_ui.page == RecordingPage::Timeline {
                quads.push(QuadInstance {
                    rect: [
                        self.recording_layout.video.x,
                        self.recording_layout.video.y,
                        self.recording_layout.video.width,
                        self.recording_layout.video.height,
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
                self.push_rythmo_base(
                    quads,
                    &rythmo_zone,
                    project,
                    render_index,
                    render_frame,
                    fps,
                    waveform,
                    waveform_offset_frames,
                    waveform_is_instrumental,
                );
                let handle = self.recording_layout.video_split_handle_rect();
                let active = handle.contains(self.cursor_pos.0, self.cursor_pos.1)
                    || self.dragging_recording_split == Some(RecordingSplitHandle::Video);
                let color = if active {
                    [0.45, 0.55, 0.95, 0.95]
                } else {
                    [0.20, 0.20, 0.24, 0.75]
                };
                quads.push(QuadInstance {
                    rect: [
                        handle.x,
                        handle.y + handle.height * 0.5 - 1.0,
                        handle.width,
                        2.0,
                    ],
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
                return;
            }
            if let Some(toolbar) = self.recording_layout.toolbar {
                self.push_toolbar_zone(
                    quads,
                    toolbar,
                    false,
                    self.recording_playback_controls_enabled(),
                );
            }
            if self.recording_ui.page == RecordingPage::Timeline {
                self.push_rythmo_base(
                    quads,
                    &rythmo_zone,
                    project,
                    render_index,
                    render_frame,
                    fps,
                    waveform,
                    waveform_offset_frames,
                    waveform_is_instrumental,
                );
                let handles = [
                    self.recording_layout.video_split_handle_rect(),
                    self.recording_layout.rythmo_split_handle_rect(),
                ];
                for rect in handles {
                    let active = rect.contains(self.cursor_pos.0, self.cursor_pos.1);
                    let color = if active {
                        [0.45, 0.55, 0.95, 0.95]
                    } else {
                        [0.20, 0.20, 0.24, 0.55]
                    };
                    quads.push(QuadInstance {
                        rect: [rect.x, rect.y + rect.height / 2.0 - 1.0, rect.width, 2.0],
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
                if let Some(rect) = self.recording_layout.assets_split_handle_rect() {
                    let active = rect.contains(self.cursor_pos.0, self.cursor_pos.1);
                    let color = if active {
                        [0.45, 0.55, 0.95, 0.95]
                    } else {
                        [0.20, 0.20, 0.24, 0.55]
                    };
                    quads.push(QuadInstance {
                        rect: [rect.x + rect.width / 2.0 - 1.0, rect.y, 2.0, rect.height],
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
            }
            if let Some(scene) = recording_scene {
                Self::append_recording_scene(quads, labels, scene);
                Self::append_recording_drag_preview(
                    quads,
                    labels,
                    scene,
                    self.recording_ui.dragging_asset_id(),
                    self.recording_layout,
                    self.recording_ui.pixels_per_frame,
                    self.cursor_pos,
                );
            }
            return;
        }

        if self.active_workspace == WorkspaceId::Voicelines {
            voicelines_workspace::append_scene(quads, labels, &self.voicelines_scene);
            self.push_toolbar_zone(quads, self.voicelines_layout.toolbar, false, true);
            return;
        }

        if self.active_workspace == WorkspaceId::ComicDubs {
            comic_dubs_workspace::append_scene(quads, labels, &self.comic_dubs_scene);
            self.push_toolbar_zone(quads, self.comic_dubs_layout.toolbar, false, true);
            return;
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

        self.push_toolbar_zone(quads, l.toolbar, true, true);

        self.push_rythmo_base(
            quads,
            &rythmo_zone,
            project,
            render_index,
            render_frame,
            fps,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
        );

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
    }

    fn update_drawing_overlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut UiRenderer,
        project: &Project,
        current_frame: f64,
        fps: f64,
        zone: Rect,
    ) {
        use crate::rythmo_drawing::{rasterize_window, visible_frame_window, DrawingStroke};

        let zone = &zone;
        let zw = zone.width.max(1.0) as u32;
        let zh = zone.height.max(1.0) as u32;
        let cf = current_frame;
        let ppf = crate::rythmo_drawing::ppf_for_scale(1.0, project.settings().scroll_speed);
        let reading_bar_offset_seconds = crate::rythmo_layout::reading_bar_offset_seconds(
            project.settings().reading_bar_offset_percent,
            zone.width,
            fps,
            ppf,
        );

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
            project.drawing_revision(),
            active_stroke_len,
        );

        // Check if we need to re-rasterize. A live transform drag mutates the
        // actual stroke points without changing the revision, so force an
        // update while a transform handle is active to keep strokes in sync.
        let transform_active = self.rythmo_state.transform_handle.is_some();
        let needs_update = self.rythmo_state.drawing_dirty
            || transform_active
            || self
                .drawing_overlay_cache
                .as_ref()
                .is_none_or(|c| c.key != key);

        if needs_update {
            // Collect visible strokes
            let (first_frame, last_frame) =
                visible_frame_window(zone.width, cf, ppf, 4, fps, reading_bar_offset_seconds);
            let mut strokes: Vec<&DrawingStroke> =
                project.drawing().query_window(first_frame, last_frame);

            // Add active stroke for live preview
            if let Some(ref active) = self.rythmo_state.active_stroke {
                if active.points.len() > 1 {
                    strokes.push(active);
                }
            }

            if !strokes.is_empty() {
                let rgba =
                    rasterize_window(&strokes, zw, zh, cf, ppf, fps, reading_bar_offset_seconds);

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
    key: (i64, u32, u32, u64, usize),
    zw: u32,
    zh: u32,
}

#[cfg(test)]
mod playback_progress_tests {
    use super::playback_progress;

    #[test]
    fn final_frame_fills_even_a_short_progress_bar() {
        assert_eq!(playback_progress(19, 20), 1.0);
        assert_eq!(playback_progress(0, 0), 0.0);
    }
}

#[cfg(test)]
mod recording_capture_view_tests {
    use super::recording_workspace::RecordingControlInfo;
    use super::{
        recording_drop_target, shows_rythmo, uses_recording_capture_view, RecordingControl,
        RecordingLayout, RecordingPage, RecordingRole, RecordingScene, RecordingWorkspaceUi, Rect,
        WorkspaceId, TRACK_ROW_H,
    };
    use crate::ui::focus::AccessibleRole;

    #[test]
    fn actor_stays_on_record_view_between_takes() {
        assert!(uses_recording_capture_view(
            RecordingPage::Timeline,
            RecordingRole::Actor,
            false
        ));
        assert!(!uses_recording_capture_view(
            RecordingPage::Timeline,
            RecordingRole::Director,
            false
        ));
    }

    #[test]
    fn voicelines_never_renders_the_rythmo_workspace() {
        let visible_zone = Rect {
            width: 800.0,
            height: 300.0,
            ..Rect::default()
        };
        assert!(!shows_rythmo(
            WorkspaceId::Voicelines,
            RecordingPage::Timeline,
            visible_zone
        ));
        assert!(shows_rythmo(
            WorkspaceId::Recording,
            RecordingPage::Timeline,
            visible_zone
        ));
    }

    #[test]
    fn audio_drop_uses_the_track_row_and_timeline_position() {
        let layout = RecordingLayout::daw(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1_000.0,
                height: 500.0,
            },
            0.25,
        );
        let body = layout.track_body.unwrap();
        let first = crate::recording::AudioTrackId::new(1);
        let second = crate::recording::AudioTrackId::new(2);
        let mut scene = RecordingScene::default();
        for track_id in [first, second] {
            scene.controls.push(RecordingControlInfo {
                control: RecordingControl::TrackMute(track_id),
                bounds: body,
                role: AccessibleRole::Button,
                label: String::new(),
                value: None,
                selected: false,
                enabled: true,
            });
        }
        let mut ui = RecordingWorkspaceUi::default();
        ui.view_start_frame = 10.0;
        ui.pixels_per_frame = 2.0;

        assert_eq!(
            recording_drop_target(
                layout,
                &scene,
                &ui,
                body.x + 20.0,
                body.y + TRACK_ROW_H + 1.0,
            ),
            Some((second, 20))
        );
    }
}
