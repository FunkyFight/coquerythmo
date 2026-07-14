//! Canonical edit execution boundary.
//!
//! UI gestures, imports and collaboration currently enter the legacy state
//! object through different paths. This service is the shared boundary for
//! document effects that must stay consistent across those paths: history,
//! dirty state and undo/redo. Network transport remains outside the service.

use crate::command::Command;
use crate::export::ProjectData as ImportProjectData;
use crate::packet::{CommandPayload, ProjectData as SyncProjectData};
use crate::project::Character;
use crate::rythmo_line::RythmoMarker;

use super::project_service::ProjectSession;

/// The source of a document mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOrigin {
    Local,
    UndoRedo,
    Remote,
    Import,
    Sync,
}

/// Effects produced when an already-applied command crosses the edit boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditEffects {
    pub recorded_in_history: bool,
    pub marked_dirty: bool,
}

/// Owns the policy for command history and dirty tracking.
pub struct EditExecutor;

impl EditExecutor {
    /// Reset the current document through the canonical edit boundary when a
    /// new project is started. This intentionally clears document history and
    /// dirty state as one operation.
    pub fn reset(session: &mut ProjectSession) {
        session.project = ProjectSession::project_for_ui_language();
        session.render_index = crate::render_index::ProjectRenderIndex::new();
        session.project_path = None;
        session.dirty = false;
        session.history.clear();
        session.loaded_project = None;
    }

    /// Mark a non-reversible domain-side change according to its origin.
    pub fn mark_dirty(session: &mut ProjectSession, origin: EditOrigin) -> EditEffects {
        let marked_dirty = matches!(origin, EditOrigin::Local | EditOrigin::Import);
        if marked_dirty {
            session.dirty = true;
        }
        EditEffects {
            recorded_in_history: false,
            marked_dirty,
        }
    }

    /// Apply a project change that is intentionally not represented by a
    /// reversible command, while keeping dirty/revision policy at this
    /// boundary.
    pub fn apply_domain_change<F>(
        session: &mut ProjectSession,
        origin: EditOrigin,
        change: F,
    ) -> EditEffects
    where
        F: FnOnce(&mut crate::project::Project),
    {
        change(&mut session.project);
        Self::mark_dirty(session, origin)
    }

    /// Apply imported project data through the same origin policy as edits.
    pub fn apply_import(session: &mut ProjectSession, data: ImportProjectData, fps: f64) {
        data.apply_to_project(&mut session.project, fps);
        Self::mark_dirty(session, EditOrigin::Import);
    }

    /// Replace the active language band while preserving the multilingual
    /// collection and its per-language media.
    pub fn apply_subtitle_import(
        session: &mut ProjectSession,
        data: ImportProjectData,
        fps: f64,
    ) -> bool {
        if !data.apply_to_active_language(&mut session.project, fps) {
            return false;
        }
        Self::mark_dirty(session, EditOrigin::Import);
        true
    }

    /// Register a command whose project mutation has already happened.
    pub fn record_applied(
        session: &mut ProjectSession,
        command: Command,
        origin: EditOrigin,
    ) -> EditEffects {
        let should_record = matches!(origin, EditOrigin::Local | EditOrigin::Import);
        let should_mark_dirty = matches!(origin, EditOrigin::Local | EditOrigin::Import);

        if should_record {
            session.history.push(command);
        }
        if should_mark_dirty {
            session.dirty = true;
        }

        EditEffects {
            recorded_in_history: should_record,
            marked_dirty: should_mark_dirty,
        }
    }

    /// Apply a reversible command and register it according to its origin.
    pub fn execute(
        session: &mut ProjectSession,
        command: Command,
        origin: EditOrigin,
    ) -> EditEffects {
        command.apply(&mut session.project);
        Self::record_applied(session, command, origin)
    }

    /// Apply the next state of a command already present in local history and
    /// update that command instead of creating a second history entry.
    pub fn coalesce<F>(
        session: &mut ProjectSession,
        command: Command,
        update_last: F,
        origin: EditOrigin,
    ) -> EditEffects
    where
        F: FnOnce(&mut Command),
    {
        command.apply(&mut session.project);
        session.history.update_last(update_last);
        Self::mark_dirty(session, origin)
    }

    /// Create a line through the edit boundary while preserving the existing
    /// domain factory's character/color defaults.
    pub fn create_line(
        session: &mut ProjectSession,
        start_frame: i64,
        duration_frames: i64,
        y_slot: f32,
        text: String,
    ) -> (u64, Command) {
        let line_id = session
            .project
            .add_line(start_frame, duration_frames, y_slot);
        if !text.is_empty() {
            if let Some(line) = session.project.get_line_mut(line_id) {
                line.text = text;
            }
        }
        let index = session.project.line_index(line_id).unwrap_or(0);
        let snapshot = session
            .project
            .get_line(line_id)
            .cloned()
            .expect("newly created line must be present");
        let command = Command::CreateLine { snapshot, index };
        Self::record_applied(session, command.clone(), EditOrigin::Local);
        (line_id, command)
    }

    /// Undo the most recent local command.
    pub fn undo(session: &mut ProjectSession) -> bool {
        let had_command = session.history.last().is_some();
        session.history.undo(&mut session.project);
        had_command
    }

    /// Redo the most recently undone local command.
    pub fn redo(session: &mut ProjectSession) -> bool {
        let had_command = session.history.can_redo();
        session.history.redo(&mut session.project);
        had_command
    }

    /// Apply a validated forward-only collaboration payload without adding it
    /// to local undo history or rebroadcasting it.
    pub fn apply_remote_payload(
        session: &mut ProjectSession,
        payload: CommandPayload,
        origin: EditOrigin,
    ) {
        if let Some(command) = Self::command_from_payload(session, payload) {
            Self::execute(session, command, origin);
        }
    }

    /// Validate a forward-only wire payload and enrich it with the previous
    /// state required by the reversible domain command.
    fn command_from_payload(session: &ProjectSession, payload: CommandPayload) -> Option<Command> {
        let project = &session.project;
        Some(match payload {
            CommandPayload::CreateLine { line } => Command::CreateLine {
                snapshot: line,
                index: project.line_count(),
            },
            CommandPayload::DeleteLine { line_id } => {
                let snapshot = project.get_line(line_id)?.clone();
                let index = project.line_index(line_id)?;
                Command::DeleteLine { snapshot, index }
            }
            CommandPayload::SplitLine {
                first_line,
                second_line,
                second_index,
            } => {
                let old_line = project.get_line(first_line.id)?.clone();
                let old_index = project.line_index(first_line.id)?;
                Command::SplitLine {
                    old_line,
                    old_index,
                    first_line,
                    second_line,
                    second_index,
                }
            }
            CommandPayload::MoveLine {
                line_id,
                start_frame,
                y_slot,
            } => {
                let line = project.get_line(line_id)?;
                Command::MoveLine {
                    line_id,
                    old_start: line.start_frame,
                    old_y_slot: line.y_slot,
                    new_start: start_frame,
                    new_y_slot: y_slot,
                }
            }
            CommandPayload::MoveLines { lines } => {
                let moves = lines
                    .into_iter()
                    .map(|movement| {
                        let line = project.get_line(movement.line_id)?;
                        Some(crate::command::LineMove {
                            line_id: movement.line_id,
                            old_start: line.start_frame,
                            old_y_slot: line.y_slot,
                            new_start: movement.start_frame,
                            new_y_slot: movement.y_slot,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                Command::MoveLines { moves }
            }
            CommandPayload::ResizeLine {
                line_id,
                start_frame,
                duration_frames,
            } => {
                let line = project.get_line(line_id)?;
                Command::ResizeLine {
                    line_id,
                    old_start: line.start_frame,
                    old_dur: line.duration_frames,
                    new_start: start_frame,
                    new_dur: duration_frames,
                }
            }
            CommandPayload::UpdateLineText { line_id, text } => Command::UpdateLineText {
                line_id,
                old_text: project.get_line(line_id)?.text.clone(),
                new_text: text,
            },
            CommandPayload::UpdateLineNote { line_id, note } => Command::UpdateLineNote {
                line_id,
                old_note: project.get_line(line_id)?.note.clone(),
                new_note: note,
            },
            CommandPayload::SetLineKaraoke {
                line_id,
                karaoke,
                syllable_ratios,
            } => {
                let line = project.get_line(line_id)?;
                Command::SetLineKaraoke {
                    line_id,
                    old_karaoke: line.karaoke,
                    old_ratios: line.syllable_ratios.clone(),
                    new_karaoke: karaoke,
                    new_ratios: syllable_ratios,
                }
            }
            CommandPayload::SetSyllableRatios { line_id, ratios } => {
                let line = project.get_line(line_id)?;
                Command::SetSyllableRatios {
                    line_id,
                    old_ratios: line.syllable_ratios.clone(),
                    new_ratios: ratios,
                }
            }
            CommandPayload::SetCharacter {
                line_id,
                name,
                color,
                voice_actor_names,
            } => {
                let line = project.get_line(line_id)?;
                let new_voice_actor_names = voice_actor_names
                    .unwrap_or_else(|| project.voice_actor_names_for_character(&name, line_id));
                Command::SetCharacter {
                    line_id,
                    old_name: line.character_name.clone(),
                    old_color: line.character_color,
                    old_voice_actor_names: line.voice_actor_names.clone(),
                    new_name: name,
                    new_color: color,
                    new_voice_actor_names,
                }
            }
            CommandPayload::SetCharacterColor { line_id, color } => {
                let line = project.get_line(line_id)?;
                Command::SetCharacterColor {
                    line_id,
                    old_color: line.character_color,
                    new_color: color,
                }
            }
            CommandPayload::RenameCharacter {
                changes,
                known_characters,
            } => Command::RenameCharacter {
                changes,
                old_known_characters: project.known_characters().to_vec(),
                new_known_characters: known_characters
                    .into_iter()
                    .map(|character| Character {
                        name: character.name,
                        color: character.color,
                    })
                    .collect(),
            },
            CommandPayload::SetVoiceActors { changes } => Command::SetVoiceActors { changes },
            CommandPayload::CreateVoiceActor { actor } => Command::CreateVoiceActor { actor },
            CommandPayload::AddMarker { kind, frame } => Command::AddMarker {
                marker: RythmoMarker { kind, frame },
                index: project.marker_count(),
            },
            CommandPayload::RemoveMarker { kind, frame } => {
                let index = project
                    .markers()
                    .iter()
                    .position(|marker| marker.kind == kind && marker.frame == frame)?;
                Command::RemoveMarker {
                    marker: project.marker(index)?.clone(),
                    index,
                }
            }
            CommandPayload::MoveMarker {
                kind,
                old_frame,
                new_frame,
            } => {
                let index = project
                    .markers()
                    .iter()
                    .position(|marker| marker.kind == kind && marker.frame == old_frame)?;
                Command::MoveMarker {
                    index,
                    old_frame,
                    new_frame,
                }
            }
            CommandPayload::AddDrawingStroke { stroke } => Command::AddDrawingStroke { stroke },
            CommandPayload::EraseDrawingStrokes { strokes } => {
                Command::EraseDrawingStrokes { strokes }
            }
            CommandPayload::TransformStrokes {
                stroke_ids,
                new_points,
            } => {
                if stroke_ids.len() != new_points.len() {
                    return None;
                }
                let old_points = stroke_ids
                    .iter()
                    .map(|id| {
                        project
                            .drawing()
                            .get(*id)
                            .map(|stroke| stroke.points.clone())
                    })
                    .collect::<Option<Vec<_>>>()?;
                Command::TransformStrokes {
                    stroke_ids,
                    old_points,
                    new_points,
                }
            }
        })
    }

    /// Merge a full collaboration snapshot without entering local history.
    pub fn apply_sync(session: &mut ProjectSession, data: SyncProjectData) {
        let remote_ids: std::collections::HashSet<u64> =
            data.lines.iter().map(|line| line.id).collect();
        session
            .project
            .retain_lines(|line| remote_ids.contains(&line.id));

        for remote_line in data.lines {
            if let Some(local) = session.project.get_line_mut(remote_line.id) {
                *local = remote_line;
            } else {
                session.project.insert_line(remote_line);
            }
        }

        session.project.set_markers(data.markers);
        session.project.set_known_characters(
            data.known_characters
                .into_iter()
                .map(|character| Character {
                    name: character.name,
                    color: character.color,
                })
                .collect(),
        );
        session.project.set_voice_actors(data.voice_actors);
        session.project.bump_revision();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;

    #[test]
    fn local_edits_are_recorded_and_mark_dirty() {
        let mut session = ProjectSession::new();
        let line_id = session.project.add_line(0, 48, 0.0);
        let snapshot = session
            .project
            .get_line(line_id)
            .cloned()
            .expect("line snapshot");
        let effects = EditExecutor::record_applied(
            &mut session,
            Command::CreateLine { snapshot, index: 0 },
            EditOrigin::Local,
        );

        assert_eq!(
            effects,
            EditEffects {
                recorded_in_history: true,
                marked_dirty: true,
            }
        );
        assert!(session.dirty);
        assert!(session.history.last().is_some());
    }

    #[test]
    fn remote_and_sync_edits_do_not_pollute_local_history() {
        let mut session = ProjectSession::new();
        let line_id = session.project.add_line(0, 48, 0.0);
        let snapshot = session
            .project
            .get_line(line_id)
            .cloned()
            .expect("line snapshot");

        let remote = EditExecutor::record_applied(
            &mut session,
            Command::CreateLine {
                snapshot: snapshot.clone(),
                index: 0,
            },
            EditOrigin::Remote,
        );
        assert_eq!(remote, EditEffects::default());
        assert!(!session.dirty);
        assert!(session.history.last().is_none());

        let sync = EditExecutor::record_applied(
            &mut session,
            Command::CreateLine { snapshot, index: 0 },
            EditOrigin::Sync,
        );
        assert_eq!(sync, EditEffects::default());
        assert!(session.history.last().is_none());
    }

    #[test]
    fn remote_payload_uses_the_reversible_command_without_local_effects() {
        let mut session = ProjectSession::new();
        let line_id =
            session
                .project
                .add_line_full(0, 48, 0.0, "before".into(), "Alice".into(), [1.0; 4]);
        let revision_before = session.project.revision();

        EditExecutor::apply_remote_payload(
            &mut session,
            CommandPayload::UpdateLineText {
                line_id,
                text: "after".into(),
            },
            EditOrigin::Remote,
        );

        assert_eq!(session.project.get_line(line_id).unwrap().text, "after");
        assert!(session.project.revision() > revision_before);
        assert!(!session.dirty);
        assert!(session.history.last().is_none());
    }

    #[test]
    fn created_lines_are_recorded_once_and_can_be_undone_and_redone() {
        let mut session = ProjectSession::new();
        let (line_id, command) =
            EditExecutor::create_line(&mut session, 12, 24, 0.25, "hello".into());

        assert_eq!(session.project.get_line(line_id).unwrap().text, "hello");
        assert!(matches!(command, Command::CreateLine { .. }));
        assert!(session.dirty);
        assert!(session.history.last().is_some());

        assert!(EditExecutor::undo(&mut session));
        assert!(session.project.get_line(line_id).is_none());
        assert!(EditExecutor::redo(&mut session));
        assert_eq!(session.project.get_line(line_id).unwrap().text, "hello");
    }

    #[test]
    fn execute_and_undo_redo_keep_command_semantics() {
        let mut source = Project::new();
        let line_id = source.add_line(0, 48, 0.0);
        let snapshot = source.get_line(line_id).cloned().expect("line snapshot");
        let mut session = ProjectSession::new();

        let _ = EditExecutor::execute(
            &mut session,
            Command::InsertLine { snapshot, index: 0 },
            EditOrigin::Local,
        );
        assert!(session.project.get_line(line_id).is_some());
        assert!(EditExecutor::undo(&mut session));
        assert!(session.project.get_line(line_id).is_none());
        assert!(EditExecutor::redo(&mut session));
        assert!(session.project.get_line(line_id).is_some());
    }

    #[test]
    fn undo_without_history_is_a_noop() {
        let mut session = ProjectSession::new();
        assert!(!EditExecutor::undo(&mut session));
        assert!(!EditExecutor::redo(&mut session));
    }
}
