from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8-sig")


def write(path: str, content: str) -> None:
    (ROOT / path).write_text(content, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise RuntimeError(f"missing expected text in {path}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))


def replace_regex(path: str, pattern: str, replacement: str, count: int = 1) -> None:
    text = read(path)
    updated, replaced = re.subn(pattern, replacement, text, count=count, flags=re.S)
    if replaced != count:
        raise RuntimeError(f"expected {count} regex replacements in {path}, got {replaced}: {pattern[:120]}")
    write(path, updated)


DETECTION_RS = r'''//! Track-scoped source detection and line-scoped text synchronization.
//!
//! Source detections belong to a stable rythmo track and an absolute media
//! position. They intentionally do not require a dialogue line: detection is
//! authored before adaptation. Text synchronization points are a separate,
//! language-specific layer attached to a dialogue line.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

pub const MEDIA_TICKS_PER_FRAME: i64 = 10;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DetectionAddress {
    pub track: u8,
    pub detection_id: DetectionCueId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SyncPointAddress {
    pub line_id: u64,
    pub sync_point_id: SyncPointId,
}

/// The exact professional sign set exposed by the detector palette.
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionCue {
    pub id: DetectionCueId,
    pub kind: DetectionKind,
    /// Absolute source-media position, independent from dialogue lines.
    pub media_tick: MediaTick,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackDetectionData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    detections: Vec<DetectionCue>,
}

impl TrackDetectionData {
    pub fn detections(&self) -> &[DetectionCue] {
        &self.detections
    }

    pub fn is_empty(&self) -> bool {
        self.detections.is_empty()
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

    fn sort(&mut self) {
        self.detections
            .sort_by_key(|cue| (cue.media_tick, cue.id));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSyncPoint {
    pub id: SyncPointId,
    /// Unicode grapheme boundary in the adaptation text.
    pub grapheme_boundary: u32,
    /// Position relative to the beginning of the dialogue line.
    pub line_tick: MediaTick,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSyncData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub original_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sync_points: Vec<TextSyncPoint>,
}

impl LineSyncData {
    pub fn sync_points(&self) -> &[TextSyncPoint] {
        &self.sync_points
    }

    pub fn is_empty(&self) -> bool {
        self.original_text.is_empty() && self.sync_points.is_empty()
    }

    pub fn next_sync_point_id(&self) -> Option<SyncPointId> {
        self.sync_points
            .iter()
            .map(|point| point.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(SyncPointId)
    }

    fn sort(&mut self) {
        self.sync_points
            .sort_by_key(|point| (point.grapheme_boundary, point.id));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionDocument {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tracks: BTreeMap<u8, TrackDetectionData>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    lines: BTreeMap<u64, LineSyncData>,
}

impl DetectionDocument {
    pub fn is_empty(&self) -> bool {
        self.tracks.values().all(TrackDetectionData::is_empty)
            && self.lines.values().all(LineSyncData::is_empty)
    }

    pub fn tracks(&self) -> impl Iterator<Item = (u8, &TrackDetectionData)> + '_ {
        self.tracks.iter().map(|(track, data)| (*track, data))
    }

    pub fn track(&self, track: u8) -> Option<&TrackDetectionData> {
        self.tracks.get(&track)
    }

    pub fn line_sync(&self, line_id: u64) -> Option<&LineSyncData> {
        self.lines.get(&line_id)
    }

    pub fn line_sync_mut(&mut self, line_id: u64) -> &mut LineSyncData {
        self.lines.entry(line_id).or_default()
    }

    pub fn detection(&self, address: DetectionAddress) -> Option<&DetectionCue> {
        self.track(address.track)?
            .detections
            .iter()
            .find(|cue| cue.id == address.detection_id)
    }

    pub fn sync_point(&self, address: SyncPointAddress) -> Option<&TextSyncPoint> {
        self.line_sync(address.line_id)?
            .sync_points
            .iter()
            .find(|point| point.id == address.sync_point_id)
    }

    pub fn next_detection_id(&self, track: u8) -> Option<DetectionCueId> {
        self.track(track)
            .map(TrackDetectionData::next_detection_id)
            .unwrap_or(Some(DetectionCueId(1)))
    }

    pub fn next_sync_point_id(&self, line_id: u64) -> Option<SyncPointId> {
        self.line_sync(line_id)
            .map(LineSyncData::next_sync_point_id)
            .unwrap_or(Some(SyncPointId(1)))
    }

    fn insert_detection(&mut self, address: DetectionAddress, cue: DetectionCue) -> bool {
        if cue.id != address.detection_id || self.detection(address).is_some() {
            return false;
        }
        let data = self.tracks.entry(address.track).or_default();
        data.detections.push(cue);
        data.sort();
        true
    }

    fn remove_detection(&mut self, address: DetectionAddress) -> Option<DetectionCue> {
        let data = self.tracks.get_mut(&address.track)?;
        let index = data
            .detections
            .iter()
            .position(|cue| cue.id == address.detection_id)?;
        let cue = data.detections.remove(index);
        if data.is_empty() {
            self.tracks.remove(&address.track);
        }
        Some(cue)
    }

    fn move_detection(&mut self, address: DetectionAddress, tick: MediaTick) -> bool {
        let Some(data) = self.tracks.get_mut(&address.track) else {
            return false;
        };
        let Some(cue) = data
            .detections
            .iter_mut()
            .find(|cue| cue.id == address.detection_id)
        else {
            return false;
        };
        cue.media_tick = tick;
        data.sort();
        true
    }

    fn insert_sync_point(&mut self, address: SyncPointAddress, point: TextSyncPoint) -> bool {
        if point.id != address.sync_point_id || self.sync_point(address).is_some() {
            return false;
        }
        let line = self.lines.entry(address.line_id).or_default();
        if line
            .sync_points
            .iter()
            .any(|existing| existing.grapheme_boundary == point.grapheme_boundary)
        {
            return false;
        }
        line.sync_points.push(point);
        line.sort();
        true
    }

    fn remove_sync_point(&mut self, address: SyncPointAddress) -> Option<TextSyncPoint> {
        let line = self.lines.get_mut(&address.line_id)?;
        let index = line
            .sync_points
            .iter()
            .position(|point| point.id == address.sync_point_id)?;
        let point = line.sync_points.remove(index);
        if line.is_empty() {
            self.lines.remove(&address.line_id);
        }
        Some(point)
    }

    fn move_sync_point(&mut self, address: SyncPointAddress, line_tick: MediaTick) -> bool {
        let Some(line) = self.lines.get_mut(&address.line_id) else {
            return false;
        };
        let Some(point) = line
            .sync_points
            .iter_mut()
            .find(|point| point.id == address.sync_point_id)
        else {
            return false;
        };
        point.line_tick = line_tick;
        line.sort();
        true
    }

    pub fn scale_media_positions(&mut self, ratio: f64) {
        for data in self.tracks.values_mut() {
            for cue in &mut data.detections {
                cue.media_tick = cue.media_tick.scaled(ratio);
            }
            data.sort();
        }
        for line in self.lines.values_mut() {
            for point in &mut line.sync_points {
                point.line_tick = point.line_tick.scaled(ratio);
            }
        }
    }

    pub fn audition_window(&self, address: DetectionAddress, fps: f64) -> Option<AuditionWindow> {
        let cue = self.detection(address)?;
        let margin = MediaTick::from_seconds(2.0, fps);
        Some(AuditionWindow {
            start: cue.media_tick.saturating_sub(margin).clamp(MediaTick::ZERO, cue.media_tick),
            beep: cue.media_tick,
            end: cue.media_tick.saturating_add(margin),
        })
    }

    /// Warp saved syllable ratios through explicit grapheme synchronization
    /// points. Each interval keeps its internal timing proportions.
    pub fn warped_ratios(
        &self,
        line_id: u64,
        text: &str,
        breaks: &[usize],
        base_ratios: &[f32],
        duration_frames: i64,
    ) -> Vec<f32> {
        let Some(line) = self.line_sync(line_id) else {
            return base_ratios.to_vec();
        };
        if line.sync_points.is_empty()
            || base_ratios.is_empty()
            || base_ratios.len() != breaks.len() + 1
            || duration_frames <= 0
        {
            return base_ratios.to_vec();
        }

        let char_count = text.chars().count();
        let grapheme_count = text.graphemes(true).count();
        if char_count == 0 || grapheme_count == 0 {
            return base_ratios.to_vec();
        }

        let mut normalized = base_ratios.to_vec();
        normalize_positive(&mut normalized);
        let mut controls = vec![(0.0_f32, 0.0_f32)];
        let duration_ticks = duration_frames.saturating_mul(MEDIA_TICKS_PER_FRAME).max(1) as f32;
        for point in &line.sync_points {
            if point.grapheme_boundary == 0 || point.grapheme_boundary as usize >= grapheme_count {
                continue;
            }
            let char_boundary = text
                .graphemes(true)
                .take(point.grapheme_boundary as usize)
                .map(str::chars)
                .map(Iterator::count)
                .sum::<usize>();
            let source = source_ratio_for_char_boundary(
                char_boundary,
                char_count,
                breaks,
                &normalized,
            );
            let target = (point.line_tick.raw() as f32 / duration_ticks).clamp(0.0, 1.0);
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

        let mapped: Vec<f32> = source_boundaries
            .into_iter()
            .map(|source| piecewise_map(source, &controls))
            .collect();
        let mut warped = mapped
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).max(0.000_1))
            .collect::<Vec<_>>();
        normalize_positive(&mut warped);
        warped
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditionWindow {
    pub start: MediaTick,
    pub beep: MediaTick,
    pub end: MediaTick,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum DetectionChange {
    AddDetection {
        address: DetectionAddress,
        cue: DetectionCue,
    },
    RemoveDetection {
        address: DetectionAddress,
        cue: DetectionCue,
    },
    MoveDetection {
        address: DetectionAddress,
        old_tick: MediaTick,
        new_tick: MediaTick,
    },
    AddSyncPoint {
        address: SyncPointAddress,
        point: TextSyncPoint,
    },
    RemoveSyncPoint {
        address: SyncPointAddress,
        point: TextSyncPoint,
    },
    MoveSyncPoint {
        address: SyncPointAddress,
        old_tick: MediaTick,
        new_tick: MediaTick,
    },
    RemoveLineSync {
        line_id: u64,
        data: LineSyncData,
    },
}

impl DetectionChange {
    pub fn is_source_detection(&self) -> bool {
        matches!(
            self,
            Self::AddDetection { .. }
                | Self::RemoveDetection { .. }
                | Self::MoveDetection { .. }
        )
    }

    pub fn apply(&self, document: &mut DetectionDocument) -> bool {
        match self {
            Self::AddDetection { address, cue } => {
                document.insert_detection(*address, cue.clone())
            }
            Self::RemoveDetection { address, .. } => {
                document.remove_detection(*address).is_some()
            }
            Self::MoveDetection {
                address, new_tick, ..
            } => document.move_detection(*address, *new_tick),
            Self::AddSyncPoint { address, point } => {
                document.insert_sync_point(*address, point.clone())
            }
            Self::RemoveSyncPoint { address, .. } => {
                document.remove_sync_point(*address).is_some()
            }
            Self::MoveSyncPoint {
                address, new_tick, ..
            } => document.move_sync_point(*address, *new_tick),
            Self::RemoveLineSync { line_id, .. } => document.lines.remove(line_id).is_some(),
        }
    }

    pub fn unapply(&self, document: &mut DetectionDocument) -> bool {
        match self {
            Self::AddDetection { address, .. } => {
                document.remove_detection(*address).is_some()
            }
            Self::RemoveDetection { address, cue } => {
                document.insert_detection(*address, cue.clone())
            }
            Self::MoveDetection {
                address, old_tick, ..
            } => document.move_detection(*address, *old_tick),
            Self::AddSyncPoint { address, .. } => {
                document.remove_sync_point(*address).is_some()
            }
            Self::RemoveSyncPoint { address, point } => {
                document.insert_sync_point(*address, point.clone())
            }
            Self::MoveSyncPoint {
                address, old_tick, ..
            } => document.move_sync_point(*address, *old_tick),
            Self::RemoveLineSync { line_id, data } => {
                if document.lines.contains_key(line_id) {
                    false
                } else {
                    document.lines.insert(*line_id, data.clone());
                    true
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_contains_exactly_the_seven_requested_signs() {
        assert_eq!(DetectionKind::ALL.len(), 7);
        assert_eq!(DetectionKind::ALL[0], DetectionKind::Labial);
        assert_eq!(DetectionKind::ALL[6], DetectionKind::Reaction);
    }

    #[test]
    fn simultaneous_tracks_are_independent_without_dialogue_lines() {
        let mut document = DetectionDocument::default();
        let a = DetectionAddress {
            track: 0,
            detection_id: DetectionCueId(1),
        };
        let b = DetectionAddress {
            track: 2,
            detection_id: DetectionCueId(1),
        };
        assert!(DetectionChange::AddDetection {
            address: a,
            cue: DetectionCue {
                id: a.detection_id,
                kind: DetectionKind::Labial,
                media_tick: MediaTick(423),
            },
        }
        .apply(&mut document));
        assert!(DetectionChange::AddDetection {
            address: b,
            cue: DetectionCue {
                id: b.detection_id,
                kind: DetectionKind::MouthOpen,
                media_tick: MediaTick(423),
            },
        }
        .apply(&mut document));
        assert_eq!(document.detection(a).unwrap().kind, DetectionKind::Labial);
        assert_eq!(document.detection(b).unwrap().kind, DetectionKind::MouthOpen);
    }

    #[test]
    fn sync_point_warps_text_timing_without_changing_segment_count() {
        let mut document = DetectionDocument::default();
        let address = SyncPointAddress {
            line_id: 12,
            sync_point_id: SyncPointId(1),
        };
        DetectionChange::AddSyncPoint {
            address,
            point: TextSyncPoint {
                id: address.sync_point_id,
                grapheme_boundary: 2,
                line_tick: MediaTick(75),
            },
        }
        .apply(&mut document);
        let warped = document.warped_ratios(12, "abcd", &[2], &[0.5, 0.5], 10);
        assert_eq!(warped.len(), 2);
        assert!(warped[0] > warped[1]);
        assert!((warped.iter().sum::<f32>() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn changes_are_reversible() {
        let mut document = DetectionDocument::default();
        let address = DetectionAddress {
            track: 1,
            detection_id: DetectionCueId(1),
        };
        let change = DetectionChange::AddDetection {
            address,
            cue: DetectionCue {
                id: address.detection_id,
                kind: DetectionKind::TeethVisible,
                media_tick: MediaTick(100),
            },
        };
        assert!(change.apply(&mut document));
        assert!(change.unapply(&mut document));
        assert!(document.detection(address).is_none());
    }

    #[test]
    fn audition_window_is_two_seconds_each_side() {
        let mut document = DetectionDocument::default();
        let address = DetectionAddress {
            track: 0,
            detection_id: DetectionCueId(1),
        };
        DetectionChange::AddDetection {
            address,
            cue: DetectionCue {
                id: address.detection_id,
                kind: DetectionKind::Breath,
                media_tick: MediaTick::from_frame(100),
            },
        }
        .apply(&mut document);
        let window = document.audition_window(address, 25.0).unwrap();
        assert_eq!(window.start, MediaTick::from_frame(50));
        assert_eq!(window.beep, MediaTick::from_frame(100));
        assert_eq!(window.end, MediaTick::from_frame(150));
    }
}
'''


DETECTION_SERVICE_RS = r'''//! Canonical application operations for source detections and text sync points.

use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionDocument, DetectionKind, MediaTick,
    SyncPointAddress, TextSyncPoint,
};

pub struct DetectionEditService;

impl DetectionEditService {
    pub fn add_detection(
        document: &DetectionDocument,
        track: u8,
        kind: DetectionKind,
        media_tick: MediaTick,
    ) -> Option<(DetectionAddress, DetectionChange)> {
        let detection_id = document.next_detection_id(track)?;
        let address = DetectionAddress {
            track,
            detection_id,
        };
        let cue = DetectionCue {
            id: detection_id,
            kind,
            media_tick,
        };
        Some((
            address,
            DetectionChange::AddDetection {
                address,
                cue,
            },
        ))
    }

    pub fn remove_detection(
        document: &DetectionDocument,
        address: DetectionAddress,
    ) -> Option<DetectionChange> {
        Some(DetectionChange::RemoveDetection {
            address,
            cue: document.detection(address)?.clone(),
        })
    }

    pub fn move_detection(
        document: &DetectionDocument,
        address: DetectionAddress,
        new_tick: MediaTick,
    ) -> Option<DetectionChange> {
        let old_tick = document.detection(address)?.media_tick;
        (old_tick != new_tick).then_some(DetectionChange::MoveDetection {
            address,
            old_tick,
            new_tick,
        })
    }

    pub fn add_sync_point(
        document: &DetectionDocument,
        line_id: u64,
        grapheme_boundary: u32,
        line_tick: MediaTick,
    ) -> Option<(SyncPointAddress, DetectionChange)> {
        let sync_point_id = document.next_sync_point_id(line_id)?;
        let address = SyncPointAddress {
            line_id,
            sync_point_id,
        };
        Some((
            address,
            DetectionChange::AddSyncPoint {
                address,
                point: TextSyncPoint {
                    id: sync_point_id,
                    grapheme_boundary,
                    line_tick,
                },
            },
        ))
    }

    pub fn remove_sync_point(
        document: &DetectionDocument,
        address: SyncPointAddress,
    ) -> Option<DetectionChange> {
        Some(DetectionChange::RemoveSyncPoint {
            address,
            point: document.sync_point(address)?.clone(),
        })
    }

    pub fn move_sync_point(
        document: &DetectionDocument,
        address: SyncPointAddress,
        new_tick: MediaTick,
    ) -> Option<DetectionChange> {
        let old_tick = document.sync_point(address)?.line_tick;
        (old_tick != new_tick).then_some(DetectionChange::MoveSyncPoint {
            address,
            old_tick,
            new_tick,
        })
    }

    pub fn coalesce(previous: &mut DetectionChange, next: &DetectionChange) -> bool {
        match (previous, next) {
            (
                DetectionChange::MoveDetection {
                    address: previous_address,
                    new_tick: previous_tick,
                    ..
                },
                DetectionChange::MoveDetection {
                    address: next_address,
                    new_tick: next_tick,
                    ..
                },
            ) if previous_address == next_address => {
                *previous_tick = *next_tick;
                true
            }
            (
                DetectionChange::MoveSyncPoint {
                    address: previous_address,
                    new_tick: previous_tick,
                    ..
                },
                DetectionChange::MoveSyncPoint {
                    address: next_address,
                    new_tick: next_tick,
                    ..
                },
            ) if previous_address == next_address => {
                *previous_tick = *next_tick;
                true
            }
            _ => false,
        }
    }
}
'''


STATE_DETECTION_RS = r'''//! Detection and text synchronization use cases attached to application state.

use crate::application::detection_service::DetectionEditService;
use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::command::Command;
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionKind, MediaTick, SyncPointAddress,
    MEDIA_TICKS_PER_FRAME,
};
use crate::state::State;
use crate::workspaces::rythmo::view::Selection;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DetectionAudition {
    pub beep: MediaTick,
    pub end: MediaTick,
    pub beeped: bool,
}

impl State {
    pub fn has_selected_detection(&self) -> bool {
        matches!(
            self.ui_shell.ui.rythmo_state.selected,
            Some(Selection::Detection(_))
        )
    }

    pub fn rythmo_detection_hovered(&self) -> bool {
        self.ui_shell.ui.rythmo_state.detection_hover.is_some()
    }

    pub fn open_detection_palette_from_hover(&mut self) -> bool {
        self.ui_shell
            .ui
            .rythmo_state
            .open_detection_palette_from_hover()
    }

    pub fn focus_detection_parent_line(&mut self) -> bool {
        match self.ui_shell.ui.rythmo_state.selected {
            Some(Selection::Detection(_)) => {
                self.ui_shell.ui.rythmo_state.selected = None;
                self.ui_shell.ui.rythmo_state.detection_drag = None;
                true
            }
            Some(Selection::SyncPoint(address)) => {
                self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
                self.ui_shell.ui.rythmo_state.sync_point_drag = None;
                true
            }
            _ => false,
        }
    }

    pub fn add_detection(&mut self, track: u8, kind: DetectionKind, media_tick: MediaTick) {
        if track as usize >= crate::rythmo_layout::track_count() {
            return;
        }
        let Some((address, change)) = DetectionEditService::add_detection(
            self.project_session.project.detections(),
            track,
            kind,
            media_tick.clamp(MediaTick::ZERO, MediaTick::from_frame(self.total_frames())),
        ) else {
            return;
        };
        self.execute_and_broadcast(Command::Detection { change });
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
        self.ui_shell.ui.rythmo_state.detection_menu = None;
    }

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) {
        let media_tick = media_tick.clamp(MediaTick::ZERO, MediaTick::from_frame(self.total_frames()));
        let Some(change) = DetectionEditService::move_detection(
            self.project_session.project.detections(),
            address,
            media_tick,
        ) else {
            return;
        };
        let command = Command::Detection {
            change: change.clone(),
        };
        let can_coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection { change: previous })
                if matches!(
                    (previous, &change),
                    (
                        DetectionChange::MoveDetection { address: a, .. },
                        DetectionChange::MoveDetection { address: b, .. }
                    ) if a == b
                )
        );
        if can_coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection { change: previous } = last {
                        let _ = DetectionEditService::coalesce(previous, &change);
                    }
                },
                EditOrigin::Local,
            );
        } else {
            self.execute_and_broadcast(command);
        }
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
    }

    pub fn delete_detection(&mut self, address: DetectionAddress) {
        let Some(change) = DetectionEditService::remove_detection(
            self.project_session.project.detections(),
            address,
        ) else {
            return;
        };
        self.execute_and_broadcast(Command::Detection { change });
        self.ui_shell.ui.rythmo_state.selected = None;
        self.ui_shell.ui.rythmo_state.detection_drag = None;
    }

    pub fn nudge_selected_detection(&mut self, delta_ticks: i64) {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return;
        };
        let Some(current) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .map(|cue| cue.media_tick)
        else {
            return;
        };
        self.move_detection(
            address,
            MediaTick(current.raw().saturating_add(delta_ticks)),
        );
    }

    pub fn add_sync_point(
        &mut self,
        line_id: u64,
        grapheme_boundary: u32,
        line_tick: MediaTick,
    ) {
        let Some(line) = self.project_session.project.get_line(line_id) else {
            return;
        };
        let max_tick = MediaTick::from_frame(line.duration_frames);
        let Some((address, change)) = DetectionEditService::add_sync_point(
            self.project_session.project.detections(),
            line_id,
            grapheme_boundary,
            line_tick.clamp(MediaTick::ZERO, max_tick),
        ) else {
            return;
        };
        self.execute_and_broadcast(Command::Detection { change });
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::SyncPoint(address));
    }

    pub fn move_sync_point(&mut self, address: SyncPointAddress, line_tick: MediaTick) {
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let line_tick = line_tick.clamp(
            MediaTick(MEDIA_TICKS_PER_FRAME),
            MediaTick::from_frame(line.duration_frames)
                .saturating_sub(MediaTick(MEDIA_TICKS_PER_FRAME)),
        );
        let Some(change) = DetectionEditService::move_sync_point(
            self.project_session.project.detections(),
            address,
            line_tick,
        ) else {
            return;
        };
        let command = Command::Detection {
            change: change.clone(),
        };
        let can_coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection { change: previous })
                if matches!(
                    (previous, &change),
                    (
                        DetectionChange::MoveSyncPoint { address: a, .. },
                        DetectionChange::MoveSyncPoint { address: b, .. }
                    ) if a == b
                )
        );
        if can_coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection { change: previous } = last {
                        let _ = DetectionEditService::coalesce(previous, &change);
                    }
                },
                EditOrigin::Local,
            );
        } else {
            self.execute_and_broadcast(command);
        }
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::SyncPoint(address));
    }

    pub fn delete_sync_point(&mut self, address: SyncPointAddress) {
        let Some(change) = DetectionEditService::remove_sync_point(
            self.project_session.project.detections(),
            address,
        ) else {
            return;
        };
        self.execute_and_broadcast(Command::Detection { change });
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        self.ui_shell.ui.rythmo_state.sync_point_drag = None;
    }

    pub fn audition_selected_detection(&mut self) -> bool {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return false;
        };
        let Some(window) = self
            .project_session
            .project
            .detections()
            .audition_window(address, self.fps())
        else {
            return false;
        };
        self.seek_absolute(window.start.as_frame_position().floor() as i64);
        self.detection_audition = Some(DetectionAudition {
            beep: window.beep,
            end: window.end,
            beeped: false,
        });
        if !self.is_playing() {
            self.toggle_play_pause();
        }
        true
    }

    pub(crate) fn tick_detection_audition(&mut self) {
        let Some(mut audition) = self.detection_audition else {
            return;
        };
        let current = MediaTick::from_frame_position(self.render_frame());
        if !audition.beeped && current >= audition.beep {
            crate::platform::play_detection_beep();
            audition.beeped = true;
        }
        if current >= audition.end {
            if self.is_playing() {
                self.toggle_play_pause();
            }
            self.detection_audition = None;
        } else {
            self.detection_audition = Some(audition);
        }
    }
}
'''


DETECTION_UI_RS = r'''//! Editor-only interaction and vector rendering for source detections and text sync points.

use super::*;
use crate::detection::{
    DetectionAddress, DetectionKind, MediaTick, SyncPointAddress, MEDIA_TICKS_PER_FRAME,
};
use unicode_segmentation::UnicodeSegmentation;

const SYMBOL_SIZE: f32 = 18.0;
const BUTTON_SIZE: f32 = 22.0;
const MENU_ROW_H: f32 = 30.0;
const MENU_PADDING: f32 = 6.0;
const MENU_WIDTH: f32 = 230.0;
const DOT_RADIUS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionHover {
    pub track: u8,
    pub media_tick: MediaTick,
    pub track_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionMenu {
    pub track: u8,
    pub media_tick: MediaTick,
    pub x: f32,
    pub y: f32,
    pub hover_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionDrag {
    pub address: DetectionAddress,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncPointDrag {
    pub address: SyncPointAddress,
}

impl RythmoState {
    pub(crate) fn open_detection_palette_from_hover(&mut self) -> bool {
        let Some(hover) = self.detection_hover else {
            return false;
        };
        let button = detection_button_rect(&hover, 0.0, &hover.track_rect);
        self.detection_menu = Some(DetectionMenu {
            track: hover.track,
            media_tick: hover.media_tick,
            x: button.x,
            y: button.y + button.height + 2.0,
            hover_index: None,
        });
        true
    }
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

fn track_rect(ctx: &RythmoCtx<'_>, track: u8) -> Rect {
    editor_track_body_rect_at_frame(
        ctx.project,
        rythmo_layout::y_slot_for_track_index(track as usize),
        ctx.current_frame,
        ctx.zone,
    )
}

fn track_under_pointer(ctx: &RythmoCtx<'_>, x: f32, y: f32) -> Option<(u8, Rect)> {
    if !ctx.zone.contains(x, y) || y < ctx.zone.y + constants::RULER_HEIGHT {
        return None;
    }
    for layout in editor_track_layouts_at_frame(ctx.project, ctx.current_frame, ctx.zone) {
        let rect = track_rect(ctx, layout.track_index as u8);
        if rect.contains(x, y) {
            return Some((layout.track_index as u8, rect));
        }
    }
    None
}

fn detection_button_rect(hover: &DetectionHover, current_frame: f64, zone: &Rect) -> Rect {
    Rect {
        x: tick_x(hover.media_tick, current_frame, zone) - BUTTON_SIZE / 2.0,
        y: hover.track_rect.y + 2.0,
        width: BUTTON_SIZE,
        height: BUTTON_SIZE,
    }
}

fn detection_symbol_rect(track_rect: Rect, tick: MediaTick, current_frame: f64, zone: &Rect) -> Rect {
    Rect {
        x: tick_x(tick, current_frame, zone) - SYMBOL_SIZE / 2.0,
        y: track_rect.y + (track_rect.height - SYMBOL_SIZE) / 2.0,
        width: SYMBOL_SIZE,
        height: SYMBOL_SIZE,
    }
}

fn menu_rect(menu: &DetectionMenu, zone: &Rect) -> Rect {
    let height = DetectionKind::ALL.len() as f32 * MENU_ROW_H + MENU_PADDING * 2.0;
    Rect {
        x: menu.x.clamp(zone.x, (zone.x + zone.width - MENU_WIDTH).max(zone.x)),
        y: menu.y.clamp(zone.y, (zone.y + zone.height - height).max(zone.y)),
        width: MENU_WIDTH,
        height,
    }
}

fn menu_item_rect(menu: &DetectionMenu, zone: &Rect, index: usize) -> Rect {
    let outer = menu_rect(menu, zone);
    Rect {
        x: outer.x + MENU_PADDING,
        y: outer.y + MENU_PADDING + index as f32 * MENU_ROW_H,
        width: outer.width - MENU_PADDING * 2.0,
        height: MENU_ROW_H,
    }
}

fn hit_detection(ctx: &RythmoCtx<'_>, x: f32, y: f32) -> Option<DetectionAddress> {
    for (track, data) in ctx.project.detections().tracks() {
        let rect = track_rect(ctx, track);
        for cue in data.detections() {
            if detection_symbol_rect(rect, cue.media_tick, ctx.current_frame, ctx.zone).contains(x, y)
            {
                return Some(DetectionAddress {
                    track,
                    detection_id: cue.id,
                });
            }
        }
    }
    None
}

fn explicit_sync_dot_rect(
    line: &crate::rythmo_line::RythmoLine,
    point: &crate::detection::TextSyncPoint,
    current_frame: f64,
    zone: &Rect,
    project: &Project,
) -> Rect {
    let absolute = MediaTick::from_frame(line.start_frame).saturating_add(point.line_tick);
    let line_rect = line_rect(project, line, current_frame, zone);
    Rect {
        x: tick_x(absolute, current_frame, zone) - DOT_RADIUS - 2.0,
        y: line_rect.y + line_rect.height - DOT_RADIUS * 2.0 - 2.0,
        width: (DOT_RADIUS + 2.0) * 2.0,
        height: (DOT_RADIUS + 2.0) * 2.0,
    }
}

fn placeholder_sync_dot_rect(
    line: &crate::rythmo_line::RythmoLine,
    boundary: usize,
    count: usize,
    current_frame: f64,
    zone: &Rect,
    project: &Project,
) -> Rect {
    let rect = line_rect(project, line, current_frame, zone);
    let ratio = boundary as f32 / count.max(1) as f32;
    Rect {
        x: rect.x + rect.width * ratio - DOT_RADIUS,
        y: rect.y + rect.height - DOT_RADIUS * 2.0 - 1.0,
        width: DOT_RADIUS * 2.0,
        height: DOT_RADIUS * 2.0,
    }
}

fn hit_explicit_sync_point(
    ctx: &RythmoCtx<'_>,
    x: f32,
    y: f32,
) -> Option<SyncPointAddress> {
    for line in ctx.project.lines() {
        let Some(data) = ctx.project.detections().line_sync(line.id) else {
            continue;
        };
        for point in data.sync_points() {
            if explicit_sync_dot_rect(line, point, ctx.current_frame, ctx.zone, ctx.project)
                .contains(x, y)
            {
                return Some(SyncPointAddress {
                    line_id: line.id,
                    sync_point_id: point.id,
                });
            }
        }
    }
    None
}

fn hit_placeholder_sync_point(
    ctx: &RythmoCtx<'_>,
    x: f32,
    y: f32,
) -> Option<(u64, u32, MediaTick)> {
    for line in ctx.project.lines() {
        if line.text.is_empty() || !line_rect(ctx.project, line, ctx.current_frame, ctx.zone).contains(x, y)
        {
            continue;
        }
        let count = line.text.graphemes(true).count();
        if count <= 1 {
            continue;
        }
        let existing = ctx.project.detections().line_sync(line.id);
        for boundary in 1..count {
            if existing.is_some_and(|data| {
                data.sync_points()
                    .iter()
                    .any(|point| point.grapheme_boundary == boundary as u32)
            }) {
                continue;
            }
            if placeholder_sync_dot_rect(
                line,
                boundary,
                count,
                ctx.current_frame,
                ctx.zone,
                ctx.project,
            )
            .contains(x, y)
            {
                let raw = ((line.duration_frames.saturating_mul(MEDIA_TICKS_PER_FRAME)) as f64
                    * boundary as f64
                    / count as f64)
                    .round() as i64;
                return Some((line.id, boundary as u32, MediaTick(raw)));
            }
        }
    }
    None
}

fn navigate_detection(project: &Project, state: &mut RythmoState, direction: i32) -> bool {
    let (track, current) = match state.selected {
        Some(Selection::Detection(address)) => (address.track, Some(address.detection_id)),
        _ => return false,
    };
    let Some(data) = project.detections().track(track) else {
        return false;
    };
    let cues = data.detections();
    if cues.is_empty() {
        return false;
    }
    let current_index = current
        .and_then(|id| cues.iter().position(|cue| cue.id == id))
        .unwrap_or(0);
    let index = if direction < 0 {
        current_index.checked_sub(1).unwrap_or(cues.len() - 1)
    } else {
        (current_index + 1) % cues.len()
    };
    state.selected = Some(Selection::Detection(DetectionAddress {
        track,
        detection_id: cues[index].id,
    }));
    true
}

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    match event {
        UiEvent::MouseMove { x, .. } if state.detection_drag.is_some() => {
            let drag = state.detection_drag.unwrap();
            return Some(EventResponse::Action(UiAction::MoveDetection {
                address: drag.address,
                media_tick: pointer_tick(*x, ctx.current_frame, ctx.zone),
            }));
        }
        UiEvent::MouseMove { x, .. } if state.sync_point_drag.is_some() => {
            let drag = state.sync_point_drag.unwrap();
            let line = ctx.project.get_line(drag.address.line_id)?;
            let absolute = pointer_tick(*x, ctx.current_frame, ctx.zone);
            let relative = MediaTick(
                absolute
                    .raw()
                    .saturating_sub(MediaTick::from_frame(line.start_frame).raw()),
            );
            return Some(EventResponse::Action(UiAction::MoveSyncPoint {
                address: drag.address,
                line_tick: relative,
            }));
        }
        UiEvent::MouseMove { x, y } => {
            if let Some(mut menu) = state.detection_menu {
                menu.hover_index = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                    .map(|(index, _)| index);
                state.detection_menu = Some(menu);
                return Some(EventResponse::Consumed);
            }
            state.detection_hover = track_under_pointer(ctx, *x, *y).map(|(track, rect)| {
                DetectionHover {
                    track,
                    media_tick: pointer_tick(*x, ctx.current_frame, ctx.zone),
                    track_rect: rect,
                }
            });
        }
        UiEvent::MousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                if let Some((_, kind)) = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                {
                    state.detection_menu = None;
                    return Some(EventResponse::Action(UiAction::AddDetection {
                        track: menu.track,
                        kind: *kind,
                        media_tick: menu.media_tick,
                    }));
                }
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }
            if let Some(address) = hit_explicit_sync_point(ctx, *x, *y) {
                state.selected = Some(Selection::SyncPoint(address));
                state.sync_point_drag = Some(SyncPointDrag { address });
                return Some(EventResponse::Consumed);
            }
            if let Some((line_id, grapheme_boundary, line_tick)) =
                hit_placeholder_sync_point(ctx, *x, *y)
            {
                return Some(EventResponse::Action(UiAction::AddSyncPoint {
                    line_id,
                    grapheme_boundary,
                    line_tick,
                }));
            }
            if let Some(address) = hit_detection(ctx, *x, *y) {
                state.selected = Some(Selection::Detection(address));
                state.detection_drag = Some(DetectionDrag { address });
                return Some(EventResponse::Consumed);
            }
            if let Some(hover) = state.detection_hover {
                if detection_button_rect(&hover, ctx.current_frame, ctx.zone).contains(*x, *y) {
                    state.open_detection_palette_from_hover();
                    return Some(EventResponse::Consumed);
                }
            }
        }
        UiEvent::MouseRelease { .. } => {
            if state.detection_drag.take().is_some() || state.sync_point_drag.take().is_some() {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            if state.detection_menu.take().is_some() {
                return Some(EventResponse::Consumed);
            }
            if matches!(state.selected, Some(Selection::Detection(_) | Selection::SyncPoint(_))) {
                state.selected = None;
                state.detection_drag = None;
                state.sync_point_drag = None;
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorLeft => {
            if navigate_detection(ctx.project, state, -1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorRight => {
            if navigate_detection(ctx.project, state, 1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::Delete => match state.selected {
            Some(Selection::Detection(address)) => {
                return Some(EventResponse::Action(UiAction::DeleteDetection { address }));
            }
            Some(Selection::SyncPoint(address)) => {
                return Some(EventResponse::Action(UiAction::DeleteSyncPoint { address }));
            }
            _ => {}
        },
        _ => {}
    }
    None
}

fn push_shape(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    border_width: f32,
    radius: f32,
    rotation: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation,
        _padding: [0.0; 2],
    });
}

fn symbol_line(quads: &mut Vec<QuadInstance>, cx: f32, cy: f32, w: f32, angle: f32, color: [f32; 4]) {
    push_shape(
        quads,
        Rect {
            x: cx - w / 2.0,
            y: cy - 0.8,
            width: w,
            height: 1.6,
        },
        color,
        [0.0; 4],
        0.0,
        0.8,
        angle,
    );
}

fn render_symbol(kind: DetectionKind, rect: Rect, selected: bool, quads: &mut Vec<QuadInstance>) {
    let color = if selected {
        [1.0, 0.72, 0.20, 1.0]
    } else {
        [0.90, 0.90, 0.94, 0.96]
    };
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let w = rect.width * 0.72;
    let h = rect.height * 0.52;
    match kind {
        DetectionKind::Labial => {
            symbol_line(quads, cx, cy - 1.5, w, -0.08, color);
            symbol_line(quads, cx, cy + 1.5, w, 0.08, color);
        }
        DetectionKind::SemiLabial => {
            symbol_line(quads, cx - 1.0, cy - 1.5, w * 0.85, -0.08, color);
            symbol_line(quads, cx + 2.0, cy + 2.0, w * 0.52, 0.08, color);
        }
        DetectionKind::MouthOpen => push_shape(
            quads,
            Rect {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                width: w,
                height: h,
            },
            [0.0; 4],
            color,
            1.8,
            h / 2.0,
            0.0,
        ),
        DetectionKind::MouthClosed => symbol_line(quads, cx, cy, w, 0.0, color),
        DetectionKind::TeethVisible => {
            push_shape(
                quads,
                Rect {
                    x: cx - w / 2.0,
                    y: cy - h / 2.0,
                    width: w,
                    height: h,
                },
                [0.0; 4],
                color,
                1.5,
                h / 2.0,
                0.0,
            );
            symbol_line(quads, cx, cy, w * 0.86, 0.0, color);
            for offset in [-0.22_f32, 0.0, 0.22] {
                push_shape(
                    quads,
                    Rect {
                        x: cx + offset * w - 0.55,
                        y: cy - h * 0.38,
                        width: 1.1,
                        height: h * 0.76,
                    },
                    color,
                    [0.0; 4],
                    0.0,
                    0.5,
                    0.0,
                );
            }
        }
        DetectionKind::Breath => {
            symbol_line(quads, cx - 2.5, cy - 3.0, w * 0.58, 0.10, color);
            symbol_line(quads, cx + 1.5, cy, w * 0.72, -0.10, color);
            symbol_line(quads, cx - 1.0, cy + 3.0, w * 0.50, 0.10, color);
        }
        DetectionKind::Reaction => {
            for angle in [0.0_f32, 0.785, 1.57, 2.355] {
                symbol_line(quads, cx, cy, w * 0.88, angle, color);
            }
            push_shape(
                quads,
                Rect {
                    x: cx - 1.8,
                    y: cy - 1.8,
                    width: 3.6,
                    height: 3.6,
                },
                color,
                [0.0; 4],
                0.0,
                1.8,
                0.0,
            );
        }
    }
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    for (track, data) in project.detections().tracks() {
        let rect = editor_track_body_rect_at_frame(
            project,
            rythmo_layout::y_slot_for_track_index(track as usize),
            current_frame,
            zone,
        );
        for cue in data.detections() {
            let x = tick_x(cue.media_tick, current_frame, zone);
            if x < zone.x - SYMBOL_SIZE || x > zone.x + zone.width + SYMBOL_SIZE {
                continue;
            }
            let address = DetectionAddress {
                track,
                detection_id: cue.id,
            };
            let selected = matches!(state.selected, Some(Selection::Detection(current)) if current == address);
            push_shape(
                quads,
                Rect {
                    x: x - 0.65,
                    y: rect.y,
                    width: 1.3,
                    height: rect.height,
                },
                if selected {
                    [1.0, 0.72, 0.20, 0.85]
                } else {
                    [0.72, 0.72, 0.78, 0.55]
                },
                [0.0; 4],
                0.0,
                0.0,
                0.0,
            );
            let symbol = detection_symbol_rect(rect, cue.media_tick, current_frame, zone);
            if selected {
                push_shape(
                    quads,
                    Rect {
                        x: symbol.x - 3.0,
                        y: symbol.y - 3.0,
                        width: symbol.width + 6.0,
                        height: symbol.height + 6.0,
                    },
                    [0.07, 0.07, 0.09, 0.88],
                    [1.0, 0.72, 0.20, 0.95],
                    1.2,
                    5.0,
                    0.0,
                );
            }
            render_symbol(cue.kind, symbol, selected, quads);
        }
    }

    // Synchronization dots live on dialogue lines and never enter export renderers.
    for line in project.lines() {
        if line.text.is_empty() {
            continue;
        }
        let count = line.text.graphemes(true).count();
        if count <= 1 {
            continue;
        }
        let line_data = project.detections().line_sync(line.id);
        for boundary in 1..count {
            if line_data.is_some_and(|data| {
                data.sync_points()
                    .iter()
                    .any(|point| point.grapheme_boundary == boundary as u32)
            }) {
                continue;
            }
            let dot = placeholder_sync_dot_rect(line, boundary, count, current_frame, zone, project);
            push_shape(
                quads,
                dot,
                [0.72, 0.72, 0.78, 0.34],
                [0.0; 4],
                0.0,
                DOT_RADIUS,
                0.0,
            );
        }
        if let Some(data) = line_data {
            for point in data.sync_points() {
                let address = SyncPointAddress {
                    line_id: line.id,
                    sync_point_id: point.id,
                };
                let selected = matches!(state.selected, Some(Selection::SyncPoint(current)) if current == address);
                let dot = explicit_sync_dot_rect(line, point, current_frame, zone, project);
                push_shape(
                    quads,
                    dot,
                    if selected {
                        [1.0, 0.72, 0.20, 1.0]
                    } else {
                        [0.38, 0.72, 1.0, 0.95]
                    },
                    [0.06, 0.06, 0.08, 0.9],
                    1.0,
                    dot.height / 2.0,
                    0.0,
                );
            }
        }
    }

    if let Some(hover) = state.detection_hover {
        let x = tick_x(hover.media_tick, current_frame, zone);
        let mut y = hover.track_rect.y;
        while y < hover.track_rect.y + hover.track_rect.height {
            push_shape(
                quads,
                Rect {
                    x: x - 0.5,
                    y,
                    width: 1.0,
                    height: 3.0_f32.min(hover.track_rect.y + hover.track_rect.height - y),
                },
                [0.65, 0.65, 0.68, 0.65],
                [0.0; 4],
                0.0,
                0.0,
                0.0,
            );
            y += 6.0;
        }
        let button = detection_button_rect(&hover, current_frame, zone);
        push_shape(
            quads,
            button,
            [0.09, 0.09, 0.12, 0.95],
            [0.62, 0.62, 0.70, 0.9],
            1.0,
            5.0,
            0.0,
        );
        render_symbol(
            DetectionKind::MouthOpen,
            Rect {
                x: button.x + 3.0,
                y: button.y + 3.0,
                width: button.width - 6.0,
                height: button.height - 6.0,
            },
            false,
            quads,
        );
    }

    if let Some(menu) = state.detection_menu {
        let outer = menu_rect(&menu, zone);
        push_shape(
            quads,
            outer,
            [0.055, 0.055, 0.07, 0.99],
            [0.48, 0.48, 0.56, 0.9],
            1.0,
            5.0,
            0.0,
        );
        for (index, kind) in DetectionKind::ALL.iter().copied().enumerate() {
            let row = menu_item_rect(&menu, zone, index);
            if menu.hover_index == Some(index) {
                push_shape(
                    quads,
                    row,
                    [0.16, 0.23, 0.39, 0.98],
                    [0.32, 0.52, 0.86, 0.65],
                    1.0,
                    3.0,
                    0.0,
                );
            }
            render_symbol(
                kind,
                Rect {
                    x: row.x + 6.0,
                    y: row.y + 6.0,
                    width: 18.0,
                    height: 18.0,
                },
                false,
                quads,
            );
            labels.push(LabelInfo {
                text: kind.display_name(),
                bounds: Rect {
                    x: row.x + 34.0,
                    y: row.y,
                    width: row.width - 38.0,
                    height: row.height,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([235, 235, 240]),
                font_family_override: None,
            });
        }
    }
}
'''

write("src/detection.rs", DETECTION_RS)
write("src/application/detection_service.rs", DETECTION_SERVICE_RS)
write("src/state_detection.rs", STATE_DETECTION_RS)
write("src/workspaces/rythmo/detection_ui.rs", DETECTION_UI_RS)

# Unicode grapheme support.
cargo = read("Cargo.toml")
if "unicode-segmentation" not in cargo:
    cargo = cargo.replace('image = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp", "bmp", "ico", "gif"] }\n', 'image = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp", "bmp", "ico", "gif"] }\nunicode-segmentation = "1.12"\n')
write("Cargo.toml", cargo)

# UI semantic actions.
replace_regex(
    "src/application/command.rs",
    r"    AddDetection \{\n        line_id: u64,\n        kind: crate::detection::DetectionKind,\n        media_tick: crate::detection::MediaTick,\n        target: crate::detection::TextAnchor,\n    \},\n    MoveDetection \{\n        address: crate::detection::DetectionAddress,\n        media_tick: crate::detection::MediaTick,\n    \},\n    DeleteDetection \{\n        address: crate::detection::DetectionAddress,\n    \},\n    NudgeSelectedDetection \{\n        delta_ticks: i64,\n    \},",
    "    AddDetection {\n        track: u8,\n        kind: crate::detection::DetectionKind,\n        media_tick: crate::detection::MediaTick,\n    },\n    MoveDetection {\n        address: crate::detection::DetectionAddress,\n        media_tick: crate::detection::MediaTick,\n    },\n    DeleteDetection {\n        address: crate::detection::DetectionAddress,\n    },\n    NudgeSelectedDetection {\n        delta_ticks: i64,\n    },\n    AddSyncPoint {\n        line_id: u64,\n        grapheme_boundary: u32,\n        line_tick: crate::detection::MediaTick,\n    },\n    MoveSyncPoint {\n        address: crate::detection::SyncPointAddress,\n        line_tick: crate::detection::MediaTick,\n    },\n    DeleteSyncPoint {\n        address: crate::detection::SyncPointAddress,\n    },",
)

# Dispatcher actions.
replace_regex(
    "src/app/dispatcher.rs",
    r"            UiAction::AddDetection \{\n                line_id,\n                kind,\n                media_tick,\n                target,\n            \} => \{\n                state\.add_detection\(line_id, kind, media_tick, target\);\n            \}",
    "            UiAction::AddDetection {\n                track,\n                kind,\n                media_tick,\n            } => {\n                state.add_detection(track, kind, media_tick);\n            }",
)
replace_once(
    "src/app/dispatcher.rs",
    "            UiAction::NudgeSelectedDetection { delta_ticks } => {\n                state.nudge_selected_detection(delta_ticks);\n            }\n",
    "            UiAction::NudgeSelectedDetection { delta_ticks } => {\n                state.nudge_selected_detection(delta_ticks);\n            }\n            UiAction::AddSyncPoint {\n                line_id,\n                grapheme_boundary,\n                line_tick,\n            } => state.add_sync_point(line_id, grapheme_boundary, line_tick),\n            UiAction::MoveSyncPoint { address, line_tick } => {\n                state.move_sync_point(address, line_tick);\n            }\n            UiAction::DeleteSyncPoint { address } => state.delete_sync_point(address),\n",
)

# Selection state and transient drag state.
replace_once(
    "src/workspaces/rythmo/state.rs",
    "    Detection(crate::detection::DetectionAddress),\n",
    "    Detection(crate::detection::DetectionAddress),\n    SyncPoint(crate::detection::SyncPointAddress),\n",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    "    pub detection_drag: Option<DetectionDrag>,\n",
    "    pub detection_drag: Option<DetectionDrag>,\n    pub sync_point_drag: Option<SyncPointDrag>,\n",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    "            detection_drag: None,\n",
    "            detection_drag: None,\n            sync_point_drag: None,\n",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    "            || self.syllable_drag.is_some()\n",
    "            || self.syllable_drag.is_some()\n            || self.detection_drag.is_some()\n            || self.sync_point_drag.is_some()\n",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    "        self.context_menu = None;\n",
    "        self.context_menu = None;\n        self.detection_hover = None;\n        self.detection_menu = None;\n        self.detection_drag = None;\n        self.sync_point_drag = None;\n",
)

# Read-only cleanup includes sync points.
replace_once(
    "src/workspaces/rythmo/controller.rs",
    "    state.detection_drag = None;\n    if matches!(state.selected, Some(Selection::Detection(_))) {\n",
    "    state.detection_drag = None;\n    state.sync_point_drag = None;\n    if matches!(\n        state.selected,\n        Some(Selection::Detection(_) | Selection::SyncPoint(_))\n    ) {\n",
)

# Generic selection helpers know sync points are not lines.
replace_once(
    "src/state.rs",
    "            Some(Selection::Marker(_) | Selection::Strokes(_) | Selection::Detection(_)) | None => {\n",
    "            Some(\n                Selection::Marker(_)\n                | Selection::Strokes(_)\n                | Selection::Detection(_)\n                | Selection::SyncPoint(_),\n            )\n            | None => {\n",
)
replace_once(
    "src/state.rs",
    "                Selection::Detection(_) => {\n                    // Routed through the semantic detection action before this\n                    // legacy selection deletion path is reached.\n                }\n",
    "                Selection::Detection(_) | Selection::SyncPoint(_) => {\n                    // Routed through semantic detection/sync actions first.\n                }\n",
)

# State owns finite audition playback state.
replace_once(
    "src/state.rs",
    "    last_progress_announcement: Option<Instant>,\n",
    "    last_progress_announcement: Option<Instant>,\n    pub(crate) detection_audition: Option<crate::state_detection::DetectionAudition>,\n",
)
replace_once(
    "src/state.rs",
    "            last_progress_announcement: None,\n",
    "            last_progress_announcement: None,\n            detection_audition: None,\n",
)
replace_once(
    "src/state.rs",
    "            if !player.is_playing() && self.ui_shell.ui.is_playing() {\n",
    "            if !player.is_playing() && self.ui_shell.ui.is_playing() {\n",
)
# Invoke audition monitor once the player borrow ends.
replace_regex(
    "src/state.rs",
    r"    fn tick_video\(&mut self\) \{\n        if let Some\(player\) = &mut self\.playback\.video_player \{(.*?)\n        \}\n    \}",
    lambda m: "    fn tick_video(&mut self) {\n        if let Some(player) = &mut self.playback.video_player {" + m.group(1) + "\n        }\n        self.tick_detection_audition();\n    }",
)

# Project applies global track detections to every language, while line sync stays local.
replace_regex(
    "src/project.rs",
    r"    pub\(crate\) fn apply_detection_change\(\n        &mut self,\n        change: &crate::detection::DetectionChange,\n        forward: bool,\n    \) -> bool \{\n        let changed = if forward \{\n            change\.apply\(&mut self\.settings\.detections\)\n        \} else \{\n            change\.unapply\(&mut self\.settings\.detections\)\n        \};\n        if changed \{\n            self\.bump_revision\(\);\n        \}\n        changed\n    \}",
    "    pub(crate) fn apply_detection_change(\n        &mut self,\n        change: &crate::detection::DetectionChange,\n        forward: bool,\n    ) -> bool {\n        let apply = |document: &mut crate::detection::DetectionDocument| {\n            if forward {\n                change.apply(document)\n            } else {\n                change.unapply(document)\n            }\n        };\n        let changed = apply(&mut self.settings.detections);\n        if changed && change.is_source_detection() {\n            for snapshot in self.language_snapshots.values_mut() {\n                let _ = apply(&mut snapshot.band.settings.detections);\n                snapshot.band.revision = snapshot.band.revision.wrapping_add(1);\n            }\n        }\n        if changed {\n            self.bump_revision();\n        }\n        changed\n    }",
)
# Preserve source detections while changing active adaptation language.
replace_once(
    "src/project.rs",
    "        let scrolling_text_uses_character_color = self.settings.scrolling_text_uses_character_color;\n        let outgoing = StoredLanguageSnapshot {\n",
    "        let scrolling_text_uses_character_color = self.settings.scrolling_text_uses_character_color;\n        let source_detections = self.settings.detections.clone();\n        let outgoing = StoredLanguageSnapshot {\n",
)
replace_once(
    "src/project.rs",
    "        incoming.band.settings.scrolling_text_uses_character_color =\n            scrolling_text_uses_character_color;\n",
    "        incoming.band.settings.scrolling_text_uses_character_color =\n            scrolling_text_uses_character_color;\n        // Track detections describe the source video and are shared by every adaptation.\n        // Line sync data remains in each language; merge only the source track layer.\n        incoming.band.settings.detections.replace_source_tracks_from(&source_detections);\n",
)

# Alt+D only; Ctrl+Space auditions the selected cue.
event_loop = read("src/app/event_loop.rs")
marker = "&& state.rythmo_detection_hovered()"
marker_index = event_loop.index(marker)
block_start = max(0, marker_index - 500)
block_end = marker_index + 350
block = event_loop[block_start:block_end]
block = block.replace("&& !keyboard_modifiers.alt", "&& keyboard_modifiers.alt", 1)
event_loop = event_loop[:block_start] + block + event_loop[block_end:]
needle = "                        if matches!(event.logical_key, Key::Named(NamedKey::Escape))\n"
insert = "                        if ctrl_held\n                            && !shift_held\n                            && !keyboard_modifiers.alt\n                            && !event.repeat\n                            && !state.captures_modal_input()\n                            && !state.is_editing_text()\n                            && state.active_workspace() == WorkspaceId::Rythmo\n                            && matches!(event.logical_key, Key::Named(NamedKey::Space))\n                            && state.audition_selected_detection()\n                        {\n                            state.request_redraw();\n                            return;\n                        }\n"
if insert not in event_loop:
    event_loop = event_loop.replace(needle, insert + needle, 1)
write("src/app/event_loop.rs", event_loop)

# Platform beep at the cue position.
platform = read("src/platform.rs")
beep = r'''

#[cfg(target_os = "windows")]
pub(crate) fn play_detection_beep() {
    unsafe {
        let _ = windows_sys::Win32::UI::WindowsAndMessaging::MessageBeep(0);
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn play_detection_beep() {
    use std::io::Write;
    print!("\x07");
    let _ = std::io::stdout().flush();
}
'''
if "pub(crate) fn play_detection_beep" not in platform:
    platform += beep
write("src/platform.rs", platform)

# Rendered dialogue text uses the piecewise synchronization mapping.
view = read("src/workspaces/rythmo/view.rs")
old_render_call = """                if let Some((breaks, ratios)) = visible_syllable_segments(
                    line,
                    drag_ratios,
                    karaoke_lang,
                    karaoke_preview,
                    state,
                ) {"""
new_render_call = """                if let Some((breaks, base_ratios)) = visible_syllable_segments(
                    line,
                    drag_ratios,
                    karaoke_lang,
                    karaoke_preview,
                    state,
                ) {
                    let ratios = project.detections().warped_ratios(
                        line.id,
                        &line.text,
                        &breaks,
                        &base_ratios,
                        line.duration_frames,
                    );"""
if old_render_call not in view:
    raise RuntimeError("render visible_syllable_segments call not found")
view = view.replace(old_render_call, new_render_call, 1)
write("src/workspaces/rythmo/view.rs", view)

print("TRACK_SCOPED_DETECTION_V2 applied")
