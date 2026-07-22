//! Application use cases for semantic dialogue detection.
//!
//! UI code should emit these operations instead of mutating detection vectors.
//! The returned [`DetectionChange`] values are ready to be wrapped by the
//! canonical project command/history boundary when the editor interaction is
//! connected.

use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionCueId, DetectionDocument,
    DetectionKind, LineDetectionData, MediaTick, TextAnchor,
};

pub struct DetectionEditService;

impl DetectionEditService {
    pub fn add(
        document: &mut DetectionDocument,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) -> Option<(DetectionAddress, DetectionChange)> {
        target.validate().ok()?;
        let detection_id = document
            .line(line_id)
            .map(LineDetectionData::next_detection_id)
            .unwrap_or(Some(DetectionCueId(1)))?;
        let address = DetectionAddress {
            line_id,
            detection_id,
        };
        let cue = DetectionCue {
            id: detection_id,
            kind,
            media_tick,
            duration: MediaTick::ZERO,
            target,
        };
        let change = DetectionChange::Add {
            address,
            cue: cue.clone(),
        };
        change.apply(document).then_some((address, change))
    }

    pub fn remove(
        document: &mut DetectionDocument,
        address: DetectionAddress,
    ) -> Option<DetectionChange> {
        let cue = document.command_cue(address)?;
        let change = DetectionChange::Remove { address, cue };
        change.apply(document).then_some(change)
    }

    pub fn move_to(
        document: &mut DetectionDocument,
        address: DetectionAddress,
        new_tick: MediaTick,
    ) -> Option<DetectionChange> {
        let old_tick = document.command_cue(address)?.media_tick;
        if old_tick == new_tick {
            return None;
        }
        let change = DetectionChange::Move {
            address,
            old_tick,
            new_tick,
        };
        change.apply(document).then_some(change)
    }

    /// Merge a stream of drag or keyboard-step changes into one undoable move.
    pub fn coalesce_move(previous: &mut DetectionChange, next: &DetectionChange) -> bool {
        let (
            DetectionChange::Move {
                address: previous_address,
                new_tick: previous_new_tick,
                ..
            },
            DetectionChange::Move {
                address: next_address,
                new_tick: next_new_tick,
                ..
            },
        ) = (previous, next)
        else {
            return false;
        };
        if previous_address != next_address {
            return false;
        }
        *previous_new_tick = *next_new_tick;
        true
    }

    pub fn remove_line(document: &mut DetectionDocument, line_id: u64) -> Option<DetectionChange> {
        let data = document.line(line_id)?.clone();
        if data.is_empty() {
            return None;
        }
        let change = DetectionChange::RemoveLine { line_id, data };
        change.apply(document).then_some(change)
    }

    pub fn undo(document: &mut DetectionDocument, change: &DetectionChange) -> bool {
        change.unapply(document)
    }

    pub fn redo(document: &mut DetectionDocument, change: &DetectionChange) -> bool {
        change.apply(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_remove_are_reversible() {
        let mut document = DetectionDocument::default();
        let (address, add) = DetectionEditService::add(
            &mut document,
            9,
            DetectionKind::Labial,
            MediaTick(120),
            TextAnchor::Grapheme { index: 0 },
        )
        .unwrap();

        assert!(document.detection(address).is_some());
        assert!(DetectionEditService::undo(&mut document, &add));
        assert!(document.detection(address).is_none());
        assert!(DetectionEditService::redo(&mut document, &add));

        let remove = DetectionEditService::remove(&mut document, address).unwrap();
        assert!(document.detection(address).is_none());
        assert!(DetectionEditService::undo(&mut document, &remove));
        assert!(document.detection(address).is_some());
    }

    #[test]
    fn repeated_moves_coalesce_without_losing_the_original_tick() {
        let mut document = DetectionDocument::default();
        let (address, _) = DetectionEditService::add(
            &mut document,
            3,
            DetectionKind::TeethVisible,
            MediaTick(10),
            TextAnchor::Grapheme { index: 1 },
        )
        .unwrap();
        let mut first =
            DetectionEditService::move_to(&mut document, address, MediaTick(11)).unwrap();
        let second = DetectionEditService::move_to(&mut document, address, MediaTick(12)).unwrap();

        assert!(DetectionEditService::coalesce_move(&mut first, &second));
        assert!(DetectionEditService::undo(&mut document, &first));
        assert_eq!(
            document.detection(address).unwrap().media_tick,
            MediaTick(10)
        );
        assert!(DetectionEditService::redo(&mut document, &first));
        assert_eq!(
            document.detection(address).unwrap().media_tick,
            MediaTick(12)
        );
    }

    #[test]
    fn moves_from_different_lines_never_coalesce() {
        let mut first = DetectionChange::Move {
            address: DetectionAddress {
                line_id: 1,
                detection_id: DetectionCueId(1),
            },
            old_tick: MediaTick(10),
            new_tick: MediaTick(11),
        };
        let second = DetectionChange::Move {
            address: DetectionAddress {
                line_id: 2,
                detection_id: DetectionCueId(1),
            },
            old_tick: MediaTick(10),
            new_tick: MediaTick(12),
        };
        assert!(!DetectionEditService::coalesce_move(&mut first, &second));
    }
}
