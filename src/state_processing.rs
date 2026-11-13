//! State-level integration of the « Traitement » workspace: choosing where
//! the engine lives, installing it and the models, importing audios, running
//! a model and listening to the tracks it produced.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime};

use crate::accessibility::AccessibilityEvent;
use crate::application::command::{
    FileFilterSpec, FilePickerIntent, FilePickerMode, FilePickerRequest,
};
use crate::application::workspace_service::WorkspaceId;
use crate::i18n::t;
use crate::processing::install::{self, Outcome, Step, TaskHandle, TaskKind, TaskSnapshot};
use crate::processing::job::{self, JobEnd, JobHandle, Stage};
use crate::processing::{
    self, format_bytes, AudioItem, GpuInfo, InstallLocation, ProcessingAction, ProcessingSession,
    TorchVariant, Track, CATALOG,
};
use crate::state::State;
use crate::ui::processing_workspace::{
    EngineView, GpuLine, ItemView, ModelState, ModelView, ProcessingModal, ProcessingView,
    ProgressView, TrackView,
};
use crate::video::VideoPlayer;

const SOURCE_EXTENSIONS: &[&str] = &[
    "flac", "wav", "mp3", "ogg", "m4a", "aac", "opus", "mp4", "mkv", "mov", "webm", "avi",
];
const EXPORT_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg", "m4a"];

struct PendingImport {
    id: u64,
    name: String,
    directory: PathBuf,
    receiver: Receiver<Result<Track, String>>,
}

struct PendingExport {
    receiver: Receiver<Result<String, String>>,
    /// Once written, the file becomes a new audio of the project.
    add_to_project: Option<PathBuf>,
}

enum GpuProbe {
    NotStarted,
    Pending(Receiver<Option<GpuInfo>>),
    Known(Option<GpuInfo>),
}

/// What is on disk, read off the UI thread's hot path: checked when the tab
/// opens and after each installation task, never every frame.
struct Installed {
    runtime: Option<TorchVariant>,
    models: Vec<bool>,
    holds_anything: bool,
    disk_bytes: u64,
}

pub(crate) struct ProcessingRuntime {
    session: ProcessingSession,
    session_dir: PathBuf,
    /// Where the engine and the models live. `None` until the user chooses.
    location: Option<InstallLocation>,
    /// Installation or removal of the engine (or of everything).
    engine_task: Option<TaskHandle>,
    /// Downloads and removals of models, by catalog index. Several run at
    /// the same time.
    model_tasks: Vec<(usize, TaskHandle)>,
    job: Option<JobHandle>,
    imports: Vec<PendingImport>,
    exports: Vec<PendingExport>,
    disk_probe: Option<Receiver<u64>>,
    selected_model: usize,
    best_quality: bool,
    /// Track of the active audio being listened to. It is also the one a
    /// model is applied to.
    listening: u64,
    gpu: GpuProbe,
    installed: Installed,
    /// The engine window was shown once on entering the tab without engine.
    engine_prompted: bool,
    /// Last quarter of the model pass that was spoken.
    announced_quarter: u8,
    /// Last installation step that was spoken.
    announced_step: Option<Step>,
}

impl ProcessingRuntime {
    pub(crate) fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0);
        Self {
            session: ProcessingSession::default(),
            session_dir: processing::work_dir()
                .join(format!("session-{}-{unique}", std::process::id())),
            location: InstallLocation::configured(),
            engine_task: None,
            model_tasks: Vec::new(),
            job: None,
            imports: Vec::new(),
            exports: Vec::new(),
            disk_probe: None,
            selected_model: 0,
            best_quality: false,
            listening: AudioItem::ROOT,
            gpu: GpuProbe::NotStarted,
            installed: Installed {
                runtime: None,
                models: vec![false; CATALOG.len()],
                holds_anything: false,
                disk_bytes: 0,
            },
            engine_prompted: false,
            announced_quarter: 0,
            announced_step: None,
        }
    }

    fn gpu(&self) -> Option<&GpuInfo> {
        match &self.gpu {
            GpuProbe::Known(gpu) => gpu.as_ref(),
            _ => None,
        }
    }

    fn torch_variant(&self) -> TorchVariant {
        match &self.gpu {
            GpuProbe::Known(Some(gpu)) => gpu.torch_variant(),
            GpuProbe::Known(None) => TorchVariant::Cpu,
            _ => TorchVariant::Cuda128,
        }
    }

    fn is_busy(&self) -> bool {
        self.engine_task.is_some()
            || !self.model_tasks.is_empty()
            || self.job.is_some()
            || !self.imports.is_empty()
    }

    fn model_task(&self, index: usize) -> Option<&TaskHandle> {
        self.model_tasks
            .iter()
            .find(|(model, _)| *model == index)
            .map(|(_, task)| task)
    }

    fn refresh_installed(&mut self) {
        let location = self.location.as_ref();
        self.installed.runtime = location.and_then(InstallLocation::installed_runtime);
        self.installed.models = CATALOG
            .iter()
            .map(|model| location.is_some_and(|location| location.has_model(model)))
            .collect();
        self.installed.holds_anything = location.is_some_and(InstallLocation::holds_anything);
        // Keep an installed model selected, so the run button is one click away.
        if !self
            .installed
            .models
            .get(self.selected_model)
            .copied()
            .unwrap_or(false)
        {
            if let Some(first) = self
                .installed
                .models
                .iter()
                .position(|installed| *installed)
            {
                self.selected_model = first;
            }
        }
        match location {
            Some(location) => {
                let root = location.root().to_path_buf();
                let (sender, receiver) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = sender.send(processing::directory_size(&root));
                });
                self.disk_probe = Some(receiver);
            }
            None => {
                self.installed.disk_bytes = 0;
                self.disk_probe = None;
            }
        }
    }
}

impl Drop for ProcessingRuntime {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel();
        }
        let _ = std::fs::remove_dir_all(&self.session_dir);
    }
}

/// Working audio of sessions that ended without cleaning up (crash, kill).
fn remove_stale_sessions() {
    let Ok(entries) = std::fs::read_dir(processing::work_dir()) else {
        return;
    };
    let day = Duration::from_secs(24 * 3_600);
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > day);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn first_line(text: &str) -> &str {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text)
        .trim()
}

fn clock(seconds: f32) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `folder/stem.extension`, or the first `stem (2)`, `stem (3)`… that does
/// not exist yet: adding a track never overwrites a file.
fn free_path(folder: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut candidate = folder.join(format!("{stem}.{extension}"));
    let mut number = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{stem} ({number}).{extension}"));
        number += 1;
    }
    candidate
}

/// The installed model before or after `current`, going round the list.
/// `None` when no model is installed.
fn next_installed(installed: &[bool], current: usize, step: i32) -> Option<usize> {
    let count = installed.len();
    if !installed.iter().any(|installed| *installed) {
        return None;
    }
    let step = if step < 0 { count - 1 } else { 1 };
    let mut index = current.min(count - 1);
    for _ in 0..count {
        index = (index + step) % count;
        if installed[index] {
            return Some(index);
        }
    }
    None
}

/// Deletes the files of removed tracks off the interface thread, then the
/// folders they leave empty.
fn delete_track_files(files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for file in &files {
            let _ = std::fs::remove_file(file);
        }
        for file in &files {
            if let Some(folder) = file.parent() {
                // `remove_dir` leaves a folder that still holds a sibling.
                let _ = std::fs::remove_dir(folder);
            }
        }
    });
}

/// The folder was chosen or accepted by the user, not merely proposed.
fn location_is_confirmed(location: &InstallLocation, saved: Option<PathBuf>) -> bool {
    saved.as_deref() == Some(location.root())
}

/// Title, detail and fraction of an installation task for a progress bar.
fn task_progress(snapshot: &TaskSnapshot) -> ProgressView {
    let title = t(snapshot.step.label_key()).to_string();
    let (detail, fraction) = match snapshot.bytes {
        Some((done, total)) if total > 0 => (
            format!("{} / {}", format_bytes(done), format_bytes(total)),
            Some(done as f32 / total as f32),
        ),
        _ => (snapshot.detail.clone(), None),
    };
    ProgressView {
        title,
        detail,
        fraction,
    }
}

impl State {
    /// Called when the tab opens: looks at what is installed and at the
    /// graphics card, without blocking the interface.
    pub(crate) fn processing_on_activate(&mut self) {
        if matches!(self.processing.gpu, GpuProbe::NotStarted) {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                remove_stale_sessions();
                let _ = sender.send(processing::detect_gpu());
            });
            self.processing.gpu = GpuProbe::Pending(receiver);
        }
        if self.processing.location.is_none() {
            // A proposal the engine window shows and the user can change.
            self.processing.location = InstallLocation::suggested();
        }
        self.processing.refresh_installed();
        if self.processing_player.is_none() && self.processing.session.active().is_some() {
            self.processing_load_player(None, false);
        }
        self.processing_refresh_view();
        // Nothing works without the engine: its window, with the hardware
        // warning, comes first. Once per session, so closing it is respected.
        if processing::platform_supported()
            && self.processing.installed.runtime.is_none()
            && self.processing.engine_task.is_none()
            && !self.processing.engine_prompted
        {
            self.processing.engine_prompted = true;
            self.processing_open_modal(ProcessingModal::Engine);
        }
    }

    /// Opens a window of the tab with up-to-date content and reads it.
    fn processing_open_modal(&mut self, modal: ProcessingModal) {
        self.processing_refresh_view();
        self.ui_shell.ui.open_processing_modal(modal);
        if let Some(label) = self.ui_shell.ui.processing_modal_announcement() {
            self.processing_say(label);
        }
    }

    /// Speaks what just happened, for the actions that show no message.
    fn processing_say(&self, label: impl Into<String>) {
        self.announce_accessibility(AccessibilityEvent::Activation {
            label: label.into(),
        });
    }

    /// Speaks the progress of long work: each quarter of a model pass, each
    /// step of the engine installation.
    fn processing_announce_progress(&mut self) {
        let quarter = self.processing.job.as_ref().map(|job| {
            let progress = job.progress();
            let quarter = if progress.stage == Stage::Separating {
                ((progress.fraction * 4.0).floor() as u8).min(3)
            } else {
                0
            };
            (quarter, t(job.model.purpose_key))
        });
        match quarter {
            Some((quarter, purpose)) if quarter > self.processing.announced_quarter => {
                self.processing.announced_quarter = quarter;
                self.announce_accessibility(AccessibilityEvent::ValueChanged {
                    label: purpose.to_string(),
                    value: format!("{} %", u32::from(quarter) * 25),
                });
            }
            Some(_) => {}
            None => self.processing.announced_quarter = 0,
        }

        let step = self
            .processing
            .engine_task
            .as_ref()
            .filter(|task| task.kind == TaskKind::InstallRuntime)
            .map(|task| task.snapshot().step);
        if step != self.processing.announced_step {
            self.processing.announced_step = step;
            if let Some(step) = step.filter(|step| *step != Step::Starting) {
                self.processing_say(t(step.label_key()));
            }
        }
    }

    pub(crate) fn processing_total_frames(&self) -> i64 {
        self.processing_player
            .as_ref()
            .map_or(0, VideoPlayer::total_frames)
    }

    pub fn processing_action(&mut self, action: ProcessingAction) {
        match action {
            // The actions that open a file dialog go through the dispatcher,
            // which owns the dialogs.
            ProcessingAction::ImportAudio
            | ProcessingAction::ExportTrack(_)
            | ProcessingAction::ExportAll
            | ProcessingAction::ExportListened
            | ProcessingAction::AddTrackToProject(_)
            | ProcessingAction::AddListenedToProject
            | ProcessingAction::ChooseDirectory => {}
            ProcessingAction::OpenCatalog => self.processing_open_modal(ProcessingModal::Catalog),
            ProcessingAction::OpenEngine => self.processing_open_modal(ProcessingModal::Engine),
            ProcessingAction::ListenRelative(step) => {
                match self
                    .processing_listening_index()
                    .and_then(|index| index.checked_add_signed(step as isize))
                {
                    Some(index) if self.processing_node_at(index).is_some() => {
                        self.processing_listen(index, None)
                    }
                    Some(_) => {}
                    None if self.processing.session.active().is_none() => {
                        self.show_toast(t("processing.toast.no_audio"), 4.0)
                    }
                    None => {}
                }
            }
            ProcessingAction::SelectModelRelative(step) => {
                match next_installed(
                    &self.processing.installed.models,
                    self.processing.selected_model,
                    step,
                ) {
                    Some(index) => {
                        self.processing.selected_model = index;
                        self.announce_accessibility(AccessibilityEvent::Selection {
                            label: t(CATALOG[index].purpose_key).to_string(),
                        });
                    }
                    None => self.show_toast(t("processing.toast.model_missing_any"), 4.0),
                }
            }
            ProcessingAction::SendProjectVideo(id) => {
                let path = self
                    .project_session
                    .project
                    .media_library()
                    .videos
                    .iter()
                    .find(|video| video.id == id)
                    .map(|video| PathBuf::from(&video.path));
                self.processing_send_project_media(path);
            }
            ProcessingAction::SendProjectAudio(id) => {
                let path = self
                    .project_session
                    .project
                    .media_library()
                    .audios
                    .iter()
                    .find(|audio| audio.id == id)
                    .map(|audio| PathBuf::from(&audio.path));
                self.processing_send_project_media(path);
            }
            ProcessingAction::UseProjectVideo => match self.video_path() {
                Some(path) => self.processing_begin_import(path),
                None => self.show_toast(t("processing.toast.no_video"), 4.0),
            },
            ProcessingAction::SelectItem(id) => {
                if self.processing.session.select(id) {
                    self.processing.listening = AudioItem::ROOT;
                    self.processing_load_player(None, false);
                }
                if let Some(name) = self
                    .processing
                    .session
                    .item(id)
                    .map(|item| item.name.clone())
                {
                    self.announce_accessibility(AccessibilityEvent::Selection { label: name });
                }
            }
            ProcessingAction::RemoveItem(id) => self.processing_remove_item(id),
            ProcessingAction::InstallRuntime => self.processing_install_runtime(),
            ProcessingAction::CancelEngine => {
                if let Some(task) = &self.processing.engine_task {
                    if task.kind == TaskKind::InstallRuntime {
                        task.cancel();
                    }
                }
            }
            ProcessingAction::UninstallRuntime => self.processing_uninstall(false),
            ProcessingAction::UninstallAll => self.processing_uninstall(true),
            ProcessingAction::DownloadModel(index) => self.processing_download_model(index),
            ProcessingAction::CancelDownload(index) => {
                if let Some(task) = self.processing.model_task(index) {
                    task.cancel();
                }
            }
            ProcessingAction::RemoveModel(index) => self.processing_remove_model(index),
            ProcessingAction::OpenFolder => {
                let Some(location) = &self.processing.location else {
                    self.show_toast(t("processing.toast.no_directory"), 4.0);
                    return;
                };
                if let Err(error) = open::that(location.root()) {
                    self.show_toast(error.to_string(), 5.0);
                }
            }
            ProcessingAction::SelectModel(index) => {
                if self
                    .processing
                    .installed
                    .models
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                {
                    self.processing.selected_model = index;
                    self.announce_accessibility(AccessibilityEvent::Selection {
                        label: t(CATALOG[index].purpose_key).to_string(),
                    });
                }
            }
            ProcessingAction::ToggleQuality => {
                self.processing.best_quality = !self.processing.best_quality;
                self.processing_say(t(if self.processing.best_quality {
                    "processing.quality.best"
                } else {
                    "processing.quality.fast"
                }));
            }
            ProcessingAction::Run => self.processing_run(),
            ProcessingAction::Cancel => {
                if let Some(job) = &self.processing.job {
                    job.cancel();
                }
            }
            ProcessingAction::Listen { track, at_ms } => self.processing_listen(track, at_ms),
            ProcessingAction::RemoveTrack(track) => self.processing_remove_track(track),
        }
        self.processing_refresh_view();
    }

    /// A video or an audio of the project goes to the tab. Only its sound is
    /// read: the project file is never modified.
    fn processing_send_project_media(&mut self, path: Option<PathBuf>) {
        match path.filter(|path| path.is_file()) {
            Some(path) => self.processing_begin_import(path),
            None => self.show_toast(t("processing.toast.media_missing"), 5.0),
        }
    }

    pub fn processing_import_request(&self) -> (String, Vec<FileFilterSpec>) {
        (
            t("processing.picker.import").to_string(),
            vec![
                FileFilterSpec::new(t("processing.picker.media"), SOURCE_EXTENSIONS),
                FileFilterSpec::new(t("picker.filter.all_files"), &["*"]),
            ],
        )
    }

    /// The folder dialog of the engine window. `None` while something is
    /// installed: the folder cannot change under it.
    pub fn processing_directory_request(&mut self) -> Option<FilePickerRequest> {
        if self.processing.installed.holds_anything || self.processing.is_busy() {
            self.show_toast(t("processing.engine.directory_locked"), 5.0);
            return None;
        }
        Some(FilePickerRequest {
            title: t("processing.picker.directory").to_string(),
            mode: FilePickerMode::Folder,
            intent: FilePickerIntent::ProcessingDirectory,
            filters: Vec::new(),
            initial_dir: self
                .processing
                .location
                .as_ref()
                .and_then(|location| location.root().parent().map(PathBuf::from)),
            default_extension: None,
            initial_filename: None,
        })
    }

    /// The user designated a folder: everything goes in its
    /// `coquerythmo-processing` subfolder, if it can be written.
    pub fn processing_set_directory(&mut self, folder: PathBuf) {
        let location = InstallLocation::from_chosen(&folder);
        if !location.is_writable() {
            self.show_toast(t("processing.toast.directory_refused"), 7.0);
            return;
        }
        if let Some(previous) = self
            .processing
            .location
            .replace(location.clone())
            .filter(|previous| *previous != location)
        {
            // The check created the previous subfolder; it stays only if the
            // user already keeps something in it.
            let _ = std::fs::remove_dir(previous.root());
        }
        crate::config::set_processing_directory(Some(location.root().to_path_buf()));
        self.processing_say(format!(
            "{} : {}",
            t("processing.engine.directory"),
            location.root().display()
        ));
        self.processing.refresh_installed();
        self.processing_refresh_view();
    }

    /// The location to install into. `seen_by_user`: the request comes from
    /// the engine window, where the folder is displayed, and so confirms it.
    /// A folder that is only a proposal is never written to before that: the
    /// engine window opens to ask.
    fn processing_confirmed_location(&mut self, seen_by_user: bool) -> Option<InstallLocation> {
        let location = self.processing.location.clone().filter(|location| {
            seen_by_user || location_is_confirmed(location, crate::config::processing_directory())
        });
        let Some(location) = location else {
            self.show_toast(t("processing.toast.no_directory"), 5.0);
            self.processing_open_modal(ProcessingModal::Engine);
            return None;
        };
        crate::config::set_processing_directory(Some(location.root().to_path_buf()));
        Some(location)
    }

    pub fn processing_begin_import(&mut self, source: PathBuf) {
        if !source.is_file() || !Self::processing_is_media_file(&source) {
            self.show_toast(t("processing.toast.unsupported_file"), 4.0);
            return;
        }
        let id = self.processing.session.allocate_id();
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".into());
        let directory = self.processing.session_dir.join(format!("audio-{id}"));
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker_directory = directory.clone();
        std::thread::spawn(move || {
            let _ = sender.send(job::prepare_source(&source, &worker_directory));
        });
        self.processing.imports.push(PendingImport {
            id,
            name: name.clone(),
            directory,
            receiver,
        });
        self.show_toast(format!("{} {name}…", t("processing.toast.importing")), 2.5);
        self.processing_refresh_view();
    }

    pub fn processing_is_media_file(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                SOURCE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            })
    }

    fn processing_remove_item(&mut self, id: u64) {
        if self
            .processing
            .job
            .as_ref()
            .is_some_and(|job| job.item_id == id)
        {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        let was_active = self.processing.session.active_id() == Some(id);
        let Some(item) = self.processing.session.remove(id) else {
            return;
        };
        if was_active {
            self.processing.listening = AudioItem::ROOT;
            self.processing_player = None;
            self.ui_shell.ui.set_playing(false);
            if self.processing.session.active().is_some() {
                self.processing_load_player(None, false);
            } else {
                self.ui_shell.ui.total_frames = 0;
            }
        }
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(item.directory);
        });
    }

    /// Display index of the track being listened to.
    pub fn processing_listening_index(&self) -> Option<usize> {
        let item = self.processing.session.active()?;
        item.ordered()
            .iter()
            .position(|node| *node == self.processing.listening)
            .or(Some(0))
    }

    /// The listened track when a model produced it: the original audio is
    /// already a file of the user.
    pub fn processing_listened_result_index(&self) -> Option<usize> {
        self.processing_listening_index().filter(|index| *index > 0)
    }

    /// Said when a shortcut has no track to export or to add to the project.
    pub fn processing_nothing_to_export(&mut self) {
        let message = if self.processing.session.active().is_none() {
            t("processing.toast.no_audio")
        } else {
            t("processing.toast.no_result")
        };
        self.show_toast(message, 4.0);
    }

    /// The track shown at `index` in the active audio.
    fn processing_node_at(&self, index: usize) -> Option<(u64, u64)> {
        let item = self.processing.session.active()?;
        Some((item.id, item.ordered().get(index).copied()?))
    }

    /// Removes one produced track, and the tracks that descend from it.
    fn processing_remove_track(&mut self, index: usize) {
        let Some((item_id, node)) = self.processing_node_at(index) else {
            return;
        };
        if node == AudioItem::ROOT {
            return;
        }
        let Some(item) = self.processing.session.item_mut(item_id) else {
            return;
        };
        let doomed = item.subtree(node);
        // A track being processed, or an ancestor of it, cannot go.
        let in_use = self
            .processing
            .job
            .as_ref()
            .is_some_and(|job| job.item_id == item_id && doomed.contains(&job.parent));
        if in_use {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        let parent = item.node(node).and_then(|node| node.parent);
        let files = item.remove(node);
        if doomed.contains(&self.processing.listening) {
            // The listened track is gone: back to the one it came from.
            self.processing.listening = parent.unwrap_or(AudioItem::ROOT);
            self.processing_player = None;
            self.processing_load_player(None, false);
        }
        delete_track_files(files);
        self.show_toast(t("processing.toast.track_removed"), 3.0);
    }

    fn processing_install_runtime(&mut self) {
        if !processing::platform_supported() {
            self.show_toast(t("processing.engine.unsupported"), 5.0);
            return;
        }
        if self.processing.engine_task.is_some() {
            return;
        }
        let Some(location) = self.processing_confirmed_location(true) else {
            return;
        };
        // The choice of the PyTorch build depends on the card: wait for the
        // answer of the driver rather than guessing.
        if let GpuProbe::Pending(receiver) = &self.processing.gpu {
            let gpu = receiver.recv_timeout(Duration::from_secs(5)).ok().flatten();
            self.processing.gpu = GpuProbe::Known(gpu);
        }
        let variant = self.processing.torch_variant();
        self.processing.engine_task = Some(install::install_runtime(location, variant));
        self.processing_say(t("processing.announce.install"));
    }

    fn processing_download_model(&mut self, index: usize) {
        let Some(model) = CATALOG.get(index) else {
            return;
        };
        if self.processing.model_task(index).is_some()
            || self
                .processing
                .installed
                .models
                .get(index)
                .copied()
                .unwrap_or(false)
        {
            return;
        }
        if self
            .processing
            .engine_task
            .as_ref()
            .is_some_and(|task| task.kind == TaskKind::Remove)
        {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        let Some(location) = self.processing_confirmed_location(false) else {
            return;
        };
        self.processing
            .model_tasks
            .push((index, install::download_model(location, model)));
        self.processing_say(format!(
            "{} : {}",
            t("processing.announce.download"),
            t(model.purpose_key)
        ));
    }

    fn processing_remove_model(&mut self, index: usize) {
        let (Some(model), Some(location)) = (CATALOG.get(index), self.processing.location.clone())
        else {
            return;
        };
        let in_use = self
            .processing
            .job
            .as_ref()
            .is_some_and(|job| job.model.id == model.id);
        if in_use || self.processing.model_task(index).is_some() {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        self.processing.model_tasks.push((
            index,
            install::remove_paths(install::model_paths(&location, model), None),
        ));
    }

    /// Removes the engine, or everything the tab installed (`everything`).
    fn processing_uninstall(&mut self, everything: bool) {
        let Some(location) = self.processing.location.clone() else {
            return;
        };
        let busy = self.processing.engine_task.is_some()
            || self.processing.job.is_some()
            || (everything && !self.processing.model_tasks.is_empty());
        if busy {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        self.processing.engine_task = Some(if everything {
            install::remove_paths(
                location.owned_entries(),
                Some(location.root().to_path_buf()),
            )
        } else {
            install::remove_paths(location.runtime_entries(), None)
        });
    }

    fn processing_run(&mut self) {
        if self.processing.job.is_some() {
            return;
        }
        let Some(model) = CATALOG.get(self.processing.selected_model) else {
            return;
        };
        let Some(location) = self
            .processing
            .location
            .clone()
            .filter(|_| self.processing.installed.runtime.is_some())
        else {
            self.show_toast(t("processing.toast.engine_missing"), 5.0);
            self.processing_open_modal(ProcessingModal::Engine);
            return;
        };
        if !location.has_model(model) {
            self.show_toast(t("processing.toast.model_missing"), 5.0);
            self.processing_open_modal(ProcessingModal::Catalog);
            return;
        }
        if self.processing.engine_task.is_some() {
            self.show_toast(t("processing.toast.busy"), 4.0);
            return;
        }
        let Some(item) = self.processing.session.active() else {
            self.show_toast(t("processing.toast.no_audio"), 4.0);
            return;
        };
        // The model is applied to the listened track, original or already
        // processed: its results will descend from it.
        let target = item
            .node(self.processing.listening)
            .unwrap_or_else(|| item.root());
        let title = self.processing_track_title(item, target.id);
        self.processing.job = Some(job::start(
            location,
            item.id,
            target.id,
            model,
            target.track.path.clone(),
            item.directory.clone(),
            self.processing.best_quality,
        ));
        self.processing_say(format!(
            "{} : {}, {title}",
            t("processing.announce.run"),
            t(model.purpose_key)
        ));
    }

    fn processing_listen(&mut self, track: usize, at_ms: Option<u64>) {
        let Some(item) = self.processing.session.active() else {
            return;
        };
        let Some(target) = item.ordered().get(track).copied() else {
            return;
        };
        if target != self.processing.listening || self.processing_player.is_none() {
            self.processing.listening = target;
            let playing = self
                .processing_player
                .as_ref()
                .is_some_and(VideoPlayer::is_playing);
            let frame = at_ms.map(|ms| (ms / 10) as i64);
            self.processing_load_player(frame, playing && at_ms.is_none());
            if let Some(item) = self.processing.session.active() {
                self.announce_accessibility(AccessibilityEvent::Selection {
                    label: format!(
                        "{} : {}",
                        t("processing.track.listening"),
                        self.processing_track_title(item, target)
                    ),
                });
            }
        } else if let Some(ms) = at_ms {
            self.seek_absolute((ms / 10) as i64);
        }
    }

    /// Loads the listened track into the player. `frame`: where to stand
    /// (`None` keeps the current position); `resume`: keep playing.
    fn processing_load_player(&mut self, frame: Option<i64>, resume: bool) {
        let previous_frame = self
            .processing_player
            .as_ref()
            .map_or(0, VideoPlayer::current_frame);
        let Some(item) = self.processing.session.active() else {
            self.processing_player = None;
            return;
        };
        let track = match item.node(self.processing.listening) {
            Some(node) => &node.track,
            None => {
                self.processing.listening = AudioItem::ROOT;
                &item.root().track
            }
        };
        let path = track.path.clone();
        let duration_ms = track.duration_ms.max(10);
        let mut player = VideoPlayer::new();
        if let Err(error) = player.load_audio_only(&path, duration_ms as f64 / 1_000.0) {
            self.processing_player = None;
            self.show_toast(error, 5.0);
            return;
        }
        player.set_volume(self.ui_shell.ui.volume());
        let target = frame
            .unwrap_or(previous_frame)
            .clamp(0, player.total_frames().saturating_sub(1).max(0));
        if target > 0 {
            player.seek_to_frame_instant(target);
        }
        if resume {
            player.toggle();
        }
        let playing = player.is_playing();
        self.processing_player = Some(player);
        if self.active_workspace() == WorkspaceId::Processing {
            self.ui_shell.ui.total_frames = (duration_ms / 10) as i64;
            self.ui_shell.ui.set_playing(playing);
        }
    }

    pub fn processing_export_track_request(&self, track: usize) -> Option<FilePickerRequest> {
        let item = self.processing.session.active()?;
        let target = item.ordered().get(track).copied()?;
        let label = self.processing_track_title(item, target);
        Some(FilePickerRequest {
            title: t("processing.picker.export").to_string(),
            mode: FilePickerMode::Save,
            intent: FilePickerIntent::ProcessingExportTrack {
                item_id: item.id,
                track,
            },
            filters: vec![
                FileFilterSpec::new("Audio", EXPORT_EXTENSIONS),
                FileFilterSpec::new(t("picker.filter.all_files"), &["*"]),
            ],
            initial_dir: None,
            default_extension: Some("wav".into()),
            initial_filename: Some(format!(
                "{}.wav",
                processing::export_file_stem(&item.name, &label)
            )),
        })
    }

    /// Where to write a produced track that joins the audios of the project.
    pub fn processing_add_to_project_request(&self, track: usize) -> Option<FilePickerRequest> {
        let mut request = self.processing_export_track_request(track)?;
        let FilePickerIntent::ProcessingExportTrack { item_id, track } = request.intent else {
            return None;
        };
        let item = self.processing.session.item(item_id)?;
        if matches!(item.ordered().get(track), None | Some(&AudioItem::ROOT)) {
            return None;
        }
        request.title = t("processing.picker.add_to_project").to_string();
        request.intent = FilePickerIntent::ProcessingAddToProject { item_id, track };
        Some(request)
    }

    pub fn processing_export_all_request(&self) -> Option<FilePickerRequest> {
        let item = self.processing.session.active()?;
        if !item.has_results() {
            return None;
        }
        Some(FilePickerRequest {
            title: t("processing.picker.export_all").to_string(),
            mode: FilePickerMode::Folder,
            intent: FilePickerIntent::ProcessingExportAll { item_id: item.id },
            filters: Vec::new(),
            initial_dir: None,
            default_extension: None,
            initial_filename: None,
        })
    }

    /// Adds a produced track to the audios of the project in one step: the
    /// file is written in `folder` under a name that replaces nothing.
    pub fn processing_add_track_to_project_in(&mut self, track: usize, folder: PathBuf) {
        let Some(item) = self.processing.session.active() else {
            return;
        };
        let Some(target) = item
            .ordered()
            .get(track)
            .copied()
            .filter(|target| *target != AudioItem::ROOT)
        else {
            return;
        };
        let label = self.processing_track_title(item, target);
        let stem = processing::export_file_stem(&item.name, &label);
        let destination = free_path(&folder, &stem, "flac");
        self.processing_export(item.id, track, destination, true);
    }

    /// Writes the track to `destination`, then lists that file among the
    /// audios of the project: a new audio, the source is left as it is.
    pub fn processing_add_track_to_project(
        &mut self,
        item_id: u64,
        track: usize,
        destination: PathBuf,
    ) {
        self.processing_export(item_id, track, destination, true);
    }

    pub fn processing_export_track_to(&mut self, item_id: u64, track: usize, destination: PathBuf) {
        self.processing_export(item_id, track, destination, false);
    }

    fn processing_export(
        &mut self,
        item_id: u64,
        track: usize,
        destination: PathBuf,
        add_to_project: bool,
    ) {
        let Some(item) = self.processing.session.item(item_id) else {
            return;
        };
        let Some(source) = item
            .ordered()
            .get(track)
            .and_then(|target| item.node(*target))
            .map(|node| node.track.path.clone())
        else {
            return;
        };
        let destination = if destination.extension().is_none() {
            destination.with_extension("wav")
        } else {
            destination
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let written = destination.clone();
        std::thread::spawn(move || {
            let result = job::export_track(&source, &destination).map(|()| {
                destination
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            let _ = sender.send(result);
        });
        self.processing.exports.push(PendingExport {
            receiver,
            add_to_project: add_to_project.then_some(written),
        });
    }

    /// Writes every produced track of an audio into `directory`, as WAV.
    pub fn processing_export_all_to(&mut self, item_id: u64, directory: PathBuf) {
        let Some(item) = self.processing.session.item(item_id) else {
            return;
        };
        // Two tracks may share a name (the same model on two parents): each
        // file gets its own.
        let mut used = std::collections::HashSet::new();
        let jobs: Vec<(PathBuf, PathBuf)> = item
            .ordered()
            .into_iter()
            .filter_map(|target| {
                let node = item.node(target)?;
                let model = node.model?.id;
                let label = self.processing_track_title(item, target);
                let stem = processing::export_file_stem(&item.name, &format!("{label} ({model})"));
                let mut name = stem.clone();
                let mut number = 2;
                while !used.insert(name.clone()) {
                    name = format!("{stem} ({number})");
                    number += 1;
                }
                Some((
                    node.track.path.clone(),
                    directory.join(format!("{name}.wav")),
                ))
            })
            .collect();
        let folder = directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| directory.display().to_string());
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = jobs
                .iter()
                .try_for_each(|(source, destination)| job::export_track(source, destination))
                .map(|()| folder);
            let _ = sender.send(result);
        });
        self.processing.exports.push(PendingExport {
            receiver,
            add_to_project: None,
        });
    }

    /// Name of a track: « Audio d'origine », or what the model called it.
    fn processing_track_title(&self, item: &AudioItem, target: u64) -> String {
        match item.node(target) {
            Some(node) if node.id != AudioItem::ROOT => match node.label_key {
                Some(key) => t(key).to_string(),
                None => node.name.clone(),
            },
            _ => t("processing.track.original").to_string(),
        }
    }

    /// Advances every background piece of the tab. Returns true when
    /// something on screen changed.
    pub(crate) fn poll_processing(&mut self) -> bool {
        let mut changed = false;

        if let GpuProbe::Pending(receiver) = &self.processing.gpu {
            match receiver.try_recv() {
                Ok(gpu) => {
                    self.processing.gpu = GpuProbe::Known(gpu);
                    changed = true;
                }
                Err(TryRecvError::Disconnected) => {
                    self.processing.gpu = GpuProbe::Known(None);
                    changed = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if let Some(receiver) = &self.processing.disk_probe {
            match receiver.try_recv() {
                Ok(bytes) => {
                    self.processing.installed.disk_bytes = bytes;
                    self.processing.disk_probe = None;
                    changed = true;
                }
                Err(TryRecvError::Disconnected) => self.processing.disk_probe = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        let mut index = self.processing.imports.len();
        while index > 0 {
            index -= 1;
            let result = match self.processing.imports[index].receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Disconnected) => Err("import interrupted".into()),
                Err(TryRecvError::Empty) => continue,
            };
            let pending = self.processing.imports.swap_remove(index);
            match result {
                Ok(source) => {
                    self.processing.session.add(AudioItem::new(
                        pending.id,
                        pending.name,
                        pending.directory,
                        source,
                    ));
                    self.processing.listening = AudioItem::ROOT;
                    self.processing_player = None;
                    if self.active_workspace() == WorkspaceId::Processing {
                        self.processing_load_player(Some(0), false);
                    }
                    self.show_toast(t("processing.toast.imported"), 2.5);
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&pending.directory);
                    log::error!("processing import failed: {error}");
                    self.show_toast(
                        format!(
                            "{} {}",
                            t("processing.toast.import_failed"),
                            first_line(&error)
                        ),
                        7.0,
                    );
                }
            }
            changed = true;
        }

        let mut index = self.processing.exports.len();
        while index > 0 {
            index -= 1;
            let result = match self.processing.exports[index].receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Disconnected) => Err("export interrupted".into()),
                Err(TryRecvError::Empty) => continue,
            };
            let pending = self.processing.exports.swap_remove(index);
            match result {
                Ok(name) => match pending.add_to_project {
                    Some(path) => {
                        // Same entry point as « Ajouter un audio » in the
                        // project files.
                        self.import_audio(path.to_string_lossy().into_owned());
                        self.show_toast(
                            format!("{} {name}", t("processing.toast.added_to_project")),
                            5.0,
                        );
                    }
                    None => {
                        self.show_toast(format!("{} {name}", t("processing.toast.exported")), 4.0)
                    }
                },
                Err(error) => self.show_toast(
                    format!(
                        "{} {}",
                        t("processing.toast.export_failed"),
                        first_line(&error)
                    ),
                    7.0,
                ),
            }
            changed = true;
        }

        self.processing_announce_progress();
        changed |= self.poll_processing_tasks();
        changed |= self.poll_processing_job();

        let visible = self.active_workspace() == WorkspaceId::Processing;
        let animating = self.processing.is_busy() || self.ui_shell.ui.processing_needs_redraw();
        if changed || (visible && animating) {
            self.processing_refresh_view();
        }
        changed || (visible && animating)
    }

    fn poll_processing_tasks(&mut self) -> bool {
        let mut finished = Vec::new();
        if let Some(task) = &self.processing.engine_task {
            let snapshot = task.snapshot();
            if snapshot.outcome != Outcome::Running {
                finished.push((task.kind, snapshot.outcome));
                self.processing.engine_task = None;
            }
        }
        let mut index = self.processing.model_tasks.len();
        while index > 0 {
            index -= 1;
            let snapshot = self.processing.model_tasks[index].1.snapshot();
            if snapshot.outcome != Outcome::Running {
                let (_, task) = self.processing.model_tasks.swap_remove(index);
                finished.push((task.kind, snapshot.outcome));
            }
        }
        if finished.is_empty() {
            return false;
        }
        self.processing.refresh_installed();
        for (kind, outcome) in finished {
            match outcome {
                Outcome::Done => {
                    let message = match kind {
                        TaskKind::InstallRuntime => t("processing.toast.engine_installed"),
                        TaskKind::DownloadModel(_) => t("processing.toast.model_installed"),
                        TaskKind::Remove => t("processing.toast.removed"),
                    };
                    self.show_toast(message, 5.0);
                }
                Outcome::Cancelled => self.show_toast(t("processing.toast.cancelled"), 4.0),
                Outcome::Failed(error) => {
                    log::error!("processing task failed: {error}");
                    let message = if error.ends_with("write access denied") {
                        t("processing.toast.directory_refused").to_string()
                    } else {
                        format!(
                            "{} {}",
                            t("processing.toast.task_failed"),
                            first_line(&error)
                        )
                    };
                    self.show_toast(message, 10.0);
                }
                Outcome::Running => {}
            }
        }
        true
    }

    fn poll_processing_job(&mut self) -> bool {
        let Some(job) = &self.processing.job else {
            return false;
        };
        let Some(end) = job.take_end() else {
            return false;
        };
        let item_id = job.item_id;
        let parent = job.parent;
        let speed = job.progress().speed;
        self.processing.job = None;
        match end {
            JobEnd::Done(group) => {
                let Some(item) = self.processing.session.item_mut(item_id) else {
                    let _ = std::fs::remove_dir_all(&group.directory);
                    return true;
                };
                let (added, replaced) = item.add_results(parent, group);
                delete_track_files(replaced);
                let Some(first) = added.first().copied() else {
                    // The processed track was removed meanwhile.
                    return true;
                };
                if self.processing.session.active_id() == Some(item_id) {
                    // Listen to the first produced track right away: it is
                    // what the user was waiting for.
                    self.processing.listening = first;
                    self.processing_player = None;
                    if self.active_workspace() == WorkspaceId::Processing {
                        self.processing_load_player(Some(0), false);
                    }
                }
                let message = match speed {
                    Some(speed) => format!(
                        "{} (×{speed:.0} {})",
                        t("processing.toast.done"),
                        t("processing.realtime")
                    ),
                    None => t("processing.toast.done").to_string(),
                };
                self.show_toast(message, 5.0);
            }
            JobEnd::Cancelled => self.show_toast(t("processing.toast.cancelled"), 4.0),
            JobEnd::Failed(error) => {
                log::error!("processing job failed: {error}");
                self.show_toast(
                    format!(
                        "{} {}",
                        t("processing.toast.job_failed"),
                        first_line(&error)
                    ),
                    10.0,
                );
            }
        }
        true
    }

    pub(crate) fn processing_refresh_view(&mut self) {
        let runtime = &self.processing;
        let variant = runtime.torch_variant();
        let gpu = match &runtime.gpu {
            GpuProbe::Known(Some(gpu)) => {
                let memory = format!(
                    "{}, {} {}",
                    gpu.name,
                    (gpu.vram_mb as f32 / 1_024.0).round(),
                    t("processing.unit.gb")
                );
                if gpu.is_recommended() {
                    GpuLine {
                        text: memory,
                        warning: false,
                    }
                } else {
                    GpuLine {
                        text: format!("{memory}, {}", t("processing.gpu.weak")),
                        warning: true,
                    }
                }
            }
            GpuProbe::Known(None) => GpuLine {
                text: t("processing.gpu.none").to_string(),
                warning: true,
            },
            _ => GpuLine::default(),
        };

        let models = CATALOG
            .iter()
            .enumerate()
            .map(|(index, model)| {
                let state = match runtime
                    .model_task(index)
                    .map(|task| (task.kind, task.snapshot()))
                {
                    Some((TaskKind::Remove, _)) => ModelState::Removing,
                    Some((_, snapshot)) => {
                        let progress = task_progress(&snapshot);
                        let detail = match progress.fraction {
                            Some(fraction) => {
                                format!("{:.0} %, {}", fraction * 100.0, progress.detail)
                            }
                            None => progress.title,
                        };
                        ModelState::Downloading {
                            fraction: progress.fraction,
                            detail,
                        }
                    }
                    None if runtime
                        .installed
                        .models
                        .get(index)
                        .copied()
                        .unwrap_or(false) =>
                    {
                        ModelState::Installed
                    }
                    None => ModelState::Available,
                };
                ModelView {
                    title: model.title.to_string(),
                    purpose: t(model.purpose_key).to_string(),
                    size: format_bytes(model.checkpoint_bytes),
                    outputs: model
                        .stems
                        .iter()
                        .map(|stem| t(stem.label_key))
                        .collect::<Vec<_>>()
                        .join(", "),
                    state,
                }
            })
            .collect();

        let engine_task = runtime
            .engine_task
            .as_ref()
            .map(|task| (task.kind, task.snapshot()));
        let engine = EngineView {
            supported: processing::platform_supported(),
            installed: runtime.installed.runtime.is_some(),
            uses_gpu: runtime
                .installed
                .runtime
                .is_some_and(TorchVariant::uses_gpu)
                && runtime.gpu().is_some(),
            download_size: format!("≈ {}", format_bytes(variant.download_bytes())),
            gpu,
            directory: runtime
                .location
                .as_ref()
                .map(|location| location.root().display().to_string())
                .unwrap_or_default(),
            directory_locked: runtime.installed.holds_anything || runtime.is_busy(),
            disk_usage: format!(
                "{} {}",
                t("processing.disk_usage"),
                format_bytes(runtime.installed.disk_bytes)
            ),
            installing: matches!(engine_task, Some((TaskKind::InstallRuntime, _))),
            task: engine_task
                .as_ref()
                .map(|(_, snapshot)| task_progress(snapshot)),
        };

        let job = runtime.job.as_ref().map(|job| {
            let progress = job.progress();
            let purpose = t(job.model.purpose_key);
            let title = match progress.stage {
                Stage::Loading => t("processing.job.loading").to_string(),
                Stage::Separating => {
                    format!("{purpose} {:.0} %", progress.fraction * 100.0)
                }
                Stage::Finishing => t("processing.job.finishing").to_string(),
            };
            let mut parts = Vec::new();
            if let Some(speed) = progress.speed {
                parts.push(format!("×{speed:.1} {}", t("processing.realtime")));
            }
            if let Some(remaining) = progress.remaining_seconds {
                parts.push(format!(
                    "{} {}",
                    t("processing.job.remaining"),
                    clock(remaining)
                ));
            }
            match &progress.device {
                Some(device) if device.cuda => {
                    parts.push(device.gpu.clone().unwrap_or_else(|| "GPU".into()))
                }
                Some(_) => parts.push(t("processing.job.cpu").to_string()),
                None => {}
            }
            if progress.reduced {
                parts.push(t("processing.job.reduced").to_string());
            }
            ProgressView {
                title,
                detail: parts.join(", "),
                fraction: matches!(progress.stage, Stage::Separating | Stage::Finishing)
                    .then_some(progress.fraction),
            }
        });

        let mut items: Vec<ItemView> = runtime
            .session
            .items()
            .iter()
            .map(|item| ItemView {
                id: item.id,
                name: item.name.clone(),
                duration_ms: item.root().track.duration_ms,
            })
            .collect();
        items.extend(runtime.imports.iter().map(|pending| ItemView {
            id: pending.id,
            name: format!("{} {}…", t("processing.toast.importing"), pending.name),
            duration_ms: 0,
        }));

        let mut tracks = Vec::new();
        let mut listening = 0;
        if let Some(item) = runtime.session.active() {
            for (index, target) in item.ordered().into_iter().enumerate() {
                let Some(node) = item.node(target) else {
                    continue;
                };
                if target == runtime.listening {
                    listening = index;
                }
                // A produced track says which treatment made it and which
                // track it comes from.
                let subtitle = match (node.model, node.parent) {
                    (Some(model), Some(parent)) => format!(
                        "{}, {} {}",
                        t(model.purpose_key),
                        t("processing.track.from"),
                        self.processing_track_title(item, parent)
                    ),
                    _ => item.name.clone(),
                };
                tracks.push(TrackView {
                    title: self.processing_track_title(item, target),
                    subtitle,
                    waveform: node.track.waveform.clone(),
                    waveform_sample_rate: node.track.waveform_sample_rate,
                    duration_ms: node.track.duration_ms,
                    depth: item.depth(target),
                });
            }
        }

        let view = ProcessingView {
            engine,
            models,
            selected_model: runtime.selected_model,
            best_quality: runtime.best_quality,
            job,
            items,
            active_item: runtime.session.active_id(),
            tracks,
            listening,
            has_project_video: self.video_path().is_some(),
        };
        self.ui_shell.ui.set_processing_view(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_toasts_show_the_last_meaningful_line() {
        assert_eq!(
            first_line("Traceback\n  File x\nRuntimeError: CUDA out of memory\n\n"),
            "RuntimeError: CUDA out of memory"
        );
        assert_eq!(first_line("single"), "single");
    }

    #[test]
    fn remaining_time_reads_as_minutes_and_seconds() {
        assert_eq!(clock(0.4), "0:00");
        assert_eq!(clock(75.2), "1:15");
    }

    #[test]
    fn a_track_added_to_the_project_never_overwrites_a_file() {
        let folder =
            std::env::temp_dir().join(format!("coquerythmo-free-path-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let first = free_path(&folder, "film - Instrumental", "flac");
        assert_eq!(first, folder.join("film - Instrumental.flac"));
        std::fs::write(&first, b"").unwrap();
        let second = free_path(&folder, "film - Instrumental", "flac");
        assert_eq!(second, folder.join("film - Instrumental (2).flac"));
        std::fs::write(&second, b"").unwrap();
        assert_eq!(
            free_path(&folder, "film - Instrumental", "flac"),
            folder.join("film - Instrumental (3).flac")
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn model_shortcuts_cycle_through_the_installed_models_only() {
        let installed = [true, false, false, true, false];
        assert_eq!(next_installed(&installed, 0, 1), Some(3));
        assert_eq!(next_installed(&installed, 3, 1), Some(0));
        assert_eq!(next_installed(&installed, 0, -1), Some(3));
        // A single installed model stays selected.
        assert_eq!(next_installed(&[false, true], 1, 1), Some(1));
        assert_eq!(next_installed(&[false, false], 0, 1), None);
    }

    #[test]
    fn a_proposed_folder_is_not_a_chosen_one() {
        let proposed = InstallLocation::from_chosen(Path::new(r"D:\Coquerythmo"));
        assert!(!location_is_confirmed(&proposed, None));
        assert!(!location_is_confirmed(
            &proposed,
            Some(PathBuf::from(r"E:\IA\coquerythmo-processing"))
        ));
        assert!(location_is_confirmed(
            &proposed,
            Some(proposed.root().to_path_buf())
        ));
    }

    #[test]
    fn dropped_videos_and_audios_are_accepted_as_sources() {
        assert!(State::processing_is_media_file(Path::new("film.MKV")));
        assert!(State::processing_is_media_file(Path::new("take.flac")));
        assert!(!State::processing_is_media_file(Path::new("notes.txt")));
    }

    #[test]
    fn a_download_shows_bytes_and_a_library_step_shows_its_last_line() {
        let download = TaskSnapshot {
            step: Step::Model,
            bytes: Some((250_000_000, 1_000_000_000)),
            detail: String::new(),
            outcome: Outcome::Running,
        };
        let view = task_progress(&download);
        assert_eq!(view.fraction, Some(0.25));
        assert!(view.detail.contains('/'));

        let packages = TaskSnapshot {
            step: Step::Packages,
            bytes: None,
            detail: "Installing collected packages: torch".into(),
            outcome: Outcome::Running,
        };
        let view = task_progress(&packages);
        assert_eq!(view.fraction, None);
        assert_eq!(view.detail, "Installing collected packages: torch");
    }
}
