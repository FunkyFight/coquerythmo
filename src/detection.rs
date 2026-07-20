//! Track-scoped source detection and line-scoped text synchronization.
//!
//! Professional detection signs belong to a rythmo track and an absolute source
//! media position. They do not require a dialogue line. Text synchronization is
//! represented by an internal cue attached to a dialogue grapheme, so it shares
//! the same reversible command and persistence path without appearing in the
//! detector palette.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Number of semantic timing units in one video frame.
pub const MEDIA_TICKS_PER_FRAME: i64 = 10;

/// Synthetic line ids reserve one storage bucket per rythmo track without
/// creating fake dialogue lines in the project.
const TRACK_STORAGE_BASE: u64 = u64::MAX - 255;

pub const fn track_storage_line_id(track: u8) -> u64 {
    TRACK_STORAGE_BASE + track as u64
}

pub const fn track_from_storage_line_id(line_id: u64) -> Option<u8> {
    if line_id >= TRACK_STORAGE_BASE {
        let offset = line_id - TRACK_STORAGE_BASE;
        if offset <= u8::MAX as u64 {
            return Some(offset as u8);
        }
    }
    None
}

pub const fn is_track_storage_line_id(line_id: u64) -> bool {
    track_from_storage_line_id(line_id).is_some()
}

/// A source-media position with tenth-frame precision.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MediaTick(pub i64);

impl MediaTick {
    pub const ZERO: Self = Self(0);

    pub const fn from_frame(frame: i64) -> Self {
        Self(frame.saturating_mul(MEDIA_TICKS_PER_FRAME))
    }

    pub fn from_frame_position(frame: f64) -> Self {
        if !frame.is_finite() {
            return Self::ZERO;
        }
        Self((frame * MEDIA_TICKS_PER_FRAME as f64).round() as i64)
    }

    pub fn from_seconds(seconds: f64, fps: f64) -> Self {
        if !seconds.is_finite() || !fps.is_finite() || fps <= 0.0 {
            return Self::ZERO;
        }
        Self::from_frame_position(seconds * fps)
    }

    pub const fn raw(self) -> i64 {
        self.0
    }

    pub fn as_frame_position(self) -> f64 {
        self.0 as f64 / MEDIA_TICKS_PER_FRAME as f64
    }

    pub fn scaled(self, ratio: f64) -> Self {
        if !ratio.is_finite() || ratio <= 0.0 {
            return self;
        }
        Self((self.0 as f64 * ratio).round() as i64)
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn clamp(self, minimum: Self, maximum: Self) -> Self {
        if self.0 < minimum.0 {
            minimum
        } else if self.0 > maximum.0 {
            maximum
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DetectionCueId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyncPointId(pub u64);

/// Stable address used by selection, drag, history and persistence.
///
/// Source signs use a synthetic track storage id. Synchronization points use
/// the actual dialogue line id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DetectionAddress {
    pub line_id: u64,
    pub detection_id: DetectionCueId,
}

impl DetectionAddress {
    pub const fn for_track(track: u8, detection_id: DetectionCueId) -> Self {
        Self {
            line_id: track_storage_line_id(track),
            detection_id,
        }
    }

    pub const fn track(self) -> Option<u8> {
        track_from_storage_line_id(self.line_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionFamily {
    Mouth,
    Performance,
    Synchronization,
}

/// The exact seven signs exposed by the detector palette. `TextSyncPoint` is an
/// internal visual timing handle and is deliberately excluded from `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionKind {
    Labial,
    SemiLabial,
    MouthOpen,
    MouthClosed,
    TeethVisible,
    Breath,
    Reaction,
    TextSyncPoint,
}

impl DetectionKind {
    pub const ALL: [Self; 7] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpen,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::Breath,
        Self::Reaction,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Labial => "Labiale",
            Self::SemiLabial => "Semi-labiale",
            Self::MouthOpen => "Bouche ouverte",
            Self::MouthClosed => "Bouche fermée",
            Self::TeethVisible => "Dents visibles",
            Self::Breath => "Respiration",
            Self::Reaction => "Réaction",
            Self::TextSyncPoint => "Point de synchronisation",
        }
    }

    pub const fn asset_name(self) -> &'static str {
        match self {
            Self::Labial => "detection/labial",
            Self::SemiLabial => "detection/semi_labial",
            Self::MouthOpen => "detection/mouth_open",
            Self::MouthClosed => "detection/mouth_closed",
            Self::TeethVisible => "detection/teeth_visible",
            Self::Breath => "detection/breath",
            Self::Reaction => "detection/reaction",
            Self::TextSyncPoint => "detection/sync_point",
        }
    }

    /// Kept for compatibility with older UI code; the detector overlay no
    /// longer renders these textual abbreviations.
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Labial => "L",
            Self::SemiLabial => "SL",
            Self::MouthOpen => "BO",
            Self::MouthClosed => "BF",
            Self::TeethVisible => "DV",
            Self::Breath => "R",
            Self::Reaction => "!",
            Self::TextSyncPoint => "•",
        }
    }

    pub const fn family(self) -> DetectionFamily {
        match self {
            Self::Labial
            | Self::SemiLabial
            | Self::MouthOpen
            | Self::MouthClosed
            | Self::TeethVisible => DetectionFamily::Mouth,
            Self::Breath | Self::Reaction => DetectionFamily::Performance,
            Self::TextSyncPoint => DetectionFamily::Synchronization,
        }
    }

    pub const fn is_sync_point(self) -> bool {
        matches!(self, Self::TextSyncPoint)
    }
}

/// A semantic attachment to the adapted text. Grapheme indices are stored
/// instead of UTF-8 byte offsets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextAnchor {
    BeforeText,
    AfterText,
    Grapheme { index: u32 },
    /// End-exclusive range.
    GraphemeRange { start: u32, end: u32 },
}

impl TextAnchor {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::GraphemeRange { start, end } = self {
            if start >= end {
                return Err(format!(
                    "invalid grapheme range {start}..{end}: end must be greater than start"
                ));
            }
        }
        Ok(())
    }

    pub const fn grapheme_index(&self) -> Option<u32> {
        match self {
            Self::Grapheme { index } => Some(*index),
            Self::GraphemeRange { start, .. } => Some(*start),
            Self::BeforeText | Self::AfterText => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionCue {
    pub id: DetectionCueId,
    pub kind: DetectionKind,
    /// Absolute position in the source media.
    pub media_tick: MediaTick,
    pub target: TextAnchor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSyncPoint {
    pub id: SyncPointId,
    pub grapheme_boundary: u32,
    pub line_tick: MediaTick,
}

/// Semantic data owned by either a real dialogue line or a synthetic track
/// bucket.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineDetectionData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub original_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    detections: Vec<DetectionCue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sync_points: Vec<TextSyncPoint>,
}

impl LineDetectionData {
    pub fn is_empty(&self) -> bool {
        self.original_text.is_empty() && self.detections.is_empty() && self.sync_points.is_empty()
    }

    pub fn detections(&self) -> &[DetectionCue] {
        &self.detections
    }

    pub fn source_detections(&self) -> impl Iterator<Item = &DetectionCue> {
        self.detections.iter().filter(|cue| !cue.kind.is_sync_point())
    }

    pub fn text_sync_cues(&self) -> impl Iterator<Item = &DetectionCue> {
        self.detections.iter().filter(|cue| cue.kind.is_sync_point())
    }

    pub fn sync_points(&self) -> &[TextSyncPoint] {
        &self.sync_points
    }

    pub fn detection(&self, id: DetectionCueId) -> Option<&DetectionCue> {
        self.detections.iter().find(|cue| cue.id == id)
    }

    pub fn next_detection_id(&self) -> Option<DetectionCueId> {
        self.detections
            .iter()
            .map(|cue| cue.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(DetectionCueId)
    }

    pub fn insert_detection(&mut self, cue: DetectionCue) -> bool {
        if self.detection(cue.id).is_some() || cue.target.validate().is_err() {
            return false;
        }
        self.detections.push(cue);
        self.sort_detections();
        true
    }

    pub fn remove_detection(&mut self, id: DetectionCueId) -> Option<DetectionCue> {
        let index = self.detections.iter().position(|cue| cue.id == id)?;
        Some(self.detections.remove(index))
    }

    pub fn move_detection(&mut self, id: DetectionCueId, media_tick: MediaTick) -> bool {
        let Some(cue) = self.detections.iter_mut().find(|cue| cue.id == id) else {
            return false;
        };
        if cue.media_tick == media_tick {
            return false;
        }
        cue.media_tick = media_tick;
        self.sort_detections();
        true
    }

    pub fn detection_before(&self, tick: MediaTick) -> Option<&DetectionCue> {
        self.detections
            .iter()
            .rev()
            .find(|cue| !cue.kind.is_sync_point() && cue.media_tick < tick)
    }

    pub fn detection_after(&self, tick: MediaTick) -> Option<&DetectionCue> {
        self.detections
            .iter()
            .find(|cue| !cue.kind.is_sync_point() && cue.media_tick > tick)
    }

    pub fn scaled_time(&self, ratio: f64) -> Self {
        let mut scaled = self.clone();
        for cue in &mut scaled.detections {
            cue.media_tick = cue.media_tick.scaled(ratio);
        }
        for point in &mut scaled.sync_points {
            point.line_tick = point.line_tick.scaled(ratio);
        }
        scaled
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut detection_ids = HashSet::new();
        for cue in &self.detections {
            if !detection_ids.insert(cue.id) {
                return Err(format!("duplicate detection id {}", cue.id.0));
            }
            cue.target.validate()?;
        }

        let mut sync_ids = HashSet::new();
        for point in &self.sync_points {
            if !sync_ids.insert(point.id) {
                return Err(format!("duplicate sync point id {}", point.id.0));
            }
        }
        Ok(())
    }

    fn sort_detections(&mut self) {
        self.detections.sort_by_key(|cue| (cue.media_tick, cue.id));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionDocument {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    lines: BTreeMap<u64, LineDetectionData>,
}

impl DetectionDocument {
    pub fn is_empty(&self) -> bool {
        self.lines.values().all(LineDetectionData::is_empty)
    }

    pub fn line(&self, line_id: u64) -> Option<&LineDetectionData> {
        self.lines.get(&line_id)
    }

    pub fn track(&self, track: u8) -> Option<&LineDetectionData> {
        self.line(track_storage_line_id(track))
    }

    pub fn line_mut(&mut self, line_id: u64) -> &mut LineDetectionData {
        self.lines.entry(line_id).or_default()
    }

    pub fn remove_line(&mut self, line_id: u64) -> Option<LineDetectionData> {
        self.lines.remove(&line_id)
    }

    pub fn restore_line(&mut self, line_id: u64, data: LineDetectionData) {
        if data.is_empty() {
            self.lines.remove(&line_id);
        } else {
            self.lines.insert(line_id, data);
        }
    }

    /// Removing dialogue lines must never prune synthetic source-track buckets.
    pub fn retain_lines(&mut self, mut keep: impl FnMut(u64) -> bool) {
        self.lines
            .retain(|line_id, _| is_track_storage_line_id(*line_id) || keep(*line_id));
    }

    pub fn add_detection(
        &mut self,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) -> Option<DetectionAddress> {
        target.validate().ok()?;
        let line = self.line_mut(line_id);
        let detection_id = line.next_detection_id()?;
        let inserted = line.insert_detection(DetectionCue {
            id: detection_id,
            kind,
            media_tick,
            target,
        });
        inserted.then_some(DetectionAddress {
            line_id,
            detection_id,
        })
    }

    pub fn insert_detection(&mut self, address: DetectionAddress, cue: DetectionCue) -> bool {
        if address.detection_id != cue.id {
            return false;
        }
        self.line_mut(address.line_id).insert_detection(cue)
    }

    pub fn detection(&self, address: DetectionAddress) -> Option<&DetectionCue> {
        self.line(address.line_id)?.detection(address.detection_id)
    }

    pub fn remove_detection(&mut self, address: DetectionAddress) -> Option<DetectionCue> {
        let cue = self
            .lines
            .get_mut(&address.line_id)?
            .remove_detection(address.detection_id)?;
        self.prune_empty_line(address.line_id);
        Some(cue)
    }

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) -> bool {
        self.lines
            .get_mut(&address.line_id)
            .is_some_and(|line| line.move_detection(address.detection_id, media_tick))
    }

    pub fn audition_window(
        &self,
        address: DetectionAddress,
        fps: f64,
    ) -> Option<(MediaTick, MediaTick)> {
        let cue = self.detection(address)?;
        if cue.kind.is_sync_point() {
            return None;
        }
        let radius = MediaTick::from_seconds(2.0, fps);
        Some((
            cue.media_tick.saturating_sub(radius).clamp(MediaTick::ZERO, cue.media_tick),
            cue.media_tick.saturating_add(radius),
        ))
    }

    pub fn scaled_time(&self, ratio: f64) -> Self {
        let mut scaled = Self::default();
        for (line_id, line) in &self.lines {
            scaled.lines.insert(*line_id, line.scaled_time(ratio));
        }
        scaled
    }

    pub fn validate(&self) -> Result<(), String> {
        for (line_id, line) in &self.lines {
            line.validate()
                .map_err(|error| format!("line {line_id}: {error}"))?;
            if is_track_storage_line_id(*line_id)
                && line.detections().iter().any(|cue| cue.kind.is_sync_point())
            {
                return Err(format!("track bucket {line_id} contains a text sync point"));
            }
        }
        Ok(())
    }

    /// Applies explicit letter synchronization points to existing syllable
    /// ratios. Points are piecewise-linear control points; segment count and
    /// normalization are preserved.
    pub fn warped_ratios(
        &self,
        line_id: u64,
        text: &str,
        breaks: &[usize],
        base_ratios: &[f32],
        line_start_frame: i64,
        duration_frames: i64,
    ) -> Vec<f32> {
        if text.is_empty()
            || duration_frames <= 0
            || base_ratios.is_empty()
            || base_ratios.len() != breaks.len() + 1
        {
            return base_ratios.to_vec();
        }
        let Some(data) = self.line(line_id) else {
            return base_ratios.to_vec();
        };
        let mut points = data
            .text_sync_cues()
            .filter_map(|cue| Some((cue.target.grapheme_index()? as usize, cue.media_tick)))
            .collect::<Vec<_>>();
        if points.is_empty() {
            return base_ratios.to_vec();
        }

        let char_count = text.chars().count();
        if char_count == 0 {
            return base_ratios.to_vec();
        }
        points.sort_by_key(|(index, tick)| (*index, *tick));

        let mut normalized = base_ratios.to_vec();
        normalize_positive(&mut normalized);
        let line_start = MediaTick::from_frame(line_start_frame);
        let duration_ticks = MediaTick::from_frame(duration_frames).raw().max(1) as f32;
        let mut controls = vec![(0.0_f32, 0.0_f32)];
        for (character_index, tick) in points {
            if character_index >= char_count {
                continue;
            }
            let source = source_ratio_for_char_boundary(
                character_index,
                char_count,
                breaks,
                &normalized,
            );
            let relative = tick.raw().saturating_sub(line_start.raw());
            let target = (relative as f32 / duration_ticks).clamp(0.0, 1.0);
            controls.push((source, target));
        }
        controls.push((1.0, 1.0));
        controls.sort_by(|a, b| a.0.total_cmp(&b.0));
        controls.dedup_by(|a, b| (a.0 - b.0).abs() < 0.000_01);
        enforce_monotonic_controls(&mut controls);

        let mut source_boundaries = Vec::with_capacity(normalized.len() + 1);
        source_boundaries.push(0.0_f32);
        let mut cumulative = 0.0_f32;
        for ratio in &normalized {
            cumulative += *ratio;
            source_boundaries.push(cumulative.clamp(0.0, 1.0));
        }
        if let Some(last) = source_boundaries.last_mut() {
            *last = 1.0;
        }
        let mapped = source_boundaries
            .into_iter()
            .map(|source| piecewise_map(source, &controls))
            .collect::<Vec<_>>();
        let mut warped = mapped
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).max(0.000_1))
            .collect::<Vec<_>>();
        normalize_positive(&mut warped);
        warped
    }

    fn prune_empty_line(&mut self, line_id: u64) {
        if self
            .lines
            .get(&line_id)
            .is_some_and(LineDetectionData::is_empty)
        {
            self.lines.remove(&line_id);
        }
    }
}

fn normalize_positive(values: &mut [f32]) {
    for value in values.iter_mut() {
        if !value.is_finite() || *value <= 0.0 {
            *value = 0.000_1;
        }
    }
    let sum: f32 = values.iter().sum();
    if sum > f32::EPSILON {
        for value in values {
            *value /= sum;
        }
    }
}

fn source_ratio_for_char_boundary(
    boundary: usize,
    char_count: usize,
    breaks: &[usize],
    ratios: &[f32],
) -> f32 {
    let mut char_start = 0usize;
    let mut time_start = 0.0_f32;
    for (index, ratio) in ratios.iter().copied().enumerate() {
        let char_end = breaks.get(index).copied().unwrap_or(char_count).min(char_count);
        if boundary <= char_end {
            let span = char_end.saturating_sub(char_start);
            let local = if span == 0 {
                0.0
            } else {
                boundary.saturating_sub(char_start) as f32 / span as f32
            };
            return (time_start + ratio * local).clamp(0.0, 1.0);
        }
        char_start = char_end;
        time_start += ratio;
    }
    1.0
}

fn enforce_monotonic_controls(controls: &mut [(f32, f32)]) {
    if controls.len() <= 2 {
        return;
    }
    let count = controls.len();
    let epsilon = 0.000_1_f32;
    controls[0] = (0.0, 0.0);
    controls[count - 1] = (1.0, 1.0);
    let mut previous = 0.0;
    for index in 1..count - 1 {
        let maximum = 1.0 - epsilon * (count - 1 - index) as f32;
        controls[index].1 = controls[index].1.clamp(previous + epsilon, maximum);
        previous = controls[index].1;
    }
}

fn piecewise_map(source: f32, controls: &[(f32, f32)]) -> f32 {
    let source = source.clamp(0.0, 1.0);
    for pair in controls.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if source <= x1 || (x1 - 1.0).abs() < f32::EPSILON {
            let width = (x1 - x0).max(0.000_1);
            let local = ((source - x0) / width).clamp(0.0, 1.0);
            return y0 + (y1 - y0) * local;
        }
    }
    source
}

/// Reversible semantic operation wrapped by the application's canonical
/// command/history boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetectionChange {
    Add {
        address: DetectionAddress,
        cue: DetectionCue,
    },
    Remove {
        address: DetectionAddress,
        cue: DetectionCue,
    },
    Move {
        address: DetectionAddress,
        old_tick: MediaTick,
        new_tick: MediaTick,
    },
    RemoveLine {
        line_id: u64,
        data: LineDetectionData,
    },
}

impl DetectionChange {
    pub fn apply(&self, document: &mut DetectionDocument) -> bool {
        match self {
            Self::Add { address, cue } => document.insert_detection(*address, cue.clone()),
            Self::Remove { address, .. } => document.remove_detection(*address).is_some(),
            Self::Move {
                address, new_tick, ..
            } => document.move_detection(*address, *new_tick),
            Self::RemoveLine { line_id, .. } => document.remove_line(*line_id).is_some(),
        }
    }

    pub fn unapply(&self, document: &mut DetectionDocument) -> bool {
        match self {
            Self::Add { address, .. } => document.remove_detection(*address).is_some(),
            Self::Remove { address, cue } => document.insert_detection(*address, cue.clone()),
            Self::Move {
                address, old_tick, ..
            } => document.move_detection(*address, *old_tick),
            Self::RemoveLine { line_id, data } => {
                document.restore_line(*line_id, data.clone());
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_contains_only_the_requested_seven_signs() {
        assert_eq!(DetectionKind::ALL.len(), 7);
        assert!(!DetectionKind::ALL.contains(&DetectionKind::TextSyncPoint));
    }

    #[test]
    fn media_ticks_round_to_tenths_of_a_frame() {
        assert_eq!(MediaTick::from_frame_position(12.34), MediaTick(123));
        assert_eq!(MediaTick::from_frame_position(12.36), MediaTick(124));
        assert_eq!(MediaTick(127).as_frame_position(), 12.7);
    }

    #[test]
    fn source_signs_exist_on_tracks_without_dialogue_lines() {
        let mut document = DetectionDocument::default();
        let upper = document
            .add_detection(
                track_storage_line_id(0),
                DetectionKind::Labial,
                MediaTick(123),
                TextAnchor::BeforeText,
            )
            .unwrap();
        let lower = document
            .add_detection(
                track_storage_line_id(2),
                DetectionKind::MouthOpen,
                MediaTick(123),
                TextAnchor::BeforeText,
            )
            .unwrap();
        assert_eq!(upper.track(), Some(0));
        assert_eq!(lower.track(), Some(2));
        assert_eq!(document.track(0).unwrap().source_detections().count(), 1);
        assert_eq!(document.track(2).unwrap().source_detections().count(), 1);
    }

    #[test]
    fn text_sync_cues_are_line_scoped_and_not_palette_signs() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_detection(
                42,
                DetectionKind::TextSyncPoint,
                MediaTick(250),
                TextAnchor::Grapheme { index: 3 },
            )
            .unwrap();
        assert_eq!(document.line(42).unwrap().text_sync_cues().count(), 1);
        assert!(document.audition_window(address, 25.0).is_none());
    }

    #[test]
    fn changes_apply_and_unapply() {
        let mut document = DetectionDocument::default();
        let address = DetectionAddress::for_track(1, DetectionCueId(1));
        let cue = DetectionCue {
            id: address.detection_id,
            kind: DetectionKind::Reaction,
            media_tick: MediaTick(75),
            target: TextAnchor::BeforeText,
        };
        let change = DetectionChange::Add {
            address,
            cue: cue.clone(),
        };
        assert!(change.apply(&mut document));
        assert_eq!(document.detection(address), Some(&cue));
        assert!(change.unapply(&mut document));
        assert!(document.detection(address).is_none());
    }

    #[test]
    fn audition_window_is_two_seconds_on_each_side() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_detection(
                track_storage_line_id(0),
                DetectionKind::Breath,
                MediaTick::from_frame(100),
                TextAnchor::BeforeText,
            )
            .unwrap();
        let (start, end) = document.audition_window(address, 25.0).unwrap();
        assert_eq!(start, MediaTick::from_frame(50));
        assert_eq!(end, MediaTick::from_frame(150));
    }

    #[test]
    fn serde_roundtrip_preserves_track_ownership() {
        let mut document = DetectionDocument::default();
        document
            .add_detection(
                track_storage_line_id(3),
                DetectionKind::TeethVisible,
                MediaTick(987),
                TextAnchor::BeforeText,
            )
            .unwrap();
        let json = serde_json::to_string(&document).unwrap();
        let restored: DetectionDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, document);
        assert_eq!(restored.track(3).unwrap().source_detections().count(), 1);
    }
}