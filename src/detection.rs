//! Track-scoped source detection and line-scoped text synchronization.
//!
//! Professional detection signs belong to a rythmo track and an absolute source
//! media position. They do not require a dialogue line. Text synchronization is
//! represented by an internal cue attached to a dialogue grapheme, so it shares
//! the same reversible command and persistence path without appearing in the
//! detector palette.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn is_sync_punctuation(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '…' | '—' | '–' | '«' | '»' | '“' | '”' | '‘' | '’' | '‹' | '›'
        )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAffinity {
    #[default]
    Auto,
    Left,
    Right,
}

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
    Grapheme {
        index: u32,
    },
    /// End-exclusive range.
    GraphemeRange {
        start: u32,
        end: u32,
    },
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
    /// Boundary before grapheme `grapheme_boundary`. The start and end are
    /// implicit anchors and are never stored as internal points.
    pub grapheme_boundary: u32,
    #[serde(default, skip_serializing_if = "SyncAffinity::is_auto")]
    pub affinity: SyncAffinity,
    /// Absolute source-media position. The historical field name is retained
    /// for project-format compatibility.
    pub line_tick: MediaTick,
}


impl SyncAffinity {
    pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

/// Semantic data owned by either a real dialogue line or a synthetic track
/// bucket.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LineDetectionData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub original_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    detections: Vec<DetectionCue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sync_points: Vec<TextSyncPoint>,
}

#[derive(Deserialize)]
struct LineDetectionDataWire {
    #[serde(default)]
    original_text: String,
    #[serde(default)]
    detections: Vec<DetectionCue>,
    #[serde(default)]
    sync_points: Vec<TextSyncPoint>,
}

impl<'de> Deserialize<'de> for LineDetectionData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = LineDetectionDataWire::deserialize(deserializer)?;
        let mut detections = Vec::with_capacity(wire.detections.len());
        let mut sync_points = wire.sync_points;
        for cue in wire.detections {
            if cue.kind.is_sync_point() {
                if let Some(grapheme_boundary) = cue.target.grapheme_index() {
                    let id = SyncPointId(cue.id.0);
                    if !sync_points.iter().any(|point| point.id == id) {
                        sync_points.push(TextSyncPoint {
                            id,
                            grapheme_boundary,
                            affinity: SyncAffinity::Auto,
                            line_tick: cue.media_tick,
                        });
                    }
                }
            } else {
                detections.push(cue);
            }
        }
        sync_points.sort_by_key(|point| (point.grapheme_boundary, point.line_tick, point.id));
        Ok(Self {
            original_text: wire.original_text,
            detections,
            sync_points,
        })
    }
}

impl LineDetectionData {
    pub fn is_empty(&self) -> bool {
        self.original_text.is_empty() && self.detections.is_empty() && self.sync_points.is_empty()
    }

    pub fn detections(&self) -> &[DetectionCue] {
        &self.detections
    }

    pub fn source_detections(&self) -> impl Iterator<Item = &DetectionCue> {
        self.detections
            .iter()
            .filter(|cue| !cue.kind.is_sync_point())
    }

    pub fn sync_points(&self) -> &[TextSyncPoint] {
        &self.sync_points
    }

    /// Shift line-scoped synchronization points when their owning dialogue is
    /// copied to another position on the media timeline.
    pub fn shift_sync_points(&mut self, delta: MediaTick) {
        for point in &mut self.sync_points {
            point.line_tick = point.line_tick.saturating_add(delta);
        }
    }

    pub fn sync_point(&self, id: SyncPointId) -> Option<&TextSyncPoint> {
        self.sync_points.iter().find(|point| point.id == id)
    }

    pub fn detection(&self, id: DetectionCueId) -> Option<&DetectionCue> {
        self.detections.iter().find(|cue| cue.id == id)
    }

    pub fn next_detection_id(&self) -> Option<DetectionCueId> {
        self.detections
            .iter()
            .map(|cue| cue.id.0)
            .chain(self.sync_points.iter().map(|point| point.id.0))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(DetectionCueId)
    }

    pub fn insert_detection(&mut self, cue: DetectionCue) -> bool {
        if cue.kind.is_sync_point() {
            let Some(grapheme_boundary) = cue.target.grapheme_index() else {
                return false;
            };
            return self.insert_sync_point(TextSyncPoint {
                id: SyncPointId(cue.id.0),
                grapheme_boundary,
                affinity: SyncAffinity::Auto,
                line_tick: cue.media_tick,
            });
        }
        if self.detection(cue.id).is_some() || cue.target.validate().is_err() {
            return false;
        }
        self.detections.push(cue);
        self.sort_detections();
        true
    }

    pub fn remove_detection(&mut self, id: DetectionCueId) -> Option<DetectionCue> {
        if let Some(index) = self.sync_points.iter().position(|point| point.id.0 == id.0) {
            return Some(self.sync_points.remove(index).as_detection_cue());
        }
        let index = self.detections.iter().position(|cue| cue.id == id)?;
        Some(self.detections.remove(index))
    }

    pub fn move_detection(&mut self, id: DetectionCueId, media_tick: MediaTick) -> bool {
        if let Some(point) = self.sync_points.iter_mut().find(|point| point.id.0 == id.0) {
            if point.line_tick == media_tick {
                return false;
            }
            point.line_tick = media_tick;
            self.sort_sync_points();
            return true;
        }
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
        let mut previous: Option<&TextSyncPoint> = None;
        for point in &self.sync_points {
            if detection_ids.contains(&DetectionCueId(point.id.0)) {
                return Err(format!("detection and sync point share id {}", point.id.0));
            }
            if !sync_ids.insert(point.id) {
                return Err(format!("duplicate sync point id {}", point.id.0));
            }
            if let Some(previous) = previous {
                // Two persistent temporal limits may temporarily share an
                // anchor when the text between them has been deleted. Keep
                // their time order; a later insertion can separate the text
                // anchors again without losing either point.
                if point.grapheme_boundary < previous.grapheme_boundary
                    || point.line_tick <= previous.line_tick
                {
                    return Err(
                        "sync point text order must be monotonic and time order strictly increasing"
                            .into(),
                    );
                }
            }
            previous = Some(point);
        }
        Ok(())
    }

    pub fn validate_sync_points(
        &self,
        grapheme_count: usize,
        line_start: MediaTick,
        line_end: MediaTick,
    ) -> Result<(), String> {
        for point in &self.sync_points {
            if grapheme_count > 0 && point.grapheme_boundary as usize >= grapheme_count {
                return Err(format!(
                    "sync point {} has an out-of-bounds grapheme anchor {}",
                    point.id.0, point.grapheme_boundary
                ));
            }
            if point.line_tick <= line_start || point.line_tick >= line_end {
                return Err(format!(
                    "sync point {} must be strictly inside the line",
                    point.id.0
                ));
            }
        }
        self.validate()
    }

    fn sort_detections(&mut self) {
        self.detections.sort_by_key(|cue| (cue.media_tick, cue.id));
    }

    fn sort_sync_points(&mut self) {
        self.sync_points
            .sort_by_key(|point| (point.grapheme_boundary, point.line_tick, point.id));
    }

    fn insert_sync_point(&mut self, point: TextSyncPoint) -> bool {
        if self.sync_point(point.id).is_some()
            || self.detections.iter().any(|cue| cue.id.0 == point.id.0)
            || self.sync_points.iter().any(|existing| {
                existing.grapheme_boundary == point.grapheme_boundary
                    && existing.line_tick == point.line_tick
            })
        {
            return false;
        }
        self.sync_points.push(point);
        self.sort_sync_points();
        true
    }
}

impl TextSyncPoint {
    pub fn as_detection_cue(&self) -> DetectionCue {
        DetectionCue {
            id: DetectionCueId(self.id.0),
            kind: DetectionKind::TextSyncPoint,
            media_tick: self.line_tick,
            target: TextAnchor::Grapheme {
                index: self.grapheme_boundary,
            },
        }
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

    pub fn line_mut_if_present(&mut self, line_id: u64) -> Option<&mut LineDetectionData> {
        self.lines.get_mut(&line_id)
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

    pub fn sync_point(&self, address: DetectionAddress) -> Option<&TextSyncPoint> {
        self.line(address.line_id)?
            .sync_point(SyncPointId(address.detection_id.0))
    }

    /// Returns the command-compatible cue representation for either a source
    /// detection or a synchronization point. Synchronization is stored only in
    /// `sync_points`; this conversion does not create a second persisted copy.
    pub fn command_cue(&self, address: DetectionAddress) -> Option<DetectionCue> {
        self.detection(address).cloned().or_else(|| {
            self.sync_point(address)
                .map(TextSyncPoint::as_detection_cue)
        })
    }

    pub fn add_sync_point(
        &mut self,
        line_id: u64,
        grapheme_count: usize,
        line_start: MediaTick,
        line_end: MediaTick,
        grapheme_boundary: u32,
        media_tick: MediaTick,
    ) -> Option<DetectionAddress> {
        if grapheme_count == 0
            || grapheme_boundary as usize >= grapheme_count
            || media_tick <= line_start
            || media_tick >= line_end
        {
            return None;
        }
        let data = self.line_mut(line_id);
        let id = data.next_detection_id()?;
        let previous = data
            .sync_points()
            .iter()
            .filter(|point| point.grapheme_boundary < grapheme_boundary)
            .max_by_key(|point| point.grapheme_boundary);
        let next = data
            .sync_points()
            .iter()
            .filter(|point| point.grapheme_boundary > grapheme_boundary)
            .min_by_key(|point| point.grapheme_boundary);
        if previous.is_some_and(|point| point.line_tick >= media_tick)
            || next.is_some_and(|point| point.line_tick <= media_tick)
        {
            return None;
        }
        let inserted = data.insert_sync_point(TextSyncPoint {
            id: SyncPointId(id.0),
            grapheme_boundary,
            affinity: SyncAffinity::Auto,
            line_tick: media_tick,
        });
        inserted.then_some(DetectionAddress {
            line_id,
            detection_id: id,
        })
    }

    pub fn move_sync_point(
        &mut self,
        address: DetectionAddress,
        line_start: MediaTick,
        line_end: MediaTick,
        media_tick: MediaTick,
    ) -> bool {
        if media_tick <= line_start || media_tick >= line_end {
            return false;
        }
        let Some(data) = self.lines.get_mut(&address.line_id) else {
            return false;
        };
        let Some(current) = data
            .sync_point(SyncPointId(address.detection_id.0))
            .cloned()
        else {
            return false;
        };
        let minimum = data
            .sync_points()
            .iter()
            .filter(|point| point.grapheme_boundary < current.grapheme_boundary)
            .map(|point| point.line_tick)
            .max()
            .unwrap_or(line_start);
        let maximum = data
            .sync_points()
            .iter()
            .filter(|point| point.grapheme_boundary > current.grapheme_boundary)
            .map(|point| point.line_tick)
            .min()
            .unwrap_or(line_end);
        if media_tick <= minimum || media_tick >= maximum {
            return false;
        }
        data.move_detection(address.detection_id, media_tick)
    }

    pub fn retarget_sync_point(
        &mut self,
        address: DetectionAddress,
        grapheme_boundary: u32,
    ) -> bool {
        let Some(data) = self.lines.get_mut(&address.line_id) else {
            return false;
        };
        let Some(current) = data
            .sync_point(SyncPointId(address.detection_id.0))
            .cloned()
        else {
            return false;
        };
        if current.grapheme_boundary == grapheme_boundary
            || data
                .sync_points()
                .iter()
                .any(|point| point.id != current.id && point.grapheme_boundary == grapheme_boundary)
        {
            return false;
        }
        let previous = data
            .sync_points()
            .iter()
            .filter(|point| point.line_tick < current.line_tick)
            .max_by_key(|point| point.line_tick);
        let next = data
            .sync_points()
            .iter()
            .filter(|point| point.line_tick > current.line_tick)
            .min_by_key(|point| point.line_tick);
        if previous.is_some_and(|point| point.grapheme_boundary >= grapheme_boundary)
            || next.is_some_and(|point| point.grapheme_boundary <= grapheme_boundary)
        {
            return false;
        }
        let point = data
            .sync_points
            .iter_mut()
            .find(|point| point.id == current.id)
            .expect("point was resolved above");
        point.grapheme_boundary = grapheme_boundary;
        data.sort_sync_points();
        true
    }

    pub fn set_sync_affinity(
        &mut self,
        address: DetectionAddress,
        affinity: SyncAffinity,
    ) -> bool {
        let Some(point) = self
            .lines
            .get_mut(&address.line_id)
            .and_then(|line| line.sync_points.iter_mut().find(|point| point.id.0 == address.detection_id.0))
        else {
            return false;
        };
        if point.affinity == affinity {
            return false;
        }
        point.affinity = affinity;
        true
    }

    /// Deterministically preserves temporal limits across a single contiguous
    /// text edit. A point inside deleted/replaced text is reattached to the
    /// corresponding boundary of the replacement; it is never discarded just
    /// because its former grapheme disappeared.
    pub fn rebase_sync_points(&mut self, line_id: u64, old_text: &str, new_text: &str) {
        let old = UnicodeSegmentation::graphemes(old_text, true).collect::<Vec<_>>();
        let new = UnicodeSegmentation::graphemes(new_text, true).collect::<Vec<_>>();
        let prefix = old
            .iter()
            .zip(&new)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        let old_changed_end = old.len().saturating_sub(suffix);
        let delta = new.len() as i64 - old.len() as i64;
        let Some(data) = self.lines.get_mut(&line_id) else {
            return;
        };
        let new_changed_end = new.len().saturating_sub(suffix);
        let shared_insertion_anchors = if old_changed_end == prefix && new_changed_end > prefix {
            data.sync_points
                .iter()
                .filter(|point| point.grapheme_boundary as usize == prefix)
                .map(|point| point.id)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for point in &mut data.sync_points {
            let boundary = point.grapheme_boundary as usize;
            if shared_insertion_anchors.len() > 1 && boundary == prefix {
                let rank = shared_insertion_anchors
                    .iter()
                    .position(|id| *id == point.id)
                    .unwrap_or(0);
                let slots = shared_insertion_anchors.len() - 1;
                let inserted_span = new_changed_end - prefix;
                point.grapheme_boundary =
                    (prefix + (rank * inserted_span + slots / 2) / slots) as u32;
            } else if boundary > prefix && boundary < old_changed_end {
                let old_span = old_changed_end.saturating_sub(prefix).max(1);
                let new_span = new_changed_end.saturating_sub(prefix);
                let offset = boundary.saturating_sub(prefix);
                point.grapheme_boundary =
                    (prefix + (offset * new_span + old_span / 2) / old_span) as u32;
            } else if boundary >= old_changed_end {
                point.grapheme_boundary =
                    (point.grapheme_boundary as i64 + delta).max(0) as u32;
            }
            if new.is_empty() {
                point.grapheme_boundary = 0;
            } else {
                point.grapheme_boundary = point
                    .grapheme_boundary
                    .min(new.len().saturating_sub(1) as u32);
            }
        }
        data.sort_sync_points();
        self.prune_empty_line(line_id);
    }

    pub fn ambiguous_sync_point_count(
        &self,
        line_id: u64,
        old_text: &str,
        new_text: &str,
    ) -> usize {
        let _ = (line_id, old_text, new_text);
        0
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
            cue.media_tick
                .saturating_sub(radius)
                .clamp(MediaTick::ZERO, cue.media_tick),
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
            .sync_points()
            .iter()
            .map(|point| {
                (
                    point.grapheme_boundary as usize,
                    point.line_tick,
                    point.affinity,
                )
            })
            .collect::<Vec<_>>();
        if points.is_empty() {
            return base_ratios.to_vec();
        }

        let mut char_start = 0usize;
        let grapheme_spans = UnicodeSegmentation::graphemes(text, true)
            .map(|grapheme| {
                let char_end = char_start + grapheme.chars().count();
                let span = (char_start, char_end);
                char_start = char_end;
                span
            })
            .collect::<Vec<_>>();
        let char_count = char_start;
        if grapheme_spans.is_empty() {
            return base_ratios.to_vec();
        }
        let text_characters = text.chars().collect::<Vec<_>>();
        points.sort_by_key(|(index, tick, _)| (*index, *tick));

        // Once explicit points exist they are the timing source of truth.
        // Legacy syllable ratios only establish segment count for compatibility.
        // Every extended grapheme occupies one uniform source span.
        let line_start = MediaTick::from_frame(line_start_frame);
        let duration_ticks = MediaTick::from_frame(duration_frames).raw().max(1) as f32;
        let mut controls = vec![(0.0_f32, 0.0_f32)];
        for (grapheme_index, tick, affinity) in points {
            if grapheme_index >= grapheme_spans.len() {
                continue;
            }
            let (start, end) = grapheme_spans[grapheme_index];
            let punctuation = text_characters[start..end]
                .iter()
                .all(|character| is_sync_punctuation(*character));
            // Letters open their interval; terminal punctuation closes the
            // preceding one. Thus a point on `a` keeps "are" together while a
            // point on `,` keeps "You two," together.
            let source_boundary = match affinity {
                SyncAffinity::Left => grapheme_index + 1,
                SyncAffinity::Right => grapheme_index,
                SyncAffinity::Auto if punctuation => grapheme_index + 1,
                SyncAffinity::Auto => grapheme_index,
            };
            let source = source_boundary as f32 / grapheme_spans.len() as f32;
            let relative = tick.raw().saturating_sub(line_start.raw());
            let target = (relative as f32 / duration_ticks).clamp(0.0, 1.0);
            controls.push((source, target));
        }
        controls.push((1.0, 1.0));
        controls.sort_by(|a, b| a.0.total_cmp(&b.0));
        controls.dedup_by(|a, b| (a.0 - b.0).abs() < 0.000_01);
        enforce_monotonic_controls(&mut controls);

        let mut source_boundaries = Vec::with_capacity(base_ratios.len() + 1);
        source_boundaries.push(0.0_f32);
        for segment in 0..base_ratios.len() {
            let char_boundary = breaks
                .get(segment)
                .copied()
                .unwrap_or(char_count)
                .min(char_count);
            source_boundaries.push(source_ratio_for_char_boundary(
                char_boundary,
                &grapheme_spans,
            ));
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

fn source_ratio_for_char_boundary(boundary: usize, grapheme_spans: &[(usize, usize)]) -> f32 {
    let count = grapheme_spans.len().max(1) as f32;
    for (index, (start, end)) in grapheme_spans.iter().copied().enumerate() {
        if boundary <= start {
            return index as f32 / count;
        }
        if boundary < end {
            let scalar_count = end.saturating_sub(start).max(1) as f32;
            let local = boundary.saturating_sub(start) as f32 / scalar_count;
            return (index as f32 + local) / count;
        }
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
    Retarget {
        address: DetectionAddress,
        old_boundary: u32,
        new_boundary: u32,
    },
    SetAffinity {
        address: DetectionAddress,
        old_affinity: SyncAffinity,
        new_affinity: SyncAffinity,
    },
    RemoveLine {
        line_id: u64,
        data: LineDetectionData,
    },
}

impl DetectionChange {
    pub const fn line_id(&self) -> u64 {
        match self {
            Self::Add { address, .. }
            | Self::Remove { address, .. }
            | Self::Move { address, .. }
            | Self::Retarget { address, .. }
            | Self::SetAffinity { address, .. } => address.line_id,
            Self::RemoveLine { line_id, .. } => *line_id,
        }
    }

    pub fn apply(&self, document: &mut DetectionDocument) -> bool {
        match self {
            Self::Add { address, cue } => document.insert_detection(*address, cue.clone()),
            Self::Remove { address, .. } => document.remove_detection(*address).is_some(),
            Self::Move {
                address, new_tick, ..
            } => document.move_detection(*address, *new_tick),
            Self::Retarget {
                address,
                new_boundary,
                ..
            } => document.retarget_sync_point(*address, *new_boundary),
            Self::SetAffinity {
                address,
                new_affinity,
                ..
            } => document.set_sync_affinity(*address, *new_affinity),
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
            Self::Retarget {
                address,
                old_boundary,
                ..
            } => document.retarget_sync_point(*address, *old_boundary),
            Self::SetAffinity {
                address,
                old_affinity,
                ..
            } => document.set_sync_affinity(*address, *old_affinity),
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
        assert_eq!(document.line(42).unwrap().sync_points().len(), 1);
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

    #[test]
    fn legacy_sync_cues_migrate_to_the_primary_sync_point_vector() {
        let json = r#"{
            "detections":[{
                "id":7,
                "kind":"text_sync_point",
                "media_tick":125,
                "target":{"kind":"grapheme","index":2}
            }]
        }"#;
        let data: LineDetectionData = serde_json::from_str(json).unwrap();
        assert!(data.detections().is_empty());
        assert_eq!(
            data.sync_points(),
            &[TextSyncPoint {
                id: SyncPointId(7),
                grapheme_boundary: 2,
                affinity: SyncAffinity::Auto,
                line_tick: MediaTick(125),
            }]
        );
        let saved = serde_json::to_value(&data).unwrap();
        assert!(saved.get("detections").is_none());
        assert_eq!(saved["sync_points"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn sync_point_validity_enforces_line_bounds_and_matching_order() {
        let mut document = DetectionDocument::default();
        let start = MediaTick::from_frame(10);
        let end = MediaTick::from_frame(20);
        assert!(document
            .add_sync_point(4, 5, start, end, 1, start)
            .is_none());
        let first = document
            .add_sync_point(4, 5, start, end, 1, MediaTick::from_frame(13))
            .unwrap();
        assert!(document
            .add_sync_point(4, 5, start, end, 3, MediaTick::from_frame(12))
            .is_none());
        let second = document
            .add_sync_point(4, 5, start, end, 3, MediaTick::from_frame(17))
            .unwrap();
        assert!(!document.move_sync_point(second, start, end, MediaTick::from_frame(12)));
        assert!(!document.retarget_sync_point(first, 4));
        assert!(document.validate().is_ok());
    }

    #[test]
    fn text_insertions_shift_only_following_grapheme_anchors() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_sync_point(
                9,
                4,
                MediaTick::ZERO,
                MediaTick::from_frame(20),
                2,
                MediaTick::from_frame(10),
            )
            .unwrap();
        document.rebase_sync_points(9, "Aïe!", "A très ïe!");
        assert_eq!(document.sync_point(address).unwrap().grapheme_boundary, 8);
    }

    #[test]
    fn deleting_an_anchored_grapheme_preserves_the_sync_point() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_sync_point(
                10,
                6,
                MediaTick::ZERO,
                MediaTick::from_frame(30),
                3,
                MediaTick::from_frame(15),
            )
            .unwrap();

        document.rebase_sync_points(10, "abcdef", "abef");

        let point = document
            .sync_point(address)
            .expect("the temporal limit must survive deletion of its grapheme");
        assert_eq!(point.grapheme_boundary, 2);
        assert_eq!(point.line_tick, MediaTick::from_frame(15));
    }

    #[test]
    fn rewriting_a_deleted_sync_zone_restores_both_limits() {
        let mut document = DetectionDocument::default();
        let start = document
            .add_sync_point(
                11,
                6,
                MediaTick::ZERO,
                MediaTick::from_frame(30),
                2,
                MediaTick::from_frame(10),
            )
            .unwrap();
        let end = document
            .add_sync_point(
                11,
                6,
                MediaTick::ZERO,
                MediaTick::from_frame(30),
                4,
                MediaTick::from_frame(20),
            )
            .unwrap();

        document.rebase_sync_points(11, "abcdef", "abef");
        assert_eq!(document.sync_point(start).unwrap().grapheme_boundary, 2);
        assert_eq!(document.sync_point(end).unwrap().grapheme_boundary, 2);

        document.rebase_sync_points(11, "abef", "abcdef");
        assert_eq!(document.sync_point(start).unwrap().grapheme_boundary, 2);
        assert_eq!(document.sync_point(end).unwrap().grapheme_boundary, 4);
    }

    #[test]
    fn warped_ratios_keep_implicit_line_anchors_and_ignore_legacy_weights() {
        let mut document = DetectionDocument::default();
        document
            .add_sync_point(
                12,
                4,
                MediaTick::ZERO,
                MediaTick::from_frame(100),
                2,
                MediaTick::from_frame(75),
            )
            .unwrap();
        let ratios = document.warped_ratios(12, "abcd", &[2], &[0.9, 0.1], 0, 100);
        assert_eq!(ratios.len(), 2);
        assert!((ratios.iter().sum::<f32>() - 1.0).abs() < 0.0001);
        // A letter opens the interval on its right.
        assert!((ratios[0] - 0.75).abs() < 0.0001);
        assert!((ratios[1] - 0.25).abs() < 0.0001);
    }

    #[test]
    fn warped_ratios_treat_combining_sequences_as_single_graphemes() {
        let mut document = DetectionDocument::default();
        document
            .add_sync_point(
                13,
                3,
                MediaTick::ZERO,
                MediaTick::from_frame(100),
                1,
                MediaTick::from_frame(90),
            )
            .unwrap();
        let text = "e\u{301}👨‍👩‍👧‍👦P";
        let first_grapheme_char_end = 2;
        let ratios =
            document.warped_ratios(13, text, &[first_grapheme_char_end], &[0.5, 0.5], 0, 100);
        assert_eq!(ratios.len(), 2);
        assert!(ratios.windows(2).all(|pair| pair[0] > 0.0 && pair[1] > 0.0));
        assert!((ratios[0] - 0.9).abs() < 0.0001);
        assert!((ratios.iter().sum::<f32>() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn retarget_changes_are_reversible() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_sync_point(
                77,
                6,
                MediaTick::ZERO,
                MediaTick::from_frame(60),
                2,
                MediaTick::from_frame(30),
            )
            .unwrap();
        let change = DetectionChange::Retarget {
            address,
            old_boundary: 2,
            new_boundary: 4,
        };
        assert!(change.apply(&mut document));
        assert_eq!(document.sync_point(address).unwrap().grapheme_boundary, 4);
        assert!(change.unapply(&mut document));
        assert_eq!(document.sync_point(address).unwrap().grapheme_boundary, 2);
        assert!(change.apply(&mut document));
        assert_eq!(document.sync_point(address).unwrap().grapheme_boundary, 4);
    }

    #[test]
    fn sync_affinity_changes_are_reversible() {
        let mut document = DetectionDocument::default();
        let address = document
            .add_sync_point(
                78,
                6,
                MediaTick::ZERO,
                MediaTick::from_frame(60),
                2,
                MediaTick::from_frame(30),
            )
            .unwrap();
        let change = DetectionChange::SetAffinity {
            address,
            old_affinity: SyncAffinity::Auto,
            new_affinity: SyncAffinity::Left,
        };

        assert!(change.apply(&mut document));
        assert_eq!(document.sync_point(address).unwrap().affinity, SyncAffinity::Left);
        assert!(change.unapply(&mut document));
        assert_eq!(document.sync_point(address).unwrap().affinity, SyncAffinity::Auto);
    }
}
