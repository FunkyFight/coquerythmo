//! Backend-neutral scene and interaction model for the recording workspace.
//!
//! The bande rythmo itself is deliberately absent from this scene: [`Ui`]
//! renders it through the exact editor helpers (`render_rythmo_base`,
//! `render_lines`, markers and drawing) inside [`RecordingLayout::rythmo`].
//! This module only describes the DAW chrome around that read-only view.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::accessibility::AccessibilityEvent;
use crate::i18n::t;
use crate::network::NetworkMember;
use crate::recording::{
    AudioAsset, AudioAssetId, AudioClip, AudioClipId, AudioTrack, AudioTrackId, CaptureState,
    RecordingEditor, RecordingError, RecordingProject, RecordingTool, WaveformData,
};

use super::focus::AccessibleRole;
use super::primitives::{
    EventResponse, HAlign, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};

const PANEL_BG: [f32; 4] = [0.075, 0.078, 0.095, 1.0];
const PANEL_ALT: [f32; 4] = [0.105, 0.108, 0.13, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.85];
const TEXT: [u8; 3] = [225, 227, 236];
const MUTED_TEXT: [u8; 3] = [155, 158, 172];
const ACCENT: [f32; 4] = [0.34, 0.28, 0.78, 1.0];
const RECORD: [f32; 4] = [0.82, 0.18, 0.24, 1.0];
const USED_AUDIO: [f32; 4] = [0.10, 0.40, 0.21, 1.0];
const USED_AUDIO_SELECTED: [f32; 4] = [0.14, 0.55, 0.27, 1.0];
/// Shape cue of an audio already on the timeline, so the state does not
/// rest on the green fill alone.
const USED_AUDIO_MARK: [f32; 4] = [0.45, 0.92, 0.58, 1.0];
/// Border of an active toggle or a selected take: thick and light, readable
/// without telling the colours apart.
const ACTIVE_BORDER: [f32; 4] = [0.96, 0.96, 1.0, 1.0];
pub const TRACK_ROW_H: f32 = 58.0;
const ASSET_ROW_H: f32 = 42.0;
const ASSET_GROUP_H: f32 = 24.0;
const ASSET_MENU_W: f32 = 190.0;
const ASSET_MENU_ITEM_H: f32 = 30.0;
/// Bounds of a control scrolled out of view. It keeps its place in the
/// keyboard focus order, but no pointer position ever falls inside it.
const HIDDEN_BOUNDS: Rect = Rect {
    x: -1.0e6,
    y: -1.0e6,
    width: 0.0,
    height: 0.0,
};

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

    pub fn can_control_playback(self) -> bool {
        matches!(
            self,
            Self::Solo | Self::Director | Self::CoDirector { has_control: true }
        )
    }

    pub fn can_change_shared_view(self) -> bool {
        matches!(self, Self::Solo | Self::Director)
    }

    pub fn can_adjust_track_volume(self) -> bool {
        self.can_edit_timeline()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingControl {
    ChooseSolo,
    ChooseOnline,
    Tool(RecordingTool),
    AddTrack,
    DeleteSelectedClips,
    RemoveTrack(AudioTrackId),
    RenameTrack(AudioTrackId),
    TrackMute(AudioTrackId),
    TrackSolo(AudioTrackId),
    TrackArm(AudioTrackId),
    TrackVolume(AudioTrackId),
    TrackExport(AudioTrackId),
    TrackReverb(AudioTrackId),
    StartCapture,
    Clip(AudioClipId),
    Asset(AudioAssetId),
    AssetGroup(String),
    ImportUsername,
    Participant(String),
}

impl RecordingControl {
    pub fn stable_id(&self) -> String {
        match self {
            Self::ChooseSolo => "recording.choice.solo".into(),
            Self::ChooseOnline => "recording.choice.online".into(),
            Self::Tool(RecordingTool::Select) => "recording.tool.select".into(),
            Self::Tool(RecordingTool::Cut) => "recording.tool.cut".into(),
            Self::AddTrack => "recording.track.add".into(),
            Self::DeleteSelectedClips => "recording.clip.delete".into(),
            Self::RemoveTrack(id) => format!("recording.track.{}.remove", id.get()),
            Self::RenameTrack(id) => format!("recording.track.{}.rename", id.get()),
            Self::TrackMute(id) => format!("recording.track.{}.mute", id.get()),
            Self::TrackSolo(id) => format!("recording.track.{}.solo", id.get()),
            Self::TrackArm(id) => format!("recording.track.{}.arm", id.get()),
            Self::TrackVolume(id) => format!("recording.track.{}.volume", id.get()),
            Self::TrackExport(id) => format!("recording.track.{}.export", id.get()),
            Self::TrackReverb(id) => format!("recording.track.{}.reverb", id.get()),
            Self::StartCapture => "recording.capture.start".into(),
            Self::Clip(id) => format!("recording.clip.{}", id.get()),
            Self::Asset(id) => format!("recording.asset.{}", id.get()),
            Self::AssetGroup(owner) => format!("recording.assets.group.{owner}"),
            Self::ImportUsername => "recording.audio.import.username".into(),
            Self::Participant(id) => format!("recording.participant.{id}"),
        }
    }
}

/// Where a control sits in the lists of the workspace, so the arrow keys can
/// move between rows and the view can scroll to a control given the focus.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RecordingControlPlace {
    /// Track row of the track header controls and of the takes.
    pub track_row: Option<usize>,
    /// Start and end frames of a take.
    pub start_frame: Option<i64>,
    pub end_frame: Option<i64>,
    /// Top of an audio-library row in the scrolled library content.
    pub library_offset: Option<f32>,
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
    /// False when the control is scrolled out of view: it stays reachable
    /// from the keyboard and the screen reader, but is neither drawn nor hit
    /// by the pointer (see [`HIDDEN_BOUNDS`]).
    pub visible: bool,
    pub place: RecordingControlPlace,
}

/// Arrow-key moves of the keyboard focus inside the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingFocusMove {
    /// `↑` / `↓`: previous / next item of the same kind (same button on the
    /// track above or below, take of the neighbouring track, library row…).
    Previous,
    Next,
    /// `←` / `→`: previous / next control of the same track (its buttons,
    /// its takes in time order), or of the same toolbar.
    PreviousInRow,
    NextInRow,
    /// `Début` / `Fin`: first / last item of the same kind.
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Choice,
    Tool,
    Track(u8),
    Take,
    Library,
    Participant,
    Prompt,
}

fn control_kind(control: &RecordingControl) -> ControlKind {
    match control {
        RecordingControl::ChooseSolo | RecordingControl::ChooseOnline => ControlKind::Choice,
        RecordingControl::Tool(_)
        | RecordingControl::AddTrack
        | RecordingControl::DeleteSelectedClips
        | RecordingControl::StartCapture => ControlKind::Tool,
        RecordingControl::RenameTrack(_) => ControlKind::Track(0),
        RecordingControl::TrackVolume(_) => ControlKind::Track(1),
        RecordingControl::TrackMute(_) => ControlKind::Track(2),
        RecordingControl::TrackSolo(_) => ControlKind::Track(3),
        RecordingControl::TrackArm(_) => ControlKind::Track(4),
        RecordingControl::TrackExport(_) => ControlKind::Track(5),
        RecordingControl::TrackReverb(_) => ControlKind::Track(6),
        RecordingControl::RemoveTrack(_) => ControlKind::Track(7),
        RecordingControl::Clip(_) => ControlKind::Take,
        RecordingControl::Asset(_) | RecordingControl::AssetGroup(_) => ControlKind::Library,
        RecordingControl::Participant(_) => ControlKind::Participant,
        RecordingControl::ImportUsername => ControlKind::Prompt,
    }
}

/// Control the arrow keys move the focus to from `current`, if any. Only
/// enabled controls are candidates, hidden (scrolled out) ones included.
pub fn neighbor_control<'a>(
    controls: &'a [RecordingControlInfo],
    current: &RecordingControl,
    movement: RecordingFocusMove,
) -> Option<&'a RecordingControlInfo> {
    let here = controls
        .iter()
        .find(|control| &control.control == current)?;
    let kind = control_kind(&here.control);
    let same_kind =
        |control: &&RecordingControlInfo| control.enabled && control_kind(&control.control) == kind;
    let step = |list: Vec<&'a RecordingControlInfo>, forward: bool| {
        let index = list
            .iter()
            .position(|control| &control.control == current)?;
        if forward {
            list.get(index + 1).copied()
        } else {
            index
                .checked_sub(1)
                .and_then(|index| list.get(index).copied())
        }
    };
    match (kind, movement) {
        (ControlKind::Track(column), RecordingFocusMove::Previous | RecordingFocusMove::Next) => {
            let row = here.place.track_row?;
            let forward = movement == RecordingFocusMove::Next;
            let rows = controls
                .iter()
                .filter(|control| control.enabled)
                .filter(|control| matches!(control_kind(&control.control), ControlKind::Track(_)))
                .filter_map(|control| control.place.track_row);
            let target_row = if forward {
                rows.filter(|candidate| *candidate > row).min()
            } else {
                rows.filter(|candidate| *candidate < row).max()
            }?;
            let in_row = |wanted: u8| {
                controls.iter().find(|control| {
                    control.enabled
                        && control.place.track_row == Some(target_row)
                        && control_kind(&control.control) == ControlKind::Track(wanted)
                })
            };
            in_row(column).or_else(|| in_row(0))
        }
        (ControlKind::Track(_), RecordingFocusMove::First | RecordingFocusMove::Last) => {
            let same_column = controls.iter().filter(same_kind);
            if movement == RecordingFocusMove::First {
                same_column.min_by_key(|control| control.place.track_row)
            } else {
                same_column.max_by_key(|control| control.place.track_row)
            }
        }
        (
            ControlKind::Track(_),
            RecordingFocusMove::PreviousInRow | RecordingFocusMove::NextInRow,
        ) => step(
            controls
                .iter()
                .filter(|control| {
                    control.enabled
                        && matches!(control_kind(&control.control), ControlKind::Track(_))
                        && control.place.track_row == here.place.track_row
                })
                .collect(),
            movement == RecordingFocusMove::NextInRow,
        ),
        (ControlKind::Take, RecordingFocusMove::Previous | RecordingFocusMove::Next) => {
            let row = here.place.track_row?;
            let start = here.place.start_frame.unwrap_or_default();
            let forward = movement == RecordingFocusMove::Next;
            let takes = || controls.iter().filter(same_kind);
            let rows = takes().filter_map(|control| control.place.track_row);
            let target_row = if forward {
                rows.filter(|candidate| *candidate > row).min()
            } else {
                rows.filter(|candidate| *candidate < row).max()
            }?;
            takes()
                .filter(|control| control.place.track_row == Some(target_row))
                .min_by_key(|control| (control.place.start_frame.unwrap_or_default() - start).abs())
        }
        (ControlKind::Take, RecordingFocusMove::PreviousInRow | RecordingFocusMove::NextInRow) => {
            step(
                controls
                    .iter()
                    .filter(same_kind)
                    .filter(|control| control.place.track_row == here.place.track_row)
                    .collect(),
                movement == RecordingFocusMove::NextInRow,
            )
        }
        (
            ControlKind::Library | ControlKind::Participant | ControlKind::Prompt,
            RecordingFocusMove::PreviousInRow | RecordingFocusMove::NextInRow,
        ) => None,
        (_, RecordingFocusMove::First) => controls.iter().find(same_kind),
        (_, RecordingFocusMove::Last) => controls.iter().rev().find(same_kind),
        (_, movement) => step(
            controls.iter().filter(same_kind).collect(),
            matches!(
                movement,
                RecordingFocusMove::Next | RecordingFocusMove::NextInRow
            ),
        ),
    }
}

/// "1 minute 12 secondes 40 centièmes": a position or a length on the
/// recording timeline as a screen reader says it. Empty units are left out.
pub fn spoken_duration(seconds: f64) -> String {
    let total = (seconds.max(0.0) * 100.0).round() as i64;
    let hours = total / 360_000;
    let minutes = (total / 6_000) % 60;
    let whole_seconds = (total / 100) % 60;
    let hundredths = total % 100;
    let unit = |value: i64, one: &str, many: &str| {
        format!("{value} {}", t(if value == 1 { one } else { many }))
    };
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(unit(hours, "accessibility.hour", "accessibility.hours"));
    }
    if minutes > 0 {
        parts.push(unit(
            minutes,
            "accessibility.minute",
            "accessibility.minutes",
        ));
    }
    if whole_seconds > 0 || (parts.is_empty() && hundredths == 0) {
        parts.push(unit(
            whole_seconds,
            "accessibility.second",
            "accessibility.seconds",
        ));
    }
    if hundredths > 0 {
        parts.push(unit(
            hundredths,
            "accessibility.hundredth",
            "accessibility.hundredths",
        ));
    }
    parts.join(" ")
}

fn count_label(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        t(one).to_string()
    } else {
        t(many).replace("{count}", &count.to_string())
    }
}

/// Name of an audio without its extension, as listed in the library.
pub fn audio_display_name(file_name: &str) -> String {
    file_name
        .strip_suffix(".flac")
        .unwrap_or(file_name)
        .to_owned()
}

/// Who recorded a take: the username its file starts with, or the file
/// name when it does not follow the take naming.
fn audio_speaker(file_name: &str) -> String {
    let owner = asset_owner(file_name);
    if owner == t("recording.assets.other") {
        audio_display_name(file_name)
    } else {
        owner
    }
}

/// Spoken name and details of a take: who recorded it, its track, where it
/// starts and how long it lasts.
pub fn clip_accessible_text(project: &RecordingProject, clip: &AudioClip) -> (String, String) {
    let fps = project.timeline_fps().max(1.0);
    let speaker = project
        .asset(clip.asset_id)
        .map(|asset| audio_speaker(&asset.file_name))
        .unwrap_or_default();
    let track = project
        .track(clip.track_id)
        .map(|track| track.name.clone())
        .unwrap_or_default();
    (
        t("recording.a11y.clip").replace("{speaker}", &speaker),
        t("recording.a11y.clip_value")
            .replace("{track}", &track)
            .replace("{start}", &spoken_duration(clip.start_frame as f64 / fps))
            .replace(
                "{duration}",
                &spoken_duration(clip.duration_frames as f64 / fps),
            ),
    )
}

/// Spoken details of an audio of the library: length and whether it is
/// already placed on the timeline.
pub fn asset_accessible_value(asset: &AudioAsset, used: bool) -> String {
    format!(
        "{}, {}",
        spoken_duration(asset.duration_seconds()),
        t(if used {
            "recording.a11y.asset_used"
        } else {
            "recording.a11y.asset_unused"
        })
    )
}

/// Spoken state of a track: armed, muted, solo, volume and number of takes.
pub fn track_accessible_summary(track: &AudioTrack, volume: f32, takes: usize) -> String {
    let mut parts = Vec::new();
    if track.armed {
        parts.push(t("recording.a11y.track_armed").to_string());
    }
    if track.muted {
        parts.push(t("recording.a11y.track_muted").to_string());
    }
    if track.solo {
        parts.push(t("recording.a11y.track_solo").to_string());
    }
    parts.push(
        t("recording.a11y.track_volume").replace("{percent}", &format!("{:.0}", volume * 100.0)),
    );
    parts.push(if takes == 0 {
        t("recording.a11y.takes_none").to_string()
    } else {
        count_label(
            takes,
            "recording.a11y.takes_one",
            "recording.a11y.takes_many",
        )
    });
    parts.join(", ")
}

/// Track label of a track control: "Piste 1 — Muet".
fn track_control_label(track: &AudioTrack, key: &str) -> String {
    format!("{} — {}", track.name, t(key))
}

fn on_off(active: bool) -> String {
    t(if active {
        "accessibility.on"
    } else {
        "accessibility.off"
    })
    .to_string()
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
    pub system_quads: Vec<QuadInstance>,
    pub system_labels: Vec<RecordingLabel>,
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
    pub fn video_split_handle_rect(&self) -> Rect {
        Rect {
            x: self.video.x,
            y: self
                .toolbar
                .map_or(self.video.y + self.video.height, |bar| bar.y)
                - super::layout::SPLIT_DRAG_ZONE / 2.0,
            width: self.video.width,
            height: super::layout::SPLIT_DRAG_ZONE,
        }
    }

    pub fn rythmo_split_handle_rect(&self) -> Rect {
        Rect {
            x: self.rythmo.x,
            y: if self.microphone_waveform.is_some() || self.timeline.is_none() {
                self.rythmo.y
            } else {
                self.timeline
                    .map_or(self.rythmo.y + self.rythmo.height, |timeline| timeline.y)
            } - super::layout::SPLIT_DRAG_ZONE / 2.0,
            width: self.rythmo.width,
            height: super::layout::SPLIT_DRAG_ZONE,
        }
    }

    pub fn assets_split_handle_rect(&self) -> Option<Rect> {
        let assets = self.assets?;
        Some(Rect {
            x: assets.x - super::layout::SPLIT_DRAG_ZONE / 2.0,
            y: assets.y,
            width: super::layout::SPLIT_DRAG_ZONE,
            height: assets.height,
        })
    }
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
        Self::timeline_with_splits(content, online, 0.48, 0.34, 0.23)
    }

    pub fn timeline_with_splits(
        content: Rect,
        online: bool,
        video_split: f32,
        rythmo_split: f32,
        assets_split: f32,
    ) -> Self {
        Self::timeline_with_splits_and_rythmo_min(
            content,
            online,
            video_split,
            rythmo_split,
            assets_split,
            100.0,
        )
    }

    pub fn timeline_with_splits_and_rythmo_min(
        content: Rect,
        online: bool,
        _video_split: f32,
        _rythmo_split: f32,
        assets_split: f32,
        rythmo_min_h: f32,
    ) -> Self {
        let assets_w = if assets_split <= 0.0 {
            0.0
        } else {
            (content.width * assets_split)
                .clamp(180.0, 420.0)
                .min(content.width)
        };
        let main_w = (content.width - assets_w).max(0.0);
        let toolbar_h = super::layout::TOOLBAR_H.min(content.height);
        let available_h = (content.height - toolbar_h).max(0.0);
        const DAW_MIN_H: f32 = 164.0;
        let video_h = (available_h - DAW_MIN_H - rythmo_min_h.max(100.0)).max(140.0);
        let available_after_video = (content.height - video_h - toolbar_h).max(0.0);
        let rythmo_h = rythmo_min_h
            .max(100.0)
            .min((available_after_video - DAW_MIN_H).max(0.0));
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
        let tools_w = 100.0_f32.min(timeline.width);
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
        let assets = (assets_w > 0.0).then_some(Rect {
            x: content.x + main_w,
            y: content.y,
            width: assets_w,
            height: content.height,
        });
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
            assets,
            participants,
        }
    }

    pub fn detached_main(content: Rect, video_split: f32) -> Self {
        let min_video = super::layout::VIDEO_MIN_H.min(content.height);
        let max_video = (content.height - super::layout::RYTHMO_MIN_H).max(min_video);
        let video_h = (content.height * video_split).clamp(min_video, max_video);
        let video = Rect {
            x: content.x,
            y: content.y,
            width: content.width,
            height: video_h,
        };
        Self {
            content,
            video,
            toolbar: None,
            rythmo: Rect {
                x: content.x,
                y: video.y + video.height,
                width: content.width,
                height: (content.height - video.height).max(0.0),
            },
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

    pub fn daw(content: Rect, assets_split: f32) -> Self {
        let toolbar_h = super::layout::TOOLBAR_H.min(content.height);
        let toolbar = Rect {
            height: toolbar_h,
            ..content
        };
        let daw_content = Rect {
            y: content.y + toolbar_h,
            height: (content.height - toolbar_h).max(0.0),
            ..content
        };
        let assets_w = (content.width * assets_split)
            .clamp(180.0, 420.0)
            .min(content.width);
        let timeline = Rect {
            width: (content.width - assets_w).max(0.0),
            ..daw_content
        };
        let tools_w = 100.0_f32.min(timeline.width);
        let headers_w = 158.0_f32.min((timeline.width - tools_w).max(0.0));
        let tools = Rect {
            width: tools_w,
            ..timeline
        };
        let track_headers = Rect {
            x: tools.x + tools.width,
            width: headers_w,
            ..timeline
        };
        let track_body = Rect {
            x: track_headers.x + track_headers.width,
            width: (timeline.width - tools.width - track_headers.width).max(0.0),
            ..timeline
        };
        Self {
            content,
            video: Rect::default(),
            toolbar: Some(toolbar),
            rythmo: Rect::default(),
            source_waveform: None,
            microphone_waveform: None,
            timeline: Some(timeline),
            tools: Some(tools),
            track_headers: Some(track_headers),
            track_body: Some(track_body),
            assets: Some(Rect {
                x: timeline.x + timeline.width,
                width: assets_w,
                ..daw_content
            }),
            participants: None,
        }
    }

    pub fn capturing(
        screen_w: f32,
        screen_h: f32,
        rythmo_min_h: f32,
        rythmo_split: Option<f32>,
    ) -> Self {
        let automatic_rythmo_h = (screen_h * 0.26).clamp(180.0, 280.0).max(rythmo_min_h);
        let rythmo_h = rythmo_split
            .map_or(automatic_rythmo_h, |split| screen_h * split)
            .min(screen_h);
        let video_h = (screen_h - rythmo_h).max(0.0);
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
            source_waveform: None,
            microphone_waveform: None,
            rythmo: Rect {
                x: 0.0,
                y: video_h,
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
    pub view_start_frame: f64,
    pub pixels_per_frame: f32,
    pub selected_asset: Option<AudioAssetId>,
    pub renaming_track: Option<AudioTrackId>,
    pub rename_buffer: String,
    pending_audio_import: Option<PendingAudioImport>,
    import_username: String,
    pub dragging_asset: Option<AudioAssetId>,
    pub dragging_clip: Option<RecordingClipDrag>,
    pub dragging_track_volume: Option<AudioTrackId>,
    track_volumes: BTreeMap<AudioTrackId, f32>,
    track_reverbs: BTreeSet<AudioTrackId>,
    track_scroll: usize,
    track_count: usize,
    dragging_track_scrollbar: bool,
    track_scrollbar_drag_offset: f32,
    asset_scroll: f32,
    asset_content_height: f32,
    dragging_asset_scrollbar: bool,
    asset_scrollbar_drag_offset: f32,
    expanded_asset_owners: BTreeSet<String>,
    asset_context_menu: Option<AssetContextMenu>,
    /// Frame of the last `Entrée` on the takes under the playhead: pressing
    /// it again there moves on to the next track.
    take_cycle_frame: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct AssetContextMenu {
    asset_id: AudioAssetId,
    x: f32,
    y: f32,
    /// Highlighted entry of [`ASSET_MENU_ITEMS`], driven by the pointer and
    /// by `↑` / `↓`.
    highlighted: usize,
    submenu_open: bool,
    hover_voicelines: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetMenuItem {
    PlaceAtPlayhead,
    SendTo,
    Delete,
}

const ASSET_MENU_ITEMS: [AssetMenuItem; 3] = [
    AssetMenuItem::PlaceAtPlayhead,
    AssetMenuItem::SendTo,
    AssetMenuItem::Delete,
];

impl AssetMenuItem {
    fn label(self) -> String {
        t(match self {
            Self::PlaceAtPlayhead => "recording.asset_menu.place",
            Self::SendTo => "recording.asset_menu.send_to",
            Self::Delete => "recording.asset_menu.delete",
        })
        .to_string()
    }
}

impl AssetContextMenu {
    fn item_rect(&self, index: usize) -> Rect {
        Rect {
            x: self.x,
            y: self.y + index as f32 * ASSET_MENU_ITEM_H,
            width: ASSET_MENU_W,
            height: ASSET_MENU_ITEM_H,
        }
    }

    fn submenu_rect(&self) -> Rect {
        let send_to = self.item_rect(1);
        Rect {
            x: send_to.x + send_to.width - 2.0,
            width: ASSET_MENU_W * 0.7,
            ..send_to
        }
    }
}

fn menu_focus(label: String) -> EventResponse {
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Focus {
        label,
        role: "menu item".into(),
    }))
}

#[derive(Debug, Clone, PartialEq)]
struct PendingAudioImport {
    path: PathBuf,
    placement: Option<(AudioTrackId, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingTextEditResult {
    Consumed,
    /// `Échap` closed the field without changing anything; the label names
    /// what was cancelled.
    Cancelled(String),
    RenameTrack {
        track_id: AudioTrackId,
        name: String,
    },
    ImportAudio {
        path: PathBuf,
        username: String,
        placement: Option<(AudioTrackId, i64)>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct RecordingClipDrag {
    pub last_x: f32,
    pub accum_px: f32,
}

impl Default for RecordingWorkspaceUi {
    fn default() -> Self {
        Self {
            page: RecordingPage::Choice,
            role: RecordingRole::Solo,
            editor: RecordingEditor::default(),
            view_start_frame: 0.0,
            pixels_per_frame: 3.0,
            selected_asset: None,
            renaming_track: None,
            rename_buffer: String::new(),
            pending_audio_import: None,
            import_username: String::new(),
            dragging_asset: None,
            dragging_clip: None,
            dragging_track_volume: None,
            track_volumes: BTreeMap::new(),
            track_reverbs: BTreeSet::new(),
            track_scroll: 0,
            track_count: 0,
            dragging_track_scrollbar: false,
            track_scrollbar_drag_offset: 0.0,
            asset_scroll: 0.0,
            asset_content_height: 0.0,
            dragging_asset_scrollbar: false,
            asset_scrollbar_drag_offset: 0.0,
            expanded_asset_owners: BTreeSet::new(),
            asset_context_menu: None,
            take_cycle_frame: None,
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
        self.cancel_rename_track();
        self.cancel_audio_import();
        self.dragging_asset = None;
        self.dragging_clip = None;
        self.dragging_track_volume = None;
        self.track_volumes.clear();
        self.track_reverbs.clear();
        self.expanded_asset_owners.clear();
        self.asset_context_menu = None;
    }

    pub fn selected_clips(&self) -> impl Iterator<Item = AudioClipId> + '_ {
        self.editor.selected_clips()
    }

    pub fn dragging_asset_id(&self) -> Option<AudioAssetId> {
        self.dragging_asset
    }

    pub fn select_clip(
        &mut self,
        project: &RecordingProject,
        clip_id: AudioClipId,
        additive: bool,
    ) -> Result<(), RecordingError> {
        self.selected_asset = None;
        self.editor.select_clip(project, clip_id, additive)
    }

    pub fn clear_selection(&mut self) {
        self.editor.clear_selection();
    }

    /// `Entrée`, as for a rythmo line: select the take under the playhead,
    /// top track first; pressing again at the same frame moves on to the take
    /// of the next track. With no take there, the selection is cleared.
    pub fn select_clip_at_playhead(
        &mut self,
        project: &RecordingProject,
        frame: i64,
    ) -> Option<AudioClipId> {
        let rows = project
            .tracks()
            .enumerate()
            .map(|(row, track)| (track.id, row))
            .collect::<BTreeMap<_, _>>();
        let mut candidates = project
            .clips()
            .filter(|clip| clip.start_frame <= frame && frame < clip.end_frame())
            .filter_map(|clip| rows.get(&clip.track_id).map(|row| (*row, clip)))
            .map(|(row, clip)| (row, clip.start_frame, clip.id.get(), clip.id))
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|(row, start, raw, _)| (*row, *start, *raw));
        let cycling = self.take_cycle_frame == Some(frame);
        self.take_cycle_frame = Some(frame);
        if candidates.is_empty() {
            self.editor.clear_selection();
            return None;
        }
        let selected = self.editor.selected_clips().collect::<Vec<_>>();
        let current = match selected.as_slice() {
            [only] => candidates
                .iter()
                .position(|(_, _, _, clip_id)| clip_id == only),
            _ => None,
        };
        let next = if cycling {
            current.map_or(0, |index| (index + 1) % candidates.len())
        } else {
            0
        };
        let clip_id = candidates[next].3;
        self.select_clip(project, clip_id, false).ok()?;
        Some(clip_id)
    }

    pub fn begin_rename_track(&mut self, project: &RecordingProject, track_id: AudioTrackId) {
        self.renaming_track = project.track(track_id).map(|track| track.id);
        self.rename_buffer = project
            .track(track_id)
            .map(|track| track.name.clone())
            .unwrap_or_default();
    }

    pub fn cancel_rename_track(&mut self) {
        self.renaming_track = None;
        self.rename_buffer.clear();
    }

    pub fn is_editing_text(&self) -> bool {
        self.renaming_track.is_some() || self.pending_audio_import.is_some()
    }

    pub fn begin_audio_import(
        &mut self,
        path: PathBuf,
        placement: Option<(AudioTrackId, i64)>,
        username: String,
    ) {
        self.cancel_rename_track();
        self.pending_audio_import = Some(PendingAudioImport { path, placement });
        self.import_username = username.chars().take(80).collect();
    }

    fn cancel_audio_import(&mut self) {
        self.pending_audio_import = None;
        self.import_username.clear();
    }

    pub fn handle_text_edit(&mut self, event: &UiEvent) -> Option<RecordingTextEditResult> {
        let UiEvent::KeyInput { text } = event else {
            return self
                .pending_audio_import
                .as_ref()
                .map(|_| RecordingTextEditResult::Consumed);
        };

        if self.pending_audio_import.is_some() {
            match text.as_str() {
                "\x1b" => {
                    self.cancel_audio_import();
                    return Some(RecordingTextEditResult::Cancelled(
                        t("recording.audio.username_prompt").to_string(),
                    ));
                }
                "\r" | "\n" => {
                    let username = self.import_username.trim().to_owned();
                    if !username.is_empty() {
                        let pending = self.pending_audio_import.take().unwrap();
                        self.import_username.clear();
                        return Some(RecordingTextEditResult::ImportAudio {
                            path: pending.path,
                            username,
                            placement: pending.placement,
                        });
                    }
                }
                "\x08" | "\x7f" => {
                    self.import_username.pop();
                }
                value if !value.chars().any(char::is_control) => {
                    let remaining = 80_usize.saturating_sub(self.import_username.chars().count());
                    self.import_username.extend(value.chars().take(remaining));
                }
                _ => {}
            }
            return Some(RecordingTextEditResult::Consumed);
        }

        let track_id = self.renaming_track?;
        match text.as_str() {
            "\x1b" => {
                self.cancel_rename_track();
                return Some(RecordingTextEditResult::Cancelled(
                    t("recording.a11y.rename_title").to_string(),
                ));
            }
            "\r" | "\n" => {
                let name = self.rename_buffer.trim().to_owned();
                self.cancel_rename_track();
                if !name.is_empty() {
                    return Some(RecordingTextEditResult::RenameTrack { track_id, name });
                }
            }
            "\x08" | "\x7f" => {
                self.rename_buffer.pop();
            }
            value if !value.chars().any(char::is_control) => {
                self.rename_buffer.push_str(value);
                self.rename_buffer.truncate(80);
            }
            _ => {}
        }
        Some(RecordingTextEditResult::Consumed)
    }

    pub fn reveal_asset(&mut self, file_name: &str, asset_id: AudioAssetId) {
        self.expanded_asset_owners.insert(asset_owner(file_name));
        self.selected_asset = Some(asset_id);
    }

    pub fn track_volume(&self, track_id: AudioTrackId) -> f32 {
        self.track_volumes.get(&track_id).copied().unwrap_or(1.0)
    }

    pub fn set_track_volume(&mut self, track_id: AudioTrackId, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, crate::recording_mix::TRACK_VOLUME_MAX)
        } else {
            1.0
        };
        if (volume - 1.0).abs() <= f32::EPSILON {
            self.track_volumes.remove(&track_id);
        } else {
            self.track_volumes.insert(track_id, volume);
        }
    }

    pub fn track_reverb(&self, track_id: AudioTrackId) -> bool {
        self.track_reverbs.contains(&track_id)
    }

    pub fn toggle_track_reverb(&mut self, track_id: AudioTrackId) {
        if !self.track_reverbs.remove(&track_id) {
            self.track_reverbs.insert(track_id);
        }
    }

    pub fn sync_view_to_playhead(
        &mut self,
        layout: RecordingLayout,
        current_frame: f64,
        fps: f64,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
    ) {
        let Some(body) = layout.track_body else {
            return;
        };
        self.pixels_per_frame = crate::constants::PIXELS_PER_FRAME * scroll_speed;
        let half_visible_frames = body.width as f64 / self.pixels_per_frame.max(0.001) as f64 / 2.0;
        let offset_frames = crate::rythmo_layout::reading_bar_offset_seconds(
            reading_bar_offset_percent,
            body.width,
            fps,
            self.pixels_per_frame,
        ) * fps;
        self.view_start_frame = current_frame - half_visible_frames + offset_frames;
    }

    pub fn sync_track_count(&mut self, track_count: usize) {
        self.track_count = track_count;
        if track_count == 0 {
            self.track_scroll = 0;
        }
    }

    pub fn sync_asset_content(&mut self, project: &RecordingProject) {
        let groups = grouped_assets(project);
        self.asset_content_height = groups.len() as f32 * ASSET_GROUP_H
            + groups
                .iter()
                .filter(|(owner, _)| self.expanded_asset_owners.contains(*owner))
                .map(|(_, assets)| assets.len() as f32 * ASSET_ROW_H)
                .sum::<f32>();
    }

    pub fn toggle_asset_group(&mut self, owner: &str) {
        if !self.expanded_asset_owners.remove(owner) {
            self.expanded_asset_owners.insert(owner.to_owned());
        }
    }

    pub fn is_asset_group_expanded(&self, owner: &str) -> bool {
        self.expanded_asset_owners.contains(owner)
    }

    pub fn has_asset_context_menu(&self) -> bool {
        self.asset_context_menu.is_some()
    }

    /// Scroll the track list and the audio library so a control that just
    /// received the keyboard focus is on screen (the time axis follows the
    /// playhead and is left alone).
    pub fn reveal(&mut self, place: RecordingControlPlace, layout: RecordingLayout) {
        if let (Some(row), Some(body)) = (place.track_row, layout.track_body) {
            let visible = visible_track_rows(body);
            let scroll = effective_track_scroll(body, self.track_count, self.track_scroll);
            if row < scroll {
                self.track_scroll = row;
            } else if row >= scroll + visible {
                self.track_scroll = row + 1 - visible;
            }
        }
        if let (Some(offset), Some(rect)) = (place.library_offset, layout.assets) {
            let viewport = (rect.height - 40.0).max(1.0);
            if offset < self.asset_scroll {
                self.asset_scroll = offset;
            } else if offset + ASSET_ROW_H > self.asset_scroll + viewport {
                self.asset_scroll = offset + ASSET_ROW_H - viewport;
            }
        }
    }

    pub fn handle_asset_scroll(&mut self, event: &UiEvent, layout: RecordingLayout) -> bool {
        let Some(rect) = layout.assets else {
            return false;
        };
        let Some((track, thumb, max_scroll)) =
            asset_scrollbar_geometry(rect, self.asset_content_height, self.asset_scroll)
        else {
            self.asset_scroll = 0.0;
            self.dragging_asset_scrollbar = false;
            return false;
        };
        self.asset_scroll = self.asset_scroll.min(max_scroll);
        match event {
            UiEvent::MousePress { x, y } if thumb.contains(*x, *y) => {
                self.dragging_asset_scrollbar = true;
                self.asset_scrollbar_drag_offset = *y - thumb.y;
                true
            }
            UiEvent::MousePress { x, y } if track.contains(*x, *y) => {
                let travel = (track.height - thumb.height).max(1.0);
                self.asset_scroll =
                    (((*y - track.y - thumb.height / 2.0) / travel).clamp(0.0, 1.0)) * max_scroll;
                self.dragging_asset_scrollbar = true;
                self.asset_scrollbar_drag_offset = thumb.height / 2.0;
                true
            }
            UiEvent::MouseMove { y, .. } if self.dragging_asset_scrollbar => {
                let travel = (track.height - thumb.height).max(1.0);
                self.asset_scroll = (((*y - self.asset_scrollbar_drag_offset - track.y) / travel)
                    .clamp(0.0, 1.0))
                    * max_scroll;
                true
            }
            UiEvent::MouseRelease { .. } if self.dragging_asset_scrollbar => {
                self.dragging_asset_scrollbar = false;
                true
            }
            UiEvent::Scroll { x, y, delta, .. } if rect.contains(*x, *y) => {
                self.asset_scroll =
                    (self.asset_scroll - *delta * ASSET_ROW_H).clamp(0.0, max_scroll);
                true
            }
            _ => false,
        }
    }

    pub fn handle_asset_context_menu(
        &mut self,
        event: &UiEvent,
        scene: &RecordingScene,
        screen: Rect,
    ) -> Option<EventResponse> {
        if let UiEvent::ContextMenu { x, y } = event {
            let asset_id = scene.controls.iter().rev().find_map(|control| {
                (control.bounds.contains(*x, *y))
                    .then_some(&control.control)
                    .and_then(|control| match control {
                        RecordingControl::Asset(id) => Some(*id),
                        _ => None,
                    })
            });
            let Some(asset_id) = asset_id else {
                self.asset_context_menu = None;
                return Some(EventResponse::Consumed);
            };
            self.open_asset_context_menu(asset_id, *x, *y, screen);
            return Some(self.asset_menu_announcement(true));
        }

        if matches!(event, UiEvent::OpenContextMenu) && self.asset_context_menu.is_none() {
            let asset_id = self.selected_asset?;
            let control = scene
                .controls
                .iter()
                .find(|control| control.control == RecordingControl::Asset(asset_id))?;
            let (x, y) = if control.visible {
                (
                    control.bounds.x + 16.0,
                    control.bounds.y + control.bounds.height,
                )
            } else {
                (
                    screen.x + screen.width * 0.5,
                    screen.y + screen.height * 0.5,
                )
            };
            self.open_asset_context_menu(asset_id, x, y, screen);
            return Some(self.asset_menu_announcement(true));
        }

        let menu = self.asset_context_menu.as_mut()?;
        let item_at = |menu: &AssetContextMenu, x: f32, y: f32| {
            (0..ASSET_MENU_ITEMS.len()).find(|index| menu.item_rect(*index).contains(x, y))
        };
        let activate = |menu: &mut AssetContextMenu| -> Option<UiAction> {
            if menu.submenu_open && menu.hover_voicelines {
                return Some(UiAction::RecordingSendAssetToVoicelines(menu.asset_id));
            }
            match ASSET_MENU_ITEMS[menu.highlighted] {
                AssetMenuItem::PlaceAtPlayhead => {
                    Some(UiAction::RecordingPlaceAssetAtPlayhead(Some(menu.asset_id)))
                }
                AssetMenuItem::SendTo => {
                    menu.submenu_open = true;
                    menu.hover_voicelines = true;
                    None
                }
                AssetMenuItem::Delete => Some(UiAction::RecordingDeleteSelectedAsset),
            }
        };
        match event {
            UiEvent::MouseMove { x, y } => {
                if let Some(index) = item_at(menu, *x, *y) {
                    menu.highlighted = index;
                    menu.submenu_open = ASSET_MENU_ITEMS[index] == AssetMenuItem::SendTo;
                }
                menu.hover_voicelines = menu.submenu_open && menu.submenu_rect().contains(*x, *y);
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                if (menu.submenu_open && menu.submenu_rect().contains(*x, *y))
                    || item_at(menu, *x, *y).is_some() =>
            {
                if let Some(index) = item_at(menu, *x, *y) {
                    menu.highlighted = index;
                    menu.hover_voicelines = false;
                } else {
                    menu.hover_voicelines = true;
                }
                match activate(menu) {
                    Some(action) => {
                        self.asset_context_menu = None;
                        Some(EventResponse::Action(action))
                    }
                    None => Some(EventResponse::Consumed),
                }
            }
            UiEvent::CursorUp | UiEvent::CursorDown => {
                let count = ASSET_MENU_ITEMS.len();
                menu.highlighted = if matches!(event, UiEvent::CursorDown) {
                    (menu.highlighted + 1) % count
                } else {
                    (menu.highlighted + count - 1) % count
                };
                menu.submenu_open = false;
                menu.hover_voicelines = false;
                Some(self.asset_menu_announcement(false))
            }
            UiEvent::CursorRight if ASSET_MENU_ITEMS[menu.highlighted] == AssetMenuItem::SendTo => {
                menu.submenu_open = true;
                menu.hover_voicelines = true;
                Some(self.asset_menu_announcement(false))
            }
            UiEvent::CursorLeft if menu.submenu_open => {
                menu.submenu_open = false;
                menu.hover_voicelines = false;
                Some(self.asset_menu_announcement(false))
            }
            UiEvent::Activate => match activate(menu) {
                Some(action) => {
                    self.asset_context_menu = None;
                    Some(EventResponse::Action(action))
                }
                None => Some(self.asset_menu_announcement(false)),
            },
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => match activate(menu) {
                Some(action) => {
                    self.asset_context_menu = None;
                    Some(EventResponse::Action(action))
                }
                None => Some(self.asset_menu_announcement(false)),
            },
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.asset_context_menu = None;
                Some(EventResponse::Action(UiAction::Accessibility(
                    AccessibilityEvent::Closed {
                        label: t("recording.asset_menu.title").to_string(),
                    },
                )))
            }
            UiEvent::MousePress { .. } | UiEvent::OpenContextMenu => {
                self.asset_context_menu = None;
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn open_asset_context_menu(&mut self, asset_id: AudioAssetId, x: f32, y: f32, screen: Rect) {
        let (x, y) = super::context_menu::clamped_origin(
            x,
            y,
            ASSET_MENU_W * 1.7 - 2.0,
            ASSET_MENU_ITEM_H * ASSET_MENU_ITEMS.len() as f32,
            screen.x + screen.width,
            screen.y + screen.height,
        );
        self.selected_asset = Some(asset_id);
        self.dragging_asset = None;
        self.asset_context_menu = Some(AssetContextMenu {
            asset_id,
            x,
            y,
            highlighted: 0,
            submenu_open: false,
            hover_voicelines: false,
        });
    }

    /// Spoken highlighted entry of the audio menu; `opened` prefixes the
    /// menu title, as for any menu that just opened.
    fn asset_menu_announcement(&self, opened: bool) -> EventResponse {
        let Some(menu) = self.asset_context_menu else {
            return EventResponse::Consumed;
        };
        let item = if menu.submenu_open && menu.hover_voicelines {
            t("workspace_tabs.voicelines").to_string()
        } else {
            ASSET_MENU_ITEMS[menu.highlighted].label()
        };
        menu_focus(if opened {
            format!("{} : {item}", t("recording.asset_menu.title"))
        } else {
            item
        })
    }

    pub fn handle_track_scroll(&mut self, event: &UiEvent, layout: RecordingLayout) -> bool {
        let Some(headers) = layout.track_headers else {
            return false;
        };
        let Some(body) = layout.track_body else {
            return false;
        };
        let Some((track, thumb, max_scroll)) =
            track_scrollbar_geometry(body, self.track_count, self.track_scroll)
        else {
            self.track_scroll = 0;
            self.dragging_track_scrollbar = false;
            return false;
        };
        self.track_scroll = self.track_scroll.min(max_scroll);

        match event {
            UiEvent::MousePress { x, y } if thumb.contains(*x, *y) => {
                self.dragging_track_scrollbar = true;
                self.track_scrollbar_drag_offset = *y - thumb.y;
                true
            }
            UiEvent::MousePress { x, y } if track.contains(*x, *y) => {
                let travel = (track.height - thumb.height).max(1.0);
                let ratio = ((*y - track.y - thumb.height / 2.0) / travel).clamp(0.0, 1.0);
                self.track_scroll = (ratio * max_scroll as f32).round() as usize;
                self.dragging_track_scrollbar = true;
                self.track_scrollbar_drag_offset = thumb.height / 2.0;
                true
            }
            UiEvent::MouseMove { y, .. } if self.dragging_track_scrollbar => {
                let travel = (track.height - thumb.height).max(1.0);
                let ratio =
                    ((*y - self.track_scrollbar_drag_offset - track.y) / travel).clamp(0.0, 1.0);
                self.track_scroll = (ratio * max_scroll as f32).round() as usize;
                true
            }
            UiEvent::MouseRelease { .. } if self.dragging_track_scrollbar => {
                self.dragging_track_scrollbar = false;
                true
            }
            UiEvent::Scroll { x, y, delta, .. }
                if headers.contains(*x, *y) || body.contains(*x, *y) =>
            {
                if *delta > 0.0 {
                    self.track_scroll = self.track_scroll.saturating_sub(1);
                } else if *delta < 0.0 {
                    self.track_scroll = (self.track_scroll + 1).min(max_scroll);
                }
                true
            }
            _ => false,
        }
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
        current_frame: f64,
        countdown_seconds: Option<u32>,
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
            countdown_seconds,
        )
    }

    pub fn audio_import_prompt_scene(&self, screen: Rect) -> Option<RecordingScene> {
        self.pending_audio_import.as_ref()?;
        let mut scene = RecordingScene::default();
        push_audio_import_prompt(&mut scene, self, screen);
        Some(scene)
    }
}

fn choice_scene(layout: RecordingLayout) -> RecordingScene {
    let mut scene = RecordingScene::default();
    push_quad(&mut scene.quads, layout.content, PANEL_BG, BORDER, 0.0);
    let gap = 28.0;
    let card_w = ((layout.content.width - gap).max(0.0) * 0.5).min(420.0);
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
            visible: true,
            place: RecordingControlPlace::default(),
        });
    }
    scene
}

fn countdown_font_size(seconds: u32) -> f32 {
    match seconds {
        3 => 92.0,
        2 => 126.0,
        1 => 164.0,
        _ => 92.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn timeline_scene(
    ui: &RecordingWorkspaceUi,
    layout: RecordingLayout,
    project: &RecordingProject,
    capture: Option<&CaptureState>,
    participants: &[NetworkMember],
    control_owner_id: Option<&str>,
    current_frame: f64,
    countdown_seconds: Option<u32>,
) -> RecordingScene {
    let mut scene = RecordingScene::default();
    push_quad(
        &mut scene.quads,
        layout.video,
        [0.0, 0.0, 0.0, 1.0],
        [0.0; 4],
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
            ui.selected_clips().next().is_some(),
            project
                .armed_track_id()
                .and_then(|track_id| project.track(track_id)),
        );
        push_tracks(&mut scene, ui, project, headers, body, current_frame);
        push_track_scrollbar(&mut scene, ui, project.tracks().count(), body);
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
                let bounds = Rect {
                    x: layout.video.x,
                    y: layout.video.y,
                    width: layout.video.width,
                    height: layout.video.height,
                };
                let seconds = countdown_seconds.unwrap_or(0);
                let text = seconds.to_string();
                let font_size = countdown_font_size(seconds);
                // A few one-pixel offsets give the countdown a bold outline
                // without adding a second text-rendering API just for this UI.
                for (dx, dy) in [(0.0, 0.0), (-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    scene.labels.push(label(
                        text.clone(),
                        Rect {
                            x: bounds.x + dx,
                            y: bounds.y + dy,
                            ..bounds
                        },
                        font_size,
                        [255, 32, 48],
                    ));
                }
            }
            CaptureState::Capturing { .. } => {
                let bounds = Rect {
                    x: layout.video.x + 14.0,
                    y: layout.video.y + 12.0,
                    width: 380.0_f32.min(layout.video.width - 28.0).max(0.0),
                    height: 30.0,
                };
                // Labels are drawn above the video, quads below it: a dark
                // outline keeps the red text readable over any picture.
                for (dx, dy, color) in [
                    (-1.0, 0.0, [0, 0, 0]),
                    (1.0, 0.0, [0, 0, 0]),
                    (0.0, -1.0, [0, 0, 0]),
                    (0.0, 1.0, [0, 0, 0]),
                    (0.0, 0.0, [255, 120, 128]),
                ] {
                    scene.labels.push(RecordingLabel {
                        text: t("recording.capture.active").into(),
                        bounds: Rect {
                            x: bounds.x + dx,
                            y: bounds.y + dy,
                            ..bounds
                        },
                        h_align: HAlign::Left,
                        v_align: VAlign::Center,
                        overflow: Overflow::Ellipsis,
                        font_size: 15.0,
                        color,
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(prompt) = ui.audio_import_prompt_scene(layout.content) {
        scene.controls.extend(prompt.controls);
    }
    if let Some(menu) = ui.asset_context_menu {
        push_asset_context_menu(&mut scene, menu);
    }
    scene
}

fn push_asset_context_menu(scene: &mut RecordingScene, menu: AssetContextMenu) {
    for (index, item) in ASSET_MENU_ITEMS.into_iter().enumerate() {
        let row = menu.item_rect(index);
        push_quad(
            &mut scene.system_quads,
            row,
            [0.13, 0.13, 0.16, 0.99],
            BORDER,
            0.0,
        );
        if menu.highlighted == index && !(menu.submenu_open && menu.hover_voicelines) {
            push_quad(
                &mut scene.system_quads,
                inset_rect(row, 3.0),
                [0.31, 0.40, 0.72, 0.85],
                [0.0; 4],
                0.0,
            );
        }
        scene.system_labels.push(RecordingLabel {
            text: item.label(),
            bounds: Rect {
                x: row.x + 10.0,
                width: row.width - 34.0,
                ..row
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 13.0,
            color: TEXT,
        });
        if item == AssetMenuItem::SendTo {
            scene.system_labels.push(label(
                ">",
                Rect {
                    x: row.x + row.width - 24.0,
                    width: 18.0,
                    ..row
                },
                13.0,
                MUTED_TEXT,
            ));
        }
    }

    if menu.submenu_open {
        let submenu = menu.submenu_rect();
        push_quad(
            &mut scene.system_quads,
            submenu,
            [0.13, 0.13, 0.16, 0.99],
            BORDER,
            0.0,
        );
        if menu.hover_voicelines {
            push_quad(
                &mut scene.system_quads,
                inset_rect(submenu, 3.0),
                [0.31, 0.40, 0.72, 0.85],
                [0.0; 4],
                0.0,
            );
        }
        scene.system_labels.push(RecordingLabel {
            text: t("workspace_tabs.voicelines").into(),
            bounds: Rect {
                x: submenu.x + 10.0,
                width: submenu.width - 20.0,
                ..submenu
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 13.0,
            color: TEXT,
        });
    }
}

fn inset_rect(rect: Rect, amount: f32) -> Rect {
    Rect {
        x: rect.x + amount,
        y: rect.y + amount,
        width: (rect.width - amount * 2.0).max(0.0),
        height: (rect.height - amount * 2.0).max(0.0),
    }
}

fn push_audio_import_prompt(scene: &mut RecordingScene, ui: &RecordingWorkspaceUi, screen: Rect) {
    push_quad(
        &mut scene.quads,
        screen,
        [0.0, 0.0, 0.0, 0.72],
        [0.0; 4],
        0.0,
    );
    let card = Rect {
        x: screen.x + (screen.width - 480.0).max(0.0) * 0.5,
        y: screen.y + (screen.height - 190.0).max(0.0) * 0.5,
        width: 480.0_f32.min(screen.width),
        height: 190.0_f32.min(screen.height),
    };
    push_quad(&mut scene.quads, card, PANEL_ALT, BORDER, 10.0);
    scene.labels.push(label(
        crate::i18n::t("recording.audio.username_prompt"),
        Rect {
            height: 48.0,
            ..card
        },
        18.0,
        TEXT,
    ));
    let input = Rect {
        x: card.x + 28.0,
        y: card.y + 62.0,
        width: card.width - 56.0,
        height: 44.0,
    };
    push_quad(&mut scene.quads, input, PANEL_BG, ACCENT, 6.0);
    scene.labels.push(RecordingLabel {
        text: ui.import_username.clone(),
        bounds: input,
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size: 15.0,
        color: TEXT,
    });
    scene.labels.push(label(
        crate::i18n::t("recording.audio.username_hint"),
        Rect {
            x: card.x + 28.0,
            y: card.y + 120.0,
            width: card.width - 56.0,
            height: 42.0,
        },
        12.0,
        MUTED_TEXT,
    ));
    scene.controls.push(RecordingControlInfo {
        control: RecordingControl::ImportUsername,
        bounds: input,
        role: AccessibleRole::TextField,
        label: crate::i18n::t("recording.audio.username_label").into(),
        value: Some(ui.import_username.clone()),
        selected: false,
        enabled: true,
        visible: true,
        place: RecordingControlPlace::default(),
    });
}

fn push_tool_controls(
    scene: &mut RecordingScene,
    tools: Rect,
    active: RecordingTool,
    enabled: bool,
    has_selection: bool,
    armed_track: Option<&AudioTrack>,
) {
    let push_control =
        |scene: &mut RecordingScene, control, bounds, label: String, value, enabled| {
            scene.controls.push(RecordingControlInfo {
                control,
                bounds,
                role: AccessibleRole::Button,
                label,
                value,
                selected: false,
                enabled,
                visible: true,
                place: RecordingControlPlace::default(),
            });
        };
    for (index, (tool, key)) in [
        (RecordingTool::Select, "recording.tool.select"),
        (RecordingTool::Cut, "recording.tool.cut"),
    ]
    .into_iter()
    .enumerate()
    {
        let bounds = Rect {
            x: tools.x + 7.0,
            y: tools.y + 8.0 + index as f32 * 40.0,
            width: tools.width - 14.0,
            height: 34.0,
        };
        let selected = tool == active;
        push_quad_with_border(
            &mut scene.quads,
            bounds,
            if selected { ACCENT } else { PANEL_BG },
            if selected { ACTIVE_BORDER } else { BORDER },
            if selected { 2.0 } else { 1.0 },
            5.0,
        );
        scene.labels.push(label(
            t(key),
            bounds,
            15.0,
            if enabled { TEXT } else { MUTED_TEXT },
        ));
        push_control(
            scene,
            RecordingControl::Tool(tool),
            bounds,
            t(key).into(),
            selected.then(|| t("recording.a11y.tool_active").to_string()),
            enabled,
        );
    }
    let add_bounds = Rect {
        x: tools.x + 7.0,
        y: tools.y + 88.0,
        width: (tools.width - 18.0) * 0.5,
        height: 30.0,
    };
    push_quad(&mut scene.quads, add_bounds, PANEL_BG, BORDER, 5.0);
    scene.labels.push(label(
        "+",
        add_bounds,
        18.0,
        if enabled { TEXT } else { MUTED_TEXT },
    ));
    push_control(
        scene,
        RecordingControl::AddTrack,
        add_bounds,
        t("recording.track.add").into(),
        None,
        enabled,
    );
    let delete_bounds = Rect {
        x: add_bounds.x + add_bounds.width + 4.0,
        y: add_bounds.y,
        width: add_bounds.width,
        height: add_bounds.height,
    };
    push_quad(&mut scene.quads, delete_bounds, PANEL_BG, BORDER, 5.0);
    scene.labels.push(label(
        "×",
        delete_bounds,
        16.0,
        if enabled && has_selection {
            TEXT
        } else {
            MUTED_TEXT
        },
    ));
    // Like the record button below, it stays reachable and says why it
    // cannot act yet.
    push_control(
        scene,
        RecordingControl::DeleteSelectedClips,
        delete_bounds,
        t("recording.clip.delete_selected").into(),
        (!has_selection).then(|| t("recording.a11y.no_take_selected").to_string()),
        enabled,
    );
    let record_bounds = Rect {
        x: tools.x + 7.0,
        y: tools.y + 124.0,
        width: tools.width - 14.0,
        height: 32.0,
    };
    push_quad(
        &mut scene.quads,
        record_bounds,
        if armed_track.is_some() {
            RECORD
        } else {
            [0.13, 0.13, 0.16, 1.0]
        },
        BORDER,
        5.0,
    );
    scene.labels.push(label(
        "● REC",
        record_bounds,
        13.0,
        if enabled && armed_track.is_some() {
            TEXT
        } else {
            MUTED_TEXT
        },
    ));
    // Reachable without an armed track: activating it then says what is
    // missing instead of the button silently doing nothing.
    push_control(
        scene,
        RecordingControl::StartCapture,
        record_bounds,
        t("recording.capture.start").into(),
        Some(match armed_track {
            Some(track) => t("recording.a11y.record_on").replace("{track}", &track.name),
            None => t("recording.a11y.no_armed_track").to_string(),
        }),
        enabled,
    );
}

fn visible_track_rows(body: Rect) -> usize {
    (body.height / TRACK_ROW_H).floor().max(1.0) as usize
}

fn effective_track_scroll(body: Rect, track_count: usize, scroll: usize) -> usize {
    scroll.min(track_count.saturating_sub(visible_track_rows(body)))
}

fn track_scrollbar_geometry(
    body: Rect,
    track_count: usize,
    scroll: usize,
) -> Option<(Rect, Rect, usize)> {
    let visible = visible_track_rows(body);
    let max_scroll = track_count.saturating_sub(visible);
    if max_scroll == 0 {
        return None;
    }
    let track = Rect {
        x: body.x + body.width - 10.0,
        y: body.y + 6.0,
        width: 4.0,
        height: (body.height - 12.0).max(1.0),
    };
    let thumb_h = (track.height * visible as f32 / track_count as f32)
        .clamp(track.height.min(24.0), track.height);
    let travel = (track.height - thumb_h).max(0.0);
    let ratio = effective_track_scroll(body, track_count, scroll) as f32 / max_scroll as f32;
    let thumb = Rect {
        x: track.x,
        y: track.y + ratio * travel,
        width: track.width,
        height: thumb_h,
    };
    Some((track, thumb, max_scroll))
}

fn push_track_scrollbar(
    scene: &mut RecordingScene,
    ui: &RecordingWorkspaceUi,
    track_count: usize,
    body: Rect,
) {
    let Some((track, thumb, _)) = track_scrollbar_geometry(body, track_count, ui.track_scroll)
    else {
        return;
    };
    push_quad(
        &mut scene.quads,
        track,
        [0.12, 0.13, 0.17, 0.9],
        [0.0; 4],
        2.0,
    );
    push_quad(
        &mut scene.quads,
        thumb,
        [0.48, 0.50, 0.62, 0.95],
        [0.0; 4],
        2.0,
    );
}

fn push_tracks(
    scene: &mut RecordingScene,
    ui: &RecordingWorkspaceUi,
    project: &RecordingProject,
    headers: Rect,
    body: Rect,
    current_frame: f64,
) {
    let track_count = project.tracks().count();
    let visible_rows = visible_track_rows(body);
    let scroll = effective_track_scroll(body, track_count, ui.track_scroll);
    let can_edit = ui.role.can_edit_timeline();
    let mut takes_per_track = BTreeMap::<AudioTrackId, usize>::new();
    for clip in project.clips() {
        *takes_per_track.entry(clip.track_id).or_default() += 1;
    }
    for (row, track) in project.tracks().enumerate() {
        // Every track keeps its controls in the focus order; only the rows
        // in view are drawn and reachable by the pointer.
        let visible = (scroll..scroll + visible_rows).contains(&row);
        let place = RecordingControlPlace {
            track_row: Some(row),
            ..RecordingControlPlace::default()
        };
        let push_control =
            |scene: &mut RecordingScene, control, bounds, role, label, value, enabled| {
                scene.controls.push(RecordingControlInfo {
                    control,
                    bounds: if visible { bounds } else { HIDDEN_BOUNDS },
                    role,
                    label,
                    value,
                    selected: false,
                    enabled,
                    visible,
                    place,
                });
            };
        let y = headers.y + row.saturating_sub(scroll) as f32 * TRACK_ROW_H;
        let header = Rect {
            x: headers.x,
            y,
            width: headers.width,
            height: TRACK_ROW_H,
        };
        let lane = Rect {
            x: body.x,
            y,
            width: body.width,
            height: TRACK_ROW_H,
        };
        if visible {
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
        }
        let volume_bounds = Rect {
            x: header.x + header.width - 70.0,
            y: header.y + 2.0,
            width: 42.0,
            height: 24.0,
        };
        let name_bounds = Rect {
            x: header.x + 8.0,
            y: header.y + 4.0,
            width: (volume_bounds.x - header.x - 12.0).max(24.0),
            height: 22.0,
        };
        let track_volume = ui.track_volume(track.id);
        if visible {
            scene.labels.push(RecordingLabel {
                text: if ui.renaming_track == Some(track.id) {
                    ui.rename_buffer.clone()
                } else {
                    track.name.clone()
                },
                bounds: name_bounds,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                font_size: 13.0,
                color: TEXT,
            });
        }
        // The name control stands for the whole track: everyone can reach it
        // and hear the track state, editors also rename it from there.
        push_control(
            scene,
            RecordingControl::RenameTrack(track.id),
            name_bounds,
            if can_edit {
                AccessibleRole::Button
            } else {
                AccessibleRole::ListItem
            },
            if can_edit {
                track_control_label(track, "recording.track.rename")
            } else {
                track.name.clone()
            },
            Some(track_accessible_summary(
                track,
                track_volume,
                takes_per_track.get(&track.id).copied().unwrap_or_default(),
            )),
            true,
        );
        if visible {
            let volume_track = Rect {
                x: volume_bounds.x + 3.0,
                y: volume_bounds.y + 18.0,
                width: volume_bounds.width - 6.0,
                height: 3.0,
            };
            push_quad(
                &mut scene.quads,
                volume_track,
                [0.18, 0.19, 0.23, 1.0],
                [0.0; 4],
                1.5,
            );
            push_quad(
                &mut scene.quads,
                Rect {
                    width: volume_track.width
                        * (track_volume / crate::recording_mix::TRACK_VOLUME_MAX),
                    ..volume_track
                },
                ACCENT,
                [0.0; 4],
                1.5,
            );
            let knob_x = volume_track.x
                + volume_track.width * (track_volume / crate::recording_mix::TRACK_VOLUME_MAX);
            push_quad(
                &mut scene.quads,
                Rect {
                    x: knob_x - 2.0,
                    y: volume_bounds.y + 14.0,
                    width: 4.0,
                    height: 11.0,
                },
                [0.9, 0.9, 0.95, 1.0],
                [0.0; 4],
                1.5,
            );
            scene.labels.push(label(
                format!("{:.0}%", track_volume * 100.0),
                Rect {
                    height: 15.0,
                    ..volume_bounds
                },
                10.0,
                TEXT,
            ));
        }
        push_control(
            scene,
            RecordingControl::TrackVolume(track.id),
            volume_bounds,
            AccessibleRole::Slider,
            track_control_label(track, "recording.track.volume"),
            Some(format!("{:.0} %", track_volume * 100.0)),
            ui.role.can_adjust_track_volume(),
        );
        let track_reverb = ui.track_reverb(track.id);
        for (index, (control, text, key, active, color)) in [
            (
                RecordingControl::TrackMute(track.id),
                "M",
                "recording.track.mute",
                Some(track.muted),
                [0.76, 0.55, 0.18, 1.0],
            ),
            (
                RecordingControl::TrackSolo(track.id),
                "S",
                "recording.track.solo",
                Some(track.solo),
                [0.78, 0.66, 0.18, 1.0],
            ),
            (
                RecordingControl::TrackArm(track.id),
                "R",
                "recording.track.arm",
                Some(track.armed),
                RECORD,
            ),
            (
                RecordingControl::TrackExport(track.id),
                "E",
                "recording.track.export",
                None,
                [0.25, 0.65, 0.35, 1.0],
            ),
            (
                RecordingControl::TrackReverb(track.id),
                "Rev",
                "recording.track.reverb",
                Some(track_reverb),
                [0.32, 0.45, 0.82, 1.0],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let bounds = Rect {
                x: header.x + 8.0 + index as f32 * 28.0,
                y: header.y + 30.0,
                width: if text == "Rev" { 32.0 } else { 24.0 },
                height: 22.0,
            };
            let on = active == Some(true);
            if visible {
                // An active toggle also gets a thick light border, so its
                // state does not depend on telling the fill colours apart.
                push_quad_with_border(
                    &mut scene.quads,
                    bounds,
                    if on { color } else { [0.13, 0.13, 0.16, 1.0] },
                    if on { ACTIVE_BORDER } else { BORDER },
                    if on { 2.0 } else { 1.0 },
                    4.0,
                );
                scene.labels.push(label(text, bounds, 12.0, TEXT));
            }
            push_control(
                scene,
                control,
                bounds,
                AccessibleRole::Button,
                track_control_label(track, key),
                active.map(on_off),
                if text == "Rev" {
                    ui.role.can_adjust_track_volume()
                } else {
                    can_edit
                },
            );
        }
        let remove_bounds = Rect {
            x: header.x + header.width - 28.0,
            y: header.y + 4.0,
            width: 22.0,
            height: 22.0,
        };
        let removable = track_count > 1;
        if visible {
            scene.labels.push(label(
                "×",
                remove_bounds,
                20.0,
                if removable { TEXT } else { MUTED_TEXT },
            ));
        }
        push_control(
            scene,
            RecordingControl::RemoveTrack(track.id),
            remove_bounds,
            AccessibleRole::Button,
            track_control_label(track, "recording.track.remove"),
            (!removable).then(|| t("recording.a11y.last_track").to_string()),
            can_edit,
        );
    }

    // Takes are listed track by track, in time order, whether or not they
    // are on screen: the keyboard reaches every one of them.
    let rows = project
        .tracks()
        .enumerate()
        .map(|(row, track)| (track.id, row))
        .collect::<BTreeMap<_, _>>();
    let mut clips = project
        .clips()
        .filter_map(|clip| rows.get(&clip.track_id).map(|row| (*row, clip)))
        .collect::<Vec<_>>();
    clips.sort_by_key(|(row, clip)| (*row, clip.start_frame, clip.id.get()));
    for (track_row, clip) in clips {
        let selected = ui.is_clip_selected(clip.id);
        let (label, value) = clip_accessible_text(project, clip);
        let mut control = RecordingControlInfo {
            control: RecordingControl::Clip(clip.id),
            bounds: HIDDEN_BOUNDS,
            role: AccessibleRole::ListItem,
            label,
            value: Some(value),
            selected,
            enabled: true,
            visible: false,
            place: RecordingControlPlace {
                track_row: Some(track_row),
                start_frame: Some(clip.start_frame),
                end_frame: Some(clip.end_frame()),
                library_offset: None,
            },
        };
        if let Some(visible_row) = track_row
            .checked_sub(scroll)
            .filter(|visible_row| *visible_row < visible_rows)
        {
            let x = body.x
                + (clip.start_frame as f64 - ui.view_start_frame) as f32 * ui.pixels_per_frame;
            let width = (clip.duration_frames as f32 * ui.pixels_per_frame).max(3.0);
            let clip_bounds = Rect {
                x,
                y: body.y + visible_row as f32 * TRACK_ROW_H + 6.0,
                width,
                height: TRACK_ROW_H - 12.0,
            };
            let left = clip_bounds.x.max(body.x);
            let right = (clip_bounds.x + clip_bounds.width).min(body.x + body.width);
            if right > left {
                let bounds = Rect {
                    x: left,
                    width: right - left,
                    ..clip_bounds
                };
                let (border, border_width) = if selected {
                    (ACTIVE_BORDER, 2.0)
                } else {
                    (BORDER, 1.0)
                };
                push_quad_with_border(
                    &mut scene.quads,
                    bounds,
                    if selected {
                        [0.20, 0.27, 0.62, 1.0]
                    } else {
                        [0.13, 0.20, 0.46, 1.0]
                    },
                    border,
                    border_width,
                    5.0,
                );
                if let Some(asset) = project.asset(clip.asset_id) {
                    push_clip_waveform(
                        &mut scene.quads,
                        bounds,
                        clip_bounds,
                        &asset.waveform,
                        asset.sample_rate,
                        project.timeline_fps(),
                        clip.source_start_frame,
                        ui.pixels_per_frame,
                    );
                    // Keep the selection border above the waveform without repainting it.
                    push_quad_with_border(
                        &mut scene.quads,
                        bounds,
                        [0.0; 4],
                        border,
                        border_width,
                        5.0,
                    );
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
                        font_size: 12.0,
                        color: TEXT,
                    });
                }
                control.bounds = bounds;
                control.visible = true;
            }
        }
        scene.controls.push(control);
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
}

fn push_assets(
    scene: &mut RecordingScene,
    ui: &RecordingWorkspaceUi,
    project: &RecordingProject,
    rect: Rect,
) {
    scene.labels.push(RecordingLabel {
        text: t("recording.assets.title").into(),
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
    let viewport_top = rect.y + 40.0;
    let viewport_bottom = rect.y + rect.height;
    let max_scroll = (ui.asset_content_height - (rect.height - 40.0).max(1.0)).max(0.0);
    let scroll = ui.asset_scroll.min(max_scroll);
    let used_assets = project
        .clips()
        .map(|clip| clip.asset_id)
        .collect::<BTreeSet<_>>();
    // Offset of the current row in the unscrolled library content.
    let mut offset = 0.0;
    for (owner, assets) in grouped_assets(project) {
        let expanded = ui.expanded_asset_owners.contains(&owner);
        let y = viewport_top + offset - scroll;
        let group_bounds = Rect {
            x: rect.x + 10.0,
            y,
            width: rect.width - 24.0,
            height: ASSET_GROUP_H,
        };
        let visible = y >= viewport_top && y + ASSET_GROUP_H <= viewport_bottom;
        if visible {
            push_asset_chevron(scene, group_bounds, expanded);
            scene.labels.push(RecordingLabel {
                text: owner.clone(),
                bounds: Rect {
                    x: group_bounds.x + 18.0,
                    width: group_bounds.width - 18.0,
                    ..group_bounds
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                font_size: 13.0,
                color: MUTED_TEXT,
            });
        }
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::AssetGroup(owner.clone()),
            bounds: if visible { group_bounds } else { HIDDEN_BOUNDS },
            role: AccessibleRole::Button,
            label: owner.clone(),
            value: Some(format!(
                "{}, {}",
                count_label(
                    assets.len(),
                    "recording.a11y.audios_one",
                    "recording.a11y.audios_many"
                ),
                t(if expanded {
                    "recording.a11y.group_expanded"
                } else {
                    "recording.a11y.group_collapsed"
                })
            )),
            selected: false,
            enabled: true,
            visible,
            place: RecordingControlPlace {
                library_offset: Some(offset),
                ..RecordingControlPlace::default()
            },
        });
        offset += ASSET_GROUP_H;
        if !expanded {
            continue;
        }
        for asset in assets {
            let row_offset = offset;
            offset += ASSET_ROW_H;
            let bounds = Rect {
                x: rect.x + 10.0,
                y: viewport_top + row_offset - scroll + 3.0,
                width: rect.width - 24.0,
                height: 36.0,
            };
            let visible = bounds.y >= viewport_top && bounds.y + bounds.height <= viewport_bottom;
            let selected = ui.selected_asset == Some(asset.id);
            let used = used_assets.contains(&asset.id);
            if visible {
                let fill = match (used, selected) {
                    (true, true) => USED_AUDIO_SELECTED,
                    (true, false) => USED_AUDIO,
                    (false, true) => ACCENT,
                    (false, false) => PANEL_BG,
                };
                push_quad_with_border(
                    &mut scene.quads,
                    bounds,
                    fill,
                    if selected { ACTIVE_BORDER } else { BORDER },
                    if selected { 2.0 } else { 1.0 },
                    5.0,
                );
                if used {
                    push_quad(
                        &mut scene.quads,
                        Rect {
                            x: bounds.x + 4.0,
                            y: bounds.y + 6.0,
                            width: 4.0,
                            height: bounds.height - 12.0,
                        },
                        USED_AUDIO_MARK,
                        [0.0; 4],
                        2.0,
                    );
                }
                scene.labels.push(RecordingLabel {
                    text: asset.file_name.clone(),
                    bounds: Rect {
                        x: bounds.x + 12.0,
                        width: bounds.width - 12.0,
                        ..bounds
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    font_size: 12.0,
                    color: TEXT,
                });
            }
            scene.controls.push(RecordingControlInfo {
                control: RecordingControl::Asset(asset.id),
                bounds: if visible { bounds } else { HIDDEN_BOUNDS },
                role: AccessibleRole::ListItem,
                label: audio_display_name(&asset.file_name),
                value: Some(asset_accessible_value(asset, used)),
                selected,
                enabled: true,
                visible,
                place: RecordingControlPlace {
                    library_offset: Some(row_offset),
                    ..RecordingControlPlace::default()
                },
            });
        }
    }
    push_asset_scrollbar(scene, ui, rect);
}

fn push_asset_chevron(scene: &mut RecordingScene, bounds: Rect, expanded: bool) {
    let color = [0.62, 0.63, 0.70, 1.0];
    let (first_x, first_y, second_x, second_y) = if expanded {
        (
            bounds.x + 4.0,
            bounds.y + 8.0,
            bounds.x + 9.0,
            bounds.y + 8.0,
        )
    } else {
        (
            bounds.x + 5.0,
            bounds.y + 6.0,
            bounds.x + 5.0,
            bounds.y + 11.0,
        )
    };
    for (x, y, rotation) in [
        (first_x, first_y, std::f32::consts::FRAC_PI_4),
        (second_x, second_y, -std::f32::consts::FRAC_PI_4),
    ] {
        scene.quads.push(QuadInstance {
            rect: [x, y, 7.0, 1.5],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.75,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation,
            _padding: [0.0; 2],
        });
    }
}

fn grouped_assets(
    project: &RecordingProject,
) -> BTreeMap<String, Vec<&crate::recording::AudioAsset>> {
    let mut groups = BTreeMap::new();
    for asset in project.assets() {
        groups
            .entry(asset_owner(&asset.file_name))
            .or_insert_with(Vec::new)
            .push(asset);
    }
    groups
}

fn asset_owner(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".flac").unwrap_or(file_name);
    for (index, _) in stem.match_indices('_') {
        let tail = &stem[index + 1..];
        let bytes = tail.as_bytes();
        if bytes.len() >= 19
            && bytes[0..4].iter().all(u8::is_ascii_digit)
            && bytes.get(4) == Some(&b'-')
            && bytes.get(7) == Some(&b'-')
            && bytes.get(10) == Some(&b'_')
            && bytes.get(13) == Some(&b'-')
            && bytes.get(16) == Some(&b'-')
        {
            return stem[..index].to_owned();
        }
    }
    crate::i18n::t("recording.assets.other").into()
}

fn asset_scrollbar_geometry(
    rect: Rect,
    content_height: f32,
    scroll: f32,
) -> Option<(Rect, Rect, f32)> {
    let viewport = (rect.height - 40.0).max(1.0);
    let max_scroll = (content_height - viewport).max(0.0);
    if max_scroll == 0.0 {
        return None;
    }
    let track = Rect {
        x: rect.x + rect.width - 9.0,
        y: rect.y + 42.0,
        width: 4.0,
        height: (rect.height - 46.0).max(1.0),
    };
    let thumb_h =
        (track.height * viewport / content_height).clamp(track.height.min(24.0), track.height);
    let thumb = Rect {
        x: track.x,
        y: track.y + scroll.min(max_scroll) / max_scroll * (track.height - thumb_h),
        width: track.width,
        height: thumb_h,
    };
    Some((track, thumb, max_scroll))
}

fn push_asset_scrollbar(scene: &mut RecordingScene, ui: &RecordingWorkspaceUi, rect: Rect) {
    let Some((track, thumb, _)) =
        asset_scrollbar_geometry(rect, ui.asset_content_height, ui.asset_scroll)
    else {
        return;
    };
    push_quad(
        &mut scene.quads,
        track,
        [0.12, 0.13, 0.17, 0.9],
        [0.0; 4],
        2.0,
    );
    push_quad(
        &mut scene.quads,
        thumb,
        [0.48, 0.50, 0.62, 0.95],
        [0.0; 4],
        2.0,
    );
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
        text: t("recording.participants.title").into(),
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y + 4.0,
            width: rect.width - 20.0,
            height: 24.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size: 13.0,
        color: TEXT,
    });
    for (index, member) in members.iter().enumerate() {
        let bounds = Rect {
            x: rect.x + 8.0,
            y: rect.y + 30.0 + index as f32 * 28.0,
            width: rect.width - 16.0,
            height: 24.0,
        };
        // Members that do not fit stay listed for the screen reader.
        let visible = bounds.y + bounds.height <= rect.y + rect.height;
        let controls = control_owner_id == Some(member.id.as_str());
        let mut details = vec![t(match member.role.as_str() {
            "admin" => "recording.role.director",
            "co_da" => "recording.role.co_director",
            _ => "recording.role.actor",
        })
        .to_string()];
        if member.muted {
            details.push(t("recording.participants.muted").to_string());
        } else if member.role == "actor" {
            details.push(
                t(if member.recording_ready {
                    "recording.microphone.ready_short"
                } else {
                    "recording.microphone.not_ready_short"
                })
                .to_string(),
            );
        }
        if controls {
            details.push(t("recording.a11y.has_control").to_string());
        }
        if visible {
            scene.labels.push(RecordingLabel {
                text: format!("{} · {}", member.username, details.join(" · ")),
                bounds,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                font_size: 12.0,
                color: if controls { [180, 210, 255] } else { TEXT },
            });
        }
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::Participant(member.id.clone()),
            bounds: if visible { bounds } else { HIDDEN_BOUNDS },
            role: AccessibleRole::ListItem,
            label: member.username.clone(),
            value: Some(details.join(", ")),
            selected: false,
            enabled: true,
            visible,
            place: RecordingControlPlace::default(),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_clip_waveform(
    quads: &mut Vec<QuadInstance>,
    visible: Rect,
    full_clip: Rect,
    waveform: &WaveformData,
    sample_rate: u32,
    fps: f64,
    source_start_frame: i64,
    pixels_per_frame: f32,
) {
    if waveform.peaks.is_empty() || visible.width <= 1.0 {
        return;
    }
    let columns = (visible.width.ceil() as usize).clamp(1, 1024);
    let column_width = visible.width / columns as f32;
    let center_y = visible.y + visible.height * 0.5;
    push_quad(
        quads,
        Rect {
            x: visible.x,
            y: center_y - 0.5,
            width: visible.width,
            height: 1.0,
        },
        [0.50, 0.60, 0.86, 0.45],
        [0.0; 4],
        0.0,
    );
    for index in 0..columns {
        let x = visible.x + index as f32 * column_width;
        let amplitude = waveform_visual_amplitude(waveform_amplitude_for_x_range(
            waveform,
            sample_rate,
            fps,
            source_start_frame,
            pixels_per_frame,
            full_clip.x,
            x,
            (x + column_width).min(visible.x + visible.width),
        ));
        let height = (amplitude * (visible.height - 8.0).max(1.0)).max(1.0);
        push_quad(
            quads,
            Rect {
                x,
                y: center_y - height * 0.5,
                width: column_width + 0.25,
                height,
            },
            [0.72, 0.82, 1.0, 0.82],
            [0.0; 4],
            0.0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn waveform_amplitude_for_x_range(
    waveform: &WaveformData,
    sample_rate: u32,
    fps: f64,
    source_start_frame: i64,
    pixels_per_frame: f32,
    full_clip_x: f32,
    x_start: f32,
    x_end: f32,
) -> f32 {
    if waveform.peaks.is_empty()
        || sample_rate == 0
        || !fps.is_finite()
        || fps <= 0.0
        || pixels_per_frame <= 0.0
    {
        return 0.0;
    }
    let peak_position = |x: f32| {
        let source_frame =
            source_start_frame as f64 + f64::from((x - full_clip_x) / pixels_per_frame);
        let sample = source_frame.max(0.0) / fps * f64::from(sample_rate);
        sample / f64::from(waveform.samples_per_peak.max(1))
    };
    let start = peak_position(x_start);
    let end = peak_position(x_end).max(start);
    if end - start < 1.0 {
        let position = (start + end) * 0.5;
        let lower = position.floor() as usize;
        let fraction = (position - lower as f64) as f32;
        let left = waveform.peaks.get(lower).copied().unwrap_or(0.0);
        let right = waveform
            .peaks
            .get(lower.saturating_add(1))
            .copied()
            .unwrap_or(left);
        return left + (right - left) * fraction;
    }
    let first = start.floor().max(0.0) as usize;
    let last = end.ceil().max(0.0) as usize;
    waveform.peaks[first.min(waveform.peaks.len())..last.min(waveform.peaks.len())]
        .iter()
        .copied()
        .fold(0.0, f32::max)
}

fn waveform_visual_amplitude(peak: f32) -> f32 {
    peak.clamp(0.0, 1.0).sqrt()
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    push_quad_with_border(quads, rect, color, border, 1.0, radius);
}

fn push_quad_with_border(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    border_width: f32,
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { border_width } else { 0.0 },
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
    use crate::recording::{AudioAsset, AudioClip, AudioTrack, RecordingOperation};

    #[test]
    fn only_solo_and_director_can_change_the_shared_view() {
        assert!(RecordingRole::Solo.can_change_shared_view());
        assert!(RecordingRole::Director.can_change_shared_view());
        assert!(!RecordingRole::CoDirector { has_control: true }.can_change_shared_view());
        assert!(!RecordingRole::Actor.can_change_shared_view());
    }

    #[test]
    fn countdown_grows_towards_recording() {
        assert!(countdown_font_size(3) < countdown_font_size(2));
        assert!(countdown_font_size(2) < countdown_font_size(1));
    }

    #[test]
    fn audio_import_waits_for_a_username_and_keeps_its_drop_target() {
        let track_id = AudioTrackId::new(7);
        let mut ui = RecordingWorkspaceUi::default();
        ui.begin_audio_import(
            PathBuf::from("voice.wav"),
            Some((track_id, 42)),
            String::new(),
        );

        let layout = RecordingLayout::timeline(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 720.0,
            },
            false,
        );
        let scene = ui.scene(
            layout,
            &RecordingProject::new(24.0).unwrap(),
            None,
            &[],
            None,
            0.0,
            None,
        );
        let prompt = ui.audio_import_prompt_scene(layout.content).unwrap();
        let title = crate::i18n::t("recording.audio.username_prompt");
        assert!(!scene.labels.iter().any(|label| label.text == title));
        assert!(prompt.labels.iter().any(|label| label.text == title));

        assert_eq!(
            ui.handle_text_edit(&UiEvent::KeyInput { text: "Bob".into() }),
            Some(RecordingTextEditResult::Consumed)
        );
        assert_eq!(
            ui.handle_text_edit(&UiEvent::KeyInput { text: "\r".into() }),
            Some(RecordingTextEditResult::ImportAudio {
                path: PathBuf::from("voice.wav"),
                username: "Bob".into(),
                placement: Some((track_id, 42)),
            })
        );
        assert!(!ui.is_editing_text());
    }

    #[test]
    fn audio_library_groups_by_username_and_marks_used_audio_green() {
        assert_eq!(
            asset_owner("alice_voice_2026-08-12_14-30-00.flac"),
            "alice_voice"
        );
        assert_eq!(
            asset_owner("legacy.flac"),
            crate::i18n::t("recording.assets.other")
        );

        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = project.allocate_track_id();
        let asset_id = project.allocate_asset_id();
        let clip_id = project.allocate_clip_id();
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(track_id, "Voix"),
                    },
                    RecordingOperation::AddAsset {
                        asset: AudioAsset {
                            id: asset_id,
                            file_name: "alice_2026-08-12_14-30-00.flac".into(),
                            sample_rate: 48_000,
                            channels: 1,
                            sample_count: 48_000,
                            checksum: "a".repeat(40),
                            waveform: WaveformData::new(480, vec![0.5]).unwrap(),
                        },
                    },
                    RecordingOperation::AddClip {
                        clip: AudioClip {
                            id: clip_id,
                            asset_id,
                            track_id,
                            start_frame: 0,
                            source_start_frame: 0,
                            duration_frames: 24,
                        },
                    },
                ],
            })
            .unwrap();
        let layout = RecordingLayout::daw(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 300.0,
            },
            0.3,
        );
        let mut ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            ..Default::default()
        };
        ui.sync_asset_content(&project);
        let collapsed = ui.scene(layout, &project, None, &[], None, 0.0, None);
        assert!(collapsed
            .controls
            .iter()
            .any(|control| { control.control == RecordingControl::AssetGroup("alice".into()) }));
        assert!(!collapsed
            .controls
            .iter()
            .any(|control| control.control == RecordingControl::Asset(asset_id)));

        ui.toggle_asset_group("alice");
        ui.sync_asset_content(&project);
        ui.asset_content_height = 1_000.0;
        let assets = layout.assets.unwrap();
        assert!(ui.handle_asset_scroll(
            &UiEvent::Scroll {
                x: assets.x + 10.0,
                y: assets.y + 50.0,
                delta: -1.0,
                fast: false,
                ctrl: false,
            },
            layout,
        ));
        assert!(ui.asset_scroll > 0.0);
        let scrolled = ui.scene(layout, &project, None, &[], None, 0.0, None);
        assert!(scrolled.labels.iter().all(|label| {
            label.bounds.x < assets.x
                || label.text == crate::i18n::t("recording.assets.title")
                || label.bounds.y >= assets.y + 40.0
        }));
        ui.asset_scroll = 0.0;
        ui.sync_asset_content(&project);
        let scene = ui.scene(layout, &project, None, &[], None, 0.0, None);
        let control = scene
            .controls
            .iter()
            .find(|control| control.control == RecordingControl::Asset(asset_id))
            .unwrap();
        assert!(scene.labels.iter().any(|label| label.text == "alice"));
        assert!(scene.quads.iter().any(|quad| {
            quad.rect
                == [
                    control.bounds.x,
                    control.bounds.y,
                    control.bounds.width,
                    control.bounds.height,
                ]
                && quad.color == USED_AUDIO
        }));
    }

    #[test]
    fn capture_layout_keeps_the_normal_rythmo_inside_the_window() {
        let layout = RecordingLayout::capturing(1280.0, 720.0, 380.0, None);
        assert_eq!(layout.rythmo.x, 0.0);
        assert_eq!(layout.rythmo.width, 1280.0);
        assert_eq!(layout.rythmo.height, 380.0);
        assert_eq!(layout.rythmo.y + layout.rythmo.height, 720.0);
        assert_eq!(layout.video.y + layout.video.height, layout.rythmo.y);
        assert_eq!(
            layout.rythmo_split_handle_rect().y + super::super::layout::SPLIT_DRAG_ZONE / 2.0,
            layout.rythmo.y
        );
        assert!(layout.timeline.is_none());
    }

    #[test]
    fn capture_layout_allows_the_actor_to_compress_the_rythmo() {
        let layout = RecordingLayout::capturing(1280.0, 720.0, 380.0, Some(0.2));
        assert_eq!(layout.rythmo.height, 720.0 * 0.2);
    }

    #[test]
    fn timeline_keeps_the_daw_compact_and_gives_space_to_rythmo() {
        let layout = RecordingLayout::timeline(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1400.0,
                height: 1000.0,
            },
            false,
        );
        assert!(layout.video.height > layout.rythmo.height);
        assert!(layout.timeline.unwrap().height <= 164.0);
    }

    #[test]
    fn detached_main_has_no_gap_between_video_and_rythmo() {
        let content = Rect {
            x: 0.0,
            y: 68.0,
            width: 1280.0,
            height: 652.0,
        };
        let layout = RecordingLayout::detached_main(content, 0.48);

        assert_eq!(layout.video.y + layout.video.height, layout.rythmo.y);
        assert_eq!(
            layout.rythmo.y + layout.rythmo.height,
            content.y + content.height
        );
        assert!(layout.toolbar.is_none());
        assert!(layout.timeline.is_none());
        assert!(layout.assets.is_none());
    }

    #[test]
    fn detached_daw_fills_the_window_with_timeline_and_assets() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 720.0,
        };
        let layout = RecordingLayout::daw(content, 0.30);
        let toolbar = layout.toolbar.unwrap();
        let timeline = layout.timeline.unwrap();
        let assets = layout.assets.unwrap();

        assert_eq!(toolbar.y, 0.0);
        assert_eq!(toolbar.height, super::super::layout::TOOLBAR_H);
        assert_eq!(timeline.y, toolbar.y + toolbar.height);
        assert_eq!(timeline.height, 720.0 - toolbar.height);
        assert_eq!(assets.y, timeline.y);
        assert_eq!(assets.height, timeline.height);
        assert_eq!(timeline.x + timeline.width, assets.x);
        assert_eq!(assets.x + assets.width, 1280.0);
        assert_eq!(layout.video, Rect::default());
        assert_eq!(layout.rythmo, Rect::default());
    }

    #[test]
    fn quiet_recording_peaks_are_visually_amplified_without_lifting_silence() {
        assert_eq!(waveform_visual_amplitude(0.0), 0.0);
        assert!(waveform_visual_amplitude(0.04) > 0.04);
        assert_eq!(waveform_visual_amplitude(1.0), 1.0);
    }

    #[test]
    fn clipped_waveform_keeps_its_source_position_instead_of_restretching() {
        let waveform = WaveformData::new(100, vec![0.1, 0.9, 0.2]).unwrap();
        let first = waveform_amplitude_for_x_range(&waveform, 100, 1.0, 0, 100.0, 0.0, 0.0, 1.0);
        let scrolled =
            waveform_amplitude_for_x_range(&waveform, 100, 1.0, 0, 100.0, -100.0, 0.0, 1.0);
        let zoomed =
            waveform_amplitude_for_x_range(&waveform, 100, 1.0, 0, 200.0, 0.0, 200.0, 201.0);

        assert!((first - 0.1).abs() < 0.01);
        assert!((scrolled - 0.9).abs() < 0.01);
        assert!((zoomed - 0.9).abs() < 0.01);
    }

    #[test]
    fn daw_playhead_uses_the_same_centered_view_as_the_rythmo() {
        crate::config::init();
        let layout = RecordingLayout::timeline(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1400.0,
                height: 1000.0,
            },
            false,
        );
        let mut ui = RecordingWorkspaceUi::default();
        ui.sync_view_to_playhead(layout, 240.0, 24.0, 1.0, 0.0);
        let body = layout.track_body.unwrap();
        let playhead_x = body.x + (240.0 - ui.view_start_frame) as f32 * ui.pixels_per_frame;
        let expected_x = body.x + body.width * 0.5
            - crate::config::reading_bar_offset_seconds() as f32 * 24.0 * ui.pixels_per_frame;
        assert!((playhead_x - expected_x).abs() <= ui.pixels_per_frame);
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
        let scene = ui.scene(layout, &project, None, &[], None, 0.0, None);
        let track_controls = scene.controls.iter().filter(|control| {
            matches!(
                control.control,
                RecordingControl::TrackMute(_)
                    | RecordingControl::TrackSolo(_)
                    | RecordingControl::TrackArm(_)
                    | RecordingControl::TrackVolume(_)
            )
        });
        assert!(track_controls.count() >= 4);
        assert!(scene
            .controls
            .iter()
            .filter(|control| matches!(
                control.control,
                RecordingControl::TrackMute(_)
                    | RecordingControl::TrackSolo(_)
                    | RecordingControl::TrackArm(_)
                    | RecordingControl::TrackVolume(_)
            ))
            .all(|control| !control.enabled));
        let record = scene
            .controls
            .iter()
            .find(|control| matches!(control.control, RecordingControl::StartCapture))
            .unwrap();
        let tools = layout.tools.unwrap();
        assert!(record.bounds.x >= tools.x);
        assert!(record.bounds.y >= tools.y);
        assert!(record.bounds.x + record.bounds.width <= tools.x + tools.width);
        assert!(record.bounds.y + record.bounds.height <= tools.y + tools.height);
    }

    #[test]
    fn extra_tracks_expose_rename_and_remove_controls() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let first = project.allocate_track_id();
        let second = project.allocate_track_id();
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(first, "Piste 1"),
                    },
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(second, "Piste 2"),
                    },
                ],
            })
            .unwrap();
        let ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            role: RecordingRole::Solo,
            ..RecordingWorkspaceUi::default()
        };
        let scene = ui.scene(
            RecordingLayout::timeline(
                Rect {
                    x: 0.0,
                    y: 68.0,
                    width: 1200.0,
                    height: 732.0,
                },
                false,
            ),
            &project,
            None,
            &[],
            None,
            0.0,
            None,
        );
        assert!(scene.controls.iter().any(|control| matches!(
            control.control,
            RecordingControl::RenameTrack(id) if id == second
        )));
        assert!(scene.controls.iter().any(|control| matches!(
            control.control,
            RecordingControl::RemoveTrack(id) if id == second
        )));
    }

    #[test]
    fn daw_scrolls_to_tracks_below_the_viewport() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_ids: Vec<_> = (0..4)
            .map(|index| {
                let id = project.allocate_track_id();
                project
                    .apply(&RecordingOperation::AddTrack {
                        track: AudioTrack::new(id, format!("Piste {}", index + 1)),
                    })
                    .unwrap();
                id
            })
            .collect();
        let layout = RecordingLayout::daw(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 200.0,
            },
            0.23,
        );
        let mut ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            role: RecordingRole::Solo,
            ..RecordingWorkspaceUi::default()
        };
        ui.sync_track_count(track_ids.len());
        let body = layout.track_body.unwrap();
        assert!(ui.handle_track_scroll(
            &UiEvent::Scroll {
                x: body.x + 10.0,
                y: body.y + 10.0,
                delta: -1.0,
                fast: false,
                ctrl: false,
            },
            layout,
        ));
        let scene = ui.scene(layout, &project, None, &[], None, 0.0, None);
        let first_visible = scene
            .controls
            .iter()
            .filter(|control| control.visible)
            .find_map(|control| match control.control {
                RecordingControl::TrackMute(id) => Some(id),
                _ => None,
            });
        assert_eq!(first_visible, Some(track_ids[1]));
        // The track scrolled out of view stays reachable from the keyboard,
        // without any pointer target.
        let hidden = scene
            .controls
            .iter()
            .find(|control| control.control == RecordingControl::TrackMute(track_ids[0]))
            .unwrap();
        assert!(!hidden.visible);
        assert!(!hidden.bounds.contains(0.0, 0.0));
        assert_eq!(hidden.place.track_row, Some(0));
    }

    #[test]
    fn focusing_a_scrolled_out_track_scrolls_it_back_into_view() {
        let mut project = RecordingProject::new(24.0).unwrap();
        for index in 0..5 {
            let id = project.allocate_track_id();
            project
                .apply(&RecordingOperation::AddTrack {
                    track: AudioTrack::new(id, format!("Piste {}", index + 1)),
                })
                .unwrap();
        }
        let layout = RecordingLayout::daw(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 200.0,
            },
            0.23,
        );
        let mut ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            ..RecordingWorkspaceUi::default()
        };
        ui.sync_track_count(5);
        let scene = ui.scene(layout, &project, None, &[], None, 0.0, None);
        let last = scene
            .controls
            .iter()
            .filter(|control| matches!(control.control, RecordingControl::RenameTrack(_)))
            .last()
            .unwrap()
            .clone();
        assert!(!last.visible);
        ui.reveal(last.place, layout);
        let scene = ui.scene(layout, &project, None, &[], None, 0.0, None);
        assert!(
            scene
                .controls
                .iter()
                .find(|control| control.control == last.control)
                .unwrap()
                .visible
        );
    }

    fn take_project() -> (RecordingProject, Vec<AudioTrackId>, Vec<AudioClipId>) {
        let mut project = RecordingProject::new(25.0).unwrap();
        let tracks = (0..2)
            .map(|_| project.allocate_track_id())
            .collect::<Vec<_>>();
        let asset_id = project.allocate_asset_id();
        let clips = (0..3)
            .map(|_| project.allocate_clip_id())
            .collect::<Vec<_>>();
        let mut operations = tracks
            .iter()
            .enumerate()
            .map(|(index, id)| RecordingOperation::AddTrack {
                track: AudioTrack::new(*id, format!("Piste {}", index + 1)),
            })
            .collect::<Vec<_>>();
        operations.push(RecordingOperation::AddAsset {
            asset: AudioAsset {
                id: asset_id,
                file_name: "alice_2026-08-12_14-30-00.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000 * 4,
                checksum: "a".repeat(40),
                waveform: WaveformData::new(480, vec![0.5]).unwrap(),
            },
        });
        // Piste 1: takes at 10 s and 0 s (inserted out of order);
        // Piste 2: a take at 9 s.
        for (clip_id, track_id, start_frame) in [
            (clips[0], tracks[0], 250),
            (clips[1], tracks[0], 0),
            (clips[2], tracks[1], 225),
        ] {
            operations.push(RecordingOperation::AddClip {
                clip: AudioClip {
                    id: clip_id,
                    asset_id,
                    track_id,
                    start_frame,
                    source_start_frame: 0,
                    duration_frames: 50,
                },
            });
        }
        project
            .apply(&RecordingOperation::Batch { operations })
            .unwrap();
        (project, tracks, clips)
    }

    fn take_scene(project: &RecordingProject, role: RecordingRole) -> RecordingScene {
        let ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            role,
            ..RecordingWorkspaceUi::default()
        };
        ui.scene(
            RecordingLayout::timeline(
                Rect {
                    x: 0.0,
                    y: 68.0,
                    width: 1200.0,
                    height: 732.0,
                },
                false,
            ),
            project,
            None,
            &[],
            None,
            0.0,
            None,
        )
    }

    #[test]
    fn takes_are_spoken_with_their_speaker_track_start_and_length() {
        crate::i18n::init("fr-fr");
        let (project, _, clips) = take_project();
        let scene = take_scene(&project, RecordingRole::Solo);
        let take = scene
            .controls
            .iter()
            .find(|control| control.control == RecordingControl::Clip(clips[0]))
            .unwrap();
        assert_eq!(take.label, "Prise de alice");
        assert_eq!(
            take.value.as_deref(),
            Some("Piste 1, début 10 secondes, durée 2 secondes")
        );
        // Every take is listed, track by track and in time order, even the
        // ones out of the visible time window.
        let order = scene
            .controls
            .iter()
            .filter_map(|control| match control.control {
                RecordingControl::Clip(id) => Some(id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(order, vec![clips[1], clips[0], clips[2]]);
    }

    #[test]
    fn enter_selects_the_take_under_the_playhead_like_a_rythmo_line() {
        let (project, _, clips) = take_project();
        let mut ui = RecordingWorkspaceUi {
            page: RecordingPage::Timeline,
            ..RecordingWorkspaceUi::default()
        };
        let selected = |ui: &RecordingWorkspaceUi| ui.selected_clips().collect::<Vec<_>>();
        // At 10.4 s, Piste 1 and Piste 2 both have a take: top track first,
        // then Enter again moves on to the next track, and wraps.
        assert_eq!(ui.select_clip_at_playhead(&project, 260), Some(clips[0]));
        assert_eq!(selected(&ui), vec![clips[0]]);
        assert_eq!(ui.select_clip_at_playhead(&project, 260), Some(clips[2]));
        assert_eq!(selected(&ui), vec![clips[2]]);
        assert_eq!(ui.select_clip_at_playhead(&project, 260), Some(clips[0]));
        // Nothing under the playhead clears the selection.
        assert_eq!(ui.select_clip_at_playhead(&project, 100), None);
        assert!(selected(&ui).is_empty());
        // Elsewhere, the first press starts again from the top track.
        assert_eq!(ui.select_clip_at_playhead(&project, 10), Some(clips[1]));
    }

    #[test]
    fn spoken_durations_skip_empty_units() {
        crate::i18n::init("fr-fr");
        assert_eq!(spoken_duration(0.0), "0 secondes");
        assert_eq!(spoken_duration(0.5), "50 centièmes");
        assert_eq!(spoken_duration(60.0), "1 minute");
        assert_eq!(spoken_duration(72.4), "1 minute 12 secondes 40 centièmes");
        assert_eq!(spoken_duration(3_601.0), "1 heure 1 seconde");
    }

    #[test]
    fn arrows_move_between_tracks_takes_and_the_library() {
        let (project, tracks, clips) = take_project();
        let scene = take_scene(&project, RecordingRole::Solo);
        let controls = &scene.controls;
        let moved = |from: RecordingControl, movement| {
            neighbor_control(controls, &from, movement).map(|control| control.control.clone())
        };
        // Same button on the next track, other buttons of the same track.
        assert_eq!(
            moved(
                RecordingControl::TrackMute(tracks[0]),
                RecordingFocusMove::Next
            ),
            Some(RecordingControl::TrackMute(tracks[1]))
        );
        assert_eq!(
            moved(
                RecordingControl::TrackMute(tracks[1]),
                RecordingFocusMove::Next
            ),
            None
        );
        assert_eq!(
            moved(
                RecordingControl::TrackMute(tracks[0]),
                RecordingFocusMove::NextInRow
            ),
            Some(RecordingControl::TrackSolo(tracks[0]))
        );
        assert_eq!(
            moved(
                RecordingControl::TrackMute(tracks[0]),
                RecordingFocusMove::Last
            ),
            Some(RecordingControl::TrackMute(tracks[1]))
        );
        // Takes: next one on the same track, nearest one on the next track.
        assert_eq!(
            moved(
                RecordingControl::Clip(clips[1]),
                RecordingFocusMove::NextInRow
            ),
            Some(RecordingControl::Clip(clips[0]))
        );
        assert_eq!(
            moved(RecordingControl::Clip(clips[0]), RecordingFocusMove::Next),
            Some(RecordingControl::Clip(clips[2]))
        );
        assert_eq!(
            moved(
                RecordingControl::Clip(clips[2]),
                RecordingFocusMove::Previous
            ),
            Some(RecordingControl::Clip(clips[0]))
        );
    }

    #[test]
    fn actors_reach_every_track_and_hear_its_state() {
        crate::i18n::init("fr-fr");
        let (project, tracks, _) = take_project();
        let scene = take_scene(&project, RecordingRole::Actor);
        let name = scene
            .controls
            .iter()
            .find(|control| control.control == RecordingControl::RenameTrack(tracks[0]))
            .unwrap();
        assert!(name.enabled);
        assert_eq!(name.role, AccessibleRole::ListItem);
        assert_eq!(name.label, "Piste 1");
        assert_eq!(name.value.as_deref(), Some("volume 100 %, 2 prises"));
        // Up / Down still move from track to track on the only enabled
        // control of each row.
        assert_eq!(
            neighbor_control(
                &scene.controls,
                &RecordingControl::RenameTrack(tracks[0]),
                RecordingFocusMove::Next
            )
            .map(|control| control.control.clone()),
            Some(RecordingControl::RenameTrack(tracks[1]))
        );
    }

    #[test]
    fn record_button_stays_reachable_and_says_which_track_is_armed() {
        crate::i18n::init("fr-fr");
        let (mut project, tracks, _) = take_project();
        let record = |project: &RecordingProject| {
            take_scene(project, RecordingRole::Solo)
                .controls
                .into_iter()
                .find(|control| control.control == RecordingControl::StartCapture)
                .unwrap()
        };
        let unarmed = record(&project);
        assert!(unarmed.enabled);
        assert_eq!(unarmed.value.as_deref(), Some("aucune piste armée"));
        project
            .apply(&RecordingOperation::ArmTrack {
                track_id: Some(tracks[1]),
            })
            .unwrap();
        assert_eq!(record(&project).value.as_deref(), Some("sur Piste 2"));
    }

    #[test]
    fn toggles_speak_their_state_and_the_volume_is_a_slider() {
        crate::i18n::init("fr-fr");
        let (mut project, tracks, _) = take_project();
        project
            .apply(&RecordingOperation::SetTrackMuted {
                track_id: tracks[0],
                muted: true,
            })
            .unwrap();
        let scene = take_scene(&project, RecordingRole::Solo);
        let find = |control: RecordingControl| {
            scene
                .controls
                .iter()
                .find(|candidate| candidate.control == control)
                .unwrap()
        };
        let mute = find(RecordingControl::TrackMute(tracks[0]));
        assert_eq!(mute.label, "Piste 1 — Muet");
        assert_eq!(mute.value.as_deref(), Some("activé"));
        assert!(!mute.selected);
        let volume = find(RecordingControl::TrackVolume(tracks[0]));
        assert_eq!(volume.role, AccessibleRole::Slider);
        assert_eq!(volume.label, "Piste 1 — Volume de la piste");
        assert_eq!(volume.value.as_deref(), Some("100 %"));
        assert!(find(RecordingControl::RenameTrack(tracks[0]))
            .value
            .as_deref()
            .unwrap()
            .starts_with("son coupé"));
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

    #[test]
    fn choice_cards_fit_without_overlapping_on_narrow_windows() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 480.0,
            height: 500.0,
        };
        let scene = choice_scene(RecordingLayout::choice(content));
        let solo = scene.controls[0].bounds;
        let online = scene.controls[1].bounds;
        assert!(solo.x + solo.width <= online.x);
        assert!(online.x + online.width <= content.x + content.width);
    }

    #[test]
    fn recording_asset_context_menu_sends_the_clicked_audio_to_voicelines() {
        let asset_id = AudioAssetId::new(7);
        let scene = RecordingScene {
            controls: vec![RecordingControlInfo {
                control: RecordingControl::Asset(asset_id),
                bounds: Rect {
                    x: 40.0,
                    y: 40.0,
                    width: 160.0,
                    height: 36.0,
                },
                role: AccessibleRole::ListItem,
                label: "audio.flac".into(),
                value: None,
                selected: false,
                enabled: true,
                visible: true,
                place: RecordingControlPlace::default(),
            }],
            ..RecordingScene::default()
        };
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let mut ui = RecordingWorkspaceUi::default();

        // The menu opens at the pointer and announces its first entry.
        assert!(matches!(
            ui.handle_asset_context_menu(
                &UiEvent::ContextMenu { x: 60.0, y: 60.0 },
                &scene,
                screen,
            ),
            Some(EventResponse::Action(UiAction::Accessibility(
                AccessibilityEvent::Focus { .. }
            )))
        ));
        // "Envoyer vers" (second row) opens the submenu…
        assert_eq!(
            ui.handle_asset_context_menu(
                &UiEvent::MousePress {
                    x: 70.0,
                    y: 60.0 + ASSET_MENU_ITEM_H * 1.5,
                },
                &scene,
                screen,
            ),
            Some(EventResponse::Consumed)
        );
        // …whose only entry sends the audio to Voicelines.
        assert_eq!(
            ui.handle_asset_context_menu(
                &UiEvent::MousePress {
                    x: 60.0 + ASSET_MENU_W + 20.0,
                    y: 60.0 + ASSET_MENU_ITEM_H * 1.5,
                },
                &scene,
                screen,
            ),
            Some(EventResponse::Action(
                UiAction::RecordingSendAssetToVoicelines(asset_id)
            ))
        );
    }

    #[test]
    fn recording_asset_menu_works_from_the_keyboard() {
        crate::i18n::init("fr-fr");
        let asset_id = AudioAssetId::new(7);
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let scene = RecordingScene {
            controls: vec![RecordingControlInfo {
                control: RecordingControl::Asset(asset_id),
                bounds: Rect {
                    x: 40.0,
                    y: 40.0,
                    width: 160.0,
                    height: 36.0,
                },
                role: AccessibleRole::ListItem,
                label: "audio".into(),
                value: None,
                selected: true,
                enabled: true,
                visible: true,
                place: RecordingControlPlace::default(),
            }],
            ..RecordingScene::default()
        };
        let focus_label = |response: Option<EventResponse>| match response {
            Some(EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Focus {
                label,
                ..
            }))) => label,
            other => panic!("expected a focus announcement, got {other:?}"),
        };
        let mut ui = RecordingWorkspaceUi {
            selected_asset: Some(asset_id),
            ..RecordingWorkspaceUi::default()
        };
        assert_eq!(
            focus_label(ui.handle_asset_context_menu(&UiEvent::OpenContextMenu, &scene, screen)),
            "Menu de l'audio : Placer au curseur de lecture"
        );
        // Enter on the first entry places the audio at the playhead.
        assert_eq!(
            ui.handle_asset_context_menu(&UiEvent::Activate, &scene, screen),
            Some(EventResponse::Action(
                UiAction::RecordingPlaceAssetAtPlayhead(Some(asset_id))
            ))
        );
        assert!(!ui.has_asset_context_menu());

        ui.handle_asset_context_menu(&UiEvent::OpenContextMenu, &scene, screen);
        assert_eq!(
            focus_label(ui.handle_asset_context_menu(&UiEvent::CursorDown, &scene, screen)),
            "Envoyer vers"
        );
        assert_eq!(
            focus_label(ui.handle_asset_context_menu(&UiEvent::CursorRight, &scene, screen)),
            "Voicelines"
        );
        assert_eq!(
            ui.handle_asset_context_menu(&UiEvent::Activate, &scene, screen),
            Some(EventResponse::Action(
                UiAction::RecordingSendAssetToVoicelines(asset_id)
            ))
        );

        ui.handle_asset_context_menu(&UiEvent::OpenContextMenu, &scene, screen);
        assert_eq!(
            focus_label(ui.handle_asset_context_menu(&UiEvent::CursorUp, &scene, screen)),
            "Supprimer l'audio"
        );
        assert!(matches!(
            ui.handle_asset_context_menu(
                &UiEvent::KeyInput {
                    text: "\x1b".into()
                },
                &scene,
                screen
            ),
            Some(EventResponse::Action(UiAction::Accessibility(
                AccessibilityEvent::Closed { .. }
            )))
        ));
        assert!(!ui.has_asset_context_menu());
    }

    #[test]
    fn recording_asset_context_menu_uses_the_system_layer() {
        let mut scene = RecordingScene::default();
        push_asset_context_menu(
            &mut scene,
            AssetContextMenu {
                asset_id: AudioAssetId::new(7),
                x: 20.0,
                y: 30.0,
                highlighted: 1,
                submenu_open: true,
                hover_voicelines: false,
            },
        );

        assert!(scene.quads.is_empty());
        assert!(scene.labels.is_empty());
        assert!(!scene.system_quads.is_empty());
        assert!(scene
            .system_labels
            .iter()
            .any(|label| label.text == "Voicelines"));
    }
}
