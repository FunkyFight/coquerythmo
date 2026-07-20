//! Runtime bridge between the existing rythmo detection UI and the semantic model.
//!
//! The editor already owns the interaction and command routing. This module
//! supplies the missing semantic registry and professional sign catalogue.

use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::detection::{DetectionAddress, DetectionChange, DetectionDocument, DetectionKind, MediaTick};
use crate::project::Project;

static DETECTION_DOCUMENT: OnceLock<Mutex<DetectionDocument>> = OnceLock::new();

fn registry() -> &'static Mutex<DetectionDocument> {
    DETECTION_DOCUMENT.get_or_init(|| Mutex::new(DetectionDocument::default()))
}

fn lock_registry() -> MutexGuard<'static, DetectionDocument> {
    registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl MediaTick {
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

impl DetectionKind {
    pub const ALL: [Self; 17] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpen,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::Breath,
        Self::Reaction,
        Self::SentenceStart,
        Self::SentenceEnd,
        Self::OverlapStart,
        Self::OverlapEnd,
        Self::SpeakerChange,
        Self::OffScreen,
        Self::VoiceOver,
        Self::Telephone,
        Self::Thought,
        Self::Crowd,
    ];

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Labial => "L",
            Self::SemiLabial => "SL",
            Self::MouthOpen => "O",
            Self::MouthClosed => "F",
            Self::TeethVisible => "D",
            Self::Breath => "R",
            Self::Reaction => "RX",
            Self::SentenceStart => "DÉB",
            Self::SentenceEnd => "FIN",
            Self::OverlapStart => "CH+",
            Self::OverlapEnd => "CH−",
            Self::SpeakerChange => "PERS",
            Self::OffScreen => "HC",
            Self::VoiceOver => "OFF",
            Self::Telephone => "TEL",
            Self::Thought => "PENS",
            Self::Crowd => "FOULE",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Labial => "Labiale",
            Self::SemiLabial => "Semi-labiale",
            Self::MouthOpen => "Bouche ouverte",
            Self::MouthClosed => "Bouche fermée",
            Self::TeethVisible => "Dents visibles",
            Self::Breath => "Respiration",
            Self::Reaction => "Réaction",
            Self::SentenceStart => "Début de phrase",
            Self::SentenceEnd => "Fin de phrase",
            Self::OverlapStart => "Début de chevauchement",
            Self::OverlapEnd => "Fin de chevauchement",
            Self::SpeakerChange => "Changement de personnage",
            Self::OffScreen => "Hors-champ",
            Self::VoiceOver => "Voix off",
            Self::Telephone => "Téléphone",
            Self::Thought => "Pensée",
            Self::Crowd => "Foule",
        }
    }
}

impl DetectionChange {
    fn address(&self) -> Option<DetectionAddress> {
        match self {
            Self::Add { address, .. }
            | Self::Remove { address, .. }
            | Self::Move { address, .. } => Some(*address),
            Self::RemoveLine { .. } => None,
        }
    }
}

impl Project {
    pub fn detections(&self) -> MutexGuard<'static, DetectionDocument> {
        lock_registry()
    }

    pub fn detections_mut(&mut self) -> MutexGuard<'static, DetectionDocument> {
        lock_registry()
    }

    pub fn apply_detection_change(&mut self, change: &DetectionChange, forward: bool) -> bool {
        let changed = {
            let mut document = lock_registry();
            if forward {
                change.apply(&mut document)
            } else {
                change.unapply(&mut document)
            }
        };
        if changed {
            if let Some(address) = change.address() {
                let _ = self.get_line_mut(address.line_id);
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_every_professional_sign() {
        assert_eq!(DetectionKind::ALL.len(), 17);
        assert!(DetectionKind::ALL.contains(&DetectionKind::Labial));
        assert!(DetectionKind::ALL.contains(&DetectionKind::Crowd));
    }

    #[test]
    fn tick_clamping_keeps_cues_inside_their_line() {
        assert_eq!(MediaTick(4).clamp(MediaTick(10), MediaTick(20)), MediaTick(10));
        assert_eq!(MediaTick(25).clamp(MediaTick(10), MediaTick(20)), MediaTick(20));
        assert_eq!(MediaTick(15).clamp(MediaTick(10), MediaTick(20)), MediaTick(15));
    }
}
