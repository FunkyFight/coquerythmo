use std::path::PathBuf;

use crate::export;
use crate::i18n;
use crate::state::State;
use crate::ui::file_explorer::{
    FileExplorerMode, FileExplorerRequest, FileFilterSpec, FilePickerIntent,
};
pub(crate) fn open_dialog_filters(filter_name: &str, extensions: &[&str]) -> Vec<FileFilterSpec> {
    vec![
        FileFilterSpec::new(i18n::t("picker.filter.all_files"), &["*"]),
        FileFilterSpec::new(filter_name, extensions),
    ]
}

pub(crate) fn save_dialog_filters(filter_name: &str, extensions: &[&str]) -> Vec<FileFilterSpec> {
    vec![
        FileFilterSpec::new(filter_name, extensions),
        FileFilterSpec::new(i18n::t("picker.filter.all_files"), &["*"]),
    ]
}

pub(crate) fn downloads_or_home_dir() -> Option<PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir)
}

pub(crate) fn project_or_video_dir(state: &State) -> Option<PathBuf> {
    state
        .project_session
        .project_path
        .as_ref()
        .and_then(|prev| prev.parent().map(PathBuf::from))
        .or_else(|| {
            state
                .video_path()
                .and_then(|video| video.parent().map(PathBuf::from))
        })
        .or_else(downloads_or_home_dir)
}

pub(crate) fn video_or_project_dir(state: &State) -> Option<PathBuf> {
    state
        .video_path()
        .and_then(|video| video.parent().map(PathBuf::from))
        .or_else(|| {
            state
                .project_session
                .project_path
                .as_ref()
                .and_then(|prev| prev.parent().map(PathBuf::from))
        })
        .or_else(downloads_or_home_dir)
}

pub(crate) fn picker_extra_locations(state: &State) -> Vec<(String, PathBuf)> {
    let mut locations = Vec::new();
    if let Some(path) = state
        .project_session
        .project_path
        .as_ref()
        .and_then(|prev| prev.parent().map(PathBuf::from))
    {
        locations.push((i18n::t("file_explorer.sidebar.project").to_string(), path));
    }
    if let Some(path) = state
        .video_path()
        .and_then(|video| video.parent().map(PathBuf::from))
    {
        if !locations.iter().any(|(_, existing)| {
            existing
                .to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        }) {
            locations.push((i18n::t("file_explorer.sidebar.video").to_string(), path));
        }
    }
    locations
}

pub(crate) fn open_file_picker(
    state: &mut State,
    title: &str,
    mode: FileExplorerMode,
    intent: FilePickerIntent,
    filters: Vec<FileFilterSpec>,
    initial_dir: Option<PathBuf>,
    default_extension: Option<&str>,
) {
    state.open_file_explorer(FileExplorerRequest {
        title: title.to_string(),
        mode,
        intent,
        filters,
        initial_dir,
        default_extension: default_extension.map(str::to_string),
        initial_filename: None,
        extra_locations: picker_extra_locations(state),
    });
}

pub(crate) fn save_project_as(state: &mut State, path: PathBuf) -> bool {
    use export::{JsonExporter, ProjectExporter};
    let fps = state.fps();
    if let Err(e) = JsonExporter.export(&state.project_session.project, fps, &path) {
        log::error!("Export failed: {e}");
        false
    } else {
        state.project_session.project_path = Some(path);
        state.project_session.dirty = false;
        state.show_toast(i18n::t("toast.saved"), 3.0);
        state.reload_linked_proxy();
        true
    }
}

pub(crate) fn quick_save_existing(state: &mut State) -> bool {
    use export::{JsonExporter, ProjectExporter};
    let Some(path) = state.project_session.project_path.clone() else {
        return false;
    };
    let fps = state.fps();
    if let Err(e) = JsonExporter.export(&state.project_session.project, fps, &path) {
        log::error!("Quick save failed: {e}");
        false
    } else {
        log::info!("Quick saved to {}", path.display());
        state.project_session.dirty = false;
        state.show_toast(i18n::t("toast.saved"), 3.0);
        true
    }
}

pub(crate) fn import_project_from_path(state: &mut State, path: PathBuf) {
    state.start_br_import(path);
}

pub(crate) fn import_cappela_from_path(state: &mut State, path: PathBuf) {
    let fps = state.fps();
    match export::import_cappela(&path, fps) {
        Ok(data) => {
            crate::application::edit_service::EditExecutor::apply_import(
                &mut state.project_session,
                data,
                fps,
            );
            state.project_session.project_path = None;
        }
        Err(e) => log::error!("Cappela import failed: {e}"),
    }
}

pub(crate) fn import_srt_from_path(state: &mut State, path: PathBuf) {
    let fps = state.fps();
    let total_frames = state.total_frames();
    match export::import_srt(&path, fps) {
        Ok(mut data) => {
            let (clipped, skipped) = data.clamp_to_total_frames(total_frames);
            if clipped > 0 || skipped > 0 {
                log::warn!(
                    "SRT import clipped to video duration: {clipped} shortened, {skipped} skipped"
                );
            }
            crate::application::edit_service::EditExecutor::apply_import(
                &mut state.project_session,
                data,
                fps,
            );
            state.project_session.project_path = None;
        }
        Err(e) => log::error!("SRT import failed: {e}"),
    }
}
