mod command;
mod config;
mod constants;
mod export;
mod graphics;
mod i18n;
mod media_binary;
mod network;
mod observer;
mod packet;
mod project;
mod render_index;
mod rythmo_cpu_renderer;
mod rythmo_gpu_renderer;
mod rythmo_layout;
mod rythmo_line;
mod state;
mod syllable;
mod ui;
mod update;
mod vector_text;
mod video;
mod video_export;
mod video_proxy;
mod voice_actor;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

use state::State;
use ui::file_explorer_modal::{
    FileExplorerMode, FileExplorerRequest, FileFilterSpec, FilePickerIntent,
};
use ui::widget::{EventResponse, UiAction, UiEvent};

#[derive(Debug)]
enum AppEvent {
    WhatsNewFetched {
        version: String,
        result: Result<update::ReleaseInfo, String>,
    },
}

fn start_whats_new_fetch(version: String, proxy: EventLoopProxy<AppEvent>) {
    let tag = format!("v{version}");
    std::thread::spawn(move || {
        let result = update::fetch_release_by_tag(&tag);
        let _ = proxy.send_event(AppEvent::WhatsNewFetched { version, result });
    });
}

fn handle_whats_new_result(
    version: String,
    result: Result<update::ReleaseInfo, String>,
    state: &mut State,
) {
    match result {
        Ok(release) => {
            let expected_tag = format!("v{version}");
            if release.tag_name != expected_tag {
                log::warn!(
                    "Ignoring release notes for {}, expected {}",
                    release.tag_name,
                    expected_tag
                );
                return;
            }
            state.open_whats_new_modal(version.clone(), release.body);
            config::mark_whats_new_seen(&version);
        }
        Err(e) => {
            log::warn!("Could not fetch release notes for version {version}: {e}");
        }
    }
}

fn new_project_reset_and_pick_video(state: &mut State, elwt: &EventLoopWindowTarget<AppEvent>) {
    state.project.reset();
    state.project_path = None;
    state.dirty = false;
    state.clear_history();
    handle_action(UiAction::AddVideo, state, elwt);
}

fn open_dialog_filters(filter_name: &str, extensions: &[&str]) -> Vec<FileFilterSpec> {
    vec![
        FileFilterSpec::new(i18n::t("picker.filter.all_files"), &["*"]),
        FileFilterSpec::new(filter_name, extensions),
    ]
}

fn save_dialog_filters(filter_name: &str, extensions: &[&str]) -> Vec<FileFilterSpec> {
    vec![
        FileFilterSpec::new(filter_name, extensions),
        FileFilterSpec::new(i18n::t("picker.filter.all_files"), &["*"]),
    ]
}

fn downloads_or_home_dir() -> Option<PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir)
}

fn project_or_video_dir(state: &State) -> Option<PathBuf> {
    state
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

fn video_or_project_dir(state: &State) -> Option<PathBuf> {
    state
        .video_path()
        .and_then(|video| video.parent().map(PathBuf::from))
        .or_else(|| {
            state
                .project_path
                .as_ref()
                .and_then(|prev| prev.parent().map(PathBuf::from))
        })
        .or_else(downloads_or_home_dir)
}

fn picker_extra_locations(state: &State) -> Vec<(String, PathBuf)> {
    let mut locations = Vec::new();
    if let Some(path) = state
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

fn open_file_picker(
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

fn save_project_as(state: &mut State, path: PathBuf) -> bool {
    use export::{JsonExporter, ProjectExporter};
    let fps = state.fps();
    if let Err(e) = JsonExporter.export(&state.project, fps, &path) {
        log::error!("Export failed: {e}");
        false
    } else {
        state.project_path = Some(path);
        state.dirty = false;
        state.show_toast(i18n::t("toast.saved"), 3.0);
        state.reload_linked_proxy();
        true
    }
}

fn quick_save_existing(state: &mut State) -> bool {
    use export::{JsonExporter, ProjectExporter};
    let Some(path) = state.project_path.clone() else {
        return false;
    };
    let fps = state.fps();
    if let Err(e) = JsonExporter.export(&state.project, fps, &path) {
        log::error!("Quick save failed: {e}");
        false
    } else {
        log::info!("Quick saved to {}", path.display());
        state.dirty = false;
        state.show_toast(i18n::t("toast.saved"), 3.0);
        true
    }
}

fn import_project_from_path(state: &mut State, path: PathBuf) {
    use export::{JsonImporter, ProjectImporter};
    let importer = JsonImporter;
    match importer.import(&path) {
        Ok(data) => {
            let fps = state.fps();
            data.apply_to_project(&mut state.project, fps);
            state.sync_audio_settings_to_player();
            state.project_path = Some(path.clone());
            state.reload_linked_proxy();
            if let Some(video) = state.video_path() {
                config::add_recent_project(video, path);
                state.rebuild_topbar_for_network();
            }
        }
        Err(e) => log::error!("Import failed: {e}"),
    }
}

fn import_cappela_from_path(state: &mut State, path: PathBuf) {
    let fps = state.fps();
    match export::import_cappela(&path, fps) {
        Ok(data) => {
            data.apply_to_project(&mut state.project, fps);
            state.project_path = None;
            state.dirty = true;
        }
        Err(e) => log::error!("Cappela import failed: {e}"),
    }
}

fn import_srt_from_path(state: &mut State, path: PathBuf) {
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
            data.apply_to_project(&mut state.project, fps);
            state.project_path = None;
            state.dirty = true;
        }
        Err(e) => log::error!("SRT import failed: {e}"),
    }
}

fn handle_file_picker_selected(
    intent: FilePickerIntent,
    path: PathBuf,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) {
    match intent {
        FilePickerIntent::AddVideo => state.load_video(&path),
        FilePickerIntent::ImportProject => import_project_from_path(state, path),
        FilePickerIntent::ImportCappelaProject => import_cappela_from_path(state, path),
        FilePickerIntent::ImportSrtProject => import_srt_from_path(state, path),
        FilePickerIntent::ExportProject | FilePickerIntent::QuickSave => {
            save_project_as(state, path);
        }
        FilePickerIntent::NewProjectSave => {
            if save_project_as(state, path) {
                new_project_reset_and_pick_video(state, elwt);
            }
        }
        FilePickerIntent::VoiceActorIcon => {
            state.set_voice_actor_modal_icon_path(path.to_string_lossy().into_owned());
        }
        FilePickerIntent::ProjectInstrumentalAudio => {
            let path = path.to_string_lossy().into_owned();
            state.set_project_instrumental_audio_path(path.clone());
            state.save_project_settings(Some(path));
            state.close_project_settings_modal();
        }
        FilePickerIntent::ExportMp4 {
            fps,
            br_scale,
            karaoke_text_scale,
            export_width,
            export_height,
            export_original_audio,
            export_instrumental_audio,
        } => {
            let _ = handle_action(
                UiAction::StartExportToPath {
                    output_path: path,
                    fps,
                    br_scale,
                    karaoke_text_scale,
                    export_width,
                    export_height,
                    export_original_audio,
                    export_instrumental_audio,
                },
                state,
                elwt,
            );
        }
    }
}

fn handle_action(
    action: UiAction,
    state: &mut State,
    elwt: &EventLoopWindowTarget<AppEvent>,
) -> bool {
    match action {
        UiAction::CloseApp => return true,
        UiAction::AddVideo => {
            let filters = open_dialog_filters("Video", &["mp4", "mov", "avi", "mkv", "webm"]);
            open_file_picker(
                state,
                i18n::t("picker.video.title"),
                FileExplorerMode::Open,
                FilePickerIntent::AddVideo,
                filters,
                project_or_video_dir(state),
                None,
            );
        }
        UiAction::ExportProject => {
            use export::{JsonExporter, ProjectExporter};
            let exporter = JsonExporter;
            let extensions = [exporter.extension()];
            let filters = save_dialog_filters(exporter.description(), &extensions);
            open_file_picker(
                state,
                i18n::t("picker.export.title"),
                FileExplorerMode::Save,
                FilePickerIntent::ExportProject,
                filters,
                project_or_video_dir(state),
                Some(exporter.extension()),
            );
        }
        UiAction::ImportProject => {
            let filters = open_dialog_filters("Bande rythmo JSON", &["json"]);
            open_file_picker(
                state,
                i18n::t("picker.import.title"),
                FileExplorerMode::Open,
                FilePickerIntent::ImportProject,
                filters,
                project_or_video_dir(state),
                None,
            );
        }
        UiAction::ImportCappelaProject => {
            let filters = open_dialog_filters("Cappela DETX", &["detx"]);
            open_file_picker(
                state,
                i18n::t("picker.import.cappela.title"),
                FileExplorerMode::Open,
                FilePickerIntent::ImportCappelaProject,
                filters,
                project_or_video_dir(state),
                None,
            );
        }
        UiAction::ImportSrtProject => {
            let filters = open_dialog_filters("SubRip SRT", &["srt"]);
            open_file_picker(
                state,
                i18n::t("picker.import.srt.title"),
                FileExplorerMode::Open,
                FilePickerIntent::ImportSrtProject,
                filters,
                project_or_video_dir(state),
                None,
            );
        }
        UiAction::QuickSave => {
            use export::{JsonExporter, ProjectExporter};
            if state.project_path.is_some() {
                quick_save_existing(state);
            } else {
                let exporter = JsonExporter;
                let extensions = [exporter.extension()];
                let filters = save_dialog_filters(exporter.description(), &extensions);
                open_file_picker(
                    state,
                    i18n::t("picker.export.title"),
                    FileExplorerMode::Save,
                    FilePickerIntent::QuickSave,
                    filters,
                    project_or_video_dir(state),
                    Some(exporter.extension()),
                );
            }
        }
        UiAction::FilePickerSelected { intent, path } => {
            handle_file_picker_selected(intent, path, state, elwt);
        }
        UiAction::TogglePlayPause => {
            state.toggle_play_pause();
        }
        UiAction::SetVolume(vol) => {
            state.set_volume(vol);
        }
        UiAction::ToggleMute => {
            state.toggle_mute();
        }
        UiAction::PrevFrame => {
            state.prev_frame();
        }
        UiAction::NextFrame => {
            state.next_frame();
        }
        UiAction::SeekRelative(delta) => {
            state.seek_relative(delta);
        }
        UiAction::SeekAbsolute(frame) => {
            state.seek_absolute(frame);
        }
        UiAction::SeekToNextBoucle { direction } => {
            state.seek_to_next_boucle(direction);
        }
        UiAction::CreateLine { frame, y_slot } => {
            let line_id = state.create_line(frame, y_slot);
            state.start_editing_line(line_id);
        }
        UiAction::ResizeLine {
            id,
            start_frame,
            duration_frames,
        } => {
            state.resize_line(id, start_frame, duration_frames);
        }
        UiAction::MoveLine {
            id,
            start_frame,
            y_slot,
        } => {
            state.move_line(id, start_frame, y_slot);
        }
        UiAction::MoveLines { moves } => {
            state.move_lines(moves);
        }
        UiAction::UpdateLineText { id, text } => {
            state.update_line_text(id, text);
        }
        UiAction::SetCharacter {
            line_id,
            name,
            color,
        } => {
            state.set_character(line_id, name, color);
        }
        UiAction::SetCharacterColor { line_id, color } => {
            state.set_character_color(line_id, color);
        }
        UiAction::UpdateCharacterName { line_id, name } => {
            state.update_character_name(line_id, name);
        }
        UiAction::FinalizeCharacter { line_id } => {
            state.finalize_character(line_id);
        }
        UiAction::OpenVoiceActorModal => {
            state.open_voice_actor_modal();
        }
        UiAction::OpenRenameCharacterModal => {
            state.open_rename_character_modal();
        }
        UiAction::RenameCharacter { old_name, new_name } => {
            state.rename_character_everywhere(old_name, new_name);
        }
        UiAction::PickVoiceActorIcon => {
            let filters = open_dialog_filters(
                "Image",
                &["png", "jpg", "jpeg", "webp", "bmp", "ico", "gif", "svg"],
            );
            open_file_picker(
                state,
                i18n::t("picker.voice_actor_icon.title"),
                FileExplorerMode::Open,
                FilePickerIntent::VoiceActorIcon,
                filters,
                video_or_project_dir(state),
                None,
            );
        }
        UiAction::CreateVoiceActor { name, icon_path } => {
            state.create_voice_actor(name, icon_path);
        }
        UiAction::AssignVoiceActorLine {
            line_id,
            actor_name,
        } => {
            state.assign_voice_actor_to_line(line_id, actor_name);
        }
        UiAction::AssignVoiceActorCharacter {
            line_id,
            actor_name,
        } => {
            state.assign_voice_actor_to_character(line_id, actor_name);
        }
        UiAction::UnassignVoiceActorLine {
            line_id,
            actor_name,
        } => {
            state.unassign_voice_actor_from_line(line_id, actor_name);
        }
        UiAction::UnassignVoiceActorCharacter {
            line_id,
            actor_name,
        } => {
            state.unassign_voice_actor_from_character(line_id, actor_name);
        }
        UiAction::AddMarker(kind) => {
            state.add_marker(kind);
        }
        UiAction::AddQuickLine { text } => {
            state.add_quick_line(text);
        }
        UiAction::OpenExportModal => {
            if state.video_path().is_some() {
                state.open_export_modal();
            } else {
                log::warn!("No video loaded — cannot export MP4");
            }
        }
        UiAction::OpenProxyModal => {
            if state.video_path().is_none() {
                log::warn!("No video loaded — cannot create proxy");
            } else if state.project_path.is_none() {
                state.show_toast(i18n::t("toast.proxy_requires_saved_br"), 5.0);
            } else {
                state.open_proxy_modal();
            }
        }
        UiAction::CreateProxy { width, height, crf } => {
            let Some(source) = state.video_path() else {
                log::warn!("No video loaded — cannot create proxy");
                return false;
            };
            let Some(br_path) = state.project_path.clone() else {
                state.show_toast(i18n::t("toast.proxy_requires_saved_br"), 5.0);
                return false;
            };

            let progress =
                std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.01_f32.to_bits()));
            let progress_for_ui = progress.clone();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let cancel_for_job = cancel.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let source_for_job = source.clone();
            let br_for_job = br_path.clone();

            std::thread::spawn(move || {
                let p = progress.clone();
                let result = video_proxy::create_proxy(
                    &source_for_job,
                    &br_for_job,
                    width,
                    height,
                    crf,
                    cancel_for_job,
                    move |v| {
                        p.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
                    },
                );
                let _ = tx.send(result);
                progress.store(2.0_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
            });

            state.set_progress_label(i18n::t("progress.proxy"));
            state.set_export_progress(Some(progress_for_ui));
            state.set_export_cancel(Some(cancel));
            state.watch_proxy_job(source, rx);
        }
        UiAction::StartExport {
            fps,
            br_scale,
            karaoke_text_scale,
            export_width,
            export_height,
            export_original_audio,
            export_instrumental_audio,
        } => {
            let filters = save_dialog_filters("MP4 Video", &["mp4"]);
            open_file_picker(
                state,
                i18n::t("picker.export_mp4.title"),
                FileExplorerMode::Save,
                FilePickerIntent::ExportMp4 {
                    fps,
                    br_scale,
                    karaoke_text_scale,
                    export_width,
                    export_height,
                    export_original_audio,
                    export_instrumental_audio,
                },
                filters,
                video_or_project_dir(state),
                Some("mp4"),
            );
        }
        UiAction::StartExportToPath {
            output_path,
            fps,
            br_scale,
            karaoke_text_scale,
            export_width,
            export_height,
            export_original_audio,
            export_instrumental_audio,
        } => {
            if let Some(source) = state.video_path() {
                let source_fps = state.fps();
                let project_snap = state.project.snapshot();
                if !export_original_audio && !export_instrumental_audio {
                    state.show_toast(i18n::t("toast.no_audio_export_selected"), 3.0);
                    return false;
                }
                let instrumental_audio_path = export_instrumental_audio
                    .then(|| {
                        project_snap
                            .settings
                            .instrumental_audio_path
                            .as_ref()
                            .filter(|path| !path.trim().is_empty())
                            .map(std::path::PathBuf::from)
                    })
                    .flatten();
                if export_instrumental_audio && instrumental_audio_path.is_none() {
                    state.show_toast(i18n::t("toast.no_instrumental_audio"), 3.0);
                    return false;
                }
                let double_export_instrumental = export_original_audio && export_instrumental_audio;
                let source_audio_offset_frames = project_snap.settings.source_audio_offset_frames;
                let instrumental_audio_offset_frames =
                    project_snap.settings.instrumental_audio_offset_frames;
                let progress =
                    std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.0_f32.to_bits()));
                let progress_for_ui = progress.clone();
                let render_backend_status = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
                    video_export::EXPORT_RENDER_BACKEND_UNKNOWN,
                ));
                let render_backend_for_ui = render_backend_status.clone();
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let cancel_for_job = cancel.clone();
                let (tx, rx) = std::sync::mpsc::channel();

                std::thread::spawn(move || {
                    progress.store(0.01_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
                    let p = progress.clone();
                    let result = video_export::export_mp4(
                        &project_snap,
                        &source,
                        &output_path,
                        fps,
                        source_fps,
                        br_scale,
                        karaoke_text_scale,
                        export_width,
                        export_height,
                        instrumental_audio_path.as_deref(),
                        source_audio_offset_frames,
                        instrumental_audio_offset_frames,
                        double_export_instrumental,
                        Some(render_backend_status.clone()),
                        cancel_for_job,
                        move |v| {
                            p.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        },
                    );
                    let _ = tx.send(result);
                    progress.store(2.0_f32.to_bits(), std::sync::atomic::Ordering::Relaxed);
                });
                // Set progress tracker — overlay only visible once progress > 0
                state.set_export_render_backend(Some(render_backend_for_ui));
                state.set_export_progress(Some(progress_for_ui));
                state.set_export_cancel(Some(cancel));
                state.watch_export_job(rx);
            } else {
                log::warn!("No video loaded — cannot export MP4");
            }
        }
        UiAction::PickProjectInstrumentalAudio => {
            let filters = open_dialog_filters(
                "Audio",
                &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"],
            );
            open_file_picker(
                state,
                i18n::t("picker.instrumental_audio.title"),
                FileExplorerMode::Open,
                FilePickerIntent::ProjectInstrumentalAudio,
                filters,
                video_or_project_dir(state),
                None,
            );
        }
        UiAction::OpenDropdown(dropdown) => {
            state.open_toolbar_dropdown(dropdown);
        }
        UiAction::OpenSecondaryDisplay => {
            if state.video_path().is_none() {
                log::warn!("No video loaded — cannot open secondary display");
            } else if state.has_secondary_display() {
                state.request_secondary_redraw();
            } else {
                match app_window_builder()
                    .with_title(i18n::t("menu.tools.secondary_display"))
                    .with_inner_size(LogicalSize::new(1280.0, 720.0))
                    .with_window_icon(parse_ico_to_winit_icon(include_bytes!("icons/app.ico")))
                    .build(elwt)
                {
                    Ok(window) => state.open_secondary_display(Arc::new(window)),
                    Err(e) => log::error!("Failed to create secondary display window: {e}"),
                }
            }
        }
        UiAction::DeleteSelected => {
            state.delete_selected();
        }
        UiAction::MoveMarker { index, frame } => {
            state.move_marker(index, frame);
        }
        UiAction::CancelExport => {
            state.cancel_export();
        }
        UiAction::StopEditing => {
            state.broadcast_finalize();
        }
        UiAction::OpenRecentProject {
            video_path,
            br_path,
        } => {
            use export::{JsonImporter, ProjectImporter};
            if video_path.exists() && br_path.exists() {
                state.project_path = Some(br_path.clone());
                state.load_video(&video_path);
                let importer = JsonImporter;
                match importer.import(&br_path) {
                    Ok(data) => {
                        let fps = state.fps();
                        data.apply_to_project(&mut state.project, fps);
                        state.sync_audio_settings_to_player();
                        config::add_recent_project(video_path, br_path);
                        state.rebuild_topbar_for_network();
                        log::info!("Loaded recent project");
                    }
                    Err(e) => log::error!("Failed to load recent BR: {e}"),
                }
            } else {
                log::warn!("Recent project files missing, skipping");
            }
        }
        UiAction::ToggleKaraokeForSelection => {
            state.toggle_karaoke_for_selection();
        }
        UiAction::SetSyllableRatios { line_id, ratios } => {
            state.set_syllable_ratios(line_id, ratios);
        }
        UiAction::SplitDialogue => {
            state.split_dialogue();
        }
        UiAction::OpenServerBrowser => {
            state.open_server_browser();
        }
        UiAction::OpenConnectModal { ip, port, join } => {
            state.open_connect_modal(&ip, port, join);
        }
        UiAction::OpenAddServerModal => {
            state.open_add_server_modal();
        }
        UiAction::AddServer { ip, port } => {
            config::add_server(ip, port);
            state.refresh_server_browser();
        }
        UiAction::RemoveServer(index) => {
            config::remove_server(index);
            state.refresh_server_browser();
        }
        UiAction::RefreshServers => {
            state.refresh_server_browser();
        }
        UiAction::NetworkConnect {
            ip,
            port,
            password,
            username,
            room_code,
        } => {
            // Save last used connection settings
            {
                let mut cfg = config::get().clone();
                cfg.network.server_ip = ip.clone();
                cfg.network.server_port = port;
                cfg.network.password = password.clone();
                cfg.network.username = username.clone();
                cfg.save();
            }
            let first_packet = if let Some(code) = room_code {
                packet::Packet::JoinRoom { code, username }
            } else {
                packet::Packet::CreateRoom { username }
            };
            state.begin_network_connect();
            state
                .network
                .connect_and_send(&ip, port, &password, first_packet);
            state.rebuild_topbar_for_network();
        }
        UiAction::NetworkDisconnect => {
            state.disconnect_network();
        }
        UiAction::RestoreBackup => {
            if state.restore_backup() {
                state.show_toast(i18n::t("toast.backup_restored"), 5.0);
            } else {
                state.show_toast(i18n::t("toast.no_backup"), 4.0);
            }
        }
        UiAction::OpenSettings => {
            state.open_settings_modal();
        }
        UiAction::OpenProjectSettings => {
            state.open_project_settings_modal();
        }
        UiAction::SaveSettings {
            lang,
            rythmo_font,
            scroll_speed,
        } => {
            crate::config::save_settings(lang, rythmo_font, scroll_speed);
            state.close_settings_modal();
        }
        UiAction::SaveProjectSettings {
            instrumental_audio_path,
        } => {
            state.save_project_settings(instrumental_audio_path);
        }
        UiAction::ToggleActiveAudio => {
            state.toggle_active_audio();
        }
        UiAction::OffsetActiveAudioBy(delta_frames) => {
            state.offset_active_audio_by(delta_frames);
        }
        UiAction::NewProject => {
            if state.dirty && !state.project.is_empty() {
                state.open_save_prompt();
            } else {
                new_project_reset_and_pick_video(state, elwt);
            }
        }
        UiAction::NewProjectSave => {
            if state.project_path.is_some() {
                if quick_save_existing(state) {
                    new_project_reset_and_pick_video(state, elwt);
                }
            } else {
                use export::{JsonExporter, ProjectExporter};
                let exporter = JsonExporter;
                let extensions = [exporter.extension()];
                let filters = save_dialog_filters(exporter.description(), &extensions);
                open_file_picker(
                    state,
                    i18n::t("picker.export.title"),
                    FileExplorerMode::Save,
                    FilePickerIntent::NewProjectSave,
                    filters,
                    project_or_video_dir(state),
                    Some(exporter.extension()),
                );
            }
        }
        UiAction::NewProjectDiscard => {
            new_project_reset_and_pick_video(state, elwt);
        }
        UiAction::EnterStudioMode => {
            state.enter_studio_mode();
        }
        UiAction::ShowStudioWarning => {
            state.open_studio_warning();
        }
        UiAction::AddNote => {
            state.start_editing_note_selected();
        }
        UiAction::UpdateLineNote { line_id, note } => {
            state.update_line_note(line_id, note);
        }
        UiAction::SetClipboard(text) => {
            clipboard_set(&text);
        }
        UiAction::SetClipboardAndUpdateLineText {
            clipboard,
            id,
            text,
        } => {
            clipboard_set(&clipboard);
            state.update_line_text(id, text);
        }
        UiAction::SetClipboardAndUpdateCharacterName {
            clipboard,
            line_id,
            name,
        } => {
            clipboard_set(&clipboard);
            state.update_character_name(line_id, name);
        }
        UiAction::SetClipboardAndUpdateLineNote {
            clipboard,
            line_id,
            note,
        } => {
            clipboard_set(&clipboard);
            state.update_line_note(line_id, note);
        }
        UiAction::CopySelectedLine => {
            state.copy_selected_line();
        }
        UiAction::CutSelectedLine => {
            state.cut_selected_line();
        }
        UiAction::PasteLine => {
            state.paste_line();
        }
    }
    false
}

fn dispatch(
    ui_event: UiEvent,
    state: &mut State,
    elwt: &winit::event_loop::EventLoopWindowTarget<AppEvent>,
) {
    if let EventResponse::Action(action) = state.handle_ui_event(&ui_event) {
        if handle_action(action, state, elwt) {
            elwt.exit();
        }
    }
    state.request_redraw();
}

fn is_space_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Space))
        || matches!(key, Key::Character(text) if text.as_str() == " ")
}

fn app_window_builder() -> WindowBuilder {
    let builder = WindowBuilder::new();
    configure_platform_window(builder)
}

#[cfg(target_os = "macos")]
fn configure_platform_window(builder: WindowBuilder) -> WindowBuilder {
    builder.with_accepts_first_mouse(true)
}

#[cfg(not(target_os = "macos"))]
fn configure_platform_window(builder: WindowBuilder) -> WindowBuilder {
    builder
}

/// Parse an ICO file and return a winit Icon (RGBA pixels).
/// Picks the largest image entry, renders it via resvg's tiny-skia PNG decoder if PNG,
/// or falls back to raw BMP parsing.
fn parse_ico_to_winit_icon(ico_data: &[u8]) -> Option<winit::window::Icon> {
    if ico_data.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([ico_data[4], ico_data[5]]) as usize;
    if count == 0 {
        return None;
    }

    // Find the largest entry
    let mut best_idx = 0;
    let mut best_size = 0u32;
    for i in 0..count {
        let off = 6 + i * 16;
        if off + 16 > ico_data.len() {
            break;
        }
        let w = if ico_data[off] == 0 {
            256
        } else {
            ico_data[off] as u32
        };
        let h = if ico_data[off + 1] == 0 {
            256
        } else {
            ico_data[off + 1] as u32
        };
        if w * h > best_size {
            best_size = w * h;
            best_idx = i;
        }
    }

    let entry_off = 6 + best_idx * 16;
    let img_size = u32::from_le_bytes([
        ico_data[entry_off + 8],
        ico_data[entry_off + 9],
        ico_data[entry_off + 10],
        ico_data[entry_off + 11],
    ]) as usize;
    let img_offset = u32::from_le_bytes([
        ico_data[entry_off + 12],
        ico_data[entry_off + 13],
        ico_data[entry_off + 14],
        ico_data[entry_off + 15],
    ]) as usize;

    if img_offset + img_size > ico_data.len() {
        return None;
    }
    let img_data = &ico_data[img_offset..img_offset + img_size];

    // Check if it's PNG (starts with PNG signature)
    if img_data.len() > 8 && img_data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        // Use resvg's tiny-skia to decode PNG
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(img_data).ok()?;
        let w = pixmap.width();
        let h = pixmap.height();
        // Convert premultiplied RGBA to straight RGBA
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for pixel in pixmap.data().chunks_exact(4) {
            let a = pixel[3] as f32 / 255.0;
            if a > 0.0 {
                rgba.push((pixel[0] as f32 / a).min(255.0) as u8);
                rgba.push((pixel[1] as f32 / a).min(255.0) as u8);
                rgba.push((pixel[2] as f32 / a).min(255.0) as u8);
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
            rgba.push(pixel[3]);
        }
        winit::window::Icon::from_rgba(rgba, w, h).ok()
    } else {
        None // BMP entries not supported, skip
    }
}

#[cfg(target_os = "windows")]
fn clipboard_set(text: &str) {
    use std::ptr;
    extern "system" {
        fn OpenClipboard(hwnd: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalAlloc(flags: u32, bytes: usize) -> *mut std::ffi::c_void;
        fn GlobalLock(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hmem: *mut std::ffi::c_void) -> i32;
    }
    const GMEM_MOVEABLE: u32 = 0x0002;
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * std::mem::size_of::<u16>();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !handle.is_null() {
            let data = GlobalLock(handle) as *mut u16;
            if !data.is_null() {
                ptr::copy_nonoverlapping(wide.as_ptr(), data, wide.len());
                GlobalUnlock(handle);
                SetClipboardData(13, handle);
            }
        }
        CloseClipboard();
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_set(_text: &str) {}

#[cfg(target_os = "windows")]
fn clipboard_paste() -> Option<String> {
    use std::ptr;
    extern "system" {
        fn OpenClipboard(hwnd: *mut std::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut std::ffi::c_void;
        fn GlobalLock(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn GlobalUnlock(hmem: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(13); // CF_UNICODETEXT
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let data = GlobalLock(handle) as *const u16;
        if data.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0;
        while *data.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(data, len));
        GlobalUnlock(handle);
        CloseClipboard();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_paste() -> Option<String> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn show_untested_platform_warning() {
    let platform = if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    let message = format!(
        "Cette version {platform} de Coquerythmo n'a pas pu être testée correctement, car je n'ai pas d'appareil Linux ni macOS pour la tester.\n\nElle peut fonctionner comme prévu, mais elle peut aussi ne pas fonctionner ou comporter des bugs spécifiques à cette plateforme."
    );

    let _ = rfd::MessageDialog::new()
        .set_title("Version non testée")
        .set_description(&message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn show_untested_platform_warning() {}

fn main() {
    env_logger::init();

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let event_loop_proxy = event_loop.create_proxy();

    config::init();
    i18n::init(&config::get().lang);
    update::promote_pending_updater_from_args();
    show_untested_platform_warning();

    // Check for updates (blocks briefly on network, shows dialog if update available)
    if update::check() {
        // Updater was launched, exit so it can replace our files
        return;
    }

    let cfg = config::get().clone();

    let window_icon = {
        let ico_data = include_bytes!("icons/app.ico");
        parse_ico_to_winit_icon(ico_data)
    };

    let window = Arc::new(
        app_window_builder()
            .with_title(&cfg.window.title)
            .with_inner_size(LogicalSize::new(cfg.window.width, cfg.window.height))
            .with_window_icon(window_icon)
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let mut state = pollster::block_on(State::new(window.clone()));
    if config::should_show_whats_new(update::current_version()) {
        start_whats_new_fetch(update::current_version().to_string(), event_loop_proxy);
    }
    state.show_toast(i18n::t("toast.welcome"), 10.0);
    let mut cursor_pos = (0.0_f32, 0.0_f32);
    let mut last_click_time = None;
    let mut ctrl_held = false;
    let mut shift_held = false;

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::UserEvent(AppEvent::WhatsNewFetched { version, result }) => {
                    handle_whats_new_result(version, result, &mut state);
                    state.request_redraw();
                }
                Event::WindowEvent { window_id, event } => {
                if state.is_secondary_window(window_id) {
                    match event {
                        WindowEvent::CloseRequested => state.close_secondary_display(),
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed {
                                if is_space_key(&event.logical_key) {
                                    state.toggle_play_pause();
                                    state.request_redraw();
                                    state.request_secondary_redraw();
                                } else if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                                    state.close_secondary_display();
                                }
                            }
                        }
                        WindowEvent::Resized(physical_size) => {
                            state.resize_secondary_display(window_id, physical_size);
                            state.request_secondary_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            state.render_secondary_display(window_id);
                        }
                        _ => {}
                    }
                    return;
                }

                if window_id != window.id() {
                    return;
                }

                match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(physical_size) => {
                    state.resize(physical_size);
                    state.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    state.resize(window.inner_size());
                    state.request_redraw();
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    ctrl_held = modifiers.state().control_key();
                    shift_held = modifiers.state().shift_key();
                    state.set_ctrl_held(ctrl_held);
                    state.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        // F5: show studio warning if video is loaded
                        if matches!(event.logical_key, Key::Named(NamedKey::F5)) && state.video_path().is_some() {
                            handle_action(UiAction::ShowStudioWarning, &mut state, elwt);
                            state.request_redraw();
                            return;
                        }
                        // ESCAPE: exit studio mode if active
                        if matches!(event.logical_key, Key::Named(NamedKey::Escape)) && state.is_studio_mode() {
                            state.exit_studio_mode();
                            state.request_redraw();
                            return;
                        }

                        let key_text = match &event.logical_key {
                            Key::Named(NamedKey::Escape) => Some("\x1b"),
                            Key::Named(NamedKey::Backspace) => Some("\x08"),
                            Key::Named(NamedKey::Enter) => Some("\r"),
                            Key::Named(NamedKey::Tab) => Some("\t"),
                            _ => None,
                        };
                        let key_text = if is_space_key(&event.logical_key) {
                            Some(" ")
                        } else {
                            key_text
                        };

                        if state.is_studio_mode() {
                            // In studio mode: only Space (play/pause) is allowed
                            if is_space_key(&event.logical_key) {
                                state.toggle_play_pause();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("k")) {
                            handle_action(UiAction::SplitDialogue, &mut state, elwt);
                            state.request_redraw();
                        } else if state.is_editing_text() {
                            if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "a") {
                                // Ctrl+A — select all text
                                dispatch(UiEvent::SelectAll, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                                dispatch(UiEvent::Copy, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                                dispatch(UiEvent::Cut, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                                dispatch(UiEvent::UndoTextEdit, &mut state, elwt);
                            } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                                // Ctrl+V — paste from clipboard
                                if let Some(text) = clipboard_paste() {
                                    dispatch(UiEvent::KeyInput { text }, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft)) {
                                if shift_held {
                                    dispatch(UiEvent::ShiftCursorLeft, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::CursorLeft, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowRight)) {
                                if shift_held {
                                    dispatch(UiEvent::ShiftCursorRight, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::CursorRight, &mut state, elwt);
                                }
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowUp)) {
                                dispatch(UiEvent::CursorUp, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::ArrowDown)) {
                                dispatch(UiEvent::CursorDown, &mut state, elwt);
                            } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                                dispatch(UiEvent::KeyInput { text: "\x7f".into() }, &mut state, elwt);
                            } else if let Some(t) = key_text {
                                dispatch(UiEvent::KeyInput { text: t.into() }, &mut state, elwt);
                            } else if let Key::Character(ch) = &event.logical_key {
                                if !ctrl_held {
                                    dispatch(UiEvent::KeyInput { text: ch.to_string() }, &mut state, elwt);
                                }
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "s") {
                            handle_action(UiAction::QuickSave, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                            dispatch(UiEvent::SelectAll, &mut state, elwt);
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                            handle_action(UiAction::CopySelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                            handle_action(UiAction::CutSelectedLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                            handle_action(UiAction::PasteLine, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "z") {
                            if event.repeat || !event.state.is_pressed() { /* skip */ } else {
                                state.undo();
                                state.request_redraw();
                            }
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "n") {
                            handle_action(UiAction::NewProject, &mut state, elwt);
                            state.request_redraw();
                        } else if ctrl_held && matches!(&event.logical_key, Key::Character(c) if c == "Z") {
                            // CTRL+SHIFT+Z = redo (capital Z)
                            state.redo();
                            state.request_redraw();
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Delete)) {
                            dispatch(UiEvent::Delete, &mut state, elwt);
                        } else if matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                            handle_action(UiAction::ToggleActiveAudio, &mut state, elwt);
                            state.request_redraw();
                        } else if is_space_key(&event.logical_key) {
                            state.toggle_play_pause();
                            state.request_redraw();
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_delta = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                    };
                    if state.is_studio_mode() {
                        // In studio mode: scroll navigates between boucles
                        // Positive delta (scroll up) = forward (+1), negative delta (scroll down) = backward (-1)
                        let direction = if scroll_delta > 0.0 { 1 } else { -1 };
                        if ctrl_held {
                            // CTRL+SHIFT+scroll: jump to next/prev boucle
                            handle_action(UiAction::SeekToNextBoucle { direction }, &mut state, elwt);
                        } else {
                            // Regular scroll: seek by frames
                            let frame_delta = ui::scroll_delta_to_frames(scroll_delta, 10.0);
                            if frame_delta != 0 {
                                handle_action(UiAction::SeekRelative(frame_delta), &mut state, elwt);
                            }
                        }
                        state.request_redraw();
                    } else {
                        dispatch(UiEvent::Scroll {
                            x: cursor_pos.0, y: cursor_pos.1, delta: scroll_delta, fast: shift_held, ctrl: ctrl_held,
                        }, &mut state, elwt);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    cursor_pos = state.window_to_ui_position(position.x as f32, position.y as f32);
                    // Always dispatch mouse move (needed for panning in studio mode)
                    dispatch(UiEvent::MouseMove {
                        x: cursor_pos.0, y: cursor_pos.1,
                    }, &mut state, elwt);

                    // Update cursor icon if hover over active text
                    let is_text_cursor = {
                        let mut res = false;
                        if state.is_editing_text() {
                            if let Some(h) = state.hovered_line() {
                                if state.editing_line() == Some(h) {
                                    res = true;
                                }
                            }
                        }
                        res
                    };

                    if is_text_cursor {
                        window.set_cursor_icon(winit::window::CursorIcon::Text);
                    } else {
                        window.set_cursor_icon(winit::window::CursorIcon::Default);
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if !state.is_studio_mode() {
                        match button_state {
                            ElementState::Pressed => {
                                let now = Instant::now();
                                let is_double = last_click_time
                                    .map(|last| now.duration_since(last).as_millis() < 400)
                                    .unwrap_or(false);
                                last_click_time = Some(now);

                                if ctrl_held {
                                    dispatch(UiEvent::CtrlClick {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else if shift_held {
                                    dispatch(UiEvent::ShiftMousePress {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else if is_double {
                                    dispatch(UiEvent::DoubleClick {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                } else {
                                    dispatch(UiEvent::MousePress {
                                        x: cursor_pos.0, y: cursor_pos.1,
                                    }, &mut state, elwt);
                                }
                            }
                            ElementState::Released => {
                                dispatch(UiEvent::MouseRelease {
                                    x: cursor_pos.0, y: cursor_pos.1,
                                }, &mut state, elwt);
                                // Broadcast coalesced command on drag end
                                state.broadcast_finalize();
                            }
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Middle,
                    ..
                } => {
                    // Allow middle click panning in both editor and studio modes
                    match button_state {
                        ElementState::Pressed => {
                            if state.is_studio_mode() {
                                state.begin_timeline_pan(cursor_pos.0);
                            }
                            dispatch(UiEvent::MiddlePress {
                                x: cursor_pos.0, y: cursor_pos.1,
                            }, &mut state, elwt);
                        }
                        ElementState::Released => {
                            dispatch(UiEvent::MiddleRelease {
                                x: cursor_pos.0, y: cursor_pos.1,
                            }, &mut state, elwt);
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ref button_state,
                    button: MouseButton::Right,
                    ..
                } => {
                    if !state.is_studio_mode() && matches!(button_state, ElementState::Pressed) {
                        dispatch(UiEvent::ContextMenu {
                            x: cursor_pos.0,
                            y: cursor_pos.1,
                        }, &mut state, elwt);
                    }
                }
                WindowEvent::RedrawRequested => {
                    state.render();
                    if state.secondary_needs_continuous_redraw() {
                        state.request_secondary_redraw();
                    }
                }
                WindowEvent::DroppedFile(path) => {
                    let ext = path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if ["mp4", "mov", "avi", "mkv", "webm"].contains(&ext.as_str()) {
                        state.load_video(&path);
                        state.request_redraw();
                    }
                }
                _ => {}
                }
            }
            Event::AboutToWait => {
                let changed = state.tick_background();
                if changed || state.needs_redraw_now() {
                    state.request_redraw();
                    if state.has_secondary_display() {
                        state.request_secondary_redraw();
                    }
                }

                if let Some(deadline) = state.next_wake_deadline() {
                    let now = Instant::now();
                    elwt.set_control_flow(ControlFlow::WaitUntil(deadline.max(now)));
                } else {
                    elwt.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
            }
        })
        .expect("Event loop error");
}
