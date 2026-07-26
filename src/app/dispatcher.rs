use std::path::PathBuf;
use winit::event_loop::EventLoopWindowTarget;

use super::event_loop::AppEvent;
use super::event_loop::{close_project_reset, new_project_reset_and_pick_video};
use super::file_picker::{
    import_cappela_from_path, import_project_from_path, import_subtitle_from_path,
    open_dialog_filters, open_file_picker, project_or_video_dir, quick_save_existing,
    quick_save_existing_with_continuation, save_dialog_filters, save_project_as,
    save_project_as_with_continuation, video_or_project_dir,
};
use crate::application::command::TextCommand;
use crate::application::job_service::SaveContinuation;
use crate::state::State;
use crate::ui::file_explorer::{FileExplorerMode, FilePickerIntent};
use crate::ui::primitives::{EventResponse, UiAction, UiEvent};
use crate::{config, i18n, packet, platform, video_export, video_proxy};
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
                state.project_session.dirty = true;
            }
        }
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
            let highlight_read_word = state.project_session.project.settings().highlight_read_word;
            let scrolling_text_uses_character_color = state
                .project_session
                .project
                .settings()
                .scrolling_text_uses_character_color;
            state.save_project_settings(
                Some(path),
                highlight_read_word,
                scrolling_text_uses_character_color,
                state
                    .project_session
                    .project
                    .settings()
                    .show_text_emotion_lanes,
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
            UiAction::OpenLanguages => Some(crate::i18n::t("languages.title")),
            UiAction::OpenDropdown(crate::ui::primitives::ToolbarDropdown::Respirations) => {
                Some(crate::i18n::t("toolbar.respirations"))
            }
            UiAction::OpenDropdown(crate::ui::primitives::ToolbarDropdown::Reactions) => {
                Some(crate::i18n::t("toolbar.reactions"))
            }
            UiAction::OpenProxyModal => Some(crate::i18n::t("menu.tools.create_proxy")),
            UiAction::OpenSettings => Some(crate::i18n::t("settings.title")),
            UiAction::OpenProjectSettings => Some(crate::i18n::t("project_settings.title")),
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
            UiAction::OpenLanguages => state.language_modal_focus_label(),
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
        if !config::dev_mode()
            && matches!(
                &action,
                UiAction::ActivateWorkspace(
                    crate::application::workspace_service::WorkspaceId::Recording
                ) | UiAction::RecordingChooseSolo
                    | UiAction::RecordingChooseOnline
                    | UiAction::RecordingSetTool(_)
                    | UiAction::RecordingToggleTrackMute(_)
                    | UiAction::RecordingToggleTrackSolo(_)
                    | UiAction::RecordingArmTrack(_)
                    | UiAction::RecordingSelectClip { .. }
                    | UiAction::RecordingSelectAsset(_)
                    | UiAction::RecordingStartCapture
                    | UiAction::RecordingStopCapture
            )
        {
            return false;
        }

        if state.active_workspace() == crate::application::workspace_service::WorkspaceId::Recording
            && action.mutates_rythmo_project()
        {
            state.announce_accessibility(crate::accessibility::AccessibilityEvent::Error {
                message: crate::i18n::t("accessibility.rythmo_read_only").to_string(),
            });
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
            UiAction::RecordingChooseSolo => state.recording_choose_solo(),
            UiAction::RecordingChooseOnline => state.recording_choose_online(),
            UiAction::RecordingSetTool(tool) => state.recording_set_tool(tool),
            UiAction::RecordingToggleTrackMute(track_id) => {
                state.recording_toggle_track_mute(track_id)
            }
            UiAction::RecordingToggleTrackSolo(track_id) => {
                state.recording_toggle_track_solo(track_id)
            }
            UiAction::RecordingArmTrack(track_id) => state.recording_arm_track(track_id),
            UiAction::RecordingSelectClip { clip_id, additive } => {
                state.recording_select_clip(clip_id, additive)
            }
            UiAction::RecordingSelectAsset(asset_id) => state.recording_select_asset(asset_id),
            UiAction::RecordingStartCapture => state.recording_start_capture(),
            UiAction::RecordingStopCapture => state.recording_stop_capture(),
            UiAction::CloseApp => {
                if state.is_project_save_in_progress() {
                    state.show_toast(i18n::t("toast.close_blocked_saving"), 5.0);
                } else {
                    return true;
                }
            }
            UiAction::CloseSecondaryDisplay => state.close_secondary_display(),
            UiAction::Undo => state.undo(),
            UiAction::Redo => state.redo(),
            UiAction::AddVideo => {
                let filters = open_dialog_filters("Video", &["mp4", "mov", "avi", "mkv", "webm"]);
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.video.title"),
                    FileExplorerMode::Open,
                    FilePickerIntent::AddVideo,
                    filters,
                    project_or_video_dir(state),
                    None,
                );
            }
            UiAction::ExportProject => {
                let filters = save_dialog_filters(
                    "Projet Coquerythmo",
                    &[crate::project_archive::PROJECT_EXTENSION],
                );
                open_file_picker(
                    state,
                    elwt,
                    i18n::t("picker.project_save.title"),
                    FileExplorerMode::Save,
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
                    elwt,
                    i18n::t("picker.import.cappela.title"),
                    FileExplorerMode::Open,
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
                    FileExplorerMode::Open,
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
                    FileExplorerMode::Open,
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
                        FileExplorerMode::Save,
                        FilePickerIntent::QuickSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
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
            UiAction::OpenLanguages => state.open_languages_modal(),
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
                    FileExplorerMode::Open,
                    FilePickerIntent::LanguageInstrumentalAudio { language_id: id },
                    filters,
                    video_or_project_dir(state),
                    None,
                );
            }
            UiAction::ClearLanguageInstrumentalAudio { id } => {
                state.set_language_instrumental_audio(id, None);
            }
            UiAction::SaveExportConfiguration { configuration } => {
                state.save_export_configuration(configuration);
            }
            UiAction::StartConfiguredExport { configuration } => {
                state.save_export_configuration(configuration.clone());
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
                    FileExplorerMode::Save,
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
                let Some(project_huuid) = state
                    .project_session
                    .huuid
                    .as_ref()
                    .filter(|_| {
                        state.project_session.project_path.is_some() && !state.project_session.dirty
                    })
                    .map(ToString::to_string)
                else {
                    let message = i18n::t("toast.network_requires_saved_project");
                    state.show_toast(message, 6.0);
                    state.announce_accessibility(crate::accessibility::AccessibilityEvent::Error {
                        message: message.to_string(),
                    });
                    return false;
                };
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
                    packet::Packet::JoinRoom {
                        code,
                        username,
                        project_huuid,
                    }
                } else {
                    packet::Packet::CreateRoom {
                        username,
                        project_huuid,
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
                reading_bar_offset_seconds,
            } => {
                let font_changed = crate::config::get().ui.rythmo_font != rythmo_font;
                crate::config::save_settings(
                    lang,
                    rythmo_font,
                    scroll_speed,
                    reading_bar_offset_seconds,
                );
                if font_changed {
                    crate::vector_text::clear_project_font();
                    state.render.ui_renderer.clear_text_cache();
                    state.project_session.dirty = true;
                }
                state.close_settings_modal();
            }
            UiAction::SaveProjectSettings {
                instrumental_audio_path,
                highlight_read_word,
                scrolling_text_uses_character_color,
                show_text_emotion_lanes,
            } => {
                state.save_project_settings(
                    instrumental_audio_path,
                    highlight_read_word,
                    scrolling_text_uses_character_color,
                    show_text_emotion_lanes,
                );
            }
            UiAction::ToggleActiveAudio => {
                state.toggle_active_audio();
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
                        FileExplorerMode::Save,
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
                if state.project_session.project_path.is_some() {
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
                        FileExplorerMode::Save,
                        FilePickerIntent::CloseProjectSave,
                        filters,
                        project_or_video_dir(state),
                        Some(crate::project_archive::PROJECT_EXTENSION),
                    );
                }
            }
            UiAction::CloseProjectDiscard => close_project_reset(state),
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
                        FileExplorerMode::Save,
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
