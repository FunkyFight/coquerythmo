use std::path::Path;
use std::sync::Arc;
use wgpu::CurrentSurfaceTexture;
use winit::window::Window;

use std::time::Instant;

use crate::command::{Command, CommandHistory, CommandKind};
use crate::graphics::GraphicsContext;
use crate::network::{ConnectionState, IncomingMessage, NetworkClient};
use crate::observer::{TimelineBus, TimelineEvent};
use crate::packet::{CommandPayload, Packet, ProjectData};
use crate::project::Project;
use crate::ui::renderer::UiRenderer;
use crate::ui::widget::{EventResponse, UiEvent};
use crate::ui::Ui;
use crate::video::VideoPlayer;

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

pub struct State {
    pub gfx: GraphicsContext,
    window: Arc<Window>,
    ui: Ui,
    ui_renderer: UiRenderer,
    video_player: Option<VideoPlayer>,
    pub project: Project,
    pub project_path: Option<std::path::PathBuf>,
    pub dirty: bool,
    history: CommandHistory,
    pub timeline: TimelineBus,
    pub network: NetworkClient,
    last_scroll_time: Option<Instant>,
    scroll_needs_decode: bool,
    ping_results: std::sync::Arc<std::sync::Mutex<Vec<PingResult>>>,
    last_autosave: Instant,
    studio_mode: bool,
    fullscreen_before_studio: Option<winit::window::Fullscreen>,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let gfx = GraphicsContext::new(window.clone()).await;
        let format = gfx.surface_format();
        let ui_renderer = UiRenderer::new(&gfx.device, &gfx.queue, format);
        let ui = Ui::new(gfx.size.width, gfx.size.height, &ui_renderer.icon_atlas);

        Self {
            gfx, window, ui, ui_renderer, video_player: None, project: Project::new(),
            project_path: None, dirty: false,
            history: CommandHistory::new(), timeline: TimelineBus::new(),
            network: NetworkClient::new(),
            last_scroll_time: None, scroll_needs_decode: false,
            ping_results: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            last_autosave: Instant::now(),
            studio_mode: false,
            fullscreen_before_studio: None,
        }
    }

    // -- Delegation helpers --

    fn renderer_refs(&self) -> (&wgpu::BindGroupLayout, &wgpu::Sampler) {
        (self.ui_renderer.texture_bind_group_layout(), self.ui_renderer.texture_sampler())
    }

    // -- Public API --

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gfx.resize(new_size);
        self.ui.resize(new_size.width, new_size.height);
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) -> EventResponse {
        self.ui.handle_event(event, &self.project, self.current_frame(), self.fps())
    }

    pub fn set_export_progress(&mut self, p: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>) {
        let is_none = p.is_none();
        self.ui.export_progress = p;
        if is_none { self.ui.progress_prefix = String::new(); }
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

    pub fn show_toast(&mut self, message: &str, duration_secs: f32) {
        self.ui.toasts.push(message, duration_secs);
    }

    pub fn open_save_prompt(&mut self) {
        self.ui.open_save_prompt();
    }

    pub fn toggle_syllable_mode(&mut self) {
        self.ui.rythmo_state.syllable_mode = !self.ui.rythmo_state.syllable_mode;
        self.ui.rebuild_toolbar();
        log::info!("Syllable mode: {}", self.ui.rythmo_state.syllable_mode);
    }

    pub fn open_export_modal(&mut self) {
        self.ui.open_export_modal();
    }

    pub fn close_settings_modal(&mut self) {
        self.ui.close_settings_modal();
        self.ui_renderer.clear_text_cache();
    }

    pub fn rebuild_topbar_for_network(&mut self) {
        self.ui.rebuild_topbar(self.network.is_in_room());
    }

    pub fn request_redraw(&self) {
        self.gfx.request_redraw();
    }

    // -- Video --

    pub fn current_frame(&self) -> i64 {
        self.video_player.as_ref().map_or(0, |p| p.current_frame())
    }

    pub fn fps(&self) -> f64 {
        self.video_player.as_ref().map_or(30.0, |p| p.fps())
    }

    pub fn waveform(&self) -> Vec<f32> {
        self.video_player.as_ref()
            .and_then(|p| p.waveform.read().ok())
            .map(|w| w.clone())
            .unwrap_or_default()
    }

    pub fn video_path(&self) -> Option<std::path::PathBuf> {
        self.video_player.as_ref().and_then(|p| p.path())
    }

    pub fn load_video(&mut self, path: &Path) {
        let (bgl, sampler) = self.renderer_refs();
        let mut player = VideoPlayer::new();
        match player.load(path, &self.gfx.device, &self.gfx.queue, bgl, sampler) {
            Ok(()) => {
                player.set_volume(self.ui.volume());
                let fps = player.fps();
                let total = player.total_frames();
                self.video_player = Some(player);
                self.timeline.emit(TimelineEvent::VideoLoaded { fps, total_frames: total });
                self.timeline.emit(TimelineEvent::FrameChanged { frame: 0 });
                self.ui.has_video = true;
                self.ui.total_frames = total;
                self.rebuild_topbar_for_network();
            }
            Err(e) => log::error!("Failed to load video: {e}"),
        }
    }

    pub fn toggle_play_pause(&mut self) {
        if let Some(player) = &mut self.video_player {
            player.toggle();
            self.ui.toggle_play_pause();
            if player.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStarted);
            } else {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
            }
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.ui.set_volume(vol);
        if let Some(player) = &mut self.video_player {
            player.set_volume(vol);
        }
    }

    pub fn prev_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_backward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() { self.ui.toggle_play_pause(); }
        }
    }

    pub fn next_frame(&mut self) {
        let bgl = self.ui_renderer.texture_bind_group_layout();
        let sampler = self.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.video_player {
            player.step_forward(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if self.ui.is_playing() { self.ui.toggle_play_pause(); }
        }
    }

    pub fn seek_absolute(&mut self, frame: i64) {
        if let Some(player) = &mut self.video_player {
            let current = player.current_frame();
            let delta = (frame - current) as i32;
            player.seek_frame_instant(delta);
            self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
        }
        self.last_scroll_time = Some(Instant::now());
        self.scroll_needs_decode = true;
    }

    pub fn seek_relative(&mut self, delta: i32) {
        if let Some(player) = &mut self.video_player {
            player.seek_frame_instant(delta);
            self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
        }
        self.last_scroll_time = Some(Instant::now());
        self.scroll_needs_decode = true;
    }

    pub fn seek_to_next_boucle(&mut self, direction: i32) {
        let current = self.current_frame();
        let boucle_frames: Vec<i64> = self.project.markers.iter()
            .filter(|m| m.kind == crate::rythmo_line::MarkerKind::Boucle)
            .map(|m| m.frame)
            .collect();
        if boucle_frames.is_empty() { return; }

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

    fn tick_scroll_decode(&mut self) {
        if !self.scroll_needs_decode { return; }
        if let Some(t) = self.last_scroll_time {
            if t.elapsed().as_millis() >= constants::SCROLL_DECODE_DELAY_MS {
                self.scroll_needs_decode = false;
                let bgl = self.ui_renderer.texture_bind_group_layout();
                let sampler = self.ui_renderer.texture_sampler();
                if let Some(player) = &mut self.video_player {
                    player.decode_current_frame(&self.gfx.device, &self.gfx.queue, bgl, sampler);
                }
            }
        }
    }

    // -- Network --

    pub fn tick_network(&mut self) {
        let prev_state = self.network.state;
        while let Some(msg) = self.network.try_recv() {
            match msg {
                IncomingMessage::Connected => {
                    self.network.state = ConnectionState::Connected;
                    self.ui.network_status = "Connecté".into();
                    log::info!("Connected and authenticated");
                }
                IncomingMessage::Packet(packet) => self.handle_network_packet(packet),
                IncomingMessage::Disconnected(reason) => {
                    log::info!("Disconnected: {reason}");
                    self.network.state = ConnectionState::Disconnected;
                    self.network.room_code = None;
                    self.network.role = None;
                    self.network.members.clear();
                    self.ui.network_status = "Déconnecté".into();
                }
                IncomingMessage::Error(err) => {
                    log::error!("Network error: {err}");
                    self.ui.network_status = format!("Erreur: {err}");
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
        }
    }

    fn handle_network_packet(&mut self, packet: Packet) {
        match packet {
            Packet::RoomCreated { code } => {
                self.network.state = ConnectionState::InRoom;
                self.network.room_code = Some(code.clone());
                self.network.role = Some("admin".into());
                self.ui.network_status = format!("Salon créé — Code: {code}");
                self.ui.toasts.push(format!("{}{code}", crate::i18n::t("toast.room_created")), 5.0);
                log::info!("Room created: {code}");
            }
            Packet::RoomJoined { code, role, members } => {
                self.network.state = ConnectionState::InRoom;
                self.network.room_code = Some(code.clone());
                self.network.role = Some(role);
                self.network.members = members;
                self.ui.network_status = format!("Connecté au salon {code}");
                self.ui.toasts.push(format!("{}{code}", crate::i18n::t("toast.room_joined")), 5.0);
                self.ui.sync_overlay = Some("Synchronisation en cours...".into());
                self.ui.sync_progress = 0.0;
                // request_sync is sent directly from the room_joined callback
            }
            Packet::JoinError { reason } => {
                log::error!("Join failed: {reason}");
                self.ui.network_status = format!("Échec: {reason}");
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
                if let Some(code) = &self.network.room_code {
                    self.ui.network_status = format!("Salon {code} — synchronisé");
                }
            }
            Packet::RequestSync => {
                // Handled via SyncRequested with requester id
            }
            Packet::Error { message } => {
                log::error!("Server error: {message}");
                self.ui.network_status = format!("Erreur: {message}");
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
            CommandPayload::MoveLine { line_id, start_frame, y_slot } => {
                log::debug!("Remote: move line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.start_frame = start_frame;
                    l.y_slot = y_slot;
                }
            }
            CommandPayload::ResizeLine { line_id, start_frame, duration_frames } => {
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
            CommandPayload::SetCharacter { line_id, name, color } => {
                log::debug!("Remote: set character for line {}", line_id);
                self.project.set_character(line_id, name, color);
            }
            CommandPayload::SetCharacterColor { line_id, color } => {
                log::debug!("Remote: set character color for line {}", line_id);
                if let Some(l) = self.project.get_line_mut(line_id) {
                    l.character_color = color;
                }
            }
            CommandPayload::AddMarker { kind, frame } => {
                log::debug!("Remote: add marker at frame {}", frame);
                self.project.markers.push(crate::rythmo_line::RythmoMarker { kind, frame });
            }
            CommandPayload::RemoveMarker { kind, frame } => {
                log::debug!("Remote: remove marker at frame {}", frame);
                self.project.markers.retain(|m| !(m.kind == kind && m.frame == frame));
            }
            CommandPayload::MoveMarker { kind, old_frame, new_frame } => {
                log::debug!("Remote: move marker from frame {} to {}", old_frame, new_frame);
                for m in &mut self.project.markers {
                    if m.kind == kind && m.frame == old_frame {
                        m.frame = new_frame;
                        break;
                    }
                }
            }
            CommandPayload::LoadVideo { .. } => {
                log::debug!("Remote: load video (ignored, using chunked transfer)");
                // Video transfer now uses chunked video_start/chunk/end events
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
            } else {
                self.project.insert_line(remote_line);
            }
        }

        // Merge markers (replace — markers don't have stable IDs)
        self.project.markers = data.markers;

        // Merge known characters
        self.project.known_characters = data.known_characters.into_iter()
            .map(|c| crate::project::Character { name: c.name, color: c.color })
            .collect();

        log::info!("Project synced (merged)");
    }

    fn apply_delta(&mut self, data: serde_json::Value) {
        log::debug!("Applying delta: {}", data["action"].as_str().unwrap_or("unknown"));
        let action = data["action"].as_str().unwrap_or("");
        match action {
            "create_line" => {
                if let Ok(line) = serde_json::from_value::<crate::rythmo_line::RythmoLine>(data["line"].clone()) {
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
            "set_character" => {
                if let (Some(id), Some(name)) = (data["line_id"].as_u64(), data["name"].as_str()) {
                    let color = parse_color(&data["color"]);
                    self.project.set_character(id, name.to_string(), color);
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
            "add_marker" => {
                if let (Some(frame), Ok(kind)) = (
                    data["frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    self.project.markers.push(crate::rythmo_line::RythmoMarker { kind, frame });
                }
            }
            "remove_marker" => {
                if let (Some(frame), Ok(kind)) = (
                    data["frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    self.project.markers.retain(|m| !(m.kind == kind && m.frame == frame));
                }
            }
            "move_marker" => {
                if let (Some(old_frame), Some(new_frame), Ok(kind)) = (
                    data["old_frame"].as_i64(),
                    data["new_frame"].as_i64(),
                    serde_json::from_value::<crate::rythmo_line::MarkerKind>(data["kind"].clone()),
                ) {
                    for m in &mut self.project.markers {
                        if m.kind == kind && m.frame == old_frame {
                            m.frame = new_frame;
                            break;
                        }
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
        if !self.network.is_in_room() { return; }
        let payload = match cmd {
            Command::CreateLine { line_id } => {
                let line = self.project.get_line(*line_id).cloned();
                if let Some(l) = line {
                    serde_json::json!({ "action": "create_line", "line": serde_json::to_value(&l).unwrap_or_default() })
                } else { return; }
            }
            Command::DeleteLine { snapshot, .. } => {
                serde_json::json!({ "action": "delete_line", "line_id": snapshot.id })
            }
            Command::MoveLine { line_id, new_start, new_y_slot, .. } => {
                serde_json::json!({ "action": "move_line", "line_id": line_id, "start_frame": new_start, "y_slot": new_y_slot })
            }
            Command::ResizeLine { line_id, new_start, new_dur, .. } => {
                serde_json::json!({ "action": "resize_line", "line_id": line_id, "start_frame": new_start, "duration_frames": new_dur })
            }
            Command::UpdateLineText { line_id, new_text, .. } => {
                serde_json::json!({ "action": "update_text", "line_id": line_id, "text": new_text })
            }
            Command::SetCharacter { line_id, new_name, new_color, .. } => {
                serde_json::json!({ "action": "set_character", "line_id": line_id, "name": new_name, "color": new_color })
            }
            Command::SetCharacterColor { line_id, new_color, .. } => {
                serde_json::json!({ "action": "set_character_color", "line_id": line_id, "color": new_color })
            }
            Command::AddMarker { index } => {
                if let Some(m) = self.project.markers.get(*index) {
                    serde_json::json!({ "action": "add_marker", "kind": serde_json::to_value(&m.kind).unwrap_or_default(), "frame": m.frame })
                } else { return; }
            }
            Command::RemoveMarker { marker, .. } => {
                serde_json::json!({ "action": "remove_marker", "kind": serde_json::to_value(&marker.kind).unwrap_or_default(), "frame": marker.frame })
            }
            Command::MoveMarker { index, old_frame, new_frame } => {
                if let Some(m) = self.project.markers.get(*index) {
                    serde_json::json!({ "action": "move_marker", "kind": serde_json::to_value(&m.kind).unwrap_or_default(), "old_frame": old_frame, "new_frame": new_frame })
                } else { return; }
            }
        };
        self.network.send_raw("delta", payload);
    }

    /// Broadcast coalesced final state on mouse release / StopEditing.
    pub fn broadcast_finalize(&self) {
        if !self.network.is_in_room() { return; }
        if let Some(cmd) = self.history.last() {
            if matches!(cmd,
                Command::MoveLine { .. } | Command::ResizeLine { .. } |
                Command::UpdateLineText { .. } | Command::SetCharacter { .. } |
                Command::SetCharacterColor { .. } | Command::MoveMarker { .. }
            ) {
                self.broadcast_delta(cmd);
            }
        }
    }

    /// Broadcast full project state (only for undo/redo/join sync).
    fn broadcast_full_sync(&self) {
        if !self.network.is_in_room() { return; }
        let data = ProjectData::from_project(&self.project);
        self.network.send_raw("sync", serde_json::json!({ "project": data }));
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
        self.ui.rythmo_state.syllable_mode = false;

        // Save current fullscreen state and enter fullscreen
        self.fullscreen_before_studio = self.window.fullscreen();
        self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
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

    // -- Project / Lines (all via Command pattern) --

    pub fn open_toolbar_dropdown(&mut self, dropdown: crate::ui::widget::ToolbarDropdown) {
        self.ui.toggle_toolbar_dropdown(dropdown);
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
                    if idx < self.project.markers.len() {
                        let marker = self.project.markers.remove(idx);
                        self.push_and_broadcast(Command::RemoveMarker { marker, index: idx });
                    }
                }
            }
            self.ui.clear_selection();
        }
    }

    pub fn move_marker(&mut self, index: usize, frame: i64) {
        if index >= self.project.markers.len() { return; }
        let old_frame = self.project.markers[index].frame;
        self.push_and_broadcast(Command::MoveMarker { index, old_frame, new_frame: frame });
        self.project.markers[index].frame = frame;
        self.dirty = true;
    }

    pub fn add_marker(&mut self, kind: crate::rythmo_line::MarkerKind) {
        let frame = self.current_frame();
        self.project.markers.push(crate::rythmo_line::RythmoMarker { kind, frame });
        self.push_and_broadcast(Command::AddMarker { index: self.project.markers.len() - 1 });
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
        let dur = (self.fps() * constants::DEFAULT_LINE_DURATION_SEC) as i64;
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
                if let Command::MoveLine { new_start, new_y_slot, .. } = cmd {
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
                line_id: id, old_start, old_y_slot: old_y, new_start: start_frame, new_y_slot: y_slot,
            });
        }
    }

    pub fn resize_line(&mut self, id: u64, start_frame: i64, duration_frames: i64) {
        if self.history.last_matches(id, CommandKind::ResizeLine) {
            if let Some(l) = self.project.get_line_mut(id) {
                l.start_frame = start_frame;
                l.duration_frames = duration_frames;
            }
            self.history.update_last(|cmd| {
                if let Command::ResizeLine { new_start, new_dur, .. } = cmd {
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
                line_id: id, old_start, old_dur, new_start: start_frame, new_dur: duration_frames,
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
            let old_text = self.project.get_line(id).map(|l| l.text.clone()).unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(id) {
                l.text = text.clone();
            }
            self.push_and_broadcast(Command::UpdateLineText { line_id: id, old_text, new_text: text });
        }
    }

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        let (old_name, old_color) = self.project.get_line(line_id)
            .map(|l| (l.character_name.clone(), l.character_color))
            .unwrap_or_default();
        self.project.set_character(line_id, name.clone(), color);
        self.push_and_broadcast(Command::SetCharacter {
            line_id, old_name, old_color, new_name: name, new_color: color,
        });
    }

    pub fn set_character_color(&mut self, line_id: u64, color: [f32; 4]) {
        if self.history.last_matches(line_id, CommandKind::SetCharacterColor) {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.history.update_last(|cmd| {
                if let Command::SetCharacterColor { new_color, .. } = cmd {
                    *new_color = color;
                }
            });
        } else {
            let old_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_color = color;
            }
            self.push_and_broadcast(Command::SetCharacterColor { line_id, old_color, new_color: color });
        }
    }

    pub fn update_character_name(&mut self, line_id: u64, name: String) {
        let known_color = self.project.known_characters.iter()
            .find(|c| c.name == name)
            .map(|c| c.color);

        // Coalesce character name edits
        if self.history.last_matches(line_id, CommandKind::SetCharacter) {
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color { l.character_color = c; }
            }
            let final_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            self.history.update_last(|cmd| {
                if let Command::SetCharacter { new_name, new_color, .. } = cmd {
                    *new_name = name;
                    *new_color = final_color;
                }
            });
        } else {
            let (old_name, old_color) = self.project.get_line(line_id)
                .map(|l| (l.character_name.clone(), l.character_color))
                .unwrap_or_default();
            if let Some(l) = self.project.get_line_mut(line_id) {
                l.character_name = name.clone();
                if let Some(c) = known_color { l.character_color = c; }
            }
            let final_color = self.project.get_line(line_id).map(|l| l.character_color).unwrap_or_default();
            self.push_and_broadcast(Command::SetCharacter {
                line_id, old_name, old_color, new_name: name, new_color: final_color,
            });
        }
    }

    pub fn finalize_character(&mut self, line_id: u64) {
        let (name, color) = match self.project.get_line(line_id) {
            Some(l) if !l.character_name.is_empty() => (l.character_name.clone(), l.character_color),
            _ => return,
        };
        self.project.set_character(line_id, name, color);
    }

    // -- Backup --

    fn backup_path() -> std::path::PathBuf {
        std::env::current_exe()
            .map(|p| p.parent().unwrap_or(std::path::Path::new(".")).join("br_backup.json"))
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

    pub fn render(&mut self) {
        // Poll ping results
        if let Ok(mut results) = self.ping_results.try_lock() {
            for r in results.drain(..) {
                if let Some(browser) = self.ui.server_browser_mut() {
                    if r.success {
                        browser.update_server_info(&r.ip, r.port, r.name, r.motd, r.online, r.max_slots);
                    } else {
                        browser.mark_offline(&r.ip, r.port);
                    }
                }
            }
        }

        // Auto-save every 60 seconds if project is dirty
        if self.dirty && self.last_autosave.elapsed().as_secs() >= 60 {
            self.save_backup();
            self.last_autosave = Instant::now();
        }

        if self.studio_mode {
            self.render_studio();
            return;
        }

        let surface_texture = match self.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.gfx.surface.configure(&self.gfx.device, &self.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gfx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Clear
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None, timestamp_writes: None,
                occlusion_query_set: None, multiview_mask: None,
            });
        }

        // Drain timeline events
        let _events = self.timeline.drain();

        // Check if export finished
        if let Some(progress) = &self.ui.export_progress {
            use std::sync::atomic::Ordering;
            let v = f32::from_bits(progress.load(Ordering::Relaxed));
            if v >= 1.5 { // sentinel: 2.0 means done
                self.ui.export_progress = None;
                log::info!("Export completed");
            }
        }

        // Network tick
        self.tick_network();

        // Debounced scroll decode
        self.tick_scroll_decode();

        // Video tick — emit FrameChanged so observers (rythmo) stay in sync
        if let Some(player) = &mut self.video_player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.ui_renderer.texture_bind_group_layout(),
                self.ui_renderer.texture_sampler(),
            );
            player.tick(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if player.current_frame() != prev_frame {
                self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
            }
            if !player.is_playing() && self.ui.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui.toggle_play_pause();
            }
        }

        // Video quad — skip when export modal is showing (it would cover the modal)
        let video_quad = if self.ui.export_progress.is_some() {
            None
        } else {
            build_video_quad(&self.video_player, &self.ui)
        };
        let current_frame = self.current_frame();

        // UI render
        let waveform = self.waveform();
        self.ui.render(
            &mut self.ui_renderer,
            &self.gfx.device, &self.gfx.queue, &mut encoder, &view,
            self.gfx.config.width, self.gfx.config.height,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project, current_frame,
            &waveform,
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }

    fn render_studio(&mut self) {
        let surface_texture = match self.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.gfx.surface.configure(&self.gfx.device, &self.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.gfx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Studio Render Encoder"),
        });

        // Clear to black
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Studio Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None, timestamp_writes: None,
                occlusion_query_set: None, multiview_mask: None,
            });
        }

        // Drain timeline, tick video, tick scroll decode
        let _events = self.timeline.drain();
        self.tick_scroll_decode();

        if let Some(player) = &mut self.video_player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.ui_renderer.texture_bind_group_layout(),
                self.ui_renderer.texture_sampler(),
            );
            player.tick(&self.gfx.device, &self.gfx.queue, bgl, sampler);
            if player.current_frame() != prev_frame {
                self.timeline.emit(TimelineEvent::FrameChanged { frame: player.current_frame() });
            }
            if !player.is_playing() && self.ui.is_playing() {
                self.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui.toggle_play_pause();
            }
        }

        let rythmo_h = crate::ui::rythmo::studio_br_height(&self.project, self.ui.screen_w());
        let video_quad = build_studio_video_quad(&self.video_player, &self.ui, rythmo_h);
        // Use interpolated frame for smooth playback in studio mode (handles low-fps source video)
        let current_frame = self.video_player.as_ref().map_or(0, |p| p.current_frame_interpolated());

        self.ui.render_studio(
            &mut self.ui_renderer,
            &self.gfx.device, &self.gfx.queue, &mut encoder, &view,
            self.gfx.config.width, self.gfx.config.height,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project, current_frame,
        );

        self.gfx.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
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
            rect: [preview.x + (preview.width - draw_w) / 2.0, preview.y + (preview.height - draw_h) / 2.0, draw_w, draw_h],
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
            rect: [(screen_w - draw_w) / 2.0, (video_zone_h - draw_h) / 2.0, draw_w, draw_h],
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
        },
    ))
}

fn ping_server_socketio(ip: &str, port: u16, results: std::sync::Arc<std::sync::Mutex<Vec<PingResult>>>) {
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
                            ip: ip_for_info.clone(), port: port_clone,
                            name, motd, online, max_slots, success: true,
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
                        ip: ip_for_disc.clone(), port: port_clone,
                        name: String::new(), motd: String::new(),
                        online: 0, max_slots: 0, success: false,
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
                if done.load(std::sync::atomic::Ordering::Relaxed) { break; }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // If still no response after timeout, mark offline
            if !done.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(mut r) = results.lock() {
                    r.push(PingResult {
                        ip: ip_clone, port, name: String::new(), motd: String::new(),
                        online: 0, max_slots: 0, success: false,
                    });
                }
            }
        }
        Err(_) => {
            if let Ok(mut r) = results.lock() {
                r.push(PingResult {
                    ip: ip_clone, port, name: String::new(), motd: String::new(),
                    online: 0, max_slots: 0, success: false,
                });
            }
        }
    }
}
