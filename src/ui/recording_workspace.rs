//! Backend-neutral scene and interaction model for the recording workspace.
//!
//! The bande rythmo itself is deliberately absent from this scene: [`Ui`]
//! renders it through the exact editor helpers (`render_rythmo_base`,
//! `render_lines`, markers and drawing) inside [`RecordingLayout::rythmo`].
//! This module only describes the DAW chrome around that read-only view.

use crate::network::NetworkMember;
use crate::recording::{
    AudioAssetId, AudioClipId, AudioTrackId, CaptureState, RecordingEditor, RecordingError,
    RecordingProject, RecordingTool,
};

use super::focus::AccessibleRole;
use super::primitives::{HAlign, Overflow, QuadInstance, Rect, VAlign};

const PANEL_BG: [f32; 4] = [0.075, 0.078, 0.095, 1.0];
const PANEL_ALT: [f32; 4] = [0.105, 0.108, 0.13, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.85];
const TEXT: [u8; 3] = [225, 227, 236];
const MUTED_TEXT: [u8; 3] = [155, 158, 172];
const ACCENT: [f32; 4] = [0.34, 0.28, 0.78, 1.0];
const RECORD: [f32; 4] = [0.82, 0.18, 0.24, 1.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingPage {
    Choice,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingRole {
    Solo,
    Director,
    CoDirector { has_control: bool },
    Actor,
}

impl RecordingRole {
    pub fn can_edit_timeline(self) -> bool {
        matches!(
            self,
            Self::Solo | Self::Director | Self::CoDirector { has_control: true }
        )
    }

    pub fn is_online(self) -> bool {
        !matches!(self, Self::Solo)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingControl {
    ChooseSolo,
    ChooseOnline,
    Tool(RecordingTool),
    TrackMute(AudioTrackId),
    TrackSolo(AudioTrackId),
    TrackArm(AudioTrackId),
    StartCapture,
    Clip(AudioClipId),
    Asset(AudioAssetId),
    Participant(String),
}

impl RecordingControl {
    pub fn stable_id(&self) -> String {
        match self {
            Self::ChooseSolo => "recording.choice.solo".into(),
            Self::ChooseOnline => "recording.choice.online".into(),
            Self::Tool(RecordingTool::Select) => "recording.tool.select".into(),
            Self::Tool(RecordingTool::Cut) => "recording.tool.cut".into(),
            Self::TrackMute(id) => format!("recording.track.{}.mute", id.get()),
            Self::TrackSolo(id) => format!("recording.track.{}.solo", id.get()),
            Self::TrackArm(id) => format!("recording.track.{}.arm", id.get()),
            Self::StartCapture => "recording.capture.start".into(),
            Self::Clip(id) => format!("recording.clip.{}", id.get()),
            Self::Asset(id) => format!("recording.asset.{}", id.get()),
            Self::Participant(id) => format!("recording.participant.{id}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordingControlInfo {
    pub control: RecordingControl,
    pub bounds: Rect,
    pub role: AccessibleRole,
    pub label: String,
    pub value: Option<String>,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct RecordingLabel {
    pub text: String,
    pub bounds: Rect,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub overflow: Overflow,
    pub font_size: f32,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Default)]
pub struct RecordingScene {
    pub quads: Vec<QuadInstance>,
    pub labels: Vec<RecordingLabel>,
    pub controls: Vec<RecordingControlInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordingLayout {
    pub content: Rect,
    pub video: Rect,
    pub toolbar: Option<Rect>,
    pub rythmo: Rect,
    pub source_waveform: Option<Rect>,
    pub microphone_waveform: Option<Rect>,
    pub timeline: Option<Rect>,
    pub tools: Option<Rect>,
    pub track_headers: Option<Rect>,
    pub track_body: Option<Rect>,
    pub assets: Option<Rect>,
    pub participants: Option<Rect>,
}

impl RecordingLayout {
    pub fn choice(content: Rect) -> Self {
        Self {
            content,
            video: Rect::default(),
            toolbar: None,
            rythmo: Rect::default(),
            source_waveform: None,
            microphone_waveform: None,
            timeline: None,
            tools: None,
            track_headers: None,
            track_body: None,
            assets: None,
            participants: None,
        }
    }

    pub fn timeline(content: Rect, online: bool) -> Self {
        let assets_w = content.width.clamp(0.0, 300.0).min(272.0);
        let main_w = (content.width - assets_w).max(0.0);
        let preview_h = (content.height * 0.46).clamp(230.0, 440.0);
        let toolbar_h = super::layout::TOOLBAR_H.min(preview_h);
        let rythmo_h = (preview_h * 0.34).clamp(120.0, 180.0);
        let video_h = (preview_h - toolbar_h - rythmo_h).max(80.0);
        let video = Rect {
            x: content.x,
            y: content.y,
            width: main_w,
            height: video_h,
        };
        let toolbar = Rect {
            x: content.x,
            y: video.y + video.height,
            width: main_w,
            height: toolbar_h,
        };
        let rythmo = Rect {
            x: content.x,
            y: toolbar.y + toolbar.height,
            width: main_w,
            height: rythmo_h,
        };
        let timeline = Rect {
            x: content.x,
            y: rythmo.y + rythmo.height,
            width: main_w,
            height: (content.y + content.height - (rythmo.y + rythmo.height)).max(0.0),
        };
        let tools_w = 54.0_f32.min(timeline.width);
        let headers_w = 158.0_f32.min((timeline.width - tools_w).max(0.0));
        let tools = Rect {
            x: timeline.x,
            y: timeline.y,
            width: tools_w,
            height: timeline.height,
        };
        let track_headers = Rect {
            x: tools.x + tools.width,
            y: timeline.y,
            width: headers_w,
            height: timeline.height,
        };
        let track_body = Rect {
            x: track_headers.x + track_headers.width,
            y: timeline.y,
            width: (timeline.x + timeline.width - (track_headers.x + track_headers.width)).max(0.0),
            height: timeline.height,
        };
        let assets = Rect {
            x: content.x + main_w,
            y: content.y,
            width: assets_w,
            height: content.height,
        };
        let participants = online.then_some(Rect {
            x: (video.x + video.width - 244.0).max(video.x),
            y: video.y + 8.0,
            width: 236.0_f32.min(video.width),
            height: (video.height - 16.0).clamp(0.0, 180.0),
        });
        Self {
            content,
            video,
            toolbar: Some(toolbar),
            rythmo,
            source_waveform: None,
            microphone_waveform: None,
            timeline: Some(timeline),
            tools: Some(tools),
            track_headers: Some(track_headers),
            track_body: Some(track_body),
            assets: Some(assets),
            participants,
        }
    }

    pub fn capturing(screen_w: f32, screen_h: f32) -> Self {
        let rythmo_h = (screen_h * 0.24).clamp(130.0, 240.0);
        let strip_h = 34.0;
        let video_h = (screen_h - rythmo_h - strip_h * 2.0).max(0.0);
        Self {
            content: Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            video: Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: video_h,
            },
            toolbar: None,
            source_waveform: Some(Rect {
                x: 0.0,
                y: video_h,
                width: screen_w,
                height: strip_h,
            }),
            microphone_waveform: Some(Rect {
                x: 0.0,
                y: video_h + strip_h,
                width: screen_w,
                height: strip_h,
            }),
            rythmo: Rect {
                x: 0.0,
                y: video_h + strip_h * 2.0,
                width: screen_w,
                height: rythmo_h,
            },
            timeline: None,
            tools: None,
            track_headers: None,
            track_body: None,
            assets: None,
            participants: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordingWorkspaceUi {
    pub page: RecordingPage,
    pub role: RecordingRole,
    pub editor: RecordingEditor,
    pub view_start_frame: i64,
    pub pixels_per_frame: f32,
    pub selected_asset: Option<AudioAssetId>,
}

impl Default for RecordingWorkspaceUi {
    fn default() -> Self {
        Self {
            page: RecordingPage::Choice,
            role: RecordingRole::Solo,
            editor: RecordingEditor::default(),
            view_start_frame: 0,
            pixels_per_frame: 3.0,
            selected_asset: None,
        }
    }
}

impl RecordingWorkspaceUi {
    pub fn enter_solo(&mut self) {
        self.page = RecordingPage::Timeline;
        self.role = RecordingRole::Solo;
    }

    pub fn enter_online(&mut self, role: RecordingRole) {
        self.page = RecordingPage::Timeline;
        self.role = role;
    }

    pub fn return_to_choice(&mut self) {
        self.page = RecordingPage::Choice;
        self.editor.clear_selection();
        self.selected_asset = None;
    }

    pub fn selected_clips(&self) -> impl Iterator<Item = AudioClipId> + '_ {
        self.editor.selected_clips()
    }

    pub fn select_clip(
        &mut self,
        project: &RecordingProject,
        clip_id: AudioClipId,
        additive: bool,
    ) -> Result<(), RecordingError> {
        self.editor.select_clip(project, clip_id, additive)
    }

    pub fn clear_selection(&mut self) {
        self.editor.clear_selection();
    }

    fn is_clip_selected(&self, clip_id: AudioClipId) -> bool {
        self.editor
            .selected_clips()
            .any(|selected| selected == clip_id)
    }

    pub fn scene(
        &self,
        layout: RecordingLayout,
        project: &RecordingProject,
        capture: Option<&CaptureState>,
        participants: &[NetworkMember],
        control_owner_id: Option<&str>,
        current_frame: i64,
    ) -> RecordingScene {
        if self.page == RecordingPage::Choice {
            return choice_scene(layout);
        }
        timeline_scene(
            self,
            layout,
            project,
            capture,
            participants,
            control_owner_id,
            current_frame,
        )
    }
}

fn choice_scene(layout: RecordingLayout) -> RecordingScene {
    let mut scene = RecordingScene::default();
    push_quad(&mut scene.quads, layout.content, PANEL_BG, BORDER, 0.0);
    let card_w = (layout.content.width * 0.32).clamp(240.0, 420.0);
    let gap = 28.0;
    let total = card_w * 2.0 + gap;
    let x = layout.content.x + (layout.content.width - total).max(0.0) * 0.5;
    let y = layout.content.y + (layout.content.height - 210.0).max(0.0) * 0.45;
    let cards = [
        (
            RecordingControl::ChooseSolo,
            Rect {
                x,
                y,
                width: card_w,
                height: 210.0,
            },
            crate::i18n::t("recording.choice.solo"),
            crate::i18n::t("recording.choice.solo_hint"),
        ),
        (
            RecordingControl::ChooseOnline,
            Rect {
                x: x + card_w + gap,
                y,
                width: card_w,
                height: 210.0,
            },
            crate::i18n::t("recording.choice.online"),
            crate::i18n::t("recording.choice.online_hint"),
        ),
    ];
    for (control, rect, title, hint) in cards {
        push_quad(&mut scene.quads, rect, PANEL_ALT, BORDER, 10.0);
        scene.labels.push(label(
            title,
            Rect {
                height: 74.0,
                ..rect
            },
            20.0,
            TEXT,
        ));
        scene.labels.push(RecordingLabel {
            text: hint.into(),
            bounds: Rect {
                x: rect.x + 20.0,
                y: rect.y + 74.0,
                width: rect.width - 40.0,
                height: rect.height - 94.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 13.0,
            color: MUTED_TEXT,
        });
        scene.controls.push(RecordingControlInfo {
            control,
            bounds: rect,
            role: AccessibleRole::Button,
            label: title.into(),
            value: Some(hint.into()),
            selected: false,
            enabled: true,
        });
    }
    scene
}

#[allow(clippy::too_many_arguments)]
fn timeline_scene(
    ui: &RecordingWorkspaceUi,
    layout: RecordingLayout,
    project: &RecordingProject,
    capture: Option<&CaptureState>,
    participants: &[NetworkMember],
    control_owner_id: Option<&str>,
    current_frame: i64,
) -> RecordingScene {
    let mut scene = RecordingScene::default();
    push_quad(&mut scene.quads, layout.content, PANEL_BG, BORDER, 0.0);
    push_quad(
        &mut scene.quads,
        layout.video,
        [0.0, 0.0, 0.0, 1.0],
        BORDER,
        0.0,
    );

    if let (Some(tools), Some(headers), Some(body), Some(assets)) = (
        layout.tools,
        layout.track_headers,
        layout.track_body,
        layout.assets,
    ) {
        push_quad(&mut scene.quads, tools, PANEL_ALT, BORDER, 0.0);
        push_quad(&mut scene.quads, headers, PANEL_BG, BORDER, 0.0);
        push_quad(
            &mut scene.quads,
            body,
            [0.055, 0.058, 0.073, 1.0],
            BORDER,
            0.0,
        );
        push_quad(&mut scene.quads, assets, PANEL_ALT, BORDER, 0.0);
        push_tool_controls(
            &mut scene,
            tools,
            ui.editor.tool,
            ui.role.can_edit_timeline(),
        );
        push_tracks(&mut scene, ui, project, headers, body, current_frame);
        push_assets(&mut scene, ui, project, assets);
    }

    if let Some(participants_rect) = layout.participants {
        push_participants(
            &mut scene,
            participants_rect,
            participants,
            control_owner_id,
        );
    }

    if let Some(state) = capture {
        match state {
            CaptureState::Countdown { .. } => {
                scene.labels.push(label(
                    crate::i18n::t("recording.capture.countdown"),
                    Rect {
                        x: layout.video.x,
                        y: layout.video.y,
                        width: layout.video.width,
                        height: layout.video.height,
                    },
                    38.0,
                    TEXT,
                ));
            }
            CaptureState::Capturing { .. } => {
                scene.labels.push(RecordingLabel {
                    text: crate::i18n::t("recording.capture.active").into(),
                    bounds: Rect {
                        x: layout.video.x + 14.0,
                        y: layout.video.y + 12.0,
                        width: 220.0,
                        height: 28.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    font_size: 13.0,
                    color: [250, 100, 110],
                });
            }
            _ => {}
        }
    }
    scene
}

fn push_tool_controls(
    scene: &mut RecordingScene,
    tools: Rect,
    active: RecordingTool,
    enabled: bool,
) {
    for (index, (tool, text, key)) in [
        (RecordingTool::Select, "S", "recording.tool.select"),
        (RecordingTool::Cut, "C", "recording.tool.cut"),
    ]
    .into_iter()
    .enumerate()
    {
        let bounds = Rect {
            x: tools.x + 7.0,
            y: tools.y + 10.0 + index as f32 * 50.0,
            width: tools.width - 14.0,
            height: 40.0,
        };
        let selected = tool == active;
        push_quad(
            &mut scene.quads,
            bounds,
            if selected { ACCENT } else { PANEL_BG },
            BORDER,
            5.0,
        );
        scene.labels.push(label(
            text,
            bounds,
            15.0,
            if enabled { TEXT } else { MUTED_TEXT },
        ));
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::Tool(tool),
            bounds,
            role: AccessibleRole::Button,
            label: crate::i18n::t(key).into(),
            value: None,
            selected,
            enabled,
        });
    }
}

fn push_tracks(
    scene: &mut RecordingScene,
    ui: &RecordingWorkspaceUi,
    project: &RecordingProject,
    headers: Rect,
    body: Rect,
    current_frame: i64,
) {
    const ROW_H: f32 = 58.0;
    for (row, track) in project.tracks().enumerate() {
        let y = headers.y + row as f32 * ROW_H;
        if y >= headers.y + headers.height {
            break;
        }
        let header = Rect {
            x: headers.x,
            y,
            width: headers.width,
            height: ROW_H,
        };
        let lane = Rect {
            x: body.x,
            y,
            width: body.width,
            height: ROW_H,
        };
        push_quad(
            &mut scene.quads,
            header,
            if row % 2 == 0 { PANEL_BG } else { PANEL_ALT },
            BORDER,
            0.0,
        );
        push_quad(
            &mut scene.quads,
            lane,
            if row % 2 == 0 {
                [0.065, 0.068, 0.084, 1.0]
            } else {
                [0.075, 0.078, 0.096, 1.0]
            },
            BORDER,
            0.0,
        );
        scene.labels.push(RecordingLabel {
            text: track.name.clone(),
            bounds: Rect {
                x: header.x + 8.0,
                y: header.y + 4.0,
                width: header.width - 16.0,
                height: 22.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 12.0,
            color: TEXT,
        });
        for (index, (control, text, active, color)) in [
            (
                RecordingControl::TrackMute(track.id),
                "M",
                track.muted,
                [0.76, 0.55, 0.18, 1.0],
            ),
            (
                RecordingControl::TrackSolo(track.id),
                "S",
                track.solo,
                [0.78, 0.66, 0.18, 1.0],
            ),
            (
                RecordingControl::TrackArm(track.id),
                "R",
                track.armed,
                RECORD,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let bounds = Rect {
                x: header.x + 8.0 + index as f32 * 34.0,
                y: header.y + 30.0,
                width: 28.0,
                height: 22.0,
            };
            push_quad(
                &mut scene.quads,
                bounds,
                if active {
                    color
                } else {
                    [0.13, 0.13, 0.16, 1.0]
                },
                BORDER,
                4.0,
            );
            scene.labels.push(label(text, bounds, 11.0, TEXT));
            scene.controls.push(RecordingControlInfo {
                control,
                bounds,
                role: AccessibleRole::Button,
                label: match text {
                    "M" => format!(
                        "{} — {}",
                        track.name,
                        crate::i18n::t("recording.track.mute")
                    ),
                    "S" => format!(
                        "{} — {}",
                        track.name,
                        crate::i18n::t("recording.track.solo")
                    ),
                    _ => format!("{} — {}", track.name, crate::i18n::t("recording.track.arm")),
                },
                value: Some(
                    if active {
                        crate::i18n::t("accessibility.on")
                    } else {
                        crate::i18n::t("accessibility.off")
                    }
                    .into(),
                ),
                selected: active,
                enabled: ui.role.can_edit_timeline(),
            });
        }
    }

    for clip in project.clips() {
        let Some(track_row) = project.tracks().position(|track| track.id == clip.track_id) else {
            continue;
        };
        let x = body.x + (clip.start_frame - ui.view_start_frame) as f32 * ui.pixels_per_frame;
        let width = (clip.duration_frames as f32 * ui.pixels_per_frame).max(3.0);
        let bounds = Rect {
            x,
            y: body.y + track_row as f32 * ROW_H + 6.0,
            width,
            height: ROW_H - 12.0,
        };
        if bounds.x + bounds.width < body.x || bounds.x > body.x + body.width {
            continue;
        }
        let selected = ui.is_clip_selected(clip.id);
        push_quad(
            &mut scene.quads,
            bounds,
            if selected {
                [0.30, 0.36, 0.76, 1.0]
            } else {
                [0.20, 0.29, 0.58, 1.0]
            },
            if selected {
                [0.62, 0.70, 1.0, 1.0]
            } else {
                BORDER
            },
            5.0,
        );
        if let Some(asset) = project.asset(clip.asset_id) {
            push_clip_waveform(&mut scene.quads, bounds, &asset.waveform.peaks);
            scene.labels.push(RecordingLabel {
                text: asset.file_name.clone(),
                bounds: Rect {
                    x: bounds.x + 6.0,
                    y: bounds.y + 2.0,
                    width: bounds.width - 12.0,
                    height: 18.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                font_size: 10.0,
                color: TEXT,
            });
            scene.controls.push(RecordingControlInfo {
                control: RecordingControl::Clip(clip.id),
                bounds,
                role: AccessibleRole::ListItem,
                label: asset.file_name.clone(),
                value: Some(format!("{} – {}", clip.start_frame, clip.end_frame())),
                selected,
                enabled: true,
            });
        }
    }

    let playhead_x = body.x + (current_frame - ui.view_start_frame) as f32 * ui.pixels_per_frame;
    if (body.x..=body.x + body.width).contains(&playhead_x) {
        push_quad(
            &mut scene.quads,
            Rect {
                x: playhead_x - 1.0,
                y: body.y,
                width: 2.0,
                height: body.height,
            },
            RECORD,
            [0.0; 4],
            0.0,
        );
    }

    if let Some(armed) = project.armed_track_id() {
        let bounds = Rect {
            x: headers.x + headers.width - 46.0,
            y: headers.y + 6.0,
            width: 38.0,
            height: 38.0,
        };
        push_quad(
            &mut scene.quads,
            bounds,
            RECORD,
            [1.0, 0.4, 0.45, 1.0],
            19.0,
        );
        scene.labels.push(label("●", bounds, 18.0, TEXT));
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::StartCapture,
            bounds,
            role: AccessibleRole::Button,
            label: format!(
                "{} {}",
                crate::i18n::t("recording.capture.start"),
                armed.get()
            ),
            value: None,
            selected: false,
            enabled: ui.role.can_edit_timeline(),
        });
    }
}

fn push_assets(
    scene: &mut RecordingScene,
    ui: &RecordingWorkspaceUi,
    project: &RecordingProject,
    rect: Rect,
) {
    scene.labels.push(RecordingLabel {
        text: crate::i18n::t("recording.assets.title").into(),
        bounds: Rect {
            x: rect.x + 12.0,
            y: rect.y + 8.0,
            width: rect.width - 24.0,
            height: 28.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size: 15.0,
        color: TEXT,
    });
    for (index, asset) in project.assets().enumerate() {
        let bounds = Rect {
            x: rect.x + 10.0,
            y: rect.y + 44.0 + index as f32 * 42.0,
            width: rect.width - 20.0,
            height: 36.0,
        };
        if bounds.y + bounds.height > rect.y + rect.height {
            break;
        }
        let selected = ui.selected_asset == Some(asset.id);
        push_quad(
            &mut scene.quads,
            bounds,
            if selected { ACCENT } else { PANEL_BG },
            BORDER,
            5.0,
        );
        scene.labels.push(RecordingLabel {
            text: asset.file_name.clone(),
            bounds,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 11.0,
            color: TEXT,
        });
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::Asset(asset.id),
            bounds,
            role: AccessibleRole::ListItem,
            label: asset.file_name.clone(),
            value: Some(format!("{:.1} s", asset.duration_seconds())),
            selected,
            enabled: true,
        });
    }
}

fn push_participants(
    scene: &mut RecordingScene,
    rect: Rect,
    members: &[NetworkMember],
    control_owner_id: Option<&str>,
) {
    push_quad(
        &mut scene.quads,
        rect,
        [0.07, 0.07, 0.09, 0.92],
        BORDER,
        7.0,
    );
    scene.labels.push(RecordingLabel {
        text: crate::i18n::t("recording.participants.title").into(),
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y + 4.0,
            width: rect.width - 20.0,
            height: 24.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size: 12.0,
        color: TEXT,
    });
    for (index, member) in members.iter().enumerate() {
        let bounds = Rect {
            x: rect.x + 8.0,
            y: rect.y + 30.0 + index as f32 * 28.0,
            width: rect.width - 16.0,
            height: 24.0,
        };
        if bounds.y + bounds.height > rect.y + rect.height {
            break;
        }
        let controls = control_owner_id == Some(member.id.as_str());
        scene.labels.push(RecordingLabel {
            text: format!(
                "{} · {}{}",
                member.username,
                member.role,
                if member.muted { " · muet" } else { "" }
            ),
            bounds,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 10.0,
            color: if controls { [180, 210, 255] } else { TEXT },
        });
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::Participant(member.id.clone()),
            bounds,
            role: AccessibleRole::ListItem,
            label: member.username.clone(),
            value: Some(member.role.clone()),
            selected: controls,
            enabled: true,
        });
    }
}

fn push_clip_waveform(quads: &mut Vec<QuadInstance>, rect: Rect, peaks: &[f32]) {
    if peaks.is_empty() || rect.width <= 2.0 {
        return;
    }
    let bars = peaks
        .len()
        .min((rect.width / 3.0).max(1.0) as usize)
        .min(96);
    for index in 0..bars {
        let peak_index = index * peaks.len() / bars;
        let amplitude = peaks[peak_index].clamp(0.0, 1.0);
        let height = amplitude * (rect.height - 16.0).max(1.0);
        let x = rect.x + index as f32 * rect.width / bars as f32;
        push_quad(
            quads,
            Rect {
                x,
                y: rect.y + (rect.height - height) * 0.5,
                width: 1.5,
                height,
            },
            [0.75, 0.82, 1.0, 0.72],
            [0.0; 4],
            0.0,
        );
    }
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn label(text: impl Into<String>, bounds: Rect, font_size: f32, color: [u8; 3]) -> RecordingLabel {
    RecordingLabel {
        text: text.into(),
        bounds,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size,
        color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{AudioTrack, RecordingOperation};

    #[test]
    fn capture_layout_keeps_the_normal_rythmo_inside_the_window() {
        let layout = RecordingLayout::capturing(1280.0, 720.0);
        assert_eq!(layout.rythmo.x, 0.0);
        assert_eq!(layout.rythmo.width, 1280.0);
        assert!(layout.rythmo.height >= 130.0);
        assert_eq!(layout.rythmo.y + layout.rythmo.height, 720.0);
        assert!(layout.timeline.is_none());
    }

    #[test]
    fn actor_scene_exposes_track_state_but_disables_mutating_controls() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = project.allocate_track_id();
        project
            .apply(&RecordingOperation::AddTrack {
                track: AudioTrack::new(track_id, "Voix"),
            })
            .unwrap();
        let ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            role: RecordingRole::Actor,
            ..RecordingWorkspaceUi::default()
        };
        let layout = RecordingLayout::timeline(
            Rect {
                x: 0.0,
                y: 68.0,
                width: 1200.0,
                height: 732.0,
            },
            true,
        );
        let scene = ui.scene(layout, &project, None, &[], None, 0);
        let track_controls = scene.controls.iter().filter(|control| {
            matches!(
                control.control,
                RecordingControl::TrackMute(_)
                    | RecordingControl::TrackSolo(_)
                    | RecordingControl::TrackArm(_)
            )
        });
        assert!(track_controls.count() >= 3);
        assert!(scene
            .controls
            .iter()
            .filter(|control| matches!(
                control.control,
                RecordingControl::TrackMute(_)
                    | RecordingControl::TrackSolo(_)
                    | RecordingControl::TrackArm(_)
            ))
            .all(|control| !control.enabled));
    }

    #[test]
    fn choice_controls_have_stable_accessible_ids() {
        assert_eq!(
            RecordingControl::ChooseSolo.stable_id(),
            "recording.choice.solo"
        );
        assert_eq!(
            RecordingControl::ChooseOnline.stable_id(),
            "recording.choice.online"
        );
    }
}
