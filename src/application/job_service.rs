//! Background job ownership for proxy, import and export workflows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::project_archive::LoadedProject;

pub(crate) struct PendingProxyJob {
    pub source_path: PathBuf,
    pub receiver: Receiver<Result<PathBuf, String>>,
}

pub(crate) struct PendingExportJob {
    pub receiver: Receiver<Result<(), String>>,
}

pub(crate) struct PendingImportJob {
    pub br_path: PathBuf,
    pub receiver: Receiver<Result<LoadedProject, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveContinuation {
    None,
    NewProject,
    CloseProject,
    ExitApplication,
}

pub(crate) struct PendingSaveJob {
    pub path: PathBuf,
    pub saved_revision: u64,
    pub source_video: PathBuf,
    pub proxy_video: Option<PathBuf>,
    pub font_asset: PathBuf,
    pub continuation: SaveContinuation,
    pub receiver: Receiver<Result<(), String>>,
}

pub struct JobManager {
    pub(crate) pending_proxy_job: Option<PendingProxyJob>,
    pub(crate) pending_export_job: Option<PendingExportJob>,
    pub(crate) pending_import_job: Option<PendingImportJob>,
    pub(crate) pending_save_job: Option<PendingSaveJob>,
    pub(crate) active_export_cancel: Option<Arc<AtomicBool>>,
    pub(crate) transition_after_save_ready: Option<SaveContinuation>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            pending_proxy_job: None,
            pending_export_job: None,
            pending_import_job: None,
            pending_save_job: None,
            active_export_cancel: None,
            transition_after_save_ready: None,
        }
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}
