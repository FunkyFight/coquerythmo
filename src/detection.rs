//! Semantic dialogue detection and text synchronization primitives.
//!
//! A detection belongs to one dialogue line, while its media position remains
//! absolute in the source video. This keeps simultaneous dialogue tracks
//! independent without turning professional detection signs into global
//! timeline markers.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Number of semantic timing units in one video frame.
pub const MEDIA_TICKS_PER_FRAME: i64 = 10;

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
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DetectionCueId(pub u64);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SyncPointId(pub u64);

/// Stable address used by selection, drag and keyboard navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DetectionAddress {
    pub line_id: u64,
    pub detection_id: DetectionCueId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionFamily {
    Mouth,
    Performance,
    Dialogue,
    Voice,
}

/// Semantic signs visible to a detector/adaptor but never exported as graphics.
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
    SentenceStart,
    SentenceEnd,
    OverlapStart,
    OverlapEnd,
    SpeakerChange,
    OffScreen,
    VoiceOver,
    Telephone,
    Thought,
    Crowd,
}

impl DetectionKind {
    pub const fn family(self) -> DetectionFamily {
        match self {
            Self::Labial
            | Self::SemiLabial
            | Self::MouthOpen
            | Self::MouthClosed
            | Self::TeethVisible => DetectionFamily::Mouth,
            Self::Breath | Self::Reaction => DetectionFamily::Performance,
            Self::SentenceStart
            | Self::SentenceEnd
            | Self::OverlapStart
            | Self::OverlapEnd
            | Self::SpeakerChange => DetectionFamily::Dialogue,
            Self::OffScreen
            | Self::VoiceOver
            | Self::Telephone
            | Self::Thought
            | Self::Crowd => DetectionFamily::Voice,
        }
    }
}

/// A semantic attachment to the adapted text.
///
/// Grapheme indices are deliberately stored rather than UTF-8 byte offsets.
/// The text layout layer is responsible for resolving them through Unicode
/// grapheme segmentation.
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
    /// Boundary before the referenced grapheme in the adapted text.
    pub grapheme_boundary: u32,
    /// Position relative to the beginning of the line.
    pub line_tick: MediaTick,
}

/// Semantic data owned by one dialogue line.
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

    pub fn sync_points(&self) -> &[TextSyncPoint] {
        &self.sync_points
    }

    pub fn detection(&self, id: DetectionCueId) -> Option<&DetectionCue> {
        self.detections.iter().find(|cue| cue.id == id)
    }

    pub fn next_detection_id(&self) -> Option<DetectionCueId> {
        let current = self
            .detections
            .iter()
            .map(|cue| cue.id.0)
            .max()
            .unwrap_or(0);
        current.checked_add(1).map(DetectionCueId)
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
            .find(|cue| cue.media_tick < tick)
    }

    pub fn detection_after(&self, tick: MediaTick) -> Option<&DetectionCue> {
        self.detections.iter().find(|cue| cue.media_tick > tick)
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
        self.detections
            .sort_by_key(|cue| (cue.media_tick, cue.id));
    }
}

/// Project-side semantic storage. The key is the stable `RythmoLine::id`.
///
/// Keeping the line id in the address means two tracks may contain a cue at the
/// exact same media tick without colliding. Moving the line to another track
/// keeps its semantic data, while moving it in time does not move absolute cues.
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

    pub fn retain_lines(&mut self, mut keep: impl FnMut(u64) -> bool) {
        self.lines.retain(|line_id, _| keep(*line_id));
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

    pub fn move_detection(
        &mut self,
        address: DetectionAddress,
        media_tick: MediaTick,
    ) -> bool {
        self.lines
            .get_mut(&address.line_id)
            .is_some_and(|line| line.move_detection(address.detection_id, media_tick))
    }

    pub fn audition_window(
        &self,
        address: DetectionAddress,
        fps: f64,
    ) -> Option<(MediaTick, MediaTick)> {
        let center = self.detection(address)?.media_tick;
        let radius = MediaTick::from_seconds(2.0, fps);
        Some((
            center.saturating_sub(radius),
            center.saturating_add(radius),
        ))
    }

    pub fn validate(&self) -> Result<(), String> {
        for (line_id, line) in &self.lines {
            line.validate()
                .map_err(|error| format!("line {line_id}: {error}"))?;
        }
        Ok(())
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

/// Reversible semantic operation ready to be wrapped by the application's
/// canonical command/history boundary.
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
    fn media_ticks_round_to_tenths_of_a_frame() {
        assert_eq!(MediaTick::from_frame_position(12.34), MediaTick(123));
        assert_eq!(MediaTick::from_frame_position(12.36), MediaTick(124));
        assert_eq!(MediaTick(127).as_frame_position(), 12.7);
    }

    #[test]
    fn simultaneous_tracks_do_not_share_detections() {
        let mut document = DetectionDocument::default();
        let tick = MediaTick(123);
        let upper = document
            .add_detection(
                10,
                DetectionKind::Labial,
                tick,
                TextAnchor::Grapheme { index: 1 },
            )
            .unwrap();
        let lower = document
            .add_detection(
                20,
                DetectionKind::MouthOpen,
                tick,
                TextAnchor::Grapheme { index: 0 },
            )
            .unwrap();

        assert_eq!(upper.line_id, 10);
        assert_eq!(lower.line_id, 20);
        assert_eq!(document.line(10).unwrap().detections().len(), 1);
        assert_eq!(document.line(20).unwrap().detections().len(), 1);
        assert_eq!(document.detection(upper).unwrap().media_tick, tick);
        assert_eq!(document.detection(lower).unwrap().media_tick, tick);
    }

    #[test]
    fn moving_a_detection_keeps_navigation_sorted() {
        let mut document = DetectionDocument::default();
        let first = document
            .add_detection(
                10,
                DetectionKind::Labial,
                MediaTick(20),
                TextAnchor::Grapheme { index: 0 },
            )
            .unwrap();
        let second = document
            .add_detection(
                10,
                DetectionKind::TeethVisible,
                MediaTick(40),
                TextAnchor::Grapheme { index: 1 },
            )
            .unwrap();

        assert!(document.move_detection(second, MediaTick(10)));
        let line = document.line(10).unwrap();
        assert_eq!(line.detections()[0].id, second.detection_id);
        assert_eq!(line.detection_after(MediaTick(10)).unwrap().id, first.detection_id);
    }

    #[test]
    fn changes_apply_and_unapply_without_touching_other_lines() {
        let mut document = DetectionDocument::default();
        let address = DetectionAddress {
            line_id: 7,
            detection_id: DetectionCueId(1),
        };
        let cue = DetectionCue {
            id: address.detection_id,
            kind: DetectionKind::Reaction,
            media_tick: MediaTick(75),
            target: TextAnchor::AfterText,
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
                1,
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
    fn serde_roundtrip_preserves_line_ownership() {
        let mut document = DetectionDocument::default();
        document
            .add_detection(
                42,
                DetectionKind::Telephone,
                MediaTick(987),
                TextAnchor::GraphemeRange { start: 2, end: 4 },
            )
            .unwrap();

        let json = serde_json::to_string(&document).unwrap();
        let restored: DetectionDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, document);
        assert!(restored.validate().is_ok());
    }
}
