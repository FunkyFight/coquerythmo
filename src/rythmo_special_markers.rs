//! Non-exported production markers for the interactive bande rythmo.
//!
//! They reuse the reversible detection command path but live in a reserved,
//! non-visible track bucket. Render/export backends that consume ordinary
//! dialogue lines and markers never see them.

use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use crate::project::Project;
use crate::ui::primitives::{EventResponse, Rect, UiAction, UiEvent};
use crate::workspaces::rythmo::view::{RythmoState, Selection};
use std::sync::{Mutex, OnceLock};

const STORAGE_TRACK: u8 = 250;
const HIT_WIDTH: f32 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialMarkerKind {
    Start,
    Bip1000,
    FirstImage,
    LastImage,
}

impl SpecialMarkerKind {
    pub const ALL: [Self; 4] = [
        Self::Start,
        Self::Bip1000,
        Self::FirstImage,
        Self::LastImage,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Start => "START",
            Self::Bip1000 => "1000",
            Self::FirstImage => "P.I",
            Self::LastImage => "D.I",
        }
    }

    pub const fn accessible_name(self) -> &'static str {
        match self {
            Self::Start => "Marqueur Start",
            Self::Bip1000 => "Marqueur bip mille hertz",
            Self::FirstImage => "Marqueur première image",
            Self::LastImage => "Marqueur dernière image",
        }
    }

    pub const fn icon_asset(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Bip1000 => "bip",
            Self::FirstImage => "pi",
            Self::LastImage => "di",
        }
    }

    pub const fn target(self) -> TextAnchor {
        match self {
            Self::Start => TextAnchor::BeforeText,
            Self::Bip1000 => TextAnchor::AfterText,
            Self::FirstImage => TextAnchor::Grapheme { index: 0 },
            Self::LastImage => TextAnchor::GraphemeRange { start: 0, end: 1 },
        }
    }

    pub fn from_target(target: &TextAnchor) -> Option<Self> {
        match target {
            TextAnchor::BeforeText => Some(Self::Start),
            TextAnchor::AfterText => Some(Self::Bip1000),
            TextAnchor::Grapheme { index: 0 } => Some(Self::FirstImage),
            TextAnchor::GraphemeRange { start: 0, end: 1 } => Some(Self::LastImage),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpecialMarker {
    pub address: DetectionAddress,
    pub kind: SpecialMarkerKind,
    pub media_tick: MediaTick,
}

pub const fn storage_line_id() -> u64 {
    track_storage_line_id(STORAGE_TRACK)
}

pub const fn is_special_address(address: DetectionAddress) -> bool {
    address.line_id == storage_line_id()
}

pub fn cue_kind(cue: &DetectionCue) -> Option<SpecialMarkerKind> {
    if cue.kind != DetectionKind::Reaction {
        return None;
    }
    SpecialMarkerKind::from_target(&cue.target)
}

pub fn markers(project: &Project) -> Vec<SpecialMarker> {
    project
        .detections()
        .line(storage_line_id())
        .into_iter()
        .flat_map(|line| line.detections())
        .filter_map(|cue| {
            Some(SpecialMarker {
                address: DetectionAddress {
                    line_id: storage_line_id(),
                    detection_id: cue.id,
                },
                kind: cue_kind(cue)?,
                media_tick: cue.media_tick,
            })
        })
        .collect()
}

pub fn add_action(kind: SpecialMarkerKind) -> UiAction {
    UiAction::AddDetection {
        line_id: storage_line_id(),
        kind: DetectionKind::Reaction,
        media_tick: MediaTick::ZERO,
        target: kind.target(),
    }
}

fn ppf() -> f32 {
    crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

pub fn frame_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x
        + zone.width / 2.0
        + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame).clamp(MediaTick::ZERO, MediaTick(i64::MAX))
}

pub fn hit_test(
    project: &Project,
    x: f32,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> Option<DetectionAddress> {
    if !zone.contains(x, y) {
        return None;
    }
    markers(project)
        .into_iter()
        .min_by(|left, right| {
            let left_distance = (frame_x(left.media_tick, current_frame, zone) - x).abs();
            let right_distance = (frame_x(right.media_tick, current_frame, zone) - x).abs();
            left_distance.total_cmp(&right_distance)
        })
        .filter(|marker| {
            (frame_x(marker.media_tick, current_frame, zone) - x).abs() <= HIT_WIDTH
        })
        .map(|marker| marker.address)
}

fn drag_slot() -> &'static Mutex<Option<DetectionAddress>> {
    static DRAG: OnceLock<Mutex<Option<DetectionAddress>>> = OnceLock::new();
    DRAG.get_or_init(|| Mutex::new(None))
}

fn lock_drag() -> std::sync::MutexGuard<'static, Option<DetectionAddress>> {
    drag_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn handle_event(
    project: &Project,
    state: &mut RythmoState,
    zone: &Rect,
    current_frame: f64,
    event: &UiEvent,
) -> Option<EventResponse> {
    match event {
        UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y } => {
            let address = hit_test(project, *x, *y, current_frame, zone)?;
            state.selected = Some(Selection::Detection(address));
            state.detection_menu = None;
            state.detection_drag = None;
            *lock_drag() = Some(address);
            Some(EventResponse::Consumed)
        }
        UiEvent::MouseMove { x, .. } => {
            let address = *lock_drag();
            address.map(|address| {
                EventResponse::Action(UiAction::MoveDetection {
                    address,
                    media_tick: pointer_tick(*x, current_frame, zone),
                })
            })
        }
        UiEvent::MouseRelease { .. } | UiEvent::MiddleRelease { .. } => {
            lock_drag().take().map(|_| EventResponse::Consumed)
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            lock_drag().take().map(|_| EventResponse::Consumed)
        }
        _ => None,
    }
}

pub fn clear_interaction() {
    *lock_drag() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_markers_round_trip_through_semantic_target() {
        for kind in SpecialMarkerKind::ALL {
            assert_eq!(SpecialMarkerKind::from_target(&kind.target()), Some(kind));
        }
    }

    #[test]
    fn storage_bucket_is_outside_visible_tracks() {
        assert_eq!(DetectionAddress::for_track(STORAGE_TRACK, crate::detection::DetectionCueId(1)).line_id, storage_line_id());
        assert!(STORAGE_TRACK as usize >= crate::rythmo_layout::track_count());
    }
}
