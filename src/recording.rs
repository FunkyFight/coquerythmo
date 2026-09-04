//! Backend-neutral domain model for the recording workspace.
//!
//! This module deliberately contains no UI, `winit`, `wgpu`, CPAL, FFmpeg or
//! filesystem code. It owns the durable audio timeline, its transaction log,
//! editor selection semantics and the deterministic capture state machine.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};

pub const DEFAULT_TIMELINE_FPS: f64 = 24.0;
pub const RECORDING_COUNTDOWN: Duration = Duration::from_secs(3);

macro_rules! domain_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

domain_id!(AudioAssetId);
domain_id!(AudioTrackId);
domain_id!(AudioClipId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingError {
    InvalidTimelineFps,
    InvalidAsset(AudioAssetId, String),
    InvalidTrack(AudioTrackId, String),
    InvalidClip(AudioClipId, String),
    MissingAsset(AudioAssetId),
    MissingTrack(AudioTrackId),
    TrackNotArmed(AudioTrackId),
    MissingClip(AudioClipId),
    DuplicateAsset(AudioAssetId),
    DuplicateTrack(AudioTrackId),
    DuplicateClip(AudioClipId),
    AssetInUse(AudioAssetId),
    TrackInUse(AudioTrackId),
    DuplicateOperationTarget,
    MoreThanOneArmedTrack,
    InvalidTransactionCursor { cursor: usize, len: usize },
    TransactionCursorNotAtEnd { cursor: usize, len: usize },
    InvalidTransactionSequence { expected: u64, actual: u64 },
    InvalidTransactionChain { sequence: u64 },
    TransactionSerialization(String),
    CaptureBusy,
    CaptureNotActive,
    InvalidCaptureStartFrame,
    Recorder(String),
}

impl fmt::Display for RecordingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimelineFps => write!(formatter, "invalid recording timeline FPS"),
            Self::InvalidAsset(id, reason) => {
                write!(formatter, "invalid audio asset {id}: {reason}")
            }
            Self::InvalidTrack(id, reason) => {
                write!(formatter, "invalid audio track {id}: {reason}")
            }
            Self::InvalidClip(id, reason) => write!(formatter, "invalid audio clip {id}: {reason}"),
            Self::MissingAsset(id) => write!(formatter, "audio asset {id} does not exist"),
            Self::MissingTrack(id) => write!(formatter, "audio track {id} does not exist"),
            Self::TrackNotArmed(id) => write!(formatter, "audio track {id} is not armed"),
            Self::MissingClip(id) => write!(formatter, "audio clip {id} does not exist"),
            Self::DuplicateAsset(id) => write!(formatter, "audio asset {id} already exists"),
            Self::DuplicateTrack(id) => write!(formatter, "audio track {id} already exists"),
            Self::DuplicateClip(id) => write!(formatter, "audio clip {id} already exists"),
            Self::AssetInUse(id) => write!(formatter, "audio asset {id} is still used by a clip"),
            Self::TrackInUse(id) => write!(formatter, "audio track {id} still contains clips"),
            Self::DuplicateOperationTarget => write!(
                formatter,
                "an operation targets the same object more than once"
            ),
            Self::MoreThanOneArmedTrack => write!(formatter, "only one audio track may be armed"),
            Self::InvalidTransactionCursor { cursor, len } => {
                write!(
                    formatter,
                    "transaction cursor {cursor} is past log length {len}"
                )
            }
            Self::TransactionCursorNotAtEnd { cursor, len } => write!(
                formatter,
                "transaction cursor {cursor} must be at log end {len} before receiving a live transaction"
            ),
            Self::InvalidTransactionSequence { expected, actual } => {
                write!(
                    formatter,
                    "invalid transaction sequence {actual}, expected {expected}"
                )
            }
            Self::InvalidTransactionChain { sequence } => {
                write!(
                    formatter,
                    "transaction integrity check failed at sequence {sequence}"
                )
            }
            Self::TransactionSerialization(reason) => {
                write!(
                    formatter,
                    "cannot serialize recording transaction: {reason}"
                )
            }
            Self::CaptureBusy => write!(formatter, "a recording capture is already active"),
            Self::CaptureNotActive => write!(formatter, "no recording capture is active"),
            Self::InvalidCaptureStartFrame => {
                write!(formatter, "capture start frame cannot be negative")
            }
            Self::Recorder(reason) => write!(formatter, "audio recorder error: {reason}"),
        }
    }
}

impl std::error::Error for RecordingError {}

/// Compact peak data independent of any renderer.
///
/// One peak represents `samples_per_peak` sample frames (all channels folded
/// into the maximum absolute amplitude). Values must be finite and in 0..=1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveformData {
    pub samples_per_peak: u32,
    pub peaks: Vec<f32>,
}

impl Default for WaveformData {
    fn default() -> Self {
        Self {
            samples_per_peak: 1,
            peaks: Vec::new(),
        }
    }
}

impl WaveformData {
    pub fn new(samples_per_peak: u32, peaks: Vec<f32>) -> Result<Self, RecordingError> {
        let waveform = Self {
            samples_per_peak,
            peaks,
        };
        waveform.validate(AudioAssetId::new(0))?;
        Ok(waveform)
    }

    pub fn peak_for_sample(&self, sample_frame: u64) -> Option<f32> {
        let index = sample_frame / u64::from(self.samples_per_peak.max(1));
        usize::try_from(index)
            .ok()
            .and_then(|index| self.peaks.get(index).copied())
    }

    fn validate(&self, asset_id: AudioAssetId) -> Result<(), RecordingError> {
        if self.samples_per_peak == 0 {
            return Err(RecordingError::InvalidAsset(
                asset_id,
                "waveform samples_per_peak must be positive".into(),
            ));
        }
        if self
            .peaks
            .iter()
            .any(|peak| !peak.is_finite() || !(0.0..=1.0).contains(peak))
        {
            return Err(RecordingError::InvalidAsset(
                asset_id,
                "waveform peaks must be finite normalized amplitudes".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAsset {
    pub id: AudioAssetId,
    /// Portable leaf name. The archive/adapter resolves it to a real path.
    pub file_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Number of sample frames, not interleaved scalar samples.
    pub sample_count: u64,
    /// Integrity identifier produced by the media adapter.
    pub checksum: String,
    #[serde(default)]
    pub waveform: WaveformData,
}

impl AudioAsset {
    pub fn duration_seconds(&self) -> f64 {
        self.sample_count as f64 / f64::from(self.sample_rate.max(1))
    }

    pub fn duration_frames(&self, timeline_fps: f64) -> i64 {
        (self.duration_seconds() * timeline_fps)
            .round()
            .clamp(1.0, i64::MAX as f64) as i64
    }

    fn validate(&self) -> Result<(), RecordingError> {
        let file_name = self.file_name.as_str();
        if file_name.trim().is_empty()
            || file_name.trim() != file_name
            || file_name.chars().any(char::is_control)
            || file_name
                .chars()
                .any(|character| matches!(character, '/' | '\\' | ':'))
            || matches!(file_name, "." | "..")
        {
            return Err(RecordingError::InvalidAsset(
                self.id,
                "file name must be a portable leaf name".into(),
            ));
        }
        if !file_name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("flac"))
        {
            return Err(RecordingError::InvalidAsset(
                self.id,
                "recorded audio must use the .flac extension".into(),
            ));
        }
        if self.sample_rate == 0 || self.channels == 0 || self.sample_count == 0 {
            return Err(RecordingError::InvalidAsset(
                self.id,
                "sample rate, channel count and sample count must be positive".into(),
            ));
        }
        if self.checksum.len() != 40
            || !self
                .checksum
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(RecordingError::InvalidAsset(
                self.id,
                "checksum must be a lowercase 40-character SHA-1 digest".into(),
            ));
        }
        self.waveform.validate(self.id)?;
        let maximum_peak_count = self
            .sample_count
            .div_ceil(u64::from(self.waveform.samples_per_peak));
        if u64::try_from(self.waveform.peaks.len()).unwrap_or(u64::MAX) > maximum_peak_count {
            return Err(RecordingError::InvalidAsset(
                self.id,
                "waveform contains more peaks than the audio duration permits".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: AudioTrackId,
    pub name: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub armed: bool,
}

impl AudioTrack {
    pub fn new(id: AudioTrackId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            muted: false,
            solo: false,
            armed: false,
        }
    }

    fn validate(&self) -> Result<(), RecordingError> {
        if self.name.trim().is_empty() || self.name.chars().any(char::is_control) {
            return Err(RecordingError::InvalidTrack(
                self.id,
                "track name is empty or contains control characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioClip {
    pub id: AudioClipId,
    pub asset_id: AudioAssetId,
    pub track_id: AudioTrackId,
    pub start_frame: i64,
    pub source_start_frame: i64,
    pub duration_frames: i64,
}

impl AudioClip {
    pub fn end_frame(&self) -> i64 {
        self.start_frame.saturating_add(self.duration_frames)
    }

    pub fn source_end_frame(&self) -> i64 {
        self.source_start_frame.saturating_add(self.duration_frames)
    }
}

fn default_timeline_fps() -> f64 {
    DEFAULT_TIMELINE_FPS
}

/// Durable recording timeline. B-tree maps provide deterministic iteration and
/// logarithmic lookup; replay cost remains proportional to operations applied.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordingProject {
    #[serde(default = "default_timeline_fps")]
    timeline_fps: f64,
    #[serde(default)]
    assets: BTreeMap<AudioAssetId, AudioAsset>,
    #[serde(default)]
    tracks: BTreeMap<AudioTrackId, AudioTrack>,
    #[serde(default)]
    clips: BTreeMap<AudioClipId, AudioClip>,
    #[serde(default = "default_next_id")]
    next_id: u64,
}

const fn default_next_id() -> u64 {
    1
}

#[derive(Deserialize)]
struct RecordingProjectWire {
    #[serde(default = "default_timeline_fps")]
    timeline_fps: f64,
    #[serde(default)]
    assets: BTreeMap<AudioAssetId, AudioAsset>,
    #[serde(default)]
    tracks: BTreeMap<AudioTrackId, AudioTrack>,
    #[serde(default)]
    clips: BTreeMap<AudioClipId, AudioClip>,
    #[serde(default = "default_next_id")]
    next_id: u64,
}

impl<'de> Deserialize<'de> for RecordingProject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RecordingProjectWire::deserialize(deserializer)?;
        let project = Self {
            timeline_fps: wire.timeline_fps,
            assets: wire.assets,
            tracks: wire.tracks,
            clips: wire.clips,
            next_id: wire.next_id.max(1),
        };
        project.validate().map_err(D::Error::custom)?;
        Ok(project)
    }
}

impl Default for RecordingProject {
    fn default() -> Self {
        Self::new(DEFAULT_TIMELINE_FPS).expect("default FPS is valid")
    }
}

impl RecordingProject {
    pub fn new(timeline_fps: f64) -> Result<Self, RecordingError> {
        if !valid_fps(timeline_fps) {
            return Err(RecordingError::InvalidTimelineFps);
        }
        Ok(Self {
            timeline_fps,
            assets: BTreeMap::new(),
            tracks: BTreeMap::new(),
            clips: BTreeMap::new(),
            next_id: 1,
        })
    }

    pub fn timeline_fps(&self) -> f64 {
        self.timeline_fps
    }

    pub fn assets(&self) -> impl ExactSizeIterator<Item = &AudioAsset> {
        self.assets.values()
    }

    pub fn tracks(&self) -> impl ExactSizeIterator<Item = &AudioTrack> {
        self.tracks.values()
    }

    pub fn clips(&self) -> impl ExactSizeIterator<Item = &AudioClip> {
        self.clips.values()
    }

    pub fn asset(&self, id: AudioAssetId) -> Option<&AudioAsset> {
        self.assets.get(&id)
    }

    pub fn track(&self, id: AudioTrackId) -> Option<&AudioTrack> {
        self.tracks.get(&id)
    }

    pub fn clip(&self, id: AudioClipId) -> Option<&AudioClip> {
        self.clips.get(&id)
    }

    pub fn armed_track_id(&self) -> Option<AudioTrackId> {
        self.tracks
            .values()
            .find(|track| track.armed)
            .map(|track| track.id)
    }

    pub fn is_track_audible(&self, id: AudioTrackId) -> Result<bool, RecordingError> {
        let track = self.track(id).ok_or(RecordingError::MissingTrack(id))?;
        let any_solo = self.tracks.values().any(|candidate| candidate.solo);
        Ok(!track.muted && (!any_solo || track.solo))
    }

    pub fn allocate_asset_id(&mut self) -> AudioAssetId {
        AudioAssetId::new(self.allocate_raw_id())
    }

    pub fn allocate_track_id(&mut self) -> AudioTrackId {
        AudioTrackId::new(self.allocate_raw_id())
    }

    pub fn allocate_clip_id(&mut self) -> AudioClipId {
        AudioClipId::new(self.allocate_raw_id())
    }

    /// Propose stable IDs for a capture without changing durable state.
    /// Cancelling a countdown therefore cannot advance the serialized ID
    /// allocator or make transaction-log reconstruction diverge.
    pub fn propose_capture_target(
        &self,
        track_id: AudioTrackId,
        start_frame: i64,
    ) -> Result<CaptureTarget, RecordingError> {
        let track = self
            .track(track_id)
            .ok_or(RecordingError::MissingTrack(track_id))?;
        if !track.armed {
            return Err(RecordingError::TrackNotArmed(track_id));
        }
        if start_frame < 0 {
            return Err(RecordingError::InvalidCaptureStartFrame);
        }
        let asset_raw = self.next_available_raw_id(self.next_id.max(1), &[]);
        let clip_raw = self.next_available_raw_id(next_raw_id(asset_raw), &[asset_raw]);
        Ok(CaptureTarget {
            track_id,
            asset_id: AudioAssetId::new(asset_raw),
            clip_id: AudioClipId::new(clip_raw),
            start_frame,
        })
    }

    fn allocate_raw_id(&mut self) -> u64 {
        let candidate = self.next_available_raw_id(self.next_id.max(1), &[]);
        self.next_id = next_raw_id(candidate);
        candidate
    }

    fn next_available_raw_id(&self, mut candidate: u64, reserved: &[u64]) -> u64 {
        loop {
            let occupied = reserved.contains(&candidate)
                || self.assets.contains_key(&AudioAssetId::new(candidate))
                || self.tracks.contains_key(&AudioTrackId::new(candidate))
                || self.clips.contains_key(&AudioClipId::new(candidate));
            if !occupied {
                return candidate;
            }
            candidate = next_raw_id(candidate);
        }
    }

    fn observe_id(&mut self, value: u64) {
        self.next_id = self.next_id.max(value.saturating_add(1)).max(1);
    }

    /// Apply one operation atomically. Invalid batches leave the project
    /// unchanged, which is important for network replay and capture finalizing.
    pub fn apply(&mut self, operation: &RecordingOperation) -> Result<(), RecordingError> {
        let mut candidate = self.clone();
        candidate.apply_inner(operation)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn apply_inner(&mut self, operation: &RecordingOperation) -> Result<(), RecordingError> {
        match operation {
            RecordingOperation::Batch { operations } => {
                for operation in operations {
                    self.apply_inner(operation)?;
                }
            }
            RecordingOperation::AddAsset { asset } => {
                if self.assets.contains_key(&asset.id) {
                    return Err(RecordingError::DuplicateAsset(asset.id));
                }
                asset.validate()?;
                self.observe_id(asset.id.get());
                self.assets.insert(asset.id, asset.clone());
            }
            RecordingOperation::ReplaceAsset { asset } => {
                if !self.assets.contains_key(&asset.id) {
                    return Err(RecordingError::MissingAsset(asset.id));
                }
                asset.validate()?;
                self.assets.insert(asset.id, asset.clone());
            }
            RecordingOperation::RemoveAsset { asset_id } => {
                if self.clips.values().any(|clip| clip.asset_id == *asset_id) {
                    return Err(RecordingError::AssetInUse(*asset_id));
                }
                self.assets
                    .remove(asset_id)
                    .ok_or(RecordingError::MissingAsset(*asset_id))?;
            }
            RecordingOperation::AddTrack { track } => {
                if self.tracks.contains_key(&track.id) {
                    return Err(RecordingError::DuplicateTrack(track.id));
                }
                track.validate()?;
                self.observe_id(track.id.get());
                self.tracks.insert(track.id, track.clone());
            }
            RecordingOperation::RemoveTrack { track_id } => {
                if self.clips.values().any(|clip| clip.track_id == *track_id) {
                    return Err(RecordingError::TrackInUse(*track_id));
                }
                self.tracks
                    .remove(track_id)
                    .ok_or(RecordingError::MissingTrack(*track_id))?;
            }
            RecordingOperation::RenameTrack { track_id, name } => {
                let track = self
                    .tracks
                    .get_mut(track_id)
                    .ok_or(RecordingError::MissingTrack(*track_id))?;
                track.name = name.clone();
            }
            RecordingOperation::SetTrackMuted { track_id, muted } => {
                self.tracks
                    .get_mut(track_id)
                    .ok_or(RecordingError::MissingTrack(*track_id))?
                    .muted = *muted;
            }
            RecordingOperation::SetTrackSolo { track_id, solo } => {
                self.tracks
                    .get_mut(track_id)
                    .ok_or(RecordingError::MissingTrack(*track_id))?
                    .solo = *solo;
            }
            RecordingOperation::ArmTrack { track_id } => {
                if let Some(track_id) = track_id {
                    if !self.tracks.contains_key(track_id) {
                        return Err(RecordingError::MissingTrack(*track_id));
                    }
                }
                for track in self.tracks.values_mut() {
                    track.armed = Some(track.id) == *track_id;
                }
            }
            RecordingOperation::AddClip { clip } => {
                if self.clips.contains_key(&clip.id) {
                    return Err(RecordingError::DuplicateClip(clip.id));
                }
                self.validate_clip(clip)?;
                self.observe_id(clip.id.get());
                self.clips.insert(clip.id, clip.clone());
            }
            RecordingOperation::MoveClips { placements } => {
                ensure_unique(placements.iter().map(|placement| placement.clip_id))?;
                for placement in placements {
                    if !self.clips.contains_key(&placement.clip_id) {
                        return Err(RecordingError::MissingClip(placement.clip_id));
                    }
                    if !self.tracks.contains_key(&placement.track_id) {
                        return Err(RecordingError::MissingTrack(placement.track_id));
                    }
                    if placement.start_frame < 0 {
                        return Err(RecordingError::InvalidClip(
                            placement.clip_id,
                            "start frame cannot be negative".into(),
                        ));
                    }
                }
                for placement in placements {
                    let clip = self
                        .clips
                        .get_mut(&placement.clip_id)
                        .expect("validated clip");
                    clip.start_frame = placement.start_frame;
                    clip.track_id = placement.track_id;
                }
            }
            RecordingOperation::SplitClip {
                clip_id,
                at_frame,
                right_clip_id,
            } => {
                if self.clips.contains_key(right_clip_id) {
                    return Err(RecordingError::DuplicateClip(*right_clip_id));
                }
                let original = self
                    .clips
                    .get(clip_id)
                    .cloned()
                    .ok_or(RecordingError::MissingClip(*clip_id))?;
                if *at_frame <= original.start_frame || *at_frame >= original.end_frame() {
                    return Err(RecordingError::InvalidClip(
                        *clip_id,
                        "cut must be strictly inside the clip".into(),
                    ));
                }
                let left_duration = at_frame - original.start_frame;
                let right_duration = original.duration_frames - left_duration;
                self.clips
                    .get_mut(clip_id)
                    .expect("validated clip")
                    .duration_frames = left_duration;
                let mut right = original;
                right.id = *right_clip_id;
                right.start_frame = *at_frame;
                right.source_start_frame = right.source_start_frame.saturating_add(left_duration);
                right.duration_frames = right_duration;
                self.observe_id(right_clip_id.get());
                self.clips.insert(*right_clip_id, right);
            }
            RecordingOperation::DeleteClips { clip_ids } => {
                ensure_unique(clip_ids.iter().copied())?;
                for clip_id in clip_ids {
                    if !self.clips.contains_key(clip_id) {
                        return Err(RecordingError::MissingClip(*clip_id));
                    }
                }
                for clip_id in clip_ids {
                    self.clips.remove(clip_id);
                }
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), RecordingError> {
        if !valid_fps(self.timeline_fps) {
            return Err(RecordingError::InvalidTimelineFps);
        }
        for (id, asset) in &self.assets {
            if *id != asset.id {
                return Err(RecordingError::InvalidAsset(
                    asset.id,
                    "asset map key does not match its embedded ID".into(),
                ));
            }
            asset.validate()?;
        }
        for (id, track) in &self.tracks {
            if *id != track.id {
                return Err(RecordingError::InvalidTrack(
                    track.id,
                    "track map key does not match its embedded ID".into(),
                ));
            }
            track.validate()?;
        }
        if self.tracks.values().filter(|track| track.armed).count() > 1 {
            return Err(RecordingError::MoreThanOneArmedTrack);
        }
        for (id, clip) in &self.clips {
            if *id != clip.id {
                return Err(RecordingError::InvalidClip(
                    clip.id,
                    "clip map key does not match its embedded ID".into(),
                ));
            }
            self.validate_clip(clip)?;
        }
        Ok(())
    }

    fn validate_clip(&self, clip: &AudioClip) -> Result<(), RecordingError> {
        if clip.start_frame < 0 || clip.source_start_frame < 0 || clip.duration_frames <= 0 {
            return Err(RecordingError::InvalidClip(
                clip.id,
                "timeline/source starts must be non-negative and duration positive".into(),
            ));
        }
        let asset = self
            .assets
            .get(&clip.asset_id)
            .ok_or(RecordingError::MissingAsset(clip.asset_id))?;
        if !self.tracks.contains_key(&clip.track_id) {
            return Err(RecordingError::MissingTrack(clip.track_id));
        }
        if clip.source_end_frame() > asset.duration_frames(self.timeline_fps) {
            return Err(RecordingError::InvalidClip(
                clip.id,
                "clip exceeds its source audio asset".into(),
            ));
        }
        Ok(())
    }
}

fn valid_fps(fps: f64) -> bool {
    fps.is_finite() && fps > 0.0
}

fn next_raw_id(id: u64) -> u64 {
    if id == u64::MAX {
        1
    } else {
        id + 1
    }
}

fn ensure_unique<T>(values: impl IntoIterator<Item = T>) -> Result<(), RecordingError>
where
    T: Ord,
{
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(RecordingError::DuplicateOperationTarget);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingTool {
    Select,
    Cut,
}

impl Default for RecordingTool {
    fn default() -> Self {
        Self::Select
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipPlacement {
    pub clip_id: AudioClipId,
    pub track_id: AudioTrackId,
    pub start_frame: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RecordingOperation {
    Batch {
        operations: Vec<RecordingOperation>,
    },
    AddAsset {
        asset: AudioAsset,
    },
    ReplaceAsset {
        asset: AudioAsset,
    },
    RemoveAsset {
        asset_id: AudioAssetId,
    },
    AddTrack {
        track: AudioTrack,
    },
    RemoveTrack {
        track_id: AudioTrackId,
    },
    RenameTrack {
        track_id: AudioTrackId,
        name: String,
    },
    SetTrackMuted {
        track_id: AudioTrackId,
        muted: bool,
    },
    SetTrackSolo {
        track_id: AudioTrackId,
        solo: bool,
    },
    /// `None` disarms every track. Arming one track atomically disarms others.
    ArmTrack {
        track_id: Option<AudioTrackId>,
    },
    AddClip {
        clip: AudioClip,
    },
    MoveClips {
        placements: Vec<ClipPlacement>,
    },
    SplitClip {
        clip_id: AudioClipId,
        at_frame: i64,
        right_clip_id: AudioClipId,
    },
    DeleteClips {
        clip_ids: Vec<AudioClipId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordingTransaction {
    pub sequence: u64,
    #[serde(with = "serde_u64_hex")]
    pub previous_integrity: u64,
    #[serde(with = "serde_u64_hex")]
    pub integrity: u64,
    pub operation: RecordingOperation,
}

// Node relays Socket.IO payloads through JavaScript numbers, which cannot
// preserve arbitrary u64 values. Hashes therefore cross every JSON boundary
// as canonical fixed-width strings while remaining integers in the domain.
mod serde_u64_hex {
    use super::*;

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{value:016x}"))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.len() != 16
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(D::Error::custom(
                "transaction integrity must be 16 lowercase hexadecimal characters",
            ));
        }
        u64::from_str_radix(&value, 16).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct TransactionLog {
    #[serde(default)]
    entries: Vec<RecordingTransaction>,
    #[serde(default)]
    cursor: usize,
}

#[derive(Deserialize)]
struct TransactionLogWire {
    #[serde(default)]
    entries: Vec<RecordingTransaction>,
    #[serde(default)]
    cursor: usize,
}

impl<'de> Deserialize<'de> for TransactionLog {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TransactionLogWire::deserialize(deserializer)?;
        let log = Self {
            entries: wire.entries,
            cursor: wire.cursor,
        };
        log.verify_integrity().map_err(D::Error::custom)?;
        Ok(log)
    }
}

impl TransactionLog {
    pub fn entries(&self) -> &[RecordingTransaction] {
        &self.entries
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn entry_by_sequence(&self, sequence: u64) -> Option<&RecordingTransaction> {
        self.entries
            .binary_search_by_key(&sequence, |transaction| transaction.sequence)
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    pub fn entries_after(&self, cursor: usize) -> Result<&[RecordingTransaction], RecordingError> {
        if cursor > self.entries.len() {
            return Err(RecordingError::InvalidTransactionCursor {
                cursor,
                len: self.entries.len(),
            });
        }
        Ok(&self.entries[cursor..])
    }

    pub fn set_cursor(&mut self, cursor: usize) -> Result<(), RecordingError> {
        if cursor > self.entries.len() {
            return Err(RecordingError::InvalidTransactionCursor {
                cursor,
                len: self.entries.len(),
            });
        }
        self.cursor = cursor;
        Ok(())
    }

    pub fn verify_integrity(&self) -> Result<(), RecordingError> {
        if self.cursor > self.entries.len() {
            return Err(RecordingError::InvalidTransactionCursor {
                cursor: self.cursor,
                len: self.entries.len(),
            });
        }
        let mut previous = 0_u64;
        for (index, transaction) in self.entries.iter().enumerate() {
            let expected_sequence = index as u64;
            if transaction.sequence != expected_sequence {
                return Err(RecordingError::InvalidTransactionSequence {
                    expected: expected_sequence,
                    actual: transaction.sequence,
                });
            }
            let expected_integrity =
                transaction_integrity(transaction.sequence, previous, &transaction.operation)?;
            if transaction.previous_integrity != previous
                || transaction.integrity != expected_integrity
            {
                return Err(RecordingError::InvalidTransactionChain {
                    sequence: transaction.sequence,
                });
            }
            previous = transaction.integrity;
        }
        Ok(())
    }

    fn verify_active_boundary(&self) -> Result<(), RecordingError> {
        if self.cursor > self.entries.len() {
            return Err(RecordingError::InvalidTransactionCursor {
                cursor: self.cursor,
                len: self.entries.len(),
            });
        }
        let Some(transaction) = self
            .cursor
            .checked_sub(1)
            .and_then(|index| self.entries.get(index))
        else {
            return Ok(());
        };
        let expected_sequence = (self.cursor - 1) as u64;
        if transaction.sequence != expected_sequence {
            return Err(RecordingError::InvalidTransactionSequence {
                expected: expected_sequence,
                actual: transaction.sequence,
            });
        }
        let previous = self
            .cursor
            .checked_sub(2)
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.integrity)
            .unwrap_or(0);
        let expected_integrity =
            transaction_integrity(transaction.sequence, previous, &transaction.operation)?;
        if transaction.previous_integrity != previous || transaction.integrity != expected_integrity
        {
            return Err(RecordingError::InvalidTransactionChain {
                sequence: transaction.sequence,
            });
        }
        Ok(())
    }

    /// Apply and append an operation as one atomic local transaction.
    /// Appending after a rewound cursor intentionally creates a new branch and
    /// discards the no-longer-applied tail, matching ordinary undo histories.
    pub fn append_and_apply(
        &mut self,
        project: &mut RecordingProject,
        operation: RecordingOperation,
    ) -> Result<&RecordingTransaction, RecordingError> {
        // A deserialized log is fully validated at its boundary. Local appends
        // only need to validate the active chain edge, keeping the journal
        // bookkeeping itself O(1).
        self.verify_active_boundary()?;
        if self.cursor < self.entries.len() {
            self.entries.truncate(self.cursor);
        }
        let sequence = self.entries.len() as u64;
        let previous_integrity = self
            .entries
            .last()
            .map(|transaction| transaction.integrity)
            .unwrap_or(0);
        let integrity = transaction_integrity(sequence, previous_integrity, &operation)?;

        project.apply(&operation)?;
        self.entries.push(RecordingTransaction {
            sequence,
            previous_integrity,
            integrity,
            operation,
        });
        self.cursor = self.entries.len();
        Ok(self.entries.last().expect("transaction was appended"))
    }

    /// Verify and apply one transaction received from the authoritative live
    /// session. Unlike a local append, this never truncates a redo tail: live
    /// deltas are only accepted on the exact active chain tip.
    pub fn append_received_and_apply(
        &mut self,
        project: &mut RecordingProject,
        transaction: RecordingTransaction,
    ) -> Result<&RecordingTransaction, RecordingError> {
        self.verify_active_boundary()?;
        if self.cursor != self.entries.len() {
            return Err(RecordingError::TransactionCursorNotAtEnd {
                cursor: self.cursor,
                len: self.entries.len(),
            });
        }

        let expected_sequence = self.entries.len() as u64;
        if transaction.sequence != expected_sequence {
            return Err(RecordingError::InvalidTransactionSequence {
                expected: expected_sequence,
                actual: transaction.sequence,
            });
        }
        let expected_previous = self
            .entries
            .last()
            .map(|entry| entry.integrity)
            .unwrap_or(0);
        let expected_integrity = transaction_integrity(
            transaction.sequence,
            expected_previous,
            &transaction.operation,
        )?;
        if transaction.previous_integrity != expected_previous
            || transaction.integrity != expected_integrity
        {
            return Err(RecordingError::InvalidTransactionChain {
                sequence: transaction.sequence,
            });
        }

        // RecordingProject::apply is atomic, so a rejected remote operation
        // leaves both project and journal untouched.
        project.apply(&transaction.operation)?;
        self.entries.push(transaction);
        self.cursor = self.entries.len();
        Ok(self
            .entries
            .last()
            .expect("received transaction was appended"))
    }

    /// Apply all entries after the cursor atomically. Integrity is checked
    /// before any domain mutation occurs.
    pub fn replay_remaining(
        &mut self,
        project: &mut RecordingProject,
    ) -> Result<usize, RecordingError> {
        self.verify_integrity()?;
        let mut candidate = project.clone();
        for transaction in &self.entries[self.cursor..] {
            candidate.apply(&transaction.operation)?;
        }
        let applied = self.entries.len() - self.cursor;
        *project = candidate;
        self.cursor = self.entries.len();
        Ok(applied)
    }

    /// Reconstruct the state represented by the current cursor from a known
    /// base snapshot without mutating either the base or this log.
    pub fn rebuild_from_base(
        &self,
        base: &RecordingProject,
    ) -> Result<RecordingProject, RecordingError> {
        self.verify_integrity()?;
        let mut rebuilt = base.clone();
        for transaction in &self.entries[..self.cursor] {
            rebuilt.apply(&transaction.operation)?;
        }
        Ok(rebuilt)
    }
}

/// Stable non-cryptographic chain hash used to detect accidental corruption.
/// Transport code may additionally attach a cryptographic file checksum.
fn transaction_integrity(
    sequence: u64,
    previous: u64,
    operation: &RecordingOperation,
) -> Result<u64, RecordingError> {
    let serialized = serde_json::to_vec(operation)
        .map_err(|error| RecordingError::TransactionSerialization(error.to_string()))?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in sequence
        .to_le_bytes()
        .into_iter()
        .chain(previous.to_le_bytes())
        .chain(serialized)
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(hash)
}

/// Transient, backend-neutral editor state. It is intentionally kept outside
/// `RecordingProject` so selection/tool state is never persisted as content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecordingEditor {
    pub tool: RecordingTool,
    selected_clips: BTreeSet<AudioClipId>,
}

impl RecordingEditor {
    pub fn selected_clips(&self) -> impl ExactSizeIterator<Item = AudioClipId> + '_ {
        self.selected_clips.iter().copied()
    }

    pub fn clear_selection(&mut self) {
        self.selected_clips.clear();
    }

    pub fn select_clip(
        &mut self,
        project: &RecordingProject,
        clip_id: AudioClipId,
        additive: bool,
    ) -> Result<(), RecordingError> {
        if project.clip(clip_id).is_none() {
            return Err(RecordingError::MissingClip(clip_id));
        }
        if !additive {
            self.selected_clips.clear();
        }
        self.selected_clips.insert(clip_id);
        Ok(())
    }

    pub fn move_selection(
        &mut self,
        project: &mut RecordingProject,
        log: &mut TransactionLog,
        delta_frames: i64,
        destination_track: Option<AudioTrackId>,
    ) -> Result<i64, RecordingError> {
        if self.selected_clips.is_empty() {
            return Ok(0);
        }
        if let Some(track_id) = destination_track {
            if project.track(track_id).is_none() {
                return Err(RecordingError::MissingTrack(track_id));
            }
        }
        let minimum_start = self
            .selected_clips
            .iter()
            .filter_map(|id| project.clip(*id))
            .map(|clip| clip.start_frame)
            .min()
            .ok_or_else(|| {
                RecordingError::MissingClip(
                    *self
                        .selected_clips
                        .iter()
                        .next()
                        .expect("selection is not empty"),
                )
            })?;
        let effective_delta = delta_frames.max(-minimum_start);
        if effective_delta == 0 && destination_track.is_none() {
            return Ok(0);
        }
        let placements = self
            .selected_clips
            .iter()
            .map(|clip_id| {
                let clip = project
                    .clip(*clip_id)
                    .ok_or(RecordingError::MissingClip(*clip_id))?;
                Ok(ClipPlacement {
                    clip_id: *clip_id,
                    track_id: destination_track.unwrap_or(clip.track_id),
                    start_frame: clip.start_frame.saturating_add(effective_delta),
                })
            })
            .collect::<Result<Vec<_>, RecordingError>>()?;
        log.append_and_apply(project, RecordingOperation::MoveClips { placements })?;
        Ok(effective_delta)
    }

    pub fn cut_clip(
        &mut self,
        project: &mut RecordingProject,
        log: &mut TransactionLog,
        clip_id: AudioClipId,
        at_frame: i64,
    ) -> Result<AudioClipId, RecordingError> {
        // Validate before consuming an ID so a rejected edit is completely
        // side-effect free, including the project's allocator state.
        log.verify_active_boundary()?;
        let clip = project
            .clip(clip_id)
            .ok_or(RecordingError::MissingClip(clip_id))?;
        if at_frame <= clip.start_frame || at_frame >= clip.end_frame() {
            return Err(RecordingError::InvalidClip(
                clip_id,
                "cut must be strictly inside the clip".into(),
            ));
        }
        let right_clip_id = project.allocate_clip_id();
        log.append_and_apply(
            project,
            RecordingOperation::SplitClip {
                clip_id,
                at_frame,
                right_clip_id,
            },
        )?;
        self.selected_clips.clear();
        self.selected_clips.insert(clip_id);
        self.selected_clips.insert(right_clip_id);
        Ok(right_clip_id)
    }

    pub fn delete_selection(
        &mut self,
        project: &mut RecordingProject,
        log: &mut TransactionLog,
    ) -> Result<usize, RecordingError> {
        let clip_ids = self.selected_clips.iter().copied().collect::<Vec<_>>();
        if clip_ids.is_empty() {
            return Ok(0);
        }
        log.append_and_apply(project, RecordingOperation::DeleteClips { clip_ids })?;
        let deleted = self.selected_clips.len();
        self.selected_clips.clear();
        Ok(deleted)
    }
}

/// Result produced by a platform media adapter after a successful stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedAudio {
    pub file_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_count: u64,
    pub checksum: String,
    pub waveform: WaveformData,
}

impl RecordedAudio {
    pub fn into_asset(self, id: AudioAssetId) -> AudioAsset {
        AudioAsset {
            id,
            file_name: self.file_name,
            sample_rate: self.sample_rate,
            channels: self.channels,
            sample_count: self.sample_count,
            checksum: self.checksum,
            waveform: self.waveform,
        }
    }
}

pub trait AudioRecorder {
    fn start(&mut self) -> Result<(), RecordingError>;
    fn stop(&mut self) -> Result<RecordedAudio, RecordingError>;
    fn is_recording(&self) -> bool;
    fn live_waveform(&self) -> WaveformData;
}

/// Monotonic time source used by the capture state machine.
pub trait Clock {
    fn now(&self) -> Duration;
}

pub struct SystemClock {
    origin: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTarget {
    pub track_id: AudioTrackId,
    pub asset_id: AudioAssetId,
    pub clip_id: AudioClipId,
    pub start_frame: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    Countdown {
        target: CaptureTarget,
        deadline: Duration,
    },
    Capturing {
        target: CaptureTarget,
        started_at: Duration,
    },
    Finalizing {
        target: CaptureTarget,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedCapture {
    pub target: CaptureTarget,
    pub audio: RecordedAudio,
}

impl CompletedCapture {
    /// Build one atomic transaction that registers the FLAC asset and places
    /// its clip exactly at the playhead captured before the countdown.
    pub fn into_project_operation(self, timeline_fps: f64) -> RecordingOperation {
        let asset = self.audio.into_asset(self.target.asset_id);
        let duration_frames = asset.duration_frames(timeline_fps);
        let clip = AudioClip {
            id: self.target.clip_id,
            asset_id: self.target.asset_id,
            track_id: self.target.track_id,
            start_frame: self.target.start_frame,
            source_start_frame: 0,
            duration_frames,
        };
        RecordingOperation::Batch {
            operations: vec![
                RecordingOperation::AddAsset { asset },
                RecordingOperation::AddClip { clip },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureEvent {
    None,
    CountdownStarted,
    CaptureStarted { target: CaptureTarget },
    Finalizing { target: CaptureTarget },
    Finished(CompletedCapture),
    Cancelled,
    Failed { message: String },
}

pub struct CaptureController<R, C> {
    recorder: R,
    clock: C,
    state: CaptureState,
}

impl<R, C> CaptureController<R, C>
where
    R: AudioRecorder,
    C: Clock,
{
    pub fn new(recorder: R, clock: C) -> Self {
        Self {
            recorder,
            clock,
            state: CaptureState::Idle,
        }
    }

    pub fn state(&self) -> &CaptureState {
        &self.state
    }

    pub fn recorder(&self) -> &R {
        &self.recorder
    }

    pub fn live_waveform(&self) -> WaveformData {
        self.recorder.live_waveform()
    }

    pub fn begin_countdown(
        &mut self,
        target: CaptureTarget,
    ) -> Result<CaptureEvent, RecordingError> {
        if !matches!(self.state, CaptureState::Idle) {
            return Err(RecordingError::CaptureBusy);
        }
        if target.start_frame < 0 {
            return Err(RecordingError::InvalidCaptureStartFrame);
        }
        self.state = CaptureState::Countdown {
            target,
            deadline: self.clock.now().saturating_add(RECORDING_COUNTDOWN),
        };
        Ok(CaptureEvent::CountdownStarted)
    }

    pub fn countdown_seconds_remaining(&self) -> Option<u32> {
        let CaptureState::Countdown { deadline, .. } = &self.state else {
            return None;
        };
        let remaining = deadline.saturating_sub(self.clock.now());
        Some(remaining.as_secs_f64().ceil() as u32)
    }

    /// Advance time-based and finalizing transitions. `Finalizing` is kept as
    /// a distinct state so UI and accessibility can announce it before the
    /// potentially slower adapter stop is polled.
    pub fn tick(&mut self) -> CaptureEvent {
        match self.state.clone() {
            CaptureState::Countdown { target, deadline } if self.clock.now() >= deadline => {
                match self.recorder.start() {
                    Ok(()) => {
                        self.state = CaptureState::Capturing {
                            target,
                            started_at: self.clock.now(),
                        };
                        CaptureEvent::CaptureStarted { target }
                    }
                    Err(error) => self.fail(error),
                }
            }
            CaptureState::Finalizing { target } => match self.recorder.stop() {
                Ok(audio) => {
                    self.state = CaptureState::Idle;
                    CaptureEvent::Finished(CompletedCapture { target, audio })
                }
                Err(error) => self.fail(error),
            },
            _ => CaptureEvent::None,
        }
    }

    /// Escape during countdown cancels it. Escape during capture enters the
    /// finalizing state; the next `tick` performs adapter finalization.
    pub fn cancel_or_stop(&mut self) -> Result<CaptureEvent, RecordingError> {
        match self.state.clone() {
            CaptureState::Countdown { .. } => {
                self.state = CaptureState::Idle;
                Ok(CaptureEvent::Cancelled)
            }
            CaptureState::Capturing { target, .. } => {
                self.state = CaptureState::Finalizing { target };
                Ok(CaptureEvent::Finalizing { target })
            }
            CaptureState::Finalizing { target } => Ok(CaptureEvent::Finalizing { target }),
            CaptureState::Idle | CaptureState::Error { .. } => {
                Err(RecordingError::CaptureNotActive)
            }
        }
    }

    pub fn acknowledge_error(&mut self) -> bool {
        if matches!(self.state, CaptureState::Error { .. }) {
            self.state = CaptureState::Idle;
            true
        } else {
            false
        }
    }

    pub fn into_parts(self) -> (R, C) {
        (self.recorder, self.clock)
    }

    fn fail(&mut self, error: RecordingError) -> CaptureEvent {
        let message = error.to_string();
        self.state = CaptureState::Error {
            message: message.clone(),
        };
        CaptureEvent::Failed { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn waveform() -> WaveformData {
        WaveformData::new(480, vec![0.1, 0.5, 1.0]).unwrap()
    }

    fn asset(id: u64) -> AudioAsset {
        AudioAsset {
            id: AudioAssetId::new(id),
            file_name: format!("take-{id}.flac"),
            sample_rate: 48_000,
            channels: 1,
            sample_count: 96_000,
            checksum: format!("{id:040x}"),
            waveform: waveform(),
        }
    }

    fn project_with_clip() -> (RecordingProject, AudioTrackId, AudioClipId) {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = AudioTrackId::new(1);
        let clip_id = AudioClipId::new(3);
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(track_id, "Voice"),
                    },
                    RecordingOperation::AddAsset { asset: asset(2) },
                    RecordingOperation::AddClip {
                        clip: AudioClip {
                            id: clip_id,
                            asset_id: AudioAssetId::new(2),
                            track_id,
                            start_frame: 10,
                            source_start_frame: 0,
                            duration_frames: 48,
                        },
                    },
                ],
            })
            .unwrap();
        (project, track_id, clip_id)
    }

    #[test]
    fn replacing_an_asset_keeps_its_id_and_does_not_duplicate_it() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let original = asset(1);
        project
            .apply(&RecordingOperation::AddAsset {
                asset: original.clone(),
            })
            .unwrap();
        let mut replacement = original;
        replacement.checksum = "f".repeat(40);

        project
            .apply(&RecordingOperation::ReplaceAsset {
                asset: replacement.clone(),
            })
            .unwrap();

        assert_eq!(project.assets().len(), 1);
        assert_eq!(project.asset(replacement.id), Some(&replacement));
    }

    #[test]
    fn arm_is_exclusive_and_solo_plus_mute_define_audibility() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let one = AudioTrackId::new(1);
        let two = AudioTrackId::new(2);
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(one, "One"),
                    },
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(two, "Two"),
                    },
                ],
            })
            .unwrap();
        project
            .apply(&RecordingOperation::ArmTrack {
                track_id: Some(one),
            })
            .unwrap();
        project
            .apply(&RecordingOperation::ArmTrack {
                track_id: Some(two),
            })
            .unwrap();
        assert!(!project.track(one).unwrap().armed);
        assert_eq!(project.armed_track_id(), Some(two));

        project
            .apply(&RecordingOperation::SetTrackSolo {
                track_id: one,
                solo: true,
            })
            .unwrap();
        assert!(project.is_track_audible(one).unwrap());
        assert!(!project.is_track_audible(two).unwrap());
        project
            .apply(&RecordingOperation::SetTrackMuted {
                track_id: one,
                muted: true,
            })
            .unwrap();
        assert!(!project.is_track_audible(one).unwrap());
    }

    #[test]
    fn select_move_cut_and_delete_are_transactional() {
        let (mut project, track_id, clip_id) = project_with_clip();
        let mut editor = RecordingEditor::default();
        let mut log = TransactionLog::default();
        editor.select_clip(&project, clip_id, false).unwrap();

        assert_eq!(
            editor
                .move_selection(&mut project, &mut log, -20, None)
                .unwrap(),
            -10
        );
        assert_eq!(project.clip(clip_id).unwrap().start_frame, 0);

        editor.tool = RecordingTool::Cut;
        let right = editor
            .cut_clip(&mut project, &mut log, clip_id, 24)
            .unwrap();
        assert_eq!(project.clip(clip_id).unwrap().duration_frames, 24);
        assert_eq!(project.clip(right).unwrap().source_start_frame, 24);
        assert_eq!(project.clip(right).unwrap().track_id, track_id);
        assert_eq!(editor.delete_selection(&mut project, &mut log).unwrap(), 2);
        assert_eq!(project.clips().len(), 0);
        assert_eq!(log.entries().len(), 3);
        log.verify_integrity().unwrap();
    }

    #[test]
    fn invalid_batch_is_atomic() {
        let (mut project, _track_id, clip_id) = project_with_clip();
        let before = project.clone();
        let result = project.apply(&RecordingOperation::Batch {
            operations: vec![
                RecordingOperation::DeleteClips {
                    clip_ids: vec![clip_id],
                },
                RecordingOperation::DeleteClips {
                    clip_ids: vec![AudioClipId::new(999)],
                },
            ],
        });
        assert!(matches!(result, Err(RecordingError::MissingClip(_))));
        assert_eq!(project, before);
    }

    #[test]
    fn rejected_cut_does_not_consume_an_id_or_mutate_the_log() {
        let (mut project, _track_id, clip_id) = project_with_clip();
        let before = project.clone();
        let mut editor = RecordingEditor::default();
        let mut log = TransactionLog::default();
        let result = editor.cut_clip(&mut project, &mut log, clip_id, 10);
        assert!(matches!(result, Err(RecordingError::InvalidClip(_, _))));
        assert_eq!(project, before);
        assert!(log.entries().is_empty());
    }

    #[test]
    fn deserialization_rejects_non_portable_audio_asset_names() {
        let (project, _track_id, _clip_id) = project_with_clip();
        let mut serialized = serde_json::to_value(project).unwrap();
        let asset = serialized["assets"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        asset["file_name"] = serde_json::json!("../outside.flac");
        assert!(serde_json::from_value::<RecordingProject>(serialized).is_err());
    }

    #[test]
    fn log_verifies_then_replays_only_after_cursor() {
        let mut source = RecordingProject::new(24.0).unwrap();
        let base = source.clone();
        let mut log = TransactionLog::default();
        log.append_and_apply(
            &mut source,
            RecordingOperation::AddTrack {
                track: AudioTrack::new(AudioTrackId::new(10), "ADR"),
            },
        )
        .unwrap();
        log.append_and_apply(
            &mut source,
            RecordingOperation::AddAsset { asset: asset(11) },
        )
        .unwrap();
        assert_eq!(log.cursor(), 2);
        assert_eq!(log.entry_by_sequence(1).unwrap().sequence, 1);
        assert_eq!(log.entries_after(1).unwrap().len(), 1);
        let wire = serde_json::to_value(&log.entries()[0]).unwrap();
        assert_eq!(
            wire["previous_integrity"].as_str(),
            Some("0000000000000000")
        );
        assert_eq!(wire["integrity"].as_str().unwrap().len(), 16);
        assert_eq!(
            serde_json::from_value::<RecordingTransaction>(wire).unwrap(),
            log.entries()[0]
        );

        log.set_cursor(0).unwrap();
        let rebuilt = log.rebuild_from_base(&base).unwrap();
        assert_eq!(rebuilt.tracks().len(), 0);
        let mut replayed = base;
        assert_eq!(log.replay_remaining(&mut replayed).unwrap(), 2);
        assert_eq!(replayed, source);
    }

    #[test]
    fn corrupted_transaction_is_rejected_before_replay() {
        let mut source = RecordingProject::new(24.0).unwrap();
        let mut log = TransactionLog::default();
        log.append_and_apply(
            &mut source,
            RecordingOperation::AddTrack {
                track: AudioTrack::new(AudioTrackId::new(1), "ADR"),
            },
        )
        .unwrap();
        log.entries[0].integrity ^= 1;
        log.set_cursor(0).unwrap();
        let mut target = RecordingProject::new(24.0).unwrap();
        let before = target.clone();
        assert!(matches!(
            log.replay_remaining(&mut target),
            Err(RecordingError::InvalidTransactionChain { .. })
        ));
        assert_eq!(target, before);
    }

    #[test]
    fn corrupted_transaction_is_rejected_during_deserialization() {
        let mut source = RecordingProject::new(24.0).unwrap();
        let mut log = TransactionLog::default();
        log.append_and_apply(
            &mut source,
            RecordingOperation::AddTrack {
                track: AudioTrack::new(AudioTrackId::new(1), "ADR"),
            },
        )
        .unwrap();
        let mut serialized = serde_json::to_value(log).unwrap();
        serialized["entries"][0]["integrity"] = serde_json::json!(0);
        assert!(serde_json::from_value::<TransactionLog>(serialized).is_err());
    }

    #[test]
    fn received_transaction_applies_only_on_the_exact_chain_tip() {
        let mut authority_project = RecordingProject::new(24.0).unwrap();
        let mut authority_log = TransactionLog::default();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(AudioTrackId::new(1), "One"),
                },
            )
            .unwrap();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(AudioTrackId::new(2), "Two"),
                },
            )
            .unwrap();

        let first = authority_log.entries()[0].clone();
        let second = authority_log.entries()[1].clone();
        let mut replica_project = RecordingProject::new(24.0).unwrap();
        let mut replica_log = TransactionLog::default();
        replica_log
            .append_received_and_apply(&mut replica_project, first)
            .unwrap();
        replica_log
            .append_received_and_apply(&mut replica_project, second)
            .unwrap();
        assert_eq!(replica_project, authority_project);
        assert_eq!(replica_log, authority_log);
    }

    #[test]
    fn connected_replica_receives_mute_and_solo_transactions() {
        let mut authority_project = RecordingProject::new(24.0).unwrap();
        let mut authority_log = TransactionLog::default();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(AudioTrackId::new(1), "Voice"),
                },
            )
            .unwrap();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::SetTrackMuted {
                    track_id: AudioTrackId::new(1),
                    muted: true,
                },
            )
            .unwrap();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::SetTrackSolo {
                    track_id: AudioTrackId::new(1),
                    solo: true,
                },
            )
            .unwrap();

        let mut replica_project = RecordingProject::new(24.0).unwrap();
        let mut replica_log = TransactionLog::default();
        for transaction in authority_log.entries() {
            replica_log
                .append_received_and_apply(&mut replica_project, transaction.clone())
                .unwrap();
        }

        assert_eq!(replica_project, authority_project);
        assert!(replica_project.track(AudioTrackId::new(1)).unwrap().muted);
        assert!(replica_project.track(AudioTrackId::new(1)).unwrap().solo);
    }

    #[test]
    fn received_transaction_rejects_tampering_and_out_of_order_delivery() {
        let mut authority_project = RecordingProject::new(24.0).unwrap();
        let mut authority_log = TransactionLog::default();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(AudioTrackId::new(1), "One"),
                },
            )
            .unwrap();
        authority_log
            .append_and_apply(
                &mut authority_project,
                RecordingOperation::AddTrack {
                    track: AudioTrack::new(AudioTrackId::new(2), "Two"),
                },
            )
            .unwrap();

        let mut replica_project = RecordingProject::new(24.0).unwrap();
        let mut replica_log = TransactionLog::default();
        let before = replica_project.clone();
        let out_of_order = authority_log.entries()[1].clone();
        assert!(matches!(
            replica_log.append_received_and_apply(&mut replica_project, out_of_order),
            Err(RecordingError::InvalidTransactionSequence {
                expected: 0,
                actual: 1
            })
        ));
        assert_eq!(replica_project, before);
        assert!(replica_log.entries().is_empty());

        let mut tampered = authority_log.entries()[0].clone();
        tampered.operation = RecordingOperation::RenameTrack {
            track_id: AudioTrackId::new(99),
            name: "Tampered".into(),
        };
        assert!(matches!(
            replica_log.append_received_and_apply(&mut replica_project, tampered),
            Err(RecordingError::InvalidTransactionChain { sequence: 0 })
        ));
        assert_eq!(replica_project, before);
        assert!(replica_log.entries().is_empty());
    }

    #[test]
    fn validly_hashed_but_inapplicable_received_operation_is_atomic() {
        let operation = RecordingOperation::AddClip {
            clip: AudioClip {
                id: AudioClipId::new(3),
                asset_id: AudioAssetId::new(2),
                track_id: AudioTrackId::new(1),
                start_frame: 0,
                source_start_frame: 0,
                duration_frames: 24,
            },
        };
        let transaction = RecordingTransaction {
            sequence: 0,
            previous_integrity: 0,
            integrity: transaction_integrity(0, 0, &operation).unwrap(),
            operation,
        };
        let mut project = RecordingProject::new(24.0).unwrap();
        let before = project.clone();
        let mut log = TransactionLog::default();
        assert!(matches!(
            log.append_received_and_apply(&mut project, transaction),
            Err(RecordingError::MissingAsset(_))
        ));
        assert_eq!(project, before);
        assert!(log.entries().is_empty());
    }

    #[derive(Clone)]
    struct FakeClock(Rc<Cell<Duration>>);

    impl FakeClock {
        fn new() -> Self {
            Self(Rc::new(Cell::new(Duration::ZERO)))
        }

        fn advance(&self, duration: Duration) {
            self.0.set(self.0.get().saturating_add(duration));
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }

    #[derive(Default)]
    struct FakeRecorder {
        recording: bool,
        starts: usize,
        stops: usize,
        fail_start: bool,
    }

    impl AudioRecorder for FakeRecorder {
        fn start(&mut self) -> Result<(), RecordingError> {
            self.starts += 1;
            if self.fail_start {
                return Err(RecordingError::Recorder("micro unavailable".into()));
            }
            self.recording = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<RecordedAudio, RecordingError> {
            self.stops += 1;
            self.recording = false;
            Ok(RecordedAudio {
                file_name: "take.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: waveform(),
            })
        }

        fn is_recording(&self) -> bool {
            self.recording
        }

        fn live_waveform(&self) -> WaveformData {
            waveform()
        }
    }

    fn capture_target() -> CaptureTarget {
        CaptureTarget {
            track_id: AudioTrackId::new(1),
            asset_id: AudioAssetId::new(2),
            clip_id: AudioClipId::new(3),
            start_frame: 120,
        }
    }

    #[test]
    fn capture_target_proposal_is_non_mutating_until_completion() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = AudioTrackId::new(7);
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(track_id, "Voice"),
                    },
                    RecordingOperation::ArmTrack {
                        track_id: Some(track_id),
                    },
                ],
            })
            .unwrap();
        let before = project.clone();
        let target = project.propose_capture_target(track_id, 96).unwrap();
        assert_eq!(project, before);
        assert_eq!(
            project.propose_capture_target(track_id, 96).unwrap(),
            target
        );

        let completed = CompletedCapture {
            target,
            audio: RecordedAudio {
                file_name: "take.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "b".repeat(40),
                waveform: waveform(),
            },
        };
        project
            .apply(&completed.into_project_operation(project.timeline_fps()))
            .unwrap();
        assert!(project.asset(target.asset_id).is_some());
        assert!(project.clip(target.clip_id).is_some());
    }

    #[test]
    fn capture_transfer_descriptors_round_trip_through_json() {
        let target = capture_target();
        let audio = RecordedAudio {
            file_name: "take.flac".into(),
            sample_rate: 48_000,
            channels: 2,
            sample_count: 96_000,
            checksum: "c".repeat(40),
            waveform: waveform(),
        };
        let serialized = serde_json::to_vec(&(target, &audio)).unwrap();
        let restored: (CaptureTarget, RecordedAudio) = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, (target, audio));
    }

    #[test]
    fn capture_waits_three_seconds_then_finalizes_on_escape() {
        let clock = FakeClock::new();
        let mut controller = CaptureController::new(FakeRecorder::default(), clock.clone());
        assert_eq!(
            controller.begin_countdown(capture_target()).unwrap(),
            CaptureEvent::CountdownStarted
        );
        assert_eq!(controller.countdown_seconds_remaining(), Some(3));
        clock.advance(Duration::from_millis(2_999));
        assert_eq!(controller.tick(), CaptureEvent::None);
        assert_eq!(controller.recorder().starts, 0);
        clock.advance(Duration::from_millis(1));
        assert_eq!(
            controller.tick(),
            CaptureEvent::CaptureStarted {
                target: capture_target()
            }
        );
        assert!(matches!(controller.state(), CaptureState::Capturing { .. }));

        assert_eq!(
            controller.cancel_or_stop().unwrap(),
            CaptureEvent::Finalizing {
                target: capture_target()
            }
        );
        assert_eq!(controller.recorder().stops, 0);
        let CaptureEvent::Finished(completed) = controller.tick() else {
            panic!("capture must finish")
        };
        assert_eq!(completed.target.start_frame, 120);
        assert_eq!(controller.recorder().stops, 1);
        assert_eq!(controller.state(), &CaptureState::Idle);
    }

    #[test]
    fn countdown_can_cancel_without_starting_microphone() {
        let clock = FakeClock::new();
        let mut controller = CaptureController::new(FakeRecorder::default(), clock);
        controller.begin_countdown(capture_target()).unwrap();
        assert_eq!(
            controller.cancel_or_stop().unwrap(),
            CaptureEvent::Cancelled
        );
        assert_eq!(controller.recorder().starts, 0);
        assert_eq!(controller.state(), &CaptureState::Idle);
    }

    #[test]
    fn recorder_start_failure_enters_acknowledgeable_error_state() {
        let clock = FakeClock::new();
        let recorder = FakeRecorder {
            fail_start: true,
            ..FakeRecorder::default()
        };
        let mut controller = CaptureController::new(recorder, clock.clone());
        controller.begin_countdown(capture_target()).unwrap();
        clock.advance(RECORDING_COUNTDOWN);
        assert!(matches!(controller.tick(), CaptureEvent::Failed { .. }));
        assert!(matches!(controller.state(), CaptureState::Error { .. }));
        assert!(controller.acknowledge_error());
        assert_eq!(controller.state(), &CaptureState::Idle);
    }
}
