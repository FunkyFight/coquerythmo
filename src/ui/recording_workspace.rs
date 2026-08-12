//! Backend-neutral scene and interaction model for the recording workspace.
//!
//! The bande rythmo itself is deliberately absent from this scene: [`Ui`]
//! renders it through the exact editor helpers (`render_rythmo_base`,
//! `render_lines`, markers and drawing) inside [`RecordingLayout::rythmo`].
//! This module only describes the DAW chrome around that read-only view.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::network::NetworkMember;
use crate::recording::{
    AudioAssetId, AudioClipId, AudioTrackId, CaptureState, RecordingEditor, RecordingError,
    RecordingProject, RecordingTool, WaveformData,
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
pub const TRACK_ROW_H: f32 = 58.0;
const ASSET_ROW_H: f32 = 42.0;
const ASSET_GROUP_H: f32 = 24.0;
const ASSET_MENU_W: f32 = 154.0;
const ASSET_MENU_ITEM_H: f32 = 30.0;

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
            Self::StartCapture => "recording.capture.start".into(),
            Self::Clip(id) => format!("recording.clip.{}", id.get()),
            Self::Asset(id) => format!("recording.asset.{}", id.get()),
            Self::AssetGroup(owner) => format!("recording.assets.group.{owner}"),
            Self::ImportUsername => "recording.audio.import.username".into(),
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
        let video_h = (available_h - 164.0 - rythmo_min_h.max(100.0)).clamp(140.0, 560.0);
        let available_after_video = (content.height - video_h - toolbar_h).max(0.0);
        const DAW_MIN_H: f32 = 164.0;
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
}

#[derive(Debug, Clone, Copy)]
struct AssetContextMenu {
    asset_id: AudioAssetId,
    x: f32,
    y: f32,
    submenu_open: bool,
    hover_parent: bool,
    hover_voicelines: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingAudioImport {
    path: PathBuf,
    placement: Option<(AudioTrackId, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordingTextEditResult {
    Consumed,
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
                "\x1b" => self.cancel_audio_import(),
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
            "\x1b" => self.cancel_rename_track(),
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
            let (x, y) = super::context_menu::clamped_origin(
                *x,
                *y,
                ASSET_MENU_W * 2.0 - 2.0,
                ASSET_MENU_ITEM_H,
                screen.x + screen.width,
                screen.y + screen.height,
            );
            self.selected_asset = Some(asset_id);
            self.dragging_asset = None;
            self.asset_context_menu = Some(AssetContextMenu {
                asset_id,
                x,
                y,
                submenu_open: false,
                hover_parent: false,
                hover_voicelines: false,
            });
            return Some(EventResponse::Consumed);
        }

        if matches!(event, UiEvent::OpenContextMenu) && self.asset_context_menu.is_none() {
            let asset_id = self.selected_asset?;
            let bounds = scene
                .controls
                .iter()
                .find_map(|control| match control.control {
                    RecordingControl::Asset(id) if id == asset_id => Some(control.bounds),
                    _ => None,
                })?;
            let (x, y) = super::context_menu::clamped_origin(
                bounds.x + 16.0,
                bounds.y + bounds.height,
                ASSET_MENU_W * 2.0 - 2.0,
                ASSET_MENU_ITEM_H,
                screen.x + screen.width,
                screen.y + screen.height,
            );
            self.asset_context_menu = Some(AssetContextMenu {
                asset_id,
                x,
                y,
                submenu_open: true,
                hover_parent: true,
                hover_voicelines: false,
            });
            return Some(EventResponse::Consumed);
        }

        let menu = self.asset_context_menu.as_mut()?;
        let parent = Rect {
            x: menu.x,
            y: menu.y,
            width: ASSET_MENU_W,
            height: ASSET_MENU_ITEM_H,
        };
        let submenu = Rect {
            x: menu.x + ASSET_MENU_W - 2.0,
            ..parent
        };
        match event {
            UiEvent::MouseMove { x, y } => {
                menu.hover_parent = parent.contains(*x, *y);
                menu.hover_voicelines = menu.submenu_open && submenu.contains(*x, *y);
                if menu.hover_parent {
                    menu.submenu_open = true;
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } if menu.submenu_open && submenu.contains(*x, *y) => {
                let asset_id = menu.asset_id;
                self.asset_context_menu = None;
                Some(EventResponse::Action(
                    UiAction::RecordingSendAssetToVoicelines(asset_id),
                ))
            }
            UiEvent::MousePress { x, y } if parent.contains(*x, *y) => {
                menu.submenu_open = true;
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorRight => {
                menu.submenu_open = true;
                menu.hover_voicelines = true;
                Some(EventResponse::Consumed)
            }
            UiEvent::Activate if menu.submenu_open => {
                let asset_id = menu.asset_id;
                self.asset_context_menu = None;
                Some(EventResponse::Action(
                    UiAction::RecordingSendAssetToVoicelines(asset_id),
                ))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.asset_context_menu = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { .. } | UiEvent::OpenContextMenu => {
                self.asset_context_menu = None;
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        }
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
            project.armed_track_id().is_some(),
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
    if let Some(prompt) = ui.audio_import_prompt_scene(layout.content) {
        scene.controls.extend(prompt.controls);
    }
    if let Some(menu) = ui.asset_context_menu {
        push_asset_context_menu(&mut scene, menu);
    }
    scene
}

fn push_asset_context_menu(scene: &mut RecordingScene, menu: AssetContextMenu) {
    let parent = Rect {
        x: menu.x,
        y: menu.y,
        width: ASSET_MENU_W,
        height: ASSET_MENU_ITEM_H,
    };
    push_quad(
        &mut scene.quads,
        parent,
        [0.13, 0.13, 0.16, 0.99],
        BORDER,
        0.0,
    );
    if menu.hover_parent {
        push_quad(
            &mut scene.quads,
            inset_rect(parent, 3.0),
            [0.31, 0.40, 0.72, 0.85],
            [0.0; 4],
            0.0,
        );
    }
    scene.labels.push(RecordingLabel {
        text: "Envoyer vers".into(),
        bounds: Rect {
            x: parent.x + 10.0,
            width: parent.width - 34.0,
            ..parent
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        font_size: 12.0,
        color: TEXT,
    });
    scene.labels.push(label(
        ">",
        Rect {
            x: parent.x + parent.width - 24.0,
            width: 18.0,
            ..parent
        },
        12.0,
        MUTED_TEXT,
    ));

    if menu.submenu_open {
        let submenu = Rect {
            x: parent.x + parent.width - 2.0,
            ..parent
        };
        push_quad(
            &mut scene.quads,
            submenu,
            [0.13, 0.13, 0.16, 0.99],
            BORDER,
            0.0,
        );
        if menu.hover_voicelines {
            push_quad(
                &mut scene.quads,
                inset_rect(submenu, 3.0),
                [0.31, 0.40, 0.72, 0.85],
                [0.0; 4],
                0.0,
            );
        }
        scene.labels.push(RecordingLabel {
            text: "Voicelines".into(),
            bounds: Rect {
                x: submenu.x + 10.0,
                width: submenu.width - 20.0,
                ..submenu
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            font_size: 12.0,
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
        selected: true,
        enabled: true,
    });
}

fn push_tool_controls(
    scene: &mut RecordingScene,
    tools: Rect,
    active: RecordingTool,
    enabled: bool,
    has_selection: bool,
    has_armed_track: bool,
) {
    for (index, (tool, text, key)) in [
        (RecordingTool::Select, "Sélection", "recording.tool.select"),
        (RecordingTool::Cut, "Couper", "recording.tool.cut"),
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
    scene.controls.push(RecordingControlInfo {
        control: RecordingControl::AddTrack,
        bounds: add_bounds,
        role: AccessibleRole::Button,
        label: crate::i18n::t("recording.track.add").into(),
        value: None,
        selected: false,
        enabled,
    });
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
    scene.controls.push(RecordingControlInfo {
        control: RecordingControl::DeleteSelectedClips,
        bounds: delete_bounds,
        role: AccessibleRole::Button,
        label: crate::i18n::t("shortcut.delete").into(),
        value: None,
        selected: false,
        enabled: enabled && has_selection,
    });
    let record_bounds = Rect {
        x: tools.x + 7.0,
        y: tools.y + 124.0,
        width: tools.width - 14.0,
        height: 32.0,
    };
    push_quad(
        &mut scene.quads,
        record_bounds,
        if has_armed_track {
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
        12.0,
        if enabled && has_armed_track {
            TEXT
        } else {
            MUTED_TEXT
        },
    ));
    scene.controls.push(RecordingControlInfo {
        control: RecordingControl::StartCapture,
        bounds: record_bounds,
        role: AccessibleRole::Button,
        label: crate::i18n::t("recording.capture.start").into(),
        value: None,
        selected: false,
        enabled: enabled && has_armed_track,
    });
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
    let scroll = effective_track_scroll(body, track_count, ui.track_scroll);
    for (visible_row, track) in project
        .tracks()
        .skip(scroll)
        .take(visible_track_rows(body))
        .enumerate()
    {
        let row = scroll + visible_row;
        let y = headers.y + visible_row as f32 * TRACK_ROW_H;
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
            font_size: 12.0,
            color: TEXT,
        });
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::RenameTrack(track.id),
            bounds: name_bounds,
            role: AccessibleRole::Button,
            label: format!(
                "{} — {}",
                track.name,
                crate::i18n::t("recording.track.rename")
            ),
            value: None,
            selected: ui.renaming_track == Some(track.id),
            enabled: ui.role.can_edit_timeline(),
        });
        let track_volume = ui.track_volume(track.id);
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
                width: volume_track.width * (track_volume / crate::recording_mix::TRACK_VOLUME_MAX),
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
            volume_bounds,
            9.0,
            TEXT,
        ));
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::TrackVolume(track.id),
            bounds: volume_bounds,
            role: AccessibleRole::Slider,
            label: format!(
                "{} â€” {}",
                track.name,
                crate::i18n::t("recording.track.volume")
            ),
            value: Some(format!("{:.0} %", track_volume * 100.0)),
            selected: false,
            enabled: ui.role.can_adjust_track_volume(),
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
            (
                RecordingControl::TrackExport(track.id),
                "E",
                false,
                [0.25, 0.65, 0.35, 1.0],
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
                    "E" => format!(
                        "{} — {}",
                        track.name,
                        crate::i18n::t("recording.track.export")
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
        let remove_bounds = Rect {
            x: header.x + header.width - 28.0,
            y: header.y + 4.0,
            width: 22.0,
            height: 22.0,
        };
        let removable = track_count > 1;
        scene.labels.push(label(
            "×",
            remove_bounds,
            20.0,
            if removable { TEXT } else { MUTED_TEXT },
        ));
        scene.controls.push(RecordingControlInfo {
            control: RecordingControl::RemoveTrack(track.id),
            bounds: remove_bounds,
            role: AccessibleRole::Button,
            label: format!(
                "{} — {}",
                track.name,
                crate::i18n::t("recording.track.remove")
            ),
            value: None,
            selected: false,
            enabled: ui.role.can_edit_timeline() && removable,
        });
    }

    for clip in project.clips() {
        let Some(track_row) = project.tracks().position(|track| track.id == clip.track_id) else {
            continue;
        };
        let Some(visible_row) = track_row.checked_sub(scroll) else {
            continue;
        };
        if visible_row >= visible_track_rows(body) {
            continue;
        }
        let x =
            body.x + (clip.start_frame as f64 - ui.view_start_frame) as f32 * ui.pixels_per_frame;
        let width = (clip.duration_frames as f32 * ui.pixels_per_frame).max(3.0);
        let clip_bounds = Rect {
            x,
            y: body.y + visible_row as f32 * TRACK_ROW_H + 6.0,
            width,
            height: TRACK_ROW_H - 12.0,
        };
        let left = clip_bounds.x.max(body.x);
        let right = (clip_bounds.x + clip_bounds.width).min(body.x + body.width);
        if right <= left {
            continue;
        }
        let bounds = Rect {
            x: left,
            width: right - left,
            ..clip_bounds
        };
        let selected = ui.is_clip_selected(clip.id);
        push_quad(
            &mut scene.quads,
            bounds,
            if selected {
                [0.20, 0.27, 0.62, 1.0]
            } else {
                [0.13, 0.20, 0.46, 1.0]
            },
            if selected {
                [0.62, 0.70, 1.0, 1.0]
            } else {
                BORDER
            },
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
            push_quad(
                &mut scene.quads,
                bounds,
                [0.0; 4],
                if selected {
                    [0.62, 0.70, 1.0, 1.0]
                } else {
                    BORDER
                },
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
    let viewport_top = rect.y + 40.0;
    let viewport_bottom = rect.y + rect.height;
    let max_scroll = (ui.asset_content_height - (rect.height - 40.0).max(1.0)).max(0.0);
    let mut y = viewport_top - ui.asset_scroll.min(max_scroll);
    for (owner, assets) in grouped_assets(project) {
        let expanded = ui.expanded_asset_owners.contains(&owner);
        let group_bounds = Rect {
            x: rect.x + 10.0,
            y,
            width: rect.width - 24.0,
            height: ASSET_GROUP_H,
        };
        if y >= viewport_top && y + ASSET_GROUP_H <= viewport_bottom {
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
                font_size: 12.0,
                color: MUTED_TEXT,
            });
            scene.controls.push(RecordingControlInfo {
                control: RecordingControl::AssetGroup(owner.clone()),
                bounds: group_bounds,
                role: AccessibleRole::Button,
                label: owner.clone(),
                value: None,
                selected: expanded,
                enabled: true,
            });
        }
        y += ASSET_GROUP_H;
        if !expanded {
            continue;
        }
        for asset in assets {
            let bounds = Rect {
                x: rect.x + 10.0,
                y: y + 3.0,
                width: rect.width - 24.0,
                height: 36.0,
            };
            y += ASSET_ROW_H;
            if bounds.y < viewport_top || bounds.y + bounds.height > viewport_bottom {
                continue;
            }
            let selected = ui.selected_asset == Some(asset.id);
            let used = project.clips().any(|clip| clip.asset_id == asset.id);
            let fill = match (used, selected) {
                (true, true) => USED_AUDIO_SELECTED,
                (true, false) => USED_AUDIO,
                (false, true) => ACCENT,
                (false, false) => PANEL_BG,
            };
            push_quad(&mut scene.quads, bounds, fill, BORDER, 5.0);
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
        let role_label = match member.role.as_str() {
            "admin" => crate::i18n::t("recording.role.director"),
            "co_da" => crate::i18n::t("recording.role.co_director"),
            _ => crate::i18n::t("recording.role.actor"),
        };
        let microphone = if member.role == "actor" && !member.muted {
            format!(
                " · {}",
                crate::i18n::t(if member.recording_ready {
                    "recording.microphone.ready_short"
                } else {
                    "recording.microphone.not_ready_short"
                })
            )
        } else {
            String::new()
        };
        scene.labels.push(RecordingLabel {
            text: format!(
                "{} · {}{}{}",
                member.username,
                role_label,
                if member.muted { " · muet" } else { "" },
                microphone
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
            value: Some(role_label.to_string()),
            selected: controls,
            enabled: true,
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
            .find_map(|control| match control.control {
                RecordingControl::TrackMute(id) => Some(id),
                _ => None,
            });
        assert_eq!(first_visible, Some(track_ids[1]));
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

        assert_eq!(
            ui.handle_asset_context_menu(
                &UiEvent::ContextMenu { x: 60.0, y: 60.0 },
                &scene,
                screen,
            ),
            Some(EventResponse::Consumed)
        );
        assert_eq!(
            ui.handle_asset_context_menu(
                &UiEvent::MousePress { x: 70.0, y: 70.0 },
                &scene,
                screen,
            ),
            Some(EventResponse::Consumed)
        );
        assert_eq!(
            ui.handle_asset_context_menu(
                &UiEvent::MousePress { x: 220.0, y: 70.0 },
                &scene,
                screen,
            ),
            Some(EventResponse::Action(
                UiAction::RecordingSendAssetToVoicelines(asset_id)
            ))
        );
    }
}
