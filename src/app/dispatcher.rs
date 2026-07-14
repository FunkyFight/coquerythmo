use std::path::PathBuf;
use winit::event_loop::EventLoopWindowTarget;

use super::event_loop::new_project_reset_and_pick_video;
use super::event_loop::AppEvent;
use super::file_picker::{
    import_cappela_from_path, import_project_from_path, import_srt_from_path, open_dialog_filters,
    open_file_picker, project_or_video_dir, quick_save_existing, save_dialog_filters,
    save_project_as, video_or_project_dir,
};
use crate::application::command::TextCommand;
use crate::state::State;
use crate::ui::file_explorer::{FileExplorerMode, FilePickerIntent};
use crate::ui::primitives::{EventResponse, UiAction, UiEvent};
use crate::{config, export, i18n, packet, platform, video_export, video_proxy};
use std::sync::Arc;
use winit::dpi::LogicalSize;

pub(crate) struct CommandDispatcher;

pub(crate) fn handle_file_picker_selected(
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
            let _ = CommandDispatcher::dispatch(
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

impl CommandDispatcher {
    pub(crate) fn dispatch(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        match action {
            UiAction::CloseApp => return true,
            UiAction::CloseSecondaryDisplay => state.close_secondary_display(),
            UiAction::Undo => state.undo(),
            UiAction::Redo => state.redo(),
            UiAction::ExitStudioMode => state.exit_studio_mode(),
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
                if state.project_session.project_path.is_some() {
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
            UiAction::FinishSeek => {
                state.finish_seek();
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
                } else if state.project_session.project_path.is_none() {
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
                let Some(br_path) = state.project_session.project_path.clone() else {
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
                    let project_snap = state.project_session.project.snapshot();
                    if !export_original_audio && !export_instrumental_audio {
                        state.show_toast(i18n::t("toast.no_audio_export_selected"), 3.0);
                        return false;
                    }
                    let instrumental_audio_path = export_instrumental_audio
                        .then(|| {
                            project_snap
                                .settings()
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
                    let double_export_instrumental =
                        export_original_audio && export_instrumental_audio;
                    let source_audio_offset_frames =
                        project_snap.settings().source_audio_offset_frames;
                    let instrumental_audio_offset_frames =
                        project_snap.settings().instrumental_audio_offset_frames;
                    let progress =
                        std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0.0_f32.to_bits()));
                    let progress_for_ui = progress.clone();
                    let render_backend_status =
                        std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
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
                    match platform::window_builder()
                        .with_title(i18n::t("menu.tools.secondary_display"))
                        .with_inner_size(LogicalSize::new(1280.0, 720.0))
                        .with_window_icon(platform::app_icon())
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
            UiAction::SelectAll => {
                dispatch(UiEvent::SelectAll, state, elwt);
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
                if video_path.exists() && br_path.exists() {
                    state.project_session.project_path = Some(br_path.clone());
                    state.load_video(&video_path);
                    state.start_br_import(br_path);
                    log::info!("Loading recent project");
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
                    .collaboration
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
            UiAction::OpenPricingPage => {
                state.open_pricing_page();
            }
            UiAction::OpenDiscord => {
                if let Err(e) = open::that("https://discord.gg/fpdsUyWuwN") {
                    log::warn!("Failed to open Discord link: {e}");
                }
            }
            UiAction::SubscribePlan { plan } => {
                let lower = plan.to_lowercase();
                let lic_type = if lower.contains("école")
                    || lower.contains("school")
                    || lower.contains("escuela")
                    || lower.contains("organisme")
                    || lower.contains("organization")
                    || lower.contains("organización")
                    || lower.contains("structure")
                    || lower.contains("enterprise")
                {
                    "organisme".into()
                } else {
                    plan.clone()
                };
                crate::config::set_license("subscribed".into(), lic_type);
                state.rebuild_topbar();
                state.show_toast(format!("{} — merci pour votre soutien !", plan), 5.0);
            }
            UiAction::ActivateLicense { key } => {
                crate::config::set_license(key.clone(), "professionnelle".into());
                state.rebuild_topbar();
                state.show_toast(i18n::t("pricing.license_modal.activated").to_string(), 4.0);
            }
            UiAction::Text(command) => match command {
                TextCommand::SelectAll => dispatch(UiEvent::SelectAll, state, elwt),
                TextCommand::Copy => dispatch(UiEvent::Copy, state, elwt),
                TextCommand::Cut => dispatch(UiEvent::Cut, state, elwt),
                TextCommand::Paste => {
                    if let Some(text) = platform::clipboard_paste() {
                        dispatch(UiEvent::KeyInput { text }, state, elwt);
                    }
                }
                TextCommand::Undo => dispatch(UiEvent::UndoTextEdit, state, elwt),
                TextCommand::CursorLeft => dispatch(UiEvent::CursorLeft, state, elwt),
                TextCommand::CursorRight => dispatch(UiEvent::CursorRight, state, elwt),
                TextCommand::SelectLeft => dispatch(UiEvent::ShiftCursorLeft, state, elwt),
                TextCommand::SelectRight => dispatch(UiEvent::ShiftCursorRight, state, elwt),
                TextCommand::CursorUp => dispatch(UiEvent::CursorUp, state, elwt),
                TextCommand::CursorDown => dispatch(UiEvent::CursorDown, state, elwt),
                TextCommand::Delete => dispatch(
                    UiEvent::KeyInput {
                        text: "\x7f".into(),
                    },
                    state,
                    elwt,
                ),
            },
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
                if state.project_session.dirty && !state.project_session.project.is_empty() {
                    state.open_save_prompt();
                } else {
                    new_project_reset_and_pick_video(state, elwt);
                }
            }
            UiAction::NewProjectSave => {
                if state.project_session.project_path.is_some() {
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
                platform::clipboard_set(&text);
            }
            UiAction::SetClipboardAndUpdateLineText {
                clipboard,
                id,
                text,
            } => {
                platform::clipboard_set(&clipboard);
                state.update_line_text(id, text);
            }
            UiAction::SetClipboardAndUpdateCharacterName {
                clipboard,
                line_id,
                name,
            } => {
                platform::clipboard_set(&clipboard);
                state.update_character_name(line_id, name);
            }
            UiAction::SetClipboardAndUpdateLineNote {
                clipboard,
                line_id,
                note,
            } => {
                platform::clipboard_set(&clipboard);
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
            UiAction::SetToolMode(mode) => {
                state.set_tool_mode(mode);
            }
            UiAction::CycleBrushSize => {
                state.cycle_brush_size();
            }
            UiAction::ToggleEraser => {
                state.toggle_eraser();
            }
            UiAction::OpenBrushColorPicker => {
                state.open_brush_color_picker();
            }
            UiAction::CycleBrushColor { index, color } => {
                state.cycle_brush_color(index, color);
            }
            UiAction::AddDrawingStroke(stroke) => {
                state.add_drawing_stroke(stroke);
            }
            UiAction::EraseDrawingStrokes(ids) => {
                state.erase_drawing_strokes(ids);
            }
            UiAction::TransformStrokes {
                stroke_ids,
                old_points,
                new_points,
            } => {
                state.transform_drawing_strokes(stroke_ids, old_points, new_points);
            }
        }
        false
    }
}

pub(crate) fn dispatch(
    ui_event: UiEvent,
    state: &mut State,
    elwt: &winit::event_loop::EventLoopWindowTarget<AppEvent>,
) {
    let response = state.handle_ui_event(&ui_event);
    let response_changed_ui = !matches!(response, EventResponse::Ignored);
    let is_pointer_move = matches!(ui_event, UiEvent::MouseMove { .. });

    if let EventResponse::Action(action) = response {
        if CommandDispatcher::dispatch(action, state, elwt) {
            elwt.exit();
        }
    }

    if should_request_redraw(
        is_pointer_move,
        response_changed_ui,
        state.needs_continuous_redraw(),
    ) {
        state.request_redraw();
    }
}

fn should_request_redraw(
    is_pointer_move: bool,
    response_changed_ui: bool,
    continuous_redraw: bool,
) -> bool {
    // During playback/animation the paced redraw loop is already active.
    // Ignored raw mouse events must not bypass that pacing and create a render
    // storm at the mouse polling rate.
    !is_pointer_move || response_changed_ui || !continuous_redraw
}

#[cfg(test)]
mod tests {
    use super::should_request_redraw;

    #[test]
    fn ignored_pointer_moves_use_the_paced_redraw_loop() {
        assert!(!should_request_redraw(true, false, true));
        assert!(should_request_redraw(true, true, true));
        assert!(should_request_redraw(true, false, false));
        assert!(should_request_redraw(false, false, true));
    }
}
