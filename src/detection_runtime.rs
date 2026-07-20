//! Runtime bridge between the existing rythmo detection UI and the semantic model.
//!
//! This module intentionally keeps the bridge small: the editor can place,
//! select, move, nudge and delete cues now. Durable archive/collaboration
//! storage can replace the registry without changing the UI contract.

use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::application::detection_service::DetectionEditService;
use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::command::Command;
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionDocument, DetectionKind, MediaTick, TextAnchor,
};
use crate::project::Project;
use crate::state::State;
use crate::workspaces::rythmo::view::Selection;

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
    /// Editor-facing semantic registry. The returned guard is deliberately
    /// short-lived at call sites (`project.detections().line(...)`).
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
                // `get_line_mut` invalidates the project revision even though
                // the line payload itself is not touched by this bridge.
                let _ = self.get_line_mut(address.line_id);
            }
        }
        changed
    }
}

impl State {
    pub fn add_detection(
        &mut self,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) -> bool {
        if self.project_session.project.get_line(line_id).is_none() {
            return false;
        }
        let result = {
            let mut document = self.project_session.project.detections_mut();
            DetectionEditService::add(&mut document, line_id, kind, media_tick, target)
        };
        let Some((address, change)) = result else {
            return false;
        };
        let command = Command::Detection { change };
        EditExecutor::record_applied(&mut self.project_session, command, EditOrigin::Local);
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
        true
    }

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) -> bool {
        let change = {
            let mut document = self.project_session.project.detections_mut();
            DetectionEditService::move_to(&mut document, address, media_tick)
        };
        let Some(change) = change else {
            return false;
        };

        let can_coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection {
                change: DetectionChange::Move { address: previous, .. }
            }) if *previous == address
        );
        if can_coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                Command::Detection {
                    change: change.clone(),
                },
                |last| {
                    if let Command::Detection { change: previous } = last {
                        let _ = DetectionEditService::coalesce_move(previous, &change);
                    }
                },
                EditOrigin::Local,
            );
        } else {
            EditExecutor::record_applied(
                &mut self.project_session,
                Command::Detection { change },
                EditOrigin::Local,
            );
        }
        true
    }

    pub fn delete_detection(&mut self, address: DetectionAddress) -> bool {
        let change = {
            let mut document = self.project_session.project.detections_mut();
            DetectionEditService::remove(&mut document, address)
        };
        let Some(change) = change else {
            return false;
        };
        EditExecutor::record_applied(
            &mut self.project_session,
            Command::Detection { change },
            EditOrigin::Local,
        );
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        true
    }

    pub fn nudge_selected_detection(&mut self, delta_ticks: i64) -> bool {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return false;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return false;
        };
        let current = {
            let document = self.project_session.project.detections();
            document.detection(address).map(|cue| cue.media_tick)
        };
        let Some(current) = current else {
            return false;
        };
        let minimum = MediaTick::from_frame(line.start_frame);
        let maximum = MediaTick::from_frame(line.end_frame());
        self.move_detection(
            address,
            MediaTick(current.0.saturating_add(delta_ticks)).clamp(minimum, maximum),
        )
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
