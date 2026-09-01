use std::path::PathBuf;
use winit::event_loop::EventLoopWindowTarget;

use super::event_loop::AppEvent;
use super::event_loop::{close_project_reset, new_project_reset_and_pick_video};
use super::file_picker::{
    import_cappela_from_path, import_project_from_path, import_subtitle_from_path,
    open_dialog_filters, open_file_picker, open_file_picker_request, open_multiple_file_picker,
    project_or_video_dir, quick_save_existing, quick_save_existing_with_continuation,
    save_dialog_filters, save_project_as, save_project_as_with_continuation, video_or_project_dir,
};
use crate::application::command::{FilePickerIntent, FilePickerMode, TextCommand};
use crate::application::job_service::SaveContinuation;
use crate::config;
use crate::i18n;
use crate::packet::Packet;
use crate::platform;
use crate::state::State;
use crate::ui::primitives::{EventResponse, UiAction, UiEvent};
use crate::video_export;
use crate::video_export::capabilities::probe_video_duration;
use crate::video_proxy;
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
        FilePickerIntent::AddVideo => {
            if state.load_video(&path) {
                if let Some(project_path) = state.project_session.project_path.as_deref() {
                    if let Err(error) = video_proxy::delete_proxy(project_path) {
                        log::warn!(
                            "Failed to remove the previous proxy after replacement: {error}"
                        );
                    }
                    if let Err(error) = video_proxy::set_source_removed(project_path, false) {
                        log::warn!("Failed to link the replacement video: {error}");
                    }
                }
                state.project_session.dirty = true;
            }
        }
        FilePickerIntent::ComicDubsImage => state.comic_dubs_begin_image_import(path),
        FilePickerIntent::ComicDubsAudio => state.comic_dubs_begin_audio_import(path),
        FilePickerIntent::ComicDubsExport { configuration } => {
            state.start_comic_dubs_export(path, configuration)
        }
        FilePickerIntent::RecordingAudio => state.recording_begin_audio_import(path, None),
        FilePickerIntent::VoicelinesAudio => state.voicelines_begin_audio_import(path),
        FilePickerIntent::VoicelinesExportRegion {
            audio_id,
            region_id,
        } => state.voicelines_export_region_to(audio_id, region_id, path),
        FilePickerIntent::VoicelinesExportAll { audio_id } => {
            state.voicelines_export_all_to(audio_id, path)
        }
        FilePickerIntent::VoicelinesSaveSession => state.voicelines_save_session(path),
        FilePickerIntent::VoicelinesLoadSession => state.voicelines_load_session(path),
        FilePickerIntent::ImportProject => import_project_from_path(state, path),
        FilePickerIntent::ImportCappelaProject => import_cappela_from_path(state, path),
        FilePickerIntent::ImportSrtProject => import_subtitle_from_path(state, path),
        FilePickerIntent::ExportProject | FilePickerIntent::QuickSave => {
            save_project_as(state, path);
        }
        FilePickerIntent::NewProjectSave => {
            save_project_as_with_continuation(state, path, SaveContinuation::NewProject);
        }
        FilePickerIntent::CloseProjectSave => {
            save_project_as_with_continuation(state, path, SaveContinuation::CloseProject);
        }
        FilePickerIntent::ExitApplicationSave => {
            save_project_as_with_continuation(state, path, SaveContinuation::ExitApplication);
        }
        FilePickerIntent::VoiceActorIcon => {
            state.set_voice_actor_modal_icon_path(path.to_string_lossy().into_owned());
        }
        FilePickerIntent::ProjectInstrumentalAudio => {
            let path = path.to_string_lossy().into_owned();
            state.set_project_instrumental_audio_path(path.clone());
            let settings = state.project_session.project.settings();
            let scroll_speed = settings.scroll_speed;
            let reading_bar_offset_percent = settings.reading_bar_offset_percent;
            let highlight_read_word = settings.highlight_read_word;
            let scrolling_text_uses_character_color = settings.scrolling_text_uses_character_color;
            let show_text_emotion_lanes = settings.show_text_emotion_lanes;
            state.save_project_settings(
                scroll_speed,
                reading_bar_offset_percent,
                Some(path),
                highlight_read_word,
                scrolling_text_uses_character_color,
                show_text_emotion_lanes,
            );
            state.close_project_settings_modal();
        }
        FilePickerIntent::LanguageInstrumentalAudio { language_id } => {
            state.set_language_instrumental_audio(
                language_id,
                Some(path.to_string_lossy().into_owned()),
            );
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
        FilePickerIntent::ConfiguredExport { configuration } => {
            let _ = CommandDispatcher::dispatch(
                UiAction::StartConfiguredExportToPath {
                    output_path: path,
                    configuration,
                },
                state,
                elwt,
            );
        }
        FilePickerIntent::ExportRecordingTrack { track_id } => {
            let project = &state.project_session.recording_project;
            if project.track(track_id).is_none() {
                return;
            }

            // Get video duration for padding silence to match full video length
            let video_duration: f64 = state
                .video_path()
                .and_then(|p| probe_video_duration(p.as_ref()))
                .unwrap_or(0.0);

            let spec = crate::recording_mix::RecordingMixSpec {
                source_duration_seconds: video_duration.is_finite().then_some(video_duration),
                clips: project
                    .clips()
                    .filter(|c| c.track_id == track_id)
                    .filter_map(|clip| {
                        let asset_path = state
                            .project_session
                            .recording_asset_paths
                            .get(&clip.asset_id)?;
                        let fps = project.timeline_fps();
                        Some(crate::recording_mix::MixClip {
                            clip_id: clip.id,
                            track_id: clip.track_id,
                            path: asset_path.clone(),
                            source_start_seconds: clip.source_start_frame as f64 / fps,
                            duration_seconds: clip.duration_frames as f64 / fps,
                            timeline_start_seconds: clip.start_frame as f64 / fps,
                            volume: 1.0,
                        })
                    })
                    .collect(),
                sample_rate: 48_000,
                source_volume: 1.0,
                total_duration_seconds: video_duration.is_finite().then_some(video_duration),
            };

            if let Err(e) = crate::recording_mix::render_recording_mix(
                &spec,
                &path,
                &std::sync::atomic::AtomicBool::new(false),
            ) {
                state.recording_error(format!("Failed to export track: {e}"));
            } else {
                state.show_toast(crate::i18n::t("recording.track.exported"), 3.0);
            }
        }
    }
}

impl CommandDispatcher {
    pub(crate) fn announce_shortcut(action: &UiAction, state: &State) {
        if let Some(event) = crate::accessibility::event_for_keyboard_shortcut(action) {
            state.announce_shortcut_accessibility(event);
        }
    }

    pub(crate) fn dispatch_shortcut(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        // Pan is driven by holding Q/D. The key itself stays silent so
        // continuous panning does not flood the speech output.
        if matches!(&action, UiAction::BeginKeyboardPan { .. }) {
            return Self::dispatch_inner(action, state, elwt, false);
        }
        let navigates_lines = matches!(&action, UiAction::NavigateLines { .. });
        let container_title = match &action {
            UiAction::OpenRecentProjects => Some(crate::i18n::t("menu.project.recent")),
            UiAction::OpenMediaExplorer => Some(crate::i18n::t("media_explorer.title")),
            UiAction::OpenDropdown(crate::ui::primitives::ToolbarDropdown::Respirations) => {
                Some(crate::i18n::t("toolbar.respirations"))
            }
            UiAction::OpenDropdown(crate::ui::primitives::ToolbarDropdown::Reactions) => {
                Some(crate::i18n::t("toolbar.reactions"))
            }
            UiAction::OpenProxyModal => Some(crate::i18n::t("menu.tools.create_proxy")),
            UiAction::OpenSettings => Some(crate::i18n::t("settings.title")),
            UiAction::OpenProjectSettings => Some(crate::i18n::t(
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::ComicDubs
                {
                    "comic_dubs_settings.title"
                } else {
                    "project_settings.title"
                },
            )),
            UiAction::OpenExportModal => Some(crate::i18n::t("export_modal.title")),
            UiAction::OpenRenameCharacterModal => {
                Some(crate::i18n::t("rename_character_modal.title"))
            }
            UiAction::OpenLinesPanel => Some(crate::i18n::t("panel.lines.title")),
            UiAction::OpenRolesPanel => Some(crate::i18n::t("panel.roles.title")),
            _ => None,
        };
        let toggles_toolbar_list = matches!(&action, UiAction::OpenDropdown(_));
        let save_prompt_was_open = state.is_save_prompt_open();
        let should_exit = Self::dispatch_inner(action.clone(), state, elwt, false);
        let opened_save_prompt = !save_prompt_was_open && state.is_save_prompt_open();
        let container_first_label = match &action {
            UiAction::OpenRecentProjects => state.recent_projects_first_accessibility_label(),
            UiAction::OpenMediaExplorer => {
                Some(crate::i18n::t("media_explorer.tab.videos").to_string())
            }
            UiAction::OpenDropdown(dropdown) => {
                state.toolbar_dropdown_first_accessibility_label(dropdown)
            }
            UiAction::OpenProxyModal => state.proxy_modal_focus_label(),
            UiAction::OpenSettings => state.settings_modal_focus_label(),
            UiAction::OpenProjectSettings => state.project_settings_modal_focus_label(),
            UiAction::OpenExportModal => state.export_modal_focus_label(),
            UiAction::OpenRenameCharacterModal => state.rename_character_modal_focus_label(),
            UiAction::OpenLinesPanel | UiAction::OpenRolesPanel => Some(
                state
                    .ui_shell
                    .ui
                    .side_panel_first_accessibility_label(&state.project_session.project),
            ),
            _ => None,
        };
        if navigates_lines {
            if let Some(label) = state.selected_line_accessibility_label() {
                state.announce_shortcut_accessibility(
                    crate::accessibility::AccessibilityEvent::Selection { label },
                );
            }
        } else if opened_save_prompt {
            // The save prompt announced its title and initial Cancel button.
        } else if container_title.is_some() && container_first_label.is_some() {
            // The opening function already emitted one atomic announcement:
            // container name followed by its first focused item.
        } else if toggles_toolbar_list {
            // Closing the toolbar list already emitted its collapsed state.
        } else if matches!(&action, UiAction::StartEditingSelectedCharacter)
            && state.selected_line_accessibility_label().is_none()
        {
            state.announce_shortcut_accessibility(
                crate::accessibility::AccessibilityEvent::Error {
                    message: crate::i18n::t("accessibility.no_line_selected").to_string(),
                },
            );
        } else {
            // Announce after the action so a toast, selection change or modal
            // opening cannot replace the shortcut before NVDA receives it.
            Self::announce_shortcut(&action, state);
        }
        should_exit
    }

    pub(crate) fn dispatch(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
    ) -> bool {
        Self::dispatch_inner(action, state, elwt, true)
    }

    fn dispatch_inner(
        action: UiAction,
        state: &mut State,
        elwt: &EventLoopWindowTarget<AppEvent>,
        announce_action: bool,
    ) -> bool {
        let workspace_history = matches!(
            state.active_workspace(),
            crate::application::workspace_service::WorkspaceId::Voicelines
                | crate::application::workspace_service::WorkspaceId::ComicDubs
        ) && matches!(action, UiAction::Undo | UiAction::Redo);
        if state.active_workspace() != crate::application::workspace_service::WorkspaceId::Rythmo
            && action.mutates_rythmo_project()
            && !workspace_history
        {
            state.announce_accessibility(crate::accessibility::AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.rythmo_read_only").to_string(),
            });
            return false;
        }
        // Background tasks (export, proxy, project import) no longer freeze
        // the UI, but project-level actions that would race the worker stay
        // refused until the task row completes.
        if state.background_task_running() && action.blocked_during_background_task() {
            state.show_toast(i18n::t("toast.action_blocked_task"), 4.0);
            return false;
        }
        if announce_action {
            if let Some(event) = crate::accessibility::event_for_action(&action) {
                if state.is_ctrl_held() {
                    state.announce_shortcut_accessibility(event);
                } else {
                    state.announce_accessibility(event);
                }
            }
        }
        match action {
            UiAction::Accessibility(event) => {
                if state.is_ctrl_held() {
                    state.announce_shortcut_accessibility(event);
                } else {
                    state.announce_accessibility(event);
                }
            }
            UiAction::ActivateWorkspace(workspace) => state.activate_workspace(workspace),
            UiAction::ComicDubsImportImages => {
                open_multiple_file_picker(
                    state,
                    elwt,
                    "Importer des images Comic Dubs",
                    FilePickerIntent::ComicDubsImage,
                    open_dialog_filters(
                        "Images",
                        &["png", "jpg", "jpeg", "webp", "bmp", "gif", "ico"],
                    ),
                    project_or_video_dir(state),
                );
            }
            UiAction::ComicDubsImportAudios => {
                open_multiple_file_picker(
                    state,
                    elwt,
                    "Importer des audios Comic Dubs",
                    FilePickerIntent::ComicDubsAudio,
                    open_dialog_filters(
                        "Audio",
                        &["flac", "wav", "mp3", "ogg", "m4a", "aac", "opus"],
                    ),
                    project_or_video_dir(state),
                );
            }
            UiAction::ComicDubsSelectPage(page_id) => state.comic_dubs_select_page(page_id),
            UiAction::ComicDubsRemovePage(page_id) => state.comic_dubs_remove_page(page_id),
            UiAction::ComicDubsMovePage { page_id, delta } => {
                state.comic_dubs_move_page(page_id, delta)
            }
            UiAction::ComicDubsRemoveAudio(audio_id) => state.comic_dubs_remove_audio(audio_id),
            UiAction::ComicDubsAddBubble { page_id, points } => {
                state.comic_dubs_add_bubble(page_id, points)
            }
            UiAction::ComicDubsSetBubbleText { bubble_id, text } => {
                state.comic_dubs_set_bubble_text(bubble_id, text)
            }
            UiAction::ComicDubsSetBubbleColor { bubble_id, color } => {
                state.comic_dubs_set_bubble_color(bubble_id, color)
            }
            UiAction::ComicDubsSetBubbleFontSize {
                bubble_id,
                font_size,
            } => state.comic_dubs_set_bubble_font_size(bubble_id, font_size),
            UiAction::ComicDubsSetBubbleLetterSpacing { bubble_id, spacing } => {
                state.comic_dubs_set_bubble_letter_spacing(bubble_id, spacing)
            }
            UiAction::ComicDubsSetBubbleLineSpacing { bubble_id, spacing } => {
                state.comic_dubs_set_bubble_line_spacing(bubble_id, spacing)
            }
            UiAction::ComicDubsSetBubbleTextColor { bubble_id, color } => {
                state.comic_dubs_set_bubble_text_color(bubble_id, color)
            }
            UiAction::ComicDubsSetBubbleTextAlignment {
                bubble_id,
                alignment,
            } => state.comic_dubs_set_bubble_text_alignment(bubble_id, alignment),
            UiAction::ComicDubsSetBubbleTextStyle {
                bubble_id,
                bold,
                strikethrough,
                underline,
            } => state.comic_dubs_set_bubble_text_style(
                bubble_id,
                bold,
                strikethrough,
                underline,
            ),
            UiAction::ComicDubsSetBubblePoints { bubble_id, points } => {
                state.comic_dubs_set_bubble_points(bubble_id, points)
            }
            UiAction::ComicDubsOpenVertexEditor(bubble_id) => {
                state.open_comic_dubs_vertex_editor(bubble_id)
            }
            UiAction::ComicDubsCloseVertexEditor => state.close_comic_dubs_vertex_editor(),
            UiAction::ComicDubsSetVertexEditorPlayhead(at_ms) => {
                state.set_comic_dubs_vertex_editor_playhead(at_ms)
            }
            UiAction::ComicDubsToggleVertexEditorPreview => {
                state.toggle_comic_dubs_vertex_editor_preview();
            }
            UiAction::ComicDubsSetBubbleVertexKeyframe {
                bubble_id,
                at_ms,
                points,
            } => state.comic_dubs_set_bubble_vertex_keyframe(bubble_id, at_ms, points),
            UiAction::ComicDubsRemoveBubbleVertexKeyframe { bubble_id, at_ms } => {
                state.comic_dubs_remove_bubble_vertex_keyframe(bubble_id, at_ms)
            }
            UiAction::ComicDubsAssignAudio {
                bubble_id,
                audio_id,
            } => state.comic_dubs_assign_audio(bubble_id, audio_id),
            UiAction::ComicDubsRemoveBubble(bubble_id) => state.comic_dubs_remove_bubble(bubble_id),
            UiAction::ComicDubsMoveBubble { bubble_id, delta } => {
                state.comic_dubs_move_bubble(bubble_id, delta)
            }
            UiAction::VoicelinesImportAudio => {
                let filters = open_dialog_filters(
                    "Audio",
                    &["flac", "wav", "mp3", "ogg", "m4a", "aac", "opus"],
                );
                open_multiple_file_picker(
                    state,
                    elwt,
                    "Ajouter un audio aux Voicelines",
                    FilePickerIntent::VoicelinesAudio,
                    filters,
                    project_or_video_dir(state),
                );
            }
            UiAction::VoicelinesSelectAudio(id) => state.voicelines_select_audio(id),
            UiAction::VoicelinesRemoveAudio(id) => state.voicelines_remove_audio(id),
            UiAction::VoicelinesAddRegion { start_ms, end_ms } => {
                state.voicelines_add_region(start_ms, end_ms)
            }
            UiAction::VoicelinesMoveRegion {
                region_id,
                start_ms,
                end_ms,
            } => state.voicelines_move_region(region_id, start_ms, end_ms),
            UiAction::VoicelinesSelectRegion(selected) => {
                state.ui_shell.ui.set_voicelines_selected_region(selected)
            }
            UiAction::VoicelinesRenameRegion { region_id, name } => {
                state.voicelines_rename_region(region_id, &name)
            }
            UiAction::VoicelinesJoinRegions(region_ids) => {
                state.voicelines_join_regions(region_ids)
            }
            UiAction::VoicelinesDeleteRegion(region_id) => {
                state.voicelines_delete_region(region_id)
            }
            UiAction::VoicelinesSetNamingPattern(pattern) => {
                state.voicelines_set_naming_pattern(pattern)
            }
            UiAction::VoicelinesAutoDetect => state.voicelines_auto_detect(),
            UiAction::VoicelinesPlayRegion(region_id) => state.voicelines_play_region(region_id),
            UiAction::VoicelinesExportRegion(region_id) => {
                if let Some(request) = state.voicelines_export_region_request(region_id) {
                    open_file_picker_request(state, elwt, request);
                }
            }
            UiAction::VoicelinesExportAll => {
                if let Some(request) = state.voicelines_export_all_request() {
                    open_file_picker_request(state, elwt, request);
                }
            }
            UiAction::VoicelinesSendAudio {
                audio_id,
                workspace,
            } => state.voicelines_send_audio(audio_id, workspace),
            UiAction::VoicelinesUpdateAudio {
                audio_id,
                workspace,
            } => state.voicelines_update_audio(audio_id, workspace),
            UiAction::VoicelinesSaveSession => {
                if state.project_session.project_path.is_some() {
                    quick_save_existing(state);
                } else {
                    let request = state.voicelines_save_request();
                    open_file_picker_request(state, elwt, request);
                }
            }
            UiAction::VoicelinesLoadSession => {
                let request = state.voicelines_load_request();
                open_file_picker_request(state, elwt, request);
            }
            UiAction::RecordingChooseSolo => state.recording_choose_solo(),
            UiAction::RecordingChooseOnline => state.recording_choose_online(),
            UiAction::RecordingSetTool(tool) => state.recording_set_tool(tool),
            UiAction::RecordingAddTrack => state.recording_add_track(),
            UiAction::RecordingRemoveTrack(track_id) => state.recording_remove_track(track_id),
            UiAction::RecordingBeginRenameTrack(track_id) => {
                state.recording_begin_rename_track(track_id)
            }
            UiAction::RecordingRenameTrack { track_id, name } => {
                state.recording_rename_track(track_id, name)
            }
            UiAction::RecordingToggleTrackMute(track_id) => {
                state.recording_toggle_track_mute(track_id)
            }
            UiAction::RecordingToggleTrackSolo(track_id) => {
                state.recording_toggle_track_solo(track_id)
            }
            UiAction::RecordingArmTrack(track_id) => state.recording_arm_track(track_id),
            UiAction::RecordingSetTrackVolume { track_id, volume } => {
                state.recording_set_track_volume(track_id, volume)
            }
            UiAction::RecordingAdjustTrackVolume { track_id, delta } => {
                state.recording_adjust_track_volume(track_id, delta)
            }
            UiAction::RecordingExportTrack(track_id) => {
                if let Some(request) = state.recording_export_track(track_id) {
                    open_file_picker_request(state, elwt, request);
                }
            }
            UiAction::RecordingCutClip { clip_id, at_frame } => {
                state.recording_cut_clip(clip_id, at_frame)
            }
            UiAction::RecordingSelectClip { clip_id, additive } => {
                state.recording_select_clip(clip_id, additive)
            }
            UiAction::RecordingSelectAsset(asset_id) => state.recording_select_asset(asset_id),
            UiAction::RecordingSendAssetToVoicelines(asset_id) => {
                state.recording_send_asset_to_voicelines(asset_id)
            }
            UiAction::RecordingDeleteSelectedAsset => state.recording_delete_selected_asset(),
            UiAction::RecordingPlaceAsset {
                asset_id,
                track_id,
                start_frame,
            } => state.recording_place_asset(asset_id, track_id, start_frame),
            UiAction::RecordingMoveSelectedClips {
                track_id,
                delta_frames,
            } => state.recording_move_selected_clips(track_id, delta_frames),
            UiAction::RecordingDeleteSelectedClips => state.recording_delete_selected_clips(),
            UiAction::RecordingStartCapture => state.recording_start_capture(),
            UiAction::RecordingStopCapture => state.recording_stop_capture(),
            UiAction::OpenRecordingActorMenu => state.open_recording_actor_menu(),
            UiAction::OpenRecordingInputDeviceModal => state.open_recording_input_device_modal(),
            UiAction::RequestActorsOpenMicrophone => state.request_actors_open_microphone(),
            UiAction::RequestActorsTransferProject => state.request_actors_project_transfer(),
            UiAction::RequestActorsTransferDisplaySettings => {
                state.request_actors_transfer_display_settings()
            }
            UiAction::RequestActorsCloseProjectTransferWaiting => {
                state.request_actors_close_project_transfer_waiting()
            }
            UiAction::ProjectTransferAccept => state.respond_to_project_transfer("accepted"),
            UiAction::ProjectTransferSaveAndAccept => {
                state.respond_to_project_transfer("saving");
                if !quick_save_existing_with_continuation(
                    state,
                    SaveContinuation::ProjectTransferAccept,
                ) {
                    state.retry_project_transfer_after_save_failure();
                }
            }
            UiAction::ProjectTransferReplace => state.respond_to_project_transfer("accepted"),
            UiAction::ProjectTransferRefuse => state.respond_to_project_transfer("refused"),
            UiAction::SetRecordingInputDevice(device) => state.set_recording_input_device(device),
            UiAction::RecordingToggleSharedAudio => state.recording_toggle_shared_audio(),
            UiAction::RecordingCycleLanguage => state.recording_cycle_language(),
            UiAction::CopyQuickHostLink => {
                state.copy_protocol_link_to_clipboard(crate::protocol::ProtocolKind::Host);
            }
            UiAction::CopyQuickJoinLink => {
                state.copy_protocol_link_to_clipboard(crate::protocol::ProtocolKind::Join);
            }
            UiAction::OpenRoomInvitation => state.open_room_invitation(),
            UiAction::CopyRoomCode => state.copy_room_code_to_clipboard(),
            UiAction::CloseApp => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.close_blocked_saving"), 5.0);
                } else {
                    return true;
                }
            }
            UiAction::CloseSecondaryDisplay => state.close_secondary_display(),
            UiAction::Undo => {
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::Voicelines
                {
                    state.voicelines_undo();
                } else if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::ComicDubs
                {
                    state.comic_dubs_undo();
                } else {
                    state.undo();
                }
            }
            UiAction::Redo => {
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::Voicelines
                {
                    state.voicelines_redo();
                } else if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::ComicDubs
                {
                    state.comic_dubs_redo();
                } else {
                    state.redo();
                }
            }
            UiAction::AddVideo => {
                let filters = open_dialog_filters("Video", &["mp4", "mov", "avi", "mkv", "webm"]);
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.video.title"),
                    FilePickerMode::Open,
                    FilePickerIntent::AddVideo,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::RecordingImportAudio => {
                let filters = open_dialog_filters(
                    i18n::t("recording.audio.filter"),
                    &["flac", "wav", "mp3", "ogg", "m4a", "aac", "opus"],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("recording.audio.import"),
                    FilePickerMode::Open,
                    FilePickerIntent::RecordingAudio,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::RecordingConfirmAudioImport {
                path,
                username,
                placement,
            } => state.recording_import_audio(path, username, placement),
            UiAction::ExportProject => {
                let filters = save_dialog_filters(
                    "Projet Coquerythmo",
                    &[crate::project_archive::PROJECT_EXTENSION],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.project_save.title"),
                    FilePickerMode::Save,
                    FilePickerIntent::ExportProject,
                    filters,
                    project_or_video_dir(state),
                    Some(crate::project_archive::PROJECT_EXTENSION),
                );
            }
            UiAction::ImportProject => {
                let filters = open_dialog_filters(
                    "Projet Coquerythmo",
                    &[crate::project_archive::PROJECT_EXTENSION, "json"],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.import.title"),
                    FilePickerMode::Open,
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
                    elwt,
                    i18n::t("picker.import.cappela.title"),
                    FilePickerMode::Open,
                    FilePickerIntent::ImportCappelaProject,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::ImportSrtProject => {
                let filters = open_dialog_filters(
                    "Sous-titres JSON / SRT / ASS / DETX",
                    &["json", "srt", "ass", "detx"],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.import.srt.title"),
                    FilePickerMode::Open,
                    FilePickerIntent::ImportSrtProject,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::ImportSubtitles => {
                let filters = open_dialog_filters(
                    "Sous-titrages et projets",
                    &[
                        crate::project_archive::PROJECT_EXTENSION,
                        "json",
                        "srt",
                        "ass",
                        "detx",
                    ],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.import.srt.title"),
                    FilePickerMode::Open,
                    FilePickerIntent::ImportSrtProject,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::QuickSave => {
                if state.project_session.project_path.is_some() {
                    quick_save_existing(state);
                } else {
                    let filters = save_dialog_filters(
                        "Projet Coquerythmo",
                        &[crate::project_archive::PROJECT_EXTENSION],
                    );
                    open_file_picker(
                        state,
                        elwt,
                        i18n::t("picker.project_save.title"),
                        FilePickerMode::Save,
                        FilePickerIntent::QuickSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
                    );
                }
            }
            UiAction::TogglePlayPause => {
                state.toggle_play_pause();
            }
            UiAction::SetVolume(vol) => {
                state.set_volume(vol);
            }
            UiAction::AdjustVolume(delta) => {
                // Text editing owns every arrow key. Keep this guard at the
                // command boundary as well as in the event loop so a stale
                // modifier state can never turn a caret move into a volume
                // change.
                if !state.is_rythmo_text_editing() {
                    let target = (state.ui_shell.ui.volume() + delta).clamp(0.0, 1.0);
                    state.set_volume(target);
                }
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
            UiAction::CreateLineAtTrack { track } => {
                let line_id = state.create_line_at_track(track);
                state.start_editing_line(line_id);
            }
            UiAction::SelectLineAtPlayhead => {
                state.select_line_at_playhead();
            }
            UiAction::NavigateLines { direction } => {
                state.navigate_lines(direction);
            }
            UiAction::ClearLineSelection => {
                state.clear_line_selection();
            }
            UiAction::SetSelectedLineStartAtPlayhead => {
                state.set_selected_line_start_at_playhead();
            }
            UiAction::SetSelectedLineEndAtPlayhead => {
                state.set_selected_line_end_at_playhead();
            }
            UiAction::StartEditingSelectedLine => {
                state.start_editing_selected_line();
            }
            UiAction::StartEditingSelectedCharacter => {
                state.start_editing_selected_character();
            }
            UiAction::BeginKeyboardPan { direction } => {
                state.begin_keyboard_pan(direction);
            }
            UiAction::EndKeyboardPan => {
                state.end_keyboard_pan();
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
            UiAction::MoveSelectedLineTrack { direction } => {
                state.move_selected_line_track(direction);
            }
            UiAction::NudgeSelectedLines { delta_frames } => {
                state.nudge_selected_lines(delta_frames);
            }
            UiAction::MoveLines { moves } => {
                state.move_lines(moves);
            }
            UiAction::AddDetection {
                line_id,
                kind,
                media_tick,
                target,
            } => {
                state.add_detection(line_id, kind, media_tick, target);
            }
            UiAction::GenerateDetectionSigns { line_id } => {
                state.generate_detection_signs(line_id);
            }
            UiAction::SetLinePresence { line_id, presence } => {
                state.set_line_presence(line_id, presence);
            }
            UiAction::MoveDetection {
                address,
                media_tick,
            } => {
                state.move_detection(address, media_tick);
            }
            UiAction::ResizeDetection {
                address,
                media_tick,
                duration,
            } => {
                state.resize_detection(address, media_tick, duration);
            }
            UiAction::DeleteDetection { address } => {
                state.delete_detection(address);
            }
            UiAction::NudgeSelectedDetection { delta_ticks } => {
                state.nudge_selected_detection(delta_ticks);
            }
            UiAction::NudgeSelectedSyncAnchor { delta_graphemes } => {
                state.nudge_selected_sync_anchor(delta_graphemes);
            }
            UiAction::ToggleSelectedSyncAffinity => {
                state.toggle_selected_sync_affinity();
            }
            UiAction::MoveSyncAnchor {
                address,
                grapheme_boundary,
            } => {
                state.move_sync_anchor(address, grapheme_boundary);
            }
            UiAction::AddSyncPointAtPlayhead => {
                state.add_sync_point_at_playhead();
            }
            UiAction::UpdateLineText { id, text } => {
                state.update_line_text(id, text);
            }
            UiAction::OpenTextEmotionMenu => {
                state.open_text_emotion_menu();
                state.announce_accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: format!(
                            "{} : {}",
                            crate::i18n::t("text_emotion.menu"),
                            crate::i18n::t("text_emotion.remove")
                        ),
                    },
                );
            }
            UiAction::SetTextEmotion {
                line_id,
                range,
                emotion,
            } => {
                state.set_text_emotion(line_id, range, emotion);
                let label = emotion
                    .map(|emotion| crate::i18n::t(emotion.i18n_key()))
                    .unwrap_or_else(|| crate::i18n::t("text_emotion.remove"));
                state.announce_accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: label.to_string(),
                    },
                );
            }
            UiAction::SetCharacter {
                line_id,
                name,
                color,
            } => {
                state.set_character(line_id, name, color);
                state.announce_character(line_id);
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
            UiAction::OpenAutomation => {
                state.open_automation();
            }
            UiAction::CloseAutomation => {
                state.close_automation();
            }
            UiAction::OpenLinesPanel => state.open_lines_panel(),
            UiAction::OpenRolesPanel => state.open_roles_panel(),
            UiAction::CloseSidePanel => state.close_side_panel(),
            UiAction::DeleteSidePanelLines { line_ids } => {
                state.delete_lines_by_ids(line_ids);
            }
            UiAction::CopySidePanelLines { line_ids, cut } => {
                state.copy_lines_by_ids(line_ids, cut);
            }
            UiAction::SetLinesRole {
                line_ids,
                name,
                color,
            } => {
                state.set_lines_role(line_ids, name, color);
            }
            UiAction::SetRoleColor { role, color } => {
                state.set_role_color(role, color);
            }
            UiAction::AutomationAddNode { kind, x, y } => {
                state.automation_add_node(kind, x, y);
            }
            UiAction::AutomationAddConnectedNode {
                kind,
                x,
                y,
                from_node,
                edge_kind,
                branch,
            } => {
                state.automation_add_connected_node(kind, x, y, from_node, edge_kind, branch);
            }
            UiAction::AutomationMoveNode { node_id, x, y } => {
                state.automation_move_node(node_id, x, y);
            }
            UiAction::AutomationDeleteNode { node_id } => {
                state.automation_delete_node(node_id);
            }
            UiAction::AutomationConnect {
                from_node,
                kind,
                branch,
                to_node,
            } => {
                state.automation_connect(from_node, kind, branch, to_node);
            }
            UiAction::AutomationDisconnect {
                from_node,
                kind,
                branch,
            } => {
                state.automation_disconnect(from_node, kind, branch);
            }
            UiAction::AutomationAddRole { node_id, role } => {
                state.automation_add_role(node_id, role);
            }
            UiAction::AutomationRemoveRole { node_id, role } => {
                state.automation_remove_role(node_id, role);
            }
            UiAction::AutomationSetTrack { node_id, track } => {
                state.automation_set_track(node_id, track);
            }
            UiAction::AutomationSetNodeEnabled { node_id, enabled } => {
                state.automation_set_node_enabled(node_id, enabled);
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
                    elwt,
                    i18n::t("picker.voice_actor_icon.title"),
                    FilePickerMode::Open,
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
                if matches!(
                    kind,
                    crate::rythmo_line::MarkerKind::LiaisonLeft
                        | crate::rythmo_line::MarkerKind::LiaisonRight
                ) {
                    state.add_ambiance_line(kind);
                } else {
                    state.add_marker(kind);
                }
            }
            UiAction::AddQuickLine { text } => {
                state.add_quick_line(text);
            }
            UiAction::OpenExportModal => {
                state.open_export_modal();
            }
            UiAction::OpenMediaExplorer => state.open_media_explorer(),
            UiAction::ToggleFileTree => state.toggle_file_tree(),
            UiAction::CloseFileTree => state.close_file_tree(),
            UiAction::MediaVideoUse { id } => state.use_media_video(id),
            UiAction::MediaVideoSetDefault { id } => state.set_default_media_video_by_id(id),
            UiAction::MediaVideoRemove { id } => state.remove_media_video(id),
            UiAction::MediaVideoRename { id, name } => state.rename_media_video(id, name),
            UiAction::MediaVideoBeginRename { id } => state.begin_rename_media_video(id),
            UiAction::MediaVideoCreateProxy { id } => state.create_proxy_for_media(id),
            UiAction::MediaVideoAssociateProxy { proxy_id, source_id } => {
                state.associate_proxy(proxy_id, source_id)
            }
            UiAction::MediaVideoDissociateProxy { id } => state.dissociate_proxy(id),
            UiAction::MediaAudioAdd { path } => state.import_audio(path),
            UiAction::MediaAudioRemove { id } => state.remove_audio(id),
            UiAction::MediaAudioRename { id, name } => state.rename_media_audio(id, name),
            UiAction::MediaAudioBeginRename { id } => state.begin_rename_media_audio(id),
            UiAction::MediaReorderVideo { id, to_index } => {
                state.reorder_media_video(id, to_index)
            }
            UiAction::MediaReorderAudio { id, to_index } => {
                state.reorder_media_audio(id, to_index)
            }
            UiAction::LanguageReorder { id, to_index } => state.reorder_language(id, to_index),
            UiAction::LanguageBeginRename { id } => state.begin_rename_language(id),
            UiAction::SetLanguageInstrumentalAudioPath { id, path } => {
                state.set_language_instrumental_audio(id, Some(path))
            }
            UiAction::SetLanguageInstrumentalAudioByMediaId { band_id, media_id } => {
                state.set_language_instrumental_audio_by_media_id(band_id, media_id)
            }
            UiAction::CreateLanguage { name } => state.create_language(name),
            UiAction::RenameLanguage { id, name } => state.rename_language(id, name),
            UiAction::DeleteLanguage { id } => state.delete_language(id),
            UiAction::SelectLanguage { id } => state.select_language(id),
            UiAction::SetLanguageSyllableLanguage { id, language } => {
                state.set_language_syllable_language(id, language)
            }
            UiAction::PickLanguageInstrumentalAudio { id } => {
                let filters = open_dialog_filters(
                    "Audio",
                    &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.instrumental_audio.title"),
                    FilePickerMode::Open,
                    FilePickerIntent::LanguageInstrumentalAudio { language_id: id },
                    filters,
                    video_or_project_dir(state),
                    None,
                );
            }
            UiAction::ClearLanguageInstrumentalAudio { id } => {
                state.set_language_instrumental_audio(id, None);
            }
            UiAction::SwitchMediaVideo { use_proxy } => state.switch_media_video(use_proxy),
            UiAction::SetDefaultMediaVideo { use_proxy } => {
                state.set_default_media_video(use_proxy)
            }
            UiAction::DeleteMediaVideo { use_proxy } => state.delete_media_video(use_proxy),
            UiAction::SaveExportConfiguration { configuration } => {
                state.save_export_configuration(configuration);
            }
            UiAction::StartConfiguredExport { configuration } => {
                state.save_export_configuration(configuration.clone());
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::ComicDubs
                {
                    let filters = if configuration.comic_dubs_pages_zip {
                        save_dialog_filters("Archive MP4 par page", &["zip"])
                    } else if configuration.comic_dubs_alpha {
                        save_dialog_filters("Vidéo alpha ProRes", &["mov"])
                    } else {
                        save_dialog_filters("Vidéo MP4", &["mp4"])
                    };
                    open_file_picker(
                        state,
                        elwt,
                        i18n::t("picker.delivery_export.title"),
                        FilePickerMode::Save,
                        FilePickerIntent::ComicDubsExport { configuration },
                        filters,
                        project_or_video_dir(state),
                        None,
                    );
                    return false;
                }
                let filters = save_dialog_filters(
                    "Export Coquerythmo",
                    &[
                        "mp4", "json", "srt", "ass", "detx", "mp3", "wav", "csv", "pdf",
                    ],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.delivery_export.title"),
                    FilePickerMode::Save,
                    FilePickerIntent::ConfiguredExport { configuration },
                    filters,
                    video_or_project_dir(state),
                    None,
                );
            }
            UiAction::StartConfiguredExportToPath {
                output_path,
                configuration,
            } => {
                state.start_configured_export(output_path, configuration);
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
            UiAction::CreateProxy {
                width,
                height,
                crf,
                encoder,
            } => {
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
                        encoder,
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
                state.announce_shortcut_accessibility(
                    crate::accessibility::AccessibilityEvent::Opened {
                        label: format!(
                            "{} {}",
                            i18n::t("progress.proxy"),
                            i18n::t("progress.cancel_hint")
                        ),
                    },
                );
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
                    elwt,
                    i18n::t("picker.export_mp4.title"),
                    FilePickerMode::Save,
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
                            false,
                            double_export_instrumental,
                            0.0,
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
                    state.announce_shortcut_accessibility(
                        crate::accessibility::AccessibilityEvent::Opened {
                            label: format!(
                                "{} {}",
                                i18n::t("progress.exporting"),
                                i18n::t("progress.cancel_hint")
                            ),
                        },
                    );
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
                    elwt,
                    i18n::t("picker.instrumental_audio.title"),
                    FilePickerMode::Open,
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
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::Recording
                    && !state.can_open_recording_daw()
                {
                    return false;
                }
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
                        Ok(window) => state.open_secondary_display(
                            Arc::new(window),
                            if state.active_workspace()
                                == crate::application::workspace_service::WorkspaceId::Recording
                            {
                                crate::application::window_service::SecondaryWindowKind::Daw
                            } else {
                                crate::application::window_service::SecondaryWindowKind::Video
                            },
                        ),
                        Err(e) => log::error!("Failed to create secondary display window: {e}"),
                    }
                }
            }
            UiAction::DeleteSelected => {
                if state.has_selected_detection() {
                    state.delete_selected_detection();
                } else {
                    state.delete_selected();
                }
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
                state.announce_selected_line();
            }
            UiAction::OpenRecentProject {
                video_path,
                br_path,
            } => {
                let is_bundle = br_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case(crate::project_archive::PROJECT_EXTENSION)
                    });
                if is_bundle && br_path.exists() {
                    state.start_br_import(br_path);
                    log::info!("Loading recent portable project");
                } else if br_path.exists() && video_proxy::source_is_removed(&br_path) {
                    state.clear_video_for_new_project();
                    state.start_br_import(br_path);
                    log::info!("Loading recent project without linked video");
                } else if video_path.exists() && br_path.exists() {
                    let previous_project_path = state.project_session.project_path.clone();
                    state.project_session.project_path = Some(br_path.clone());
                    if state.load_video(&video_path) {
                        state.start_br_import(br_path);
                        log::info!("Loading recent legacy project");
                    } else {
                        state.project_session.project_path = previous_project_path;
                    }
                } else {
                    log::warn!("Recent project files missing, skipping");
                }
            }
            UiAction::RemoveRecentProject {
                video_path,
                br_path,
            } => {
                config::remove_recent_project(&video_path, &br_path);
                state.rebuild_topbar_for_network();
            }
            UiAction::OpenRecentProjects => {
                state.open_recent_projects();
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
                let project_huuid = state
                    .project_session
                    .huuid
                    .as_ref()
                    .filter(|_| {
                        state.project_session.project_path.is_some() && !state.project_session.dirty
                    })
                    .map(ToString::to_string);
                if room_code.is_none() && project_huuid.is_none() {
                    let message = i18n::t("toast.network_requires_saved_project");
                    state.show_toast(message, 6.0);
                    state.announce_accessibility(crate::accessibility::AccessibilityEvent::Error {
                        message: message.to_string(),
                    });
                    return false;
                }
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
                    Packet::JoinRoom {
                        code,
                        username,
                        project_huuid,
                    }
                } else {
                    Packet::CreateRoom {
                        username,
                        project_huuid: project_huuid.expect("create room requires a saved project"),
                    }
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
            UiAction::PickTemporaryDirectory => {
                let current = crate::config::temporary_directory();
                let mut dialog =
                    rfd::FileDialog::new().set_title(i18n::t("settings.temporary_directory"));
                if current.is_dir() {
                    dialog = dialog.set_directory(&current);
                } else if let Some(parent) = current.parent().filter(|path| path.is_dir()) {
                    dialog = dialog.set_directory(parent);
                }
                if let Some(path) = dialog.pick_folder() {
                    state.set_settings_temporary_directory(path);
                }
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
                temporary_directory,
                show_controls_hint,
            } => {
                let rythmo_font = crate::config::get().ui.rythmo_font.clone();
                crate::config::save_settings(lang, rythmo_font, temporary_directory);
                crate::config::set_show_controls_hint(show_controls_hint);
                state.recording_runtime.refresh_temporary_directory();
                state.close_settings_modal();
            }
            UiAction::SaveProjectSettings {
                rythmo_font,
                scroll_speed,
                reading_bar_offset_percent,
                instrumental_audio_path,
                highlight_read_word,
                scrolling_text_uses_character_color,
                show_text_emotion_lanes,
            } => {
                let (lang, old_font) = {
                    let config = crate::config::get();
                    (config.lang.clone(), config.ui.rythmo_font.clone())
                };
                let font_changed = old_font != rythmo_font;
                crate::config::save_settings(
                    lang,
                    rythmo_font,
                    crate::config::temporary_directory(),
                );
                state.save_project_settings(
                    scroll_speed,
                    reading_bar_offset_percent,
                    instrumental_audio_path,
                    highlight_read_word,
                    scrolling_text_uses_character_color,
                    show_text_emotion_lanes,
                );
                if font_changed {
                    crate::vector_text::clear_project_font();
                    state.render.ui_renderer.clear_text_cache();
                }
            }
            UiAction::SaveComicDubsSettings {
                font_family,
                bubble_duration_ms,
                page_duration_ms,
                default_font_size,
            } => state.save_comic_dubs_settings(
                font_family,
                bubble_duration_ms,
                page_duration_ms,
                default_font_size,
            ),
            UiAction::ToggleActiveAudio => {
                if state.active_workspace()
                    == crate::application::workspace_service::WorkspaceId::Recording
                {
                    state.recording_toggle_shared_audio();
                } else {
                    state.toggle_active_audio();
                }
            }
            UiAction::OffsetActiveAudioBy(delta_frames) => {
                state.offset_active_audio_by(delta_frames);
            }
            UiAction::NewProject => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.project_change_blocked_saving"), 5.0);
                } else if state.project_session.dirty {
                    state
                        .open_save_prompt(crate::ui::save_prompt_modal::SavePromptKind::NewProject);
                } else {
                    new_project_reset_and_pick_video(state, elwt);
                }
            }
            UiAction::NewProjectSave => {
                if state.project_session.project_path.is_some() {
                    quick_save_existing_with_continuation(state, SaveContinuation::NewProject);
                } else {
                    let filters = save_dialog_filters(
                        "Projet Coquerythmo",
                        &[crate::project_archive::PROJECT_EXTENSION],
                    );
                    open_file_picker(
                        state,
                        elwt,
                        i18n::t("picker.project_save.title"),
                        FilePickerMode::Save,
                        FilePickerIntent::NewProjectSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
                    );
                }
            }
            UiAction::NewProjectDiscard => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.project_change_blocked_saving"), 5.0);
                } else {
                    new_project_reset_and_pick_video(state, elwt);
                }
            }
            UiAction::CloseProject => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.project_change_blocked_saving"), 5.0);
                } else if state.project_session.dirty {
                    state.open_save_prompt(
                        crate::ui::save_prompt_modal::SavePromptKind::CloseProject,
                    );
                } else {
                    close_project_reset(state);
                }
            }
            UiAction::CloseProjectSave => {
                if state.protocol_is_awaiting_close() {
                    // The save prompt was shown as part of a `coquerythmo://`
                    // quick-setup flow: run our quick-save with the protocol
                    // continuation so that once the file is written we close
                    // it, load the linked project and create the room.
                    quick_save_existing_with_continuation(state, SaveContinuation::ProtocolHost);
                } else if state.project_session.project_path.is_some() {
                    quick_save_existing_with_continuation(state, SaveContinuation::CloseProject);
                } else {
                    let filters = save_dialog_filters(
                        "Projet Coquerythmo",
                        &[crate::project_archive::PROJECT_EXTENSION],
                    );
                    open_file_picker(
                        state,
                        elwt,
                        i18n::t("picker.project_save.title"),
                        FilePickerMode::Save,
                        FilePickerIntent::CloseProjectSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
                    );
                }
            }
            UiAction::CloseProjectDiscard => {
                if state.protocol_is_awaiting_close() {
                    // Part of a coquerythmo:// quick-setup flow: close without
                    // saving and immediately import the linked project.
                    close_project_reset(state);
                    state.protocol_discard_current_and_continue();
                } else {
                    close_project_reset(state);
                }
            }
            UiAction::ExitApplication => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.close_blocked_saving"), 5.0);
                } else if state.project_session.dirty {
                    state.open_save_prompt(
                        crate::ui::save_prompt_modal::SavePromptKind::ExitApplication,
                    );
                } else {
                    return true;
                }
            }
            UiAction::ExitApplicationSave => {
                if state.project_session.project_path.is_some() {
                    quick_save_existing_with_continuation(state, SaveContinuation::ExitApplication);
                } else {
                    let filters = save_dialog_filters(
                        "Projet Coquerythmo",
                        &[crate::project_archive::PROJECT_EXTENSION],
                    );
                    open_file_picker(
                        state,
                        elwt,
                        i18n::t("picker.project_save.title"),
                        FilePickerMode::Save,
                        FilePickerIntent::ExitApplicationSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
                    );
                }
            }
            UiAction::ExitApplicationDiscard => return true,
            UiAction::ToggleScreenReader => {
                state.toggle_screen_reader();
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

    match response {
        EventResponse::Action(action) => {
            if CommandDispatcher::dispatch(action, state, elwt) {
                elwt.exit();
            }
        }
        EventResponse::Actions(actions) => {
            for action in actions {
                if CommandDispatcher::dispatch(action, state, elwt) {
                    elwt.exit();
                    break;
                }
            }
        }
        EventResponse::Ignored | EventResponse::Consumed => {}
    }

    if should_request_redraw(
        &ui_event,
        response_changed_ui,
        state.needs_continuous_redraw(),
    ) {
        state.request_redraw();
    }
}

pub(crate) fn dispatch_secondary_daw(
    ui_event: UiEvent,
    state: &mut State,
    elwt: &winit::event_loop::EventLoopWindowTarget<AppEvent>,
) {
    match state.handle_secondary_daw_event(&ui_event) {
        EventResponse::Action(action) => {
            if CommandDispatcher::dispatch(action, state, elwt) {
                elwt.exit();
            }
        }
        EventResponse::Actions(actions) => {
            for action in actions {
                if CommandDispatcher::dispatch(action, state, elwt) {
                    elwt.exit();
                    break;
                }
            }
        }
        EventResponse::Ignored | EventResponse::Consumed => {}
    }
    state.request_redraw();
    state.request_secondary_redraw();
}

fn should_request_redraw(
    ui_event: &UiEvent,
    _response_changed_ui: bool,
    continuous_redraw: bool,
) -> bool {
    if continuous_redraw {
        return !matches!(
            ui_event,
            UiEvent::MouseMove { .. } | UiEvent::ContextMenu { .. }
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::should_request_redraw;
    use crate::ui::primitives::UiEvent;

    #[test]
    fn ignored_pointer_moves_use_the_paced_redraw_loop() {
        assert!(!should_request_redraw(
            &UiEvent::MouseMove { x: 0.0, y: 0.0 },
            false,
            true
        ));
        assert!(!should_request_redraw(
            &UiEvent::MouseMove { x: 0.0, y: 0.0 },
            true,
            true
        ));
        assert!(should_request_redraw(
            &UiEvent::MouseMove { x: 0.0, y: 0.0 },
            false,
            false
        ));
        assert!(!should_request_redraw(
            &UiEvent::MouseMove { x: 0.0, y: 0.0 },
            false,
            true
        ));
        assert!(!should_request_redraw(
            &UiEvent::ContextMenu { x: 0.0, y: 0.0 },
            false,
            true
        ));
        assert!(!should_request_redraw(
            &UiEvent::ContextMenu { x: 0.0, y: 0.0 },
            true,
            true
        ));
        assert!(should_request_redraw(
            &UiEvent::ContextMenu { x: 0.0, y: 0.0 },
            false,
            false
        ));
    }
}
