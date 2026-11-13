//! State-level integration of « Ajouter et conformer »: the window, the
//! background comparison of the two cuts and the move of the band.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

use crate::accessibility::AccessibilityEvent;
use crate::application::job_service::PendingConformJob;
use crate::conform::{self, ConformAnalysis, ConformReport};
use crate::i18n::t;
use crate::project::MediaId;
use crate::state::State;
use crate::ui::conform_modal::{ConformChoice, ConformModal};

impl State {
    fn conform_video_choices(&self) -> Vec<ConformChoice> {
        self.project_session
            .project
            .media_library()
            .videos
            .iter()
            .filter(|video| video.proxy_of.is_none())
            .map(|video| ConformChoice {
                id: video.id,
                name: video.name.clone(),
            })
            .collect()
    }

    fn conform_band_choices(&self) -> Vec<ConformChoice> {
        self.project_session
            .project
            .languages()
            .into_iter()
            .map(|language| ConformChoice {
                id: language.id,
                name: language.name,
            })
            .collect()
    }

    /// The library video whose file is loaded for playback, if any.
    fn loaded_media_video(&self) -> Option<MediaId> {
        let source = self.video_path()?;
        self.project_session
            .project
            .media_library()
            .videos
            .iter()
            .find(|video| {
                video.proxy_of.is_none() && crate::video_proxy::paths_match(Path::new(&video.path), &source)
            })
            .map(|video| video.id)
    }

    pub fn open_conform_modal(&mut self, video_id: Option<MediaId>) {
        if self.background_task_running() {
            self.show_toast(t("toast.action_blocked_task"), 5.0);
            return;
        }
        let videos = self.conform_video_choices();
        if videos.is_empty() {
            self.show_toast(t("conform.no_video"), 5.0);
            return;
        }
        let bands = self.conform_band_choices();
        let library = self.project_session.project.media_library();
        let default_video = video_id
            .filter(|id| videos.iter().any(|video| video.id == *id))
            .or_else(|| self.loaded_media_video())
            .or(library.default_video)
            .filter(|id| videos.iter().any(|video| video.id == *id));
        let band = self.project_session.project.active_language_id();
        let modal = ConformModal::new(videos, bands, default_video, Some(band), true);
        self.ui_shell.ui.open_conform_modal(modal);
        self.announce_accessibility(AccessibilityEvent::Opened {
            label: t("conform.title").to_string(),
        });
    }

    pub fn conform_select_video(&mut self, id: MediaId) {
        if self.ui_shell.ui.conform_modal().is_none() {
            self.open_conform_modal(Some(id));
            return;
        }
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            if !modal.select_video(id) {
                self.show_toast(t("toast.media_file_missing"), 4.0);
            }
        }
    }

    pub fn conform_select_band(&mut self, id: u64) {
        if self.ui_shell.ui.conform_modal().is_none() {
            self.open_conform_modal(None);
        }
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            modal.select_band(id);
        }
    }

    pub fn conform_set_move_takes(&mut self, move_takes: bool) {
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            modal.set_move_takes(move_takes);
        }
    }

    pub fn conform_modal_focus_label(&self) -> Option<String> {
        self.ui_shell
            .ui
            .conform_modal()
            .map(ConformModal::keyboard_focus_label)
    }

    pub fn is_conform_modal_open(&self) -> bool {
        self.ui_shell.ui.conform_modal().is_some()
    }

    pub fn close_conform_modal(&mut self) {
        if self.ui_shell.ui.close_conform_modal() {
            self.announce_accessibility(AccessibilityEvent::Closed {
                label: t("conform.title").to_string(),
            });
        }
    }

    /// Stops the comparison; the project is untouched until it finishes.
    pub fn cancel_conform(&mut self) {
        if let Some(job) = self.jobs.pending_conform_job.as_ref() {
            job.cancel.store(true, Ordering::Relaxed);
            self.set_progress_label(t("progress.canceling"));
        } else {
            self.close_conform_modal();
        }
    }

    /// Compares `video_id` with the cut at `new_path`, then conforms `band_id`.
    pub fn start_conform(
        &mut self,
        video_id: MediaId,
        band_id: u64,
        new_path: PathBuf,
        move_takes: bool,
    ) {
        if self.background_task_running() || self.jobs.pending_conform_job.is_some() {
            self.show_toast(t("toast.action_blocked_task"), 5.0);
            return;
        }
        let Some(video) = self
            .project_session
            .project
            .media_library()
            .videos
            .iter()
            .find(|video| video.id == video_id && video.proxy_of.is_none())
            .cloned()
        else {
            self.show_toast(t("toast.media_file_missing"), 5.0);
            return;
        };
        let old_path = PathBuf::from(&video.path);
        if !old_path.exists() {
            self.show_toast(t("toast.media_file_missing"), 5.0);
            return;
        }
        if !new_path.exists() {
            self.show_toast(t("toast.media_file_missing"), 5.0);
            return;
        }
        if crate::video_proxy::paths_match(&old_path, &new_path) {
            self.show_toast(t("toast.conform_same_video"), 5.0);
            return;
        }
        if self.project_session.project.language(band_id).is_none() {
            self.show_toast(t("conform.no_band"), 5.0);
            return;
        }
        // Takes follow the pictures once per new cut, whatever the band.
        let move_takes = move_takes
            && !self
                .last_conform_target
                .as_ref()
                .is_some_and(|previous| crate::video_proxy::paths_match(previous, &new_path));

        let progress = Arc::new(AtomicU32::new(0.01_f32.to_bits()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = std::sync::mpsc::channel();
        {
            let progress = progress.clone();
            let cancel = cancel.clone();
            let old_path = old_path.clone();
            let new_path = new_path.clone();
            std::thread::spawn(move || {
                let result = conform::analyze(&old_path, &new_path, &cancel, &|value| {
                    progress.store(value.max(0.01).to_bits(), Ordering::Relaxed);
                });
                let _ = sender.send(result);
                progress.store(2.0_f32.to_bits(), Ordering::Relaxed);
            });
        }

        if self.ui_shell.ui.conform_modal().is_none() {
            let videos = self.conform_video_choices();
            let bands = self.conform_band_choices();
            let modal = ConformModal::new(videos, bands, Some(video_id), Some(band_id), move_takes);
            self.ui_shell.ui.open_conform_modal(modal);
        }
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            modal.start(progress.clone());
            modal.refresh_progress();
        }
        self.set_progress_label(t("progress.conform"));
        self.set_export_progress(Some(progress));
        self.set_export_cancel(Some(cancel.clone()));
        self.jobs.pending_conform_job = Some(PendingConformJob {
            video_id,
            band_id,
            new_path,
            move_takes,
            cancel,
            receiver,
        });
        self.announce_accessibility(AccessibilityEvent::Opened {
            label: format!("{} {}", t("progress.conform"), t("progress.cancel_hint")),
        });
    }

    pub(crate) fn poll_conform_job(&mut self) -> bool {
        let mut changed = false;
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            changed |= modal.refresh_progress();
        }
        let result = match self.jobs.pending_conform_job.as_ref() {
            Some(job) => match job.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("conform job disconnected".into())),
            },
            None => None,
        };
        let Some(result) = result else {
            return changed;
        };
        let Some(job) = self.jobs.pending_conform_job.take() else {
            return changed;
        };
        self.set_export_progress(None);
        self.set_export_cancel(None);
        match result {
            Ok(analysis) => self.finish_conform(job, analysis),
            Err(error) if job.cancel.load(Ordering::Relaxed) || error == "cancelled" => {
                log::info!("Conform cancelled");
                if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
                    modal.back_to_setup();
                }
                self.show_toast(t("toast.conform_cancelled"), 4.0);
            }
            Err(error) => {
                log::error!("Conform failed: {error}");
                if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
                    modal.fail(error.clone());
                }
                self.show_toast(format!("{} {error}", t("conform.failed")), 8.0);
            }
        }
        true
    }

    fn finish_conform(&mut self, job: PendingConformJob, analysis: ConformAnalysis) {
        let PendingConformJob {
            video_id,
            band_id,
            new_path,
            move_takes,
            ..
        } = job;
        let Some(old_video) = self
            .project_session
            .project
            .media_library()
            .videos
            .iter()
            .find(|video| video.id == video_id)
            .cloned()
        else {
            self.show_toast(t("toast.media_file_missing"), 5.0);
            return;
        };
        let Some(band) = self.project_session.project.language(band_id) else {
            self.show_toast(t("conform.no_band"), 5.0);
            return;
        };
        let new_name = crate::ui::conform_modal::display_name(&new_path);

        // 1. A new band, copied from the chosen one, receives the conformed
        //    positions; the chosen band itself is left as it was.
        let new_band_name = self.project_session.project.unique_language_name(&format!(
            "{} – {}",
            band.name,
            new_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| new_name.clone())
        ));
        let Some(new_band_id) = self
            .project_session
            .project
            .duplicate_language(band_id, new_band_name.clone())
        else {
            self.show_toast(t("conform.no_band"), 5.0);
            return;
        };
        self.project_session.history.clear();
        self.refresh_after_language_change();
        let band = self
            .project_session
            .project
            .language(new_band_id)
            .unwrap_or(band);
        let map = analysis.time_map();
        let ratio = analysis.fps_ratio();
        let plan = conform::plan::plan_band(&self.project_session.project, &map, ratio);
        let mut entries = plan.entries;
        if let Some(command) = plan.command {
            self.execute_local_command(command);
        }

        // 2. The takes of the Recording tab.
        if move_takes {
            let recording = conform::plan::plan_recording(
                &self.project_session.recording_project,
                &map,
                analysis.fps,
                analysis.new_fps,
            );
            if let Some(operation) = recording.operation {
                if let Err(error) = self.apply_recording_operation(operation) {
                    log::error!("Conform: takes could not be moved: {error}");
                    self.show_toast(error.to_string(), 6.0);
                }
            }
            entries.extend(recording.entries);
        }

        // 3. The new cut joins the library, becomes the default and plays.
        let library = self.project_session.project.media_library();
        let existing = library
            .videos
            .iter()
            .find(|video| crate::video_proxy::paths_match(Path::new(&video.path), &new_path))
            .map(|video| video.id);
        let new_id = match existing {
            Some(id) => Some(id),
            None => {
                let stem = new_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| new_name.clone());
                match self.project_session.project.add_media_video(
                    stem,
                    new_path.to_string_lossy(),
                    None,
                    false,
                ) {
                    Ok(id) => Some(id),
                    Err(error) => {
                        log::error!("Conform: cannot add the new cut: {error}");
                        self.show_toast(error, 6.0);
                        None
                    }
                }
            }
        };
        if let Some(id) = new_id {
            let _ = self.project_session.project.set_default_video(Some(id));
        }
        self.project_session.dirty = true;
        let seek = map
            .map_frame(self.current_frame())
            .map(|frame| (frame as f64 * ratio).round() as i64)
            .unwrap_or(0);
        self.load_video_for_playback(&new_path, None, Some(seek), false);
        self.last_conform_target = Some(new_path);

        // 4. The report.
        let report = ConformReport {
            old_video: old_video.name,
            new_video: new_name,
            band: band.name,
            fps: analysis.fps,
            new_fps: analysis.new_fps,
            coverage: analysis.coverage,
            entries,
        };
        let summary = format!(
            "{} {} {}, {} {}",
            t("toast.conform_done"),
            report.count(conform::SpanStatus::Moved)
                + report.count(conform::SpanStatus::Modified)
                + report.count(conform::SpanStatus::Truncated),
            t("conform.status.moved").to_lowercase(),
            report.rerecord_count(),
            t("conform.status.rerecord").to_lowercase()
        );
        if self.ui_shell.ui.conform_modal().is_none() {
            let modal = ConformModal::new(
                self.conform_video_choices(),
                self.conform_band_choices(),
                Some(video_id),
                Some(band_id),
                move_takes,
            );
            self.ui_shell.ui.open_conform_modal(modal);
        }
        if let Some(modal) = self.ui_shell.ui.conform_modal_mut() {
            modal.show_report(report);
        }
        self.show_toast(summary.clone(), 6.0);
        self.announce_accessibility(AccessibilityEvent::Success { message: summary });
    }
}
