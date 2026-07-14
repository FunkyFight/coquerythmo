//! Background job ownership for proxy, import and export workflows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::export::ProjectData;

pub(crate) struct PendingProxyJob {
    pub source_path: PathBuf,
    pub receiver: Receiver<Result<PathBuf, String>>,
}

pub(crate) struct PendingExportJob {
    pub receiver: Receiver<Result<(), String>>,
}

pub(crate) struct PendingImportJob {
    pub br_path: PathBuf,
    pub receiver: Receiver<Result<ProjectData, String>>,
}

pub struct JobManager {
    pub(crate) pending_proxy_job: Option<PendingProxyJob>,
    pub(crate) pending_export_job: Option<PendingExportJob>,
    pub(crate) pending_import_job: Option<PendingImportJob>,
    pub(crate) active_export_cancel: Option<Arc<AtomicBool>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            pending_proxy_job: None,
            pending_export_job: None,
            pending_import_job: None,
            active_export_cancel: None,
        }
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}
