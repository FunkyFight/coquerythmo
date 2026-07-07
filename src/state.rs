use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use wgpu::CurrentSurfaceTexture;
use winit::window::{Window, WindowId};

use std::time::{Duration, Instant};

use crate::command::{Command, CommandHistory, CommandKind, LineMove};
use crate::graphics::{GraphicsContext, WindowSurface};
use crate::network::{ConnectionState, IncomingMessage, NetworkClient};
use crate::observer::{TimelineBus, TimelineEvent};
use crate::packet::{CommandPayload, Packet, ProjectData};
use crate::project::{Character, LineCharacterNameChange, Project};
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_line::RythmoLine;
use crate::ui::renderer::UiRenderer;
use crate::ui::widget::{EventResponse, UiEvent};
use crate::ui::Ui;
use crate::video::{AudioTrack, VideoPlayer};
use crate::voice_actor::{LineVoiceActorsChange, VoiceActor};

use crate::constants;

fn parse_color(v: &serde_json::Value) -> [f32; 4] {
    if let Some(arr) = v.as_array() {
        let mut c = [0.0f32; 4];
        for (i, val) in arr.iter().enumerate().take(4) {
            c[i] = val.as_f64().unwrap_or(0.0) as f32;
        }
        c
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

/// Result from a background server ping.
pub struct PingResult {
    pub ip: String,
    pub port: u16,
    pub name: String,
    pub motd: String,
    pub online: u32,
    pub max_slots: u32,
    pub success: bool,
}

struct PendingProxyJob {
    source_path: PathBuf,
    receiver: Receiver<Result<PathBuf, String>>,
}

struct PendingExportJob {
    receiver: Receiver<Result<(), String>>,
}

enum DialogueSplitTarget {
    Cursor { line_id: u64, cursor_pos: usize },
    Playhead { line_id: u64, progress: f32 },
}

pub struct State {
    pub gfx: GraphicsContext,
    window: Arc<Window>,
    ui_scale: f32,
    ui: Ui,
    ui_renderer: UiRenderer,
    video_player: Option<VideoPlayer>,
    source_video_path: Option<PathBuf>,
    source_video_size: Option<(u32, u32)>,
    proxy_video_path: Option<PathBuf>,
    pending_proxy_job: Option<PendingProxyJob>,
    pending_export_job: Option<PendingExportJob>,
    active_export_cancel: Option<Arc<AtomicBool>>,
    pub project: Project,
    render_index: ProjectRenderIndex,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    history: CommandHistory,
    pub timeline: TimelineBus,
    pub network: NetworkClient,
    last_scroll_time: Option<Instant>,
    scroll_needs_decode: bool,
    ping_results: std::sync::Arc<std::sync::Mutex<Vec<PingResult>>>,
    last_autosave: Instant,
    last_redraw: Instant,
    last_waveform_revision: u64,
    studio_mode: bool,
    fullscreen_before_studio: Option<winit::window::Fullscreen>,
    show_studio_warning: bool,
    line_clipboard: Option<RythmoLine>,
    last_nonzero_volume: f32,
    secondary_display: Option<WindowSurface>,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window.clone()).await;
        let format = gfx.surface_format();
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, format);
        let ui_scale = Self::window_ui_scale(&window);
        let (ui_width, ui_height) = Self::logical_ui_size(gfx.size, ui_scale);
        let ui = Ui::new(ui_width, ui_height, &ui_renderer.icon_atlas);

        Self {
            gfx,
            window,
            ui_scale,
            ui,
            ui_renderer,
            video_player: None,
            source_video_path: None,
            source_video_size: None,
            proxy_video_path: None,
            pending_proxy_job: None,
            pending_export_job: None,
            active_export_cancel: None,
            project: Project::new(),
            render_index: ProjectRenderIndex::new(),
            project_path: None,
            dirty: false,
            history: CommandHistory::new(),
            timeline: TimelineBus::new(),
            network: NetworkClient::new(),
            last_scroll_time: None,
            scroll_needs_decode: false,
            ping_results: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            last_autosave: Instant::now(),
            last_redraw: Instant::now(),
            last_waveform_revision: 0,
            studio_mode: false,
            fullscreen_before_studio: None,
            show_studio_warning: false,
            line_clipboard: None,
            last_nonzero_volume: 0.75,
            secondary_display: None,
        }
    }

    #[cfg(target_os = "macos")]
    fn window_ui_scale(window: &Window) -> f32 {
        (window.scale_factor() as f32).max(1.0)
    }

    #[cfg(not(target_os = "macos"))]
    fn window_ui_scale(_window: &Window) -> f32 {
        1.0
    }

    fn logical_ui_size(physical_size: winit::dpi::PhysicalSize<u32>, ui_scale: f32) -> (u32, u32) {
        let ui_scale = ui_scale.max(1.0);
        (
            ((physical_size.width as f32 / ui_scale).round() as u32).max(1),
            ((physical_size.height as f32 / ui_scale).round() as u32).max(1),
        )
    }

    // -- Delegation helpers --

    fn renderer_refs(&self) -> (&wgpu::BindGroupLayout, &wgpu::Sampler) {
        (
            self.ui_renderer.texture_bind_group_layout(),
            self.ui_renderer.texture_sampler(),
        )
    }

    // -- Public API --

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gfx.resize(new_size);
        self.ui_scale = Self::window_ui_scale(&self.window);
        let (ui_width, ui_height) = Self::logical_ui_size(new_size, self.ui_scale);
        self.ui.resize(ui_width, ui_height);
    }

    pub fn window_to_ui_position(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.ui_scale, y / self.ui_scale)
    }

    pub fn begin_timeline_pan(&mut self, x: f32) {
        self.ui.rythmo_state.panning = true;
        self.ui.rythmo_state.pan_last_x = x;
        self.ui.rythmo_state.pan_accum = 0.0;
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) -> EventResponse {
        self.ui
            .handle_event(event, &self.project, self.render_frame(), self.fps())
    }

    pub fn set_export_progress(&mut self, p: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>) {
        let is_none = p.is_none();
        self.ui.export_progress = p;
        if is_none {
            self.ui.export_render_backend = None;
            self.ui.progress_prefix = String::new();
            self.active_export_cancel = None;
        }
    }

    pub fn set_export_cancel(&mut self, cancel: Option<Arc<AtomicBool>>) {
        self.active_export_cancel = cancel;
    }

    pub fn cancel_export(&mut self) {
        if let Some(cancel) = &self.active_export_cancel {
            cancel.store(true, Ordering::Relaxed);
            self.ui.progress_prefix = "Annulation...".to_string();
        }
    }

    pub fn set_export_render_backend(
        &mut self,
        status: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    ) {
        self.ui.export_render_backend = status;
    }

    pub fn set_progress_label(&mut self, label: &str) {
        self.ui.progress_prefix = label.to_string();
    }

    pub fn set_ctrl_held(&mut self, held: bool) {
        self.ui.rythmo_state.ctrl_held = held;
        if !held {
            self.ui.rythmo_state.ghost_preview = None;
        }
    }

    pub fn is_editing_text(&self) -> bool {
        self.ui.is_editing_text()
    }

    pub fn hovered_line(&self) -> Option<u64> {
        self.ui.rythmo_state.hovered_line
    }

    pub fn editing_line(&self) -> Option<u64> {
        self.ui.rythmo_state.editing_line
    }

    pub fn open_server_browser(&mut self) {
        self.ui.open_server_browser();
        self.ping_servers();
    }

    pub fn open_connect_modal(&mut self, ip: &str, port: u16, join: bool) {
        self.ui.open_connect_modal(ip, port, join);
    }

    pub fn open_add_server_modal(&mut self) {
        self.ui.open_add_server_modal();
    }

    pub fn refresh_server_browser(&mut self) {
        // Re-open browser with fresh server list
        self.ui.open_server_browser();
        self.ping_servers();
    }

    fn ping_servers(&mut self) {
        if let Some(browser) = self.ui.server_browser_mut() {
            for s in &mut browser.servers {
                s.status = crate::ui::server_browser::ServerStatus::Pinging;
            }
        }
        let servers = crate::config::saved_servers();
        for s in servers {
            let ip = s.ip.clone();
            let port = s.port;
            let ping_results = self.ping_results.clone();
            std::thread::spawn(move || {
                ping_server_socketio(&ip, port, ping_results);
            });
        }
    }

    pub fn open_settings_modal(&mut self) {
        let fonts = self.ui_renderer.enumerate_font_families();
        self.ui.open_settings_modal(fonts);
    }

    pub fn open_project_settings_modal(&mut self) {
        self.ui
            .open_project_settings_modal(self.project.settings.instrumental_audio_path.clone());
    }

    pub fn set_project_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.ui.set_project_instrumental_audio_path(path);
    }

    pub fn close_project_settings_modal(&mut self) {
        self.ui.close_project_settings_modal();
    }

    pub fn save_project_settings(&mut self, instrumental_audio_path: Option<String>) {
        let mut settings = self.project.settings.clone();
        settings.instrumental_audio_path = instrumental_audio_path;
        self.project.set_settings(settings);
        self.dirty = true;
        self.sync_audio_settings_to_player();
    }

    pub fn show_toast(&mut self, message: impl Into<String>, duration_secs: f32) {
        self.ui.toasts.push(message, duration_secs);
    }

    pub fn show_proxy_error(&mut self, detail: impl Into<String>) {
        self.ui.open_proxy_error_modal(detail);
    }

    pub fn open_whats_new_modal(&mut self, version: impl Into<String>, body: impl Into<String>) {
        self.ui.open_whats_new_modal(version, body);
    }

    pub fn open_pricing_page(&mut self) {
        self.ui.open_pricing_page();
    }

    pub fn close_pricing_page(&mut self) {
        self.ui.close_pricing_page();
    }

    pub fn open_save_prompt(&mut self) {
        self.ui.open_save_prompt();
    }

    pub fn toggle_karaoke_for_selection(&mut self) {
        let line_id = match self.ui.rythmo_state.selected {
            Some(crate::ui::rythmo::Selection::Line(id)) => Some(id),
            _ => self.ui.rythmo_state.hovered_line,
        };
        let Some(line_id) = line_id else {
            self.show_toast(crate::i18n::t("toast.karaoke_select_line"), 3.0);
            return;
        };

        let Some(line) = self.project.get_line(line_id) else {
            return;
        };
        let old_karaoke = line.karaoke;
        let old_ratios = line.syllable_ratios.clone();
        let new_karaoke = !old_karaoke;
        let new_ratios = if new_karaoke {
            crate::syllable::timing_ratios(
                &line.text,
                &line.syllable_ratios,
                &crate::config::get().lang,
            )
        } else {
            old_ratios.clone()
        };

        if let Some(line) = self.project.get_line_mut(line_id) {
            line.karaoke = new_karaoke;
            line.syllable_ratios = new_ratios.clone();
        }
        self.ui.rythmo_state.selected = Some(crate::ui::rythmo::Selection::Line(line_id));
        self.push_and_broadcast(Command::SetLineKaraoke {
            line_id,
            old_karaoke,
            old_ratios,
            new_karaoke,
            new_ratios,
        });
    }

    pub fn open_export_modal(&mut self) {
        let (video_width, video_height) = self.source_video_size().unwrap_or((1920, 1080));
        self.ui.open_export_modal(
            video_width,
            video_height,
            self.project
                .settings
                .instrumental_audio_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty()),
        );
    }

    pub fn open_file_explorer(
        &mut self,
        request: crate::ui::file_explorer_modal::FileExplorerRequest,
    ) {
        self.ui.open_file_explorer(request);
    }

    pub fn poll_file_explorer(&mut self) -> bool {
        self.ui.poll_file_explorer()
    }

    pub fn open_voice_actor_modal(&mut self) {
        self.ui.open_voice_actor_modal();
    }

    pub fn open_proxy_modal(&mut self) {
        let (video_width, video_height) = self.source_video_size().unwrap_or((1920, 1080));
        self.ui.open_proxy_modal(video_width, video_height);
    }

    pub fn close_settings_modal(&mut self) {
        self.ui.close_settings_modal();
        self.ui_renderer.clear_text_cache();
    }

    pub fn rebuild_topbar_for_network(&mut self) {
        let room_code = self.network.room_code.clone();
        self.ui.set_network_room_code(room_code.as_deref());
        self.ui.rebuild_topbar(self.network.is_in_room());
    }

    pub fn rebuild_topbar(&mut self) {
        self.ui.rebuild_topbar(self.network.is_in_room());
    }

    pub fn begin_network_connect(&mut self) {
        self.set_network_status("Connexion...");
        self.ui.set_network_room_code(None);
    }

    pub fn disconnect_network(&mut self) {
        self.network.disconnect();
        self.set_network_status("");
        self.rebuild_topbar_for_network();
    }

    pub fn set_network_status(&mut self, status: impl Into<String>) {
        self.ui.network_status = status.into();
        let display = if self.ui.network_status.is_empty() {
            "Déconnecté"
        } else {
            &self.ui.network_status
        };
        self.window
            .set_title(&format!("Coquerythmo v{} - {}", crate::update::current_version(), display));
    }

    pub fn update_window_title(&self) {
        let display = if self.ui.network_status.is_empty() {
            "Déconnecté"
        } else {
            &self.ui.network_status
        };
        self.window
            .set_title(&format!("Coquerythmo v{} - {}", crate::update::current_version(), display));
    }

    pub fn request_redraw(&self) {
        self.gfx.request_redraw();
    }

    pub fn has_secondary_display(&self) -> bool {
        self.secondary_display.is_some()
    }

    pub fn secondary_window_id(&self) -> Option<WindowId> {
        self.secondary_display
            .as_ref()
            .map(|display| display.window.id())
    }

    pub fn is_video_playing(&self) -> bool {
        self.video_player
            .as_ref()
            .is_some_and(|player| player.is_playing())
    }

    pub fn is_secondary_window(&self, window_id: WindowId) -> bool {
        self.secondary_display
            .as_ref()
            .is_some_and(|display| display.window.id() == window_id)
    }

    pub fn open_secondary_display(&mut self, window: Arc<Window>) {
        if self.video_player.is_none() {
            log::warn!("No video loaded — cannot open secondary display");
            return;
        }

        if let Some(display) = &self.secondary_display {
            display.window.request_redraw();
            return;
        }

        match self.gfx.create_window_surface(window) {
            Ok(display) => {
                self.secondary_display = Some(display);
                self.request_redraw();
                self.request_secondary_redraw();
            }
            Err(e) => log::error!("Failed to open secondary display: {e}"),
        }
    }

    pub fn close_secondary_display(&mut self) {
        self.secondary_display = None;
        self.request_redraw();
    }

    pub fn resize_secondary_display(
        &mut self,
        window_id: WindowId,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) {
        if let Some(display) = &mut self.secondary_display {
            if display.window.id() == window_id {
                display.resize(&self.gfx.device, new_size);
            }
        }
    }

    pub fn request_secondary_redraw(&self) {
        if let Some(display) = &self.secondary_display {
            display.request_redraw();
        }
    }

    // -- Video --

    pub fn current_frame(&self) -> i64 {
        self.video_player.as_ref().map_or(0, |p| p.current_frame())
    }

    pub fn render_frame(&self) -> f64 {
        self.video_player
            .as_ref()
            .map_or(self.current_frame() as f64, |p| {
                p.current_frame_for_render()
            })
    }

    pub fn fps(&self) -> f64 {
        self.video_player.as_ref().map_or(30.0, |p| p.fps())
    }

    pub fn total_frames(&self) -> i64 {
        self.video_player.as_ref().map_or(0, |p| p.total_frames())
    }

    pub fn source_video_size(&self) -> Option<(u32, u32)> {
        self.source_video_size.or_else(|| {
            self.video_player
                .as_ref()
                .and_then(|player| player.video_size())
        })
    }

    pub fn video_path(&self) -> Option<PathBuf> {
        self.source_video_path
            .clone()
            .or_else(|| self.video_player.as_ref().and_then(|p| p.path()))
    }

    pub fn load_video(&mut self, path: &Path) {
        let proxy_path = self
            .project_path
            .as_ref()
            .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, path));
        self.load_video_for_playback(path, proxy_path.as_deref(), None);
        self.sync_audio_settings_to_player();
    }

    pub fn reload_linked_proxy(&mut self) {
        if let Some(br_path) = &self.project_path {
            if let Some(link) = crate::video_proxy::proxy_link_for_br(br_path) {
                let source_matches = self.video_path().as_ref().is_some_and(|path| {
                    crate::video_proxy::paths_match(path, &link.source_video_path)
                });
                let proxy_matches = self.proxy_video_path.as_ref().is_some_and(|path| {
                    crate::video_proxy::paths_match(path, &link.proxy_video_path)
                });

                if source_matches && proxy_matches {
                    return;
                }

                let frame = if source_matches {
                    self.current_frame()
                } else {
                    0
                };
                self.load_video_for_playback(
                    &link.source_video_path,
                    Some(&link.proxy_video_path),
                    Some(frame),
                );
                return;
            }
        }

        let Some(source_path) = self.video_path() else {
            return;
        };
        let proxy_path = self
            .project_path
            .as_ref()
            .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, &source_path));

        if proxy_path == self.proxy_video_path {
            return;
        }

        let frame = self.current_frame();
        self.load_video_for_playback(&source_path, proxy_path.as_deref(), Some(frame));
    }

    pub fn watch_proxy_job(
        &mut self,
        source_path: PathBuf,
        receiver: Receiver<Result<PathBuf, String>>,
    ) {
        self.pending_proxy_job = Some(PendingProxyJob {
            source_path,
            receiver,
        });
    }

    pub fn watch_export_job(&mut self, receiver: Receiver<Result<(), String>>) {
        self.pending_export_job = Some(PendingExportJob { receiver });
    }

    fn load_video_for_playback(
        &mut self,
        source_path: &Path,
        proxy_path: Option<&Path>,
        seek_frame: Option<i64>,
    ) {
        let (bgl, sampler) = self.renderer_refs();
        let mut player = VideoPlayer::new();

        let mut active_proxy_path = proxy_path.map(Path::to_path_buf);
        let mut load_path = active_proxy_path.as_deref().unwrap_or(source_path);
        let mut load_result = player.load_with_audio(
            load_path,
            source_path,
            &self.gfx.device,
            &self.gfx.queue,
            bgl,
            sampler,
        );

        if let Err(e) = &load_result {
            if active_proxy_path.is_some() {
                log::warn!(
                    "Failed to load proxy {}, falling back to original video: {e}",
                    load_path.display()
                );
                active_proxy_path = None;
                load_path = source_path;
                player = VideoPlayer::new();
                load_result = player.load_with_audio(
                    load_path,
                    source_path,
                    &self.gfx.device,
                    &self.gfx.queue,
                    bgl,
                    sampler,
                );
            }
        }

        match load_result {
            Ok(()) => {}
            Err(e) => {
                log::error!("Failed to load video: {e}");
                let detail = e.lines().next().unwrap_or(&e);
                self.show_toast(
                    format!("{} {detail}", crate::i18n::t("toast.video_load_failed")),
                    6.0,
                );
                return;
            }
        }

        player.set_volume(self.ui.volume());
        if let Some(frame) = seek_frame {
            let total = player.total_frames();
            let target = if total > 0 {
                frame.clamp(0, total - 1)
            } else {
                frame.max(0)
            };
            player.seek_frame_instant(target as i32);
            player.decode_current_frame(&self.gfx.device, &self.gfx.queue, bgl, sampler);
        }

        if self.ui.is_playing() {
            self.ui.toggle_play_pause();
        }

        let fps = player.fps();
        let total = player.total_frames();
        let current_frame = player.current_frame();
        let source_size = crate::video_proxy::probe_video(source_path)
            .ok()
            .map(|info| (info.width, info.height))
            .or_else(|| player.video_size());

        self.source_video_path = Some(source_path.to_path_buf());
        self.source_video_size = source_size;
        self.proxy_video_path = active_proxy_path;
        self.video_player = Some(player);
        self.sync_audio_settings_to_player();
        self.timeline.emit(TimelineEvent::VideoLoaded {
            fps,
            total_frames: total,
        });
        self.timeline.emit(TimelineEvent::FrameChanged {
            frame: current_frame,
        });
        self.ui.has_video = true;
        self.ui.total_frames = total;
        self.rebuild_topbar_for_network();
    }

    pub fn toggle_play_pause(&mut self) {
        if let Some(player) = &mut self.video_player {
            if !player.toggle() {
                return;
            }
            if self.ui.is_playing() != player.is_playing() {
                self.ui.toggle_play_pause();
            }
            if player.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStarted);
            } else {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
            }
        }
    }

    pub fn toggle_active_audio(&mut self) {
        let Some(player) = &mut self.video_player else {
            return;
        };
        if player.toggle_audio_track() {
            let label = match player.active_audio_track() {
                AudioTrack::Source => "Audio original",
                AudioTrack::Instrumental => "Audio instrumental",
            };
            self.show_toast(label, 1.5);
        } else {
            self.show_toast("Aucune version instrumentale", 2.5);
        }
    }

    pub fn active_audio_offset_frames(&self) -> i64 {
        self.video_player
            .as_ref()
            .map(|player| player.active_audio_offset_frames())
            .unwrap_or(0)
    }

    pub fn active_audio_is_instrumental(&self) -> bool {
        self.video_player
            .as_ref()
            .is_some_and(|player| player.active_audio_track() == AudioTrack::Instrumental)
    }

    pub fn offset_active_audio_by(&mut self, delta_frames: i64) {
        if delta_frames == 0 {
            return;
        }
        let Some(player) = &mut self.video_player else {
            return;
        };
        match player.active_audio_track() {
            AudioTrack::Source => self.project.adjust_source_audio_offset(delta_frames),
            AudioTrack::Instrumental => self.project.adjust_instrumental_audio_offset(delta_frames),
        }
        player.adjust_active_audio_offset(delta_frames);
        self.dirty = true;
    }

    pub fn sync_audio_settings_to_player(&mut self) {
        let Some(player) = &mut self.video_player else {
            return;
        };
        let settings = &self.project.settings;
        player.set_instrumental_audio_path(
            settings
                .instrumental_audio_path
                .as_ref()
                .map(std::path::PathBuf::from),
        );
        player.set_audio_offsets(
            settings.source_audio_offset_frames,
            settings.instrumental_audio_offset_frames,
        );
    }

    pub fn set_volume(&mut self, vol: f32) {
        if vol > 0.001 {
            self.last_nonzero_volume = vol;
        }
        self.ui.set_volume(vol);
        if let Some(player) = &mut self.video_player {
            player.set_volume(vol);
        }
    }

    pub fn toggle_mute(&mut self) {
        let target = if self.ui.volume() > 0.001 {
            0.0
        } else {
            self.last_nonzero_volume.max(0.75)
        };
        self.set_volume(target);
    }

    pub fn prev_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_backward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() {
                self.ui.toggle_play_pause();
            }
        }
    }

    pub fn next_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_forward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() {
                self.ui.toggle_play_pause();
            }
        }
    }

    pub fn seek_absolute(&mut self, frame: i64) {
        if let Some(player) = &mut self.video_player {
            let current = player.current_frame();
            let delta = (frame - current) as i32;
            player.seek_frame_instant(delta);
            self.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
        }
        self.last_scroll_time = Some(Instant::now());
        self.scroll_needs_decode = true;
    }

    pub fn seek_relative(&mut self, delta: i32) {
        if let Some(player) = &mut self.video_player {
            player.seek_frame_instant(delta);
            self.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
        }
        self.last_scroll_time = Some(Instant::now());
        self.scroll_needs_decode = true;
    }

    pub fn seek_to_next_boucle(&mut self, direction: i32) {
        let current = self.current_frame();
        let boucle_frames: Vec<i64> = self
            .project
            .markers
            .iter()
            .filter(|m| m.kind == crate::rythmo_line::MarkerKind::Boucle)
            .map(|m| m.frame)
            .collect();
        if boucle_frames.is_empty() {
            return;
        }

        let target = if direction > 0 {
            // Forward: find first boucle strictly after current frame
            boucle_frames.iter().find(|&&f| f > current).copied()
        } else {
            // Backward: find last boucle strictly before current frame
            boucle_frames.iter().rev().find(|&&f| f < current).copied()
        };

        if let Some(frame) = target {
            self.seek_absolute(frame);
        }
    }

    fn tick_scroll_decode(&mut self) -> bool {
        if !self.scroll_needs_decode {
            return false;
        }
        if let Some(t) = self.last_scroll_time {
            if t.elapsed().as_millis() >= constants::SCROLL_DECODE_DELAY_MS {
                self.scroll_needs_decode = false;
                let bgl = self.ui_renderer.texture_bind_group_layout();
                let sampler = self.ui_renderer.texture_sampler();
                if let Some(player) = &mut self.video_player {
                    player.decode_current_frame(&self.gfx.device, &self.gfx.queue, bgl, sampler);
                }
                return true;
            }
        }

        false
    }

    // -- Network --

    pub fn tick_network(&mut self) -> bool {
        let prev_state = self.network.state;
        let mut changed = false;
        while let Some(msg) = self.network.try_recv() {
            changed = true;
            match msg {
                IncomingMessage::Connected => {
                    self.network.state = ConnectionState::Connected;
                    self.set_network_status("Connecté au serveur");
                    log::info!("Connected and authenticated");
                }
                IncomingMessage::Packet(packet) => self.handle_network_packet(packet),
                IncomingMessage::Disconnected(reason) => {
                    log::info!("Disconnected: {reason}");
                    self.network.state = ConnectionState::Disconnected;
                    self.network.room_code = None;
                    self.network.role = None;
                    self.network.members.clear();
                    self.set_network_status("");
                    self.ui.set_network_room_code(None);
                }
                IncomingMessage::Error(err) => {
                    log::error!("Network error: {err}");
                    self.set_network_status(format!("Erreur: {err}"));
                }
                IncomingMessage::Delta(data) => {
                    // Ignore network updates in studio mode (read-only playback)
                    if !self.studio_mode {
                        self.apply_delta(data);
                    }
                }
                IncomingMessage::SyncRequested { requester } => {
                    log::info!("Sync requested by {requester}");
                    let data = ProjectData::from_project(&self.project);
                    let mut json = serde_json::json!({ "project": data });
                    if !requester.is_empty() {
                        json["_target"] = serde_json::Value::String(requester);
                    }
                    self.network.send_raw("sync", json);
                }
                // Video transfer messages (unused for now)
                IncomingMessage::VideoStart { .. }
                | IncomingMessage::VideoChunk { .. }
                | IncomingMessage::VideoEnd => {}
            }
        }
        // Rebuild topbar if connection state changed
        if self.network.state != prev_state {
            self.ui.rebuild_topbar(self.network.is_in_room());
            changed = true;
        }

        changed
    }

    fn handle_network_packet(&mut self, packet: Packet) {
        match packet {
            Packet::RoomCreated { code } => {
                self.network.state = ConnectionState::InRoom;
                self.network.room_code = Some(code.clone());
                self.network.role = Some("admin".into());
                self.set_network_status("Salon créé");
                self.ui.set_network_room_code(Some(&code));
                self.ui.toasts.push(
                    format!("{}{code}", crate::i18n::t("toast.room_created")),
                    5.0,
                );
                log::info!("Room created: {code}");
            }
            Packet::RoomJoined {
                code,
                role,
                members,
            } => {
                self.network.state = ConnectionState::InRoom;
                self.network.room_code = Some(code.clone());
                self.network.role = Some(role);
                self.network.members = members;
                self.set_network_status("Connecté au salon");
                self.ui.set_network_room_code(Some(&code));
                self.ui.toasts.push(
                    format!("{}{code}", crate::i18n::t("toast.room_joined")),
                    5.0,
                );
                self.ui.sync_overlay = Some("Synchronisation en cours...".into());
                self.ui.sync_progress = 0.0;
                // request_sync is sent directly from the room_joined callback
            }
            Packet::JoinError { reason } => {
                log::error!("Join failed: {reason}");
                self.set_network_status(format!("Échec: {reason}"));
                self.ui.set_network_room_code(None);
            }
            Packet::MemberJoined { username } => {
                self.network.members.push(username.clone());
                log::info!("Member joined: {username}");
            }
            Packet::MemberLeft { username } => {
                self.network.members.retain(|m| m != &username);
                log::info!("Member left: {username}");
            }
            Packet::RemoteCommand { from, payload } => {
                log::debug!("Remote command from {from}");
                self.apply_remote_command(payload);
            }
            Packet::Sync { project: data } => {
                self.apply_project_sync(data);
                self.ui.sync_overlay = None;
                if self.network.room_code.is_some() {
                    self.set_network_status("Salon synchronisé");
                }
            }
            Packet::RequestSync => {
                // Handled via SyncRequested with requester id
            }
            Packet::Error { message } => {
                log::error!("Server error: {message}");
                self.set_network_status(format!("Erreur: {message}"));
            }
            _ => {} // Client-only packets (Auth, CreateRoom, etc.) ignored here
        }
    }

    fn apply_remote_command(&mut self, payload: CommandPayload) {
        match payload {
            CommandPayload::CreateLine { line } => {
                log::debug!("Remote: create line {}", line.id);
                self.project.insert_line(line);
            }
            CommandPayload::DeleteLine { line_id } => {
                log::debug!("Remote: delete line {}", line_id);
                self.project.remove_line(line_id);
            }
            CommandPayload::SplitLine {
                first_line,
                second_line,
                second_index,
            } => {
                log::debug!("Remote: split line {}", first_line.id);
                self.project
                    .upsert_line_at(second_index.saturating_sub(1), first_line);
                self.project.upsert_line_at(second_index, second_line);
            }
            CommandPayload::MoveLine {
                line_id,
                start_frame,
                y_slot,
            } => {
                log::debug!("Remote: move line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.start_frame = start_frame;
                    l.y_slot = y_slot;
                }
            }
            CommandPayload::MoveLines { lines } => {
                log::debug!("Remote: move {} lines", lines.len());
                for movement in lines {
                    if let Some(l) = self.project.get_line_mut(movement.line_id) {
                        l.start_frame = movement.start_frame;
                        l.y_slot = movement.y_slot;
                    }
                }
            }
            CommandPayload::ResizeLine {
                line_id,
                start_frame,
                duration_frames,
            } => {
                log::debug!("Remote: resize line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.start_frame = start_frame;
                    l.duration_frames = duration_frames;
                }
            }
            CommandPayload::UpdateLineText { line_id, text } => {
                log::debug!("Remote: update text for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.text = text;
                }
            }
            CommandPayload::SetLineKaraoke {
                line_id,
                karaoke,
                syllable_ratios,
            } => {
                log::debug!("Remote: set karaoke for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.karaoke = karaoke;
                    l.syllable_ratios = syllable_ratios;
                }
            }
            CommandPayload::SetSyllableRatios { line_id, ratios } => {
                log::debug!("Remote: set syllable ratios for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.syllable_ratios = ratios;
                }
            }
            CommandPayload::SetCharacter {
                line_id,
                name,
                color,
                voice_actor_names,
            } => {
                log::debug!("Remote: set character for line {}", line_id);
                self.apply_character_change(line_id, name, color, voice_actor_names);
            }
            CommandPayload::SetCharacterColor { line_id, color } => {
                log::debug!("Remote: set character color for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.character_color = color;
                }
            }
            CommandPayload::RenameCharacter {
                changes,
                known_characters,
            } => {
                log::debug!("Remote: rename character on {} lines", changes.len());
                let known_characters = known_characters
                    .into_iter()
                    .map(|c| Character {
                        name: c.name,
                        color: c.color,
                    })
                    .collect();
                self.apply_character_rename_changes(&changes, known_characters);
            }
            CommandPayload::SetVoiceActors { changes } => {
                log::debug!("Remote: set voice actors for {} lines", changes.len());
                self.apply_voice_actor_changes(&changes, true);
            }
            CommandPayload::CreateVoiceActor { actor } => {
                log::debug!("Remote: create voice actor {}", actor.name);
                self.project.upsert_voice_actor(actor);
            }
            CommandPayload::AddMarker { kind, frame } => {
                log::debug!("Remote: add marker at frame {}", frame);
                self.project
                    .add_marker(crate::rythmo_line::RythmoMarker { kind, frame });
            }
            CommandPayload::RemoveMarker { kind, frame } => {
                log::debug!("Remote: remove marker at frame {}", frame);
                self.project
                    .retain_markers(|m| !(m.kind == kind && m.frame == frame));
            }
            CommandPayload::MoveMarker {
                kind,
                old_frame,
                new_frame,
            } => {
                log::debug!(
                    "Remote: move marker from frame {} to {}",
                    old_frame,
                    new_frame
                );
                if let Some(index) = self
                    .project
                    .markers
                    .iter()
                    .position(|m| m.kind == kind && m.frame == old_frame)
                {
                    self.project.move_marker(index, new_frame);
                }
            }
            CommandPayload::LoadVideo { .. } => {
                log::debug!("Remote: load video (ignored, using chunked transfer)");
                // Video transfer now uses chunked video_start/chunk/end events
            }
            CommandPayload::UpdateLineNote { line_id, note } => {
                log::debug!("Remote: update note for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.note = note;
                }
            }
        }
    }

    fn apply_project_sync(&mut self, data: ProjectData) {
        // Merge lines: update existing, add new, remove deleted
        let remote_ids: std::collections::HashSet<u64> = data.lines.iter().map(|l| l.id).collect();

        // Remove lines that no longer exist remotely
        self.project.retain_lines(|l| remote_ids.contains(&l.id));

        // Update existing or add new
        for remote_line in data.lines {
            if let Some(local) = self.project.get_line_mut(remote_line.id) {
                local.start_frame = remote_line.start_frame;
                local.duration_frames = remote_line.duration_frames;
                local.y_slot = remote_line.y_slot;
                local.text = remote_line.text;
                local.character_name = remote_line.character_name;
                local.character_color = remote_line.character_color;
                local.voice_actor_names = remote_line.voice_actor_names;
                local.karaoke = remote_line.karaoke;
                local.syllable_ratios = remote_line.syllable_ratios;
                local.note = remote_line.note;
            } else {
                self.project.insert_line(remote_line);
            }
        }

        // Merge markers (replace — markers don't have stable IDs)
        self.project.set_markers(data.markers);

        // Merge known characters
        self.project.known_characters = data
            .known_characters
            .into_iter()
            .map(|c| crate::project::Character {
                name: c.name,
                color: c.color,
            })
            .collect();
        self.project.voice_actors = data.voice_actors;
        self.project.bump_revision();

        log::info!("Project synced (merged)");
    }

    fn apply_delta(&mut self, data: serde_json::Value) {
        log::debug!(
            "Applying delta: {}",
            data["action"].as_str().unwrap_or("unknown")
        );
        let action = data["action"].as_str().unwrap_or("");
        match action {
            "create_line" => {
                if let Ok(line) =
                    serde_json::from_value::<crate::rythmo_line::RythmoLine>(data["line"].clone())
                {
                    // Don't add if already exists
                    if self.project.get_line(line.id).is_none() {
                        self.project.insert_line(line);
                    }
                }
            }
            "delete_line" => {
                if let Some(id) = data["line_id"].as_u64() {
                    self.project.remove_line(id);
                }
            }
            "split_line" => {
                if let (Ok(first_line), Ok(second_line), Some(second_index)) = (
                    serde_json::from_value::<crate::rythmo_line::RythmoLine>(
                        data["first_line"].clone(),
                    ),
                    serde_json::from_value::<crate::rythmo_line::RythmoLine>(
                        data["second_line"].clone(),
                    ),
                    data["second_index"].as_u64(),
                ) {
                    let second_index = second_index as usize;
                    self.project
                        .upsert_line_at(second_index.saturating_sub(1), first_line);
                    self.project.upsert_line_at(second_index, second_line);
                }
            }
            "move_line" => {
                if let (Some(id), Some(sf), Some(ys)) = (
                    data["line_id"].as_u64(),
                    data["start_frame"].as_i64(),
                    data["y_slot"].as_f64(),
                ) {
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.start_frame = sf;
                        l.y_slot = ys as f32;
                    }
                }
            }
            "move_lines" => {
                if let Some(lines) = data["lines"].as_array() {
                    for movement in lines {
                        if let (Some(id), Some(sf), Some(ys)) = (
                            movement["line_id"].as_u64(),
                            movement["start_frame"].as_i64(),
                            movement["y_slot"].as_f64(),
                        ) {
                            if let Some(l) = self.project.get_line_mut(id) {
                                l.start_frame = sf;
                                l.y_slot = ys as f32;
                            }
                        }
                    }
                }
            }
            "resize_line" => {
                if let (Some(id), Some(sf), Some(df)) = (
                    data["line_id"].as_u64(),
                    data["start_frame"].as_i64(),
                    data["duration_frames"].as_i64(),
                ) {
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.start_frame = sf;
                        l.duration_frames = df;
                    }
                }
            }
            "update_text" => {
                if let (Some(id), Some(text)) = (data["line_id"].as_u64(), data["text"].as_str()) {
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.text = text.to_string();
                    }
                }
            }
            "update_note" => {
                if let (Some(id), Some(note)) = (data["line_id"].as_u64(), data["note"].as_str()) {
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.note = note.to_string();
                    }
                }
            }
            "set_line_karaoke" => {
                if let (Some(id), Some(karaoke)) =
                    (data["line_id"].as_u64(), data["karaoke"].as_bool())
                {
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.karaoke = karaoke;
                        if let Ok(ratios) =
                            serde_json::from_value::<Vec<f32>>(data["syllable_ratios"].clone())
                        {
                            l.syllable_ratios = ratios;
                        }
                    }
                }
            }
            "set_syllable_ratios" => {
                if let Some(id) = data["line_id"].as_u64() {
                    if let Ok(ratios) = serde_json::from_value::<Vec<f32>>(data["ratios"].clone()) {
                        if let Some(l) = self.project.get_line_mut(id) {
                            l.syllable_ratios = ratios;
                        }
                    }
                }
            }
            "set_character" => {
                if let (Some(id), Some(name)) = (data["line_id"].as_u64(), data["name"].as_str()) {
                    let color = parse_color(&data["color"]);
                    let voice_actor_names =
                        serde_json::from_value::<Vec<String>>(data["voice_actor_names"].clone())
                            .ok();
                    self.apply_character_change(id, name.to_string(), color, voice_actor_names);
                }
            }
            "set_character_color" => {
                if let Some(id) = data["line_id"].as_u64() {
                    let color = parse_color(&data["color"]);
                    if let Some(l) = self.project.get_line_mut(id) {
                        l.character_color = color;
                    }
                }
            }
            "rename_character" => {
                if let (Ok(changes), Ok(known_characters)) = (
                    serde_json::from_value::<Vec<LineCharacterNameChange>>(data["changes"].clone()),
                    serde_json::from_value::<Vec<Character>>(data["known_characters"].clone()),
                ) {
                    self.apply_character_rename_changes(&changes, known_characters);
                }
            }
            "set_voice_actors" => {
                if let Ok(changes) =
                    serde_json::from_value::<Vec<LineVoiceActorsChange>>(data["changes"].clone())
                {
                    self.apply_voice_actor_changes(&changes, true);
                }
            }
            "create_voice_actor" => {
                if let Ok(actor) = serde_json::from_value::<VoiceActor>(data["actor"].clone()) {
                    self.project.upsert_voice_actor(actor);
                }
            }
            "add_marker" => {
                if let (Some(frame), Ok(kind)) = (
                    data["frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    self.project
                        .add_marker(crate::rythmo_line::RythmoMarker { kind, frame });
                }
            }
            "remove_marker" => {
                if let (Some(frame), Ok(kind)) = (
                    data["frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    self.project
                        .retain_markers(|m| !(m.kind == kind && m.frame == frame));
                }
            }
            "move_marker" => {
                if let (Some(old_frame), Some(new_frame), Ok(kind)) = (
                    data["old_frame"].as_i64(),
                    data["new_frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    if let Some(index) = self
                        .project
                        .markers
                        .iter()
                        .position(|m| m.kind == kind && m.frame == old_frame)
                    {
                        self.project.move_marker(index, new_frame);
                    }
                }
            }
            _ => log::warn!("Unknown delta action: {action}"),
        }
    }

    /// Push a command to history and broadcast the delta.
    fn push_and_broadcast(&mut self, cmd: Command) {
        self.broadcast_delta(&cmd);
        self.history.push(cmd);
        self.dirty = true;
    }

    /// Broadcast a single command as a delta via the "delta" event.
    fn broadcast_delta(&self, cmd: &Command) {
        if !self.network.is_in_room() {
            return;
        }
        let payload = match cmd {
            Command::CreateLine { line_id } => {
                let line = self.project.get_line(*line_id).cloned();
                if let Some(l) = line {
                    serde_json::json!({ "action": "create_line", "line": serde_json::to_value(&l).unwrap_or_default() })
                } else {
                    return;
                }
            }
            Command::InsertLine { snapshot, .. } => {
                serde_json::json!({ "action": "create_line", "line": serde_json::to_value(snapshot).unwrap_or_default() })
            }
            Command::DeleteLine { snapshot, .. } => {
                serde_json::json!({ "action": "delete_line", "line_id": snapshot.id })
            }
            Command::SplitLine {
                first_line,
                second_line,
                second_index,
                ..
            } => {
                serde_json::json!({
                    "action": "split_line",
                    "first_line": serde_json::to_value(first_line).unwrap_or_default(),
                    "second_line": serde_json::to_value(second_line).unwrap_or_default(),
                    "second_index": second_index,
                })
            }
            Command::MoveLine {
                line_id,
                new_start,
                new_y_slot,
                ..
            } => {
                serde_json::json!({ "action": "move_line", "line_id": line_id, "start_frame": new_start, "y_slot": new_y_slot })
            }
            Command::MoveLines { moves } => {
                let lines: Vec<_> = moves
                    .iter()
                    .map(|movement| {
                        serde_json::json!({
                            "line_id": movement.line_id,
                            "start_frame": movement.new_start,
                            "y_slot": movement.new_y_slot,
                        })
                    })
                    .collect();
                serde_json::json!({ "action": "move_lines", "lines": lines })
            }
            Command::ResizeLine {
                line_id,
                new_start,
                new_dur,
                ..
            } => {
                serde_json::json!({ "action": "resize_line", "line_id": line_id, "start_frame": new_start, "duration_frames": new_dur })
            }
            Command::UpdateLineText {
                line_id, new_text, ..
            } => {
                serde_json::json!({ "action": "update_text", "line_id": line_id, "text": new_text })
            }
            Command::UpdateLineNote {
                line_id, new_note, ..
            } => {
                serde_json::json!({ "action": "update_note", "line_id": line_id, "note": new_note })
            }
            Command::SetLineKaraoke {
                line_id,
                new_karaoke,
                new_ratios,
                ..
            } => {
                serde_json::json!({ "action": "set_line_karaoke", "line_id": line_id, "karaoke": new_karaoke, "syllable_ratios": new_ratios })
            }
            Command::SetSyllableRatios {
                line_id,
                new_ratios,
                ..
            } => {
                serde_json::json!({ "action": "set_syllable_ratios", "line_id": line_id, "ratios": new_ratios })
            }
            Command::SetCharacter {
                line_id,
                new_name,
                new_color,
                new_voice_actor_names,
                ..
            } => {
                serde_json::json!({ "action": "set_character", "line_id": line_id, "name": new_name, "color": new_color, "voice_actor_names": new_voice_actor_names })
            }
            Command::SetCharacterColor {
                line_id, new_color, ..
            } => {
                serde_json::json!({ "action": "set_character_color", "line_id": line_id, "color": new_color })
            }
            Command::RenameCharacter {
                changes,
                new_known_characters,
                ..
            } => {
                serde_json::json!({ "action": "rename_character", "changes": changes, "known_characters": new_known_characters })
            }
            Command::SetVoiceActors { changes } => {
                serde_json::json!({ "action": "set_voice_actors", "changes": changes })
            }
            Command::CreateVoiceActor { actor } => {
                serde_json::json!({ "action": "create_voice_actor", "actor": actor })
            }
            Command::AddMarker { index } => {
                if let Some(m) = self.project.markers.get(*index) {
                    serde_json::json!({ "action": "add_marker", "kind": serde_json::to_value(&m.kind).unwrap_or_default(), "frame": m.frame })
                } else {
                    return;
                }
            }
            Command::RemoveMarker { marker, .. } => {
                serde_json::json!({ "action": "remove_marker", "kind": serde_json::to_value(&marker.kind).unwrap_or_default(), "frame": marker.frame })
            }
            Command::MoveMarker {
                index,
                old_frame,
                new_frame,
            } => {
                if let Some(m) = self.project.markers.get(*index) {
                    serde_json::json!({ "action": "move_marker", "kind": serde_json::to_value(&m.kind).unwrap_or_default(), "old_frame": old_frame, "new_frame": new_frame })
                } else {
                    return;
                }
            }
        };
        self.network.send_raw("delta", payload);
    }

    /// Broadcast coalesced final state on mouse release / StopEditing.
    pub fn broadcast_finalize(&self) {
        if !self.network.is_in_room() {
            return;
        }
        if let Some(cmd) = self.history.last() {
            if matches!(
                cmd,
                Command::MoveLine { .. }
                    | Command::ResizeLine { .. }
                    | Command::MoveLines { .. }
                    | Command::UpdateLineText { .. }
                    | Command::SetCharacter { .. }
                    | Command::SetCharacterColor { .. }
                    | Command::SetLineKaraoke { .. }
                    | Command::SetSyllableRatios { .. }
                    | Command::SetVoiceActors { .. }
                    | Command::MoveMarker { .. }
            ) {
                self.broadcast_delta(cmd);
            }
        }
    }

    /// Broadcast full project state (only for undo/redo/join sync).
    fn broadcast_full_sync(&self) {
        if !self.network.is_in_room() {
            return;
        }
        let data = ProjectData::from_project(&self.project);
        self.network
            .send_raw("sync", serde_json::json!({ "project": data }));
    }

    // -- Undo / Redo --

    pub fn undo(&mut self) {
        self.history.undo(&mut self.project);
        self.broadcast_full_sync();
    }

    pub fn redo(&mut self) {
        self.history.redo(&mut self.project);
        self.broadcast_full_sync();
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn enter_studio_mode(&mut self) {
        self.studio_mode = true;
        self.ui.rythmo_state.editing_line = None;
        self.ui.rythmo_state.editing_character = None;
        self.ui.rythmo_state.selected = None;
        self.ui.rythmo_state.dragging = None;
        self.ui.rythmo_state.ghost_preview = None;
        self.ui.rythmo_state.context_menu = None;

        // Save current fullscreen state and enter fullscreen
        self.fullscreen_before_studio = self.window.fullscreen();
        self.window
            .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
    }

    pub fn exit_studio_mode(&mut self) {
        self.studio_mode = false;

        // Restore fullscreen state: if we were windowed before, fullscreen_before_studio is None
        // so set_fullscreen(None) should exit fullscreen
        self.fullscreen_before_studio = None;
        self.window.set_fullscreen(None);
    }

    pub fn is_studio_mode(&self) -> bool {
        self.studio_mode
    }

    pub fn show_studio_warning(&self) -> bool {
        self.show_studio_warning
    }

    pub fn request_studio_mode(&mut self) {
        self.show_studio_warning = true;
    }

    pub fn confirm_studio_mode(&mut self) {
        self.show_studio_warning = false;
        self.enter_studio_mode();
    }

    pub fn cancel_studio_mode(&mut self) {
        self.show_studio_warning = false;
    }

    pub fn open_studio_warning(&mut self) {
        self.ui.open_studio_warning();
    }

    // -- Project / Lines (all via Command pattern) --

    pub fn open_toolbar_dropdown(&mut self, dropdown: crate::ui::widget::ToolbarDropdown) {
        self.ui.toggle_toolbar_dropdown(dropdown);
    }

    pub fn open_rename_character_modal(&mut self) {
        let mut characters = self.project.character_names_from_lines();
        characters.sort_by_key(|name| name.to_lowercase());
        if characters.is_empty() {
            self.show_toast(crate::i18n::t("toast.no_character_to_rename"), 4.0);
            return;
        }
        self.ui.open_rename_character_modal(characters);
    }

    pub fn rename_character_everywhere(&mut self, old_name: String, new_name: String) {
        if old_name.trim().is_empty() {
            self.show_toast(crate::i18n::t("toast.rename_character_select"), 4.0);
            return;
        }

        let new_name = new_name.trim().to_string();
        if new_name.is_empty() {
            self.show_toast(crate::i18n::t("toast.rename_character_name_required"), 4.0);
            return;
        }
        if old_name == new_name {
            return;
        }

        let changes: Vec<_> = self
            .project
            .lines()
            .filter(|line| line.character_name == old_name)
            .map(|line| LineCharacterNameChange {
                line_id: line.id,
                old_name: old_name.clone(),
                new_name: new_name.clone(),
            })
            .collect();
        if changes.is_empty() {
            self.show_toast(crate::i18n::t("toast.no_character_to_rename"), 4.0);
            return;
        }

        let old_known_characters = self.project.known_characters.clone();
        let new_known_characters = self.known_characters_after_rename(&old_name, &new_name);
        self.project.apply_character_name_changes(&changes, true);
        self.project
            .set_known_characters(new_known_characters.clone());
        self.push_and_broadcast(Command::RenameCharacter {
            changes,
            old_known_characters,
            new_known_characters,
        });
        self.show_toast(crate::i18n::t("toast.character_renamed"), 3.0);
    }

    fn known_characters_after_rename(&self, old_name: &str, new_name: &str) -> Vec<Character> {
        let mut known_characters = self.project.known_characters.clone();
        let old_index = known_characters
            .iter()
            .position(|character| character.name == old_name);
        let new_index = known_characters
            .iter()
            .position(|character| character.name == new_name);

        if new_index.is_some() {
            if let Some(old_index) = old_index {
                known_characters.remove(old_index);
            }
            return known_characters;
        }

        if let Some(old_index) = old_index {
            known_characters[old_index].name = new_name.to_string();
            return known_characters;
        }

        if let Some(color) = self
            .project
            .lines()
            .find(|line| line.character_name == old_name)
            .map(|line| line.character_color)
        {
            known_characters.push(Character {
                name: new_name.to_string(),
                color,
            });
        }

        known_characters
    }

    fn apply_character_rename_changes(
        &mut self,
        changes: &[LineCharacterNameChange],
        known_characters: Vec<Character>,
    ) {
        self.project.apply_character_name_changes(changes, true);
        self.project.set_known_characters(known_characters);
    }

    pub fn delete_selected(&mut self) {
        use crate::ui::rythmo::Selection;
        if let Some(sel) = self.ui.rythmo_state().selected {
            match sel {
                Selection::Line(id) => {
                    if let Some((snapshot, index)) = self.project.remove_line(id) {
                        self.push_and_broadcast(Command::DeleteLine { snapshot, index });
                    }
                }
                Selection::Marker(idx) => {
                    if let Some(marker) = self.project.remove_marker_at(idx) {
                        self.push_and_broadcast(Command::RemoveMarker { marker, index: idx });
                    }
                }
                Selection::AllLines => {}
            }
            self.ui.clear_selection();
        }
    }

    pub fn copy_selected_line(&mut self) {
        use crate::ui::rythmo::Selection;
        if let Some(Selection::Line(id)) = self.ui.rythmo_state().selected {
            self.line_clipboard = self.project.get_line(id).cloned();
        }
    }

    pub fn cut_selected_line(&mut self) {
        self.copy_selected_line();
        self.delete_selected();
    }

    pub fn paste_line(&mut self) {
        let Some(snapshot) = self.line_clipboard.clone() else {
            return;
        };
        let start_frame = self.current_frame();
        let (line, index) = self.project.duplicate_line_from(&snapshot, start_frame);
        let id = line.id;
        self.ui.rythmo_state.selected = Some(crate::ui::rythmo::Selection::Line(id));
        self.push_and_broadcast(Command::InsertLine {
            snapshot: line,
            index,
        });
    }

    pub fn split_dialogue(&mut self) -> bool {
        let Some(target) = self.dialogue_split_target() else {
            self.show_toast("Sélectionne un dialogue et place le curseur dedans.", 3.0);
            return false;
        };
        let line_id = match &target {
            DialogueSplitTarget::Cursor { line_id, .. }
            | DialogueSplitTarget::Playhead { line_id, .. } => *line_id,
        };
        let Some(old_line) = self.project.get_line(line_id).cloned() else {
            return false;
        };
        if old_line.duration_frames <= 1 {
            self.show_toast("Dialogue trop court pour être coupé.", 3.0);
            return false;
        }

        let lang = crate::config::get().lang.clone();
        let split = match target {
            DialogueSplitTarget::Cursor { cursor_pos, .. } => {
                crate::syllable::split_dialogue_at_syllable_cursor(
                    &old_line.text,
                    &old_line.syllable_ratios,
                    &lang,
                    cursor_pos,
                )
            }
            DialogueSplitTarget::Playhead { progress, .. } => {
                crate::syllable::split_dialogue_at_syllable_progress(
                    &old_line.text,
                    &old_line.syllable_ratios,
                    &lang,
                    progress,
                )
            }
        };
        let Some(split) = split else {
            self.show_toast("Aucune coupure syllabique disponible.", 3.0);
            return false;
        };

        let first_duration =
            ((old_line.duration_frames as f32) * split.split_progress).round() as i64;
        let first_duration = first_duration.clamp(1, old_line.duration_frames - 1);
        let second_duration = old_line.duration_frames - first_duration;
        let old_index = self
            .project
            .line_index(line_id)
            .unwrap_or_else(|| self.project.line_count());
        let second_index = old_index + 1;

        let mut first_line = old_line.clone();
        first_line.duration_frames = first_duration;
        first_line.text = split.first_text;
        first_line.syllable_ratios = split.first_ratios;

        let mut second_line = old_line.clone();
        second_line.id = self.project.generate_line_id();
        second_line.start_frame = old_line.start_frame + first_duration;
        second_line.duration_frames = second_duration;
        second_line.text = split.second_text;
        second_line.syllable_ratios = split.second_ratios;

        if let Some(line) = self.project.get_line_mut(line_id) {
            *line = first_line.clone();
        } else {
            return false;
        }
        self.project
            .insert_line_at(second_index, second_line.clone());
        self.ui.rythmo_state.stop_line_editing();
        self.ui.rythmo_state.stop_char_editing();
        self.ui.rythmo_state.stop_note_editing();
        self.ui.rythmo_state.dragging = None;
        self.ui.rythmo_state.syllable_drag = None;
        self.ui.rythmo_state.context_menu = None;
        self.ui.rythmo_state.selected = Some(crate::ui::rythmo::Selection::Line(second_line.id));

        self.push_and_broadcast(Command::SplitLine {
            old_line,
            old_index,
            first_line,
            second_line,
            second_index,
        });
        true
    }

    fn dialogue_split_target(&self) -> Option<DialogueSplitTarget> {
        if let Some(line_id) = self.ui.rythmo_state.editing_line {
            return Some(DialogueSplitTarget::Cursor {
                line_id,
                cursor_pos: self.ui.rythmo_state.line_input.cursor_pos,
            });
        }
        if self.ui.rythmo_state.editing_character.is_some()
            || self.ui.rythmo_state.editing_note.is_some()
        {
            return None;
        }

        let frame = self.current_frame();
        let line_id = match self.ui.rythmo_state.selected {
            Some(crate::ui::rythmo::Selection::Line(line_id)) => Some(line_id),
            _ => self.ui.rythmo_state.hovered_line.or_else(|| {
                let mut active = self
                    .project
                    .lines()
                    .filter(|line| frame > line.start_frame && frame < line.end_frame())
                    .map(|line| line.id);
                let first = active.next()?;
                if active.next().is_none() {
                    Some(first)
                } else {
                    None
                }
            }),
        }?;

        let line = self.project.get_line(line_id)?;
        if frame <= line.start_frame || frame >= line.end_frame() {
            return None;
        }
        let progress =
            ((frame - line.start_frame) as f32 / line.duration_frames as f32).clamp(0.0, 1.0);
        Some(DialogueSplitTarget::Playhead { line_id, progress })
    }

    pub fn move_marker(&mut self, index: usize, frame: i64) {
        if index >= self.project.markers.len() {
            return;
        }
        let old_frame = self.project.markers[index].frame;
        self.push_and_broadcast(Command::MoveMarker {
            index,
            old_frame,
            new_frame: frame,
        });
        self.project.move_marker(index, frame);
        self.dirty = true;
    }

    pub fn add_marker(&mut self, kind: crate::rythmo_line::MarkerKind) {
        let frame = self.current_frame();
        let index = self
            .project
            .add_marker(crate::rythmo_line::RythmoMarker { kind, frame });
        self.push_and_broadcast(Command::AddMarker { index });
    }

    pub fn add_quick_line(&mut self, text: String) {
        let frame = self.current_frame();
        let dur = (self.fps() * 1.0) as i64; // 1 second
        let line_id = self.project.add_line(frame, dur, 0.0);
        if let Some(line) = self.project.get_line_mut(line_id) {
            line.text = text;
        }
        self.push_and_broadcast(Command::CreateLine { line_id });
    }

    pub fn create_line(&mut self, frame: i64, y_slot: f32) -> u64 {
        let default_dur = (self.fps() * constants::DEFAULT_LINE_DURATION_SEC) as i64;
        let dur = self
            .project
            .lines()
            .filter(|line| (line.y_slot - y_slot).abs() < 0.01 && line.start_frame > frame)
            .map(|line| line.start_frame)
            .min()
            .map(|start| (start - frame - constants::TICK_GAP_FRAMES).clamp(1, default_dur))
            .unwrap_or(default_dur);
        let line_id = self.project.add_line(frame, dur, y_slot);
        self.push_and_broadcast(Command::CreateLine { line_id });
        line_id
    }

    pub fn start_editing_line(&mut self, line_id: u64) {
        if let Some(line) = self.project.get_line(line_id) {
            let text = line.text.clone();
            self.ui.rythmo_state.start_editing_line(line_id, &text);
        }
    }

    pub fn move_line(&mut self, id: u64, start_frame: i64, y_slot: f32) {
        // Coalesce: update last command if same line drag
        if self.history.last_matches(id, CommandKind::MoveLine) {
            if let Some(line) = self.project.get_line_mut(id) {
                line.start_frame = start_frame;
                line.y_slot = y_slot;
            }
            self.history.update_last(|cmd| {
                if let Command::MoveLine {
                    new_start,
                    new_y_slot,
                    ..
                } = cmd
                {
                    *new_start = start_frame;
                    *new_y_slot = y_slot;
                }
            });
        } else if let Some(line) = self.project.get_line(id) {
            let old_start = line.start_frame;
            let old_y = line.y_slot;
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.y_slot = y_slot;
            }
            self.push_and_broadcast(Command::MoveLine {
                line_id: id,
                old_start,
                old_y_slot: old_y,
                new_start: start_frame,
                new_y_slot: y_slot,
            });
        }
    }

    pub fn move_lines(&mut self, moves: Vec<(u64, i64, f32)>) {
        let mut requested = Vec::new();
        for (line_id, new_start, new_y_slot) in moves {
            if self.project.get_line(line_id).is_some() {
                requested.push((line_id, new_start, new_y_slot));
            }
        }
        if requested.is_empty() {
            return;
        }

        let same_group = matches!(
            self.history.last(),
            Some(Command::MoveLines { moves })
                if moves.len() == requested.len()
                    && moves
                        .iter()
                        .zip(requested.iter())
                        .all(|(movement, (line_id, _, _))| movement.line_id == *line_id)
        );

        if same_group {
            for (line_id, new_start, new_y_slot) in &requested {
                if let Some(line) = self.project.get_line_mut(*line_id) {
                    line.start_frame = *new_start;
                    line.y_slot = *new_y_slot;
                }
            }
            self.history.update_last(|cmd| {
                if let Command::MoveLines { moves } = cmd {
                    for (movement, (_, new_start, new_y_slot)) in moves.iter_mut().zip(&requested) {
                        movement.new_start = *new_start;
                        movement.new_y_slot = *new_y_slot;
                    }
                }
            });
            self.dirty = true;
            return;
        }

        let mut command_moves = Vec::new();
        for (line_id, new_start, new_y_slot) in requested {
            if let Some(line) = self.project.get_line(line_id) {
                if line.start_frame == new_start && (line.y_slot - new_y_slot).abs() < f32::EPSILON
                {
                    continue;
                }
                command_moves.push(LineMove {
                    line_id,
                    old_start: line.start_frame,
                    old_y_slot: line.y_slot,
                    new_start,
                    new_y_slot,
                });
            }
        }
        if command_moves.is_empty() {
            return;
        }

        for movement in &command_moves {
            if let Some(line) = self.project.get_line_mut(movement.line_id) {
                line.start_frame = movement.new_start;
                line.y_slot = movement.new_y_slot;
            }
        }
        self.push_and_broadcast(Command::MoveLines {
            moves: command_moves,
        });
    }

    pub fn resize_line(&mut self, id: u64, start_frame: i64, duration_frames: i64) {
        if self.history.last_matches(id, CommandKind::ResizeLine) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.duration_frames = duration_frames;
            }
            self.history.update_last(|cmd| {
                if let Command::ResizeLine {
                    new_start, new_dur, ..
                } = cmd
                {
                    *new_start = start_frame;
                    *new_dur = duration_frames;
                }
            });
        } else if let Some(line) = self.project.get_line(id) {
            let old_start = line.start_frame;
            let old_dur = line.duration_frames;
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.duration_frames = duration_frames;
            }
            self.push_and_broadcast(Command::ResizeLine {
                line_id: id,
                old_start,
                old_dur,
                new_start: start_frame,
                new_dur: duration_frames,
            });
        }
    }

    pub fn update_line_text(&mut self, id: u64, text: String) {
        // Coalesce: update last text command for same line
        if self.history.last_matches(id, CommandKind::UpdateLineText) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.text = text.clone();
            }
            self.history.update_last(|cmd| {
                if let Command::UpdateLineText { new_text, .. } = cmd {
                    *new_text = text;
                }
            });
        } else {
            let old_text = self
                .project
                .get_line(id)
                .map(|l| l.text.clone())
                .unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(id) {
                l.text = text.clone();
            }
            self.push_and_broadcast(Command::UpdateLineText {
                line_id: id,
                old_text,
                new_text: text,
            });
        }
    }

    pub fn set_syllable_ratios(&mut self, line_id: u64, ratios: Vec<f32>) {
        let Some(line) = self.project.get_line(line_id) else {
            return;
        };
        let old_ratios = line.syllable_ratios.clone();
        if old_ratios == ratios {
            return;
        }

        if let Some(line) = self.project.get_line_mut(line_id) {
            line.syllable_ratios = ratios.clone();
        }
        self.push_and_broadcast(Command::SetSyllableRatios {
            line_id,
            old_ratios,
            new_ratios: ratios,
        });
    }

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        let Some(line) = self.project.get_line(line_id) else {
            return;
        };
        let old_name = line.character_name.clone();
        let old_color = line.character_color;
        let old_voice_actor_names = line.voice_actor_names.clone();
        let new_voice_actor_names = self.voice_actor_names_for_character_change(line_id, &name);
        if old_name == name && old_color == color && old_voice_actor_names == new_voice_actor_names
        {
            return;
        }

        self.project.set_character_with_voice_actors(
            line_id,
            name.clone(),
            color,
            new_voice_actor_names.clone(),
        );
        self.push_and_broadcast(Command::SetCharacter {
            line_id,
            old_name,
            old_color,
            old_voice_actor_names,
            new_name: name,
            new_color: color,
            new_voice_actor_names,
        });
    }

    fn apply_character_change(
        &mut self,
        line_id: u64,
        name: String,
        color: [f32; 4],
        voice_actor_names: Option<Vec<String>>,
    ) {
        let voice_actor_names = voice_actor_names
            .unwrap_or_else(|| self.voice_actor_names_for_character_change(line_id, &name));
        self.project
            .set_character_with_voice_actors(line_id, name, color, voice_actor_names);
    }

    fn voice_actor_names_for_character_change(&self, line_id: u64, name: &str) -> Vec<String> {
        let Some(line) = self.project.get_line(line_id) else {
            return Vec::new();
        };
        if line.character_name == name {
            line.voice_actor_names.clone()
        } else {
            self.project.voice_actor_names_for_character(name, line_id)
        }
    }

    pub fn set_character_color(&mut self, line_id: u64, color: [f32; 4]) {
        if self
            .history
            .last_matches(line_id, CommandKind::SetCharacterColor)
        {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.history.update_last(|cmd| {
                if let Command::SetCharacterColor { new_color, .. } = cmd {
                    *new_color = color;
                }
            });
        } else {
            let old_color = self
                .project
                .get_line(line_id)
                .map(|l| l.character_color)
                .unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.push_and_broadcast(Command::SetCharacterColor {
                line_id,
                old_color,
                new_color: color,
            });
        }
    }

    pub fn update_character_name(&mut self, line_id: u64, name: String) {
        let Some(current_line) = self.project.get_line(line_id) else {
            return;
        };
        let old_name = current_line.character_name.clone();
        let old_color = current_line.character_color;
        let old_voice_actor_names = current_line.voice_actor_names.clone();
        let new_voice_actor_names = match self.history.last() {
            Some(Command::SetCharacter {
                line_id: command_line_id,
                old_name,
                old_voice_actor_names,
                ..
            }) if *command_line_id == line_id && old_name == &name => old_voice_actor_names.clone(),
            _ => self.voice_actor_names_for_character_change(line_id, &name),
        };
        let known_color = self
            .project
            .known_characters
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.color);

        // Coalesce character name edits
        if self
            .history
            .last_matches(line_id, CommandKind::SetCharacter)
        {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color {
                    l.character_color = c;
                }
                l.voice_actor_names = new_voice_actor_names.clone();
            }
            let final_color = self
                .project
                .get_line(line_id)
                .map(|l| l.character_color)
                .unwrap_or_default();
            self.history.update_last(|cmd| {
                if let Command::SetCharacter {
                    new_name,
                    new_color,
                    new_voice_actor_names: command_voice_actor_names,
                    ..
                } = cmd
                {
                    *new_name = name;
                    *new_color = final_color;
                    *command_voice_actor_names = new_voice_actor_names;
                }
            });
        } else {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color {
                    l.character_color = c;
                }
                l.voice_actor_names = new_voice_actor_names.clone();
            }
            let final_color = self
                .project
                .get_line(line_id)
                .map(|l| l.character_color)
                .unwrap_or_default();
            self.push_and_broadcast(Command::SetCharacter {
                line_id,
                old_name,
                old_color,
                old_voice_actor_names,
                new_name: name,
                new_color: final_color,
                new_voice_actor_names,
            });
        }
    }

    pub fn finalize_character(&mut self, line_id: u64) {
        let (name, color) = match self.project.get_line(line_id) {
            Some(l) if !l.character_name.is_empty() => {
                (l.character_name.clone(), l.character_color)
            }
            _ => return,
        };
        self.project.set_character(line_id, name, color);
    }

    pub fn create_voice_actor(&mut self, name: String, icon_path: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.show_toast(crate::i18n::t("toast.voice_actor_name_required"), 4.0);
            return;
        }
        if self.project.find_voice_actor(&name).is_some() {
            self.show_toast(crate::i18n::t("toast.voice_actor_exists"), 4.0);
            return;
        }

        let icon_path = icon_path
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        let icon_png_base64 = if icon_path.is_empty() {
            None
        } else {
            match crate::voice_actor::load_icon_png_base64(Path::new(&icon_path)) {
                Ok(icon) => Some(icon),
                Err(e) => {
                    self.show_toast(
                        format!("{} {e}", crate::i18n::t("toast.voice_actor_icon_failed")),
                        6.0,
                    );
                    return;
                }
            }
        };

        let actor = VoiceActor {
            name: name.clone(),
            icon_path,
            icon_png_base64,
        };
        if self.project.add_voice_actor(actor.clone()) {
            self.push_and_broadcast(Command::CreateVoiceActor { actor });
            self.show_toast(crate::i18n::t("toast.voice_actor_created"), 3.0);
        }
    }

    pub fn set_voice_actor_modal_icon_path(&mut self, path: impl Into<String>) {
        self.ui.set_voice_actor_modal_icon_path(path);
    }

    pub fn assign_voice_actor_to_line(&mut self, line_id: u64, actor_name: String) {
        let Some(line) = self.project.get_line(line_id) else {
            return;
        };
        let new_names =
            Project::with_voice_actor_assignment(&line.voice_actor_names, &actor_name, true);
        self.set_voice_actor_changes(vec![LineVoiceActorsChange {
            line_id,
            old_voice_actor_names: line.voice_actor_names.clone(),
            new_voice_actor_names: new_names,
        }]);
    }

    pub fn unassign_voice_actor_from_line(&mut self, line_id: u64, actor_name: String) {
        let Some(line) = self.project.get_line(line_id) else {
            return;
        };
        let new_names =
            Project::with_voice_actor_assignment(&line.voice_actor_names, &actor_name, false);
        self.set_voice_actor_changes(vec![LineVoiceActorsChange {
            line_id,
            old_voice_actor_names: line.voice_actor_names.clone(),
            new_voice_actor_names: new_names,
        }]);
    }

    pub fn assign_voice_actor_to_character(&mut self, line_id: u64, actor_name: String) {
        self.set_voice_actor_for_character(line_id, actor_name, true);
    }

    pub fn unassign_voice_actor_from_character(&mut self, line_id: u64, actor_name: String) {
        self.set_voice_actor_for_character(line_id, actor_name, false);
    }

    fn set_voice_actor_for_character(&mut self, line_id: u64, actor_name: String, assign: bool) {
        let Some(character_name) = self
            .project
            .get_line(line_id)
            .map(|line| line.character_name.clone())
            .filter(|name| !name.trim().is_empty())
        else {
            return;
        };

        let changes = self
            .project
            .lines()
            .filter(|line| line.character_name == character_name)
            .filter_map(|line| {
                let new_names = Project::with_voice_actor_assignment(
                    &line.voice_actor_names,
                    &actor_name,
                    assign,
                );
                if new_names == line.voice_actor_names {
                    None
                } else {
                    Some(LineVoiceActorsChange {
                        line_id: line.id,
                        old_voice_actor_names: line.voice_actor_names.clone(),
                        new_voice_actor_names: new_names,
                    })
                }
            })
            .collect();
        self.set_voice_actor_changes(changes);
    }

    fn set_voice_actor_changes(&mut self, changes: Vec<LineVoiceActorsChange>) {
        let changes: Vec<_> = changes
            .into_iter()
            .filter(|change| change.old_voice_actor_names != change.new_voice_actor_names)
            .collect();
        if changes.is_empty() {
            return;
        }

        self.apply_voice_actor_changes(&changes, true);
        self.push_and_broadcast(Command::SetVoiceActors { changes });
    }

    fn apply_voice_actor_changes(&mut self, changes: &[LineVoiceActorsChange], use_new: bool) {
        for change in changes {
            let names = if use_new {
                change.new_voice_actor_names.clone()
            } else {
                change.old_voice_actor_names.clone()
            };
            self.project
                .set_line_voice_actor_names(change.line_id, names);
        }
    }

    pub fn start_editing_note(&mut self, line_id: u64) {
        let note = self
            .project
            .get_line(line_id)
            .map(|l| l.note.clone())
            .unwrap_or_default();
        let text = if note.is_empty() {
            "Note".to_string()
        } else {
            note
        };
        self.ui.rythmo_state.start_editing_note(line_id, &text);
        if self
            .project
            .get_line(line_id)
            .map(|l| l.note.is_empty())
            .unwrap_or(true)
        {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.note = "Note".to_string();
            }
            self.dirty = true;
        }
    }

    pub fn start_editing_note_selected(&mut self) {
        if let Some(crate::ui::rythmo::Selection::Line(id)) = self.ui.rythmo_state.selected {
            self.start_editing_note(id);
        }
    }

    pub fn update_line_note(&mut self, id: u64, note: String) {
        use crate::command::{Command, CommandKind};
        if self.history.last_matches(id, CommandKind::UpdateLineNote) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.note = note.clone();
            }
            self.history.update_last(|cmd| {
                if let Command::UpdateLineNote { new_note, .. } = cmd {
                    *new_note = note;
                }
            });
        } else {
            let old_note = self
                .project
                .get_line(id)
                .map(|l| l.note.clone())
                .unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(id) {
                l.note = note.clone();
            }
            self.push_and_broadcast(Command::UpdateLineNote {
                line_id: id,
                old_note,
                new_note: note,
            });
        }
    }

    // -- Backup --

    fn backup_path() -> std::path::PathBuf {
        std::env::current_exe()
            .map(|p| {
                p.parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("br_backup.json")
            })
            .unwrap_or_else(|_| std::path::PathBuf::from("br_backup.json"))
    }

    pub fn save_backup(&self) {
        use crate::export::{JsonExporter, ProjectExporter};
        let path = Self::backup_path();
        let fps = self.fps();
        if let Err(e) = JsonExporter.export(&self.project, fps, &path) {
            log::warn!("Auto-save failed: {e}");
        } else {
            log::info!("Auto-saved to {}", path.display());
        }
    }

    pub fn restore_backup(&mut self) -> bool {
        use crate::export::{JsonImporter, ProjectImporter};
        let path = Self::backup_path();
        if !path.exists() {
            return false;
        }
        match JsonImporter.import(&path) {
            Ok(data) => {
                let fps = self.fps();
                data.apply_to_project(&mut self.project, fps);
                true
            }
            Err(e) => {
                log::error!("Restore backup failed: {e}");
                false
            }
        }
    }

    // -- Render --

    fn tick_video(&mut self) {
        if let Some(player) = &mut self.video_player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.ui_renderer.texture_bind_group_layout(),
                self.ui_renderer.texture_sampler(),
            );
            player.tick(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if player.current_frame() != prev_frame {
                self.timeline.emit(TimelineEvent::FrameChanged {
                    frame: player.current_frame(),
                });
            }
            if !player.is_playing() && self.ui.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui.toggle_play_pause();
            }
        }
    }

    fn poll_proxy_job(&mut self) -> bool {
        let result = match self.pending_proxy_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Proxy job disconnected".into())),
            },
            None => None,
        };

        let Some(result) = result else {
            return false;
        };

        let Some(job) = self.pending_proxy_job.take() else {
            return false;
        };

        self.set_export_progress(None);
        match result {
            Ok(proxy_path) => {
                log::info!("Proxy created at {}", proxy_path.display());
                let current_source = self.video_path();
                if current_source
                    .as_ref()
                    .is_some_and(|path| crate::video_proxy::paths_match(path, &job.source_path))
                {
                    let frame = self.current_frame();
                    self.load_video_for_playback(&job.source_path, Some(&proxy_path), Some(frame));
                    self.show_toast(crate::i18n::t("toast.proxy_created"), 4.0);
                } else {
                    self.show_toast(crate::i18n::t("toast.proxy_created_not_loaded"), 5.0);
                }
            }
            Err(e) => {
                if crate::video_proxy::is_cancelled_error(&e) {
                    log::info!("Proxy creation canceled");
                } else {
                    log::error!("Proxy creation failed: {e}");
                    self.show_proxy_error(e);
                }
            }
        }

        true
    }

    fn poll_export_job(&mut self) -> bool {
        let result = match self.pending_export_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Export job disconnected".into())),
            },
            None => None,
        };

        let Some(result) = result else {
            return false;
        };

        self.pending_export_job = None;
        self.set_export_progress(None);
        match result {
            Ok(()) => {
                log::info!("MP4 export completed");
                self.show_toast(crate::i18n::t("toast.export_completed"), 4.0);
            }
            Err(e) => {
                if crate::video_export::is_cancelled_error(&e) {
                    log::info!("MP4 export canceled");
                } else {
                    log::error!("MP4 export failed: {e}");
                    self.show_toast(
                        format!("{} {e}", crate::i18n::t("toast.export_failed")),
                        8.0,
                    );
                }
            }
        }

        true
    }

    pub fn tick_background(&mut self) -> bool {
        let mut changed = false;

        if let Ok(mut results) = self.ping_results.try_lock() {
            for r in results.drain(..) {
                if let Some(browser) = self.ui.server_browser_mut() {
                    if r.success {
                        browser.update_server_info(
                            &r.ip,
                            r.port,
                            r.name,
                            r.motd,
                            r.online,
                            r.max_slots,
                        );
                    } else {
                        browser.mark_offline(&r.ip, r.port);
                    }
                    changed = true;
                }
            }
        }

        // Auto-save every 60 seconds if project is dirty. This is not directly visible,
        // but it needs a timer now that idle redraw no longer drives render calls.
        if self.dirty && self.last_autosave.elapsed().as_secs() >= 60 {
            self.save_backup();
            self.last_autosave = Instant::now();
        }

        if let Some(progress) = &self.ui.export_progress {
            use std::sync::atomic::Ordering;
            let v = f32::from_bits(progress.load(Ordering::Relaxed));
            if v >= 1.5 {
                // Sentinel: 2.0 means the worker thread has actually exited.
                self.ui.export_progress = None;
                self.ui.export_render_backend = None;
                self.ui.progress_prefix = String::new();
                self.active_export_cancel = None;
                log::info!("Export completed");
                changed = true;
            }
        }

        changed |= self.tick_network();
        changed |= self.tick_scroll_decode();
        changed |= self.poll_export_job();
        changed |= self.poll_proxy_job();
        changed |= self.poll_file_explorer();
        changed |= self.poll_waveform_change();
        changed
    }

    fn poll_waveform_change(&mut self) -> bool {
        let revision = self.current_waveform_revision();
        if revision != self.last_waveform_revision {
            self.last_waveform_revision = revision;
            return true;
        }
        false
    }

    fn current_waveform_revision(&self) -> u64 {
        self.video_player
            .as_ref()
            .map(|player| player.waveform_revision())
            .unwrap_or(0)
    }

    fn waveform_redraw_pending(&self) -> bool {
        self.current_waveform_revision() != self.last_waveform_revision
    }

    fn waveform_decode_pending(&self) -> bool {
        self.video_player
            .as_ref()
            .is_some_and(|player| player.is_waveform_decoding())
    }

    fn app_refresh_interval() -> Duration {
        Duration::from_nanos(constants::APP_REFRESH_INTERVAL_NS)
    }
    fn scroll_decode_due(&self, now: Instant) -> bool {
        self.scroll_needs_decode
            && self.last_scroll_time.is_some_and(|last| {
                now.duration_since(last).as_millis() >= constants::SCROLL_DECODE_DELAY_MS
            })
    }

    fn periodic_redraw_due(&self, now: Instant) -> bool {
        if self.ui.has_active_progress() || self.pending_proxy_job.is_some() {
            return now.duration_since(self.last_redraw) >= Duration::from_millis(100);
        }

        if self.ui.is_editing_text() {
            return self
                .ui
                .next_cursor_blink_deadline()
                .is_some_and(|deadline| deadline <= now)
                || now.duration_since(self.last_redraw) >= Duration::from_millis(500);
        }

        false
    }

    fn continuous_redraw_due(&self, now: Instant) -> bool {
        (self.needs_continuous_redraw() || self.secondary_needs_continuous_redraw())
            && now.saturating_duration_since(self.last_redraw) >= Self::app_refresh_interval()
    }
    pub fn needs_redraw_now(&self) -> bool {
        let now = Instant::now();
        self.scroll_decode_due(now)
            || self.periodic_redraw_due(now)
            || self.waveform_redraw_pending()
            || self.continuous_redraw_due(now)
    }

    pub fn needs_continuous_redraw(&self) -> bool {
        self.is_video_playing() || self.ui.needs_animation_or_interaction()
    }

    pub fn secondary_needs_continuous_redraw(&self) -> bool {
        self.has_secondary_display() && self.is_video_playing()
    }

    pub fn next_wake_deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut deadline: Option<Instant> = None;
        let mut push_deadline = |candidate: Instant| {
            deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
        };

        if self.needs_continuous_redraw() || self.secondary_needs_continuous_redraw() {
            push_deadline(self.last_redraw + Self::app_refresh_interval());
        }

        if self.ui.has_active_progress() || self.pending_proxy_job.is_some() {
            push_deadline(self.last_redraw + Duration::from_millis(100));
        }

        if self.ui.is_editing_text() {
            if let Some(cursor_deadline) = self.ui.next_cursor_blink_deadline() {
                push_deadline(cursor_deadline);
            } else {
                push_deadline(self.last_redraw + Duration::from_millis(500));
            }
        }

        if self.scroll_needs_decode {
            if let Some(last_scroll) = self.last_scroll_time {
                push_deadline(
                    last_scroll + Duration::from_millis(constants::SCROLL_DECODE_DELAY_MS as u64),
                );
            }
        }

        if self.dirty {
            push_deadline(self.last_autosave + Duration::from_secs(60));
        }

        if self.waveform_decode_pending() || self.waveform_redraw_pending() {
            push_deadline(now + Duration::from_millis(100));
        }

        if self.network.state != ConnectionState::Disconnected || self.ui.needs_background_poll() {
            push_deadline(now + Duration::from_millis(100));
        }

        deadline
    }

    pub fn render(&mut self) {
        if self.studio_mode {
            self.render_studio();
            return;
        }

        let surface_texture = match self.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.gfx
                    .surface
                    .configure(&self.gfx.device, &self.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Clear
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // Drain timeline events
        let _events = self.timeline.drain();

        self.tick_video();

        // Video quad — skip when export modal is showing (it would cover the modal)
        let video_quad = if self.ui.export_progress.is_some() || self.secondary_display.is_some() {
            None
        } else {
            build_video_quad(&self.video_player, &self.ui)
        };
        let current_frame = self.current_frame();
        let render_frame = self.render_frame();

        // UI render. Keep a read guard instead of cloning the waveform every frame.
        let waveform_arc = self
            .video_player
            .as_ref()
            .map(|player| player.waveform_for_render());
        let waveform_guard = waveform_arc
            .as_ref()
            .and_then(|waveform| waveform.read().ok());
        let empty_waveform: &[f32] = &[];
        let waveform = waveform_guard
            .as_deref()
            .map(Vec::as_slice)
            .unwrap_or(empty_waveform);
        let fps = self.fps();
        let waveform_offset_frames = self.active_audio_offset_frames();
        let waveform_is_instrumental = self.active_audio_is_instrumental();
        self.render_index.refresh(&self.project);
        self.ui.render(
            &mut self.ui_renderer,
            &self.gfx.device,
            &self.gfx.queue,
            &mut encoder,
            &view,
            self.gfx.config.width,
            self.gfx.config.height,
            self.ui_scale,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project,
            &self.render_index,
            current_frame,
            render_frame,
            fps,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        self.last_redraw = Instant::now();
    }

    fn render_studio(&mut self) {
        let surface_texture = match self.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.gfx
                    .surface
                    .configure(&self.gfx.device, &self.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Studio Render Encoder"),
            });

        // Clear to black
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Studio Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // Drain timeline and tick video. Background work is handled from AboutToWait.
        let _events = self.timeline.drain();
        self.tick_video();

        let rythmo_h = crate::ui::rythmo::studio_br_height(&self.project, self.ui.screen_w());
        let video_quad = if self.secondary_display.is_some() {
            None
        } else {
            build_studio_video_quad(&self.video_player, &self.ui, rythmo_h)
        };
        let render_frame = self.render_frame();
        let fps = self.fps();
        self.render_index.refresh(&self.project);

        self.ui.render_studio(
            &mut self.ui_renderer,
            &self.gfx.device,
            &self.gfx.queue,
            &mut encoder,
            &view,
            self.gfx.config.width,
            self.gfx.config.height,
            self.ui_scale,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project,
            &self.render_index,
            render_frame,
            fps,
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        self.last_redraw = Instant::now();
    }

    pub fn render_secondary_display(&mut self, window_id: WindowId) {
        self.tick_video();

        let Some(display) = &mut self.secondary_display else {
            return;
        };
        if display.window.id() != window_id {
            return;
        }

        let surface_texture = match display.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                display.surface.configure(&self.gfx.device, &display.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let width = display.config.width;
        let height = display.config.height;
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Secondary Display Render Encoder"),
            });

        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Secondary Display Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        let video_quad =
            build_full_window_video_quad(&self.video_player, width as f32, height as f32);
        self.ui_renderer.render(
            &self.gfx.device,
            &self.gfx.queue,
            &mut encoder,
            &view,
            width,
            height,
            1.0,
            &[],
            &[],
            &[],
            &[],
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &[],
            &[],
            &[],
            &[],
            &[],
            &[], // no modal quads
            &[], // no modal labels
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        self.last_redraw = Instant::now();
    }
}

fn build_video_quad<'a>(
    video_player: &'a Option<VideoPlayer>,
    ui: &Ui,
) -> Option<(&'a wgpu::BindGroup, crate::ui::widget::IconInstance)> {
    let player = video_player.as_ref()?;
    let bind_group = player.bind_group.as_ref()?;
    let (vid_w, vid_h) = player.video_size()?;
    let preview = &ui.layout().video_preview;

    let vid_aspect = vid_w as f32 / vid_h as f32;
    let zone_aspect = preview.width / preview.height;
    let (draw_w, draw_h) = if vid_aspect > zone_aspect {
        (preview.width, preview.width / vid_aspect)
    } else {
        (preview.height * vid_aspect, preview.height)
    };

    Some((
        bind_group,
        crate::ui::widget::IconInstance {
            rect: [
                preview.x + (preview.width - draw_w) / 2.0,
                preview.y + (preview.height - draw_h) / 2.0,
                draw_w,
                draw_h,
            ],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
        },
    ))
}

fn build_studio_video_quad<'a>(
    video_player: &'a Option<VideoPlayer>,
    ui: &Ui,
    rythmo_h: f32,
) -> Option<(&'a wgpu::BindGroup, crate::ui::widget::IconInstance)> {
    let player = video_player.as_ref()?;
    let bind_group = player.bind_group.as_ref()?;
    let (vid_w, vid_h) = player.video_size()?;

    let screen_w = ui.screen_w();
    let screen_h = ui.screen_h();
    let video_zone_h = screen_h - rythmo_h;

    let vid_aspect = vid_w as f32 / vid_h as f32;
    let zone_aspect = screen_w / video_zone_h;
    let (draw_w, draw_h) = if vid_aspect > zone_aspect {
        (screen_w, screen_w / vid_aspect)
    } else {
        (video_zone_h * vid_aspect, video_zone_h)
    };

    Some((
        bind_group,
        crate::ui::widget::IconInstance {
            rect: [
                (screen_w - draw_w) / 2.0,
                (video_zone_h - draw_h) / 2.0,
                draw_w,
                draw_h,
            ],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
        },
    ))
}

fn build_full_window_video_quad<'a>(
    video_player: &'a Option<VideoPlayer>,
    screen_w: f32,
    screen_h: f32,
) -> Option<(&'a wgpu::BindGroup, crate::ui::widget::IconInstance)> {
    let player = video_player.as_ref()?;
    let bind_group = player.bind_group.as_ref()?;
    let (vid_w, vid_h) = player.video_size()?;

    let vid_aspect = vid_w as f32 / vid_h as f32;
    let screen_aspect = screen_w / screen_h;
    let (draw_w, draw_h) = if vid_aspect > screen_aspect {
        (screen_w, screen_w / vid_aspect)
    } else {
        (screen_h * vid_aspect, screen_h)
    };

    Some((
        bind_group,
        crate::ui::widget::IconInstance {
            rect: [
                (screen_w - draw_w) / 2.0,
                (screen_h - draw_h) / 2.0,
                draw_w,
                draw_h,
            ],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
        },
    ))
}

fn ping_server_socketio(
    ip: &str,
    port: u16,
    results: std::sync::Arc<std::sync::Mutex<Vec<PingResult>>>,
) {
    use rust_socketio::{ClientBuilder, Event, Payload};

    let ip_clone = ip.to_string();
    let port_clone = port;
    let results_clone = results.clone();
    let url = format!("http://{}:{}", ip, port);

    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done2 = done.clone();
    let done3 = done.clone();
    let ip_for_info = ip_clone.clone();
    let ip_for_disc = ip_clone.clone();
    let results_disc = results.clone();

    let client = ClientBuilder::new(&url)
        .on("server_info", move |payload, _client| {
            if let Payload::Text(values) = payload {
                if let Some(info) = values.first() {
                    let name = info["name"].as_str().unwrap_or("").to_string();
                    let motd = info["motd"].as_str().unwrap_or("").to_string();
                    let online = info["online"].as_u64().unwrap_or(0) as u32;
                    let max_slots = info["max_slots"].as_u64().unwrap_or(0) as u32;
                    if let Ok(mut r) = results_clone.lock() {
                        r.push(PingResult {
                            ip: ip_for_info.clone(),
                            port: port_clone,
                            name,
                            motd,
                            online,
                            max_slots,
                            success: true,
                        });
                    }
                    done2.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        })
        .on(Event::Connect, move |_, client| {
            let _ = client.emit("ping_server", serde_json::json!({}));
        })
        .on(Event::Close, move |_, _| {
            // Server disconnected us — if we didn't get info, mark offline
            if !done3.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(mut r) = results_disc.lock() {
                    r.push(PingResult {
                        ip: ip_for_disc.clone(),
                        port: port_clone,
                        name: String::new(),
                        motd: String::new(),
                        online: 0,
                        max_slots: 0,
                        success: false,
                    });
                }
                done3.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .connect();

    match client {
        Ok(_c) => {
            // Wait for server to disconnect us (max 6s safety)
            for _ in 0..60 {
                if done.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // If still no response after timeout, mark offline
            if !done.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(mut r) = results.lock() {
                    r.push(PingResult {
                        ip: ip_clone,
                        port,
                        name: String::new(),
                        motd: String::new(),
                        online: 0,
                        max_slots: 0,
                        success: false,
                    });
                }
            }
        }
        Err(_) => {
            if let Ok(mut r) = results.lock() {
                r.push(PingResult {
                    ip: ip_clone,
                    port,
                    name: String::new(),
                    motd: String::new(),
                    online: 0,
                    max_slots: 0,
                    success: false,
                });
            }
        }
    }
}
