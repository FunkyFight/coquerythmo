//! Background job ownership for proxy, import and export workflows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use crate::project_archive::{LoadedProject, ProjectLoadProgress};

pub(crate) struct PendingProxyJob {
    pub source_path: PathBuf,
    pub source_media_id: Option<crate::project::MediaId>,
    pub receiver: Receiver<Result<PathBuf, String>>,
}

pub(crate) struct PendingExportJob {
    pub receiver: Receiver<Result<(), String>>,
}

pub(crate) struct PendingRecordingMixJob {
    pub cancel: Arc<AtomicBool>,
    pub receiver: Receiver<Result<Vec<(PathBuf, Arc<Vec<f32>>)>, String>>,
}

pub(crate) struct PendingImportJob {
    pub br_path: PathBuf,
    pub receiver: Receiver<Result<LoadedProject, String>>,
    pub progress: Arc<Mutex<ProjectLoadProgress>>,
    pub transfer_request_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveContinuation {
    None,
    NewProject,
    CloseProject,
    ExitApplication,
    ProjectTransfer,
    ProjectTransferAccept,
    /// After saving, close the current project chain and continue the pending
    /// `coquerythmo://` host flow (load the linked project, create a room).
    ProtocolHost,
}

pub(crate) struct PendingSaveJob {
    pub path: PathBuf,
    pub saved_revision: u64,
    pub saved_recording_revision: u64,
    pub saved_voicelines_revision: u64,
    pub saved_comic_dubs_revision: u64,
    pub source_video: Option<PathBuf>,
    pub proxy_video: Option<PathBuf>,
    pub default_uses_proxy: bool,
    pub font_asset: Option<PathBuf>,
    pub continuation: SaveContinuation,
    pub receiver: Receiver<Result<crate::project_archive::SavedProjectMetadata, String>>,
}

pub struct JobManager {
    pub(crate) pending_proxy_job: Option<PendingProxyJob>,
    pub(crate) pending_export_job: Option<PendingExportJob>,
    pub(crate) pending_recording_mix_job: Option<PendingRecordingMixJob>,
    pub(crate) pending_import_job: Option<PendingImportJob>,
    pub(crate) pending_save_job: Option<PendingSaveJob>,
    pub(crate) active_export_cancel: Option<Arc<AtomicBool>>,
    pub(crate) requested_proxy_source: Option<(crate::project::MediaId, PathBuf)>,
    pub(crate) transition_after_save_ready: Option<SaveContinuation>,
    pub(crate) play_recording_mix_when_ready: bool,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            pending_proxy_job: None,
            pending_export_job: None,
            pending_recording_mix_job: None,
            pending_import_job: None,
            pending_save_job: None,
            active_export_cancel: None,
            requested_proxy_source: None,
            transition_after_save_ready: None,
            play_recording_mix_when_ready: false,
        }
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}
