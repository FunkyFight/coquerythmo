use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use wgpu::CurrentSurfaceTexture;
use winit::window::{Fullscreen, Window, WindowId};

use std::time::{Duration, Instant};

use crate::accessibility::{AccessibilityEvent, NarrationService};
use crate::application::collaboration_service::{CollaborationSession, PingResult};
use crate::application::context::AppContext;
use crate::application::delta_codec::{decode_delta, encode_delta};
use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::application::job_service::{
    JobManager, PendingExportJob, PendingImportJob, PendingProxyJob, PendingRecordingMixJob,
    PendingSaveJob, SaveContinuation,
};
use crate::application::playback_service::PlaybackSession;
use crate::application::project_service::ProjectSession;
use crate::application::render_service::RenderCoordinator;
use crate::application::ui_shell::UiShell;
use crate::application::window_service::{SecondaryWindowKind, WindowManager};
use crate::application::workspace_service::{WorkspaceHost, WorkspaceId};
use crate::command::{Command, CommandKind, LineMove};
use crate::network::{
    ConnectionState, IncomingMessage, ProjectTransferMetadata, ProjectTransferStatus,
};
use crate::observer::TimelineEvent;
use crate::packet::{CommandPayload, Packet, ProjectData};
use crate::project::{Character, LineCharacterNameChange, Project};
use crate::project_archive::{ProjectLoadProgress, ProjectLoadStage};
use crate::protocol::{ProtocolKind, ProtocolPayload};

// Marker only — the real "close project with protocol continuation" logic
// lives in `crate::app::dispatcher::protocol_close_current_project`, which
// has access to the save helpers. State just tracks the pending flow and
// polls its stages.
use crate::rythmo_line::RythmoLine;
use crate::ui::primitives::{EventResponse, UiEvent};
use crate::ui::Ui;
use crate::video::{AudioTrack, VideoPlayer};
use crate::voice_actor::{LineVoiceActorsChange, VoiceActor};
use crate::workspaces::comic_dubs::ComicDubsWorkspace;
use crate::workspaces::recording::RecordingWorkspace;
use crate::workspaces::rythmo::RythmoWorkspace;
use crate::workspaces::voicelines::VoicelinesWorkspace;

use crate::constants;
use crate::recording_mix::REALTIME_SAMPLE_RATE;

enum DialogueSplitTarget {
    Cursor { line_id: u64, cursor_pos: usize },
    Playhead { line_id: u64, progress: f32 },
}

fn rebase_pasted_start_frame(source_start: i64, source_anchor: i64, target_anchor: i64) -> i64 {
    target_anchor.saturating_add(source_start.saturating_sub(source_anchor))
}

fn recording_playback_waits_for_mix(
    workspace: WorkspaceId,
    mix_pending: bool,
    player_is_playing: bool,
    capture_started: bool,
) -> bool {
    workspace == WorkspaceId::Recording && mix_pending && !player_is_playing && !capture_started
}

fn recording_playback_is_blocked_during_countdown(
    capture: Option<&crate::recording::CaptureState>,
) -> bool {
    matches!(
        capture,
        Some(crate::recording::CaptureState::Countdown { .. })
    )
}

fn workspace_shows_project_video(workspace: WorkspaceId) -> bool {
    matches!(workspace, WorkspaceId::Rythmo | WorkspaceId::Recording)
}

fn next_comic_dubs_position(
    project: &crate::comic_dubs::ComicDubsProject,
    page_index: usize,
    bubble_index: usize,
) -> Option<(usize, usize, u64)> {
    let page = project.pages().get(page_index)?;
    if bubble_index + 1 < page.bubbles.len() {
        return Some((page_index, bubble_index + 1, project.bubble_gap_ms()));
    }
    project
        .pages()
        .iter()
        .enumerate()
        .skip(page_index + 1)
        .find(|(_, page)| !page.bubbles.is_empty())
        .map(|(page_index, _)| (page_index, 0, project.page_gap_ms()))
}

fn comic_dubs_start_page(project: &crate::comic_dubs::ComicDubsProject) -> Option<usize> {
    let active = project.active_page_id()?;
    let start = project.pages().iter().position(|page| page.id == active)?;
    project
        .pages()
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, page)| !page.bubbles.is_empty())
        .map(|(index, _)| index)
}

fn recording_workspace_has_content(
    project: &crate::recording::RecordingProject,
    revision: u64,
) -> bool {
    revision != 0 || project.assets().len() != 0 || project.clips().len() != 0
}

fn comic_dubs_playback_due(now: Instant, deadline: Instant, audio_playing: bool) -> bool {
    now >= deadline && !audio_playing
}

fn media_video_item(path: &Path) -> crate::ui::language_modal::MediaVideoItem {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let (summary, audio_summary) = match crate::video_proxy::probe_video(path) {
        Ok(info) => {
            let bitrate = if info.bitrate >= 1_000_000 {
                format!("{:.1} Mb/s", info.bitrate as f64 / 1_000_000.0)
            } else if info.bitrate > 0 {
                format!("{} kb/s", info.bitrate / 1_000)
            } else {
                "—".to_string()
            };
            let size = if info.file_size >= 1_000_000_000 {
                format!("{:.1} Go", info.file_size as f64 / 1_000_000_000.0)
            } else {
                format!("{:.1} Mo", info.file_size as f64 / 1_000_000.0)
            };
            let duration = format!(
                "{:02}:{:02}:{:02}",
                (info.duration_secs / 3600.0) as u64,
                ((info.duration_secs % 3600.0) / 60.0) as u64,
                (info.duration_secs % 60.0) as u64
            );
            let audio = info.audio_codec.as_ref().map(|codec| {
                format!(
                    "{} • {} canaux • {} kHz",
                    codec.to_uppercase(),
                    info.audio_channels.unwrap_or(0),
                    info.audio_sample_rate.unwrap_or(0) / 1_000
                )
            });
            (
                format!(
                    "{} × {} • {} • {:.3} i/s • {} • {} • {}",
                    info.width,
                    info.height,
                    info.video_codec.to_uppercase(),
                    info.fps,
                    bitrate,
                    duration,
                    size
                ),
                audio,
            )
        }
        Err(error) => (
            format!(
                "{} ({error})",
                crate::i18n::t("media_explorer.info_unavailable")
            ),
            None,
        ),
    };
    crate::ui::language_modal::MediaVideoItem {
        name,
        path: path.display().to_string(),
        summary,
        audio_summary,
    }
}

fn recording_added_assets<'a>(
    operation: &'a crate::recording::RecordingOperation,
    assets: &mut Vec<&'a crate::recording::AudioAsset>,
) {
    match operation {
        crate::recording::RecordingOperation::Batch { operations } => {
            for operation in operations {
                recording_added_assets(operation, assets);
            }
        }
        crate::recording::RecordingOperation::AddAsset { asset }
        | crate::recording::RecordingOperation::ReplaceAsset { asset } => assets.push(asset),
        _ => {}
    }
}

fn imported_audio_operation(
    project: &mut crate::recording::RecordingProject,
    audio: crate::recording::RecordedAudio,
    placement: Option<(crate::recording::AudioTrackId, i64)>,
) -> (
    crate::recording::AudioAssetId,
    String,
    crate::recording::RecordingOperation,
) {
    let asset_id = project.allocate_asset_id();
    let asset = audio.into_asset(asset_id);
    let file_name = asset.file_name.clone();
    let operation = if let Some((track_id, start_frame)) = placement {
        let clip_id = project.allocate_clip_id();
        let duration_frames = asset.duration_frames(project.timeline_fps());
        crate::recording::RecordingOperation::Batch {
            operations: vec![
                crate::recording::RecordingOperation::AddAsset { asset },
                crate::recording::RecordingOperation::AddClip {
                    clip: crate::recording::AudioClip {
                        id: clip_id,
                        asset_id,
                        track_id,
                        start_frame,
                        source_start_frame: 0,
                        duration_frames,
                    },
                },
            ],
        }
    } else {
        crate::recording::RecordingOperation::AddAsset { asset }
    };
    (asset_id, file_name, operation)
}

fn project_load_stage_key(stage: ProjectLoadStage) -> &'static str {
    match stage {
        ProjectLoadStage::ReadingManifest => "loading_project.reading_manifest",
        ProjectLoadStage::ExtractingAssets => "loading_project.extracting_assets",
        ProjectLoadStage::VerifyingAssets => "loading_project.verifying_assets",
    }
}

fn project_load_overall_progress(progress: ProjectLoadProgress) -> f32 {
    match progress.stage {
        ProjectLoadStage::ReadingManifest => progress.fraction * 0.08,
        ProjectLoadStage::ExtractingAssets => 0.08 + progress.fraction * 0.82,
        ProjectLoadStage::VerifyingAssets => 0.08 + progress.fraction * 0.82,
    }
}

fn convert_comic_image(source: &Path, output: &Path) -> Result<(u32, u32), String> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let image = image::open(source).map_err(|error| error.to_string())?;
    let dimensions = (image.width(), image.height());
    image
        .save_with_format(output, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(dimensions)
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

#[cfg(test)]
mod playback_tests {
    use super::{
        comic_dubs_playback_due, comic_dubs_start_page, next_comic_dubs_position,
        recording_playback_is_blocked_during_countdown, recording_playback_waits_for_mix,
        recording_workspace_has_content, workspace_shows_project_video, WorkspaceId,
    };
    use crate::comic_dubs::{ComicDubsProject, Point};
    use crate::recording::{AudioAssetId, AudioClipId, AudioTrackId, CaptureState, CaptureTarget};
    use std::time::Duration;

    #[test]
    fn recording_play_waits_for_the_pending_mix() {
        assert!(recording_playback_waits_for_mix(
            WorkspaceId::Recording,
            true,
            false,
            false,
        ));
        assert!(!recording_playback_waits_for_mix(
            WorkspaceId::Recording,
            false,
            false,
            false,
        ));
        assert!(!recording_playback_waits_for_mix(
            WorkspaceId::Recording,
            true,
            false,
            true,
        ));
    }

    #[test]
    fn remote_playback_is_ignored_during_recording_countdown() {
        let target = CaptureTarget {
            track_id: AudioTrackId::new(1),
            asset_id: AudioAssetId::new(2),
            clip_id: AudioClipId::new(3),
            start_frame: 48,
        };
        let countdown = CaptureState::Countdown {
            target,
            deadline: Duration::from_secs(3),
        };

        assert!(recording_playback_is_blocked_during_countdown(Some(
            &countdown,
        )));
        assert!(!recording_playback_is_blocked_during_countdown(None));
    }

    #[test]
    fn project_video_stays_out_of_unrelated_workspaces() {
        assert!(workspace_shows_project_video(WorkspaceId::Rythmo));
        assert!(workspace_shows_project_video(WorkspaceId::Recording));
        assert!(!workspace_shows_project_video(WorkspaceId::Voicelines));
        assert!(!workspace_shows_project_video(WorkspaceId::ComicDubs));
    }

    #[test]
    fn comic_dubs_sequence_uses_bubble_then_page_gaps() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(250, 900);
        let first = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let second = project.add_page("2.png".into(), "2.png".into(), 10, 10);
        let triangle = || {
            vec![
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.1 },
                Point { x: 0.5, y: 0.9 },
            ]
        };
        project.add_bubble(first, triangle());
        project.add_bubble(first, triangle());
        project.add_bubble(second, triangle());

        assert_eq!(next_comic_dubs_position(&project, 0, 0), Some((0, 1, 250)));
        assert_eq!(next_comic_dubs_position(&project, 0, 1), Some((1, 0, 900)));
        assert_eq!(next_comic_dubs_position(&project, 1, 0), None);
    }

    #[test]
    fn comic_dubs_sequence_starts_on_the_active_page() {
        let mut project = ComicDubsProject::default();
        let first = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let second = project.add_page("2.png".into(), "2.png".into(), 10, 10);
        let triangle = || {
            vec![
                Point { x: 0.1, y: 0.1 },
                Point { x: 0.9, y: 0.1 },
                Point { x: 0.5, y: 0.9 },
            ]
        };
        project.add_bubble(first, triangle());
        project.add_bubble(second, triangle());
        project.select_page(second);

        assert_eq!(comic_dubs_start_page(&project), Some(1));
    }

    #[test]
    fn comic_dubs_never_advances_before_audio_finishes() {
        let deadline = std::time::Instant::now();
        let after = deadline + Duration::from_secs(1);
        assert!(!comic_dubs_playback_due(after, deadline, true));
        assert!(comic_dubs_playback_due(after, deadline, false));
    }

    #[test]
    fn untouched_recording_workspace_is_not_added_to_feature_only_projects() {
        let mut recording = crate::recording::RecordingProject::new(24.0).unwrap();
        recording
            .apply(&crate::recording::RecordingOperation::AddTrack {
                track: crate::recording::AudioTrack::new(
                    crate::recording::AudioTrackId::new(1),
                    "Voix",
                ),
            })
            .unwrap();
        assert!(!recording_workspace_has_content(&recording, 0));
        assert!(recording_workspace_has_content(&recording, 1));
    }
}

#[cfg(test)]
mod recording_import_tests {
    use super::imported_audio_operation;
    use crate::recording::{
        AudioTrack, RecordedAudio, RecordingOperation, RecordingProject, WaveformData,
    };

    #[test]
    fn dropped_audio_is_registered_and_placed_in_one_operation() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = project.allocate_track_id();
        project
            .apply(&RecordingOperation::AddTrack {
                track: AudioTrack::new(track_id, "Voix"),
            })
            .unwrap();
        let audio = RecordedAudio {
            file_name: "Bob_2026-08-12_14-30-00_voice.flac".into(),
            sample_rate: 48_000,
            channels: 1,
            sample_count: 96_000,
            checksum: "a".repeat(40),
            waveform: WaveformData::new(480, vec![0.5]).unwrap(),
        };

        let (asset_id, _, operation) =
            imported_audio_operation(&mut project, audio, Some((track_id, 42)));
        assert!(matches!(operation, RecordingOperation::Batch { .. }));
        project.apply(&operation).unwrap();

        assert_eq!(
            project.asset(asset_id).unwrap().file_name,
            "Bob_2026-08-12_14-30-00_voice.flac"
        );
        let clip = project.clips().next().unwrap();
        assert_eq!(
            (clip.asset_id, clip.track_id, clip.start_frame),
            (asset_id, track_id, 42)
        );
    }
}

#[cfg(test)]
mod comic_dubs_import_tests {
    use super::convert_comic_image;

    #[test]
    fn imported_images_are_stored_as_png() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "coquerythmo-comic-image-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("page.bmp");
        let output = directory.join("page.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .save_with_format(&source, image::ImageFormat::Bmp)
            .unwrap();

        assert_eq!(convert_comic_image(&source, &output).unwrap(), (3, 2));
        assert_eq!(&std::fs::read(&output).unwrap()[..8], b"\x89PNG\r\n\x1a\n");
        std::fs::remove_dir_all(directory).unwrap();
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
    pub voicelines_project: crate::voicelines::VoicelinesProject,
    voicelines_revision: u64,
    voicelines_player: Option<VideoPlayer>,
    voicelines_imports: Vec<PendingVoicelinesImport>,
    voicelines_joins: Vec<PendingVoicelinesJoin>,
    voicelines_exports: Vec<PendingVoicelinesExport>,
    voicelines_transfers: Vec<PendingVoicelinesTransfer>,
    voicelines_play_until_ms: Option<u64>,
    voicelines_undo: Vec<crate::voicelines::VoicelinesProject>,
    voicelines_redo: Vec<crate::voicelines::VoicelinesProject>,
    pub comic_dubs_project: crate::comic_dubs::ComicDubsProject,
    comic_dubs_revision: u64,
    comic_dubs_player: Option<VideoPlayer>,
    comic_dubs_playback: Option<ComicDubsPlayback>,
    comic_dubs_imports: Vec<PendingComicDubsImport>,
    comic_dubs_undo: Vec<crate::comic_dubs::ComicDubsProject>,
    comic_dubs_redo: Vec<crate::comic_dubs::ComicDubsProject>,
    project_transfer: Option<ProjectTransferRuntime>,
    project_transfer_prepare: Option<Receiver<Result<ProjectTransferMetadata, String>>>,
    project_transfer_source: Option<PathBuf>,
    project_transfer_send: Option<(String, Receiver<Result<(), String>>)>,
    project_transfer_loading_request: Option<String>,
    project_transfer_waiting_dismissed: Option<String>,
    recording_input_preflight: Option<(Option<String>, bool)>,
    recording_uploads: Vec<(String, Receiver<Result<(), String>>)>,
    recording_upload_acks: Vec<String>,
    /// In-flight chunked `big_*` events being reassembled (sync, recording_prepare).
    big_receives: crate::big_event::BigEventReceiver,
    pub workspace_host: WorkspaceHost,
    pub narration: NarrationService,
    last_autosave: Instant,
    line_clipboard: Option<Vec<LineClipboardEntry>>,
    automation_last_run: Option<(u64, u64)>,
    last_progress_percent: Option<u32>,
    last_progress_announcement: Option<Instant>,
    last_recording_countdown_second: Option<u32>,
    /// Pending `coquerythmo://` quick-setup flow awaiting either a project
    /// save/close decision, a project import or (join only) a username prompt.
    pending_protocol: Option<PendingProtocolFlow>,
}

/// Internal state machine for protocol quick-setup links. At most one flow
/// may run at a time; each step is resumable so the app never deadlocks when
/// the user cancels midway.
pub(crate) struct PendingProtocolFlow {
    pub(crate) payload: ProtocolPayload,
    pub(crate) stage: PendingProtocolStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingProtocolStage {
    /// Waiting for the current project to close (save prompt, discard or
    /// "no save needed"). Continues via [`State::protocol_current_closed`].
    ClosingCurrentProject,
    /// Waiting for the `.coquerythmo` import job to finish parsing the file
    /// from the link. Continues via [`State::poll_pending_protocol`].
    ImportingTargetProject,
}

struct ProjectTransferRuntime {
    metadata: ProjectTransferMetadata,
    status: Option<ProjectTransferStatus>,
    receiver: crate::file_transfer::FileTransferReceiver,
}

struct PendingVoicelinesImport {
    source_path: PathBuf,
    output_path: PathBuf,
    receiver: Receiver<Result<crate::recording::RecordedAudio, crate::recording::RecordingError>>,
}

struct PendingVoicelinesJoin {
    before: crate::voicelines::VoicelinesProject,
    project: crate::voicelines::VoicelinesProject,
    join: crate::voicelines::RegionJoin,
    output_path: PathBuf,
    revision: u64,
    receiver: Receiver<Result<crate::recording::RecordedAudio, String>>,
}

struct PendingVoicelinesExport {
    receiver: Receiver<Result<String, String>>,
}

struct TransferredVoiceline {
    region_id: crate::voicelines::RegionId,
    output_path: PathBuf,
    recorded: crate::recording::RecordedAudio,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VoicelinesTransferMode {
    Send,
    Update,
}

struct PendingVoicelinesTransfer {
    audio_id: crate::voicelines::AudioId,
    workspace: WorkspaceId,
    mode: VoicelinesTransferMode,
    output_paths: Vec<PathBuf>,
    receiver: Receiver<Result<Vec<TransferredVoiceline>, String>>,
}

enum PendingComicDubsImport {
    Image {
        source_path: PathBuf,
        output_path: PathBuf,
        receiver: Receiver<Result<(u32, u32), String>>,
    },
    Audio {
        source_path: PathBuf,
        output_path: PathBuf,
        receiver:
            Receiver<Result<crate::recording::RecordedAudio, crate::recording::RecordingError>>,
    },
}

struct ComicDubsPlayback {
    page_index: usize,
    bubble_index: usize,
    started_at: Instant,
    deadline: Instant,
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
            voicelines_project: crate::voicelines::VoicelinesProject::default(),
            voicelines_revision: 0,
            voicelines_player: None,
            voicelines_imports: Vec::new(),
            voicelines_joins: Vec::new(),
            voicelines_exports: Vec::new(),
            voicelines_transfers: Vec::new(),
            voicelines_play_until_ms: None,
            voicelines_undo: Vec::new(),
            voicelines_redo: Vec::new(),
            comic_dubs_project: crate::comic_dubs::ComicDubsProject::default(),
            comic_dubs_revision: 0,
            comic_dubs_player: None,
            comic_dubs_playback: None,
            comic_dubs_imports: Vec::new(),
            comic_dubs_undo: Vec::new(),
            comic_dubs_redo: Vec::new(),
            project_transfer: None,
            project_transfer_prepare: None,
            project_transfer_source: None,
            project_transfer_send: None,
            project_transfer_loading_request: None,
            project_transfer_waiting_dismissed: None,
            recording_input_preflight: None,
            recording_uploads: Vec::new(),
            recording_upload_acks: Vec::new(),
            big_receives: crate::big_event::BigEventReceiver::default(),
            workspace_host: WorkspaceHost::new(
                vec![
                    Box::new(RythmoWorkspace::new()),
                    Box::new(RecordingWorkspace::new()),
                    Box::new(VoicelinesWorkspace::new()),
                    Box::new(ComicDubsWorkspace::new()),
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
            last_recording_countdown_second: None,
            pending_protocol: None,
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

    // -- Voicelines --

    pub fn voicelines_begin_audio_import(&mut self, source_path: PathBuf) {
        let supported = source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["flac", "wav", "mp3", "ogg", "m4a", "aac", "opus"]
                    .contains(&extension.to_ascii_lowercase().as_str())
            });
        if !source_path.is_file() || !supported {
            self.show_toast("Format audio non pris en charge", 4.0);
            return;
        }
        let output_path = self
            .recording_runtime
            .allocate_external_audio_path(&source_path, "voicelines");
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker_source = source_path.clone();
        let worker_output = output_path.clone();
        std::thread::spawn(move || {
            let _ = sender.send(crate::media_recording::import_audio(
                &worker_source,
                &worker_output,
            ));
        });
        self.voicelines_imports.push(PendingVoicelinesImport {
            source_path: source_path.clone(),
            output_path,
            receiver,
        });
        self.show_toast(
            format!(
                "Import de {} en cours…",
                source_path
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default()
            ),
            2.0,
        );
    }

    pub fn voicelines_select_audio(&mut self, id: crate::voicelines::AudioId) {
        if self.voicelines_project.select_audio(id)
            || self.voicelines_project.active_audio_id() == Some(id)
        {
            if let Err(error) = self.voicelines_load_active_player() {
                self.show_toast(error, 5.0);
            }
        }
    }

    pub(crate) fn reset_voicelines_document(&mut self) {
        self.voicelines_project = crate::voicelines::VoicelinesProject::default();
        self.voicelines_revision = self.voicelines_revision.wrapping_add(1);
        self.voicelines_undo.clear();
        self.voicelines_redo.clear();
        self.voicelines_player = None;
        self.voicelines_play_until_ms = None;
        self.ui_shell.ui.set_voicelines_selected_region(None);
    }

    pub fn voicelines_remove_audio(&mut self, id: crate::voicelines::AudioId) {
        let before = self.voicelines_project.clone();
        if !self.voicelines_project.remove_audio(id) {
            return;
        }
        self.voicelines_commit(before);
        self.voicelines_play_until_ms = None;
        if self.voicelines_project.active_audio().is_some() {
            if let Err(error) = self.voicelines_load_active_player() {
                self.show_toast(error, 5.0);
            }
        } else {
            self.voicelines_player = None;
            self.ui_shell.ui.total_frames = 0;
            self.ui_shell.ui.set_playing(false);
        }
    }

    pub fn voicelines_add_region(&mut self, start_ms: u64, end_ms: u64) {
        let before = self.voicelines_project.clone();
        let manual = matches!(
            self.voicelines_project.naming(),
            crate::voicelines::NamingMode::Manual
        );
        if let Some(id) = self.voicelines_project.add_region(start_ms, end_ms) {
            self.voicelines_commit(before);
            self.ui_shell.ui.set_voicelines_selected_region(Some(id));
            if manual {
                let name = self
                    .voicelines_project
                    .active_audio()
                    .and_then(|audio| audio.regions.iter().find(|region| region.id == id))
                    .map(|region| region.name.clone())
                    .unwrap_or_default();
                self.ui_shell.ui.begin_voicelines_region_rename(id, name);
            }
        }
    }

    pub fn voicelines_move_region(
        &mut self,
        region_id: crate::voicelines::RegionId,
        start_ms: u64,
        end_ms: u64,
    ) {
        let before = self.voicelines_project.clone();
        if self
            .voicelines_project
            .move_region(region_id, start_ms, end_ms)
        {
            self.voicelines_commit(before);
        }
    }

    pub fn voicelines_rename_region(&mut self, region_id: crate::voicelines::RegionId, name: &str) {
        let before = self.voicelines_project.clone();
        if self.voicelines_project.rename_region(region_id, name) {
            self.voicelines_commit(before);
        }
    }

    pub fn voicelines_delete_region(&mut self, region_id: crate::voicelines::RegionId) {
        let before = self.voicelines_project.clone();
        if self.voicelines_project.remove_region(region_id) {
            self.voicelines_commit(before);
            self.ui_shell.ui.set_voicelines_selected_region(None);
        }
    }

    pub fn voicelines_join_regions(&mut self, region_ids: Vec<crate::voicelines::RegionId>) {
        let before = self.voicelines_project.clone();
        let mut project = before.clone();
        let Some(join) = project.join_regions(&region_ids) else {
            self.show_toast("Sélectionnez au moins deux zones valides", 4.0);
            return;
        };
        let Some(audio) = before.audio(join.audio_id) else {
            return;
        };
        let input_path = audio.playback_path.clone();
        let output_path = self
            .recording_runtime
            .allocate_external_audio_path(&input_path, "voicelines-join");
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker_output = output_path.clone();
        let worker_join = join.clone();
        std::thread::spawn(move || {
            let _ = sender.send(crate::voicelines::join_audio_regions(
                &input_path,
                &worker_output,
                &worker_join,
            ));
        });
        self.voicelines_joins.push(PendingVoicelinesJoin {
            before,
            project,
            join,
            output_path,
            revision: self.voicelines_revision,
            receiver,
        });
        self.show_toast("Raccord des zones en cours…", 2.5);
    }

    pub fn voicelines_set_naming_pattern(&mut self, pattern: String) {
        let before = self.voicelines_project.clone();
        if let Err(error) = self.voicelines_project.set_automatic_naming(&pattern) {
            self.ui_shell.ui.begin_voicelines_naming_pattern(pattern);
            self.show_toast(error, 4.0);
            return;
        }
        self.voicelines_commit(before);
    }

    pub fn voicelines_auto_detect(&mut self) {
        let before = self.voicelines_project.clone();
        let count = self.voicelines_project.auto_detect_regions();
        self.voicelines_commit(before);
        self.ui_shell.ui.set_voicelines_selected_region(None);
        self.show_toast(format!("{count} zone(s) détectée(s)"), 3.0);
    }

    pub fn voicelines_play_region(&mut self, region_id: crate::voicelines::RegionId) {
        let Some(region) = self
            .voicelines_project
            .active_audio()
            .and_then(|audio| audio.regions.iter().find(|region| region.id == region_id))
            .cloned()
        else {
            return;
        };
        self.seek_absolute_internal((region.start_ms / 10) as i64, false);
        self.toggle_play_pause_internal(false);
        self.voicelines_play_until_ms = Some(region.end_ms);
    }

    pub fn voicelines_export_region_request(
        &self,
        region_id: crate::voicelines::RegionId,
    ) -> Option<crate::application::command::FilePickerRequest> {
        let audio = self.voicelines_project.active_audio()?;
        let region = audio.regions.iter().find(|region| region.id == region_id)?;
        Some(crate::application::command::FilePickerRequest {
            title: "Exporter la voiceline".into(),
            mode: crate::application::command::FilePickerMode::Save,
            intent: crate::application::command::FilePickerIntent::VoicelinesExportRegion {
                audio_id: audio.id,
                region_id,
            },
            filters: vec![crate::application::command::FileFilterSpec::new(
                "Audio OGG",
                &["ogg"],
            )],
            initial_dir: audio.source_path.parent().map(Path::to_path_buf),
            default_extension: Some("ogg".into()),
            initial_filename: Some(format!(
                "{}.ogg",
                crate::voicelines::export_stem(&region.name)
            )),
        })
    }

    pub fn voicelines_export_all_request(
        &self,
    ) -> Option<crate::application::command::FilePickerRequest> {
        let audio = self.voicelines_project.active_audio()?;
        (!audio.regions.is_empty()).then(|| crate::application::command::FilePickerRequest {
            title: "Dossier d'export des voicelines".into(),
            mode: crate::application::command::FilePickerMode::Folder,
            intent: crate::application::command::FilePickerIntent::VoicelinesExportAll {
                audio_id: audio.id,
            },
            filters: Vec::new(),
            initial_dir: audio.source_path.parent().map(Path::to_path_buf),
            default_extension: None,
            initial_filename: None,
        })
    }

    pub fn voicelines_save_request(&self) -> crate::application::command::FilePickerRequest {
        crate::application::command::FilePickerRequest {
            title: "Sauvegarder la session Voicelines".into(),
            mode: crate::application::command::FilePickerMode::Save,
            intent: crate::application::command::FilePickerIntent::VoicelinesSaveSession,
            filters: vec![crate::application::command::FileFilterSpec::new(
                "Session Voicelines",
                &[crate::project_archive::PROJECT_EXTENSION],
            )],
            initial_dir: self
                .voicelines_project
                .active_audio()
                .and_then(|audio| audio.source_path.parent().map(Path::to_path_buf)),
            default_extension: Some(crate::project_archive::PROJECT_EXTENSION.into()),
            initial_filename: Some("voicelines.coquerythmo".into()),
        }
    }

    pub fn voicelines_load_request(&self) -> crate::application::command::FilePickerRequest {
        crate::application::command::FilePickerRequest {
            title: "Charger une session Voicelines".into(),
            mode: crate::application::command::FilePickerMode::Open,
            intent: crate::application::command::FilePickerIntent::VoicelinesLoadSession,
            filters: vec![crate::application::command::FileFilterSpec::new(
                "Session Voicelines",
                &[crate::project_archive::PROJECT_EXTENSION],
            )],
            initial_dir: None,
            default_extension: None,
            initial_filename: None,
        }
    }

    pub fn voicelines_export_region_to(
        &mut self,
        audio_id: crate::voicelines::AudioId,
        region_id: crate::voicelines::RegionId,
        output: PathBuf,
    ) {
        let Some(audio) = self.voicelines_project.audio(audio_id).cloned() else {
            return;
        };
        let Some(region) = audio
            .regions
            .iter()
            .find(|region| region.id == region_id)
            .cloned()
        else {
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::voicelines::export_region(&audio, &region, &output)
                .map(|()| format!("{} exportée", region.name));
            let _ = sender.send(result);
        });
        self.voicelines_exports
            .push(PendingVoicelinesExport { receiver });
        self.show_toast("Export en cours…", 2.0);
    }

    pub fn voicelines_export_all_to(
        &mut self,
        audio_id: crate::voicelines::AudioId,
        directory: PathBuf,
    ) {
        let Some(audio) = self.voicelines_project.audio(audio_id).cloned() else {
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::voicelines::export_all(&audio, &directory)
                .map(|paths| format!("{} voiceline(s) exportée(s)", paths.len()));
            let _ = sender.send(result);
        });
        self.voicelines_exports
            .push(PendingVoicelinesExport { receiver });
        self.show_toast("Export en cours…", 2.0);
    }

    pub fn voicelines_send_audio(
        &mut self,
        audio_id: crate::voicelines::AudioId,
        workspace: WorkspaceId,
    ) {
        self.voicelines_transfer_audio(audio_id, workspace, VoicelinesTransferMode::Send);
    }

    pub fn voicelines_update_audio(
        &mut self,
        audio_id: crate::voicelines::AudioId,
        workspace: WorkspaceId,
    ) {
        self.voicelines_transfer_audio(audio_id, workspace, VoicelinesTransferMode::Update);
    }

    fn voicelines_transfer_audio(
        &mut self,
        audio_id: crate::voicelines::AudioId,
        workspace: WorkspaceId,
        mode: VoicelinesTransferMode,
    ) {
        if !matches!(workspace, WorkspaceId::ComicDubs | WorkspaceId::Recording) {
            return;
        }
        if workspace == WorkspaceId::Recording && self.collaboration.network.is_in_room() {
            self.show_toast(
                "Envoi vers Enregistrement indisponible en session serveur",
                4.0,
            );
            return;
        }
        if workspace == WorkspaceId::Recording && !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let Some(audio) = self.voicelines_project.audio(audio_id).cloned() else {
            return;
        };
        let destination = if workspace == WorkspaceId::ComicDubs {
            crate::voicelines::DeliveryDestination::ComicDubs
        } else {
            crate::voicelines::DeliveryDestination::Recording
        };
        if mode == VoicelinesTransferMode::Update && !audio.has_delivery(destination) {
            self.show_toast("Aucun envoi à mettre à jour dans cette destination", 4.0);
            return;
        }
        if audio.regions.is_empty() {
            self.show_toast("Aucune voiceline à envoyer", 3.0);
            return;
        }
        let label = if workspace == WorkspaceId::ComicDubs {
            "comic-dubs"
        } else {
            "recording"
        };
        let jobs = audio
            .regions
            .iter()
            .cloned()
            .map(|region| {
                let source = PathBuf::from(format!(
                    "{}.ogg",
                    crate::voicelines::export_stem(&region.name)
                ));
                let output = self
                    .recording_runtime
                    .allocate_external_audio_path(&source, label);
                (region, output)
            })
            .collect::<Vec<_>>();
        let output_paths = jobs
            .iter()
            .map(|(_, output)| output.clone())
            .collect::<Vec<_>>();
        let cleanup_paths = output_paths.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let mut transferred = Vec::with_capacity(jobs.len());
                for (region, output_path) in jobs {
                    let intermediate = output_path.with_extension("region.flac");
                    let item = (|| {
                        crate::voicelines::export_region(&audio, &region, &intermediate)?;
                        let mut recorded =
                            crate::media_recording::import_audio(&intermediate, &output_path)
                                .map_err(|error| error.to_string())?;
                        recorded.file_name =
                            format!("{}.flac", crate::voicelines::export_stem(&region.name));
                        Ok(TransferredVoiceline {
                            region_id: region.id,
                            output_path,
                            recorded,
                        })
                    })();
                    let _ = std::fs::remove_file(intermediate);
                    transferred
                        .push(item.map_err(|error: String| format!("{} : {error}", region.name))?);
                }
                Ok(transferred)
            })();
            if result.is_err() {
                for output in cleanup_paths {
                    let _ = std::fs::remove_file(output);
                }
            }
            let _ = sender.send(result);
        });
        self.voicelines_transfers.push(PendingVoicelinesTransfer {
            audio_id,
            workspace,
            mode,
            output_paths,
            receiver,
        });
        self.show_toast(
            if mode == VoicelinesTransferMode::Update {
                "Mise à jour des voicelines en cours…"
            } else {
                "Envoi des voicelines en cours…"
            },
            2.0,
        );
    }

    pub fn voicelines_save_session(&mut self, path: PathBuf) {
        match self.voicelines_project.save(&path) {
            Ok(()) => self.show_toast("Session Voicelines sauvegardée", 3.0),
            Err(error) => self.show_toast(error, 6.0),
        }
    }

    pub fn voicelines_load_session(&mut self, path: PathBuf) {
        let loaded = match crate::voicelines::VoicelinesProject::load(&path) {
            Ok(project) => project,
            Err(error) => {
                self.show_toast(error, 6.0);
                return;
            }
        };
        let project = match Self::voicelines_bind_loaded_project(
            &mut self.recording_runtime,
            loaded.project,
            loaded.audio_paths,
        ) {
            Ok(project) => project,
            Err(error) => {
                self.show_toast(error, 7.0);
                return;
            }
        };
        let before = std::mem::replace(&mut self.voicelines_project, project);
        self.voicelines_commit(before);
        if let Err(error) = self.voicelines_load_active_player() {
            self.show_toast(error, 6.0);
        } else {
            self.show_toast("Session Voicelines chargée", 3.0);
        }
    }

    fn voicelines_bind_loaded_project(
        runtime: &mut crate::recording_runtime::RecordingRuntime,
        mut project: crate::voicelines::VoicelinesProject,
        sources: std::collections::BTreeMap<crate::voicelines::AudioId, PathBuf>,
    ) -> Result<crate::voicelines::VoicelinesProject, String> {
        for (id, source) in sources {
            let recorded = runtime
                .import_external_audio(&source, "voicelines")
                .map_err(|error| format!("{}: {error}", source.display()))?;
            let playback_path = runtime
                .audio_path(&recorded.checksum)
                .cloned()
                .ok_or_else(|| "Audio Voicelines importé introuvable".to_string())?;
            project.bind_audio(id, playback_path, recorded);
        }
        Ok(project)
    }

    fn comic_dubs_bind_loaded_project(
        runtime: &mut crate::recording_runtime::RecordingRuntime,
        mut project: crate::comic_dubs::ComicDubsProject,
        image_paths: std::collections::BTreeMap<crate::comic_dubs::PageId, PathBuf>,
        audio_paths: std::collections::BTreeMap<crate::comic_dubs::ComicAudioId, PathBuf>,
    ) -> Result<crate::comic_dubs::ComicDubsProject, String> {
        for (id, path) in image_paths {
            if !project.bind_page(id, path) {
                return Err(format!("Page Comic Dubs inconnue {id}"));
            }
        }
        for (id, source) in audio_paths {
            let recorded = runtime
                .import_external_audio(&source, "comic-dubs")
                .map_err(|error| format!("{}: {error}", source.display()))?;
            let playback_path = runtime
                .audio_path(&recorded.checksum)
                .cloned()
                .ok_or_else(|| "Audio Comic Dubs importé introuvable".to_string())?;
            if !project.bind_audio(id, playback_path, recorded) {
                return Err(format!("Audio Comic Dubs inconnu {id}"));
            }
        }
        Ok(project)
    }

    fn voicelines_load_active_player(&mut self) -> Result<(), String> {
        let audio = self
            .voicelines_project
            .active_audio()
            .ok_or_else(|| "Aucun audio sélectionné".to_string())?;
        let path = audio.playback_path.clone();
        let duration_ms = audio.duration_ms();
        let audio_index = self
            .voicelines_project
            .audios()
            .iter()
            .position(|candidate| candidate.id == audio.id)
            .unwrap_or(0);
        let mut player = VideoPlayer::new();
        player.load_audio_only(&path, duration_ms as f64 / 1_000.0)?;
        player.set_volume(self.ui_shell.ui.volume());
        self.voicelines_player = Some(player);
        self.voicelines_play_until_ms = None;
        self.ui_shell.ui.set_playing(false);
        self.ui_shell.ui.total_frames = (duration_ms / 10) as i64;
        self.ui_shell
            .ui
            .voicelines_audio_selected(duration_ms, audio_index);
        Ok(())
    }

    fn voicelines_commit(&mut self, before: crate::voicelines::VoicelinesProject) {
        if before == self.voicelines_project {
            return;
        }
        // ponytail: snapshots are capped; use operation deltas if very large audio lists make this measurable.
        if self.voicelines_undo.len() == 20 {
            self.voicelines_undo.remove(0);
        }
        self.voicelines_undo.push(before);
        self.voicelines_redo.clear();
        self.voicelines_revision = self.voicelines_revision.wrapping_add(1);
        self.project_session.dirty = true;
    }

    pub fn voicelines_undo(&mut self) {
        let Some(previous) = self.voicelines_undo.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.voicelines_project, previous);
        self.voicelines_redo.push(current);
        self.voicelines_after_history_restore();
    }

    pub fn voicelines_redo(&mut self) {
        let Some(next) = self.voicelines_redo.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.voicelines_project, next);
        self.voicelines_undo.push(current);
        self.voicelines_after_history_restore();
    }

    fn voicelines_after_history_restore(&mut self) {
        self.voicelines_revision = self.voicelines_revision.wrapping_add(1);
        self.project_session.dirty = true;
        self.ui_shell.ui.set_voicelines_selected_region(None);
        if self.voicelines_project.active_audio().is_some() {
            if let Err(error) = self.voicelines_load_active_player() {
                self.show_toast(error, 5.0);
            }
        } else {
            self.voicelines_player = None;
            self.voicelines_play_until_ms = None;
            self.ui_shell.ui.total_frames = 0;
            self.ui_shell.ui.set_playing(false);
        }
    }

    fn poll_voicelines_jobs(&mut self) -> bool {
        let mut changed = false;
        let mut index = self.voicelines_imports.len();
        while index > 0 {
            index -= 1;
            let result = match self.voicelines_imports[index].receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err(
                    crate::recording::RecordingError::Recorder("audio import disconnected".into()),
                )),
                Err(TryRecvError::Empty) => None,
            };
            let Some(result) = result else { continue };
            let pending = self.voicelines_imports.swap_remove(index);
            match result {
                Ok(recorded) => {
                    self.recording_runtime
                        .remember_external_audio(&recorded, pending.output_path.clone());
                    let before = self.voicelines_project.clone();
                    let id = self.voicelines_project.add_audio(
                        pending.source_path,
                        pending.output_path,
                        recorded,
                    );
                    self.voicelines_commit(before);
                    self.voicelines_select_audio(id);
                    self.show_toast("Audio ajouté", 2.5);
                }
                Err(error) => {
                    let _ = std::fs::remove_file(pending.output_path);
                    self.show_toast(error.to_string(), 6.0);
                }
            }
            changed = true;
        }

        let mut index = self.voicelines_joins.len();
        while index > 0 {
            index -= 1;
            let result = match self.voicelines_joins[index].receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("raccord audio interrompu".into())),
                Err(TryRecvError::Empty) => None,
            };
            let Some(result) = result else { continue };
            let mut pending = self.voicelines_joins.swap_remove(index);
            match result {
                Ok(recorded) if pending.revision == self.voicelines_revision => {
                    self.recording_runtime
                        .remember_external_audio(&recorded, pending.output_path.clone());
                    pending.project.bind_audio(
                        pending.join.audio_id,
                        pending.output_path,
                        recorded,
                    );
                    self.voicelines_project = pending.project;
                    self.voicelines_commit(pending.before);
                    if let Err(error) = self.voicelines_load_active_player() {
                        self.show_toast(error, 5.0);
                    } else {
                        self.ui_shell
                            .ui
                            .set_voicelines_selected_region(Some(pending.join.region_id));
                        self.show_toast("Zones raccordées", 3.0);
                    }
                }
                Ok(_) => {
                    let _ = std::fs::remove_file(pending.output_path);
                    self.show_toast("Le projet a changé pendant le raccord", 5.0);
                }
                Err(error) => {
                    let _ = std::fs::remove_file(pending.output_path);
                    self.show_toast(error, 6.0);
                }
            }
            changed = true;
        }

        let mut index = self.voicelines_exports.len();
        while index > 0 {
            index -= 1;
            let result = match self.voicelines_exports[index].receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("export interrompu".into())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.voicelines_exports.swap_remove(index);
                match result {
                    Ok(message) => self.show_toast(message, 4.0),
                    Err(error) => self.show_toast(error, 7.0),
                }
                changed = true;
            }
        }

        let mut index = self.voicelines_transfers.len();
        while index > 0 {
            index -= 1;
            let result = match self.voicelines_transfers[index].receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("transfert interrompu".into())),
                Err(TryRecvError::Empty) => None,
            };
            let Some(result) = result else { continue };
            let pending = self.voicelines_transfers.swap_remove(index);
            match result {
                Ok(items) if pending.workspace == WorkspaceId::ComicDubs => {
                    let count = items.len();
                    let before_comic_dubs = self.comic_dubs_project.clone();
                    let before_voicelines = self.voicelines_project.clone();
                    let source_audio = self.voicelines_project.audio(pending.audio_id).cloned();
                    let result = (|| {
                        let source_audio = source_audio
                            .ok_or_else(|| "Audio Voicelines source introuvable".to_string())?;
                        if pending.mode == VoicelinesTransferMode::Update {
                            for item in &items {
                                if let Some(target_id) = source_audio.delivery_target(
                                    crate::voicelines::DeliveryDestination::ComicDubs,
                                    item.region_id,
                                ) {
                                    if self.comic_dubs_project.audio(target_id).is_none() {
                                        return Err(format!(
                                            "Audio Comic Dubs à mettre à jour introuvable ({target_id})"
                                        ));
                                    }
                                }
                            }
                        }
                        for item in items {
                            let target_id = if pending.mode == VoicelinesTransferMode::Update {
                                source_audio.delivery_target(
                                    crate::voicelines::DeliveryDestination::ComicDubs,
                                    item.region_id,
                                )
                            } else {
                                None
                            };
                            self.recording_runtime.remember_external_audio(
                                &item.recorded,
                                item.output_path.clone(),
                            );
                            let target_id = if let Some(target_id) = target_id {
                                self.comic_dubs_project.bind_audio(
                                    target_id,
                                    item.output_path,
                                    item.recorded,
                                );
                                target_id
                            } else {
                                let file_name = item.recorded.file_name.clone();
                                self.comic_dubs_project.add_audio(
                                    file_name,
                                    item.output_path,
                                    item.recorded,
                                )
                            };
                            self.voicelines_project.set_delivery_target(
                                pending.audio_id,
                                crate::voicelines::DeliveryDestination::ComicDubs,
                                item.region_id,
                                target_id,
                            );
                        }
                        Ok::<(), String>(())
                    })();
                    match result {
                        Ok(()) => {
                            self.comic_dubs_commit(before_comic_dubs);
                            self.voicelines_commit(before_voicelines);
                            self.show_toast(
                                format!(
                                    "{count} voiceline(s) {} Comic Dubs",
                                    if pending.mode == VoicelinesTransferMode::Update {
                                        "mise(s) à jour dans"
                                    } else {
                                        "envoyée(s) vers"
                                    }
                                ),
                                4.0,
                            );
                        }
                        Err(error) => {
                            self.comic_dubs_project = before_comic_dubs;
                            self.voicelines_project = before_voicelines;
                            for output in pending.output_paths {
                                let _ = std::fs::remove_file(output);
                            }
                            self.show_toast(error, 7.0);
                        }
                    }
                }
                Ok(items) if pending.workspace == WorkspaceId::Recording => {
                    if !self.ui_shell.ui.recording_can_edit_timeline() {
                        for item in items {
                            let _ = std::fs::remove_file(item.output_path);
                        }
                        self.recording_read_only_error();
                        changed = true;
                        continue;
                    }
                    let count = items.len();
                    let mut operations = Vec::with_capacity(count);
                    let mut deliveries = Vec::with_capacity(count);
                    let mut last = None;
                    for item in items {
                        let target_id = (pending.mode == VoicelinesTransferMode::Update)
                            .then(|| {
                                self.voicelines_project
                                    .audio(pending.audio_id)
                                    .and_then(|audio| {
                                        audio.delivery_target(
                                            crate::voicelines::DeliveryDestination::Recording,
                                            item.region_id,
                                        )
                                    })
                            })
                            .flatten();
                        self.recording_runtime
                            .remember_external_audio(&item.recorded, item.output_path);
                        let (id, file_name, operation) = if let Some(raw_id) = target_id {
                            let id = crate::recording::AudioAssetId::new(raw_id);
                            let asset = item.recorded.into_asset(id);
                            let file_name = asset.file_name.clone();
                            (
                                id,
                                file_name,
                                crate::recording::RecordingOperation::ReplaceAsset { asset },
                            )
                        } else {
                            imported_audio_operation(
                                &mut self.project_session.recording_project,
                                item.recorded,
                                None,
                            )
                        };
                        last = Some((file_name, id));
                        deliveries.push((item.region_id, id));
                        operations.push(operation);
                    }
                    match self.apply_recording_operation(
                        crate::recording::RecordingOperation::Batch { operations },
                    ) {
                        Ok(()) => {
                            let before = self.voicelines_project.clone();
                            for (region_id, id) in deliveries {
                                self.voicelines_project.set_delivery_target(
                                    pending.audio_id,
                                    crate::voicelines::DeliveryDestination::Recording,
                                    region_id,
                                    id.get(),
                                );
                            }
                            self.voicelines_commit(before);
                            if let Some((file_name, id)) = last {
                                self.ui_shell.ui.recording_reveal_asset(&file_name, id);
                            }
                            self.show_toast(
                                format!(
                                    "{count} voiceline(s) {} Enregistrement",
                                    if pending.mode == VoicelinesTransferMode::Update {
                                        "mise(s) à jour dans"
                                    } else {
                                        "envoyée(s) vers"
                                    }
                                ),
                                4.0,
                            );
                        }
                        Err(error) => self.recording_error(error.to_string()),
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    for output in pending.output_paths {
                        let _ = std::fs::remove_file(output);
                    }
                    self.show_toast(error, 7.0);
                }
            }
            changed = true;
        }
        changed
    }

    // -- Comic Dubs --

    pub fn comic_dubs_begin_image_import(&mut self, source_path: PathBuf) {
        let supported = source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["png", "jpg", "jpeg", "webp", "bmp", "gif", "ico"]
                    .contains(&extension.to_ascii_lowercase().as_str())
            });
        if !source_path.is_file() || !supported {
            self.show_toast("Format d’image non pris en charge", 4.0);
            return;
        }
        let output_path = self
            .recording_runtime
            .allocate_external_image_path(&source_path, "comic-dubs");
        let worker_source = source_path.clone();
        let worker_output = output_path.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(convert_comic_image(&worker_source, &worker_output));
        });
        self.comic_dubs_imports.push(PendingComicDubsImport::Image {
            source_path,
            output_path,
            receiver,
        });
    }

    pub fn comic_dubs_begin_audio_import(&mut self, source_path: PathBuf) {
        let supported = source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                ["flac", "wav", "mp3", "ogg", "m4a", "aac", "opus"]
                    .contains(&extension.to_ascii_lowercase().as_str())
            });
        if !source_path.is_file() || !supported {
            self.show_toast("Format audio non pris en charge", 4.0);
            return;
        }
        let output_path = self
            .recording_runtime
            .allocate_external_audio_path(&source_path, "comic-dubs");
        let worker_source = source_path.clone();
        let worker_output = output_path.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::media_recording::import_audio(
                &worker_source,
                &worker_output,
            ));
        });
        self.comic_dubs_imports.push(PendingComicDubsImport::Audio {
            source_path,
            output_path,
            receiver,
        });
        let pending = self
            .comic_dubs_imports
            .iter()
            .filter(|job| matches!(job, PendingComicDubsImport::Audio { .. }))
            .count();
        self.ui_shell.ui.set_comic_dubs_pending_audio_imports(pending);
    }

    pub fn comic_dubs_select_page(&mut self, page_id: crate::comic_dubs::PageId) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.select_page(page_id);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_remove_page(&mut self, page_id: crate::comic_dubs::PageId) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.remove_page(page_id);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_move_page(&mut self, page_id: crate::comic_dubs::PageId, delta: isize) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.move_page(page_id, delta);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_remove_audio(&mut self, audio_id: crate::comic_dubs::ComicAudioId) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.remove_audio(audio_id);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_add_bubble(
        &mut self,
        page_id: crate::comic_dubs::PageId,
        points: Vec<crate::comic_dubs::Point>,
    ) {
        let before = self.comic_dubs_project.clone();
        let id = self.comic_dubs_project.add_bubble(page_id, points);
        self.comic_dubs_commit(before);
        if let Some(id) = id {
            let text = self.comic_dubs_project.bubble(id).unwrap().text.clone();
            self.ui_shell.ui.begin_comic_dubs_text_edit(id, text);
        }
    }

    pub fn comic_dubs_set_bubble_text(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        text: String,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.set_bubble_text(bubble_id, text);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_color(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        color: [u8; 4],
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.set_bubble_color(bubble_id, color);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_font_size(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        font_size: f32,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_font_size(bubble_id, font_size);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_letter_spacing(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        spacing: f32,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_letter_spacing(bubble_id, spacing);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_line_spacing(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        spacing: f32,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_line_spacing(bubble_id, spacing);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_text_color(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        color: [u8; 4],
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_text_color(bubble_id, color);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_text_alignment(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        alignment: crate::comic_dubs::TextAlignment,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_text_alignment(bubble_id, alignment);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_text_style(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        bold: bool,
        strikethrough: bool,
        underline: bool,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.set_bubble_text_style(
            bubble_id,
            bold,
            strikethrough,
            underline,
        );
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_set_bubble_points(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        points: Vec<crate::comic_dubs::Point>,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.set_bubble_points(bubble_id, points);
        self.comic_dubs_commit(before);
    }

    pub fn open_comic_dubs_vertex_editor(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
    ) {
        if self.comic_dubs_project.bubble(bubble_id).is_none() {
            return;
        }
        self.stop_comic_dubs_playback();
        self.ui_shell.ui.open_comic_dubs_vertex_editor(bubble_id);
    }

    pub fn close_comic_dubs_vertex_editor(&mut self) {
        self.ui_shell.ui.close_comic_dubs_vertex_editor();
    }

    pub fn set_comic_dubs_vertex_editor_playhead(&mut self, at_ms: u64) {
        self.ui_shell.ui.set_comic_dubs_vertex_editor_playhead(
            at_ms,
            &self.comic_dubs_project,
        );
    }

    pub fn toggle_comic_dubs_vertex_editor_preview(&mut self) -> bool {
        self.ui_shell.ui.toggle_comic_dubs_vertex_editor_preview(
            &self.comic_dubs_project,
        )
    }

    pub fn comic_dubs_set_bubble_vertex_keyframe(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        at_ms: u64,
        points: Vec<crate::comic_dubs::Point>,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .set_bubble_vertex_keyframe(bubble_id, at_ms, points);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_remove_bubble_vertex_keyframe(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        at_ms: u64,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project
            .remove_bubble_vertex_keyframe(bubble_id, at_ms);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_assign_audio(
        &mut self,
        bubble_id: crate::comic_dubs::BubbleId,
        audio_id: Option<crate::comic_dubs::ComicAudioId>,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.assign_audio(bubble_id, audio_id);
        self.comic_dubs_commit(before);
        let Some(audio) = audio_id.and_then(|id| self.comic_dubs_project.audio(id)) else {
            return;
        };
        let mut player = VideoPlayer::new();
        match player.load_audio_only(&audio.playback_path, audio.duration_ms() as f64 / 1_000.0) {
            Ok(()) => {
                player.set_volume(self.ui_shell.ui.volume());
                let _ = player.toggle();
                self.ui_shell.ui.total_frames = player.total_frames();
                self.ui_shell.ui.set_playing(true);
                self.comic_dubs_player = Some(player);
            }
            Err(error) => self.show_toast(format!("Audio Comic Dubs : {error}"), 5.0),
        }
    }

    pub fn comic_dubs_remove_bubble(&mut self, bubble_id: crate::comic_dubs::BubbleId) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.remove_bubble(bubble_id);
        self.comic_dubs_commit(before);
    }

    pub fn comic_dubs_move_bubble(&mut self, bubble_id: crate::comic_dubs::BubbleId, delta: isize) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.move_bubble(bubble_id, delta);
        self.comic_dubs_commit(before);
    }

    fn toggle_comic_dubs_playback(&mut self) {
        if self.comic_dubs_playback.is_some() {
            self.stop_comic_dubs_playback();
            return;
        }
        let Some(page_index) = comic_dubs_start_page(&self.comic_dubs_project) else {
            self.show_toast("Aucune bulle à lire", 3.0);
            return;
        };
        self.comic_dubs_playback = Some(ComicDubsPlayback {
            page_index,
            bubble_index: 0,
            started_at: Instant::now(),
            deadline: Instant::now(),
        });
        self.ui_shell.ui.set_playing(true);
        self.start_comic_dubs_bubble(Instant::now());
    }

    fn start_comic_dubs_bubble(&mut self, now: Instant) {
        let Some(playback) = self.comic_dubs_playback.as_ref() else {
            return;
        };
        let page_index = playback.page_index;
        let bubble_index = playback.bubble_index;
        let Some(page) = self.comic_dubs_project.pages().get(page_index) else {
            self.stop_comic_dubs_playback();
            return;
        };
        let Some(bubble) = page.bubbles.get(bubble_index) else {
            self.stop_comic_dubs_playback();
            return;
        };
        let animation_duration_ms = bubble.vertex_animation_duration_ms();
        let page_id = page.id;
        let audio = bubble.audio_id.and_then(|id| {
            self.comic_dubs_project
                .audio(id)
                .map(|audio| (audio.playback_path.clone(), audio.duration_ms()))
        });
        let gap_ms = next_comic_dubs_position(&self.comic_dubs_project, page_index, bubble_index)
            .map(|(_, _, gap_ms)| gap_ms)
            .unwrap_or(0);

        self.comic_dubs_project.select_page(page_id);
        self.ui_shell
            .ui
            .set_comic_dubs_playback(Some(page_id), bubble_index + 1, 0);
        self.comic_dubs_player = None;
        let mut duration_ms = 0;
        if let Some((path, audio_duration_ms)) = audio {
            let mut player = VideoPlayer::new();
            match player.load_audio_only(&path, audio_duration_ms as f64 / 1_000.0) {
                Ok(()) => {
                    player.set_volume(self.ui_shell.ui.volume());
                    let _ = player.toggle();
                    self.ui_shell.ui.total_frames = player.total_frames();
                    duration_ms = audio_duration_ms;
                    self.comic_dubs_player = Some(player);
                }
                Err(error) => self.show_toast(format!("Audio Comic Dubs : {error}"), 5.0),
            }
        }
        if let Some(playback) = self.comic_dubs_playback.as_mut() {
            playback.started_at = now;
            playback.deadline = now
                + Duration::from_millis(
                    duration_ms
                        .max(animation_duration_ms)
                        .saturating_add(gap_ms),
                );
        }
    }

    fn tick_comic_dubs_playback(&mut self, now: Instant) {
        if let Some(playback) = self.comic_dubs_playback.as_ref() {
            if let Some(page) = self.comic_dubs_project.pages().get(playback.page_index) {
                self.ui_shell.ui.set_comic_dubs_playback(
                    Some(page.id),
                    playback.bubble_index + 1,
                    now.saturating_duration_since(playback.started_at).as_millis() as u64,
                );
            }
        }
        let deadline = self
            .comic_dubs_playback
            .as_ref()
            .map(|playback| playback.deadline);
        let audio_playing = self
            .comic_dubs_player
            .as_ref()
            .is_some_and(|player| player.is_playing());
        if !deadline.is_some_and(|deadline| comic_dubs_playback_due(now, deadline, audio_playing)) {
            return;
        }
        let Some(playback) = self.comic_dubs_playback.as_ref() else {
            return;
        };
        let next = next_comic_dubs_position(
            &self.comic_dubs_project,
            playback.page_index,
            playback.bubble_index,
        );
        if let Some((page_index, bubble_index, _)) = next {
            let playback = self.comic_dubs_playback.as_mut().unwrap();
            playback.page_index = page_index;
            playback.bubble_index = bubble_index;
            self.start_comic_dubs_bubble(now);
        } else {
            self.stop_comic_dubs_playback();
        }
    }

    fn stop_comic_dubs_playback(&mut self) {
        if let Some(player) = &mut self.comic_dubs_player {
            player.pause_for_seek();
        }
        self.comic_dubs_player = None;
        self.comic_dubs_playback = None;
        self.ui_shell.ui.set_comic_dubs_playback(None, 0, 0);
        self.ui_shell.ui.set_playing(false);
    }

    pub(crate) fn reset_comic_dubs_document(&mut self) {
        self.stop_comic_dubs_playback();
        self.comic_dubs_project = crate::comic_dubs::ComicDubsProject::default();
        self.comic_dubs_revision = 0;
        self.comic_dubs_undo.clear();
        self.comic_dubs_redo.clear();
        self.comic_dubs_imports.clear();
        self.ui_shell.ui.reset_comic_dubs_workspace();
    }

    fn comic_dubs_commit(&mut self, before: crate::comic_dubs::ComicDubsProject) {
        if before == self.comic_dubs_project {
            return;
        }
        if self.comic_dubs_playback.is_some() {
            self.stop_comic_dubs_playback();
        }
        // ponytail: snapshots are capped; use operation deltas only if large comics make this measurable.
        if self.comic_dubs_undo.len() == 20 {
            self.comic_dubs_undo.remove(0);
        }
        self.comic_dubs_undo.push(before);
        self.comic_dubs_redo.clear();
        self.comic_dubs_revision = self.comic_dubs_revision.wrapping_add(1);
        self.project_session.dirty = true;
    }

    pub fn comic_dubs_undo(&mut self) {
        let Some(previous) = self.comic_dubs_undo.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.comic_dubs_project, previous);
        self.comic_dubs_redo.push(current);
        self.comic_dubs_revision = self.comic_dubs_revision.wrapping_add(1);
        self.project_session.dirty = true;
    }

    pub fn comic_dubs_redo(&mut self) {
        let Some(next) = self.comic_dubs_redo.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.comic_dubs_project, next);
        self.comic_dubs_undo.push(current);
        self.comic_dubs_revision = self.comic_dubs_revision.wrapping_add(1);
        self.project_session.dirty = true;
    }

    fn poll_comic_dubs_jobs(&mut self) -> bool {
        enum Completed {
            Image(Result<(u32, u32), String>),
            Audio(Result<crate::recording::RecordedAudio, String>),
        }
        let mut changed = false;
        let mut index = self.comic_dubs_imports.len();
        while index > 0 {
            index -= 1;
            let completed = match &self.comic_dubs_imports[index] {
                PendingComicDubsImport::Image { receiver, .. } => match receiver.try_recv() {
                    Ok(result) => Some(Completed::Image(result)),
                    Err(TryRecvError::Disconnected) => {
                        Some(Completed::Image(Err("conversion PNG interrompue".into())))
                    }
                    Err(TryRecvError::Empty) => None,
                },
                PendingComicDubsImport::Audio { receiver, .. } => match receiver.try_recv() {
                    Ok(result) => Some(Completed::Audio(result.map_err(|error| error.to_string()))),
                    Err(TryRecvError::Disconnected) => {
                        Some(Completed::Audio(Err("conversion FLAC interrompue".into())))
                    }
                    Err(TryRecvError::Empty) => None,
                },
            };
            let Some(completed) = completed else { continue };
            let pending = self.comic_dubs_imports.swap_remove(index);
            let pending_audio = self
                .comic_dubs_imports
                .iter()
                .filter(|job| matches!(job, PendingComicDubsImport::Audio { .. }))
                .count();
            self.ui_shell
                .ui
                .set_comic_dubs_pending_audio_imports(pending_audio);
            match (pending, completed) {
                (
                    PendingComicDubsImport::Image {
                        source_path,
                        output_path,
                        ..
                    },
                    Completed::Image(Ok((width, height))),
                ) => {
                    self.recording_runtime
                        .remember_owned_file(output_path.clone());
                    let before = self.comic_dubs_project.clone();
                    self.comic_dubs_project.add_page(
                        source_path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "page.png".into()),
                        output_path,
                        width,
                        height,
                    );
                    self.comic_dubs_commit(before);
                }
                (
                    PendingComicDubsImport::Audio {
                        source_path,
                        output_path,
                        ..
                    },
                    Completed::Audio(Ok(recorded)),
                ) => {
                    self.recording_runtime
                        .remember_external_audio(&recorded, output_path.clone());
                    let before = self.comic_dubs_project.clone();
                    self.comic_dubs_project.add_audio(
                        source_path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "audio.flac".into()),
                        output_path,
                        recorded,
                    );
                    self.comic_dubs_commit(before);
                }
                (
                    PendingComicDubsImport::Image { output_path, .. },
                    Completed::Image(Err(error)),
                )
                | (
                    PendingComicDubsImport::Audio { output_path, .. },
                    Completed::Audio(Err(error)),
                ) => {
                    let _ = std::fs::remove_file(output_path);
                    self.show_toast(error, 6.0);
                }
                _ => unreachable!("Comic Dubs import result must match its job"),
            }
            changed = true;
        }
        changed
    }

    pub fn handle_ui_event(&mut self, event: &UiEvent) -> EventResponse {
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
        let render_frame = self.render_frame();
        let fps = self.active_fps();
        self.project_session
            .render_index
            .refresh(&self.project_session.project);
        let response = self.ui_shell.ui.handle_event(
            event,
            &self.project_session.project,
            &self.voicelines_project,
            &self.comic_dubs_project,
            &self.project_session.render_index,
            render_frame,
            fps,
        );
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui();
        }
        response
    }

    pub fn active_workspace(&self) -> WorkspaceId {
        self.workspace_host.active_id()
    }

    pub fn activate_workspace(&mut self, workspace: WorkspaceId) {
        if workspace != WorkspaceId::Recording
            && self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Actor
            )
        {
            return;
        }
        let previous = self.active_workspace();
        if previous != workspace {
            if previous == WorkspaceId::ComicDubs {
                self.stop_comic_dubs_playback();
            } else if previous == WorkspaceId::Voicelines {
                if let Some(player) = &mut self.voicelines_player {
                    player.pause_for_seek();
                }
            } else if matches!(workspace, WorkspaceId::Voicelines | WorkspaceId::ComicDubs) {
                if let Some(player) = &mut self.playback.video_player {
                    player.pause_for_seek();
                }
            }
            self.ui_shell.ui.set_playing(false);
        }
        let changed = self.workspace_host.activate(workspace);
        if changed || self.ui_shell.ui.active_workspace() != workspace {
            self.ui_shell.ui.set_active_workspace(workspace);
        }
        if workspace == WorkspaceId::Voicelines
            && self.voicelines_player.is_none()
            && self.voicelines_project.active_audio().is_some()
        {
            if let Err(error) = self.voicelines_load_active_player() {
                self.show_toast(error, 5.0);
            }
        }
        self.ui_shell.ui.total_frames = match workspace {
            WorkspaceId::Voicelines => self
                .voicelines_player
                .as_ref()
                .map_or(0, VideoPlayer::total_frames),
            WorkspaceId::ComicDubs => self
                .comic_dubs_player
                .as_ref()
                .map_or(0, VideoPlayer::total_frames),
            _ => self
                .playback
                .video_player
                .as_ref()
                .map_or(0, VideoPlayer::total_frames),
        };
        let label = match workspace {
            WorkspaceId::Rythmo => crate::i18n::t("workspace_tabs.rythmo"),
            WorkspaceId::Recording => crate::i18n::t("workspace_tabs.recording"),
            WorkspaceId::Voicelines => crate::i18n::t("workspace_tabs.voicelines"),
            WorkspaceId::ComicDubs => crate::i18n::t("workspace_tabs.comic_dubs"),
        };
        self.announce_accessibility(AccessibilityEvent::Selection {
            label: format!(
                "{}: {label}",
                crate::i18n::t("accessibility.workspace_selected")
            ),
        });
        match workspace {
            WorkspaceId::Rythmo | WorkspaceId::Voicelines | WorkspaceId::ComicDubs => {
                self.jobs.play_recording_mix_when_ready = false;
                self.clear_recording_mix_preview();
            }
            WorkspaceId::Recording
                if self.ui_shell.ui.recording_page()
                    == crate::ui::recording_workspace::RecordingPage::Timeline =>
            {
                self.schedule_recording_mix();
            }
            WorkspaceId::Recording => {}
        }
    }

    fn recording_network_role(&self) -> crate::ui::recording_workspace::RecordingRole {
        use crate::ui::recording_workspace::RecordingRole;

        let network = &self.collaboration.network;
        let role = network
            .member_id
            .as_deref()
            .and_then(|member_id| {
                network
                    .member_details
                    .iter()
                    .find(|member| member.id == member_id)
            })
            .map(|member| member.role.as_str())
            .or(network.role.as_deref())
            .unwrap_or("actor");
        match role {
            "admin" => RecordingRole::Director,
            "co_da" => RecordingRole::CoDirector {
                has_control: network.member_id.as_deref().is_some_and(|member_id| {
                    network.control_owner_id.as_deref() == Some(member_id)
                }),
            },
            _ => RecordingRole::Actor,
        }
    }

    fn enter_online_recording_view(&mut self) {
        if !self.collaboration.network.is_in_room() {
            return;
        }
        let role = self.recording_network_role();
        self.ui_shell.ui.recording_enter_online(role);
        self.activate_workspace(WorkspaceId::Recording);
        self.rebuild_topbar_for_network();
        self.sync_recording_workspace_ui();
        self.schedule_recording_mix();
        self.ensure_recording_input_ready();
    }

    fn ensure_recording_input_ready(&mut self) {
        if !matches!(
            self.recording_network_role(),
            crate::ui::recording_workspace::RecordingRole::Actor
        ) {
            return;
        }
        let device = crate::config::recording_input_device();
        if self
            .recording_input_preflight
            .as_ref()
            .is_some_and(|(checked, _)| checked == &device)
        {
            return;
        }
        match crate::media_recording::preflight_input_device(device.as_deref()) {
            Ok(()) => {
                self.recording_input_preflight = Some((device, true));
                self.collaboration.network.send_recording_ready(true);
                self.show_toast(crate::i18n::t("recording.microphone.ready"), 3.0);
            }
            Err(error) => {
                self.recording_input_preflight = Some((device, false));
                self.collaboration.network.send_recording_ready(false);
                self.recording_error(error.to_string());
            }
        }
    }

    pub fn sync_recording_workspace_ui(&mut self) {
        let current_frame = self.render_frame();
        self.sync_recording_workspace_ui_at(current_frame);
    }

    fn sync_recording_workspace_ui_at(&mut self, current_frame: f64) {
        self.project_session
            .render_index
            .refresh(&self.project_session.project);
        let capture = self.recording_runtime.capture_state();
        let countdown_seconds = self.recording_runtime.countdown_seconds_remaining();
        if countdown_seconds != self.last_recording_countdown_second {
            self.last_recording_countdown_second = countdown_seconds;
            if let Some(seconds @ 1..=3) = countdown_seconds {
                crate::accessibility::countdown_tone(seconds);
            }
        }
        self.ui_shell.ui.sync_recording_scene(
            &self.project_session.render_index,
            &self.project_session.recording_project,
            self.project_session.project.settings().scroll_speed,
            self.project_session
                .project
                .settings()
                .reading_bar_offset_percent,
            capture,
            &self.collaboration.network.member_details,
            self.collaboration.network.control_owner_id.as_deref(),
            current_frame,
            countdown_seconds,
        );
        self.sync_recording_daw_ui_at(current_frame);
    }

    fn sync_recording_daw_ui(&mut self) {
        let current_frame = self.render_frame();
        self.sync_recording_daw_ui_at(current_frame);
    }

    fn sync_recording_daw_ui_at(&mut self, current_frame: f64) {
        if self.window_manager.secondary_kind != Some(SecondaryWindowKind::Daw) {
            return;
        }
        let Some(display) = self.window_manager.secondary_display.as_ref() else {
            return;
        };
        let (width, height) = (display.config.width as f32, display.config.height as f32);
        self.ui_shell.ui.sync_recording_daw_scene(
            width,
            height,
            &self.project_session.recording_project,
            self.project_session.project.settings().scroll_speed,
            self.project_session
                .project
                .settings()
                .reading_bar_offset_percent,
            self.recording_runtime.capture_state(),
            &self.collaboration.network.member_details,
            self.collaboration.network.control_owner_id.as_deref(),
            current_frame,
            self.recording_runtime.countdown_seconds_remaining(),
        );
    }

    pub fn handle_secondary_daw_event(&mut self, event: &UiEvent) -> EventResponse {
        if self.window_manager.secondary_kind != Some(SecondaryWindowKind::Daw) {
            return EventResponse::Ignored;
        }
        self.sync_recording_daw_ui();
        let response = self.ui_shell.ui.handle_recording_daw_event(event);
        self.sync_recording_daw_ui();
        response
    }

    pub fn recording_choose_solo(&mut self) {
        self.ui_shell.ui.recording_enter_solo();
        self.sync_recording_workspace_ui();
        self.schedule_recording_mix();
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
        self.schedule_recording_mix();
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

    pub fn recording_add_track(&mut self) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let next_number = self.project_session.recording_project.tracks().count() + 1;
        let id = self.project_session.recording_project.allocate_track_id();
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::AddTrack {
                track: crate::recording::AudioTrack::new(
                    id,
                    crate::i18n::t("recording.track.new")
                        .replace("{number}", &next_number.to_string()),
                ),
            })
        {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_begin_audio_import(
        &mut self,
        path: std::path::PathBuf,
        drop_position: Option<(f32, f32)>,
    ) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let placement = match drop_position {
            Some((x, y)) => match self.ui_shell.ui.recording_drop_target(x, y) {
                Some(target) => Some(target),
                None => {
                    self.recording_error(crate::i18n::t("recording.audio.drop_on_track"));
                    return;
                }
            },
            None => None,
        };
        self.recording_prompt_audio_import(path, placement);
    }

    pub fn recording_begin_daw_audio_import(
        &mut self,
        path: std::path::PathBuf,
        drop_position: (f32, f32),
    ) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let Some(placement) = self
            .ui_shell
            .ui
            .recording_daw_drop_target(drop_position.0, drop_position.1)
        else {
            self.recording_error(crate::i18n::t("recording.audio.drop_on_track"));
            return;
        };
        self.recording_prompt_audio_import(path, Some(placement));
    }

    fn recording_prompt_audio_import(
        &mut self,
        path: std::path::PathBuf,
        placement: Option<(crate::recording::AudioTrackId, i64)>,
    ) {
        let username = self.recording_username();
        self.ui_shell
            .ui
            .recording_begin_audio_import(path, placement, username);
        self.sync_recording_workspace_ui();
        self.announce_open_container(
            crate::i18n::t("recording.audio.username_prompt"),
            crate::i18n::t("recording.audio.username_label").to_string(),
        );
    }

    pub fn recording_import_audio(
        &mut self,
        path: std::path::PathBuf,
        username: String,
        placement: Option<(crate::recording::AudioTrackId, i64)>,
    ) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        match self
            .recording_runtime
            .import_external_audio(&path, &username)
        {
            Ok(audio) => {
                let (id, file_name, operation) = imported_audio_operation(
                    &mut self.project_session.recording_project,
                    audio,
                    placement,
                );
                match self.apply_recording_operation(operation) {
                    Ok(()) => {
                        self.ui_shell.ui.recording_reveal_asset(&file_name, id);
                        self.sync_recording_workspace_ui();
                        self.show_toast(crate::i18n::t("recording.audio.imported"), 3.0);
                    }
                    Err(error) => self.recording_error(error.to_string()),
                }
            }
            Err(error) => self.recording_error(error.to_string()),
        }
    }

    pub fn recording_begin_rename_track(&mut self, track_id: crate::recording::AudioTrackId) {
        if self.ui_shell.ui.recording_can_edit_timeline() {
            self.ui_shell
                .ui
                .recording_begin_rename_track(&self.project_session.recording_project, track_id);
            self.sync_recording_workspace_ui();
        } else {
            self.recording_read_only_error();
        }
    }

    pub fn recording_remove_track(&mut self, track_id: crate::recording::AudioTrackId) {
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::RemoveTrack {
                track_id,
            })
        {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_rename_track(
        &mut self,
        track_id: crate::recording::AudioTrackId,
        name: String,
    ) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::RenameTrack {
                track_id,
                name,
            })
        {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_place_asset(
        &mut self,
        asset_id: crate::recording::AudioAssetId,
        track_id: crate::recording::AudioTrackId,
        start_frame: i64,
    ) {
        let Some(asset) = self.project_session.recording_project.asset(asset_id) else {
            return;
        };
        let duration_frames =
            asset.duration_frames(self.project_session.recording_project.timeline_fps());
        let clip_id = self.project_session.recording_project.allocate_clip_id();
        let operation = crate::recording::RecordingOperation::AddClip {
            clip: crate::recording::AudioClip {
                id: clip_id,
                asset_id,
                track_id,
                start_frame: start_frame.max(0),
                source_start_frame: 0,
                duration_frames,
            },
        };
        if let Err(error) = self.apply_recording_operation(operation) {
            self.recording_error(error.to_string());
        }
    }

    pub fn recording_move_selected_clips(
        &mut self,
        track_id: crate::recording::AudioTrackId,
        delta_frames: i64,
    ) {
        if self
            .project_session
            .recording_project
            .track(track_id)
            .is_none()
        {
            return;
        }
        let clip_ids = self
            .ui_shell
            .ui
            .recording_editor_mut()
            .selected_clips()
            .collect::<Vec<_>>();
        let minimum_start = clip_ids
            .iter()
            .filter_map(|clip_id| self.project_session.recording_project.clip(*clip_id))
            .map(|clip| clip.start_frame)
            .min();
        let Some(minimum_start) = minimum_start else {
            return;
        };
        let effective_delta = delta_frames.max(-minimum_start);
        let placements = clip_ids
            .iter()
            .filter_map(|clip_id| {
                self.project_session
                    .recording_project
                    .clip(*clip_id)
                    .map(|clip| crate::recording::ClipPlacement {
                        clip_id: *clip_id,
                        track_id,
                        start_frame: clip.start_frame.saturating_add(effective_delta),
                    })
            })
            .collect::<Vec<_>>();
        let changed = placements.iter().any(|placement| {
            self.project_session
                .recording_project
                .clip(placement.clip_id)
                .is_some_and(|clip| {
                    clip.track_id != placement.track_id || clip.start_frame != placement.start_frame
                })
        });
        if changed {
            if let Err(error) =
                self.apply_recording_operation(crate::recording::RecordingOperation::MoveClips {
                    placements,
                })
            {
                self.recording_error(error.to_string());
            }
        }
    }

    pub fn recording_cut_clip(&mut self, clip_id: crate::recording::AudioClipId, at_frame: i64) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let Some(clip) = self.project_session.recording_project.clip(clip_id) else {
            self.recording_error(
                crate::recording::RecordingError::MissingClip(clip_id).to_string(),
            );
            return;
        };
        if at_frame <= clip.start_frame || at_frame >= clip.end_frame() {
            self.recording_error(
                crate::recording::RecordingError::InvalidClip(
                    clip_id,
                    "cut must be strictly inside the clip".into(),
                )
                .to_string(),
            );
            return;
        }
        let right_clip_id = self.project_session.recording_project.allocate_clip_id();
        if let Err(error) =
            self.apply_recording_operation(crate::recording::RecordingOperation::SplitClip {
                clip_id,
                at_frame,
                right_clip_id,
            })
        {
            self.recording_error(error.to_string());
            return;
        }
        let project = &self.project_session.recording_project;
        let _ = self
            .ui_shell
            .ui
            .recording_select_clip(project, clip_id, false);
        let _ = self
            .ui_shell
            .ui
            .recording_select_clip(project, right_clip_id, true);
        self.sync_recording_workspace_ui();
    }

    pub fn recording_delete_selected_clips(&mut self) {
        let clip_ids = self
            .ui_shell
            .ui
            .recording_editor_mut()
            .selected_clips()
            .collect::<Vec<_>>();
        if clip_ids.is_empty() {
            return;
        }
        match self.apply_recording_operation(crate::recording::RecordingOperation::DeleteClips {
            clip_ids,
        }) {
            Ok(()) => {
                self.ui_shell.ui.recording_editor_mut().clear_selection();
                self.sync_recording_workspace_ui();
            }
            Err(error) => self.recording_error(error.to_string()),
        }
    }

    pub fn recording_delete_selected_asset(&mut self) {
        let Some(asset_id) = self.ui_shell.ui.recording_selected_asset() else {
            return;
        };
        let clip_ids = self
            .project_session
            .recording_project
            .clips()
            .filter(|clip| clip.asset_id == asset_id)
            .map(|clip| clip.id)
            .collect::<Vec<_>>();
        let mut operations = clip_ids
            .into_iter()
            .map(
                |clip_id| crate::recording::RecordingOperation::DeleteClips {
                    clip_ids: vec![clip_id],
                },
            )
            .collect::<Vec<_>>();
        operations.push(crate::recording::RecordingOperation::RemoveAsset { asset_id });
        match self
            .apply_recording_operation(crate::recording::RecordingOperation::Batch { operations })
        {
            Ok(()) => {
                if let Some(path) = self.project_session.recording_asset_paths.remove(&asset_id) {
                    self.playback.recording_audio_cache.remove(&path);
                }
                self.ui_shell.ui.recording_clear_asset_selection();
                self.sync_recording_workspace_ui();
            }
            Err(error) => self.recording_error(error.to_string()),
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
        self.bind_recording_audio_paths(&transaction.operation);
        self.project_session.mark_recording_changed();
        if self.ui_shell.ui.recording_role().is_online() {
            self.collaboration
                .network
                .send_recording_transaction(&transaction);
        }
        self.sync_recording_workspace_ui();
        self.schedule_recording_mix();
        Ok(())
    }

    fn bind_recording_audio_paths(&mut self, operation: &crate::recording::RecordingOperation) {
        let mut assets = Vec::new();
        recording_added_assets(operation, &mut assets);
        for asset in assets {
            if let Some(path) = self.recording_runtime.audio_path(&asset.checksum).cloned() {
                self.project_session
                    .recording_asset_paths
                    .insert(asset.id, path);
            }
        }
    }

    fn schedule_recording_mix(&mut self) {
        if self.active_workspace() != WorkspaceId::Recording
            || self.ui_shell.ui.recording_page()
                != crate::ui::recording_workspace::RecordingPage::Timeline
        {
            return;
        }
        if self.project_session.recording_project.clips().len() == 0 {
            self.clear_recording_mix_preview();
            self.start_deferred_recording_playback();
            return;
        }
        let Some(source) = self.playback.source_video_path.clone() else {
            return;
        };
        let mut spec = match crate::recording_mix::RecordingMixSpec::from_project(
            &self.project_session.recording_project,
            &self.project_session.recording_asset_paths,
            None,
            None,
        ) {
            Ok(spec) => spec,
            Err(error) => {
                log::warn!("Recording preview mix is not ready: {error}");
                return;
            }
        };
        spec.set_source_volume(self.ui_shell.ui.volume());
        if self.ui_shell.ui.recording_role().can_adjust_track_volume() {
            for track in self.project_session.recording_project.tracks() {
                spec.set_track_volume(track.id, self.ui_shell.ui.recording_track_volume(track.id));
            }
        }
        if let Some(job) = self.jobs.pending_recording_mix_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let mut missing_paths = Vec::new();
        for clip in &spec.clips {
            if !self.playback.recording_audio_cache.contains_key(&clip.path)
                && !missing_paths.contains(&clip.path)
            {
                missing_paths.push(clip.path.clone());
            }
        }
        if missing_paths.is_empty() {
            let output_sample_rate = self
                .playback
                .video_player
                .as_ref()
                .and_then(|p| p.audio_output_sample_rate())
                .unwrap_or(REALTIME_SAMPLE_RATE);
            match crate::recording_mix::RealtimeRecordingMix::from_spec(
                &spec,
                &self.playback.recording_audio_cache,
                output_sample_rate,
            ) {
                Ok(mix) => {
                    if let Some(player) = &mut self.playback.video_player {
                        player.set_recording_mix(Some(Arc::new(mix)));
                    }
                    self.start_deferred_recording_playback();
                }
                Err(error) => {
                    self.jobs.play_recording_mix_when_ready = false;
                    self.recording_error(error);
                }
            }
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = missing_paths
                .into_iter()
                .map(|path| {
                    crate::recording_mix::decode_realtime_asset(&path, &worker_cancel)
                        .map(|samples| (path, samples))
                })
                .collect();
            let _ = sender.send(result);
        });
        self.jobs.pending_recording_mix_job = Some(PendingRecordingMixJob { cancel, receiver });
    }

    fn clear_recording_mix_preview(&mut self) {
        if let Some(job) = self.jobs.pending_recording_mix_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(player) = &mut self.playback.video_player {
            player.set_recording_mix(None);
        }
    }

    fn start_deferred_recording_playback(&mut self) {
        if self.jobs.play_recording_mix_when_ready && self.jobs.pending_recording_mix_job.is_none()
        {
            self.jobs.play_recording_mix_when_ready = false;
            self.toggle_play_pause_internal(false);
        }
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

    pub fn recording_set_track_volume(
        &mut self,
        track_id: crate::recording::AudioTrackId,
        volume: f32,
    ) {
        if !self.ui_shell.ui.recording_role().can_adjust_track_volume() {
            self.recording_read_only_error();
            return;
        }
        if self
            .project_session
            .recording_project
            .track(track_id)
            .is_none()
        {
            return;
        }
        self.ui_shell
            .ui
            .recording_set_track_volume(track_id, volume);
        self.sync_recording_workspace_ui();
        self.schedule_recording_mix();
    }

    pub fn recording_adjust_track_volume(
        &mut self,
        track_id: crate::recording::AudioTrackId,
        delta: f32,
    ) {
        let current = self.ui_shell.ui.recording_track_volume(track_id);
        self.recording_set_track_volume(track_id, current + delta);
    }

    pub fn recording_export_track(
        &mut self,
        track_id: crate::recording::AudioTrackId,
    ) -> Option<crate::application::command::FilePickerRequest> {
        let project = &self.project_session.recording_project;
        let Some(track) = project.track(track_id) else {
            return None;
        };

        let project_dir = self
            .project_session
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let safe_name = track
            .name
            .chars()
            .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            .collect::<String>();

        Some(crate::application::command::FilePickerRequest {
            title: crate::i18n::t("recording.track.export").to_string(),
            mode: crate::application::command::FilePickerMode::Save,
            intent: crate::application::command::FilePickerIntent::ExportRecordingTrack {
                track_id,
            },
            filters: vec![crate::application::command::FileFilterSpec {
                name: "FLAC Audio".to_string(),
                extensions: vec!["flac".to_string()],
            }],
            initial_dir: Some(project_dir),
            default_extension: Some("flac".to_string()),
            initial_filename: Some(safe_name),
        })
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

    pub fn recording_send_asset_to_voicelines(&mut self, asset_id: crate::recording::AudioAssetId) {
        let Some(path) = self
            .project_session
            .recording_asset_paths
            .get(&asset_id)
            .cloned()
        else {
            self.show_toast("Audio d'enregistrement introuvable", 4.0);
            return;
        };
        self.voicelines_begin_audio_import(path);
    }

    pub fn recording_start_capture(&mut self) {
        if !self.ui_shell.ui.recording_can_edit_timeline() {
            self.recording_read_only_error();
            return;
        }
        let role = self.ui_shell.ui.recording_role();
        if role.is_online() {
            let waiting = self
                .collaboration
                .network
                .member_details
                .iter()
                .filter(|member| member.role == "actor" && !member.muted && !member.recording_ready)
                .map(|member| member.username.as_str())
                .collect::<Vec<_>>();
            if !waiting.is_empty() {
                self.show_toast(
                    crate::i18n::t("recording.capture.waiting_for_microphones")
                        .replace("{actors}", &waiting.join(", ")),
                    6.0,
                );
                return;
            }
        }
        let username = self.recording_username();
        let input_device = crate::config::recording_input_device();
        let result = if role.is_online() {
            self.project_session
                .recording_project
                .armed_track_id()
                .ok_or_else(|| {
                    crate::recording::RecordingError::Recorder("no recording track is armed".into())
                })
                .and_then(|track_id| {
                    self.project_session
                        .recording_project
                        .propose_capture_target(track_id, self.current_frame())
                })
                .and_then(|target| self.recording_runtime.begin_observed_capture(target))
        } else {
            self.recording_runtime.begin_capture(
                &self.project_session.recording_project,
                self.current_frame(),
                &username,
                input_device.as_deref(),
            )
        };
        if let Err(error) = result {
            self.recording_error(error.to_string());
            return;
        }
        // A mix job requested before Record must not resume playback in the
        // countdown; the capture start event owns the next Play transition.
        self.jobs.play_recording_mix_when_ready = false;
        let capture_start_frame = self.recording_runtime.capture_state().and_then(|state| {
            if let crate::recording::CaptureState::Countdown { target, .. } = state {
                Some(target.start_frame)
            } else {
                None
            }
        });
        if let Some(start_frame) = capture_start_frame {
            self.seek_absolute_internal(start_frame, false);
            self.finish_seek();
        }

        if self.ui_shell.ui.recording_role().is_online() {
            let capture_target = match self.recording_runtime.capture_state() {
                Some(crate::recording::CaptureState::Countdown { target, .. }) => Some(*target),
                _ => None,
            };
            // Applying this canonical snapshot starts the remote countdown;
            // peers can never record against an older transaction log.
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
        let online = self.ui_shell.ui.recording_role().is_online();
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
        if online {
            self.collaboration.network.send_recording_prepare(
                &crate::network::RecordingPreparePayload {
                    project: self.project_session.recording_project.clone(),
                    transactions: self.project_session.recording_transactions.clone(),
                    current_frame: self.current_frame(),
                    capture_target: None,
                },
            );
        }
        self.sync_recording_workspace_ui();
    }

    pub fn open_recording_input_device_modal(&mut self) {
        match crate::media_recording::input_device_names() {
            Ok(devices) => {
                let selected = crate::config::recording_input_device();
                self.ui_shell
                    .ui
                    .open_recording_input_device_modal(devices, selected);
                self.announce_open_container(
                    crate::i18n::t("recording.microphone.title"),
                    crate::i18n::t("recording.microphone.default").to_string(),
                );
            }
            Err(error) => self.recording_error(error.to_string()),
        }
    }

    pub fn request_actors_open_microphone(&self) {
        if self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Director
            )
        {
            self.collaboration.network.send_raw(
                "actor_request",
                serde_json::json!({ "action": "open_microphone" }),
            );
        }
    }

    pub fn request_actors_transfer_display_settings(&self) {
        if self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Director
            )
        {
            let settings = self.project_session.project.settings();
            self.collaboration.network.send_raw(
                "actor_request",
                serde_json::json!({
                    "action": "apply_display_settings",
                    "scroll_speed": settings.scroll_speed,
                    "reading_bar_offset_percent": settings.reading_bar_offset_percent,
                }),
            );
        }
    }

    pub fn request_actors_close_project_transfer_waiting(&self) {
        if self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Director
            )
        {
            self.collaboration.network.send_raw(
                "actor_request",
                serde_json::json!({ "action": "close_project_transfer_waiting" }),
            );
        }
    }

    fn close_project_transfer_waiting(&mut self) {
        let transfer = self.project_transfer.as_ref().map(|runtime| {
            let response = self
                .collaboration
                .network
                .member_id
                .as_deref()
                .and_then(|member_id| {
                    runtime
                        .status
                        .as_ref()?
                        .participants
                        .iter()
                        .find(|participant| participant.member_id == member_id)
                        .map(|participant| participant.response.clone())
                });
            (
                runtime.metadata.request_id.clone(),
                runtime.status.as_ref().map(|status| status.phase.clone()),
                response,
                runtime.receiver.is_active(),
            )
        });

        if let Some((request_id, phase, response, receiver_active)) = &transfer {
            if (*receiver_active && !matches!(phase.as_deref(), Some("collecting")))
                || (matches!(phase.as_deref(), Some("transferring" | "finishing"))
                    && matches!(response.as_deref(), Some("receiving" | "loading")))
            {
                self.collaboration.network.report_project_transfer(
                    request_id,
                    false,
                    Some("transfer waiting closed by the director"),
                );
            }
            self.project_transfer_waiting_dismissed = Some(request_id.clone());
            if self
                .jobs
                .pending_import_job
                .as_ref()
                .is_some_and(|job| job.transfer_request_id.as_deref() == Some(request_id.as_str()))
            {
                self.jobs.pending_import_job = None;
                self.ui_shell.ui.finish_project_load();
            }
        }

        if let Some(runtime) = self.project_transfer.as_mut() {
            runtime.receiver.cancel();
        }
        self.project_transfer = None;
        self.project_transfer_loading_request = None;
        self.ui_shell.ui.close_project_transfer_modal();
        self.ui_shell.ui.sync_overlay = None;
        self.ui_shell.ui.sync_progress = 0.0;
        self.narration.publish_progress(String::new(), None);
        self.show_toast(crate::i18n::t("recording.project_transfer.dismissed"), 5.0);
    }

    pub fn request_actors_project_transfer(&mut self) {
        if !self.collaboration.network.is_in_room()
            || !matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Director
            )
        {
            return;
        }
        let Some(path) = self.project_session.project_path.clone() else {
            self.show_toast(
                crate::i18n::t("recording.project_transfer_requires_saved"),
                5.0,
            );
            return;
        };
        if self.project_session.dirty {
            let source_video = self.video_path();
            let proxy_video = source_video.as_ref().and_then(|_| self.proxy_video_path());
            let font_asset = crate::vector_text::selected_font_asset().map(|(_, path)| path);
            if self.start_project_save(
                path,
                source_video,
                proxy_video,
                font_asset,
                SaveContinuation::ProjectTransfer,
            ) {
                self.show_toast(
                    crate::i18n::t("recording.project_transfer.save_and_transfer"),
                    4.0,
                );
            }
            return;
        }
        let Some(project_huuid) = self.project_session.huuid.clone() else {
            self.show_toast(
                crate::i18n::t("recording.project_transfer_requires_saved"),
                5.0,
            );
            return;
        };
        let request_id = format!(
            "project_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        );
        let prepare_path = path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::file_transfer::FileTransferMetadata::from_path(
                request_id.clone(),
                &prepare_path,
            )
            .map(|file| ProjectTransferMetadata {
                request_id,
                project_huuid: project_huuid.to_string(),
                file_name: file.file_name,
                total_bytes: file.total_bytes,
                total_chunks: file.total_chunks,
                chunk_size: file.chunk_size,
                sha1: file.sha1,
            });
            let _ = tx.send(result);
        });
        self.project_transfer_prepare = Some(rx);
        self.project_transfer_source = Some(path);
        self.ui_shell.ui.sync_overlay =
            Some(crate::i18n::t("recording.project_transfer_preparing").into());
        self.ui_shell.ui.sync_progress = 0.0;
    }

    pub fn respond_to_project_transfer(&mut self, response: &str) {
        let Some(request_id) = self
            .project_transfer
            .as_ref()
            .map(|runtime| runtime.metadata.request_id.clone())
        else {
            return;
        };
        self.ui_shell.ui.mark_project_transfer_responded();
        self.update_project_transfer_response(response);
        self.collaboration
            .network
            .respond_project_transfer(&request_id, response);
        if response == "accepted" {
            self.begin_project_transfer_receive(&request_id);
        }
    }

    fn update_project_transfer_response(&mut self, response: &str) {
        let Some(member_id) = self.collaboration.network.member_id.clone() else {
            return;
        };
        let status = self.project_transfer.as_mut().and_then(|runtime| {
            let status = runtime.status.as_mut()?;
            let participant = status
                .participants
                .iter_mut()
                .find(|participant| participant.member_id == member_id)?;
            participant.response = response.to_string();
            participant.deadline = None;
            Some(status.clone())
        });
        if let Some(status) = status {
            self.ui_shell.ui.set_project_transfer_status(status);
        }
    }

    fn begin_project_transfer_receive(&mut self, request_id: &str) {
        let Some(metadata) = self
            .project_transfer
            .as_ref()
            .map(|runtime| runtime.metadata.clone())
        else {
            return;
        };
        let destination = crate::media_binary::user_data_dir().join("transferred_projects");
        let result = self.project_transfer.as_mut().map(|runtime| {
            runtime.receiver.begin(
                crate::file_transfer::FileTransferMetadata {
                    transfer_id: metadata.request_id.clone(),
                    file_name: metadata.file_name.clone(),
                    total_bytes: metadata.total_bytes,
                    total_chunks: metadata.total_chunks,
                    chunk_size: metadata.chunk_size,
                    sha1: metadata.sha1.clone(),
                },
                &destination,
            )
        });
        if let Some(Err(error)) = result {
            self.collaboration
                .network
                .report_project_transfer(request_id, false, Some(&error));
        }
    }

    pub fn accept_project_transfer_after_save(&mut self) {
        self.respond_to_project_transfer("accepted");
    }

    pub fn retry_project_transfer_after_save_failure(&mut self) {
        let Some(request_id) = self
            .project_transfer
            .as_ref()
            .map(|runtime| runtime.metadata.request_id.clone())
        else {
            return;
        };
        self.ui_shell.ui.reset_project_transfer_response();
        self.update_project_transfer_response("saving");
        self.collaboration
            .network
            .respond_project_transfer(&request_id, "saving");
    }

    pub fn open_recording_actor_menu(&mut self) {
        if !self.is_online_recording_actor() {
            return;
        }
        self.ui_shell.ui.open_recording_actor_menu();
        self.announce_open_container(
            crate::i18n::t("recording.actor_menu.title"),
            crate::i18n::t("recording.actor_menu.microphone").to_string(),
        );
    }

    pub fn is_online_recording_actor(&self) -> bool {
        self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Actor
            )
    }

    pub fn toggle_main_window_fullscreen(&self) {
        let window = &self.window_manager.main_window;
        window.set_fullscreen(
            window
                .fullscreen()
                .is_none()
                .then_some(Fullscreen::Borderless(None)),
        );
    }

    pub fn set_recording_input_device(&mut self, device: Option<String>) {
        let label = device
            .clone()
            .unwrap_or_else(|| crate::i18n::t("recording.microphone.default").to_string());
        crate::config::set_recording_input_device(device);
        self.recording_input_preflight = None;
        self.ensure_recording_input_ready();
        self.show_toast(
            crate::i18n::t("recording.microphone.saved").replace("{device}", &label),
            3.0,
        );
    }

    fn recording_username(&self) -> String {
        self.collaboration
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
            .map(|member| member.username.clone())
            .unwrap_or_else(|| crate::config::get().network.username.clone())
    }

    pub fn recording_error(&mut self, message: impl Into<String>) {
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
            .or_else(|| {
                self.ui_shell
                    .ui
                    .modal_host
                    .comic_dubs_settings
                    .as_ref()
                    .map(|modal| modal.keyboard_focus_label())
            })
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

    /// A background task (export, proxy or project import) is running. The
    /// UI stays interactive, but project-level actions that would replace
    /// the document or touch the media read by the worker are refused.
    pub fn background_task_running(&self) -> bool {
        self.ui_shell.ui.has_active_progress() || self.jobs.pending_import_job.is_some()
    }

    pub fn set_export_progress(&mut self, p: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>) {
        let is_none = p.is_none();
        if !is_none {
            self.ui_shell.ui.task_rows.export_expanded = false;
        }
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
        source_video: Option<PathBuf>,
        proxy_video: Option<PathBuf>,
        font_asset: Option<PathBuf>,
        continuation: SaveContinuation,
    ) -> bool {
        if self.jobs.pending_save_job.is_some() {
            self.show_toast(crate::i18n::t("toast.save_already_running"), 4.0);
            return false;
        }

        let project = self.project_session.project.snapshot();
        let saved_revision = project.revision();
        let saved_recording_revision = self.project_session.recording_revision;
        let saved_voicelines_revision = self.voicelines_revision;
        let saved_comic_dubs_revision = self.comic_dubs_revision;
        let transaction_journal = self.project_session.transaction_journal.clone();
        let recording_project = self.project_session.recording_project.clone();
        let save_recording = recording_workspace_has_content(
            &recording_project,
            self.project_session.recording_revision,
        );
        let recording_transactions = self.project_session.recording_transactions.clone();
        let recording_asset_paths = self.project_session.recording_asset_paths.clone();
        let voicelines_project = self.voicelines_project.clone();
        let comic_dubs_project = self.comic_dubs_project.clone();
        let fps = self.fps();
        let default_uses_proxy = source_video.is_some() && self.default_media_uses_proxy();
        let worker_path = path.clone();
        let worker_source = source_video.clone();
        let worker_proxy = proxy_video.clone();
        let worker_font = font_asset.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let recording_assets: Vec<_> = recording_asset_paths
                .iter()
                .filter(|(asset_id, _)| recording_project.asset(**asset_id).is_some())
                .map(
                    |(asset_id, path)| crate::project_archive::RecordingAssetInput {
                        asset_id: *asset_id,
                        path: path.as_path(),
                    },
                )
                .collect();
            let recording =
                save_recording.then_some(crate::project_archive::RecordingBundleInput {
                    project: &recording_project,
                    transaction_log: &recording_transactions,
                    assets: &recording_assets,
                });
            let result =
                crate::project_archive::save_bundle_with_recording_voicelines_and_comic_dubs_data(
                    &project,
                    fps,
                    &worker_path,
                    worker_source.as_deref(),
                    worker_proxy.as_deref(),
                    default_uses_proxy,
                    worker_font.as_deref(),
                    Some(&transaction_journal),
                    recording,
                    Some(&voicelines_project),
                    Some(&comic_dubs_project),
                )
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });

        self.jobs.pending_save_job = Some(PendingSaveJob {
            path,
            saved_revision,
            saved_recording_revision,
            saved_voicelines_revision,
            saved_comic_dubs_revision,
            source_video,
            proxy_video,
            default_uses_proxy,
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

    /// Ordered shortcut contexts for the current UI state (shared with the
    /// bottom-left shortcut panel).
    pub fn shortcut_contexts(&self) -> Vec<crate::input::context::InputContext> {
        self.ui_shell.ui.shortcut_contexts()
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

    pub fn open_connect_modal_with_room(
        &mut self,
        ip: &str,
        port: u16,
        room_code: &str,
        password: &str,
    ) {
        self.ui_shell
            .ui
            .open_connect_modal_with_room(ip, port, room_code, password);
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
        // so the ping must authenticate just like a real connection — otherwise every server
        // with a password shows Offline despite being up.
        let password = crate::config::get().network.password.clone();
        for s in servers {
            let ip = s.ip.clone();
            let port = s.port;
            let ping_results = self.collaboration.ping_results.clone();
            let pw = password.clone();
            std::thread::spawn(move || {
                ping_server_http(&ip, port, pw, ping_results);
            });
        }
    }

    pub fn open_settings_modal(&mut self) {
        self.ui_shell
            .ui
            .open_settings_modal(crate::config::temporary_directory());
        if let Some(first_label) = self.settings_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("settings.title"), first_label);
        }
    }

    pub fn open_project_settings_modal(&mut self) {
        if self.active_workspace() == WorkspaceId::ComicDubs {
            let fonts = self.render.ui_renderer.enumerate_font_families();
            self.ui_shell.ui.open_comic_dubs_settings_modal(
                fonts,
                self.comic_dubs_project.font_family().map(str::to_owned),
                self.comic_dubs_project.bubble_gap_ms(),
                self.comic_dubs_project.page_gap_ms(),
                self.comic_dubs_project.default_font_size(),
            );
            if let Some(first_label) = self.project_settings_modal_focus_label() {
                self.announce_open_container(
                    crate::i18n::t("comic_dubs_settings.title"),
                    first_label,
                );
            }
            return;
        }
        let fonts = self.render.ui_renderer.enumerate_font_families();
        let rythmo_font = crate::config::get().ui.rythmo_font.clone();
        let settings = self.project_session.project.settings();
        self.ui_shell.ui.open_project_settings_modal(
            fonts,
            rythmo_font,
            settings.scroll_speed,
            settings.reading_bar_offset_percent,
            settings.instrumental_audio_path.clone(),
            settings.highlight_read_word,
            settings.scrolling_text_uses_character_color,
            settings.show_text_emotion_lanes,
        );
        if let Some(first_label) = self.project_settings_modal_focus_label() {
            self.announce_open_container(crate::i18n::t("project_settings.title"), first_label);
        }
    }

    pub fn save_comic_dubs_settings(
        &mut self,
        font_family: Option<String>,
        bubble_duration_ms: u64,
        page_duration_ms: u64,
        default_font_size: f32,
    ) {
        let before = self.comic_dubs_project.clone();
        self.comic_dubs_project.set_settings(
            font_family,
            bubble_duration_ms,
            page_duration_ms,
            default_font_size,
        );
        self.comic_dubs_commit(before);
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

    pub fn has_line_context_menu(&self) -> bool {
        self.ui_shell.ui.rythmo_state().context_menu.is_some()
    }

    pub fn save_project_settings(
        &mut self,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
        show_text_emotion_lanes: bool,
    ) {
        let mut settings = self.project_session.project.settings().clone();
        settings.scroll_speed = scroll_speed.clamp(0.25, 4.0);
        settings.reading_bar_offset_percent = reading_bar_offset_percent.clamp(-50.0, 50.0);
        settings.instrumental_audio_path = instrumental_audio_path;
        settings.highlight_read_word = highlight_read_word;
        settings.scrolling_text_uses_character_color = scrolling_text_uses_character_color;
        settings.show_text_emotion_lanes = show_text_emotion_lanes;
        EditExecutor::apply_domain_change(
            &mut self.project_session,
            EditOrigin::Local,
            |project| project.set_settings(settings),
        );
        self.sync_audio_settings_to_player();
    }

    fn apply_project_view_settings(
        &mut self,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        origin: EditOrigin,
    ) {
        let mut settings = self.project_session.project.settings().clone();
        settings.scroll_speed = scroll_speed.clamp(0.25, 4.0);
        settings.reading_bar_offset_percent = reading_bar_offset_percent.clamp(-50.0, 50.0);
        EditExecutor::apply_domain_change(&mut self.project_session, origin, |project| {
            project.set_settings(settings)
        });
    }

    pub fn save_project_view_settings(
        &mut self,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
    ) {
        self.apply_project_view_settings(
            scroll_speed,
            reading_bar_offset_percent,
            EditOrigin::Local,
        );
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
                    let mut commands = Vec::new();
                    if new_karaoke && !line.text_emotions.is_empty() {
                        commands.push(Command::SetTextEmotions {
                            line_id,
                            old_emotions: line.text_emotions.clone(),
                            new_emotions: Vec::new(),
                        });
                    }
                    commands.push(Command::SetLineKaraoke {
                        line_id,
                        old_karaoke,
                        old_ratios,
                        new_karaoke,
                        new_ratios,
                    });
                    commands
                })
            })
            .flatten()
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
        if self.active_workspace() == WorkspaceId::ComicDubs {
            let source_size = self
                .comic_dubs_project
                .pages()
                .first()
                .map(|page| (page.width, page.height))
                .unwrap_or((1920, 1080));
            let configuration = self
                .project_session
                .project
                .settings()
                .export_configuration
                .clone();
            self.ui_shell.ui.open_video_only_export_modal(
                source_size.0,
                source_size.1,
                configuration,
            );
            if let Some(first_label) = self.export_modal_focus_label() {
                self.announce_open_container(crate::i18n::t("export_modal.title"), first_label);
            }
            return;
        }
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

    fn media_explorer_data(&self) -> crate::ui::language_modal::MediaExplorerData {
        let source_path = self.video_path();
        let proxy_path = self.proxy_video_path();
        crate::ui::language_modal::MediaExplorerData {
            source: source_path.as_deref().map(media_video_item),
            proxy: proxy_path.as_deref().map(media_video_item),
            active_proxy: self.playback.proxy_video_path.is_some(),
            default_proxy: self.default_media_uses_proxy(),
            can_persist_default: self.project_session.project_path.is_some(),
        }
    }

    pub fn open_media_explorer(&mut self) {
        let active = self.project_session.project.active_language_id();
        let languages = self.language_modal_items();
        let media = self.media_explorer_data();
        self.ui_shell
            .ui
            .open_media_explorer(languages, active, media);
        self.announce_open_container(
            crate::i18n::t("media_explorer.title"),
            crate::i18n::t("media_explorer.tab.videos").to_string(),
        );
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

    fn refresh_media_explorer(&mut self) {
        let media = self.media_explorer_data();
        self.ui_shell.ui.refresh_media_explorer(media);
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
        let label = self
            .project_session
            .project
            .language(id)
            .map(|language| language.name)
            .unwrap_or_else(|| crate::i18n::t("media_explorer.tab.audios").to_string());
        let value = path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::i18n::t("languages.no_instrumental").to_string());
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
            self.announce_accessibility(AccessibilityEvent::ValueChanged { label, value });
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

    pub fn set_settings_temporary_directory(&mut self, path: PathBuf) {
        self.ui_shell.ui.set_settings_temporary_directory(path);
    }

    pub fn rebuild_topbar_for_network(&mut self) {
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
        self.collaboration.network.room_code = None;
        self.set_network_status("Connexion...");
    }

    pub fn disconnect_network(&mut self) {
        self.collaboration.network.disconnect();
        self.set_network_status("");
        self.rebuild_topbar_for_network();
    }

    pub fn set_network_status(&mut self, status: impl Into<String>) {
        self.ui_shell.ui.network_status = status.into();
        self.update_window_title();
    }

    pub fn update_window_title(&self) {
        let display = if self.ui_shell.ui.network_status.is_empty() {
            "Déconnecté"
        } else {
            &self.ui_shell.ui.network_status
        };
        let code = self
            .collaboration
            .network
            .is_in_room()
            .then(|| self.collaboration.network.room_code.as_deref())
            .flatten()
            .filter(|code| !code.trim().is_empty());
        let title = match code {
            Some(code) => format!(
                "Coquerythmo v{} - {} - {code}",
                crate::update::current_version(),
                display
            ),
            None => format!(
                "Coquerythmo v{} - {}",
                crate::update::current_version(),
                display
            ),
        };
        self.window_manager.main_window.set_title(&title);
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

    pub fn is_secondary_daw(&self) -> bool {
        self.window_manager.secondary_kind == Some(SecondaryWindowKind::Daw)
    }

    pub fn secondary_cursor_position(&self) -> Option<(f32, f32)> {
        self.window_manager
            .secondary_display
            .as_ref()
            .and_then(|display| crate::platform::cursor_position(&display.window))
    }

    pub fn can_open_recording_daw(&self) -> bool {
        !self.collaboration.network.is_in_room()
            || !matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Actor
            )
    }

    pub fn open_secondary_display(&mut self, window: Arc<Window>, kind: SecondaryWindowKind) {
        if kind == SecondaryWindowKind::Daw
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Actor
            )
        {
            self.recording_read_only_error();
            return;
        }
        if self.playback.video_player.is_none() {
            log::warn!("No video loaded — cannot open secondary display");
            return;
        }

        if let Some(display) = &self.window_manager.secondary_display {
            display.window.request_redraw();
            return;
        }

        match self.render.gfx.create_window_surface(window) {
            Ok(display) => {
                self.window_manager.secondary_display = Some(display);
                self.window_manager.secondary_kind = Some(kind);
                self.ui_shell
                    .ui
                    .set_recording_daw_detached(kind == SecondaryWindowKind::Daw);
                self.request_redraw();
                self.request_secondary_redraw();
            }
            Err(e) => log::error!("Failed to open secondary display: {e}"),
        }
    }

    pub fn close_secondary_display(&mut self) {
        self.window_manager.secondary_display = None;
        self.window_manager.secondary_kind = None;
        self.ui_shell.ui.set_recording_daw_detached(false);
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
        self.active_player()
            .as_ref()
            .map_or(0, |p| p.current_frame())
    }

    fn timecode_for_frame(&self, frame: i64) -> String {
        let fps = self.active_fps().max(1.0);
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
        self.active_player()
            .map_or(self.current_frame() as f64, |p| {
                p.current_frame_for_render()
            })
    }

    fn render_frame_at(&self, now: Instant) -> f64 {
        self.active_player()
            .map_or(self.current_frame() as f64, |player| {
                player.current_frame_for_render_at(now)
            })
    }

    pub fn fps(&self) -> f64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(30.0, VideoPlayer::fps)
    }

    fn active_fps(&self) -> f64 {
        self.active_player().map_or(30.0, VideoPlayer::fps)
    }

    pub fn total_frames(&self) -> i64 {
        self.playback
            .video_player
            .as_ref()
            .map_or(0, VideoPlayer::total_frames)
    }

    fn active_player(&self) -> Option<&VideoPlayer> {
        if self.active_workspace() == WorkspaceId::Voicelines {
            self.voicelines_player.as_ref()
        } else if self.active_workspace() == WorkspaceId::ComicDubs {
            self.comic_dubs_player.as_ref()
        } else {
            self.playback.video_player.as_ref()
        }
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

    pub(crate) fn proxy_video_path(&self) -> Option<PathBuf> {
        let source = self.video_path()?;
        self.playback
            .proxy_video_path
            .clone()
            .or_else(|| {
                self.project_session
                    .loaded_project
                    .as_ref()
                    .filter(|loaded| {
                        loaded
                            .source_video_path
                            .as_ref()
                            .is_some_and(|bundled_source| {
                                crate::video_proxy::paths_match(&source, bundled_source)
                            })
                    })
                    .and_then(|loaded| loaded.proxy_video_path.clone())
            })
            .or_else(|| {
                self.project_session
                    .project_path
                    .as_deref()
                    .and_then(crate::video_proxy::proxy_link_for_br)
                    .filter(|link| {
                        crate::video_proxy::paths_match(&source, &link.source_video_path)
                    })
                    .map(|link| link.proxy_video_path)
            })
    }

    fn default_media_uses_proxy(&self) -> bool {
        let fallback = self
            .project_session
            .loaded_project
            .as_ref()
            .map_or(self.playback.proxy_video_path.is_some(), |loaded| {
                loaded.default_uses_proxy
            });
        self.project_session
            .project_path
            .as_deref()
            .map_or(fallback, |path| {
                crate::video_proxy::default_uses_proxy_or(path, fallback)
            })
            && self.proxy_video_path().is_some()
    }

    pub fn load_video(&mut self, path: &Path) -> bool {
        let proxy_path = self
            .project_session
            .project_path
            .as_ref()
            .and_then(|br_path| crate::video_proxy::linked_proxy_path(br_path, path));
        let loaded = self.load_video_for_playback(path, proxy_path.as_deref(), None, false);
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
        self.playback.recording_audio_cache.clear();
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
                let desired_proxy = crate::video_proxy::default_uses_proxy(br_path)
                    .then_some(link.proxy_video_path.as_path());
                let source_matches = self.video_path().as_ref().is_some_and(|path| {
                    crate::video_proxy::paths_match(path, &link.source_video_path)
                });
                let proxy_matches = match (self.playback.proxy_video_path.as_deref(), desired_proxy)
                {
                    (None, None) => true,
                    (Some(current), Some(desired)) => {
                        crate::video_proxy::paths_match(current, desired)
                    }
                    _ => false,
                };

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
                    desired_proxy,
                    Some(frame),
                    false,
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
        self.load_video_for_playback(&source_path, proxy_path.as_deref(), Some(frame), false);
    }

    pub fn switch_media_video(&mut self, use_proxy: bool) {
        let Some(source) = self.video_path() else {
            return;
        };
        let proxy = self.proxy_video_path();
        if use_proxy && proxy.is_none() {
            self.show_toast(crate::i18n::t("toast.media_proxy_missing"), 4.0);
            return;
        }
        let frame = self.current_frame();
        if self.load_video_for_playback(
            &source,
            use_proxy.then_some(proxy.as_deref()).flatten(),
            Some(frame),
            false,
        ) {
            self.refresh_media_explorer();
            self.announce_accessibility(AccessibilityEvent::ValueChanged {
                label: crate::i18n::t("media_explorer.active").to_string(),
                value: crate::i18n::t(if use_proxy {
                    "media_explorer.video.proxy"
                } else {
                    "media_explorer.video.original"
                })
                .to_string(),
            });
        }
    }

    pub fn set_default_media_video(&mut self, use_proxy: bool) {
        let Some(project_path) = self.project_session.project_path.as_deref() else {
            self.show_toast(crate::i18n::t("toast.media_save_project_first"), 5.0);
            return;
        };
        if use_proxy && self.media_explorer_data().proxy.is_none() {
            self.show_toast(crate::i18n::t("toast.media_proxy_missing"), 4.0);
            return;
        }
        match crate::video_proxy::set_default_uses_proxy(project_path, use_proxy) {
            Ok(()) => {
                if let Some(loaded) = self.project_session.loaded_project.as_mut() {
                    loaded.default_uses_proxy = use_proxy;
                }
                self.project_session.dirty = true;
                self.show_toast(crate::i18n::t("toast.media_default_saved"), 4.0);
                self.refresh_media_explorer();
            }
            Err(error) => {
                log::error!("Failed to save default media: {error}");
                self.show_toast(crate::i18n::t("toast.media_default_failed"), 5.0);
            }
        }
    }

    pub fn delete_media_video(&mut self, use_proxy: bool) {
        if use_proxy {
            let Some(project_path) = self.project_session.project_path.clone() else {
                self.show_toast(crate::i18n::t("toast.media_save_project_first"), 5.0);
                return;
            };
            if self.playback.proxy_video_path.is_some() {
                let Some(source) = self.video_path() else {
                    return;
                };
                let frame = self.current_frame();
                if !self.load_video_for_playback(&source, None, Some(frame), false) {
                    return;
                }
            }
            match crate::video_proxy::delete_proxy(&project_path) {
                Ok(()) => {
                    if let Some(loaded) = self.project_session.loaded_project.as_mut() {
                        loaded.proxy_video_path = None;
                        loaded.default_uses_proxy = false;
                    }
                    self.project_session.dirty = true;
                    self.show_toast(crate::i18n::t("toast.media_proxy_deleted"), 4.0);
                }
                Err(error) => {
                    log::error!("Failed to delete proxy: {error}");
                    self.show_toast(crate::i18n::t("toast.media_delete_failed"), 5.0);
                }
            }
        } else {
            if let Some(project_path) = self.project_session.project_path.clone() {
                if let Err(error) = crate::video_proxy::delete_proxy(&project_path) {
                    log::warn!("Failed to remove linked proxy while unlinking video: {error}");
                }
                if let Err(error) = crate::video_proxy::set_source_removed(&project_path, true) {
                    log::warn!("Failed to persist removed source video: {error}");
                }
            }
            self.clear_video_for_new_project();
            self.project_session.dirty = true;
            self.show_toast(crate::i18n::t("toast.media_video_unlinked"), 4.0);
        }
        self.refresh_media_explorer();
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

    pub fn start_comic_dubs_export(
        &mut self,
        mut output: PathBuf,
        configuration: crate::project::ExportConfiguration,
    ) {
        if self.background_task_running() {
            self.show_toast(crate::i18n::t("toast.action_blocked_task"), 5.0);
            return;
        }
        let pages = self.comic_dubs_project.pages();
        if pages.is_empty()
            || (!configuration.comic_dubs_pages_zip
                && !pages.iter().any(|page| !page.bubbles.is_empty()))
        {
            self.show_toast("Aucune page Comic Dubs à exporter", 4.0);
            return;
        }
        let extension = if configuration.comic_dubs_pages_zip {
            "zip"
        } else if configuration.comic_dubs_alpha {
            "mov"
        } else {
            "mp4"
        };
        if !output
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            output.set_extension(extension);
        }
        let project = self.comic_dubs_project.clone();
        let progress = Arc::new(std::sync::atomic::AtomicU32::new(0.001_f32.to_bits()));
        let progress_for_ui = progress.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_job = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::comic_dubs_export::export(
                &project,
                &output,
                &configuration,
                progress.clone(),
                cancel_for_job,
            );
            let _ = sender.send(result);
            progress.store(2.0_f32.to_bits(), Ordering::Relaxed);
        });

        self.set_progress_label(crate::i18n::t("progress.exporting"));
        self.set_export_render_backend(None);
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

    pub fn start_configured_export(
        &mut self,
        output_base: PathBuf,
        configuration: crate::project::ExportConfiguration,
    ) {
        if self.background_task_running() {
            self.show_toast(crate::i18n::t("toast.action_blocked_task"), 5.0);
            return;
        }
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
        if self.background_task_running() {
            self.show_toast(crate::i18n::t("toast.action_blocked_task"), 5.0);
            return;
        }

        let label = br_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let thread_path = br_path.clone();
        let progress = Arc::new(std::sync::Mutex::new(ProjectLoadProgress {
            stage: ProjectLoadStage::ReadingManifest,
            fraction: 0.0,
        }));
        let progress_for_job = progress.clone();
        let transfer_request_id = self.project_transfer_loading_request.clone();
        std::thread::spawn(move || {
            let result =
                crate::project_archive::load_project_file_with_progress(&thread_path, |update| {
                    if let Ok(mut progress) = progress_for_job.lock() {
                        *progress = update;
                    }
                })
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        self.jobs.pending_import_job = Some(PendingImportJob {
            br_path,
            receiver: rx,
            progress,
            transfer_request_id,
        });
        self.ui_shell.ui.start_project_load(label);
        self.narration.announce_event(AccessibilityEvent::Opened {
            label: format!(
                "{} {}",
                crate::i18n::t("loading_project.title"),
                self.ui_shell
                    .ui
                    .loading_project
                    .as_ref()
                    .map(|load| load.label.as_str())
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
        resolve_linked_proxy: bool,
    ) -> bool {
        self.clear_recording_mix_preview();
        let (bgl, sampler) = self.renderer_refs();
        let mut player = VideoPlayer::new();

        // Every decoder load resolves the proxy again. This covers project loads,
        // recent projects and explicit reloads without requiring each caller to
        // remember the proxy policy.
        let linked_proxy = proxy_path.map(Path::to_path_buf).or_else(|| {
            resolve_linked_proxy
                .then(|| {
                    self.project_session
                        .project_path
                        .as_deref()
                        .and_then(|br_path| {
                            crate::video_proxy::linked_proxy_path(br_path, source_path)
                        })
                })
                .flatten()
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
            player.seek_to_frame_instant(target);
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
        self.schedule_recording_mix();
        true
    }

    pub fn toggle_play_pause(&mut self) {
        if self.recording_playback_is_read_only() {
            return;
        }
        self.toggle_play_pause_internal(true);
    }

    fn recording_playback_is_read_only(&self) -> bool {
        self.active_workspace() == WorkspaceId::Recording
            && self.collaboration.network.is_in_room()
            && !self.ui_shell.ui.recording_role().can_control_playback()
    }

    fn should_broadcast_recording_playback(&self) -> bool {
        self.collaboration.network.is_in_room()
            && self.ui_shell.ui.recording_role().can_edit_timeline()
    }

    fn broadcast_recording_playback(&self, frame: i64, playing: bool) {
        if self.should_broadcast_recording_playback() {
            self.collaboration
                .network
                .send_recording_playback(frame.max(0), playing);
        }
    }

    fn toggle_play_pause_internal(&mut self, broadcast: bool) {
        if self.active_workspace() == WorkspaceId::ComicDubs {
            if self.toggle_comic_dubs_vertex_editor_preview() {
                return;
            }
            self.toggle_comic_dubs_playback();
            return;
        }
        let capture_started = matches!(
            self.recording_runtime.capture_state(),
            Some(crate::recording::CaptureState::Capturing { .. })
        );
        let recording_mix_pending = recording_playback_waits_for_mix(
            self.active_workspace(),
            self.jobs.pending_recording_mix_job.is_some(),
            self.playback
                .video_player
                .as_ref()
                .is_some_and(|player| player.is_playing()),
            capture_started,
        );
        if recording_mix_pending {
            self.jobs.play_recording_mix_when_ready = true;
            return;
        }
        self.jobs.play_recording_mix_when_ready = false;
        let playing = {
            let player = if self.active_workspace() == WorkspaceId::Voicelines {
                &mut self.voicelines_player
            } else {
                &mut self.playback.video_player
            };
            let Some(player) = player else {
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
        if broadcast && self.active_workspace() == WorkspaceId::Recording {
            let frame = self.current_frame();
            self.broadcast_recording_playback(frame, playing);
        }
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

    fn recording_view_payload(&self) -> crate::network::RecordingViewPayload {
        crate::network::RecordingViewPayload {
            language_id: self.project_session.project.active_language_id(),
            instrumental: self.active_audio_is_instrumental(),
        }
    }

    fn send_recording_view(&self, target: Option<&str>) {
        if self.collaboration.network.is_in_room()
            && matches!(
                self.ui_shell.ui.recording_role(),
                crate::ui::recording_workspace::RecordingRole::Director
            )
        {
            self.collaboration
                .network
                .send_recording_view(self.recording_view_payload(), target);
        }
    }

    fn recording_shared_view_is_allowed(&mut self) -> bool {
        if self.ui_shell.ui.recording_role().can_change_shared_view() {
            true
        } else {
            let message = crate::i18n::t("recording.director_only");
            self.show_toast(message, 3.0);
            self.announce_accessibility(AccessibilityEvent::Error {
                message: message.to_string(),
            });
            false
        }
    }

    pub fn recording_toggle_shared_audio(&mut self) {
        if !self.recording_shared_view_is_allowed() {
            return;
        }
        self.toggle_active_audio();
        self.send_recording_view(None);
    }

    pub fn recording_cycle_language(&mut self) {
        if !self.recording_shared_view_is_allowed() {
            return;
        }
        let languages = self.project_session.project.languages();
        let Some(index) = languages
            .iter()
            .position(|language| language.id == self.project_session.project.active_language_id())
        else {
            return;
        };
        if let Some(next) = languages.get((index + 1) % languages.len()) {
            self.select_language(next.id);
            self.send_recording_view(None);
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
        if let Some(player) = &mut self.voicelines_player {
            player.set_volume(vol);
        }
        if let Some(player) = &mut self.comic_dubs_player {
            player.set_volume(vol);
        }
        if self.active_workspace() == WorkspaceId::Recording {
            self.schedule_recording_mix();
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
        if self.active_workspace() == WorkspaceId::ComicDubs
            && self
                .ui_shell
                .ui
                .nudge_comic_dubs_vertex_editor(-50, &self.comic_dubs_project)
        {
            return;
        }
        if self.recording_playback_is_read_only() {
            return;
        }
        self.prev_frame_internal(true);
    }

    fn prev_frame_internal(&mut self, broadcast: bool) {
        if matches!(
            self.active_workspace(),
            WorkspaceId::Voicelines | WorkspaceId::ComicDubs
        ) {
            if self.active_workspace() == WorkspaceId::ComicDubs {
                self.comic_dubs_playback = None;
                self.ui_shell.ui.set_comic_dubs_playback(None, 0, 0);
            }
            self.seek_relative_internal(-1, false);
            return;
        }
        let mut playback = None;
        if let Some(player) = &mut self.playback.video_player {
            // Step through the debounced async seek path: the previous
            // synchronous per-frame ffmpeg decode froze the UI as soon as the
            // key was held down.
            player.pause_for_seek();
            player.seek_frame_instant(-1);
            if self.ui_shell.ui.is_playing() {
                self.ui_shell.ui.toggle_play_pause();
            }
            playback = Some((player.current_frame(), self.ui_shell.ui.is_playing()));
        }
        if broadcast {
            if let Some((frame, playing)) = playback {
                self.broadcast_recording_playback(frame, playing);
            }
        }
        self.playback.last_scroll_time = Some(Instant::now());
        self.playback.scroll_needs_decode = true;
    }

    pub fn next_frame(&mut self) {
        if self.active_workspace() == WorkspaceId::ComicDubs
            && self
                .ui_shell
                .ui
                .nudge_comic_dubs_vertex_editor(50, &self.comic_dubs_project)
        {
            return;
        }
        if self.recording_playback_is_read_only() {
            return;
        }
        self.next_frame_internal(true);
    }

    fn next_frame_internal(&mut self, broadcast: bool) {
        if matches!(
            self.active_workspace(),
            WorkspaceId::Voicelines | WorkspaceId::ComicDubs
        ) {
            if self.active_workspace() == WorkspaceId::ComicDubs {
                self.comic_dubs_playback = None;
                self.ui_shell.ui.set_comic_dubs_playback(None, 0, 0);
            }
            self.seek_relative_internal(1, false);
            return;
        }
        let mut playback = None;
        if let Some(player) = &mut self.playback.video_player {
            // Step through the debounced async seek path: the previous
            // synchronous per-frame ffmpeg decode froze the UI as soon as the
            // key was held down.
            player.pause_for_seek();
            player.seek_frame_instant(1);
            if self.ui_shell.ui.is_playing() {
                self.ui_shell.ui.toggle_play_pause();
            }
            playback = Some((player.current_frame(), self.ui_shell.ui.is_playing()));
        }
        if broadcast {
            if let Some((frame, playing)) = playback {
                self.broadcast_recording_playback(frame, playing);
            }
        }
        self.playback.last_scroll_time = Some(Instant::now());
        self.playback.scroll_needs_decode = true;
    }

    pub fn seek_absolute(&mut self, frame: i64) {
        if self.recording_playback_is_read_only() {
            return;
        }
        self.seek_absolute_internal(frame, true);
    }

    fn seek_absolute_internal(&mut self, frame: i64, broadcast: bool) {
        let mut playback = None;
        if self.active_workspace() == WorkspaceId::ComicDubs {
            self.comic_dubs_playback = None;
            self.ui_shell.ui.set_comic_dubs_playback(None, 0, 0);
        }
        let player = match self.active_workspace() {
            WorkspaceId::Voicelines => &mut self.voicelines_player,
            WorkspaceId::ComicDubs => &mut self.comic_dubs_player,
            _ => &mut self.playback.video_player,
        };
        if let Some(player) = player {
            if player.pause_for_seek() {
                if self.ui_shell.ui.is_playing() {
                    self.ui_shell.ui.toggle_play_pause();
                }
                self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
            }
            player.seek_to_frame_instant(frame);
            self.playback.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
            playback = Some((player.current_frame(), self.ui_shell.ui.is_playing()));
        }
        if broadcast && self.active_workspace() == WorkspaceId::Recording {
            if let Some((frame, playing)) = playback {
                self.broadcast_recording_playback(frame, playing);
            }
        }
        self.playback.last_scroll_time = Some(Instant::now());
        self.playback.scroll_needs_decode = true;
    }

    pub fn finish_seek(&mut self) {
        self.playback.scroll_needs_decode = false;
        self.playback.last_scroll_time = None;

        let player = match self.active_workspace() {
            WorkspaceId::Voicelines => &mut self.voicelines_player,
            WorkspaceId::ComicDubs => &mut self.comic_dubs_player,
            _ => &mut self.playback.video_player,
        };
        if let Some(player) = player {
            player.prepare_current_frame();
        }
    }

    pub fn seek_relative(&mut self, delta: i32) {
        if self.recording_playback_is_read_only() {
            return;
        }
        self.seek_relative_internal(delta, true);
    }

    fn seek_relative_internal(&mut self, delta: i32, broadcast: bool) {
        let mut playback = None;
        let player = match self.active_workspace() {
            WorkspaceId::Voicelines => &mut self.voicelines_player,
            WorkspaceId::ComicDubs => &mut self.comic_dubs_player,
            _ => &mut self.playback.video_player,
        };
        if let Some(player) = player {
            player.seek_frame_instant(delta);
            self.playback.timeline.emit(TimelineEvent::FrameChanged {
                frame: player.current_frame(),
            });
            playback = Some((player.current_frame(), self.ui_shell.ui.is_playing()));
        }
        if broadcast && self.active_workspace() == WorkspaceId::Recording {
            if let Some((frame, playing)) = playback {
                self.broadcast_recording_playback(frame, playing);
            }
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

        let operation = transaction.operation.clone();
        let result = self
            .project_session
            .recording_transactions
            .append_received_and_apply(&mut self.project_session.recording_project, transaction);
        match result {
            Ok(_) => {
                self.bind_recording_audio_paths(&operation);
                // A remote RemoveAsset can invalidate a path retained from a
                // previous audio transfer. Never persist an orphaned FLAC
                // input when the project is saved for replacement.
                self.project_session
                    .recording_asset_paths
                    .retain(|asset_id, _| {
                        self.project_session
                            .recording_project
                            .asset(*asset_id)
                            .is_some()
                    });
                self.project_session.mark_recording_changed();
                self.sync_recording_workspace_ui();
                self.schedule_recording_mix();
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

        self.receive_recording_capture(current_frame, capture_target);
        self.sync_recording_workspace_ui();
        self.schedule_recording_mix();
    }

    fn receive_recording_capture(
        &mut self,
        current_frame: i64,
        capture_target: Option<crate::recording::CaptureTarget>,
    ) {
        if capture_target.is_some() {
            self.jobs.play_recording_mix_when_ready = false;
        }
        self.seek_absolute_internal(current_frame, false);
        if capture_target.is_some() {
            self.finish_seek();
        }
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
        self.enter_online_recording_view();
        match capture_target {
            Some(target) if !self.recording_runtime.is_active() => {
                let result = if matches!(
                    self.recording_network_role(),
                    crate::ui::recording_workspace::RecordingRole::Actor
                ) && !local_member_is_muted
                {
                    self.recording_runtime.begin_capture_target(
                        target,
                        &self.recording_username(),
                        crate::config::recording_input_device().as_deref(),
                    )
                } else {
                    self.recording_runtime.begin_observed_capture(target)
                };
                if let Err(error) = result {
                    self.recording_error(error.to_string());
                }
            }
            None if self.recording_runtime.is_active() => {
                if let Err(error) = self.recording_runtime.cancel_or_stop() {
                    self.recording_error(error.to_string());
                }
            }
            _ => {}
        }
        self.sync_recording_workspace_ui();
    }

    fn receive_recording_playback(&mut self, playback: crate::network::RecordingPlaybackPayload) {
        if recording_playback_is_blocked_during_countdown(self.recording_runtime.capture_state()) {
            return;
        }
        self.seek_absolute_internal(playback.frame, false);
        let is_playing = self
            .playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.is_playing());
        if playback.playing != is_playing {
            self.toggle_play_pause_internal(false);
        }
    }

    fn receive_recording_view(&mut self, view: crate::network::RecordingViewPayload) {
        if self
            .project_session
            .project
            .language(view.language_id)
            .is_none()
        {
            return;
        }
        self.select_language(view.language_id);
        if self.active_audio_is_instrumental() != view.instrumental {
            self.toggle_active_audio();
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
        self.recording_runtime
            .remember_audio_path(&metadata.audio.checksum, &path);

        let matching_asset_id = self
            .project_session
            .recording_project
            .assets()
            .find(|asset| asset.checksum == metadata.audio.checksum)
            .map(|asset| asset.id);
        if let Some(asset_id) = matching_asset_id {
            self.project_session
                .recording_asset_paths
                .insert(asset_id, path);
            self.project_session.mark_recording_changed();
            self.schedule_recording_mix();
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
            let operation = crate::recording::CompletedCapture {
                target,
                audio: metadata.audio,
            }
            .into_project_operation(self.project_session.recording_project.timeline_fps());
            if let Err(error) = self.apply_recording_operation(operation) {
                self.recording_error(error.to_string());
                return;
            }
        } else {
            // Non-authoritative peers receive the matching transaction from
            // the DA. The verified path is already retained by checksum and
            // will be bound to the asset ID carried by that transaction.
            self.project_session.mark_recording_changed();
            self.schedule_recording_mix();
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
                    self.set_network_status("Connecté au serveur");
                    log::info!("Connected and authenticated");
                }
                IncomingMessage::Packet(packet) => self.handle_network_packet(packet),
                IncomingMessage::Disconnected(reason) => {
                    log::info!("Disconnected: {reason}");
                    if let Some(runtime) = self.project_transfer.as_mut() {
                        runtime.receiver.cancel();
                    }
                    self.project_transfer = None;
                    self.project_transfer_prepare = None;
                    self.project_transfer_source = None;
                    self.project_transfer_send = None;
                    self.project_transfer_loading_request = None;
                    self.project_transfer_waiting_dismissed = None;
                    self.recording_input_preflight = None;
                    self.recording_uploads.clear();
                    self.recording_upload_acks.clear();
                    self.big_receives.clear();
                    self.ui_shell.ui.close_project_transfer_modal();
                    self.ui_shell.ui.sync_overlay = None;
                    self.collaboration.network.state = ConnectionState::Disconnected;
                    self.collaboration.network.room_code = None;
                    self.collaboration.network.role = None;
                    self.collaboration.network.members.clear();
                    self.collaboration.network.member_id = None;
                    self.collaboration.network.project_huuid = None;
                    self.collaboration.network.project_matches = false;
                    self.collaboration.network.sync_requested_this_session = false;
                    self.collaboration.network.member_details.clear();
                    self.collaboration.network.control_owner_id = None;
                    self.set_network_status("");
                }
                IncomingMessage::Error(err) => {
                    log::error!("Network error: {err}");
                    self.set_network_status(format!("Erreur: {err}"));
                }
                IncomingMessage::RoomMetadata {
                    member_id,
                    project_huuid,
                    project_matches,
                } => {
                    self.collaboration.network.member_id = Some(member_id);
                    self.collaboration.network.project_huuid = Some(project_huuid);
                    self.collaboration.network.project_matches = project_matches;
                    if project_matches && !self.collaboration.network.sync_requested_this_session {
                        self.collaboration.network.sync_requested_this_session = true;
                        self.collaboration.network.send_raw("request_sync", serde_json::json!({}));
                    }
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
                    self.enter_online_recording_view();
                }
                IncomingMessage::Delta(data) => self.apply_delta(data),
                IncomingMessage::BigBegin(begin) => {
                    if let Err(error) = self.big_receives.begin(begin) {
                        log::warn!("Rejected big transfer: {error}");
                    }
                }
                IncomingMessage::BigChunk {
                    transfer_id,
                    index,
                    data_base64,
                } => {
                    if let Err(error) =
                        self.big_receives.push_base64(&transfer_id, index, &data_base64)
                    {
                        log::warn!("Rejected big chunk: {error}");
                    }
                }
                IncomingMessage::BigEnd { transfer_id } => {
                    match self.big_receives.finish(&transfer_id) {
                        Ok((event, payload)) => self.dispatch_big_event(&event, &payload),
                        Err(error) => log::warn!("Rejected big transfer {transfer_id}: {error}"),
                    }
                }
                IncomingMessage::RecordingTransaction(transaction) => {
                    self.receive_recording_transaction(transaction)
                }
                IncomingMessage::RecordingPrepare(prepare) => {
                    self.receive_recording_prepare(prepare)
                }
                IncomingMessage::RecordingCapture(capture) => {
                    self.receive_recording_capture(capture.current_frame, capture.capture_target)
                }
                IncomingMessage::RecordingPlayback(playback) => {
                    self.receive_recording_playback(playback)
                }
                IncomingMessage::RecordingView(view) => self.receive_recording_view(view),
                IncomingMessage::ActorRequestOpenMicrophone => {
                    self.open_recording_input_device_modal()
                }
                IncomingMessage::ActorRequestApplyDisplaySettings {
                    scroll_speed,
                    reading_bar_offset_percent,
                } => self.apply_project_view_settings(
                    scroll_speed,
                    reading_bar_offset_percent,
                    EditOrigin::Remote,
                ),
                IncomingMessage::ActorRequestCloseProjectTransferWaiting => {
                    self.close_project_transfer_waiting()
                }
                IncomingMessage::ProjectTransferRequest(metadata) => {
                    // A fresh in-memory document is not a local project to protect:
                    // only offer the save-and-replace path when a saved project exists.
                    let dirty =
                        self.project_session.project_path.is_some() && self.project_session.dirty;
                    self.ui_shell
                        .ui
                        .open_project_transfer_modal(metadata.clone(), false, dirty);
                    self.project_transfer = Some(ProjectTransferRuntime {
                        metadata,
                        status: None,
                        receiver: crate::file_transfer::FileTransferReceiver::default(),
                    });
                    self.project_transfer_waiting_dismissed = None;
                    self.announce_open_container(
                        crate::i18n::t("recording.project_transfer.title"),
                        crate::i18n::t("recording.project_transfer_request_received").to_string(),
                    );
                    self.ui_shell.ui.sync_overlay = None;
                    self.ui_shell.ui.sync_progress = 0.0;
                }
                IncomingMessage::ProjectTransferReady(metadata) => {
                    if let Some(path) = self.project_transfer_source.clone() {
                        let request_id = metadata.request_id.clone();
                        let receiver = self.collaboration.network.send_project_file(path, metadata);
                        self.project_transfer_send = Some((request_id, receiver));
                    }
                }
                IncomingMessage::ProjectTransferStatus(mut status) => {
                    let dismissed = self.project_transfer_waiting_dismissed.as_deref()
                        == Some(status.request_id.as_str());
                    if self.project_transfer_loading_request.as_deref()
                        == Some(status.request_id.as_str())
                    {
                        if let Some(member_id) = self.collaboration.network.member_id.as_deref() {
                            if let Some(participant) = status
                                .participants
                                .iter_mut()
                                .find(|participant| participant.member_id == member_id)
                            {
                                participant.response = "loading".into();
                            }
                        }
                    }
                    let progress = if status.total_bytes == 0 {
                        0.0
                    } else {
                        status.transferred_bytes as f32 / status.total_bytes as f32
                    };
                    self.ui_shell.ui.sync_progress = progress;
                    self.narration.publish_progress(
                        crate::i18n::t("recording.project_transfer.title").to_string(),
                        Some((progress * 100.0).round() as u32),
                    );
                    if let Some(runtime) = self.project_transfer.as_mut() {
                        runtime.status = Some(status.clone());
                    }
                    self.ui_shell.ui.set_project_transfer_status(status.clone());
                    if matches!(status.phase.as_str(), "completed" | "cancelled") {
                        if dismissed {
                            self.ui_shell.ui.sync_overlay = None;
                            self.ui_shell.ui.sync_progress = 0.0;
                            self.narration.publish_progress(String::new(), None);
                            self.project_transfer_waiting_dismissed = None;
                        } else {
                            // Only touch the overlay when this client takes
                            // part in the transfer. Bystanders waiting on a
                            // project sync must keep their "Synchronisation en
                            // cours..." overlay until the sync truly arrives.
                            if self.project_transfer.is_some() {
                                if let Some(runtime) = self.project_transfer.as_mut() {
                                    runtime.receiver.cancel();
                                }
                                if self.collaboration.network.project_matches {
                                    self.ui_shell.ui.sync_overlay = None;
                                } else {
                                    self.ui_shell.ui.sync_progress = 0.0;
                                    self.ui_shell.ui.sync_overlay = Some(
                                        crate::i18n::t("recording.project_transfer.no_project")
                                            .into(),
                                    );
                                }
                            }
                            self.project_transfer_source = None;
                            self.project_transfer_send = None;
                            self.narration.publish_progress(String::new(), None);
                            self.ui_shell.ui.close_project_transfer_modal();
                        }
                    }
                }
                IncomingMessage::ProjectTransferChunk {
                    request_id,
                    index,
                    data_base64,
                } => {
                    if let Some(runtime) = self.project_transfer.as_mut() {
                        if runtime.metadata.request_id == request_id {
                            if let Err(error) = runtime.receiver.push_base64(index, &data_base64) {
                                runtime.receiver.cancel();
                                self.collaboration.network.report_project_transfer(
                                    &request_id,
                                    false,
                                    Some(&error),
                                );
                            }
                        }
                    }
                }
                IncomingMessage::ProjectTransferEnd { request_id } => {
                    let is_current = self
                        .project_transfer
                        .as_ref()
                        .is_some_and(|runtime| runtime.metadata.request_id == request_id);
                    if is_current {
                        self.project_transfer_loading_request = Some(request_id.clone());
                        self.update_project_transfer_response("loading");
                        self.collaboration
                            .network
                            .report_project_transfer_loading(&request_id);
                        let result = self
                            .project_transfer
                            .as_mut()
                            .expect("the current transfer exists")
                            .receiver
                            .finish(&request_id);
                        match result {
                            Ok(received) => {
                                if self.is_project_save_in_progress() {
                                    if let Some(request_id) =
                                        self.project_transfer_loading_request.take()
                                    {
                                        self.collaboration.network.report_project_transfer(
                                            &request_id,
                                            false,
                                            Some("project save is still in progress"),
                                        );
                                    }
                                } else {
                                    self.start_br_import(received.path);
                                }
                            }
                            Err(error) => {
                                self.project_transfer_loading_request = None;
                                self.collaboration.network.report_project_transfer(
                                    &request_id,
                                    false,
                                    Some(&error),
                                );
                            }
                        }
                    }
                }
                IncomingMessage::SyncRequested { requester } => {
                    log::info!("Sync requested by {requester}");
                    let data = ProjectData::from_project(&self.project_session.project);
                    let json = serde_json::json!({ "project": data });
                    self.collaboration
                        .network
                        .send_sync(json, (!requester.is_empty()).then_some(requester.as_str()));
                    let prepare = crate::network::RecordingPreparePayload {
                        project: self.project_session.recording_project.clone(),
                        transactions: self.project_session.recording_transactions.clone(),
                        current_frame: self.current_frame(),
                        capture_target: self.recording_runtime.capture_state().and_then(|state| {
                            match state {
                                crate::recording::CaptureState::Countdown { target, .. }
                                | crate::recording::CaptureState::Capturing { target, .. }
                                | crate::recording::CaptureState::Finalizing { target } => {
                                    Some(*target)
                                }
                                _ => None,
                            }
                        }),
                    };
                    if requester.is_empty() {
                        self.collaboration.network.send_recording_prepare(&prepare);
                    } else {
                        self.collaboration
                            .network
                            .send_recording_prepare_to(&prepare, &requester);
                    }
                    self.send_recording_view((!requester.is_empty()).then_some(requester.as_str()));
                }
                IncomingMessage::AudioStart { metadata } => {
                    match serde_json::from_value::<crate::audio_transfer::AudioTransferMetadata>(
                        metadata,
                    ) {
                        Ok(mut metadata) => {
                            let sender_username =
                                metadata.from_member_id.as_deref().and_then(|member_id| {
                                    self.collaboration
                                        .network
                                        .member_details
                                        .iter()
                                        .find(|member| member.id == member_id)
                                        .map(|member| member.username.clone())
                                });
                            if let Some(username) = sender_username {
                                if let Err(error) = metadata.prefix_file_name_with_user(&username) {
                                    self.recording_error(error);
                                    continue;
                                }
                            }
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
                IncomingMessage::AudioUploaded { transfer_id } => {
                    let was_pending = self.recording_upload_acks.len();
                    self.recording_upload_acks
                        .retain(|pending| pending != &transfer_id);
                    if self.recording_upload_acks.len() != was_pending {
                        self.show_toast(crate::i18n::t("recording.capture.uploaded"), 4.0);
                    }
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
                self.collaboration.network.set_rejoin_code(&code);
                self.set_network_status("Salon créé");
                self.show_toast(
                    format!("{}{code}", crate::i18n::t("toast.room_created")),
                    5.0,
                );
                log::info!("Room created: {code}");
                self.enter_online_recording_view();
                // No recording_prepare broadcast here: a freshly created room
                // is empty so it is a no-op, while on a promotion (director
                // lost, oldest member promoted) it would clobber every peer
                // with this member's possibly stale recording state. Late
                // joiners are prepared through their sync request instead.
            }
            Packet::RoomJoined {
                code,
                role,
                members,
            } => {
                self.collaboration.network.state = ConnectionState::InRoom;
                self.collaboration.network.room_code = Some(code.clone());
                self.collaboration.network.set_rejoin_code(&code);
                self.collaboration.network.role = Some(role);
                self.collaboration.network.members = members;
                self.set_network_status("Connecté au salon");
                self.show_toast(
                    format!("{}{code}", crate::i18n::t("toast.room_joined")),
                    5.0,
                );
                self.ui_shell.ui.sync_overlay = if self.collaboration.network.project_matches {
                    Some("Synchronisation en cours...".into())
                } else {
                    Some(crate::i18n::t("recording.project_transfer.no_project").into())
                };
                self.ui_shell.ui.sync_progress = 0.0;
                self.enter_online_recording_view();
                // request_sync is sent directly from the room_joined callback
            }
            Packet::JoinError { reason } => {
                log::error!("Join failed: {reason}");
                self.set_network_status(format!("Échec: {reason}"));
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
        EditExecutor::apply_remote_payload(&mut self.project_session, payload, EditOrigin::Remote);
    }
    fn apply_project_sync(&mut self, data: ProjectData) {
        EditExecutor::apply_sync(&mut self.project_session, data);
        log::info!("Project synced (merged)");
    }

    /// Dispatch a reassembled chunked event along the same path as its legacy
    /// single-frame equivalent.
    fn dispatch_big_event(&mut self, event: &str, payload: &[u8]) {
        let value: serde_json::Value = match serde_json::from_slice(payload) {
            Ok(value) => value,
            Err(error) => {
                log::warn!("Big event {event} is not valid JSON: {error}");
                return;
            }
        };
        match event {
            "sync" => match serde_json::from_value::<ProjectData>(value["project"].clone()) {
                Ok(project) => {
                    self.apply_project_sync(project);
                    self.ui_shell.ui.sync_overlay = None;
                    if self.collaboration.network.room_code.is_some() {
                        self.set_network_status("Salon synchronisé");
                    }
                }
                Err(error) => log::warn!("Big sync payload is invalid: {error}"),
            },
            "recording_prepare" => {
                match serde_json::from_value::<crate::network::RecordingPreparePayload>(value) {
                    Ok(prepare) => self.receive_recording_prepare(prepare),
                    Err(error) => log::warn!("Big recording_prepare payload is invalid: {error}"),
                }
            }
            other => log::warn!("Unknown big event: {other}"),
        }
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
            .send_sync(serde_json::json!({ "project": data }), None);
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
        let target_anchor_frame = self
            .ui_shell
            .ui
            .rythmo_state
            .hovered_frame
            .unwrap_or_else(|| self.current_frame());
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
            self.show_toast("Sélectionne un dialogue et place le curseur dedans.", 3.0);
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
            self.show_toast("Dialogue trop court pour être coupé.", 3.0);
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
        if self.active_workspace() == WorkspaceId::ComicDubs {
            return self.ui_shell.ui.cancel_comic_dubs_draft();
        }
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
        let scroll_speed = self.project_session.project.settings().scroll_speed;
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
        let generated_signs_became_stale = self
            .project_session
            .project
            .get_line(id)
            .and_then(|line| {
                self.project_session
                    .project
                    .detections()
                    .line(id)
                    .and_then(crate::detection::LineDetectionData::generated_signs)
                    .map(|info| !info.is_stale_for(&line.text) && info.is_stale_for(&text))
            })
            .unwrap_or(false);
        if generated_signs_became_stale {
            self.show_toast(
                "Le texte a changé : les signes de détection générés doivent être vérifiés avant régénération",
                5.0,
            );
        }
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
            let (old_text, old_emotions) = self
                .project_session
                .project
                .get_line(id)
                .map(|line| (line.text.clone(), line.text_emotions.clone()))
                .unwrap_or_default();
            let new_emotions =
                crate::rythmo_line::rebase_text_emotions(&old_emotions, &old_text, &text);
            let command = Command::UpdateLineText {
                line_id: id,
                old_text,
                new_text: text.clone(),
                old_emotions,
                new_emotions: new_emotions.clone(),
            };
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |cmd| {
                    if let Command::UpdateLineText {
                        new_text,
                        new_emotions: emotions,
                        ..
                    } = cmd
                    {
                        *new_text = text;
                        *emotions = new_emotions;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            let (old_text, old_emotions) = self
                .project_session
                .project
                .get_line(id)
                .map(|line| (line.text.clone(), line.text_emotions.clone()))
                .unwrap_or_default();
            let new_emotions =
                crate::rythmo_line::rebase_text_emotions(&old_emotions, &old_text, &text);
            self.execute_and_broadcast(Command::UpdateLineText {
                line_id: id,
                old_text,
                new_text: text,
                old_emotions,
                new_emotions,
            });
        }
    }

    pub fn set_text_emotion(
        &mut self,
        line_id: u64,
        range: Option<(usize, usize)>,
        emotion: Option<crate::rythmo_line::TextEmotion>,
    ) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        if !line.can_have_text_emotions() {
            return;
        }
        let old_emotions = line.text_emotions.clone();
        let mut changed = line.clone();
        let (start, end) = range.unwrap_or((0, changed.text.chars().count()));
        changed.set_text_emotion(start, end, emotion);
        if old_emotions == changed.text_emotions {
            return;
        }
        self.execute_and_broadcast(Command::SetTextEmotions {
            line_id,
            old_emotions,
            new_emotions: changed.text_emotions,
        });
    }

    pub fn open_text_emotion_menu(&mut self) {
        let line_id = self
            .ui_shell
            .ui
            .rythmo_state
            .editing_line
            .or_else(|| self.selected_line_ids().first().copied());
        let Some(line_id) = line_id else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        if !line.can_have_text_emotions() {
            return;
        }
        let range = (self.ui_shell.ui.rythmo_state.editing_line == Some(line_id))
            .then(|| self.ui_shell.ui.rythmo_state.line_input.selection_range())
            .flatten();
        let (x, y) = self.ui_shell.ui.cursor_pos;
        self.ui_shell.ui.rythmo_state.context_menu =
            Some(crate::workspaces::rythmo::view::LineContextMenu {
                line_id,
                x,
                y,
                hover_main: false,
                hover_change_character: false,
                hover_text_emotion: true,
                hover_generate_detection: false,
                hover_emotion_index: Some(0),
                hover_emotion_variant: None,
                text_range: range,
                hover_actor_index: None,
                hover_action_index: None,
                actor_scroll: 0.0,
            });
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
        crate::media_binary::user_data_dir().join("br_backup.json")
    }

    pub fn save_backup(&self) {
        use crate::export::{JsonExporter, ProjectExporter};
        use std::sync::atomic::{AtomicBool, Ordering};
        static BACKUP_RUNNING: AtomicBool = AtomicBool::new(false);
        // A slow disk must not pile up concurrent exports of the same file.
        if BACKUP_RUNNING.swap(true, Ordering::AcqRel) {
            return;
        }
        let path = Self::backup_path();
        let fps = self.fps();
        // Snapshot here; the JSON serialization and file write — the
        // expensive part on large projects — run off the UI thread.
        let project = self.project_session.project.snapshot();
        std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = JsonExporter.export(&project, fps, &path) {
                log::warn!("Auto-save failed: {e}");
            } else {
                log::info!("Auto-saved to {}", path.display());
            }
            BACKUP_RUNNING.store(false, Ordering::Release);
        });
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

    fn tick_video_at(&mut self, now: Instant) {
        let voicelines = self.active_workspace() == WorkspaceId::Voicelines;
        let comic_dubs = self.active_workspace() == WorkspaceId::ComicDubs;
        let comic_sequence = self.comic_dubs_playback.is_some();
        let player = if voicelines {
            &mut self.voicelines_player
        } else if comic_dubs {
            &mut self.comic_dubs_player
        } else {
            &mut self.playback.video_player
        };
        if let Some(player) = player {
            let prev_frame = player.current_frame();
            let (bgl, sampler) = (
                self.render.ui_renderer.texture_bind_group_layout(),
                self.render.ui_renderer.texture_sampler(),
            );
            player.tick_at(
                now,
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
            if (!comic_dubs || !comic_sequence)
                && !player.is_playing()
                && self.ui_shell.ui.is_playing()
            {
                self.playback.timeline.emit(TimelineEvent::PlaybackStopped);
                self.ui_shell.ui.toggle_play_pause();
            }
        }
        if voicelines {
            if let Some(end_ms) = self.voicelines_play_until_ms {
                if self.render_frame_at(now) * 10.0 >= end_ms as f64 {
                    if let Some(player) = &mut self.voicelines_player {
                        player.pause_for_seek();
                    }
                    self.voicelines_play_until_ms = None;
                    self.ui_shell.ui.set_playing(false);
                }
            }
        }
        if comic_dubs {
            self.tick_comic_dubs_playback(now);
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
                        false,
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

    fn poll_recording_mix_job(&mut self) -> bool {
        let result = match self.jobs.pending_recording_mix_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Recording preview mix job disconnected".into()))
                }
            },
            None => None,
        };
        let Some(result) = result else {
            return false;
        };
        self.jobs.pending_recording_mix_job = None;
        match result {
            Ok(decoded) => {
                for (path, samples) in decoded {
                    self.playback.recording_audio_cache.insert(path, samples);
                }
                self.schedule_recording_mix();
            }
            Err(error) if error == "recording mix cancelled" => {}
            Err(error) => {
                self.jobs.play_recording_mix_when_ready = false;
                log::error!("Recording preview mix failed: {error}");
                self.show_toast(error, 5.0);
            }
        }
        true
    }

    fn poll_project_transfer_prepare(&mut self) -> bool {
        let result = match self.project_transfer_prepare.as_ref() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("project preparation stopped".into())),
            },
            None => None,
        };
        let Some(result) = result else { return false };
        self.project_transfer_prepare = None;
        match result {
            Ok(metadata) => {
                if self.project_session.dirty
                    || self.project_session.project_path.as_ref()
                        != self.project_transfer_source.as_ref()
                    || self.project_session.huuid.as_ref().map(ToString::to_string)
                        != Some(metadata.project_huuid.clone())
                {
                    self.project_transfer_source = None;
                    self.show_toast(
                        crate::i18n::t("recording.project_transfer_requires_save"),
                        5.0,
                    );
                    return true;
                }
                self.project_transfer = Some(ProjectTransferRuntime {
                    metadata: metadata.clone(),
                    status: None,
                    receiver: crate::file_transfer::FileTransferReceiver::default(),
                });
                self.ui_shell
                    .ui
                    .open_project_transfer_modal(metadata.clone(), true, false);
                self.announce_open_container(
                    crate::i18n::t("recording.project_transfer.title"),
                    crate::i18n::t("recording.project_transfer.waiting").to_string(),
                );
                self.collaboration
                    .network
                    .request_project_transfer(&metadata);
                self.ui_shell.ui.sync_overlay =
                    Some(crate::i18n::t("recording.project_transfer_waiting").into());
                self.ui_shell.ui.sync_progress = 0.0;
            }
            Err(error) => {
                self.project_transfer_source = None;
                self.ui_shell.ui.sync_overlay = None;
                self.show_toast(
                    format!("{} {error}", crate::i18n::t("toast.save_failed")),
                    6.0,
                );
            }
        }
        true
    }

    fn poll_project_transfer_send(&mut self) -> bool {
        let result = match self.project_transfer_send.as_ref() {
            Some((_, receiver)) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("project transfer sender stopped".into()))
                }
            },
            None => None,
        };
        let Some(result) = result else { return false };
        let Some((request_id, _)) = self.project_transfer_send.take() else {
            return false;
        };
        if let Err(error) = result {
            self.collaboration
                .network
                .report_project_transfer(&request_id, false, Some(&error));
            self.show_toast(
                format!(
                    "{} {error}",
                    crate::i18n::t("recording.project_transfer.failed")
                ),
                6.0,
            );
        }
        true
    }

    fn poll_project_load_progress(&mut self) -> bool {
        let progress = self
            .jobs
            .pending_import_job
            .as_ref()
            .and_then(|job| job.progress.lock().ok().map(|progress| *progress));
        let Some(progress) = progress else {
            return false;
        };
        self.ui_shell.ui.set_project_load_progress(
            project_load_stage_key(progress.stage),
            project_load_overall_progress(progress),
        );
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
        let transfer_request_id = job.transfer_request_id.clone();
        if transfer_request_id.is_some() {
            self.project_transfer_loading_request.take();
        }
        self.ui_shell
            .ui
            .set_project_load_progress("loading_project.preparing_project", 0.94);

        match result {
            Ok(mut loaded) => {
                let is_legacy_json = loaded.is_legacy_json();
                let loaded_huuid = loaded.huuid.clone();
                let loaded_transaction_journal = loaded.transaction_journal.clone();
                let loaded_recording = loaded.recording.take();
                let loaded_voicelines = loaded.voicelines.take();
                let loaded_comic_dubs = loaded.comic_dubs.take();
                let loaded_default_uses_proxy = loaded.default_uses_proxy;
                let source_removed = crate::video_proxy::source_is_removed(&job.br_path);
                if source_removed {
                    self.clear_video_for_new_project();
                }
                let bundled_source = (!source_removed)
                    .then(|| loaded.source_video_path.clone())
                    .flatten();
                if bundled_source.is_none() {
                    self.clear_video_for_new_project();
                }
                let bundled_proxy = loaded
                    .proxy_video_path
                    .clone()
                    .filter(|_| loaded_default_uses_proxy);
                if let Some(source) = bundled_source.as_deref() {
                    if !self.load_video_for_playback(source, bundled_proxy.as_deref(), None, false)
                    {
                        let message = crate::i18n::t("toast.import_video_failed");
                        log::error!("{message} {}", source.display());
                        self.show_toast(message, 7.0);
                        if let Some(request_id) = transfer_request_id.as_deref() {
                            self.collaboration.network.report_project_transfer(
                                request_id,
                                false,
                                Some(message),
                            );
                        }
                        self.narration.announce_event(AccessibilityEvent::Error {
                            message: crate::i18n::t("accessibility.project_load_failed")
                                .to_string(),
                        });
                        self.ui_shell.ui.finish_project_load();
                        return true;
                    }
                }

                let mut recording_runtime = crate::recording_runtime::RecordingRuntime::new();
                let voicelines_project = match loaded_voicelines {
                    Some(voicelines) => match Self::voicelines_bind_loaded_project(
                        &mut recording_runtime,
                        voicelines.project,
                        voicelines.audio_paths,
                    ) {
                        Ok(project) => project,
                        Err(error) => {
                            self.show_toast(error, 7.0);
                            self.ui_shell.ui.finish_project_load();
                            return true;
                        }
                    },
                    None => crate::voicelines::VoicelinesProject::default(),
                };
                let comic_dubs_project = match loaded_comic_dubs {
                    Some(comic_dubs) => match Self::comic_dubs_bind_loaded_project(
                        &mut recording_runtime,
                        comic_dubs.project,
                        comic_dubs.image_paths,
                        comic_dubs.audio_paths,
                    ) {
                        Ok(project) => project,
                        Err(error) => {
                            self.show_toast(error, 7.0);
                            self.ui_shell.ui.finish_project_load();
                            return true;
                        }
                    },
                    None => crate::comic_dubs::ComicDubsProject::default(),
                };

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
                self.recording_runtime = recording_runtime;
                self.voicelines_project = voicelines_project;
                self.voicelines_revision = self.voicelines_revision.wrapping_add(1);
                self.voicelines_undo.clear();
                self.voicelines_redo.clear();
                self.voicelines_player = None;
                self.voicelines_play_until_ms = None;
                self.comic_dubs_project = comic_dubs_project;
                self.comic_dubs_revision = 0;
                self.comic_dubs_player = None;
                self.comic_dubs_playback = None;
                self.comic_dubs_undo.clear();
                self.comic_dubs_redo.clear();
                self.comic_dubs_imports.clear();
                self.ui_shell.ui.reset_comic_dubs_workspace();
                self.project_session.dirty = false;
                self.sync_audio_settings_to_player();
                self.project_session.project_path = if is_legacy_json {
                    None
                } else {
                    Some(job.br_path.clone())
                };
                self.project_session.huuid = if is_legacy_json { None } else { loaded_huuid };
                self.collaboration.network.update_local_huuid(
                    self.project_session.huuid.as_ref().map(ToString::to_string),
                );
                if !is_legacy_json {
                    if let Err(error) = crate::video_proxy::set_default_uses_proxy(
                        &job.br_path,
                        loaded_default_uses_proxy,
                    ) {
                        log::warn!("Failed to cache the project's default video: {error}");
                    }
                }
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
                self.schedule_recording_mix();
                self.ui_shell
                    .ui
                    .set_project_load_progress("loading_project.ready", 1.0);
                self.ui_shell.ui.finish_project_load();
                if let Some(request_id) = transfer_request_id.as_deref() {
                    self.collaboration.network.project_matches = true;
                    self.ui_shell
                        .ui
                        .set_project_transfer_result_path(job.br_path.display().to_string());
                    self.collaboration
                        .network
                        .report_project_transfer(request_id, true, None);
                    self.show_toast(
                        format!(
                            "{} {}",
                            crate::i18n::t("recording.project_transfer.loaded"),
                            job.br_path.display()
                        ),
                        6.0,
                    );
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
                self.ui_shell.ui.finish_project_load();
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
                if let Some(request_id) = transfer_request_id.as_deref() {
                    self.collaboration
                        .network
                        .report_project_transfer(request_id, false, Some(&e));
                }
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
                    && self.voicelines_revision == job.saved_voicelines_revision
                    && self.comic_dubs_revision == job.saved_comic_dubs_revision
                    && self.video_path() == job.source_video
                    && self.proxy_video_path() == job.proxy_video
                    && self.default_media_uses_proxy() == job.default_uses_proxy
                    && current_font == job.font_asset;

                self.project_session.project_path = Some(job.path.clone());
                self.project_session.huuid = Some(metadata.huuid);
                self.collaboration.network.update_local_huuid(
                    self.project_session.huuid.as_ref().map(ToString::to_string),
                );
                if let Err(error) =
                    crate::video_proxy::set_default_uses_proxy(&job.path, job.default_uses_proxy)
                {
                    log::warn!("Failed to cache the project's default video: {error}");
                }
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
                        if job.continuation == SaveContinuation::ProjectTransferAccept {
                            self.retry_project_transfer_after_save_failure();
                        }
                        self.show_toast(
                            crate::i18n::t("toast.transition_canceled_after_edit"),
                            7.0,
                        );
                    }
                }
            }
            Err(error) => {
                log::error!("Project save failed: {error}");
                if job.continuation == SaveContinuation::ProjectTransferAccept {
                    self.retry_project_transfer_after_save_failure();
                }
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
                let needs_seek = self
                    .playback
                    .video_player
                    .as_ref()
                    .is_some_and(|player| player.current_frame() != target.start_frame);
                if needs_seek {
                    self.seek_absolute_internal(target.start_frame, false);
                    self.finish_seek();
                }
                if self
                    .playback
                    .video_player
                    .as_ref()
                    .is_some_and(|player| !player.is_playing())
                {
                    self.toggle_play_pause_internal(false);
                }
                self.broadcast_recording_playback(target.start_frame, true);
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
                if self.is_online_recording_actor() {
                    let device = crate::config::recording_input_device();
                    self.recording_input_preflight = Some((device, false));
                    self.collaboration.network.send_recording_ready(false);
                }
                self.recording_error(message);
                changed = true;
            }
            RecordingRuntimeEvent::Finished { completed, path } => {
                let target = completed.target;
                let audio = completed.audio.clone();
                let role = self.ui_shell.ui.recording_role();
                let commits_locally = matches!(role, RecordingRole::Solo | RecordingRole::Director);
                self.recording_runtime
                    .remember_audio_path(&audio.checksum, &path);

                if commits_locally {
                    let operation = completed.clone().into_project_operation(
                        self.project_session.recording_project.timeline_fps(),
                    );
                    match self.apply_recording_operation(operation) {
                        Ok(()) => {}
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
                            let transfer_id = metadata.transfer_id.clone();
                            self.recording_upload_acks.push(transfer_id.clone());
                            self.recording_uploads.push((
                                transfer_id,
                                self.collaboration.network.send_audio_file(path, metadata),
                            ));
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
                self.show_toast(
                    crate::i18n::t(if role.is_online() {
                        "recording.capture.uploading"
                    } else {
                        "recording.capture.finished"
                    }),
                    4.0,
                );
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

    fn poll_recording_uploads(&mut self) -> bool {
        let mut pending = Vec::with_capacity(self.recording_uploads.len());
        let mut completed = 0_usize;
        let mut failures = Vec::new();
        for (transfer_id, receiver) in self.recording_uploads.drain(..) {
            match receiver.try_recv() {
                Ok(Ok(())) => completed += 1,
                Ok(Err(error)) => failures.push((transfer_id, error)),
                Err(TryRecvError::Empty) => pending.push((transfer_id, receiver)),
                Err(TryRecvError::Disconnected) => failures.push((
                    transfer_id,
                    "recording upload stopped unexpectedly".to_string(),
                )),
            }
        }
        self.recording_uploads = pending;
        for (transfer_id, error) in failures.iter() {
            self.recording_upload_acks
                .retain(|pending| pending != transfer_id);
            self.recording_error(error);
        }
        completed > 0 || !failures.is_empty()
    }

    pub fn tick_background(&mut self) -> bool {
        let mut changed = false;

        changed |= self.tick_keyboard_pan();
        changed |= self.poll_recording_runtime();
        changed |= self.poll_recording_uploads();
        changed |= self.poll_voicelines_jobs();
        changed |= self.poll_comic_dubs_jobs();

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
        changed |= self.poll_project_transfer_prepare();
        changed |= self.poll_project_transfer_send();
        changed |= self.poll_project_load_progress();
        changed |= self.tick_scroll_decode();
        changed |= self.poll_export_job();
        changed |= self.poll_recording_mix_job();
        changed |= self.poll_proxy_job();
        changed |= self.poll_import_job();
        changed |= self.poll_save_job();
        changed |= self.poll_pending_protocol();
        changed |= self.poll_waveform_change();
        changed
    }

    /// Entry point called at startup when the app is launched from a
    /// `coquerythmo://` URI.
    pub fn handle_protocol_url(&mut self, url: &str) {
        let Some(payload) = ProtocolPayload::from_url(url) else {
            self.show_toast(crate::i18n::t("toast.protocol_invalid_link"), 6.0);
            return;
        };
        if !payload.is_valid() {
            self.show_toast(crate::i18n::t("toast.protocol_invalid_link"), 6.0);
            return;
        }
        match payload.kind() {
            Some(ProtocolKind::Host) => self.protocol_start_host(payload),
            Some(ProtocolKind::Join) => self.protocol_start_join(payload),
            None => {
                self.show_toast(crate::i18n::t("toast.protocol_invalid_link"), 6.0);
            }
        }
    }

    /// Host flow: close the current project (with save prompt when dirty),
    /// load the target project, then create a room as the director. The flow
    /// continues either via `protocol_current_closed` once the close is done
    /// (with/without a save, see [`SaveContinuation::ProtocolHost`]), or via
    /// [`Self::poll_pending_protocol`] once the `.coquerythmo` import job has
    /// finished parsing the target file.
    fn protocol_start_host(&mut self, payload: ProtocolPayload) {
        let Some(project_path) = payload
            .project
            .clone()
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from)
        else {
            log::warn!("protocol host link: missing project path");
            self.show_toast(crate::i18n::t("toast.protocol_invalid_link"), 6.0);
            return;
        };
        if !project_path.exists() {
            self.show_toast(
                format!(
                    "{} {}",
                    crate::i18n::t("toast.protocol_project_missing"),
                    project_path.display()
                ),
                6.0,
            );
            return;
        }
        if self.is_project_save_in_progress() {
            self.show_toast(crate::i18n::t("toast.project_change_blocked_saving"), 5.0);
            return;
        }
        self.pending_protocol = Some(PendingProtocolFlow {
            payload,
            stage: PendingProtocolStage::ClosingCurrentProject,
        });
        if self.project_session.dirty && self.project_session.project_path.is_some() {
            // Reuse the standard close-project prompt; `handle_protocol_close_action`
            // below intercepts the `CloseProject{Save,Discard}` actions before the
            // generic dispatcher can act on them, so we can resume the flow.
            self.open_save_prompt(crate::ui::save_prompt_modal::SavePromptKind::CloseProject);
        } else if self.project_session.dirty {
            // Project was freshly created and never saved: no raccourci.
            // Fall back to a manual save — the flow is aborted so the user
            // can pick a location without losing the target project path.
            self.pending_protocol = None;
            self.open_save_prompt(crate::ui::save_prompt_modal::SavePromptKind::CloseProject);
            self.show_toast(crate::i18n::t("toast.protocol_requires_saved_project"), 6.0);
            return;
        } else {
            // Nothing worth saving → close immediately and load the target.
            self.protocol_current_closed();
        }
    }

    /// True when a `coquerythmo://` host flow is currently waiting for the
    /// current project to close and there is a save prompt on screen. Used by
    /// the dispatcher to dispatch `SaveContinuation::ProtocolHost`.
    pub(crate) fn protocol_is_awaiting_close(&self) -> bool {
        matches!(
            self.pending_protocol.as_ref().map(|p| p.stage),
            Some(PendingProtocolStage::ClosingCurrentProject)
        )
    }

    /// Abort the pending protocol quick-setup flow (e.g. when the user cancels
    /// the save prompt or an error blocks progression).
    pub(crate) fn protocol_abort(&mut self) {
        self.pending_protocol = None;
    }

    /// Called by the dispatcher when the save prompt answered "Discard": close
    /// without saving, then load the linked project.
    pub(crate) fn protocol_discard_current_and_continue(&mut self) {
        self.protocol_current_closed();
    }

    /// Build and copy a `coquerythmo://` quick-setup link of the given kind
    /// to the clipboard, using the current network configuration and session
    /// state. Toasts are shown both on success and when prerequisites are
    /// missing (no active room, no saved project for hosts).
    pub(crate) fn copy_protocol_link_to_clipboard(&mut self, kind: ProtocolKind) {
        let cfg = crate::config::get().clone();
        let server = format!("{}:{}", cfg.network.server_ip, cfg.network.server_port);
        let payload = match kind {
            ProtocolKind::Host => {
                let Some(project_path) = self.project_session.project_path.clone() else {
                    self.show_toast(crate::i18n::t("toast.protocol_host_requires_project"), 6.0);
                    return;
                };
                let Some(username) = (!cfg.network.username.trim().is_empty())
                    .then(|| cfg.network.username.trim().to_string())
                else {
                    self.show_toast(crate::i18n::t("toast.protocol_host_requires_username"), 6.0);
                    return;
                };
                ProtocolPayload::host(
                    &server,
                    username,
                    cfg.network.password.clone(),
                    project_path.to_string_lossy(),
                )
            }
            ProtocolKind::Join => {
                let Some(code) = self.collaboration.network.room_code.clone() else {
                    self.show_toast(crate::i18n::t("toast.protocol_join_requires_room"), 6.0);
                    return;
                };
                ProtocolPayload::join(&server, cfg.network.password.clone(), code)
            }
        };
        let url = payload.to_url();
        crate::platform::clipboard_set(&url);
        log::info!("protocol: copied quick-setup link to clipboard");
        self.show_toast(crate::i18n::t("toast.protocol_copied"), 4.0);
    }

    pub fn open_room_invitation(&mut self) {
        let Some(code) = self.collaboration.network.room_code.clone() else {
            self.show_toast(crate::i18n::t("toast.protocol_join_requires_room"), 6.0);
            return;
        };
        let cfg = crate::config::get().clone();
        let server = format!("{}:{}", cfg.network.server_ip, cfg.network.server_port);
        let link = ProtocolPayload::join(&server, cfg.network.password, code.clone()).to_url();
        self.ui_shell.ui.open_room_invitation(code, link);
        let first = self
            .ui_shell
            .ui
            .modal_host
            .invitation
            .as_ref()
            .map(|modal| modal.keyboard_focus_label())
            .unwrap_or_else(|| crate::i18n::t("invite.copy_link").to_string());
        self.announce_open_container(crate::i18n::t("invite.title"), first);
    }

    pub fn copy_room_code_to_clipboard(&mut self) {
        let Some(code) = self.collaboration.network.room_code.as_deref() else {
            self.show_toast(crate::i18n::t("toast.protocol_join_requires_room"), 6.0);
            return;
        };
        crate::platform::clipboard_set(code);
        self.show_toast(crate::i18n::t("toast.room_code_copied"), 4.0);
    }

    /// Advances the host flow once the previous project has fully closed
    /// (either discarded, saved with [`SaveContinuation::ProtocolHost`], or
    /// the app was already idle). Clears the workspace, kicks off the target
    /// import, and moves to `ImportingTargetProject`.
    pub(crate) fn protocol_current_closed(&mut self) {
        let stage = self
            .pending_protocol
            .as_ref()
            .map(|p| p.stage)
            .unwrap_or(PendingProtocolStage::ClosingCurrentProject);
        // Only act while still in the closing stage.
        if stage != PendingProtocolStage::ClosingCurrentProject {
            return;
        }
        self.clear_video_for_new_project();
        crate::application::edit_service::EditExecutor::reset(&mut self.project_session);
        self.recording_runtime = crate::recording_runtime::RecordingRuntime::new();
        self.reset_voicelines_document();
        self.reset_comic_dubs_document();
        self.ui_shell.ui.reset_recording_workspace();
        crate::vector_text::clear_project_font();
        self.render.ui_renderer.clear_text_cache();

        let Some(pending) = self.pending_protocol.as_mut() else {
            return;
        };
        let Some(project_path) = pending.payload.project.clone() else {
            self.pending_protocol = None;
            return;
        };
        pending.stage = PendingProtocolStage::ImportingTargetProject;
        log::info!("protocol: closed current project, loading {}", project_path);
        self.start_br_import(std::path::PathBuf::from(project_path));
    }

    /// Drive the protocol flow past its `ImportingTargetProject` stage. Returns
    /// true when UI state changed (redraw requested).
    pub(crate) fn poll_pending_protocol(&mut self) -> bool {
        let Some(pending) = self.pending_protocol.as_mut() else {
            return false;
        };
        if pending.stage != PendingProtocolStage::ImportingTargetProject {
            return false;
        }
        if self.jobs.pending_import_job.is_some() {
            return false;
        }
        // Import just completed; make sure the project is actually ready to be
        // shared. A silent failure here would block the room creation, so a
        // toast explains what went wrong.
        let payload = pending.payload.clone();
        let project_path_ok = self
            .project_session
            .project_path
            .as_ref()
            .is_some_and(|current| match payload.project.as_deref() {
                Some(expected) => current == std::path::Path::new(expected),
                None => true,
            });
        let ready =
            project_path_ok && !self.project_session.dirty && self.project_session.huuid.is_some();
        if !ready {
            log::warn!(
                "protocol: import done but not ready (dirty={}, path ok={})",
                self.project_session.dirty,
                project_path_ok
            );
            self.pending_protocol = None;
            self.show_toast(crate::i18n::t("toast.protocol_project_not_ready"), 6.0);
            return true;
        }

        // Everything is ready — create a fresh room as the director. The
        // pending flow is dropped once the connect request is fired; room
        // arrival / director role / workspace switch are handled by
        // `handle_network_packet` (RoomCreated).
        let payload = payload.clone();
        let (server_ip, server_port) = payload.server_endpoint();
        let Some(username) = payload.username else {
            self.pending_protocol = None;
            return true;
        };
        let password = payload.password;
        let project_huuid = self
            .project_session
            .huuid
            .as_ref()
            .map(|h| h.to_string())
            .expect("ready implies a project huuid");
        self.pending_protocol = None;
        self.begin_network_connect();
        self.collaboration.network.connect_and_send(
            &server_ip,
            server_port,
            &password,
            Packet::CreateRoom {
                username,
                project_huuid,
            },
        );
        self.rebuild_topbar_for_network();
        true
    }

    /// Join flow: open the connect modal so the user only has to type their
    /// name. The actual network connect is left to the standard
    /// [`crate::ui::primitives::UiAction::NetworkConnect`] handler — no need
    /// for a parallel path.
    fn protocol_start_join(&mut self, payload: ProtocolPayload) {
        let code = payload.code.clone().unwrap_or_default();
        let (ip, port) = payload.server_endpoint();
        // Persist the server and password so the connect action has the
        // hidden link fields available without exposing them in the prompt.
        {
            let mut cfg = crate::config::get().clone();
            cfg.network.server_ip = ip.clone();
            cfg.network.server_port = port;
            cfg.network.password = payload.password.clone();
            cfg.save();
        }
        // The invitation never embeds a username: the recipient must provide
        // their own, while the modal keeps the other link fields hidden.
        self.open_connect_modal_with_room(&ip, port, &code, &payload.password);
    }

    /// Resumes the pending host flow once the user has saved (or discarded)
    /// the previous project. Called from the save dispatch chain (the
    /// `SaveContinuation::ProtocolHost` branch of [`super::event_loop::run`])
    /// and from the `CloseProjectDiscard` action handler.
    pub(crate) fn protocol_resume_after_save(&mut self) {
        self.protocol_current_closed();
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
        self.render.refresh_interval()
    }

    fn scroll_decode_due(&self, now: Instant) -> bool {
        self.playback.scroll_needs_decode
            && self.playback.last_scroll_time.is_some_and(|last| {
                now.duration_since(last).as_millis() >= constants::SCROLL_DECODE_DELAY_MS
            })
    }

    fn playback_preparation_pending(&self) -> bool {
        self.playback
            .video_player
            .as_ref()
            .is_some_and(|player| player.is_playback_preparing())
    }

    fn playback_preparation_redraw_due(&self, now: Instant) -> bool {
        self.playback_preparation_pending()
            && now.saturating_duration_since(self.render.last_redraw()) >= Duration::from_millis(50)
    }

    fn periodic_redraw_due(&self, now: Instant) -> bool {
        if self.ui_shell.ui.has_active_progress()
            || self.jobs.pending_proxy_job.is_some()
            || self.jobs.pending_recording_mix_job.is_some()
            || self.jobs.pending_import_job.is_some()
            || self.jobs.pending_save_job.is_some()
            || self.ui_shell.ui.project_transfer_modal.is_some()
        {
            return now.saturating_duration_since(self.render.last_redraw())
                >= Duration::from_millis(100);
        }

        if self.ui_shell.ui.is_editing_text() {
            return self
                .ui_shell
                .ui
                .next_cursor_blink_deadline()
                .is_some_and(|deadline| deadline <= now)
                || now.saturating_duration_since(self.render.last_redraw())
                    >= Duration::from_millis(500);
        }

        false
    }

    pub fn needs_redraw_now(&self) -> bool {
        let now = Instant::now();
        self.scroll_decode_due(now)
            || self.periodic_redraw_due(now)
            || self.playback_preparation_redraw_due(now)
            || self.waveform_redraw_pending()
            || self.needs_continuous_redraw()
            || self.secondary_needs_continuous_redraw()
    }

    pub fn needs_continuous_redraw(&self) -> bool {
        (self.is_video_playing() && !self.playback_preparation_pending())
            || self.recording_runtime.is_active()
            || self.ui_shell.ui.needs_animation_or_interaction()
    }

    pub fn secondary_needs_continuous_redraw(&self) -> bool {
        self.has_secondary_display()
            && self.is_video_playing()
            && !self.playback_preparation_pending()
    }

    pub fn next_wake_deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut deadline: Option<Instant> = None;
        let mut push_deadline = |candidate: Instant| {
            deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
        };

        if self.ui_shell.ui.has_active_progress()
            || self.jobs.pending_proxy_job.is_some()
            || self.jobs.pending_recording_mix_job.is_some()
            || self.jobs.pending_import_job.is_some()
            || self.jobs.pending_save_job.is_some()
            || self.ui_shell.ui.project_transfer_modal.is_some()
        {
            push_deadline(self.render.last_redraw() + Duration::from_millis(100));
        }

        if self.ui_shell.ui.is_editing_text() {
            if let Some(cursor_deadline) = self.ui_shell.ui.next_cursor_blink_deadline() {
                push_deadline(cursor_deadline);
            } else {
                push_deadline(self.render.last_redraw() + Duration::from_millis(500));
            }
        }

        if self.playback_preparation_pending() {
            push_deadline(self.render.last_redraw() + Duration::from_millis(50));
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
        // FIFO may block while acquiring an available swapchain texture. Do
        // that before sampling visual time so the bande rythmo and video are
        // not rendered from a timestamp that became stale during acquisition.
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

        // One monotonic sample drives every time-dependent visual decision in
        // this frame. FrameTiming observes the frame but never schedules it.
        let frame_sample = self.render.begin_frame(Instant::now());
        self.tick_video_at(frame_sample.instant);

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

        self.apply_automation_if_needed();
        let render_frame = self.render_frame_at(frame_sample.instant);
        if self.active_workspace() == WorkspaceId::Recording {
            self.sync_recording_workspace_ui_at(render_frame);
        }

        // Video quad
        let recording_choice = self.active_workspace() == WorkspaceId::Recording
            && self.ui_shell.ui.recording_page()
                == crate::ui::recording_workspace::RecordingPage::Choice;
        let video_quad = if recording_choice
            || !workspace_shows_project_video(self.active_workspace())
            || self.window_manager.secondary_kind
                == Some(crate::application::window_service::SecondaryWindowKind::Video)
            || self.ui_shell.ui.automation_open()
        {
            None
        } else {
            build_video_quad(&self.playback.video_player, &self.ui_shell.ui)
        };
        let current_frame = self.current_frame();

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
        let fps = self.active_fps();
        let waveform_offset_frames = self.active_audio_offset_frames();
        let waveform_is_instrumental = self.active_audio_is_instrumental();
        self.project_session
            .render_index
            .refresh(&self.project_session.project);
        // Bridge the line clipboard fact for the contextual shortcut panel.
        self.ui_shell.ui.line_clipboard_available = self.line_clipboard.is_some();
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
            &self.voicelines_project,
            &self.comic_dubs_project,
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
        self.render.finish_present(Instant::now());
    }

    pub fn render_secondary_display(&mut self, window_id: WindowId) {
        // The main frame owns playback advancement and display cadence.
        // Rendering the secondary surface must not create a second clock or
        // consume decoded frames twice.
        let is_daw = self.window_manager.secondary_kind
            == Some(crate::application::window_service::SecondaryWindowKind::Daw);
        if is_daw {
            self.sync_recording_daw_ui();
        }
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

        if is_daw {
            self.ui_shell.ui.render_recording_daw(
                &mut self.render.ui_renderer,
                &self.render.gfx.device,
                &self.render.gfx.queue,
                &mut encoder,
                &view,
                width,
                height,
            );
            self.render
                .gfx
                .queue
                .submit(std::iter::once(encoder.finish()));
            surface_texture.present();
            return;
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
            video_quad.as_ref().map(|(bg, inst)| (*bg, *inst)),
            &[],
            &[],
            &[],
            &[],
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
            transform: [0.0, 0.0, 0.5, 0.5],
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
            transform: [0.0, 0.0, 0.5, 0.5],
        },
    ))
}

fn ping_server_http(
    ip: &str,
    port: u16,
    password: String,
    results: std::sync::Arc<std::sync::Mutex<Vec<PingResult>>>,
) {
    let url = format!("http://{}:{}/info?password={}", ip, port, urlencoding::encode(&password));

    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build(),
    );

    let response = agent.get(&url).call();

    match response {
        Ok(mut resp) => {
            if resp.status() == 200 {
                if let Ok(info) = resp.body_mut().read_json::<serde_json::Value>() {
                    let name = info["name"].as_str().unwrap_or("").to_string();
                    let motd = info["motd"].as_str().unwrap_or("").to_string();
                    let online = info["online"].as_u64().unwrap_or(0) as u32;
                    let max_slots = info["max_slots"].as_u64().unwrap_or(0) as u32;
                    if let Ok(mut r) = results.lock() {
                        r.push(PingResult {
                            ip: ip.to_string(),
                            port,
                            name,
                            motd,
                            online,
                            max_slots,
                            success: true,
                        });
                    }
                    return;
                }
            } else if resp.status() == 401 {
                // Invalid password - server is up but auth failed
                if let Ok(mut r) = results.lock() {
                    r.push(PingResult {
                        ip: ip.to_string(),
                        port,
                        name: String::new(),
                        motd: String::new(),
                        online: 0,
                        max_slots: 0,
                        success: false,
                    });
                }
                return;
            }
            // Other HTTP error
            if let Ok(mut r) = results.lock() {
                r.push(PingResult {
                    ip: ip.to_string(),
                    port,
                    name: String::new(),
                    motd: String::new(),
                    online: 0,
                    max_slots: 0,
                    success: false,
                });
            }
        }
        Err(_) => {
            // Connection error / timeout
            if let Ok(mut r) = results.lock() {
                r.push(PingResult {
                    ip: ip.to_string(),
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
