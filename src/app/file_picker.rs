use std::path::PathBuf;

use crate::application::command::{
    FileFilterSpec, FilePickerIntent, FilePickerMode, FilePickerRequest,
};
use crate::application::job_service::SaveContinuation;
use crate::export;
use crate::export::ProjectImporter;
use crate::i18n;
use crate::state::State;
use winit::event_loop::EventLoopWindowTarget;

use super::dispatcher::handle_file_picker_selected;
use super::event_loop::AppEvent;
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

pub(crate) fn open_file_picker(
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
    title: &str,
    mode: FilePickerMode,
    intent: FilePickerIntent,
    filters: Vec<FileFilterSpec>,
    initial_dir: Option<PathBuf>,
    default_extension: Option<&str>,
) {
    open_file_picker_request(
        state,
        elwt,
        FilePickerRequest {
            title: title.to_string(),
            mode,
            intent,
            filters,
            initial_dir,
            default_extension: default_extension.map(str::to_string),
            initial_filename: None,
        },
    );
}

pub(crate) fn open_file_picker_request(
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
    request: FilePickerRequest,
) {
    if let Some(path) = native_file_picker(&request) {
        handle_file_picker_selected(request.intent, path, state, elwt);
    } else {
        log::debug!("File picker cancelled: {}", request.title);
    }
}

fn native_file_picker(request: &FilePickerRequest) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(&request.title);
    if let Some(initial_dir) = request.initial_dir.as_deref() {
        dialog = dialog.set_directory(initial_dir);
    }
    if let Some(initial_filename) = request.initial_filename.as_deref() {
        dialog = dialog.set_file_name(initial_filename);
    }
    for filter in &request.filters {
        let extensions: Vec<_> = filter
            .extensions
            .iter()
            .filter(|extension| extension.as_str() != "*")
            .map(String::as_str)
            .collect();
        if !extensions.is_empty() {
            dialog = dialog.add_filter(&filter.name, &extensions);
        }
    }

    let path = match request.mode {
        FilePickerMode::Open => dialog.pick_file(),
        FilePickerMode::Save => dialog.save_file(),
    }?;
    if request.mode == FilePickerMode::Save
        && path.extension().is_none()
        && request.default_extension.is_some()
    {
        Some(path.with_extension(request.default_extension.as_deref().unwrap()))
    } else {
        Some(path)
    }
}

pub(crate) fn save_project_as(state: &mut State, path: PathBuf) -> bool {
    save_project_as_with_continuation(state, path, SaveContinuation::None)
}

pub(crate) fn save_project_as_with_continuation(
    state: &mut State,
    path: PathBuf,
    continuation: SaveContinuation,
) -> bool {
    let path = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case(crate::project_archive::PROJECT_EXTENSION)
        }) {
        path
    } else {
        path.with_extension(crate::project_archive::PROJECT_EXTENSION)
    };
    let Some(source_video) = state.video_path() else {
        state.show_toast(i18n::t("toast.save_requires_video"), 5.0);
        return false;
    };
    let proxy_video = state.playback.proxy_video_path.clone();
    let Some((_, font_asset)) = crate::vector_text::selected_font_asset() else {
        state.show_toast(i18n::t("toast.save_font_unavailable"), 6.0);
        return false;
    };
    state.start_project_save(path, source_video, proxy_video, font_asset, continuation)
}

pub(crate) fn quick_save_existing(state: &mut State) -> bool {
    quick_save_existing_with_continuation(state, SaveContinuation::None)
}

pub(crate) fn quick_save_existing_with_continuation(
    state: &mut State,
    continuation: SaveContinuation,
) -> bool {
    let Some(path) = state.project_session.project_path.clone() else {
        return false;
    };
    let Some(source_video) = state.video_path() else {
        state.show_toast(i18n::t("toast.save_requires_video"), 5.0);
        return false;
    };
    let proxy_video = state.playback.proxy_video_path.clone();
    let Some((_, font_asset)) = crate::vector_text::selected_font_asset() else {
        state.show_toast(i18n::t("toast.save_font_unavailable"), 6.0);
        return false;
    };
    state.start_project_save(path, source_video, proxy_video, font_asset, continuation)
}

pub(crate) fn import_project_from_path(state: &mut State, path: PathBuf) {
    state.start_br_import(path);
}

pub(crate) fn import_cappela_from_path(state: &mut State, path: PathBuf) {
    let fps = state.fps();
    match export::import_cappela(&path, fps) {
        Ok(data) => {
            crate::application::edit_service::EditExecutor::apply_subtitle_import(
                &mut state.project_session,
                data,
                fps,
            );
            state.project_session.project_path = None;
        }
        Err(e) => log::error!("Cappela import failed: {e}"),
    }
}

pub(crate) fn import_subtitle_from_path(state: &mut State, path: PathBuf) {
    let fps = state.fps();
    let total_frames = state.total_frames();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension.eq_ignore_ascii_case(crate::project_archive::PROJECT_EXTENSION) {
        import_project_from_path(state, path);
        return;
    }
    let result = match extension.as_str() {
        "json" => export::JsonImporter.import(&path),
        "srt" => export::import_srt(&path, fps),
        "ass" => export::import_ass(&path, fps),
        "detx" => export::import_cappela(&path, fps),
        _ => Err(format!("Unsupported subtitle format: .{extension}")),
    };
    match result {
        Ok(mut data) => {
            let (clipped, skipped) = data.clamp_to_total_frames(total_frames);
            if clipped > 0 || skipped > 0 {
                log::warn!(
                    "Subtitle import clipped to video duration: {clipped} shortened, {skipped} skipped"
                );
            }
            crate::application::edit_service::EditExecutor::apply_subtitle_import(
                &mut state.project_session,
                data,
                fps,
            );
            state.project_session.project_path = None;
            state.show_toast(i18n::t("toast.subtitle_imported"), 4.0);
        }
        Err(e) => {
            log::error!("Subtitle import failed: {e}");
            state.show_toast(format!("{} {e}", i18n::t("toast.import_failed")), 8.0);
        }
    }
}
