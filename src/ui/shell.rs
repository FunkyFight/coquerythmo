//! Shared shell policies for layout-level UI interaction.
//!
//! This module owns geometry that belongs to the common shell rather than to
//! the active workspace: split handles, timeline hit zones and platform-aware
//! scroll conversion. Modal lifecycle is kept in [`super::modal_host`].

use super::layout::Layout;
use super::primitives::{EventResponse, LabelInfo, QuadInstance, Rect, UiAction, UiEvent, Widget};
use super::theme::{SLIDER_W, TOOLBAR_BTN_SIZE, TOPBAR_HEIGHT};
use super::{
    dropdown::Dropdown, icon_button::IconButton, slider::Slider, tab_button::TabButton,
    text_button::TextButton,
};
use crate::application::command::ToolMode;
use crate::application::workspace_service::WorkspaceId;
use crate::i18n::t;

use std::collections::HashMap;

/// Read-only state required to construct the context toolbar.
pub(crate) struct ToolbarBuildContext<'a> {
    pub(crate) toolbar: Rect,
    pub(crate) icon_uvs: &'a HashMap<String, [f32; 4]>,
    pub(crate) playing: bool,
    pub(crate) volume: f32,
    pub(crate) active_mode: Option<ToolMode>,
    pub(crate) brush_color: [f32; 4],
    pub(crate) brush_radius_index: usize,
    pub(crate) brush_color_preset_index: usize,
    pub(crate) erasing: bool,
    pub(crate) brush_color_presets: &'a [[f32; 4]; 8],
    pub(crate) ctrl_held: bool,
    pub(crate) editable: bool,
    pub(crate) playback_enabled: bool,
}

fn icon_uv(icon_uvs: &HashMap<String, [f32; 4]>, name: &str) -> [f32; 4] {
    icon_uvs.get(name).copied().unwrap_or([0.0; 4])
}

pub(crate) fn build_topbar(
    in_room: bool,
    has_video: bool,
    screen_w: f32,
    settings_uv: [f32; 4],
    project_uv: [f32; 4],
    active_workspace: WorkspaceId,
    recording_daw_enabled: bool,
    actor_requests_enabled: bool,
    voicelines_selected_regions: Vec<crate::voicelines::RegionId>,
    comic_dubs_selected_bubble: Option<crate::comic_dubs::BubbleId>,
) -> Vec<Box<dyn Widget>> {
    // Build project menu with "Récent" submenu
    let recents = crate::config::recent_projects();
    let recent_labels: Vec<String> = recents
        .iter()
        .map(|r| {
            if r.video_path == r.br_path {
                return r
                    .br_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default();
            }
            let video = r
                .video_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let br = r
                .br_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            format!("{} + {}", video, br)
        })
        .collect();

    let recents_clone = recents.clone();
    let comic_dubs = active_workspace == WorkspaceId::ComicDubs;
    let voicelines = active_workspace == WorkspaceId::Voicelines;
    let import_label = if comic_dubs {
        t("menu.project.import.coquerythmo")
    } else {
        t("menu.project.import")
    };
    let mut project_menu = Dropdown::new(
        Rect {
            x: 4.0,
            y: 2.0,
            width: 80.0,
            height: 28.0,
        },
        vec![
            t("menu.project.add_video").into(),
            import_label.into(),
            t("menu.project.export").into(),
            t("menu.project.restore_backup").into(),
            format!("{} ▸", t("menu.project.recent")),
            t("menu.project.close").into(),
        ],
        move |index, _label| match index {
            0 => EventResponse::Action(UiAction::AddVideo),
            1 => EventResponse::Action(if comic_dubs {
                UiAction::ImportProject
            } else {
                UiAction::ImportSubtitles
            }),
            2 => EventResponse::Action(UiAction::ExportProject),
            3 => EventResponse::Action(UiAction::RestoreBackup),
            5 => EventResponse::Action(UiAction::CloseProject),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.project"))
    .with_panel_width(340.0)
    .with_disabled_items(vec![false, false, !has_video, false, false, false]);

    if !recent_labels.is_empty() {
        let recents_remove = recents.clone();
        project_menu = project_menu.with_removable_submenu(
            4,
            recent_labels,
            move |index, _label| {
                if let Some(r) = recents_clone.get(index) {
                    EventResponse::Action(UiAction::OpenRecentProject {
                        video_path: r.video_path.clone(),
                        br_path: r.br_path.clone(),
                    })
                } else {
                    EventResponse::Consumed
                }
            },
            move |index, _label| {
                if let Some(r) = recents_remove.get(index) {
                    EventResponse::Action(UiAction::RemoveRecentProject {
                        video_path: r.video_path.clone(),
                        br_path: r.br_path.clone(),
                    })
                } else {
                    EventResponse::Consumed
                }
            },
        );
    }

    let microphone_button = |x| {
        Box::new(
            TextButton::new(
                Rect {
                    x,
                    y: 2.0,
                    width: 150.0,
                    height: 28.0,
                },
                t("recording.microphone.select"),
                || EventResponse::Action(UiAction::OpenRecordingInputDeviceModal),
            )
            .with_tooltip(t("recording.microphone.select")),
        ) as Box<dyn Widget>
    };

    if active_workspace == WorkspaceId::Recording && recording_daw_enabled {
        let recording_left = 188.0;
        let actor_requests_offset = if actor_requests_enabled { 164.0 } else { 0.0 };
        let quick_setup_offset = actor_requests_offset + 200.0;
        let daw_button = TextButton::new(
            Rect {
                x: recording_left + quick_setup_offset + 164.0,
                y: 2.0,
                width: 150.0,
                height: 28.0,
            },
            t("recording.detach_daw"),
            || EventResponse::Action(UiAction::OpenSecondaryDisplay),
        )
        .with_accent()
        .with_tooltip(t("recording.detach_daw"));
        let mut widgets: Vec<Box<dyn Widget>> = vec![Box::new(project_menu)];
        widgets.push(Box::new(
            Dropdown::new(
                Rect {
                    x: 88.0,
                    y: 2.0,
                    width: 96.0,
                    height: 28.0,
                },
                vec![t("recording.audio.import").into()],
                |index, _| match index {
                    0 => EventResponse::Action(UiAction::RecordingImportAudio),
                    _ => EventResponse::Consumed,
                },
            )
            .with_arrow(false)
            .with_trigger_bg(false)
            .with_trigger_label(t("recording.audio.menu"))
            .with_panel_width(260.0),
        ));
        if actor_requests_enabled {
            widgets.push(Box::new(
                Dropdown::new(
                    Rect {
                        x: recording_left,
                        y: 2.0,
                        width: 160.0,
                        height: 28.0,
                    },
                    vec![
                        t("recording.actor_requests.open_microphone").into(),
                        t("recording.actor_requests.transfer_project").into(),
                        t("recording.actor_requests.transfer_display_settings").into(),
                        t("recording.actor_requests.close_transfer_waiting").into(),
                    ],
                    |index, _label| match index {
                        0 => EventResponse::Action(UiAction::RequestActorsOpenMicrophone),
                        1 => EventResponse::Action(UiAction::RequestActorsTransferProject),
                        2 => EventResponse::Action(UiAction::RequestActorsTransferDisplaySettings),
                        3 => EventResponse::Action(
                            UiAction::RequestActorsCloseProjectTransferWaiting,
                        ),
                        _ => EventResponse::Consumed,
                    },
                )
                .with_arrow(false)
                .with_trigger_bg(false)
                .with_trigger_label(t("recording.actor_requests"))
                .with_panel_width(420.0),
            ));
        }
        if in_room {
            widgets.push(Box::new(
                TextButton::new(
                    Rect {
                        x: recording_left + actor_requests_offset,
                        y: 2.0,
                        width: 192.0,
                        height: 28.0,
                    },
                    t("recording.invitation"),
                    || EventResponse::Action(UiAction::OpenRoomInvitation),
                )
                .with_tooltip(t("recording.invitation")),
            ));
        } else {
            widgets.push(Box::new(
                Dropdown::new(
                    Rect {
                        x: recording_left + actor_requests_offset,
                        y: 2.0,
                        width: 192.0,
                        height: 28.0,
                    },
                    vec![
                        t("recording.quick_setup.host_session").into(),
                        t("recording.quick_setup.join_session").into(),
                    ],
                    |index, _label| match index {
                        0 => EventResponse::Action(UiAction::CopyQuickHostLink),
                        1 => EventResponse::Action(UiAction::CopyQuickJoinLink),
                        _ => EventResponse::Consumed,
                    },
                )
                .with_arrow(false)
                .with_trigger_bg(false)
                .with_trigger_label(t("recording.quick_setup"))
                .with_panel_width(392.0),
            ));
        }
        widgets.push(microphone_button(recording_left + quick_setup_offset));
        widgets.push(Box::new(daw_button));
        return widgets;
    }

    let export_menu = Dropdown::new(
        Rect {
            x: if comic_dubs { 188.0 } else { 88.0 },
            y: 2.0,
            width: 80.0,
            height: 28.0,
        },
        vec![t(if voicelines {
            "menu.export.voicelines_audio"
        } else {
            "menu.export.mp4"
        })
        .into()],
        move |index, _label| match index {
            0 => EventResponse::Action(if voicelines {
                UiAction::VoicelinesExportAll
            } else {
                UiAction::OpenExportModal
            }),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.export"))
    .with_panel_width(260.0);

    let tools_menu = Dropdown::new(
        Rect {
            x: 172.0,
            y: 2.0,
            width: 80.0,
            height: 28.0,
        },
        vec![
            t("menu.tools.automation").into(),
            t("menu.tools.secondary_display").into(),
            t("menu.tools.rename_character").into(),
        ],
        |index, _label| match index {
            0 => EventResponse::Action(UiAction::OpenAutomation),
            1 => EventResponse::Action(UiAction::OpenSecondaryDisplay),
            2 => EventResponse::Action(UiAction::OpenRenameCharacterModal),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.tools"))
    .with_panel_width(280.0)
    .with_disabled_items(vec![false, !has_video, false]);

    let connect_menu = Dropdown::new(
        Rect {
            x: 464.0,
            y: 2.0,
            width: 120.0,
            height: 28.0,
        },
        vec![
            t("menu.connect.servers").into(),
            t("menu.connect.disconnect").into(),
        ],
        |index, _label| match index {
            0 => EventResponse::Action(UiAction::OpenServerBrowser),
            1 => EventResponse::Action(UiAction::NetworkDisconnect),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.connect"))
    .with_panel_width(250.0)
    .with_disabled_items(vec![false, !in_room]);

    let panels_menu = Dropdown::new(
        Rect {
            x: 256.0,
            y: 2.0,
            width: 80.0,
            height: 28.0,
        },
        vec![
            format!("{}    Ctrl+I", t("menu.panels.lines")),
            format!("{}    Ctrl+P", t("menu.panels.roles")),
        ],
        |index, _label| match index {
            0 => EventResponse::Action(UiAction::OpenLinesPanel),
            1 => EventResponse::Action(UiAction::OpenRolesPanel),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.panels"))
    .with_panel_width(240.0);

    let explorers_menu = Dropdown::new(
        Rect {
            x: 340.0,
            y: 2.0,
            width: 120.0,
            height: 28.0,
        },
        vec![format!("{}    Ctrl+L", t("menu.explorers.media"))],
        |index, _label| match index {
            0 => EventResponse::Action(UiAction::OpenMediaExplorer),
            _ => EventResponse::Consumed,
        },
    )
    .with_arrow(false)
    .with_trigger_bg(false)
    .with_trigger_label(t("menu.explorers"))
    .with_panel_width(300.0);

    let settings_size = 24.0;
    let settings_x = screen_w - settings_size - 8.0;
    let settings_y = (TOPBAR_HEIGHT - settings_size) / 2.0;
    let project_x = settings_x - settings_size - 8.0;
    let project_btn = IconButton::new(
        Rect {
            x: project_x,
            y: settings_y,
            width: settings_size,
            height: settings_size,
        },
        "",
        project_uv,
        || EventResponse::Action(UiAction::OpenProjectSettings),
    )
    .with_tooltip(if comic_dubs {
        t("comic_dubs_settings.tooltip")
    } else {
        t("project_settings.tooltip")
    });
    let settings_btn = IconButton::new(
        Rect {
            x: settings_x,
            y: settings_y,
            width: settings_size,
            height: settings_size,
        },
        "",
        settings_uv,
        || EventResponse::Action(UiAction::OpenSettings),
    )
    .with_tooltip(t("settings.tooltip"));

    let mut topbar_widgets: Vec<Box<dyn Widget>> = if comic_dubs {
        let mut widgets: Vec<Box<dyn Widget>> = vec![
            Box::new(project_menu),
            Box::new(
                Dropdown::new(
                    Rect {
                        x: 88.0,
                        y: 2.0,
                        width: 96.0,
                        height: 28.0,
                    },
                    vec!["Importer des images".into(), "Importer des audios".into()],
                    |index, _| match index {
                        0 => EventResponse::Action(UiAction::ComicDubsImportImages),
                        1 => EventResponse::Action(UiAction::ComicDubsImportAudios),
                        _ => EventResponse::Consumed,
                    },
                )
                .with_arrow(false)
                .with_trigger_bg(false)
                .with_trigger_label("Imports")
                .with_panel_width(260.0),
            ),
            Box::new(export_menu),
        ];
        if let Some(bubble_id) = comic_dubs_selected_bubble {
            widgets.push(Box::new(
                Dropdown::new(
                    Rect {
                        x: 272.0,
                        y: 2.0,
                        width: 88.0,
                        height: 28.0,
                    },
                    vec!["Animer les sommets de la bulle".into()],
                    move |index, _| match index {
                        0 => EventResponse::Action(UiAction::ComicDubsOpenVertexEditor(bubble_id)),
                        _ => EventResponse::Consumed,
                    },
                )
                .with_arrow(false)
                .with_trigger_bg(false)
                .with_trigger_label("Actions")
                .with_panel_width(300.0),
            ));
        }
        widgets
    } else if voicelines {
        let mut widgets: Vec<Box<dyn Widget>> = vec![Box::new(project_menu), Box::new(export_menu)];
        if voicelines_selected_regions.len() >= 2 {
            widgets.push(Box::new(
                Dropdown::new(
                    Rect {
                        x: 172.0,
                        y: 2.0,
                        width: 88.0,
                        height: 28.0,
                    },
                    vec!["Raccorder les zones sélectionnées".into()],
                    move |index, _| match index {
                        0 => EventResponse::Action(UiAction::VoicelinesJoinRegions(
                            voicelines_selected_regions.clone(),
                        )),
                        _ => EventResponse::Consumed,
                    },
                )
                .with_arrow(false)
                .with_trigger_bg(false)
                .with_trigger_label("Actions")
                .with_panel_width(280.0),
            ));
        }
        widgets
    } else {
        vec![
            Box::new(project_menu),
            Box::new(export_menu),
            Box::new(tools_menu),
            Box::new(panels_menu),
            Box::new(explorers_menu),
            Box::new(connect_menu),
        ]
    };
    if active_workspace == WorkspaceId::Recording {
        topbar_widgets.push(microphone_button(464.0));
        if in_room {
            topbar_widgets.push(Box::new(
                TextButton::new(
                    Rect {
                        x: 618.0,
                        y: 2.0,
                        width: 120.0,
                        height: 28.0,
                    },
                    t("recording.invitation"),
                    || EventResponse::Action(UiAction::OpenRoomInvitation),
                )
                .with_tooltip(t("recording.invitation")),
            ));
        }
    }

    let discord_w = 80.0;
    let discord_h = 24.0;
    let discord_x = if crate::config::dev_mode() {
        let lic_key = crate::config::license_key();
        let lic_type = crate::config::license_type();
        let support_x = if !lic_key.is_empty() || !lic_type.is_empty() {
            let badge_w = 200.0;
            let badge_h = 24.0;
            let badge_x = project_x - badge_w - 8.0;
            let badge_y = (TOPBAR_HEIGHT - badge_h) / 2.0;
            let badge_label = crate::config::license_display_label();
            let badge = super::license_badge::LicenseBadge::new(
                Rect {
                    x: badge_x,
                    y: badge_y,
                    width: badge_w,
                    height: badge_h,
                },
                badge_label,
            );
            topbar_widgets.push(Box::new(badge));
            badge_x - discord_w - 8.0
        } else {
            let support_w = 160.0;
            let support_h = 24.0;
            let support_x = project_x - support_w - 8.0;
            let support_y = (TOPBAR_HEIGHT - support_h) / 2.0;
            let support_btn = TextButton::new(
                Rect {
                    x: support_x,
                    y: support_y,
                    width: support_w,
                    height: support_h,
                },
                t("topbar.support"),
                || EventResponse::Action(UiAction::OpenPricingPage),
            )
            .with_accent()
            .with_tooltip(t("topbar.support"));
            topbar_widgets.push(Box::new(support_btn));
            support_x - discord_w - 8.0
        };
        support_x
    } else {
        project_x - discord_w - 8.0
    };
    let discord_y = (TOPBAR_HEIGHT - discord_h) / 2.0;
    let discord_btn = TextButton::new(
        Rect {
            x: discord_x,
            y: discord_y,
            width: discord_w,
            height: discord_h,
        },
        t("topbar.discord"),
        || EventResponse::Action(UiAction::OpenDiscord),
    )
    .with_tooltip(t("topbar.discord"));

    topbar_widgets.push(Box::new(discord_btn));
    topbar_widgets.push(Box::new(project_btn));
    topbar_widgets.push(Box::new(settings_btn));
    topbar_widgets
}

pub(crate) fn build_workspace_tabs(
    layout: &Layout,
    active_workspace: WorkspaceId,
) -> Vec<Box<dyn Widget>> {
    let tab_width = 164.0;
    let gap = 4.0;
    let y = layout.tabs.y + 2.0;
    let height = (layout.tabs.height - 4.0).max(1.0);
    [
        (WorkspaceId::Rythmo, t("workspace_tabs.rythmo")),
        (WorkspaceId::Recording, t("workspace_tabs.recording")),
        (WorkspaceId::Voicelines, t("workspace_tabs.voicelines")),
        (WorkspaceId::ComicDubs, t("workspace_tabs.comic_dubs")),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (workspace, label))| {
        let tab = TabButton::new(
            Rect {
                x: layout.tabs.x + 8.0 + index as f32 * (tab_width + gap),
                y,
                width: tab_width,
                height,
            },
            label,
            active_workspace == workspace,
            move || EventResponse::Action(UiAction::ActivateWorkspace(workspace)),
        );
        Box::new(tab) as Box<dyn Widget>
    })
    .collect()
}

pub(crate) fn build_toolbar(ctx: ToolbarBuildContext<'_>) -> Vec<Box<dyn Widget>> {
    use crate::rythmo_line::MarkerKind;

    let tb = &ctx.toolbar;
    let s = TOOLBAR_BTN_SIZE;
    let y1 = tb.y
        + if ctx.editable {
            0.0
        } else {
            (tb.height - s) / 2.0
        };
    let gap = 4.0;
    let mut x = tb.x + 8.0;

    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();

    macro_rules! btn {
        ($icon:expr, $action:expr, $tip:expr) => {{
            let b = IconButton::new(
                Rect {
                    x,
                    y: y1,
                    width: s,
                    height: s,
                },
                "",
                icon_uv(ctx.icon_uvs, $icon),
                $action,
            )
            .with_tooltip(t($tip));
            widgets.push(Box::new(b));
            x += s + gap;
        }};
    }

    if ctx.playback_enabled {
        btn!(
            "prev_frame",
            || EventResponse::Action(UiAction::PrevFrame),
            "toolbar.prev_frame"
        );
        let play_uv = if ctx.playing {
            icon_uv(ctx.icon_uvs, "pause")
        } else {
            icon_uv(ctx.icon_uvs, "resume")
        };
        let play_tip = if ctx.playing {
            "toolbar.stop"
        } else {
            "toolbar.play"
        };
        let play = IconButton::new(
            Rect {
                x,
                y: y1,
                width: s,
                height: s,
            },
            "",
            play_uv,
            || EventResponse::Action(UiAction::TogglePlayPause),
        )
        .with_tooltip(t(play_tip));
        widgets.push(Box::new(play));
        x += s + gap;
        btn!(
            "next_frame",
            || EventResponse::Action(UiAction::NextFrame),
            "toolbar.next_frame"
        );
    }

    if ctx.editable {
        x += gap * 2.0;
        btn!(
            "boucle",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::Boucle)),
            "toolbar.boucle"
        );
        btn!(
            "out",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::Out)),
            "toolbar.out"
        );
        btn!(
            "scene",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::SceneChange)),
            "toolbar.scene"
        );

        x += gap * 2.0;
        btn!(
            "respirations",
            || EventResponse::Action(UiAction::OpenDropdown(
                super::primitives::ToolbarDropdown::Respirations
            )),
            "toolbar.respirations"
        );
        btn!(
            "reactions",
            || EventResponse::Action(UiAction::OpenDropdown(
                super::primitives::ToolbarDropdown::Reactions
            )),
            "toolbar.reactions"
        );

        x += gap * 2.0;
        btn!(
            "note",
            || EventResponse::Action(UiAction::AddNote),
            "toolbar.note"
        );

        x += gap * 2.0;
        btn!(
            "liaison_left",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonLeft)),
            "toolbar.liaison_left"
        );
        btn!(
            "liaison_right",
            || EventResponse::Action(UiAction::AddMarker(MarkerKind::LiaisonRight)),
            "toolbar.liaison_right"
        );

        x += gap * 2.0;
        btn!(
            "karaoke",
            || EventResponse::Action(UiAction::ToggleKaraokeForSelection),
            "toolbar.karaoke"
        );
    }
    let _ = x;

    let slider_w = SLIDER_W;
    let slider_h = 24.0;
    let slider_x = tb.x + tb.width - slider_w - 8.0;
    let slider_y = tb.y
        + if ctx.editable {
            (TOOLBAR_BTN_SIZE - slider_h) / 2.0
        } else {
            (tb.height - slider_h) / 2.0
        };
    let mute_x = slider_x - s - gap;
    let mute_icon = if ctx.volume <= 0.001 { "mute" } else { "sound" };
    let mute_tip = if ctx.volume <= 0.001 {
        "toolbar.unmute"
    } else {
        "toolbar.mute"
    };
    let mute = IconButton::new(
        Rect {
            x: mute_x,
            y: y1,
            width: s,
            height: s,
        },
        "",
        icon_uv(ctx.icon_uvs, mute_icon),
        || EventResponse::Action(UiAction::ToggleMute),
    )
    .with_tooltip(t(mute_tip));
    widgets.push(Box::new(mute));
    let volume = Slider::new(
        Rect {
            x: slider_x,
            y: slider_y,
            width: slider_w,
            height: slider_h,
        },
        ctx.volume,
        |val| EventResponse::Action(UiAction::SetVolume(val)),
    );
    widgets.push(Box::new(volume));

    if !ctx.editable {
        return widgets;
    }

    let y2 = tb.y + TOOLBAR_BTN_SIZE + 6.0;
    x = tb.x + 8.0;
    let select_active = ctx.active_mode == Some(ToolMode::Select);
    let select = IconButton::new(
        Rect {
            x,
            y: y2,
            width: s,
            height: s,
        },
        "",
        icon_uv(ctx.icon_uvs, "select-mode"),
        || EventResponse::Action(UiAction::SetToolMode(ToolMode::Select)),
    )
    .with_tooltip(t("toolbar.select_mode"))
    .with_active(select_active);
    widgets.push(Box::new(select));
    x += s + gap;

    let draw_active = ctx.active_mode == Some(ToolMode::Draw);
    let draw = IconButton::new(
        Rect {
            x,
            y: y2,
            width: s,
            height: s,
        },
        "",
        icon_uv(ctx.icon_uvs, "draw-mode"),
        || EventResponse::Action(UiAction::SetToolMode(ToolMode::Draw)),
    )
    .with_tooltip(t("toolbar.draw_mode"))
    .with_active(draw_active);
    widgets.push(Box::new(draw));
    x += s + gap;

    struct ColorButton {
        bounds: Rect,
        color: [f32; 4],
        preset_index: usize,
        presets: Vec<[f32; 4]>,
        on_pick: Box<dyn FnMut() -> EventResponse>,
        on_cycle: Box<dyn FnMut(usize, [f32; 4]) -> EventResponse>,
        ctrl_held: bool,
    }
    impl Widget for ColorButton {
        fn bounds(&self) -> Rect {
            self.bounds
        }
        fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
            match event {
                UiEvent::MousePress { x, y } if self.bounds.contains(*x, *y) => {
                    if self.ctrl_held {
                        return (self.on_pick)();
                    }
                    self.preset_index = (self.preset_index + 1) % self.presets.len();
                    self.color = self.presets[self.preset_index];
                    (self.on_cycle)(self.preset_index, self.color)
                }
                _ => EventResponse::Ignored,
            }
        }
        fn render_quads(&self) -> Vec<QuadInstance> {
            let padding = 4.0;
            let sz = self.bounds.width - padding * 2.0;
            vec![QuadInstance {
                rect: [self.bounds.x + padding, self.bounds.y + padding, sz, sz],
                color: self.color,
                color_bottom: self.color,
                border_color: [0.5, 0.5, 0.55, 0.5],
                border_width: 1.0,
                border_radius: 3.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            }]
        }
        fn labels(&self) -> Vec<LabelInfo<'_>> {
            vec![]
        }
    }
    let color_btn = ColorButton {
        bounds: Rect {
            x,
            y: y2,
            width: s,
            height: s,
        },
        color: ctx.brush_color,
        preset_index: ctx.brush_color_preset_index,
        presets: ctx.brush_color_presets.to_vec(),
        on_pick: Box::new(|| EventResponse::Action(UiAction::OpenBrushColorPicker)),
        on_cycle: Box::new(|idx, color| {
            EventResponse::Action(UiAction::CycleBrushColor { index: idx, color })
        }),
        ctrl_held: ctx.ctrl_held,
    };
    widgets.push(Box::new(color_btn));
    x += s + gap;

    let size_labels = ["S", "M", "L"];
    let size_tip = [
        "toolbar.brush_size_small",
        "toolbar.brush_size_medium",
        "toolbar.brush_size_large",
    ];
    let size_active = ctx.brush_radius_index;
    let size = TextButton::new(
        Rect {
            x,
            y: y2,
            width: s,
            height: s,
        },
        size_labels[size_active],
        || EventResponse::Action(UiAction::CycleBrushSize),
    )
    .with_tooltip(t(size_tip[size_active]));
    widgets.push(Box::new(size));
    x += s + gap;

    let eraser = IconButton::new(
        Rect {
            x,
            y: y2,
            width: s,
            height: s,
        },
        "",
        icon_uv(ctx.icon_uvs, "eraser"),
        || EventResponse::Action(UiAction::ToggleEraser),
    )
    .with_tooltip(t("toolbar.eraser"))
    .with_active(ctx.erasing);
    widgets.push(Box::new(eraser));

    widgets
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitHandle {
    Video,
    Rythmo,
}

pub(crate) fn scroll_delta_to_frames(delta: f32, multiplier: f32) -> i32 {
    scroll_delta_to_frames_impl(delta, multiplier)
}

#[cfg(target_os = "macos")]
fn scroll_delta_to_frames_impl(delta: f32, multiplier: f32) -> i32 {
    let frames = (delta * multiplier).round() as i32;
    if frames == 0 && delta.abs() > f32::EPSILON {
        if delta > 0.0 {
            1
        } else {
            -1
        }
    } else {
        frames
    }
}

#[cfg(not(target_os = "macos"))]
fn scroll_delta_to_frames_impl(delta: f32, multiplier: f32) -> i32 {
    (delta * multiplier) as i32
}

pub(crate) fn progress_bar_rect(tb: &Rect, editable: bool) -> Rect {
    let gap = 4.0;
    let buttons_end = if editable {
        tb.x + 8.0 + 13.0 * (TOOLBAR_BTN_SIZE + gap) + 4.0 * gap * 2.0 + gap
    } else {
        tb.x + 8.0 + 3.0 * (TOOLBAR_BTN_SIZE + gap) + gap
    };
    let slider_start = tb.x + tb.width - SLIDER_W - 8.0;
    let mute_start = slider_start - TOOLBAR_BTN_SIZE - gap;
    let left = buttons_end + 8.0;
    let right = mute_start - 8.0;
    let w = (right - left).max(40.0);
    let h = 6.0;
    Rect {
        x: left,
        y: tb.y + (tb.height - h) / 2.0,
        width: w,
        height: h,
    }
}

pub(crate) fn progress_bar_hit_rect(toolbar: &Rect, editable: bool) -> Rect {
    let rect = progress_bar_rect(toolbar, editable);
    Rect {
        x: rect.x,
        y: rect.y - 8.0,
        width: rect.width,
        height: rect.height + 16.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_choice_topbar_buttons_do_not_overlap() {
        crate::config::init();
        let widgets = build_topbar(
            false,
            true,
            1280.0,
            [0.0; 4],
            [0.0; 4],
            WorkspaceId::Recording,
            true,
            false,
            Vec::new(),
            None,
        );
        for pair in widgets.windows(2) {
            let left = pair[0].bounds();
            let right = pair[1].bounds();
            assert!(left.x + left.width <= right.x);
        }
    }

    #[test]
    fn comic_dubs_topbar_has_room_for_imports_and_export() {
        crate::config::init();
        let widgets = build_topbar(
            false,
            false,
            1280.0,
            [0.0; 4],
            [0.0; 4],
            WorkspaceId::ComicDubs,
            false,
            false,
            Vec::new(),
            None,
        );
        assert!(widgets.len() >= 5);
        for pair in widgets.windows(2) {
            let left = pair[0].bounds();
            let right = pair[1].bounds();
            assert!(left.x + left.width <= right.x);
        }

        let mut widgets = build_topbar(
            false,
            false,
            1280.0,
            [0.0; 4],
            [0.0; 4],
            WorkspaceId::ComicDubs,
            false,
            false,
            Vec::new(),
            Some(42),
        );
        widgets[3].handle_event(&UiEvent::Activate);
        assert!(matches!(
            widgets[3].handle_event(&UiEvent::Activate),
            EventResponse::Actions(actions)
                if actions.contains(&UiAction::ComicDubsOpenVertexEditor(42))
        ));
    }

    #[test]
    fn voicelines_actions_sits_next_to_export_only_for_multi_selection() {
        crate::config::init();
        let mut widgets = build_topbar(
            false,
            false,
            1280.0,
            [0.0; 4],
            [0.0; 4],
            WorkspaceId::Voicelines,
            false,
            false,
            Vec::new(),
            None,
        );
        assert_eq!(widgets[0].bounds().x, 4.0);
        assert_eq!(widgets[1].bounds().x, 88.0);
        assert_eq!(
            widgets
                .iter()
                .filter(|widget| widget.bounds().x < 500.0)
                .count(),
            2
        );
        widgets[1].handle_event(&UiEvent::Activate);
        assert!(matches!(
            widgets[1].handle_event(&UiEvent::Activate),
            EventResponse::Actions(actions)
                if actions.iter().any(|action| action == &UiAction::VoicelinesExportAll)
        ));

        let mut widgets = build_topbar(
            false,
            false,
            1280.0,
            [0.0; 4],
            [0.0; 4],
            WorkspaceId::Voicelines,
            false,
            false,
            vec![7, 9],
            None,
        );
        assert_eq!(widgets[2].bounds().x, 172.0);
        widgets[2].handle_event(&UiEvent::Activate);
        assert!(matches!(
            widgets[2].handle_event(&UiEvent::Activate),
            EventResponse::Actions(actions)
                if actions.contains(&UiAction::VoicelinesJoinRegions(vec![7, 9]))
        ));
    }

    #[test]
    fn compact_playback_bar_is_vertically_centered() {
        let toolbar = Rect {
            x: 0.0,
            y: 100.0,
            width: 800.0,
            height: 42.0,
        };
        let progress = progress_bar_rect(&toolbar, false);
        assert_eq!(progress.y + progress.height / 2.0, 121.0);
        let icon_uvs = HashMap::new();
        let presets = [[0.0; 4]; 8];
        let widgets = build_toolbar(ToolbarBuildContext {
            toolbar,
            icon_uvs: &icon_uvs,
            playing: false,
            volume: 0.75,
            active_mode: None,
            brush_color: [0.0; 4],
            brush_radius_index: 0,
            brush_color_preset_index: 0,
            erasing: false,
            brush_color_presets: &presets,
            ctrl_held: false,
            editable: false,
            playback_enabled: true,
        });
        assert!(widgets.iter().all(|widget| {
            let bounds = widget.bounds();
            bounds.y + bounds.height / 2.0 == 121.0
        }));
    }
}
