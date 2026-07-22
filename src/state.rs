use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use wgpu::CurrentSurfaceTexture;
use winit::window::{Window, WindowId};

use std::time::{Duration, Instant};

use crate::accessibility::{AccessibilityEvent, NarrationService};
use crate::application::collaboration_service::{CollaborationSession, PingResult};
use crate::application::context::AppContext;
use crate::application::delta_codec::{decode_delta, encode_delta};
use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::application::job_service::{
    JobManager, PendingExportJob, PendingImportJob, PendingProxyJob, PendingSaveJob,
    SaveContinuation,
};
use crate::application::playback_service::PlaybackSession;
use crate::application::project_service::ProjectSession;
use crate::application::render_service::RenderCoordinator;
use crate::application::ui_shell::UiShell;
use crate::application::window_service::WindowManager;
use crate::application::workspace_service::{WorkspaceHost, WorkspaceId};
use crate::command::{Command, CommandKind, LineMove};
use crate::network::{ConnectionState, IncomingMessage};
use crate::observer::TimelineEvent;
use crate::packet::{CommandPayload, Packet, ProjectData};
use crate::project::{Character, LineCharacterNameChange, Project};
use crate::rythmo_line::RythmoLine;
use crate::ui::primitives::{EventResponse, UiEvent};
use crate::ui::Ui;
use crate::video::{AudioTrack, VideoPlayer};
use crate::voice_actor::{LineVoiceActorsChange, VoiceActor};
use crate::workspaces::recording::RecordingWorkspace;
use crate::workspaces::rythmo::RythmoWorkspace;

use crate::constants;

enum DialogueSplitTarget {
    Cursor { line_id: u64, cursor_pos: usize },
    Playhead { line_id: u64, progress: f32 },
}

fn rebase_pasted_start_frame(source_start: i64, source_anchor: i64, target_anchor: i64) -> i64 {
    target_anchor.saturating_add(source_start.saturating_sub(source_anchor))
}

#[derive(Clone)]
struct LineClipboardEntry {
    line: RythmoLine,
    detections: Option<crate::detection::LineDetectionData>,
}

#[cfg(test)]
mod clipboard_tests {
    use super::rebase_pasted_start_frame;
    use crate::detection::{DetectionDocument, MediaTick};

    #[test]
    fn pasted_lines_are_rebased_to_the_playhead_and_keep_their_spacing() {
        let source_anchor = 120;
        let playhead = 900;

        assert_eq!(
            rebase_pasted_start_frame(source_anchor, source_anchor, playhead),
            playhead
        );
        assert_eq!(
            rebase_pasted_start_frame(165, source_anchor, playhead),
            playhead + 45
        );
    }

    #[test]
    fn pasted_sync_points_follow_the_line_timeline_offset() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_sync_point(
                42,
                4,
                MediaTick::from_frame(100),
                MediaTick::from_frame(140),
                2,
                MediaTick::from_frame(120),
            )
            .unwrap();
        let mut copied = document.line(42).unwrap().clone();

        copied.shift_sync_points(MediaTick::from_frame(300));

        assert_eq!(
            copied
                .sync_point(crate::detection::SyncPointId(address.detection_id.0))
                .unwrap()
                .line_tick,
            MediaTick::from_frame(420)
        );
    }
}

pub struct State {
    pub render: RenderCoordinator,
    pub window_manager: WindowManager,
    ui_scale: f32,
    pub ui_shell: UiShell,
    pub playback: PlaybackSession,
    pub collaboration: CollaborationSession,
    pub jobs: JobManager,
    pub project_session: ProjectSession,
    pub recording_runtime: crate::recording_runtime::RecordingRuntime,
    pub workspace_host: WorkspaceHost,
    pub narration: NarrationService,
    last_autosave: Instant,
    line_clipboard: Option<Vec<LineClipboardEntry>>,
    automation_last_run: Option<(u64, u64)>,
    last_progress_percent: Option<u32>,
    last_progress_announcement: Option<Instant>,
}

impl State {
    pub async fn new(
        window: Arc<Window>,
        accessibility_sender: Option<std::sync::mpsc::Sender<AccessibilityEvent>>,
    ) -> Self {
        let render = RenderCoordinator::new(window.clone()).await;
        let ui_scale = Self::window_ui_scale(&window);
        let (ui_width, ui_height) = Self::logical_ui_size(render.gfx.size, ui_scale);
        let ui = Ui::new(ui_width, ui_height, &render.ui_renderer.icon_atlas);

        Self {
            render,
            window_manager: WindowManager::new(window),
            ui_scale,
            ui_shell: UiShell::new(ui),
            playback: PlaybackSession::new(),
            collaboration: CollaborationSession::new(),
            jobs: JobManager::new(),
            project_session: ProjectSession::new(),
            recording_runtime: crate::recording_runtime::RecordingRuntime::new(),
            workspace_host: WorkspaceHost::new(
                vec![
                    Box::new(RythmoWorkspace::new()),
                    Box::new(RecordingWorkspace::new()),
                ],
                WorkspaceId::Rythmo,
            ),
            narration: NarrationService::new(
                crate::config::get().accessibility.screen_reader_enabled,
                accessibility_sender,
            ),
            last_autosave: Instant::now(),
            line_clipboard: None,
            automation_last_run: None,
            last_progress_percent: None,
            last_progress_announcement: None,
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

    pub fn app_context(&self) -> AppContext<'_> {
        AppContext {
            project: &self.project_session,
            playback: &self.playback,
            collaboration: &self.collaboration,
        }
    }

    fn renderer_refs(&self) -> (&wgpu::BindGroupLayout, &wgpu::Sampler) {
        (
            self.render.ui_renderer.texture_bind_group_layout(),
            self.render.ui_renderer.texture_sampler(),
        )
    }

    // -- Public API --

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.render.gfx.resize(new_size);
        self.ui_scale = Self::window_ui_scale(&self.window_manager.main_window);
        let (ui_width, ui_height) = Self::logical_ui_size(new_size, self.ui_scale);
        self.ui_shell.ui.resize(ui_width, ui_height);
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
    }

    pub fn window_to_ui_position(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.ui_scale, y / self.ui_scale)
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) -> EventResponse {
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
        let render_frame = self.render_frame();
        let fps = self.fps();
        self.project_session
            .render_index
            .refresh(&self.project_session.project);
        self.ui_shell.ui.handle_event(
            event,
            &self.project_session.project,
            &self.project_session.render_index,
            render_frame,
            fps,
        )
    }

    pub fn active_workspace(&self) -> WorkspaceId {
        self.workspace_host.active_id()
    }

    pub fn activate_workspace(&mut self, workspace: WorkspaceId) {
        let changed = self.workspace_host.activate(workspace);
        if changed || self.ui_shell.ui.active_workspace() != workspace {
            self.ui_shell.ui.set_active_workspace(workspace);
        }
        let label = match workspace {
            WorkspaceId::Rythmo => crate::i18n::t("workspace_tabs.rythmo"),
            WorkspaceId::Recording => crate::i18n::t("workspace_tabs.recording"),
        };
        self.announce_accessibility(AccessibilityEvent::Selection {
            label: format!(
                "{}: {label}",
                crate::i18n::t("accessibility.workspace_selected")
            ),
        });
    }

    fn recording_network_role(&self) -> crate::ui::recording_workspace::RecordingRole {
        use crate::ui::recording_workspace::RecordingRole;

        let network = &self.collaboration.network;
        let Some(member_id) = network.member_id.as_deref() else {
            return RecordingRole::Actor;
        };
        let role = network
            .member_details
            .iter()
            .find(|member| member.id == member_id)
            .map(|member| member.role.as_str())
            .or(network.role.as_deref())
            .unwrap_or("actor");
        match role {
            "admin" => RecordingRole::Director,
            "co_da" => RecordingRole::CoDirector {
                has_control: network.control_owner_id.as_deref() == Some(member_id),
            },
            _ => RecordingRole::Actor,
        }
    }

    pub fn sync_recording_workspace_ui(&mut self) {
        let current_frame = self.current_frame();
        let capture = self.recording_runtime.capture_state();
        self.ui_shell.ui.sync_recording_scene(
            &self.project_session.recording_project,
            capture,
            &self.collaboration.network.member_details,
            self.collaboration.network.control_owner_id.as_deref(),
            current_frame,
        );
    }

    pub fn recording_choose_solo(&mut self) {
        self.ui_shell.ui.recording_enter_solo();
        self.sync_recording_workspace_ui();
        self.announce_accessibility(AccessibilityEvent::Selection {
            label: crate::i18n::t("recording.choice.solo").to_string(),
        });
    }

    pub fn recording_choose_online(&mut self) {
        if !self.collaboration.network.is_in_room() {
            self.open_server_browser();
            return;
        }
        let role = self.recording_network_role();
        self.ui_shell.ui.recording_enter_online(role);
        self.sync_recording_workspace_ui();
        self.announce_accessibility(AccessibilityEvent::Selection {
            label: match role {
                crate::ui::recording_workspace::RecordingRole::Director => {
                    crate::i18n::t("recording.role.director")
                }
                crate::ui::recording_workspace::RecordingRole::CoDirector { .. } => {
                    crate::i18n::t("recording.role.co_director")
                }
                crate::ui::recording_workspace::RecordingRole::Actor => {
                    crate::i18n::t("recording.role.actor")
                }
                crate::ui::recording_workspace::RecordingRole::Solo => {
                    crate::i18n::t("recording.choice.solo")
                }
            }
            .to_string(),
        });
    }

    pub fn recording_set_tool(&mut self, tool: crate::recording::RecordingTool) {
        if self.ui_shell.ui.recording_can_edit_timeline() {
            self.ui_shell.ui.recording_set_tool(tool);
            self.sync_recording_workspace_ui();
        } else {
            self.recording_read_only_error();
        }
    }

    fn recording_read_only_error(&mut self) {
        let message = crate::i18n::t("recording.read_only");
        self.show_toast(message, 3.0);
        self.announce_accessibility(AccessibilityEvent::Error {
            message: message.to_string(),
        });
    }

    pub fn apply_recording_operation(
        &mut self,
        operation: crate::recording::RecordingOperation,
    ) -> Result<(), crate::recording::RecordingError> {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return Ok(());
        }
        let transaction = self
            .project_session
            .recording_transactions
            .append_and_apply(&mut self.project_session.recording_project, operation)?
            .clone();
        self.project_session.mark_recording_changed();
        if self.ui_shell.ui.recording_role().is_online() {
            self.collaboration
                .network
                .send_recording_transaction(&transaction);
        }
        self.sync_recording_workspace_ui();
        Ok(())
    }

    pub fn recording_toggle_track_mute(&mut self, track_id: crate::recording::AudioTrackId) {
        let Some(muted) = self
            .project_session
            .recording_project
            .track(track_id)
            .map(|track| track.muted)
        else {
            return;
        };
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::SetTrackMuted {
                track_id,
                muted: !muted,
            })
        {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_toggle_track_solo(&mut self, track_id: crate::recording::AudioTrackId) {
        let Some(solo) = self
            .project_session
            .recording_project
            .track(track_id)
            .map(|track| track.solo)
        else {
            return;
        };
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::SetTrackSolo {
                track_id,
                solo: !solo,
            })
        {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_arm_track(&mut self, track_id: crate::recording::AudioTrackId) {
        let armed = self.project_session.recording_project.armed_track_id();
        let operation = crate::recording::RecordingOperation::ArmTrack {
            track_id: (armed != Some(track_id)).then_some(track_id),
        };
        if let Err(error) = self.apply_recording_operation(operation) {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_select_clip(
        &mut self,
        clip_id: crate::recording::AudioClipId,
        additive: bool,
    ) {
        if let Err(error) = self.ui_shell.ui.recording_select_clip(
            &self.project_session.recording_project,
            clip_id,
            additive,
        ) {
            self.recording_error(error.to_string());
        } else {
            self.sync_recording_workspace_ui();
        }
    }

    pub fn recording_select_asset(&mut self, asset_id: crate::recording::AudioAssetId) {
        if self
            .project_session
            .recording_project
            .asset(asset_id)
            .is_some()
        {
            self.ui_shell.ui.recording_select_asset(asset_id);
            self.sync_recording_workspace_ui();
        }
    }

    pub fn recording_start_capture(&mut self) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let result = self.recording_runtime.begin_capture(
            &self.project_session.recording_project,
            self.current_frame(),
        );
        if let Err(error) = result {
            self.recording_error(error.to_string());
            return;
        }

        if self.ui_shell.ui.recording_role().is_online() {
            let capture_target = match self.recording_runtime.capture_state() {
                Some(crate::recording::CaptureState::Countdown { target, .. }) => Some(*target),
                _ => None,
            };
            self.collaboration.network.send_recording_prepare(
                &crate::network::RecordingPreparePayload {
                    project: self.project_session.recording_project.clone(),
                    transactions: self.project_session.recording_transactions.clone(),
                    current_frame: self.current_frame(),
                    capture_target,
                },
            );
        }
        self.sync_recording_workspace_ui();
        self.announce_accessibility(AccessibilityEvent::Activation {
            label: crate::i18n::t("recording.capture.countdown").to_string(),
        });
    }

    pub fn recording_stop_capture(&mut self) {
        match self.recording_runtime.cancel_or_stop() {
            Ok(crate::recording_runtime::RecordingRuntimeEvent::Cancelled) => {
                self.show_toast(crate::i18n::t("recording.capture.cancelled"), 3.0);
            }
            Ok(crate::recording_runtime::RecordingRuntimeEvent::Finalizing { .. }) => {
                if self
                    .playback
                    .video_player
                    .as_ref()
                    .is_some_and(|player| player.is_playing())
                {
                    self.toggle_play_pause();
                }
                self.announce_accessibility(AccessibilityEvent::Activation {
                    label: crate::i18n::t("recording.capture.finalizing").to_string(),
                });
            }
            Ok(_) => {}
            Err(error) => self.recording_error(error.to_string()),
        }
        self.sync_recording_workspace_ui();
    }

    fn recording_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        let label = crate::i18n::t("recording.capture.error").replace("{error}", &message);
        self.show_toast(label.clone(), 7.0);
        self.announce_accessibility(AccessibilityEvent::Error { message: label });
    }

    pub fn is_rythmo_text_editing(&self) -> bool {
        self.ui_shell.ui.rythmo_state.is_editing()
    }

    pub fn side_panel_open(&self) -> bool {
        self.ui_shell.ui.side_panel_open()
    }

    pub fn captures_modal_input(&self) -> bool {
        self.ui_shell.ui.modal_host.captures_input()
    }

    pub fn is_proxy_modal_open(&self) -> bool {
        self.ui_shell.ui.modal_host.proxy.is_some()
    }

    pub fn is_save_prompt_open(&self) -> bool {
        self.ui_shell.ui.modal_host.save_prompt.is_some()
    }

    pub fn proxy_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .proxy
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn settings_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .settings
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn project_settings_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .project_settings
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn export_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .export
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn language_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .languages
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn rename_character_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .modal_host
            .rename_character
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
    }

    pub fn toolbar_dropdown_first_accessibility_label(
        &self,
        dropdown: &crate::ui::primitives::ToolbarDropdown,
    ) -> Option<String> {
        if !self.ui_shell.ui.toolbar_dropdown_is_open(dropdown) {
            return None;
        }
        Some(
            crate::i18n::t(match dropdown {
                crate::ui::primitives::ToolbarDropdown::Respirations => "resp.up",
                crate::ui::primitives::ToolbarDropdown::Reactions => "react.x",
            })
            .to_string(),
        )
    }

    fn announce_open_container(&self, title: &str, first_label: String) {
        self.announce_accessibility(AccessibilityEvent::Activation {
            label: format!("{title} : {first_label}"),
        });
    }

    pub fn set_export_progress(&mut self, p: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>) {
        let is_none = p.is_none();
        self.ui_shell.ui.export_progress = p;
        self.last_progress_percent = None;
        self.last_progress_announcement = if is_none { None } else { Some(Instant::now()) };
        if is_none {
            self.narration.publish_progress(String::new(), None);
            self.ui_shell.ui.export_render_backend = None;
            self.ui_shell.ui.progress_prefix = String::new();
            self.jobs.active_export_cancel = None;
        }
    }

    fn active_progress_label(&self) -> String {
        if self.jobs.pending_proxy_job.is_some() {
            crate::i18n::t("progress.proxy").to_string()
        } else if self.ui_shell.ui.progress_prefix.is_empty() {
            crate::i18n::t("progress.exporting").to_string()
        } else {
            self.ui_shell.ui.progress_prefix.clone()
        }
    }

    pub fn is_project_save_in_progress(&self) -> bool {
        self.jobs.pending_save_job.is_some()
    }

    pub(crate) fn take_transition_after_save_ready(&mut self) -> Option<SaveContinuation> {
        self.jobs.transition_after_save_ready.take()
    }

    pub(crate) fn start_project_save(
        &mut self,
        path: PathBuf,
        source_video: PathBuf,
        proxy_video: Option<PathBuf>,
        font_asset: PathBuf,
        continuation: SaveContinuation,
    ) -> bool {
        if self.jobs.pending_save_job.is_some() {
            self.show_toast(crate::i18n::t("toast.save_already_running"), 4.0);
            return false;
        }

        let project = self.project_session.project.snapshot();
        let saved_revision = project.revision();
        let saved_recording_revision = self.project_session.recording_revision;
        let transaction_journal = self.project_session.transaction_journal.clone();
        let recording_project = self.project_session.recording_project.clone();
        let recording_transactions = self.project_session.recording_transactions.clone();
        let recording_asset_paths = self.project_session.recording_asset_paths.clone();
        let fps = self.fps();
        let worker_path = path.clone();
        let worker_source = source_video.clone();
        let worker_proxy = proxy_video.clone();
        let worker_font = font_asset.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let recording_assets: Vec<_> = recording_asset_paths
                .iter()
                .map(
                    |(asset_id, path)| crate::project_archive::RecordingAssetInput {
                        asset_id: *asset_id,
                        path: path.as_path(),
                    },
                )
                .collect();
            let result = crate::project_archive::save_bundle_with_recording_data(
                &project,
                fps,
                &worker_path,
                &worker_source,
                worker_proxy.as_deref(),
                Some(&worker_font),
                Some(&transaction_journal),
                Some(crate::project_archive::RecordingBundleInput {
                    project: &recording_project,
                    transaction_log: &recording_transactions,
                    assets: &recording_assets,
                }),
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });

        self.jobs.pending_save_job = Some(PendingSaveJob {
            path,
            saved_revision,
            saved_recording_revision,
            source_video,
            proxy_video,
            font_asset,
            continuation,
            receiver,
        });
        self.show_toast(crate::i18n::t("toast.save_started"), 5.0);
        true
    }

    pub fn set_export_cancel(&mut self, cancel: Option<Arc<AtomicBool>>) {
        self.jobs.active_export_cancel = cancel;
    }

    pub fn cancel_export(&mut self) {
        if let Some(cancel) = &self.jobs.active_export_cancel {
            cancel.store(true, Ordering::Relaxed);
            self.ui_shell.ui.progress_prefix = crate::i18n::t("progress.canceling").to_string();
            self.announce_shortcut_accessibility(AccessibilityEvent::Activation {
                label: crate::i18n::t("progress.canceling").to_string(),
            });
        }
    }

    pub fn set_export_render_backend(
        &mut self,
        status: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    ) {
        self.ui_shell.ui.export_render_backend = status;
    }

    pub fn set_progress_label(&mut self, label: &str) {
        self.ui_shell.ui.progress_prefix = label.to_string();
    }

    pub fn set_ctrl_held(&mut self, held: bool) {
        let was_held = self.ui_shell.ui.rythmo_state.ctrl_held;
        self.ui_shell.ui.rythmo_state.ctrl_held = held;
        if !held {
            self.ui_shell.ui.rythmo_state.ghost_preview = None;
            if was_held {
                self.narration.flush_control_shortcut();
            }
        }
    }

    pub fn is_ctrl_held(&self) -> bool {
        self.ui_shell.ui.rythmo_state.ctrl_held
    }

    pub fn is_editing_text(&self) -> bool {
        self.ui_shell.ui.is_editing_text()
    }

    pub fn has_keyboard_focus(&self) -> bool {
        self.ui_shell.ui.has_keyboard_focus()
    }

    pub fn focused_workspace_tab(&self) -> bool {
        self.ui_shell.ui.focused_workspace_tab()
    }

    pub fn is_sensitive_text_context(&self) -> bool {
        self.ui_shell.ui.is_sensitive_text_context()
    }

    pub fn hovering_resize_handle(&self) -> bool {
        self.ui_shell.ui.hovering_split_handle()
    }

    pub fn dragging_resize_handle(&self) -> bool {
        self.ui_shell.ui.dragging_split_handle()
    }

    pub fn hovering_panel_resize_handle(&self) -> bool {
        self.ui_shell.ui.hovering_props_handle()
    }

    pub fn dragging_panel_resize_handle(&self) -> bool {
        self.ui_shell.ui.dragging_props_handle()
    }

    pub fn hovered_line(&self) -> Option<u64> {
        self.ui_shell.ui.rythmo_state.hovered_line
    }

    pub fn editing_line(&self) -> Option<u64> {
        self.ui_shell.ui.rythmo_state.editing_line
    }

    pub fn open_server_browser(&mut self) {
        self.ui_shell.ui.open_server_browser();
        let first = self
            .ui_shell
            .ui
            .modal_host
            .server_browser
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
            .unwrap_or_else(|| crate::i18n::t("server_browser.empty").to_string());
        self.announce_open_container(crate::i18n::t("server_browser.title"), first);
        self.ping_servers();
    }

    pub fn open_connect_modal(&mut self, ip: &str, port: u16, join: bool) {
        self.ui_shell.ui.open_connect_modal(ip, port, join);
        if let Some(first) = self
            .ui_shell
            .ui
            .modal_host
            .connect
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
        {
            self.announce_open_container(crate::i18n::t("menu.connect"), first);
        }
    }

    pub fn open_add_server_modal(&mut self) {
        self.ui_shell.ui.open_add_server_modal();
        if let Some(first) = self
            .ui_shell
            .ui
            .modal_host
            .add_server
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
        {
            self.announce_open_container(crate::i18n::t("server_browser.add_title"), first);
        }
    }

    pub fn refresh_server_browser(&mut self) {
        // Re-open browser with fresh server list
        self.ui_shell.ui.open_server_browser();
        self.ping_servers();
    }

    fn ping_servers(&mut self) {
        if let Some(browser) = self.ui_shell.ui.server_browser_mut() {
            for s in &mut browser.servers {
                s.status = crate::ui::server_browser::ServerStatus::Pinging;
            }
        }
        let servers = crate::config::saved_servers();
        // ponytail: server rejects handshakes without a valid password (auth middleware),
        // so the ping must authenticate just like a real connection ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â else every server
        // with a password shows Offline despite being up.
        let password = crate::config::get().network.password.clone();
        for s in servers {
            let ip = s.ip.clone();
            let port = s.port;
            let ping_results = self.collaboration.ping_results.clone();
            let pw = password.clone();
            std::thread::spawn(move || {
                ping_server_socketio(&ip, port, pw, ping_results);
            });
        }
    }

    pub fn open_settings_modal(&mut self) {
        let fonts = self.render.ui_renderer.enumerate_font_families();
        self.ui_shell.ui.open_settings_modal(fonts);
        if let Some(first_label) = self.settings_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("settings.title"), first_label);
        }
    }

    pub fn open_project_settings_modal(&mut self) {
        let settings = self.project_session.project.settings();
        self.ui_shell.ui.open_project_settings_modal(
            settings.instrumental_audio_path.clone(),
            settings.highlight_read_word,
            settings.scrolling_text_uses_character_color,
        );
        if let Some(first_label) = self.project_settings_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("project_settings.title"), first_label);
        }
    }

    pub fn open_automation(&mut self) {
        self.ui_shell.ui.open_automation();
    }

    pub fn close_automation(&mut self) {
        self.ui_shell.ui.close_automation();
    }

    fn update_automation_graph(
        &mut self,
        update: impl FnOnce(&mut crate::automation::AutomationGraph) -> bool,
    ) {
        let mut settings = self.project_session.project.settings().clone();
        if !update(&mut settings.automation) {
            return;
        }
        EditExecutor::apply_domain_change(
            &mut self.project_session,
            EditOrigin::Local,
            |project| project.set_settings(settings),
        );
        self.automation_last_run = None;
        if self.collaboration.network.is_in_room() {
            self.broadcast_full_sync();
        }
    }

    pub fn automation_add_node(
        &mut self,
        kind: crate::automation::AutomationNodeKind,
        x: f32,
        y: f32,
    ) {
        self.update_automation_graph(move |graph| graph.add_node(kind, x, y).is_some());
    }

    pub fn automation_add_connected_node(
        &mut self,
        kind: crate::automation::AutomationNodeKind,
        x: f32,
        y: f32,
        from_node: u64,
        edge_kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
    ) {
        self.update_automation_graph(move |graph| {
            let Some(to_node) = graph.add_node(kind, x, y) else {
                return false;
            };
            if graph.connect(crate::automation::AutomationEdge {
                from_node,
                kind: edge_kind,
                branch,
                to_node,
            }) {
                true
            } else {
                graph.delete_node(to_node);
                false
            }
        });
    }

    pub fn automation_move_node(&mut self, node_id: u64, x: f32, y: f32) {
        self.update_automation_graph(move |graph| graph.move_node(node_id, x, y));
    }

    pub fn automation_delete_node(&mut self, node_id: u64) {
        self.update_automation_graph(move |graph| graph.delete_node(node_id));
    }

    pub fn automation_connect(
        &mut self,
        from_node: u64,
        kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
        to_node: u64,
    ) {
        self.update_automation_graph(move |graph| {
            graph.connect(crate::automation::AutomationEdge {
                from_node,
                kind,
                branch,
                to_node,
            })
        });
    }

    pub fn automation_disconnect(
        &mut self,
        from_node: u64,
        kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
    ) {
        self.update_automation_graph(move |graph| graph.disconnect(from_node, kind, &branch));
    }

    pub fn automation_add_role(&mut self, node_id: u64, role: String) {
        self.update_automation_graph(move |graph| graph.add_role(node_id, role));
    }

    pub fn automation_remove_role(&mut self, node_id: u64, role: String) {
        self.update_automation_graph(move |graph| graph.remove_role(node_id, &role));
    }

    pub fn automation_set_track(&mut self, node_id: u64, track: u8) {
        self.update_automation_graph(move |graph| graph.set_track(node_id, track));
    }

    pub fn automation_set_node_enabled(&mut self, node_id: u64, enabled: bool) {
        self.update_automation_graph(move |graph| graph.set_enabled(node_id, enabled));
    }

    /// The entry node is conceptually evaluated every frame. Since the graph
    /// is deterministic, the runtime skips the walk when neither the active
    /// language nor its project revision changed.
    fn apply_automation_if_needed(&mut self) {
        let key = (
            self.project_session.project.active_language_id(),
            self.project_session.project.revision(),
        );
        if self.automation_last_run == Some(key) {
            return;
        }
        let moves = self
            .project_session
            .project
            .settings()
            .automation
            .desired_track_moves(&self.project_session.project);
        if !moves.is_empty() {
            self.move_lines(moves);
        }
        self.automation_last_run = Some((
            self.project_session.project.active_language_id(),
            self.project_session.project.revision(),
        ));
    }

    pub fn set_project_instrumental_audio_path(&mut self, path: impl Into<String>) {
        self.ui_shell.ui.set_project_instrumental_audio_path(path);
    }

    pub fn close_project_settings_modal(&mut self) {
        self.ui_shell.ui.close_project_settings_modal();
        self.announce_accessibility(AccessibilityEvent::Closed {
            label: crate::i18n::t("project_settings.title").to_string(),
        });
    }

    pub fn save_project_settings(
        &mut self,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
    ) {
        let mut settings = self.project_session.project.settings().clone();
        settings.instrumental_audio_path = instrumental_audio_path;
        settings.highlight_read_word = highlight_read_word;
        settings.scrolling_text_uses_character_color = scrolling_text_uses_character_color;
        EditExecutor::apply_domain_change(
            &mut self.project_session,
            EditOrigin::Local,
            |project| project.set_settings(settings),
        );
        self.sync_audio_settings_to_player();
    }

    pub fn show_toast(&mut self, message: impl Into<String>, duration_secs: f32) {
        let message = message.into();
        self.ui_shell.ui.toasts.push(message.clone(), duration_secs);
        self.announce_shortcut_accessibility(AccessibilityEvent::Success { message });
    }

    pub fn show_proxy_error(&mut self, detail: impl Into<String>) {
        let detail = detail.into();
        self.ui_shell.ui.open_proxy_error_modal(detail.clone());
        self.announce_open_container(
            crate::i18n::t("proxy_error.title"),
            format!("{detail}, {}", crate::i18n::t("proxy_error.close")),
        );
    }

    pub fn open_whats_new_modal(
        &mut self,
        version: impl Into<String>,
        body: impl Into<String>,
        video_url: Option<String>,
        thumbnail: Option<Vec<u8>>,
    ) {
        let version = version.into();
        self.ui_shell
            .ui
            .open_whats_new_modal(version.clone(), body, video_url, thumbnail);
        let content = self
            .ui_shell
            .ui
            .modal_host
            .whats_new
            .as_ref()
            .map(|modal| modal.accessibility_label())
            .unwrap_or_else(|| crate::i18n::t("whats_new.close_hint").to_string());
        self.announce_open_container(crate::i18n::t("whats_new.title"), content);
    }

    pub fn open_pricing_page(&mut self) {
        self.ui_shell.ui.open_pricing_page();
    }

    pub fn close_pricing_page(&mut self) {
        self.ui_shell.ui.close_pricing_page();
    }

    pub fn open_save_prompt(&mut self, kind: crate::ui::save_prompt_modal::SavePromptKind) {
        self.ui_shell.ui.open_save_prompt(kind);
        self.announce_open_container(
            crate::i18n::t("save_prompt.title"),
            crate::i18n::t("save_prompt.cancel").to_string(),
        );
    }

    pub fn toggle_karaoke_for_selection(&mut self) {
        let mut line_ids = self.selected_line_ids();
        if line_ids.is_empty() {
            line_ids.extend(self.ui_shell.ui.rythmo_state.hovered_line);
        }
        if line_ids.is_empty() {
            self.show_toast(crate::i18n::t("toast.karaoke_select_line"), 3.0);
            return;
        }

        let announced_state = line_ids
            .first()
            .and_then(|line_id| self.project_session.project.get_line(*line_id))
            .map(|line| !line.karaoke);

        let lang = self.project_session.project.syllable_language_code();
        let commands: Vec<_> = line_ids
            .into_iter()
            .filter_map(|line_id| {
                self.project_session.project.get_line(line_id).map(|line| {
                    let old_karaoke = line.karaoke;
                    let old_ratios = line.syllable_ratios.clone();
                    let new_karaoke = !old_karaoke;
                    let new_ratios = if new_karaoke {
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang)
                    } else {
                        old_ratios.clone()
                    };
                    Command::SetLineKaraoke {
                        line_id,
                        old_karaoke,
                        old_ratios,
                        new_karaoke,
                        new_ratios,
                    }
                })
            })
            .collect();
        for command in commands {
            self.execute_and_broadcast(command);
        }
        if let Some(enabled) = announced_state {
            self.narration
                .announce_event(AccessibilityEvent::Activation {
                    label: crate::i18n::t(if enabled {
                        "accessibility.checked"
                    } else {
                        "accessibility.unchecked"
                    })
                    .to_string(),
                });
        }
    }

    pub fn set_line_presence(&mut self, line_id: u64, presence: crate::rythmo_line::LinePresence) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let old_presence = line.presence;
        if old_presence == presence {
            return;
        }
        self.execute_and_broadcast(Command::SetLinePresence {
            line_id,
            old_presence,
            new_presence: presence,
        });
    }

    pub fn set_hovered_line_presence(&mut self, presence: crate::rythmo_line::LinePresence) {
        let line_id = self
            .ui_shell
            .ui
            .rythmo_state
            .hovered_line
            .or_else(|| self.selected_line_ids().first().copied());
        if let Some(line_id) = line_id {
            self.set_line_presence(line_id, presence);
        }
    }

    pub fn open_export_modal(&mut self) {
        let (video_width, video_height) = self.source_video_size().unwrap_or((1920, 1080));
        let languages = self
            .project_session
            .project
            .languages()
            .into_iter()
            .map(|language| crate::ui::export_modal::ExportLanguageOption {
                id: language.id,
                name: language.name,
                has_instrumental: self
                    .project_session
                    .project
                    .language_instrumental_audio_path(language.id)
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty()),
            })
            .collect();
        let configuration = self
            .project_session
            .project
            .settings()
            .export_configuration
            .clone();
        self.ui_shell
            .ui
            .open_export_modal(video_width, video_height, languages, configuration);
        if let Some(first_label) = self.export_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("export_modal.title"), first_label);
        }
    }

    fn language_modal_items(&self) -> Vec<crate::ui::language_modal::LanguageListItem> {
        self.project_session
            .project
            .languages()
            .into_iter()
            .map(|language| crate::ui::language_modal::LanguageListItem {
                id: language.id,
                name: language.name,
                instrumental_audio_path: self
                    .project_session
                    .project
                    .language_instrumental_audio_path(language.id),
                syllable_language: self
                    .project_session
                    .project
                    .language_syllable_language(language.id)
                    .unwrap_or_default(),
            })
            .collect()
    }

    pub(crate) fn language_list_initial_accessibility_label(&self) -> Option<String> {
        let active = self.project_session.project.active_language_id();
        let languages = self.language_modal_items();
        languages
            .iter()
            .find(|language| language.id == active)
            .or_else(|| languages.first())
            .map(|language| language.name.clone())
    }

    pub fn open_languages_modal(&mut self) {
        let active = self.project_session.project.active_language_id();
        let languages = self.language_modal_items();
        let first_label = self.language_list_initial_accessibility_label();
        self.ui_shell.ui.open_languages_modal(languages, active);
        if let Some(label) = first_label {
            self.announce_open_container(crate::i18n::t("languages.title"), label);
        }
    }

    pub(crate) fn recent_projects_first_accessibility_label(&self) -> Option<String> {
        crate::config::recent_projects().first().map(|recent| {
            if recent.video_path == recent.br_path {
                return recent
                    .br_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default();
            }
            let video = recent
                .video_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            let project = recent
                .br_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            format!("{video} + {project}")
        })
    }

    pub fn open_recent_projects(&mut self) {
        let first_label = self.recent_projects_first_accessibility_label();
        self.ui_shell.ui.open_recent_projects();
        if let Some(label) = first_label {
            self.announce_open_container(crate::i18n::t("menu.project.recent"), label);
        }
    }

    fn refresh_languages_modal(&mut self) {
        let active = self.project_session.project.active_language_id();
        let languages = self.language_modal_items();
        self.ui_shell.ui.refresh_languages_modal(languages, active);
    }

    pub fn create_language(&mut self, name: String) {
        let id = self
            .project_session
            .project
            .create_language_named(name.clone());
        self.project_session.dirty = true;
        self.project_session.history.clear();
        self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
        self.ui_shell.ui.clear_selection();
        self.sync_audio_settings_to_player();
        self.refresh_languages_modal();
        let selected = self
            .project_session
            .project
            .language(id)
            .map(|language| language.name)
            .unwrap_or(name);
        self.show_toast(
            format!("{} {}", crate::i18n::t("toast.language_created"), selected),
            4.0,
        );
    }

    pub fn rename_language(&mut self, id: u64, name: String) {
        if self
            .project_session
            .project
            .rename_language(id, name.clone())
        {
            self.project_session.dirty = true;
            self.refresh_languages_modal();
            self.show_toast(
                format!("{} {}", crate::i18n::t("toast.language_renamed"), name),
                3.0,
            );
        }
    }

    pub fn select_language(&mut self, id: u64) {
        if id == self.project_session.project.active_language_id() {
            return;
        }
        if self.project_session.project.select_language(id) {
            self.project_session.dirty = true;
            self.project_session.history.clear();
            self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
            self.ui_shell.ui.clear_selection();
            self.sync_audio_settings_to_player();
            self.refresh_languages_modal();
            if let Some(language) = self.project_session.project.language(id) {
                self.show_toast(
                    format!(
                        "{} {}",
                        crate::i18n::t("toast.language_selected"),
                        language.name
                    ),
                    3.0,
                );
            }
        }
    }

    pub fn delete_language(&mut self, id: u64) {
        let name = self
            .project_session
            .project
            .language(id)
            .map(|language| language.name)
            .unwrap_or_default();
        if self.project_session.project.delete_language(id) {
            self.project_session.dirty = true;
            self.project_session.history.clear();
            self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
            self.ui_shell.ui.clear_selection();
            self.sync_audio_settings_to_player();
            self.refresh_languages_modal();
            self.show_toast(
                format!("{} {}", crate::i18n::t("toast.language_deleted"), name),
                3.0,
            );
        }
    }

    pub fn set_language_syllable_language(
        &mut self,
        id: u64,
        language: crate::project::SyllableLanguage,
    ) {
        let active = id == self.project_session.project.active_language_id();
        if self
            .project_session
            .project
            .set_language_syllable_language(id, language)
        {
            self.project_session.dirty = true;
            if active {
                self.project_session.history.clear();
                self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
            }
            self.refresh_languages_modal();
        }
    }

    pub fn set_language_instrumental_audio(&mut self, id: u64, path: Option<String>) {
        if self
            .project_session
            .project
            .set_language_instrumental_audio_path(id, path)
        {
            self.project_session.dirty = true;
            if id == self.project_session.project.active_language_id() {
                self.sync_audio_settings_to_player();
            }
            self.refresh_languages_modal();
        }
    }

    pub fn save_export_configuration(
        &mut self,
        configuration: crate::project::ExportConfiguration,
    ) {
        let mut settings = self.project_session.project.settings().clone();
        if settings.export_configuration == configuration {
            return;
        }
        settings.export_configuration = configuration;
        self.project_session.project.set_settings(settings);
        self.project_session.dirty = true;
    }

    pub fn open_file_explorer(&mut self, request: crate::ui::file_explorer::FileExplorerRequest) {
        let title = request.title.clone();
        let first = match request.mode {
            crate::ui::file_explorer::FileExplorerMode::Open => {
                crate::i18n::t("file_explorer.back").to_string()
            }
            crate::ui::file_explorer::FileExplorerMode::Save => {
                crate::i18n::t("file_explorer.filename").to_string()
            }
        };
        self.ui_shell.ui.open_file_explorer(request);
        self.announce_open_container(&title, first);
    }

    pub fn poll_file_explorer(&mut self) -> bool {
        self.ui_shell.ui.poll_file_explorer()
    }

    pub fn open_voice_actor_modal(&mut self) {
        self.ui_shell.ui.open_voice_actor_modal();
        if let Some(first) = self
            .ui_shell
            .ui
            .modal_host
            .voice_actor
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
        {
            self.announce_open_container(crate::i18n::t("voice_actor_modal.title"), first);
        }
    }

    pub fn open_proxy_modal(&mut self) {
        let (video_width, video_height) = self.source_video_size().unwrap_or((1920, 1080));
        self.ui_shell.ui.open_proxy_modal(video_width, video_height);
        if let Some(first_label) = self.proxy_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("menu.tools.create_proxy"), first_label);
        }
    }

    pub fn close_settings_modal(&mut self) {
        self.ui_shell.ui.close_settings_modal();
        self.render.ui_renderer.clear_text_cache();
        self.announce_accessibility(AccessibilityEvent::Closed {
            label: crate::i18n::t("settings.title").to_string(),
        });
    }

    pub fn rebuild_topbar_for_network(&mut self) {
        let room_code = self.collaboration.network.room_code.clone();
        self.ui_shell.ui.set_network_room_code(room_code.as_deref());
        self.ui_shell
            .ui
            .rebuild_topbar(self.collaboration.network.is_in_room());
    }

    pub fn rebuild_topbar(&mut self) {
        self.ui_shell
            .ui
            .rebuild_topbar(self.collaboration.network.is_in_room());
    }

    pub fn begin_network_connect(&mut self) {
        self.set_network_status("Connexion...");
        self.ui_shell.ui.set_network_room_code(None);
    }

    pub fn disconnect_network(&mut self) {
        self.collaboration.network.disconnect();
        self.set_network_status("");
        self.rebuild_topbar_for_network();
    }

    pub fn set_network_status(&mut self, status: impl Into<String>) {
        self.ui_shell.ui.network_status = status.into();
        let display = if self.ui_shell.ui.network_status.is_empty() {
            "DÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©connectÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©"
        } else {
            &self.ui_shell.ui.network_status
        };
        let display = if self.ui_shell.ui.network_status.is_empty() {
            "Déconnecté"
        } else {
            display
        };
        self.window_manager.main_window.set_title(&format!(
            "Coquerythmo v{} - {}",
            crate::update::current_version(),
            display
        ));
    }

    pub fn update_window_title(&self) {
        let display = if self.ui_shell.ui.network_status.is_empty() {
            "DÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©connectÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©"
        } else {
            &self.ui_shell.ui.network_status
        };
        let display = if self.ui_shell.ui.network_status.is_empty() {
            "Déconnecté"
        } else {
            display
        };
        self.window_manager.main_window.set_title(&format!(
            "Coquerythmo v{} - {}",
            crate::update::current_version(),
            display
        ));
    }

    pub fn request_redraw(&self) {
        self.render.gfx.request_redraw();
    }

    pub fn has_secondary_display(&self) -> bool {
        self.window_manager.secondary_display.is_some()
    }

    pub fn secondary_window_id(&self) -> Option<WindowId> {
        self.window_manager
            .secondary_display
            .as_ref()
            .map(|display| display.window.id())
    }

    pub fn is_video_playing(&self) -> bool {
        self.playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.is_playing())
    }

    pub fn is_secondary_window(&self, window_id: WindowId) -> bool {
        self.window_manager
            .secondary_display
            .as_ref()
            .is_some_and(|display| display.window.id() == window_id)
    }

    pub fn open_secondary_display(&mut self, window: Arc<Window>) {
        if self.playback.video_player.is_none() {
            log::warn!("No video loaded ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â cannot open secondary display");
            return;
        }

        if let Some(display) = &self.window_manager.secondary_display {
            display.window.request_redraw();
            return;
        }

        match self.render.gfx.create_window_surface(window) {
            Ok(display) => {
                self.window_manager.secondary_display = Some(display);
                self.request_redraw();
                self.request_secondary_redraw();
            }
            Err(e) => log::error!("Failed to open secondary display: {e}"),
        }
    }

    pub fn close_secondary_display(&mut self) {
        self.window_manager.secondary_display = None;
        self.request_redraw();
    }

    pub fn resize_secondary_display(
        &mut self,
        window_id: WindowId,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) {
        if let Some(display) = &mut self.window_manager.secondary_display {
            if display.window.id() == window_id {
                display.resize(&self.render.gfx.device, new_size);
            }
        }
    }

    pub fn request_secondary_redraw(&self) {
        if let Some(display) = &self.window_manager.secondary_display {
            display.request_redraw();
        }
    }

    // -- Video --

    pub fn current_frame(&self) -> i64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(0, |p| p.current_frame())
    }

    fn timecode_for_frame(&self, frame: i64) -> String {
        let fps = self.fps().max(1.0);
        let total_centiseconds = ((frame.max(0) as f64 / fps) * 100.0).round() as i64;
        let hours = total_centiseconds / 360_000;
        let minutes = (total_centiseconds / 6_000) % 60;
        let seconds = (total_centiseconds / 100) % 60;
        let centiseconds = total_centiseconds % 100;
        let hour_label = if hours == 1 {
            crate::i18n::t("accessibility.hour")
        } else {
            crate::i18n::t("accessibility.hours")
        };
        let minute_label = if minutes == 1 {
            crate::i18n::t("accessibility.minute")
        } else {
            crate::i18n::t("accessibility.minutes")
        };
        let second_label = if seconds == 1 {
            crate::i18n::t("accessibility.second")
        } else {
            crate::i18n::t("accessibility.seconds")
        };
        let centisecond_label = if centiseconds == 1 {
            crate::i18n::t("accessibility.hundredth")
        } else {
            crate::i18n::t("accessibility.hundredths")
        };
        format!(
            "{hours} {hour_label}, {minutes} {minute_label}, {seconds} {second_label}, {centiseconds} {centisecond_label}"
        )
    }

    fn announce_current_timecode(&self) {
        self.narration
            .announce_event(AccessibilityEvent::ValueChanged {
                label: crate::i18n::t("accessibility.timecode").to_string(),
                value: self.timecode_for_frame(self.current_frame()),
            });
    }

    pub fn render_frame(&self) -> f64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(self.current_frame() as f64, |p| {
                p.current_frame_for_render()
            })
    }

    pub fn fps(&self) -> f64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(30.0, |p| p.fps())
    }

    pub fn total_frames(&self) -> i64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(0, |p| p.total_frames())
    }

    pub fn source_video_size(&self) -> Option<(u32, u32)> {
        self.playback.source_video_size.or_else(|| {
            self.playback
                .video_player
                .as_ref()
                .and_then(|player| player.video_size())
        })
    }

    pub fn video_path(&self) -> Option<PathBuf> {
        self.playback
            .source_video_path
            .clone()
            .or_else(|| self.playback.video_player.as_ref().and_then(|p| p.path()))
    }

    pub fn load_video(&mut self, path: &Path) -> bool {
        let proxy_path = self
            .project_session
            .project_path
            .as_ref()
            .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, path));
        let loaded = self.load_video_for_playback(path, proxy_path.as_deref(), None);
        if loaded {
            self.sync_audio_settings_to_player();
        }
        loaded
    }

    /// Drop the decoder before releasing a portable project's extraction
    /// guard, so no player keeps paths into an already-cleaned temporary tree.
    pub fn clear_video_for_new_project(&mut self) {
        if self.ui_shell.ui.is_playing() {
            self.ui_shell.ui.toggle_play_pause();
        }
        self.playback.video_player = None;
        self.playback.source_video_path = None;
        self.playback.source_video_size = None;
        self.playback.proxy_video_path = None;
        self.playback.last_scroll_time = None;
        self.playback.scroll_needs_decode = false;
        self.playback.last_waveform_revision = 0;
        self.ui_shell.ui.has_video = false;
        self.ui_shell.ui.total_frames = 0;
        self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
        self.playback.timeline.emit(TimelineEvent::VideoLoaded {
            fps: 30.0,
            total_frames: 0,
        });
        self.playback
            .timeline
            .emit(TimelineEvent::FrameChanged { frame: 0 });
        self.rebuild_topbar_for_network();
    }

    pub fn reload_linked_proxy(&mut self) {
        if let Some(br_path) = &self.project_session.project_path {
            if let Some(link) = crate::video_proxy::proxy_link_for_br(br_path) {
                let source_matches = self.video_path().as_ref().is_some_and(|path| {
                    crate::video_proxy::paths_match(path, &link.source_video_path)
                });
                let proxy_matches = self.playback.proxy_video_path.as_ref().is_some_and(|path| {
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
            .project_session
            .project_path
            .as_ref()
            .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, &source_path));

        if proxy_path == self.playback.proxy_video_path {
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
        self.jobs.pending_proxy_job = Some(PendingProxyJob {
            source_path,
            receiver,
        });
    }

    pub fn watch_export_job(&mut self, receiver: Receiver<Result<(), String>>) {
        self.jobs.pending_export_job = Some(PendingExportJob { receiver });
    }

    pub fn start_configured_export(
        &mut self,
        output_base: PathBuf,
        configuration: crate::project::ExportConfiguration,
    ) {
        let audio_outputs_enabled = configuration.audio_formats.mp3
            || configuration.audio_formats.wav
            || configuration.audio_formats.bwf_stems;
        let original_audio_selected =
            configuration
                .selected_language_ids
                .iter()
                .any(|language_id| {
                    configuration
                        .audio_by_language
                        .get(language_id)
                        .copied()
                        .unwrap_or_default()
                        .original
                });
        let source_video = self.video_path();
        if source_video.is_none()
            && (configuration.video_enabled || (audio_outputs_enabled && original_audio_selected))
        {
            self.show_toast(crate::i18n::t("toast.export_requires_video"), 4.0);
            return;
        }
        self.save_export_configuration(configuration.clone());
        let project = self.project_session.project.snapshot();
        let source_fps = self.fps();
        let source_total_frames = self.total_frames();
        let source_size = self.source_video_size().unwrap_or((1920, 1080));
        let progress = Arc::new(std::sync::atomic::AtomicU32::new(0.0_f32.to_bits()));
        let progress_for_ui = progress.clone();
        let render_backend = Arc::new(std::sync::atomic::AtomicU32::new(
            crate::video_export::EXPORT_RENDER_BACKEND_UNKNOWN,
        ));
        let render_backend_for_ui = render_backend.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_job = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            progress.store(0.001_f32.to_bits(), Ordering::Relaxed);
            let result =
                crate::configured_export::run(crate::configured_export::ConfiguredExportContext {
                    project: &project,
                    source_video: source_video.as_deref(),
                    output_base: &output_base,
                    source_fps,
                    source_total_frames,
                    source_size,
                    configuration: &configuration,
                    render_backend_status: Some(render_backend),
                    progress: progress.clone(),
                    cancel: cancel_for_job,
                })
                .map(|outputs| {
                    for output in outputs {
                        log::info!("Delivery exported to {}", output.display());
                    }
                });
            let _ = sender.send(result);
            progress.store(2.0_f32.to_bits(), Ordering::Relaxed);
        });

        self.set_progress_label(crate::i18n::t("progress.exporting"));
        self.set_export_render_backend(Some(render_backend_for_ui));
        self.set_export_progress(Some(progress_for_ui));
        self.set_export_cancel(Some(cancel));
        self.watch_export_job(receiver);
        self.announce_shortcut_accessibility(AccessibilityEvent::Opened {
            label: format!(
                "{} {}",
                crate::i18n::t("progress.exporting"),
                crate::i18n::t("progress.cancel_hint")
            ),
        });
    }

    /// Kick off a background parse of a bande rythmo file and show a loading
    /// modal while it runs. `apply_to_project` (main-thread) happens on completion.
    pub fn start_br_import(&mut self, br_path: PathBuf) {
        use std::sync::mpsc;

        if self.is_project_save_in_progress() {
            self.show_toast(crate::i18n::t("toast.project_change_blocked_saving"), 5.0);
            return;
        }

        let label = br_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let thread_path = br_path.clone();
        std::thread::spawn(move || {
            let result = crate::project_archive::load_project_file(&thread_path)
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        self.jobs.pending_import_job = Some(PendingImportJob {
            br_path,
            receiver: rx,
        });
        self.ui_shell.ui.loading_project = Some((label, Instant::now()));
        self.narration.announce_event(AccessibilityEvent::Opened {
            label: format!(
                "{} {}",
                crate::i18n::t("loading_project.title"),
                self.ui_shell
                    .ui
                    .loading_project
                    .as_ref()
                    .map(|(label, _)| label.as_str())
                    .unwrap_or_default()
            ),
        });
        self.request_redraw();
    }

    fn load_video_for_playback(
        &mut self,
        source_path: &Path,
        proxy_path: Option<&Path>,
        seek_frame: Option<i64>,
    ) -> bool {
        let (bgl, sampler) = self.renderer_refs();
        let mut player = VideoPlayer::new();

        // Every decoder load resolves the proxy again. This covers project loads,
        // recent projects and explicit reloads without requiring each caller to
        // remember the proxy policy.
        let linked_proxy = proxy_path.map(Path::to_path_buf).or_else(|| {
            self.project_session
                .project_path
                .as_deref()
                .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, source_path))
        });
        let mut active_proxy_path = linked_proxy;
        let mut load_path = active_proxy_path.as_deref().unwrap_or(source_path);
        let mut load_result = player.load_with_audio(
            load_path,
            source_path,
            &self.render.gfx.device,
            &self.render.gfx.queue,
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
                    &self.render.gfx.device,
                    &self.render.gfx.queue,
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
                return false;
            }
        }

        player.set_volume(self.ui_shell.ui.volume());
        if let Some(frame) = seek_frame {
            let total = player.total_frames();
            let target = if total > 0 {
                frame.clamp(0, total - 1)
            } else {
                frame.max(0)
            };
            player.seek_frame_instant(target as i32);
            player.decode_current_frame(
                &self.render.gfx.device,
                &self.render.gfx.queue,
                bgl,
                sampler,
            );
        }

        if self.ui_shell.ui.is_playing() {
            self.ui_shell.ui.toggle_play_pause();
        }

        let fps = player.fps();
        let total = player.total_frames();
        let current_frame = player.current_frame();
        // A fresh Recording document is created before a video is selected.
        // Align that untouched document with the source timebase as soon as the
        // real FPS is known; never replace a document the user already edited.
        if self.project_session.recording_revision == 0
            && self.project_session.recording_project.assets().len() == 0
            && self.project_session.recording_project.clips().len() == 0
        {
            self.project_session.reset_recording_document(fps);
        }
        let source_size = crate::video_proxy::probe_video(source_path)
            .ok()
            .map(|info| (info.width, info.height))
            .or_else(|| player.video_size());

        self.playback.source_video_path = Some(source_path.to_path_buf());
        self.playback.source_video_size = source_size;
        self.playback.proxy_video_path = active_proxy_path;
        self.playback.video_player = Some(player);
        self.sync_audio_settings_to_player();
        self.playback.timeline.emit(TimelineEvent::VideoLoaded {
            fps,
            total_frames: total,
        });
        self.playback.timeline.emit(TimelineEvent::FrameChanged {
            frame: current_frame,
        });
        self.ui_shell.ui.has_video = true;
        self.ui_shell.ui.total_frames = total;
        self.rebuild_topbar_for_network();
        true
    }

    pub fn toggle_play_pause(&mut self) {
        let playing = {
            let Some(player) = &mut self.playback.video_player else {
                return;
            };
            if !player.toggle() {
                return;
            }
            let playing = player.is_playing();
            if self.ui_shell.ui.is_playing() != playing {
                self.ui_shell.ui.toggle_play_pause();
            }
            if playing {
                self.playback.timeline.emit(TimelineEvent::PlaybackStarted);
            } else {
                self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
            }
            playing
        };
        self.narration
            .announce_event(AccessibilityEvent::Activation {
                label: crate::i18n::t(if playing {
                    "toolbar.play"
                } else {
                    "toolbar.stop"
                })
                .to_string(),
            });
    }

    pub fn toggle_active_audio(&mut self) {
        let Some(player) = &mut self.playback.video_player else {
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
        self.playback
            .video_player
            .as_ref()
            .map(|player| player.active_audio_offset_frames())
            .unwrap_or(0)
    }

    pub fn active_audio_is_instrumental(&self) -> bool {
        self.playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.active_audio_track() == AudioTrack::Instrumental)
    }

    pub fn offset_active_audio_by(&mut self, delta_frames: i64) {
        if delta_frames == 0 {
            return;
        }
        let Some(player) = &mut self.playback.video_player else {
            return;
        };
        match player.active_audio_track() {
            AudioTrack::Source => EditExecutor::apply_domain_change(
                &mut self.project_session,
                EditOrigin::Local,
                |project| project.adjust_source_audio_offset(delta_frames),
            ),
            AudioTrack::Instrumental => EditExecutor::apply_domain_change(
                &mut self.project_session,
                EditOrigin::Local,
                |project| project.adjust_instrumental_audio_offset(delta_frames),
            ),
        };
        player.adjust_active_audio_offset(delta_frames);
    }

    pub fn sync_audio_settings_to_player(&mut self) {
        let Some(player) = &mut self.playback.video_player else {
            return;
        };
        let settings = self.project_session.project.settings();
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
            self.playback.last_nonzero_volume = vol;
        }
        self.ui_shell.ui.set_volume(vol);
        if let Some(player) = &mut self.playback.video_player {
            player.set_volume(vol);
        }
        self.narration
            .announce_event(AccessibilityEvent::ValueChanged {
                label: crate::i18n::t("accessibility.volume").to_string(),
                value: format!("{} %", (vol.clamp(0.0, 1.0) * 100.0).round()),
            });
    }

    pub fn toggle_mute(&mut self) {
        let target = if self.ui_shell.ui.volume() > 0.001 {
            0.0
        } else {
            self.playback.last_nonzero_volume.max(0.75)
        };
        self.set_volume(target);
    }

    pub fn toggle_screen_reader(&mut self) {
        if !self.narration.is_available() {
            self.show_toast(crate::i18n::t("accessibility.unavailable"), 4.0);
            return;
        }
        let enabled = self.narration.set_enabled(!self.narration.is_enabled());
        crate::config::set_screen_reader_enabled(enabled);
        let message = if enabled {
            crate::i18n::t("accessibility.enabled")
        } else {
            crate::i18n::t("accessibility.disabled")
        };
        self.show_toast(message, 3.0);
    }

    pub fn announce_accessibility(&self, event: AccessibilityEvent) {
        if self.is_ctrl_held() {
            self.narration.defer_control_shortcut(event);
        } else {
            self.narration.announce_event(event);
        }
    }

    pub fn announce_shortcut_accessibility(&self, event: AccessibilityEvent) {
        if self.is_ctrl_held() {
            self.narration.defer_control_shortcut(event);
        } else {
            self.narration.announce_shortcut_event(event);
        }
    }

    pub fn stop_narration(&self) {
        self.narration.stop();
    }

    pub fn resume_narration(&self) {
        self.narration.resume();
    }

    pub fn prev_frame(&mut self) {
        let bgl = self.render.ui_renderer.texture_bind_group_layout();
        let sampler = self.render.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.playback.video_player {
            player.step_backward(
                &self.render.gfx.device,
                &self.render.gfx.queue,
                bgl,
                sampler,
            );
            if self.ui_shell.ui.is_playing() {
                self.ui_shell.ui.toggle_play_pause();
            }
        }
    }

    pub fn next_frame(&mut self) {
        let bgl = self.render.ui_renderer.texture_bind_group_layout();
        let sampler = self.render.ui_renderer.texture_sampler();
        if let Some(player) = &mut self.playback.video_player {
            player.step_forward(
                &self.render.gfx.device,
                &self.render.gfx.queue,
                bgl,
                sampler,
            );
            if self.ui_shell.ui.is_playing() {
                self.ui_shell.ui.toggle_play_pause();
            }
        }
    }

    pub fn seek_absolute(&mut self, frame: i64) {
        if let Some(player) = &mut self.playback.video_player {
            if player.pause_for_seek() {
                if self.ui_shell.ui.is_playing() {
                    self.ui_shell.ui.toggle_play_pause();
                }
                self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
            }
            let current = player.current_frame();
            let delta = (frame - current) as i32;
            player.seek_frame_instant(delta);
            self.playback.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
        }
        self.playback.last_scroll_time = Some(Instant::now());
        self.playback.scroll_needs_decode = true;
    }

    pub fn finish_seek(&mut self) {
        self.playback.scroll_needs_decode = false;
        self.playback.last_scroll_time = None;

        if let Some(player) = &mut self.playback.video_player {
            player.prepare_current_frame();
        }
    }

    pub fn seek_relative(&mut self, delta: i32) {
        if let Some(player) = &mut self.playback.video_player {
            player.seek_frame_instant(delta);
            self.playback.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
        }
        self.playback.last_scroll_time = Some(Instant::now());
        self.playback.scroll_needs_decode = true;
    }

    pub fn seek_to_next_boucle(&mut self, direction: i32) {
        let current = self.current_frame();
        let boucle_frames: Vec<i64> = self
            .project_session
            .project
            .markers()
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
        if !self.playback.scroll_needs_decode {
            return false;
        }
        if let Some(t) = self.playback.last_scroll_time {
            if t.elapsed().as_millis() >= constants::SCROLL_DECODE_DELAY_MS {
                self.playback.scroll_needs_decode = false;
                if let Some(player) = &mut self.playback.video_player {
                    if player.is_playing() {
                        player.restart_playback_decoders();
                    } else {
                        player.prepare_current_frame();
                    }
                }
                return true;
            }
        }

        false
    }

    // -- Network --

    fn receive_recording_transaction(
        &mut self,
        transaction: crate::recording::RecordingTransaction,
    ) {
        if self
            .project_session
            .recording_transactions
            .entry_by_sequence(transaction.sequence)
            .is_some_and(|existing| existing == &transaction)
        {
            // Socket.IO servers may echo a controller's own transaction. The
            // integrity chain makes an identical sequence entry idempotent.
            return;
        }

        let result = self
            .project_session
            .recording_transactions
            .append_received_and_apply(&mut self.project_session.recording_project, transaction);
        match result {
            Ok(_) => {
                self.project_session.mark_recording_changed();
                self.sync_recording_workspace_ui();
            }
            Err(error) => self.recording_error(error.to_string()),
        }
    }

    fn receive_recording_prepare(&mut self, prepare: crate::network::RecordingPreparePayload) {
        let crate::network::RecordingPreparePayload {
            project,
            transactions,
            current_frame,
            capture_target,
        } = prepare;

        // Never trust a snapshot independently from its transaction journal:
        // rebuild from the canonical empty base and require byte-level domain
        // equality before replacing the live session.
        let rebuilt = crate::recording::RecordingProject::new(project.timeline_fps())
            .and_then(|base| transactions.rebuild_from_base(&base));
        let rebuilt = match rebuilt {
            Ok(rebuilt) if rebuilt == project => rebuilt,
            Ok(_) => {
                self.recording_error(
                    "the received recording snapshot does not match its transaction log",
                );
                return;
            }
            Err(error) => {
                self.recording_error(error.to_string());
                return;
            }
        };

        let changed = self.project_session.recording_project != rebuilt
            || self.project_session.recording_transactions != transactions;
        self.project_session.recording_project = rebuilt;
        self.project_session.recording_transactions = transactions;
        self.project_session
            .recording_asset_paths
            .retain(|asset_id, _| {
                self.project_session
                    .recording_project
                    .asset(*asset_id)
                    .is_some()
            });
        if changed {
            self.project_session.mark_recording_changed();
        }

        self.seek_absolute(current_frame);
        let local_member_is_muted = self
            .collaboration
            .network
            .member_id
            .as_deref()
            .and_then(|member_id| {
                self.collaboration
                    .network
                    .member_details
                    .iter()
                    .find(|member| member.id == member_id)
            })
            .is_some_and(|member| member.muted);
        if let Some(target) = capture_target {
            if !local_member_is_muted && !self.recording_runtime.is_active() {
                if let Err(error) = self.recording_runtime.begin_capture_target(target) {
                    self.recording_error(error.to_string());
                }
            }
        }
        self.sync_recording_workspace_ui();
    }

    fn receive_recording_playback(&mut self, playback: crate::network::RecordingPlaybackPayload) {
        self.seek_absolute(playback.frame);
        let is_playing = self
            .playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.is_playing());
        if playback.playing != is_playing {
            self.toggle_play_pause();
        }
    }

    fn finish_recording_audio_receive(&mut self, transfer_id: &str) {
        let received = match self.recording_runtime.finish_audio_receive(transfer_id) {
            Ok(received) => received,
            Err(error) => {
                self.recording_error(error);
                return;
            }
        };
        let crate::audio_transfer::ReceivedAudio { metadata, path } = received;

        let matching_asset_already_exists = self
            .project_session
            .recording_project
            .asset(metadata.target.asset_id)
            .is_some_and(|asset| asset.checksum == metadata.audio.checksum);
        if matching_asset_already_exists {
            self.project_session
                .recording_asset_paths
                .insert(metadata.target.asset_id, path);
            self.project_session.mark_recording_changed();
            return;
        }

        if matches!(
            self.recording_network_role(),
            crate::ui::recording_workspace::RecordingRole::Director
        ) {
            // Every participant receives the same reserved capture IDs. The
            // authoritative DA proposes fresh IDs against its current state so
            // simultaneous takes never collide, then broadcasts the resulting
            // atomic AddAsset + AddClip transaction.
            let target = match self
                .project_session
                .recording_project
                .propose_capture_target(metadata.target.track_id, metadata.target.start_frame)
            {
                Ok(target) => target,
                Err(error) => {
                    self.recording_error(error.to_string());
                    return;
                }
            };
            let asset_id = target.asset_id;
            let operation = crate::recording::CompletedCapture {
                target,
                audio: metadata.audio,
            }
            .into_project_operation(self.project_session.recording_project.timeline_fps());
            if let Err(error) = self.apply_recording_operation(operation) {
                self.recording_error(error.to_string());
                return;
            }
            self.project_session
                .recording_asset_paths
                .insert(asset_id, path);
        } else {
            // Non-authoritative peers receive the matching transaction from
            // the DA; keeping the verified local path is intentionally not a
            // second durable timeline mutation.
            self.project_session
                .recording_asset_paths
                .insert(metadata.target.asset_id, path);
            self.project_session.mark_recording_changed();
        }
    }

    pub fn tick_network(&mut self) -> bool {
        let prev_state = self.collaboration.network.state;
        let mut changed = false;
        while let Some(msg) = self.collaboration.network.try_recv() {
            changed = true;
            match msg {
                IncomingMessage::Connected => {
                    self.collaboration.network.state = ConnectionState::Connected;
                    self.set_network_status("ConnectÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â© au serveur");
                    log::info!("Connected and authenticated");
                }
                IncomingMessage::Packet(packet) => self.handle_network_packet(packet),
                IncomingMessage::Disconnected(reason) => {
                    log::info!("Disconnected: {reason}");
                    self.collaboration.network.state = ConnectionState::Disconnected;
                    self.collaboration.network.room_code = None;
                    self.collaboration.network.role = None;
                    self.collaboration.network.members.clear();
                    self.collaboration.network.member_id = None;
                    self.collaboration.network.project_huuid = None;
                    self.collaboration.network.member_details.clear();
                    self.collaboration.network.control_owner_id = None;
                    self.set_network_status("");
                    self.ui_shell.ui.set_network_room_code(None);
                }
                IncomingMessage::Error(err) => {
                    log::error!("Network error: {err}");
                    self.set_network_status(format!("Erreur: {err}"));
                }
                IncomingMessage::RoomMetadata {
                    member_id,
                    project_huuid,
                } => {
                    self.collaboration.network.member_id = Some(member_id);
                    self.collaboration.network.project_huuid = Some(project_huuid);
                }
                IncomingMessage::RoomState {
                    members,
                    control_owner_id,
                } => {
                    self.collaboration.network.members = members
                        .iter()
                        .map(|member| member.username.clone())
                        .collect();
                    self.collaboration.network.member_details = members;
                    self.collaboration.network.control_owner_id = control_owner_id;
                }
                IncomingMessage::Delta(data) => self.apply_delta(data),
                IncomingMessage::RecordingTransaction(transaction) => {
                    self.receive_recording_transaction(transaction)
                }
                IncomingMessage::RecordingPrepare(prepare) => {
                    self.receive_recording_prepare(prepare)
                }
                IncomingMessage::RecordingPlayback(playback) => {
                    self.receive_recording_playback(playback)
                }
                IncomingMessage::SyncRequested { requester } => {
                    log::info!("Sync requested by {requester}");
                    let data = ProjectData::from_project(&self.project_session.project);
                    let mut json = serde_json::json!({ "project": data });
                    if !requester.is_empty() {
                        json["_target"] = serde_json::Value::String(requester);
                    }
                    self.collaboration.network.send_raw("sync", json);
                }
                IncomingMessage::AudioStart { metadata } => {
                    match serde_json::from_value(metadata) {
                        Ok(metadata) => {
                            if let Err(error) = self.recording_runtime.begin_audio_receive(metadata)
                            {
                                self.recording_error(error);
                            }
                        }
                        Err(error) => self
                            .recording_error(format!("invalid recording audio metadata: {error}")),
                    }
                }
                IncomingMessage::AudioChunk {
                    transfer_id,
                    index,
                    data_base64,
                } => {
                    if let Err(error) =
                        self.recording_runtime
                            .push_audio_chunk(&transfer_id, index, &data_base64)
                    {
                        self.recording_error(error);
                    }
                }
                IncomingMessage::AudioEnd { transfer_id } => {
                    self.finish_recording_audio_receive(&transfer_id)
                }
                // Video transfer messages remain unused.
                IncomingMessage::VideoStart { .. }
                | IncomingMessage::VideoChunk { .. }
                | IncomingMessage::VideoEnd => {}
            }
        }
        // Rebuild topbar if connection state changed
        if self.collaboration.network.state != prev_state {
            self.ui_shell
                .ui
                .rebuild_topbar(self.collaboration.network.is_in_room());
            changed = true;
        }

        changed
    }

    fn handle_network_packet(&mut self, packet: Packet) {
        match packet {
            Packet::RoomCreated { code } => {
                self.collaboration.network.state = ConnectionState::InRoom;
                self.collaboration.network.room_code = Some(code.clone());
                self.collaboration.network.role = Some("admin".into());
                self.set_network_status("Salon crÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©");
                self.ui_shell.ui.set_network_room_code(Some(&code));
                self.show_toast(
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
                self.collaboration.network.state = ConnectionState::InRoom;
                self.collaboration.network.room_code = Some(code.clone());
                self.collaboration.network.role = Some(role);
                self.collaboration.network.members = members;
                self.set_network_status("ConnectÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â© au salon");
                self.ui_shell.ui.set_network_room_code(Some(&code));
                self.show_toast(
                    format!("{}{code}", crate::i18n::t("toast.room_joined")),
                    5.0,
                );
                self.ui_shell.ui.sync_overlay = Some("Synchronisation en cours...".into());
                self.ui_shell.ui.sync_progress = 0.0;
                // request_sync is sent directly from the room_joined callback
            }
            Packet::JoinError { reason } => {
                log::error!("Join failed: {reason}");
                self.set_network_status(format!("ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â°chec: {reason}"));
                self.ui_shell.ui.set_network_room_code(None);
            }
            Packet::MemberJoined { username } => {
                self.collaboration.network.members.push(username.clone());
                log::info!("Member joined: {username}");
            }
            Packet::MemberLeft { username } => {
                self.collaboration
                    .network
                    .members
                    .retain(|m| m != &username);
                log::info!("Member left: {username}");
            }
            Packet::RemoteCommand { from, payload } => {
                log::debug!("Remote command from {from}");
                self.apply_remote_command(payload);
            }
            Packet::Sync { project: data } => {
                self.apply_project_sync(data);
                self.ui_shell.ui.sync_overlay = None;
                if self.collaboration.network.room_code.is_some() {
                    self.set_network_status("Salon synchronisÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©");
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
        EditExecutor::apply_remote_payload(&mut self.project_session, payload, EditOrigin::Remote);
    }
    fn apply_project_sync(&mut self, data: ProjectData) {
        EditExecutor::apply_sync(&mut self.project_session, data);
        log::info!("Project synced (merged)");
    }
    fn apply_delta(&mut self, data: serde_json::Value) {
        log::debug!(
            "Applying delta: {}",
            data["action"].as_str().unwrap_or("unknown")
        );
        if let Some(payload) = decode_delta(&data) {
            EditExecutor::apply_remote_payload(
                &mut self.project_session,
                payload,
                EditOrigin::Remote,
            );
        } else {
            log::warn!("Rejected malformed or unknown delta payload");
        }
    }
    /// Apply a canonical local command, record it, then broadcast its legacy
    /// delta. Encoding happens before `apply` because move-marker deltas read
    /// the marker's current position from the project.
    fn execute_and_broadcast(&mut self, cmd: Command) {
        let requires_full_sync = matches!(
            cmd,
            Command::InsertLines { .. } | Command::DeleteLines { .. }
        ) && self.collaboration.network.is_in_room();
        let payload = if self.collaboration.network.is_in_room() {
            encode_delta(&cmd, &self.project_session.project)
        } else {
            None
        };
        EditExecutor::execute(&mut self.project_session, cmd, EditOrigin::Local);
        if let Some(payload) = payload {
            self.collaboration.network.send_raw("delta", payload);
        } else if requires_full_sync {
            self.broadcast_full_sync();
        }
    }

    /// Broadcast a single command as a delta via the "delta" event.
    fn broadcast_delta(&self, cmd: &Command) {
        if !self.collaboration.network.is_in_room() {
            return;
        }
        let Some(payload) = encode_delta(cmd, &self.project_session.project) else {
            return;
        };
        self.collaboration.network.send_raw("delta", payload);
    }

    /// Broadcast coalesced final state on mouse release / StopEditing.
    pub fn broadcast_finalize(&self) {
        if !self.collaboration.network.is_in_room() {
            return;
        }
        if let Some(cmd) = self.project_session.history.last() {
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
                    | Command::AddDrawingStroke { .. }
                    | Command::EraseDrawingStrokes { .. }
                    | Command::TransformStrokes { .. }
            ) {
                self.broadcast_delta(cmd);
            }
        }
    }

    /// Broadcast full project state (only for undo/redo/join sync).
    fn broadcast_full_sync(&self) {
        if !self.collaboration.network.is_in_room() {
            return;
        }
        let data = ProjectData::from_project(&self.project_session.project);
        self.collaboration
            .network
            .send_raw("sync", serde_json::json!({ "project": data }));
    }

    // -- Undo / Redo --

    pub fn undo(&mut self) {
        if EditExecutor::undo(&mut self.project_session) {
            self.broadcast_full_sync();
        }
    }

    pub fn redo(&mut self) {
        if EditExecutor::redo(&mut self.project_session) {
            self.broadcast_full_sync();
        }
    }

    pub fn clear_history(&mut self) {
        self.project_session.history.clear();
    }

    // -- Project / Lines (all via Command pattern) --

    pub fn open_toolbar_dropdown(&mut self, dropdown: crate::ui::primitives::ToolbarDropdown) {
        let (list_label, first_item) = match &dropdown {
            crate::ui::primitives::ToolbarDropdown::Respirations => (
                crate::i18n::t("toolbar.respirations").to_string(),
                crate::i18n::t("resp.up").to_string(),
            ),
            crate::ui::primitives::ToolbarDropdown::Reactions => (
                crate::i18n::t("toolbar.reactions").to_string(),
                crate::i18n::t("react.x").to_string(),
            ),
        };
        if self.ui_shell.ui.toggle_toolbar_dropdown(dropdown) {
            self.announce_open_container(&list_label, first_item);
        } else {
            self.announce_accessibility(AccessibilityEvent::Collapsed { label: list_label });
        }
    }

    pub fn open_rename_character_modal(&mut self) {
        let mut characters = self.project_session.project.character_names_from_lines();
        characters.sort_by_key(|name| name.to_lowercase());
        if characters.is_empty() {
            self.show_toast(crate::i18n::t("toast.no_character_to_rename"), 4.0);
            return;
        }
        self.ui_shell.ui.open_rename_character_modal(characters);
        if let Some(first_label) = self.rename_character_modal_focus_label() {
            self.announce_open_container(
                crate::i18n::t("rename_character_modal.title"),
                first_label,
            );
        }
    }

    pub fn open_lines_panel(&mut self) {
        self.ui_shell.ui.open_side_panel_with_selection(
            crate::ui::side_panel::SidePanelKind::Lines,
            self.selected_line_ids(),
        );
        let first = self
            .ui_shell
            .ui
            .side_panel_first_accessibility_label(&self.project_session.project);
        self.announce_open_container(crate::i18n::t("panel.lines.title"), first);
    }

    pub fn open_roles_panel(&mut self) {
        self.ui_shell
            .ui
            .open_side_panel(crate::ui::side_panel::SidePanelKind::Roles);
        let first = self
            .ui_shell
            .ui
            .side_panel_first_accessibility_label(&self.project_session.project);
        self.announce_open_container(crate::i18n::t("panel.roles.title"), first);
    }

    pub fn close_side_panel(&mut self) {
        let title = self
            .ui_shell
            .ui
            .side_panel_accessibility_title()
            .map(str::to_string);
        self.ui_shell.ui.close_side_panel();
        if let Some(label) = title {
            self.announce_accessibility(AccessibilityEvent::Closed { label });
        }
    }

    pub fn set_lines_role(&mut self, line_ids: Vec<u64>, name: String, color: [f32; 4]) {
        for line_id in line_ids {
            self.set_character(line_id, name.clone(), color);
        }
    }

    pub fn set_role_color(&mut self, role: String, color: [f32; 4]) {
        let ids: Vec<u64> = self
            .project_session
            .project
            .lines()
            .filter(|line| line.character_name == role)
            .map(|line| line.id)
            .collect();
        for line_id in ids {
            self.set_character(line_id, role.clone(), color);
        }
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
            .project_session
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

        let old_known_characters = self.project_session.project.known_characters().to_vec();
        let new_known_characters = self.known_characters_after_rename(&old_name, &new_name);
        self.execute_and_broadcast(Command::RenameCharacter {
            changes,
            old_known_characters,
            new_known_characters,
        });
        self.show_toast(crate::i18n::t("toast.character_renamed"), 3.0);
    }

    fn known_characters_after_rename(&self, old_name: &str, new_name: &str) -> Vec<Character> {
        let mut known_characters = self.project_session.project.known_characters().to_vec();
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
            .project_session
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

    pub fn delete_selected(&mut self) {
        if self.ui_shell.ui.automation_open() {
            if let Some(node_id) = self.ui_shell.ui.take_selected_automation_node() {
                self.automation_delete_node(node_id);
            }
            return;
        }
        use crate::workspaces::rythmo::view::Selection;
        let mut deleted_lines = 0usize;
        if let Some(ref sel) = self.ui_shell.ui.rythmo_state().selected {
            match sel {
                Selection::Line(id) => {
                    if let (Some(snapshot), Some(index)) = (
                        self.project_session.project.get_line(*id).cloned(),
                        self.project_session.project.line_index(*id),
                    ) {
                        self.execute_and_broadcast(Command::DeleteLine { snapshot, index });
                        deleted_lines = 1;
                    }
                }
                Selection::Lines(ids) => {
                    let lines: Vec<_> = self
                        .project_session
                        .project
                        .lines()
                        .filter(|line| ids.contains(&line.id))
                        .filter_map(|line| {
                            self.project_session
                                .project
                                .line_index(line.id)
                                .map(|index| (line.clone(), index))
                        })
                        .collect();
                    if !lines.is_empty() {
                        deleted_lines = lines.len();
                        self.execute_and_broadcast(Command::DeleteLines { lines });
                    }
                }
                Selection::Marker(idx) => {
                    if let Some(marker) = self.project_session.project.marker(*idx).cloned() {
                        self.execute_and_broadcast(Command::RemoveMarker {
                            marker,
                            index: *idx,
                        });
                    }
                }
                Selection::AllLines => {
                    // Snapshot the active band before mutating it. Deleting
                    // through canonical commands keeps undo/redo and network
                    // collaboration consistent with single-line deletion.
                    let lines: Vec<_> = self
                        .project_session
                        .project
                        .lines()
                        .filter_map(|line| {
                            self.project_session
                                .project
                                .line_index(line.id)
                                .map(|index| (line.clone(), index))
                        })
                        .collect();
                    if !lines.is_empty() {
                        deleted_lines = lines.len();
                        self.execute_and_broadcast(Command::DeleteLines { lines });
                    }
                }
                Selection::Strokes(ids) => {
                    if !ids.is_empty() {
                        self.erase_drawing_strokes(ids.clone());
                    }
                }
                Selection::Detection(_) => {
                    // Routed through the semantic detection action before this
                    // legacy selection deletion path is reached.
                }
            }
            self.ui_shell.ui.clear_selection();
        }
        if deleted_lines > 0 {
            let key = if deleted_lines == 1 {
                "accessibility.line_deleted"
            } else {
                "accessibility.lines_deleted"
            };
            self.narration.announce_event(AccessibilityEvent::Success {
                message: crate::i18n::t(key).to_string(),
            });
        }
    }

    pub fn delete_lines_by_ids(&mut self, line_ids: Vec<u64>) {
        self.delete_lines_by_ids_internal(line_ids, true);
    }

    fn delete_lines_by_ids_internal(&mut self, line_ids: Vec<u64>, announce: bool) {
        let lines: Vec<_> = self
            .project_session
            .project
            .lines()
            .filter(|line| line_ids.contains(&line.id))
            .filter_map(|line| {
                self.project_session
                    .project
                    .line_index(line.id)
                    .map(|index| (line.clone(), index))
            })
            .collect();
        let deleted_lines = lines.len();
        if deleted_lines == 0 {
            return;
        }
        if deleted_lines == 1 {
            let (snapshot, index) = lines.into_iter().next().unwrap();
            self.execute_and_broadcast(Command::DeleteLine { snapshot, index });
        } else {
            self.execute_and_broadcast(Command::DeleteLines { lines });
        }
        if announce {
            self.announce_accessibility(AccessibilityEvent::Success {
                message: crate::i18n::t(if deleted_lines == 1 {
                    "accessibility.line_deleted"
                } else {
                    "accessibility.lines_deleted"
                })
                .to_string(),
            });
        }
    }

    pub fn copy_lines_by_ids(&mut self, line_ids: Vec<u64>, cut: bool) {
        let lines: Vec<LineClipboardEntry> = self
            .project_session
            .project
            .lines()
            .filter(|line| line_ids.contains(&line.id))
            .map(|line| LineClipboardEntry {
                line: line.clone(),
                detections: self
                    .project_session
                    .project
                    .detections()
                    .line(line.id)
                    .cloned(),
            })
            .collect();
        if lines.is_empty() {
            return;
        }
        let clipboard_text = lines
            .iter()
            .map(|entry| entry.line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.line_clipboard = Some(lines);
        crate::platform::clipboard_set(&clipboard_text);
        if cut {
            self.delete_lines_by_ids_internal(line_ids, false);
        }
    }

    pub fn copy_selected_line(&mut self) {
        use crate::workspaces::rythmo::view::Selection;
        let lines: Vec<RythmoLine> = match self.ui_shell.ui.rythmo_state().selected.as_ref() {
            Some(Selection::Line(id)) => self
                .project_session
                .project
                .get_line(*id)
                .cloned()
                .into_iter()
                .collect(),
            Some(Selection::Lines(ids)) => self
                .project_session
                .project
                .lines()
                .filter(|line| ids.contains(&line.id))
                .cloned()
                .collect(),
            Some(Selection::AllLines) => self.project_session.project.lines().cloned().collect(),
            _ => Vec::new(),
        };
        if !lines.is_empty() {
            let clipboard_text = lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            self.line_clipboard = Some(
                lines
                    .iter()
                    .map(|line| LineClipboardEntry {
                        line: line.clone(),
                        detections: self
                            .project_session
                            .project
                            .detections()
                            .line(line.id)
                            .cloned(),
                    })
                    .collect(),
            );
            crate::platform::clipboard_set(&clipboard_text);
            let key = if lines.len() == 1 {
                "accessibility.line_copied"
            } else {
                "accessibility.lines_copied"
            };
            self.narration.announce_event(AccessibilityEvent::Success {
                message: crate::i18n::t(key).to_string(),
            });
            return;
        }
        self.narration.announce_event(AccessibilityEvent::Error {
            message: crate::i18n::t("accessibility.no_line_selected").to_string(),
        });
    }

    pub fn cut_selected_line(&mut self) {
        self.copy_selected_line();
        self.delete_selected();
    }

    pub fn paste_line(&mut self) {
        let Some(snapshots) = self.line_clipboard.clone() else {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_clipboard").to_string(),
            });
            return;
        };
        let Some(first_snapshot) = snapshots.first() else {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_clipboard").to_string(),
            });
            return;
        };
        // Pasting follows the track currently under the mouse.  When the
        // pointer is outside the rythmo band, retain the keyboard-selected
        // track as the deterministic fallback used by keyboard operations.
        let target_track = self
            .ui_shell
            .ui
            .rythmo_state
            .hovered_track
            .unwrap_or(self.ui_shell.ui.rythmo_state.keyboard_track);
        let target_anchor_frame = self.current_frame();
        let source_anchor_frame = snapshots
            .iter()
            .map(|entry| entry.line.start_frame)
            .min()
            .unwrap_or(first_snapshot.line.start_frame);
        let source_anchor_track =
            crate::rythmo_layout::track_index_for_y_slot(first_snapshot.line.y_slot) as i32;
        let last_track = crate::rythmo_layout::track_count().saturating_sub(1) as i32;
        let source_track_offsets: Vec<i32> = snapshots
            .iter()
            .map(|entry| {
                crate::rythmo_layout::track_index_for_y_slot(entry.line.y_slot) as i32
                    - source_anchor_track
            })
            .collect();
        let min_offset = source_track_offsets.iter().copied().min().unwrap_or(0);
        let max_offset = source_track_offsets.iter().copied().max().unwrap_or(0);
        let target_anchor_track = (target_track as i32).clamp(-min_offset, last_track - max_offset);
        self.ui_shell.ui.rythmo_state.keyboard_track = target_anchor_track as usize;
        let pasted_count = snapshots.len();
        let base_index = self.project_session.project.line_count();
        let mut inserted_lines: Vec<(RythmoLine, usize)> = Vec::with_capacity(pasted_count);
        let mut pasted_detections = Vec::new();
        for (offset, entry) in snapshots.into_iter().enumerate() {
            let mut line = entry.line;
            let source_track = crate::rythmo_layout::track_index_for_y_slot(line.y_slot) as i32;
            let pasted_track = target_anchor_track + source_track - source_anchor_track;
            let old_start_frame = line.start_frame;
            line.id = loop {
                let id = self.project_session.project.generate_line_id();
                if inserted_lines.iter().all(|(inserted, _)| inserted.id != id) {
                    break id;
                }
            };
            line.start_frame = rebase_pasted_start_frame(
                line.start_frame,
                source_anchor_frame,
                target_anchor_frame,
            );
            if let Some(mut detections) = entry.detections {
                let delta = crate::detection::MediaTick::from_frame(
                    line.start_frame.saturating_sub(old_start_frame),
                );
                detections.shift_sync_points(delta);
                pasted_detections.push((line.id, detections));
            }
            line.y_slot = crate::rythmo_layout::y_slot_for_track_index(pasted_track as usize);
            inserted_lines.push((line, base_index + offset));
        }
        let pasted_ids: Vec<u64> = inserted_lines.iter().map(|(line, _)| line.id).collect();
        if inserted_lines.len() == 1 {
            let (snapshot, index) = inserted_lines.pop().unwrap();
            self.execute_and_broadcast(Command::InsertLine { snapshot, index });
        } else {
            self.execute_and_broadcast(Command::InsertLines {
                lines: inserted_lines,
            });
        }
        for (line_id, detections) in pasted_detections {
            self.project_session
                .project
                .restore_line_detections(line_id, detections);
        }
        self.ui_shell.ui.rythmo_state.selected = Some(if pasted_ids.len() == 1 {
            crate::workspaces::rythmo::view::Selection::Line(pasted_ids[0])
        } else {
            crate::workspaces::rythmo::view::Selection::Lines(pasted_ids)
        });
        let key = if pasted_count == 1 {
            "accessibility.line_pasted"
        } else {
            "accessibility.lines_pasted"
        };
        self.narration.announce_event(AccessibilityEvent::Success {
            message: crate::i18n::t(key).to_string(),
        });
    }

    pub fn add_drawing_stroke(&mut self, stroke: crate::rythmo_drawing::DrawingStroke) {
        self.execute_and_broadcast(Command::AddDrawingStroke { stroke });
    }

    pub fn erase_drawing_strokes(&mut self, ids: Vec<u64>) {
        let strokes: Vec<crate::rythmo_drawing::DrawingStroke> = ids
            .into_iter()
            .filter_map(|id| self.project_session.project.drawing().get(id).cloned())
            .collect();
        if !strokes.is_empty() {
            self.execute_and_broadcast(Command::EraseDrawingStrokes { strokes });
        }
    }

    pub fn transform_drawing_strokes(
        &mut self,
        stroke_ids: Vec<u64>,
        old_points: Vec<Vec<(f64, f32)>>,
        new_points: Vec<Vec<(f64, f32)>>,
    ) {
        let command = Command::TransformStrokes {
            stroke_ids,
            old_points,
            new_points,
        };
        if self
            .project_session
            .history
            .last_matches_strokes(match &command {
                Command::TransformStrokes { stroke_ids, .. } => stroke_ids,
                _ => unreachable!(),
            })
        {
            let final_points = match &command {
                Command::TransformStrokes { new_points, .. } => new_points.clone(),
                _ => unreachable!(),
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::TransformStrokes { new_points, .. } = last {
                        *new_points = final_points;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            EditExecutor::execute(&mut self.project_session, command, EditOrigin::Local);
        }
    }

    pub fn set_tool_mode(&mut self, mode: crate::ui::ToolMode) {
        self.ui_shell.ui.active_mode = Some(mode);
        if mode == crate::ui::ToolMode::Select {
            self.ui_shell.ui.erasing = false;
        }
        self.ui_shell.ui.rebuild_toolbar();
    }

    pub fn cycle_brush_size(&mut self) {
        self.ui_shell.ui.brush_radius_index = (self.ui_shell.ui.brush_radius_index + 1) % 3;
        self.ui_shell.ui.rebuild_toolbar();
    }

    pub fn toggle_eraser(&mut self) {
        self.ui_shell.ui.erasing = !self.ui_shell.ui.erasing;
        if self.ui_shell.ui.erasing {
            self.ui_shell.ui.active_mode = Some(crate::ui::ToolMode::Draw);
        }
        self.ui_shell.ui.rebuild_toolbar();
    }

    pub fn cycle_brush_color(&mut self, index: usize, color: [f32; 4]) {
        self.ui_shell.ui.brush_color_preset_index = index;
        self.ui_shell.ui.brush_color = color;
        self.ui_shell.ui.rebuild_toolbar();
    }

    pub fn open_brush_color_picker(&mut self) {
        let x = self.ui_shell.ui.cursor_pos.0;
        let y = self.ui_shell.ui.cursor_pos.1 + 40.0;
        self.ui_shell
            .ui
            .rythmo_state
            .color_picker
            .open(x, y, self.ui_shell.ui.brush_color);
        self.ui_shell.ui.brush_picking = true;
    }

    pub fn split_dialogue(&mut self) -> bool {
        let Some(target) = self.dialogue_split_target() else {
            self.show_toast(
                "SÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©lectionne un dialogue et place le curseur dedans.",
                3.0,
            );
            return false;
        };
        let line_id = match &target {
            DialogueSplitTarget::Cursor { line_id, .. }
            | DialogueSplitTarget::Playhead { line_id, .. } => *line_id,
        };
        let Some(old_line) = self.project_session.project.get_line(line_id).cloned() else {
            return false;
        };
        if old_line.duration_frames <= 1 {
            self.show_toast(
                "Dialogue trop court pour ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Âªtre coupÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â©.",
                3.0,
            );
            return false;
        }

        let lang = self.project_session.project.syllable_language_code();
        let split = match target {
            DialogueSplitTarget::Cursor { cursor_pos, .. } => {
                crate::syllable::split_dialogue_at_syllable_cursor(
                    &old_line.text,
                    &old_line.syllable_ratios,
                    lang,
                    cursor_pos,
                )
            }
            DialogueSplitTarget::Playhead { progress, .. } => {
                crate::syllable::split_dialogue_at_syllable_progress(
                    &old_line.text,
                    &old_line.syllable_ratios,
                    lang,
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
            .project_session
            .project
            .line_index(line_id)
            .unwrap_or_else(|| self.project_session.project.line_count());
        let second_index = old_index + 1;

        let mut first_line = old_line.clone();
        first_line.duration_frames = first_duration;
        first_line.text = split.first_text;
        first_line.syllable_ratios = split.first_ratios;

        let mut second_line = old_line.clone();
        second_line.id = self.project_session.project.generate_line_id();
        second_line.start_frame = old_line.start_frame + first_duration;
        second_line.duration_frames = second_duration;
        second_line.text = split.second_text;
        second_line.syllable_ratios = split.second_ratios;

        if self.project_session.project.get_line(line_id).is_none() {
            return false;
        }
        self.ui_shell.ui.rythmo_state.stop_line_editing();
        self.ui_shell.ui.rythmo_state.stop_char_editing();
        self.ui_shell.ui.rythmo_state.stop_note_editing();
        self.ui_shell.ui.rythmo_state.dragging = None;
        self.ui_shell.ui.rythmo_state.syllable_drag = None;
        self.ui_shell.ui.rythmo_state.context_menu = None;
        self.ui_shell.ui.rythmo_state.selected = Some(
            crate::workspaces::rythmo::view::Selection::Line(second_line.id),
        );

        self.execute_and_broadcast(Command::SplitLine {
            old_line,
            old_index,
            first_line,
            second_line,
            second_index,
        });
        true
    }

    fn dialogue_split_target(&self) -> Option<DialogueSplitTarget> {
        if let Some(line_id) = self.ui_shell.ui.rythmo_state.editing_line {
            return Some(DialogueSplitTarget::Cursor {
                line_id,
                cursor_pos: self.ui_shell.ui.rythmo_state.line_input.cursor_pos,
            });
        }
        if self.ui_shell.ui.rythmo_state.editing_character.is_some()
            || self.ui_shell.ui.rythmo_state.editing_note.is_some()
        {
            return None;
        }

        let frame = self.current_frame();
        let line_id = match self.ui_shell.ui.rythmo_state.selected {
            Some(crate::workspaces::rythmo::view::Selection::Line(line_id)) => Some(line_id),
            _ => self.ui_shell.ui.rythmo_state.hovered_line.or_else(|| {
                let mut active = self
                    .project_session
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

        let line = self.project_session.project.get_line(line_id)?;
        if frame <= line.start_frame || frame >= line.end_frame() {
            return None;
        }
        let progress =
            ((frame - line.start_frame) as f32 / line.duration_frames as f32).clamp(0.0, 1.0);
        Some(DialogueSplitTarget::Playhead { line_id, progress })
    }

    pub fn move_marker(&mut self, index: usize, frame: i64) {
        if index >= self.project_session.project.marker_count() {
            return;
        }
        let old_frame = self.project_session.project.marker(index).unwrap().frame;
        self.execute_and_broadcast(Command::MoveMarker {
            index,
            old_frame,
            new_frame: frame,
        });
    }

    pub fn add_marker(&mut self, kind: crate::rythmo_line::MarkerKind) {
        let frame = self.current_frame();
        let marker = crate::rythmo_line::RythmoMarker { kind, frame };
        let index = self.project_session.project.marker_count();
        self.execute_and_broadcast(Command::AddMarker { marker, index });
    }

    pub fn add_ambiance_line(&mut self, liaison: crate::rythmo_line::MarkerKind) {
        use crate::rythmo_line::{MarkerKind, RythmoLineKind};
        let frame = self.current_frame();
        let dur = (self.fps() * constants::DEFAULT_LINE_DURATION_SEC) as i64;
        let previous_ambiance_name = self
            .project_session
            .project
            .lines()
            .filter(|line| line.kind.is_ambiance() && !line.character_name.trim().is_empty())
            .map(|line| line.character_name.clone())
            .last()
            .unwrap_or_default();
        let (line_id, _) = EditExecutor::create_line(
            &mut self.project_session,
            frame,
            dur,
            crate::rythmo_layout::y_slot_for_track_index(0),
            String::new(),
        );
        if let Some(line) = self.project_session.project.get_line_mut(line_id) {
            line.kind = if matches!(liaison, MarkerKind::LiaisonRight) {
                RythmoLineKind::AmbianceStart
            } else {
                RythmoLineKind::AmbianceEnd
            };
            // Ambiance text never inherits a dialogue role or colour.
            line.character_name = previous_ambiance_name;
            line.character_color = [1.0, 1.0, 1.0, 1.0];
        }
        self.project_session.project.prune_unused_characters();
        // The create command snapshot must include the semantic kind for undo,
        // collaboration and project persistence.
        let index = self
            .project_session
            .project
            .line_index(line_id)
            .unwrap_or(0);
        if let Some(snapshot) = self.project_session.project.get_line(line_id).cloned() {
            let command = Command::CreateLine { snapshot, index };
            self.project_session
                .history
                .update_last(|last| *last = command.clone());
            let _ = self
                .project_session
                .transaction_journal
                .replace_last(command.clone());
            self.broadcast_delta(&command);
        }
        let rythmo_state = &mut self.ui_shell.ui.rythmo_state;
        rythmo_state.selected = Some(crate::workspaces::rythmo::view::Selection::Line(line_id));
        if matches!(liaison, MarkerKind::LiaisonRight) {
            let name = self
                .project_session
                .project
                .get_line(line_id)
                .map(|line| line.character_name.clone())
                .unwrap_or_default();
            rythmo_state.stop_line_editing();
            rythmo_state.editing_character = Some(line_id);
            rythmo_state.char_input.activate(&name);
            rythmo_state.char_input.select_all(&name);
            rythmo_state.autocomplete_index = None;
            rythmo_state.autocomplete_hover = None;
            rythmo_state.autocomplete_scroll = 0;
        } else {
            rythmo_state.stop_char_editing();
            rythmo_state.stop_note_editing();
            rythmo_state.start_editing_line(line_id, "");
            rythmo_state.line_input.select_all("");
        }
    }

    pub fn add_quick_line(&mut self, text: String) {
        let frame = self.current_frame();
        let dur = (self.fps() * 1.0) as i64; // 1 second
        let (_, command) =
            EditExecutor::create_line(&mut self.project_session, frame, dur, 0.0, text);
        self.broadcast_delta(&command);
    }

    pub fn create_line(&mut self, frame: i64, y_slot: f32) -> u64 {
        let default_dur = (self.fps() * constants::DEFAULT_LINE_DURATION_SEC) as i64;
        let dur = self
            .project_session
            .project
            .lines()
            .filter(|line| (line.y_slot - y_slot).abs() < 0.01 && line.start_frame > frame)
            .map(|line| line.start_frame)
            .min()
            .map(|start| (start - frame - constants::TICK_GAP_FRAMES).clamp(1, default_dur))
            .unwrap_or(default_dur);
        let (line_id, command) =
            EditExecutor::create_line(&mut self.project_session, frame, dur, y_slot, String::new());
        self.broadcast_delta(&command);
        line_id
    }

    pub fn create_line_at_track(&mut self, track: usize) -> u64 {
        let frame = self.current_frame();
        let y_slot = crate::rythmo_layout::y_slot_for_track_index(track.min(3));
        let id = self.create_line(frame, y_slot);
        self.narration.announce_event(AccessibilityEvent::Success {
            message: crate::i18n::t("accessibility.line_created").to_string(),
        });
        id
    }

    pub fn select_line_at_playhead(&mut self) -> Option<u64> {
        use crate::workspaces::rythmo::view::Selection;

        let frame = self.current_frame();
        let mut candidates: Vec<(usize, u64)> = self
            .project_session
            .project
            .lines()
            .filter(|line| line.start_frame <= frame && frame < line.end_frame())
            .map(|line| {
                (
                    crate::rythmo_layout::track_index_for_y_slot(line.y_slot),
                    line.id,
                )
            })
            .collect();
        candidates.sort_unstable();
        if candidates.is_empty() {
            self.ui_shell.ui.rythmo_state.selected = None;
            self.ui_shell.ui.rythmo_state.keyboard_cycle_frame = Some(frame);
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_at_cursor").to_string(),
            });
            return None;
        }

        let current = match self.ui_shell.ui.rythmo_state.selected {
            Some(Selection::Line(id)) => candidates
                .iter()
                .position(|(_, candidate)| *candidate == id),
            _ => None,
        };
        let next = if self.ui_shell.ui.rythmo_state.keyboard_cycle_frame == Some(frame) {
            current.map_or(0, |index| (index + 1) % candidates.len())
        } else {
            0
        };
        let (track, id) = candidates[next];
        self.ui_shell.ui.rythmo_state.keyboard_track = track;
        self.ui_shell.ui.rythmo_state.keyboard_cycle_frame = Some(frame);
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(id));
        self.announce_line(id);
        Some(id)
    }

    /// Select and jump to the previous or next line in timeline order.
    /// Navigation wraps so every line remains reachable from the keyboard.
    /// Without an existing selection, it starts on the line whose beginning
    /// is closest to the current playhead.
    pub fn navigate_lines(&mut self, direction: i32) -> Option<u64> {
        use crate::workspaces::rythmo::view::Selection;

        if direction == 0 {
            return None;
        }
        let mut lines: Vec<_> = self
            .project_session
            .project
            .lines()
            .enumerate()
            .map(|(order, line)| {
                (
                    line.start_frame,
                    crate::rythmo_layout::track_index_for_y_slot(line.y_slot),
                    order,
                    line.id,
                )
            })
            .collect();
        lines.sort_by_key(|(start_frame, track, order, _)| (*start_frame, *track, *order));
        if lines.is_empty() {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_lines").to_string(),
            });
            return None;
        }

        let current = self
            .selected_line_id()
            .and_then(|id| lines.iter().position(|(_, _, _, line_id)| *line_id == id));
        let playhead = self.current_frame();
        let nearest = lines
            .iter()
            .enumerate()
            .min_by_key(|(_, (start_frame, track, order, _))| {
                let opposite_direction = if direction < 0 {
                    *start_frame > playhead
                } else {
                    *start_frame < playhead
                };
                (
                    (*start_frame).abs_diff(playhead),
                    opposite_direction,
                    *track,
                    *order,
                )
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        let target_index = match (current, direction.signum()) {
            (Some(index), -1) => index.checked_sub(1).unwrap_or(lines.len() - 1),
            (Some(index), _) => (index + 1) % lines.len(),
            (None, _) => nearest,
        };
        let (start_frame, track, _, id) = lines[target_index];
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(id));
        self.ui_shell.ui.rythmo_state.keyboard_track = track;
        self.ui_shell.ui.rythmo_state.keyboard_cycle_frame = Some(start_frame);
        self.seek_absolute(start_frame);
        Some(id)
    }

    pub fn clear_line_selection(&mut self) -> bool {
        if !self.has_selected_lines() {
            return false;
        }
        self.ui_shell.ui.clear_selection();
        true
    }

    fn line_accessibility_label(&self, id: u64) -> String {
        self.project_session
            .project
            .get_line(id)
            .map(|line| {
                let character = if line.character_name.trim().is_empty() {
                    crate::i18n::t("accessibility.character").to_string()
                } else {
                    line.character_name.clone()
                };
                let dialogue = if line.text.trim().is_empty() {
                    crate::i18n::t("accessibility.line").to_string()
                } else {
                    line.text.clone()
                };
                let track = crate::rythmo_layout::track_index_for_y_slot(line.y_slot) + 1;
                let label = format!(
                    "{character}, {dialogue}, {} {track}",
                    crate::i18n::t("accessibility.track")
                );
                let label = if line.karaoke {
                    format!("{label}, {}", crate::i18n::t("accessibility.karaoke_line"))
                } else {
                    label
                };
                // Convention diagnostics are appended last so AccessKit reads
                // the normal line description before its line and zone issues.
                if let Some(suffix) = crate::lint::line_description_suffix(
                    &self.project_session.project,
                    self.fps(),
                    id,
                ) {
                    format!("{label}. {suffix}")
                } else {
                    label
                }
            })
            .unwrap_or_else(|| {
                format!(
                    "{}, {}, {}",
                    crate::i18n::t("accessibility.character"),
                    crate::i18n::t("accessibility.line"),
                    crate::i18n::t("accessibility.track")
                )
            })
    }

    pub fn selected_line_accessibility_label(&self) -> Option<String> {
        self.selected_line_id()
            .map(|id| self.line_accessibility_label(id))
    }

    pub fn announce_line(&self, id: u64) {
        self.narration
            .announce_event(AccessibilityEvent::Selection {
                label: self.line_accessibility_label(id),
            });
    }

    pub fn announce_character(&self, id: u64) {
        let label = self
            .project_session
            .project
            .get_line(id)
            .map(|line| line.character_name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| crate::i18n::t("accessibility.character").to_string());
        self.narration
            .announce_event(AccessibilityEvent::Selection { label });
    }

    pub fn announce_selected_line(&self) {
        if let Some(id) = self.selected_line_id() {
            self.announce_line(id);
        }
    }

    /// Move the selected line to the neighbouring rythmo track.
    ///
    /// Keeping this as a state-level operation means the keyboard shortcut
    /// follows the same reversible `MoveLine` command path as a mouse drag,
    /// while also giving screen-reader users a concise confirmation of the
    /// resulting track number.
    pub fn move_selected_line_track(&mut self, direction: i32) -> bool {
        let selected_ids = self.selected_line_ids();
        if selected_ids.is_empty() {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_selected").to_string(),
            });
            return false;
        }
        let selected_lines: Vec<_> = selected_ids
            .into_iter()
            .filter_map(|id| {
                self.project_session.project.get_line(id).map(|line| {
                    (
                        id,
                        line.start_frame,
                        crate::rythmo_layout::track_index_for_y_slot(line.y_slot),
                    )
                })
            })
            .collect();
        let Some((_, _, primary_track)) = selected_lines.first().copied() else {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_selected").to_string(),
            });
            return false;
        };
        let last_track = crate::rythmo_layout::track_count().saturating_sub(1);
        let track_delta = direction.signum();
        let can_move_group = selected_lines.iter().all(|(_, _, current_track)| {
            let target_track = *current_track as i32 + track_delta;
            (0..=last_track as i32).contains(&target_track)
        });
        if !can_move_group {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: format!(
                    "{} {}",
                    crate::i18n::t("accessibility.track_limit"),
                    primary_track + 1
                ),
            });
            return false;
        }

        let moves: Vec<_> = selected_lines
            .iter()
            .map(|(id, start_frame, current_track)| {
                let target_track = (*current_track as i32 + track_delta) as usize;
                (
                    *id,
                    *start_frame,
                    crate::rythmo_layout::y_slot_for_track_index(target_track),
                )
            })
            .collect();

        let primary_target_track = (primary_track as i32 + track_delta) as usize;
        if moves.len() == 1 {
            let (id, start_frame, y_slot) = moves[0];
            self.move_line(id, start_frame, y_slot);
        } else {
            self.move_lines(moves);
        }
        self.ui_shell.ui.rythmo_state.keyboard_track = primary_target_track;
        self.narration
            .announce_event(AccessibilityEvent::ValueChanged {
                label: crate::i18n::t("accessibility.track").to_string(),
                value: (primary_target_track + 1).to_string(),
            });
        true
    }

    /// Shift every selected line by the same number of frames while preserving
    /// durations, tracks and spacing. Moving left stops at frame zero for the
    /// whole group so a multi-selection never gets compressed.
    pub fn nudge_selected_lines(&mut self, delta_frames: i64) -> bool {
        if delta_frames == 0 {
            return false;
        }
        let selected_lines: Vec<_> = self
            .selected_line_ids()
            .into_iter()
            .filter_map(|id| {
                self.project_session
                    .project
                    .get_line(id)
                    .map(|line| (id, line.start_frame, line.y_slot))
            })
            .collect();
        if selected_lines.is_empty() {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_selected").to_string(),
            });
            return false;
        }

        let minimum_start = selected_lines
            .iter()
            .map(|(_, start_frame, _)| *start_frame)
            .min()
            .unwrap_or(0);
        let effective_delta = delta_frames.max(-minimum_start);
        if effective_delta != 0 {
            let moves: Vec<_> = selected_lines
                .iter()
                .map(|(id, start_frame, y_slot)| (*id, *start_frame + effective_delta, *y_slot))
                .collect();
            if moves.len() == 1 {
                let (id, start_frame, y_slot) = moves[0];
                self.move_line(id, start_frame, y_slot);
            } else {
                self.move_lines(moves);
            }
        }
        for (id, _, _) in &selected_lines {
            self.announce_line(*id);
        }
        effective_delta != 0
    }

    pub fn has_selected_lines(&self) -> bool {
        !self.selected_line_ids().is_empty()
    }

    fn selected_line_ids(&self) -> Vec<u64> {
        use crate::workspaces::rythmo::view::Selection;

        match self.ui_shell.ui.rythmo_state.selected.as_ref() {
            Some(Selection::Line(id)) => self
                .project_session
                .project
                .get_line(*id)
                .map(|_| vec![*id])
                .unwrap_or_default(),
            Some(Selection::Lines(ids)) => self
                .project_session
                .project
                .lines()
                .filter(|line| ids.contains(&line.id))
                .map(|line| line.id)
                .collect(),
            Some(Selection::AllLines) => self
                .project_session
                .project
                .lines()
                .map(|line| line.id)
                .collect(),
            Some(Selection::Marker(_) | Selection::Strokes(_) | Selection::Detection(_)) | None => {
                Vec::new()
            }
        }
    }

    fn selected_line_id(&self) -> Option<u64> {
        self.selected_line_ids().into_iter().next()
    }

    pub fn set_selected_line_start_at_playhead(&mut self) -> bool {
        let Some(id) = self.selected_line_id() else {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_selected").to_string(),
            });
            return false;
        };
        let Some(line) = self.project_session.project.get_line(id) else {
            return false;
        };
        let frame = self.current_frame();
        let end = line.end_frame();
        if frame >= end {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.invalid_line_limit").to_string(),
            });
            return false;
        }
        self.resize_line(id, frame, end - frame);
        true
    }

    pub fn set_selected_line_end_at_playhead(&mut self) -> bool {
        let Some(id) = self.selected_line_id() else {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.no_line_selected").to_string(),
            });
            return false;
        };
        let Some(line) = self.project_session.project.get_line(id) else {
            return false;
        };
        let frame = self.current_frame();
        if frame <= line.start_frame {
            self.narration.announce_event(AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.invalid_line_limit").to_string(),
            });
            return false;
        }
        self.resize_line(id, line.start_frame, frame - line.start_frame);
        true
    }

    pub fn start_editing_selected_line(&mut self) -> bool {
        let Some(id) = self.selected_line_id() else {
            return false;
        };
        self.start_editing_line(id);
        true
    }

    pub fn start_editing_selected_character(&mut self) -> bool {
        let Some(id) = self.selected_line_id() else {
            return false;
        };
        let Some(line) = self.project_session.project.get_line(id) else {
            return false;
        };
        let name = line.character_name.clone();
        self.ui_shell.ui.rythmo_state.selected =
            Some(crate::workspaces::rythmo::view::Selection::Line(id));
        self.ui_shell.ui.rythmo_state.editing_character = Some(id);
        self.ui_shell.ui.rythmo_state.char_input.activate(&name);
        self.ui_shell.ui.rythmo_state.char_input.select_all(&name);
        self.ui_shell.ui.rythmo_state.autocomplete_index = None;
        self.ui_shell.ui.rythmo_state.autocomplete_hover = None;
        self.ui_shell.ui.rythmo_state.autocomplete_scroll = 0;
        true
    }

    pub fn begin_keyboard_pan(&mut self, direction: i32) {
        let state = &mut self.ui_shell.ui.rythmo_state;
        state.keyboard_pan_direction = direction.signum();
        state.keyboard_pan_last_tick = Some(Instant::now());
        state.keyboard_pan_accum_px = 0.0;
    }

    pub fn end_keyboard_pan(&mut self) {
        let state = &mut self.ui_shell.ui.rythmo_state;
        state.keyboard_pan_direction = 0;
        state.keyboard_pan_last_tick = None;
        state.keyboard_pan_accum_px = 0.0;
        self.finish_seek();
        self.announce_current_timecode();
    }

    fn tick_keyboard_pan(&mut self) -> bool {
        let now = Instant::now();
        let state = &mut self.ui_shell.ui.rythmo_state;
        if state.keyboard_pan_direction == 0 {
            return false;
        }
        let last = state.keyboard_pan_last_tick.replace(now).unwrap_or(now);
        let elapsed = now.saturating_duration_since(last).as_secs_f32().min(0.05);
        let scroll_speed = crate::config::scroll_speed();
        state.keyboard_pan_accum_px +=
            state.keyboard_pan_direction as f32 * 240.0 * scroll_speed * elapsed;
        let ppf = crate::constants::PIXELS_PER_FRAME * scroll_speed;
        let frames = (state.keyboard_pan_accum_px / ppf).trunc() as i32;
        if frames == 0 {
            return false;
        }
        state.keyboard_pan_accum_px -= frames as f32 * ppf;
        self.seek_relative(frames);
        true
    }

    pub fn start_editing_line(&mut self, line_id: u64) {
        if let Some(line) = self.project_session.project.get_line(line_id) {
            let text = line.text.clone();
            self.ui_shell
                .ui
                .rythmo_state
                .start_editing_line(line_id, &text);
        }
    }

    pub fn move_line(&mut self, id: u64, start_frame: i64, y_slot: f32) {
        // Coalesce: update last command if same line drag
        if self
            .project_session
            .history
            .last_matches(id, CommandKind::MoveLine)
        {
            let Some(line) = self.project_session.project.get_line(id) else {
                return;
            };
            let command = Command::MoveLine {
                line_id: id,
                old_start: line.start_frame,
                old_y_slot: line.y_slot,
                new_start: start_frame,
                new_y_slot: y_slot,
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::MoveLine {
                        new_start,
                        new_y_slot,
                        ..
                    } = cmd
                    {
                        *new_start = start_frame;
                        *new_y_slot = y_slot;
                    }
                },
                EditOrigin::Local,
            );
        } else if let Some(line) = self.project_session.project.get_line(id) {
            let old_start = line.start_frame;
            let old_y = line.y_slot;
            self.execute_and_broadcast(Command::MoveLine {
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
            if self.project_session.project.get_line(line_id).is_some() {
                requested.push((line_id, new_start, new_y_slot));
            }
        }
        if requested.is_empty() {
            return;
        }

        let same_group = matches!(
            self.project_session.history.last(),
            Some(Command::MoveLines { moves })
                if moves.len() == requested.len()
                    && moves
                        .iter()
                        .zip(requested.iter())
                        .all(|(movement, (line_id, _, _))| movement.line_id == *line_id)
        );

        if same_group {
            let command_moves: Vec<_> = requested
                .iter()
                .filter_map(|(line_id, new_start, new_y_slot)| {
                    self.project_session
                        .project
                        .get_line(*line_id)
                        .map(|line| LineMove {
                            line_id: *line_id,
                            old_start: line.start_frame,
                            old_y_slot: line.y_slot,
                            new_start: *new_start,
                            new_y_slot: *new_y_slot,
                        })
                })
                .collect();
            EditExecutor::coalesce(
                &mut self.project_session,
                Command::MoveLines {
                    moves: command_moves,
                },
                |cmd| {
                    if let Command::MoveLines { moves } = cmd {
                        for (movement, (_, new_start, new_y_slot)) in
                            moves.iter_mut().zip(&requested)
                        {
                            movement.new_start = *new_start;
                            movement.new_y_slot = *new_y_slot;
                        }
                    }
                },
                EditOrigin::Local,
            );
            return;
        }

        let mut command_moves = Vec::new();
        for (line_id, new_start, new_y_slot) in requested {
            if let Some(line) = self.project_session.project.get_line(line_id) {
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

        self.execute_and_broadcast(Command::MoveLines {
            moves: command_moves,
        });
    }

    pub fn resize_line(&mut self, id: u64, start_frame: i64, duration_frames: i64) {
        if self
            .project_session
            .history
            .last_matches(id, CommandKind::ResizeLine)
        {
            let Some(line) = self.project_session.project.get_line(id) else {
                return;
            };
            let command = Command::ResizeLine {
                line_id: id,
                old_start: line.start_frame,
                old_dur: line.duration_frames,
                new_start: start_frame,
                new_dur: duration_frames,
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::ResizeLine {
                        new_start, new_dur, ..
                    } = cmd
                    {
                        *new_start = start_frame;
                        *new_dur = duration_frames;
                    }
                },
                EditOrigin::Local,
            );
        } else if let Some(line) = self.project_session.project.get_line(id) {
            let old_start = line.start_frame;
            let old_dur = line.duration_frames;
            self.execute_and_broadcast(Command::ResizeLine {
                line_id: id,
                old_start,
                old_dur,
                new_start: start_frame,
                new_dur: duration_frames,
            });
        }
    }

    pub fn update_line_text(&mut self, id: u64, text: String) {
        let ambiguous_sync_points = self
            .project_session
            .project
            .get_line(id)
            .map(|line| {
                self.project_session
                    .project
                    .detections()
                    .ambiguous_sync_point_count(id, &line.text, &text)
            })
            .unwrap_or(0);
        if ambiguous_sync_points > 0 {
            self.show_toast(
                format!(
                    "{} ({ambiguous_sync_points})",
                    crate::i18n::t("toast.sync_points_ambiguous")
                ),
                5.0,
            );
        }
        // Coalesce: update last text command for same line
        if self
            .project_session
            .history
            .last_matches(id, CommandKind::UpdateLineText)
        {
            let old_text = self
                .project_session
                .project
                .get_line(id)
                .map(|line| line.text.clone())
                .unwrap_or_default();
            let command = Command::UpdateLineText {
                line_id: id,
                old_text,
                new_text: text.clone(),
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::UpdateLineText { new_text, .. } = cmd {
                        *new_text = text;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            let old_text = self
                .project_session
                .project
                .get_line(id)
                .map(|l| l.text.clone())
                .unwrap_or_default();
            self.execute_and_broadcast(Command::UpdateLineText {
                line_id: id,
                old_text,
                new_text: text,
            });
        }
    }

    pub fn set_syllable_ratios(&mut self, line_id: u64, ratios: Vec<f32>) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let old_ratios = line.syllable_ratios.clone();
        if old_ratios == ratios {
            return;
        }

        self.execute_and_broadcast(Command::SetSyllableRatios {
            line_id,
            old_ratios,
            new_ratios: ratios,
        });
    }

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let is_ambiance = line.kind.is_ambiance();
        let name = if is_ambiance {
            crate::rythmo_line::ambiance_name(&name).to_string()
        } else {
            name
        };
        let color = if is_ambiance { [1.0; 4] } else { color };
        let old_name = line.character_name.clone();
        let old_color = line.character_color;
        let old_voice_actor_names = line.voice_actor_names.clone();
        let new_voice_actor_names = if is_ambiance {
            Vec::new()
        } else {
            self.voice_actor_names_for_character_change(line_id, &name)
        };
        if old_name == name && old_color == color && old_voice_actor_names == new_voice_actor_names
        {
            return;
        }

        self.execute_and_broadcast(Command::SetCharacter {
            line_id,
            old_name,
            old_color,
            old_voice_actor_names,
            new_name: name,
            new_color: color,
            new_voice_actor_names,
        });
    }

    fn voice_actor_names_for_character_change(&self, line_id: u64, name: &str) -> Vec<String> {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return Vec::new();
        };
        if line.character_name == name {
            line.voice_actor_names.clone()
        } else {
            self.project_session
                .project
                .voice_actor_names_for_character(name, line_id)
        }
    }

    pub fn set_character_color(&mut self, line_id: u64, color: [f32; 4]) {
        if self
            .project_session
            .history
            .last_matches(line_id, CommandKind::SetCharacterColor)
        {
            let old_color = self
                .project_session
                .project
                .get_line(line_id)
                .map(|line| line.character_color)
                .unwrap_or_default();
            let command = Command::SetCharacterColor {
                line_id,
                old_color,
                new_color: color,
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::SetCharacterColor { new_color, .. } = cmd {
                        *new_color = color;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            let old_color = self
                .project_session
                .project
                .get_line(line_id)
                .map(|l| l.character_color)
                .unwrap_or_default();
            self.execute_and_broadcast(Command::SetCharacterColor {
                line_id,
                old_color,
                new_color: color,
            });
        }
    }

    pub fn update_character_name(&mut self, line_id: u64, name: String) {
        let Some(current_line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let name = if current_line.kind.is_ambiance() {
            crate::rythmo_line::ambiance_name(&name).to_string()
        } else {
            name
        };
        let old_name = current_line.character_name.clone();
        let old_color = current_line.character_color;
        let old_voice_actor_names = current_line.voice_actor_names.clone();
        let new_voice_actor_names = match self.project_session.history.last() {
            Some(Command::SetCharacter {
                line_id: command_line_id,
                old_name,
                old_voice_actor_names,
                ..
            }) if *command_line_id == line_id && old_name == &name => old_voice_actor_names.clone(),
            _ => self.voice_actor_names_for_character_change(line_id, &name),
        };
        let known_color = self
            .project_session
            .project
            .known_characters()
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.color);

        // Coalesce character name edits
        if self
            .project_session
            .history
            .last_matches(line_id, CommandKind::SetCharacter)
        {
            let final_color = known_color.unwrap_or_else(|| {
                self.project_session
                    .project
                    .get_line(line_id)
                    .map(|l| l.character_color)
                    .unwrap_or_default()
            });
            let command = Command::SetCharacter {
                line_id,
                old_name: old_name.clone(),
                old_color,
                old_voice_actor_names: old_voice_actor_names.clone(),
                new_name: name.clone(),
                new_color: final_color,
                new_voice_actor_names: new_voice_actor_names.clone(),
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
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
                },
                EditOrigin::Local,
            );
        } else {
            let final_color = known_color.unwrap_or_else(|| {
                self.project_session
                    .project
                    .get_line(line_id)
                    .map(|l| l.character_color)
                    .unwrap_or_default()
            });
            self.execute_and_broadcast(Command::SetCharacter {
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

    pub fn finalize_character(&mut self, _line_id: u64) {
        // SetCharacter is applied through EditExecutor when the edit is
        // emitted; this hook remains for the existing dispatcher sequence.
    }

    pub fn create_voice_actor(&mut self, name: String, icon_path: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.show_toast(crate::i18n::t("toast.voice_actor_name_required"), 4.0);
            return;
        }
        if self
            .project_session
            .project
            .find_voice_actor(&name)
            .is_some()
        {
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
        self.execute_and_broadcast(Command::CreateVoiceActor { actor });
        self.show_toast(crate::i18n::t("toast.voice_actor_created"), 3.0);
    }

    pub fn set_voice_actor_modal_icon_path(&mut self, path: impl Into<String>) {
        self.ui_shell.ui.set_voice_actor_modal_icon_path(path);
    }

    pub fn assign_voice_actor_to_line(&mut self, line_id: u64, actor_name: String) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
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
        let Some(line) = self.project_session.project.get_line(line_id) else {
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
            .project_session
            .project
            .get_line(line_id)
            .map(|line| line.character_name.clone())
            .filter(|name| !name.trim().is_empty())
        else {
            return;
        };

        let changes = self
            .project_session
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

        self.execute_and_broadcast(Command::SetVoiceActors { changes });
    }

    pub fn start_editing_note(&mut self, line_id: u64) {
        let note = self
            .project_session
            .project
            .get_line(line_id)
            .map(|l| l.note.clone())
            .unwrap_or_default();
        let text = if note.is_empty() {
            "Note".to_string()
        } else {
            note
        };
        self.ui_shell
            .ui
            .rythmo_state
            .start_editing_note(line_id, &text);
        if self
            .project_session
            .project
            .get_line(line_id)
            .map(|l| l.note.is_empty())
            .unwrap_or(true)
        {
            self.execute_and_broadcast(Command::UpdateLineNote {
                line_id,
                old_note: String::new(),
                new_note: "Note".to_string(),
            });
        }
    }

    pub fn start_editing_note_selected(&mut self) {
        if let Some(id) = self.selected_line_id() {
            self.start_editing_note(id);
        }
    }

    pub fn update_line_note(&mut self, id: u64, note: String) {
        use crate::command::{Command, CommandKind};
        if self
            .project_session
            .history
            .last_matches(id, CommandKind::UpdateLineNote)
        {
            let old_note = self
                .project_session
                .project
                .get_line(id)
                .map(|line| line.note.clone())
                .unwrap_or_default();
            let command = Command::UpdateLineNote {
                line_id: id,
                old_note,
                new_note: note.clone(),
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::UpdateLineNote { new_note, .. } = cmd {
                        *new_note = note;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            let old_note = self
                .project_session
                .project
                .get_line(id)
                .map(|l| l.note.clone())
                .unwrap_or_default();
            self.execute_and_broadcast(Command::UpdateLineNote {
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
        if let Err(e) = JsonExporter.export(&self.project_session.project, fps, &path) {
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
                EditExecutor::apply_import(&mut self.project_session, data, fps);
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
        if let Some(player) = &mut self.playback.video_player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.render.ui_renderer.texture_bind_group_layout(),
                self.render.ui_renderer.texture_sampler(),
            );
            player.tick(
                &self.render.gfx.device,
                &self.render.gfx.queue,
                bgl,
                sampler,
            );
            if player.current_frame() != prev_frame {
                self.playback.timeline.emit(TimelineEvent::FrameChanged {
                    frame: player.current_frame(),
                });
            }
            if !player.is_playing() && self.ui_shell.ui.is_playing() {
                self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui_shell.ui.toggle_play_pause();
            }
        }
    }

    fn poll_proxy_job(&mut self) -> bool {
        let result = match self.jobs.pending_proxy_job.as_ref() {
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

        let Some(job) = self.jobs.pending_proxy_job.take() else {
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
                    if self.load_video_for_playback(
                        &job.source_path,
                        Some(&proxy_path),
                        Some(frame),
                    ) {
                        self.project_session.dirty = true;
                        self.show_toast(crate::i18n::t("toast.proxy_created"), 4.0);
                    }
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
        let result = match self.jobs.pending_export_job.as_ref() {
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

        self.jobs.pending_export_job = None;
        self.set_export_progress(None);
        match result {
            Ok(()) => {
                log::info!("Export completed");
                self.show_toast(crate::i18n::t("toast.export_completed"), 4.0);
            }
            Err(e) => {
                if crate::video_export::is_cancelled_error(&e) {
                    log::info!("Export canceled");
                } else {
                    log::error!("Export failed: {e}");
                    self.show_toast(
                        format!("{} {e}", crate::i18n::t("toast.export_failed")),
                        8.0,
                    );
                }
            }
        }

        true
    }

    fn poll_import_job(&mut self) -> bool {
        let result = match self.jobs.pending_import_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Import job disconnected".into())),
            },
            None => None,
        };

        let Some(result) = result else {
            return false;
        };

        let Some(job) = self.jobs.pending_import_job.take() else {
            return false;
        };
        self.ui_shell.ui.loading_project = None;

        match result {
            Ok(mut loaded) => {
                let is_legacy_json = loaded.is_legacy_json();
                let loaded_huuid = loaded.huuid.clone();
                let loaded_transaction_journal = loaded.transaction_journal.clone();
                let loaded_recording = loaded.recording.take();
                let bundled_source = loaded.source_video_path.clone();
                let bundled_proxy = loaded.proxy_video_path.clone();
                if let Some(source) = bundled_source.as_deref() {
                    if !self.load_video_for_playback(source, bundled_proxy.as_deref(), None) {
                        let message = crate::i18n::t("toast.import_video_failed");
                        log::error!("{message} {}", source.display());
                        self.show_toast(message, 7.0);
                        self.narration.announce_event(AccessibilityEvent::Error {
                            message: crate::i18n::t("accessibility.project_load_failed")
                                .to_string(),
                        });
                        return true;
                    }
                }

                crate::vector_text::clear_project_font();
                if let Some(font_path) = loaded.font_asset_path.as_deref() {
                    if let Some(family) = crate::vector_text::register_project_font_file(font_path)
                    {
                        log::info!("Loaded bundled rythmo font: {family}");
                    } else {
                        log::warn!("Bundled font could not be loaded: {}", font_path.display());
                    }
                }
                let fps = self.fps();
                loaded
                    .project_data
                    .apply_to_project(&mut self.project_session.project, fps);
                self.project_session.history.clear();
                self.project_session.transaction_journal = loaded_transaction_journal
                    .unwrap_or_else(|| {
                        crate::project_metadata::TransactionJournal::from_project(
                            &self.project_session.project,
                            fps,
                        )
                        .expect("a loaded project must form a valid transaction checkpoint")
                    });
                if let Some(recording) = loaded_recording {
                    self.project_session.recording_project = recording.project;
                    self.project_session.recording_transactions = recording.transaction_log;
                    self.project_session.recording_asset_paths = recording.audio_asset_paths;
                    self.project_session.recording_revision = 0;
                } else {
                    self.project_session.reset_recording_document(fps);
                }
                self.recording_runtime = crate::recording_runtime::RecordingRuntime::new();
                self.project_session.dirty = false;
                self.sync_audio_settings_to_player();
                self.project_session.project_path = if is_legacy_json {
                    None
                } else {
                    Some(job.br_path.clone())
                };
                self.project_session.huuid = if is_legacy_json { None } else { loaded_huuid };
                if is_legacy_json {
                    self.show_toast(crate::i18n::t("toast.legacy_project_loaded"), 6.0);
                }
                self.project_session.render_index = crate::render_index::ProjectRenderIndex::new();
                self.render.ui_renderer.clear_text_cache();
                if is_legacy_json {
                    if let Some(video) = self.video_path() {
                        crate::config::add_recent_project(video, job.br_path.clone());
                    }
                } else {
                    crate::config::add_recent_project(job.br_path.clone(), job.br_path.clone());
                }
                self.project_session.loaded_project = None;
                if !is_legacy_json {
                    self.project_session.loaded_project = Some(loaded);
                }
                self.rebuild_topbar_for_network();
                log::info!("Project imported from {}", job.br_path.display());
                self.narration.announce_event(AccessibilityEvent::Success {
                    message: format!(
                        "{} {}",
                        crate::i18n::t("accessibility.project_loaded"),
                        job.br_path
                            .file_stem()
                            .map(|name| name.to_string_lossy())
                            .unwrap_or_default()
                    ),
                });
            }
            Err(e) => {
                log::error!("Import failed: {e}");
                self.show_toast(
                    format!("{} {e}", crate::i18n::t("toast.import_failed")),
                    6.0,
                );
                self.narration.announce_event(AccessibilityEvent::Error {
                    message: format!(
                        "{} {}",
                        crate::i18n::t("accessibility.project_load_failed"),
                        e
                    ),
                });
            }
        }

        true
    }

    fn poll_save_job(&mut self) -> bool {
        let result = match self.jobs.pending_save_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Project save job disconnected".into()))
                }
            },
            None => None,
        };
        let Some(result) = result else {
            return false;
        };
        let Some(job) = self.jobs.pending_save_job.take() else {
            return false;
        };

        match result {
            Ok(metadata) => {
                let current_font = crate::vector_text::selected_font_asset().map(|(_, path)| path);
                let snapshot_is_current = self.project_session.project.revision()
                    == job.saved_revision
                    && self.project_session.recording_revision == job.saved_recording_revision
                    && self.video_path().as_ref() == Some(&job.source_video)
                    && self.playback.proxy_video_path == job.proxy_video
                    && current_font.as_ref() == Some(&job.font_asset);

                self.project_session.project_path = Some(job.path.clone());
                self.project_session.huuid = Some(metadata.huuid);
                if snapshot_is_current {
                    if let Some(journal) = metadata.transaction_journal {
                        self.project_session.transaction_journal = journal;
                    }
                    self.project_session.dirty = false;
                }
                crate::config::add_recent_project(job.path.clone(), job.path.clone());
                self.rebuild_topbar_for_network();
                self.show_toast(crate::i18n::t("toast.saved"), 4.0);
                log::info!("Project saved to {}", job.path.display());

                if job.continuation != SaveContinuation::None {
                    if snapshot_is_current {
                        self.jobs.transition_after_save_ready = Some(job.continuation);
                    } else {
                        self.show_toast(
                            crate::i18n::t("toast.transition_canceled_after_edit"),
                            7.0,
                        );
                    }
                }
            }
            Err(error) => {
                log::error!("Project save failed: {error}");
                self.show_toast(
                    format!("{} {error}", crate::i18n::t("toast.save_failed")),
                    8.0,
                );
            }
        }
        true
    }

    fn poll_recording_runtime(&mut self) -> bool {
        use crate::recording_runtime::RecordingRuntimeEvent;
        use crate::ui::recording_workspace::RecordingRole;

        let event = self.recording_runtime.tick();
        let mut changed = self.recording_runtime.is_active();
        match event {
            RecordingRuntimeEvent::None => {}
            RecordingRuntimeEvent::CountdownStarted => changed = true,
            RecordingRuntimeEvent::CaptureStarted { target } => {
                self.seek_absolute(target.start_frame);
                self.finish_seek();
                if self
                    .playback
                    .video_player
                    .as_ref()
                    .is_some_and(|player| !player.is_playing())
                {
                    self.toggle_play_pause();
                }
                if self.ui_shell.ui.recording_role().is_online() {
                    self.collaboration
                        .network
                        .send_recording_playback(target.start_frame, true);
                }
                self.announce_accessibility(AccessibilityEvent::Activation {
                    label: crate::i18n::t("recording.capture.active").to_string(),
                });
                changed = true;
            }
            RecordingRuntimeEvent::Finalizing { .. } => changed = true,
            RecordingRuntimeEvent::Cancelled => {
                self.show_toast(crate::i18n::t("recording.capture.cancelled"), 3.0);
                changed = true;
            }
            RecordingRuntimeEvent::Failed { message } => {
                self.recording_error(message);
                changed = true;
            }
            RecordingRuntimeEvent::Finished { completed, path } => {
                let target = completed.target;
                let audio = completed.audio.clone();
                let role = self.ui_shell.ui.recording_role();
                let commits_locally = matches!(role, RecordingRole::Solo | RecordingRole::Director);

                if commits_locally {
                    let operation = completed.clone().into_project_operation(
                        self.project_session.recording_project.timeline_fps(),
                    );
                    match self.apply_recording_operation(operation) {
                        Ok(()) => {
                            self.project_session
                                .recording_asset_paths
                                .insert(target.asset_id, path.clone());
                        }
                        Err(error) => {
                            self.recording_error(error.to_string());
                            self.sync_recording_workspace_ui();
                            return true;
                        }
                    }
                }

                if role.is_online() {
                    let transfer_id = format!(
                        "take_{}_{}",
                        target.asset_id.get(),
                        audio.checksum.chars().take(12).collect::<String>()
                    );
                    match crate::audio_transfer::AudioTransferMetadata::from_file(
                        transfer_id,
                        &path,
                        target,
                        audio,
                    ) {
                        Ok(metadata) => {
                            let _ = self.collaboration.network.send_audio_file(path, metadata);
                        }
                        Err(error) => self.recording_error(error),
                    }
                }

                if self
                    .playback
                    .video_player
                    .as_ref()
                    .is_some_and(|player| player.is_playing())
                {
                    self.toggle_play_pause();
                }
                self.show_toast(crate::i18n::t("recording.capture.finished"), 4.0);
                self.announce_accessibility(AccessibilityEvent::Success {
                    message: crate::i18n::t("recording.capture.finished").to_string(),
                });
                changed = true;
            }
        }

        if changed && self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
        changed
    }

    pub fn tick_background(&mut self) -> bool {
        let mut changed = false;

        changed |= self.tick_keyboard_pan();
        changed |= self.poll_recording_runtime();

        if let Ok(mut results) = self.collaboration.ping_results.try_lock() {
            for r in results.drain(..) {
                if let Some(browser) = self.ui_shell.ui.server_browser_mut() {
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
        if self.project_session.dirty && self.last_autosave.elapsed().as_secs() >= 60 {
            self.save_backup();
            self.last_autosave = Instant::now();
        }

        if let Some(progress) = self.ui_shell.ui.export_progress.clone() {
            use std::sync::atomic::Ordering;
            let v = f32::from_bits(progress.load(Ordering::Relaxed));
            if v <= 1.0 {
                let percent = (v.clamp(0.0, 1.0) * 100.0) as u32;
                if self.last_progress_percent != Some(percent) {
                    self.last_progress_percent = Some(percent);
                    self.narration
                        .publish_progress(self.active_progress_label(), Some(percent));
                    #[cfg(target_os = "windows")]
                    // A screen reader receives the persistent AccessKit
                    // progress node; an additional beep would be redundant.
                    if !self.narration.is_enabled() {
                        crate::accessibility::progress_tone(percent);
                    }
                }
                let now = Instant::now();
                if self
                    .last_progress_announcement
                    .as_ref()
                    .is_some_and(|last| now.duration_since(*last) >= Duration::from_secs(60))
                {
                    self.last_progress_announcement = Some(now);
                    self.announce_shortcut_accessibility(AccessibilityEvent::Activation {
                        label: format!(
                            "{} : {percent} {}",
                            self.active_progress_label(),
                            crate::i18n::t("progress.percent")
                        ),
                    });
                }
            }
            if v >= 1.5 {
                // Sentinel: 2.0 means the worker thread has actually exited.
                self.set_export_progress(None);
                log::info!("Export completed");
                changed = true;
            }
        }

        changed |= self.tick_network();
        changed |= self.tick_scroll_decode();
        changed |= self.poll_export_job();
        changed |= self.poll_proxy_job();
        changed |= self.poll_import_job();
        changed |= self.poll_save_job();
        changed |= self.poll_file_explorer();
        changed |= self.poll_waveform_change();
        changed
    }

    fn poll_waveform_change(&mut self) -> bool {
        let revision = self.current_waveform_revision();
        if revision != self.playback.last_waveform_revision {
            self.playback.last_waveform_revision = revision;
            return true;
        }
        false
    }

    fn current_waveform_revision(&self) -> u64 {
        self.playback
            .video_player
            .as_ref()
            .map(|player| player.waveform_revision())
            .unwrap_or(0)
    }

    fn waveform_redraw_pending(&self) -> bool {
        self.current_waveform_revision() != self.playback.last_waveform_revision
    }

    fn waveform_decode_pending(&self) -> bool {
        self.playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.is_waveform_decoding())
    }

    pub fn display_refresh_interval(&self) -> Duration {
        self.render.refresh_interval
    }

    /// The scrolling bande rythmo owns a 240 Hz animation clock.  Do not tie
    /// this cadence to decoded video frames or to the monitor refresh rate.
    pub fn rythmo_refresh_interval(&self) -> Duration {
        constants::RYTHMO_RENDER_INTERVAL
    }

    fn active_animation_interval(&self) -> Duration {
        if self.is_video_playing() && self.active_workspace() == WorkspaceId::Rythmo {
            self.rythmo_refresh_interval()
        } else {
            self.display_refresh_interval()
        }
    }
    fn scroll_decode_due(&self, now: Instant) -> bool {
        self.playback.scroll_needs_decode
            && self.playback.last_scroll_time.is_some_and(|last| {
                now.duration_since(last).as_millis() >= constants::SCROLL_DECODE_DELAY_MS
            })
    }

    fn periodic_redraw_due(&self, now: Instant) -> bool {
        if self.ui_shell.ui.has_active_progress()
            || self.jobs.pending_proxy_job.is_some()
            || self.jobs.pending_import_job.is_some()
            || self.jobs.pending_save_job.is_some()
        {
            return now.duration_since(self.render.last_redraw) >= Duration::from_millis(100);
        }

        if self.ui_shell.ui.is_editing_text() {
            return self
                .ui_shell
                .ui
                .next_cursor_blink_deadline()
                .is_some_and(|deadline| deadline <= now)
                || now.duration_since(self.render.last_redraw) >= Duration::from_millis(500);
        }

        false
    }

    fn continuous_redraw_due(&self, now: Instant) -> bool {
        (self.needs_continuous_redraw() || self.secondary_needs_continuous_redraw())
            && now.saturating_duration_since(self.render.last_redraw)
                >= self.active_animation_interval()
    }
    pub fn needs_redraw_now(&self) -> bool {
        let now = Instant::now();
        self.scroll_decode_due(now)
            || self.periodic_redraw_due(now)
            || self.waveform_redraw_pending()
            || self.continuous_redraw_due(now)
    }

    pub fn needs_continuous_redraw(&self) -> bool {
        self.is_video_playing()
            || self
                .playback
                .video_player
                .as_ref()
                .is_some_and(|player| player.is_preparing_frame())
            || self.recording_runtime.is_active()
            || self.ui_shell.ui.needs_animation_or_interaction()
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
            push_deadline(self.render.last_redraw + self.active_animation_interval());
        }

        if self.ui_shell.ui.has_active_progress()
            || self.jobs.pending_proxy_job.is_some()
            || self.jobs.pending_import_job.is_some()
            || self.jobs.pending_save_job.is_some()
        {
            push_deadline(self.render.last_redraw + Duration::from_millis(100));
        }

        if self.ui_shell.ui.is_editing_text() {
            if let Some(cursor_deadline) = self.ui_shell.ui.next_cursor_blink_deadline() {
                push_deadline(cursor_deadline);
            } else {
                push_deadline(self.render.last_redraw + Duration::from_millis(500));
            }
        }

        if self.playback.scroll_needs_decode {
            if let Some(last_scroll) = self.playback.last_scroll_time {
                push_deadline(
                    last_scroll + Duration::from_millis(constants::SCROLL_DECODE_DELAY_MS as u64),
                );
            }
        }

        if self.project_session.dirty {
            push_deadline(self.last_autosave + Duration::from_secs(60));
        }

        if self.waveform_decode_pending() || self.waveform_redraw_pending() {
            push_deadline(now + Duration::from_millis(100));
        }

        if self.collaboration.network.state != ConnectionState::Disconnected
            || self.ui_shell.ui.needs_background_poll()
        {
            push_deadline(now + Duration::from_millis(100));
        }

        deadline
    }

    pub fn render(&mut self) {
        // Pace from frame start, so CPU/GPU preparation is part of the display
        // budget. Pacing from the end added a full refresh interval after the
        // render cost and produced severe judder on large projects.
        self.render.last_redraw = Instant::now();
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
        let surface_texture = match self.render.gfx.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.render
                    .gfx
                    .surface
                    .configure(&self.render.gfx.device, &self.render.gfx.config);
                return;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return,
            _ => return,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.render
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
        let _events = self.playback.timeline.drain();

        self.tick_video();
        self.apply_automation_if_needed();

        // Video quad ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â skip when export modal is showing (it would cover the modal)
        let video_quad = if self.ui_shell.ui.export_progress.is_some()
            || self.window_manager.secondary_display.is_some()
            || self.ui_shell.ui.automation_open()
        {
            None
        } else {
            build_video_quad(&self.playback.video_player, &self.ui_shell.ui)
        };
        let current_frame = self.current_frame();
        let render_frame = self.render_frame();

        // UI render. Keep a read guard instead of cloning the waveform every frame.
        let waveform_arc = self
            .playback
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
        self.project_session
            .render_index
            .refresh(&self.project_session.project);
        self.ui_shell.ui.render(
            &mut self.render.ui_renderer,
            &self.render.gfx.device,
            &self.render.gfx.queue,
            &mut encoder,
            &view,
            self.render.gfx.config.width,
            self.render.gfx.config.height,
            self.ui_scale,
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &self.project_session.project,
            &self.project_session.render_index,
            current_frame,
            render_frame,
            fps,
            waveform,
            waveform_offset_frames,
            waveform_is_instrumental,
        );

        self.render
            .gfx
            .queue
            .submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }

    pub fn render_secondary_display(&mut self, window_id: WindowId) {
        self.render.last_redraw = Instant::now();
        self.tick_video();

        let Some(display) = &mut self.window_manager.secondary_display else {
            return;
        };
        if display.window.id() != window_id {
            return;
        }

        let surface_texture = match display.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(tex) | CurrentSurfaceTexture::Suboptimal(tex) => tex,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                display
                    .surface
                    .configure(&self.render.gfx.device, &display.config);
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
        let mut encoder =
            self.render
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
            build_full_window_video_quad(&self.playback.video_player, width as f32, height as f32);
        self.render.ui_renderer.render(
            &self.render.gfx.device,
            &self.render.gfx.queue,
            &mut encoder,
            &view,
            width,
            height,
            1.0,
            &[],
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
            &[], // no modal textured quads
            &[], // no modal quads
            &[], // no modal labels
            &[], // no modal overlay quads
            &[], // no modal overlay labels
        );

        self.render
            .gfx
            .queue
            .submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }
}

fn build_video_quad<'a>(
    video_player: &'a Option<VideoPlayer>,
    ui: &Ui,
) -> Option<(&'a wgpu::BindGroup, crate::ui::primitives::IconInstance)> {
    let player = video_player.as_ref()?;
    let bind_group = player.bind_group.as_ref()?;
    let (vid_w, vid_h) = player.video_size()?;
    let preview = ui.video_preview_rect();

    let vid_aspect = vid_w as f32 / vid_h as f32;
    let zone_aspect = preview.width / preview.height.max(1.0);
    let (draw_w, draw_h) = if vid_aspect > zone_aspect {
        (preview.width, preview.width / vid_aspect)
    } else {
        (preview.height * vid_aspect, preview.height)
    };

    Some((
        bind_group,
        crate::ui::primitives::IconInstance {
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

fn build_full_window_video_quad(
    video_player: &Option<VideoPlayer>,
    screen_w: f32,
    screen_h: f32,
) -> Option<(&wgpu::BindGroup, crate::ui::primitives::IconInstance)> {
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
        crate::ui::primitives::IconInstance {
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
    password: String,
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
        .auth(serde_json::json!({ "password": password }))
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
            // Server disconnected us ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â if we didn't get info, mark offline
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
