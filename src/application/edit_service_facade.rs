//! Application edit boundary with text-aware coalescing.
//!
//! The legacy service remains the implementation for ordinary commands. Text
//! edits are special: every coalesced version is replayed from the command's
//! original snapshot so synchronization cuts are never incrementally detached
//! when a fitted box is emptied and then filled again.

use crate::command::Command;
use crate::export::ProjectData as ImportProjectData;
use crate::packet::{CommandPayload, ProjectData as SyncProjectData};

use super::edit_service_base as base;
use super::project_service::ProjectSession;

pub use base::{EditEffects, EditOrigin};

pub struct EditExecutor;

impl EditExecutor {
    pub fn reset(session: &mut ProjectSession) {
        base::EditExecutor::reset(session);
    }

    pub fn mark_dirty(session: &mut ProjectSession, origin: EditOrigin) -> EditEffects {
        base::EditExecutor::mark_dirty(session, origin)
    }

    pub fn apply_domain_change<F>(
        session: &mut ProjectSession,
        origin: EditOrigin,
        change: F,
    ) -> EditEffects
    where
        F: FnOnce(&mut crate::project::Project),
    {
        base::EditExecutor::apply_domain_change(session, origin, change)
    }

    pub fn apply_import(session: &mut ProjectSession, data: ImportProjectData, fps: f64) {
        base::EditExecutor::apply_import(session, data, fps);
    }

    pub fn apply_subtitle_import(
        session: &mut ProjectSession,
        data: ImportProjectData,
        fps: f64,
    ) -> bool {
        base::EditExecutor::apply_subtitle_import(session, data, fps)
    }

    pub fn record_applied(
        session: &mut ProjectSession,
        command: Command,
        origin: EditOrigin,
    ) -> EditEffects {
        base::EditExecutor::record_applied(session, command, origin)
    }

    pub fn execute(
        session: &mut ProjectSession,
        command: Command,
        origin: EditOrigin,
    ) -> EditEffects {
        base::EditExecutor::execute(session, command, origin)
    }

    pub fn coalesce<F>(
        session: &mut ProjectSession,
        mut command: Command,
        update_last: F,
        origin: EditOrigin,
    ) -> EditEffects
    where
        F: FnOnce(&mut Command),
    {
        let previous = session.history.last().cloned();
        let replay_text = match (&command, previous.as_ref()) {
            (
                Command::UpdateLineText {
                    line_id,
                    new_text,
                    ..
                },
                Some(Command::UpdateLineText {
                    line_id: previous_line_id,
                    old_text: original_text,
                    ..
                }),
            ) if line_id == previous_line_id => {
                Some((*line_id, original_text.clone(), new_text.clone()))
            }
            _ => None,
        };
        if let Some((line_id, original_text, new_text)) = replay_text {
            // Restore both the original text and the original sync-point indices
            // before deriving the next coalesced version. Applying incremental
            // diffs here loses which side of a collapsed pair owns the next
            // typed character.
            previous
                .as_ref()
                .expect("coalescing requires an existing history command")
                .unapply(&mut session.project);
            command = Command::UpdateLineText {
                line_id,
                old_text: original_text,
                new_text,
            };
        }

        command.apply(&mut session.project);
        session.history.update_last(update_last);
        if matches!(origin, EditOrigin::Local | EditOrigin::Import) {
            let coalesced = session
                .history
                .last()
                .cloned()
                .expect("coalescing requires an existing history command");
            session
                .transaction_journal
                .replace_last(coalesced)
                .expect("coalescing must replace the active journal tail");
        }
        Self::mark_dirty(session, origin)
    }

    pub fn create_line(
        session: &mut ProjectSession,
        start_frame: i64,
        duration_frames: i64,
        y_slot: f32,
        text: String,
    ) -> (u64, Command) {
        base::EditExecutor::create_line(
            session,
            start_frame,
            duration_frames,
            y_slot,
            text,
        )
    }

    pub fn undo(session: &mut ProjectSession) -> bool {
        base::EditExecutor::undo(session)
    }

    pub fn redo(session: &mut ProjectSession) -> bool {
        base::EditExecutor::redo(session)
    }

    pub fn apply_remote_payload(
        session: &mut ProjectSession,
        payload: CommandPayload,
        origin: EditOrigin,
    ) {
        base::EditExecutor::apply_remote_payload(session, payload, origin);
    }

    pub fn apply_sync(session: &mut ProjectSession, data: SyncProjectData) {
        base::EditExecutor::apply_sync(session, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{
        DetectionAddress, DetectionChange, DetectionCue, DetectionCueId, DetectionKind, MediaTick,
        TextAnchor,
    };

    fn add_sync(session: &mut ProjectSession, line_id: u64, id: u64, index: u32, tick: i64) {
        let cue = DetectionCue {
            id: DetectionCueId(id),
            kind: DetectionKind::TextSyncPoint,
            media_tick: MediaTick(tick),
            target: TextAnchor::Grapheme { index },
        };
        let address = DetectionAddress {
            line_id,
            detection_id: cue.id,
        };
        assert!(session.project.apply_detection_change(
            &DetectionChange::Add { address, cue },
            true,
        ));
    }

    fn sync_values(session: &ProjectSession, line_id: u64) -> Vec<(u32, MediaTick)> {
        let mut values = session
            .project
            .detections()
            .line(line_id)
            .unwrap()
            .text_sync_cues()
            .map(|cue| (cue.target.grapheme_index().unwrap(), cue.media_tick))
            .collect::<Vec<_>>();
        values.sort_by_key(|(index, tick)| (*index, *tick));
        values
    }

    #[test]
    fn coalesced_delete_and_retype_replays_from_original_sync_indices() {
        let mut session = ProjectSession::new();
        let line_id = session.project.add_line_full(
            0,
            100,
            0.0,
            "abcdefghi".into(),
            String::new(),
            [1.0; 4],
        );
        add_sync(&mut session, line_id, 1, 3, 300);
        add_sync(&mut session, line_id, 2, 6, 700);

        EditExecutor::execute(
            &mut session,
            Command::UpdateLineText {
                line_id,
                old_text: "abcdefghi".into(),
                new_text: "abcghi".into(),
            },
            EditOrigin::Local,
        );

        for (old_text, new_text) in [
            ("abcghi", "abcXghi"),
            ("abcXghi", "abcXYghi"),
            ("abcXYghi", "abcXYZghi"),
        ] {
            let latest = new_text.to_string();
            EditExecutor::coalesce(
                &mut session,
                Command::UpdateLineText {
                    line_id,
                    old_text: old_text.into(),
                    new_text: latest.clone(),
                },
                move |command| {
                    if let Command::UpdateLineText { new_text, .. } = command {
                        *new_text = latest;
                    }
                },
                EditOrigin::Local,
            );
        }

        assert_eq!(session.project.get_line(line_id).unwrap().text, "abcXYZghi");
        assert_eq!(
            sync_values(&session, line_id),
            vec![(3, MediaTick(300)), (6, MediaTick(700))]
        );
        assert_eq!(session.transaction_journal.cursor(), 1);

        assert!(EditExecutor::undo(&mut session));
        assert_eq!(session.project.get_line(line_id).unwrap().text, "abcdefghi");
        assert_eq!(
            sync_values(&session, line_id),
            vec![(3, MediaTick(300)), (6, MediaTick(700))]
        );
        assert!(EditExecutor::redo(&mut session));
        assert_eq!(session.project.get_line(line_id).unwrap().text, "abcXYZghi");
        assert_eq!(
            sync_values(&session, line_id),
            vec![(3, MediaTick(300)), (6, MediaTick(700))]
        );
    }
}
