//! Reversible project mutations and their history.
//!
//! The enum shape is intentionally explicit: each variant is a stable
//! snapshot of one user operation.
#![allow(clippy::large_enum_variant)]

use crate::project::{Character, LineCharacterNameChange, Project};
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_line::{RythmoLine, RythmoMarker};
use crate::voice_actor::{LineVoiceActorsChange, VoiceActor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct LineMove {
    pub line_id: u64,
    pub old_start: i64,
    pub old_y_slot: f32,
    pub new_start: i64,
    pub new_y_slot: f32,
}

/// Each command stores before/after state for reversibility.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    CreateLine {
        snapshot: RythmoLine,
        index: usize,
    },
    InsertLine {
        snapshot: RythmoLine,
        index: usize,
    },
    InsertLines {
        lines: Vec<(RythmoLine, usize)>,
    },
    DeleteLine {
        snapshot: RythmoLine,
        index: usize,
    },
    DeleteLines {
        lines: Vec<(RythmoLine, usize)>,
    },
    SplitLine {
        old_line: RythmoLine,
        old_index: usize,
        first_line: RythmoLine,
        second_line: RythmoLine,
        second_index: usize,
    },
    MoveLine {
        line_id: u64,
        old_start: i64,
        old_y_slot: f32,
        new_start: i64,
        new_y_slot: f32,
    },
    MoveLines {
        moves: Vec<LineMove>,
    },
    ResizeLine {
        line_id: u64,
        old_start: i64,
        old_dur: i64,
        new_start: i64,
        new_dur: i64,
    },
    UpdateLineText {
        line_id: u64,
        old_text: String,
        new_text: String,
    },
    SetLineKaraoke {
        line_id: u64,
        old_karaoke: bool,
        old_ratios: Vec<f32>,
        new_karaoke: bool,
        new_ratios: Vec<f32>,
    },
    SetSyllableRatios {
        line_id: u64,
        old_ratios: Vec<f32>,
        new_ratios: Vec<f32>,
    },
    SetCharacter {
        line_id: u64,
        old_name: String,
        old_color: [f32; 4],
        old_voice_actor_names: Vec<String>,
        new_name: String,
        new_color: [f32; 4],
        new_voice_actor_names: Vec<String>,
    },
    SetCharacterColor {
        line_id: u64,
        old_color: [f32; 4],
        new_color: [f32; 4],
    },
    RenameCharacter {
        changes: Vec<LineCharacterNameChange>,
        old_known_characters: Vec<Character>,
        new_known_characters: Vec<Character>,
    },
    SetVoiceActors {
        changes: Vec<LineVoiceActorsChange>,
    },
    CreateVoiceActor {
        actor: VoiceActor,
    },
    AddMarker {
        marker: RythmoMarker,
        index: usize,
    },
    RemoveMarker {
        marker: RythmoMarker,
        index: usize,
    },
    MoveMarker {
        index: usize,
        old_frame: i64,
        new_frame: i64,
    },
    UpdateLineNote {
        line_id: u64,
        old_note: String,
        new_note: String,
    },
    Detection {
        change: crate::detection::DetectionChange,
    },
    AddDrawingStroke {
        stroke: DrawingStroke,
    },
    EraseDrawingStrokes {
        strokes: Vec<DrawingStroke>,
    },
    TransformStrokes {
        stroke_ids: Vec<u64>,
        old_points: Vec<Vec<(f64, f32)>>,
        new_points: Vec<Vec<(f64, f32)>>,
    },
}

impl Command {
    pub(crate) fn apply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { snapshot, index } => {
                project.upsert_line_at(*index, snapshot.clone());
                // Line was already added — nothing to re-apply for redo
            }
            Command::InsertLine { snapshot, index } => {
                project.upsert_line_at(*index, snapshot.clone());
            }
            Command::InsertLines { lines } => {
                let mut lines = lines.clone();
                lines.sort_by_key(|(_, index)| *index);
                for (snapshot, index) in lines {
                    project.upsert_line_at(index, snapshot);
                }
            }
            Command::DeleteLine { snapshot, .. } => {
                project.remove_line(snapshot.id);
            }
            Command::DeleteLines { lines } => {
                for (snapshot, _) in lines {
                    project.remove_line(snapshot.id);
                }
            }
            Command::SplitLine {
                first_line,
                second_line,
                second_index,
                ..
            } => {
                project.remove_line(second_line.id);
                project.upsert_line_at(second_index.saturating_sub(1), first_line.clone());
                project.insert_line_at(*second_index, second_line.clone());
            }
            Command::MoveLine {
                line_id,
                new_start,
                new_y_slot,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *new_start;
                    l.y_slot = *new_y_slot;
                }
            }
            Command::MoveLines { moves } => {
                for movement in moves {
                    if let Some(l) = project.get_line_mut(movement.line_id) {
                        l.start_frame = movement.new_start;
                        l.y_slot = movement.new_y_slot;
                    }
                }
            }
            Command::ResizeLine {
                line_id,
                new_start,
                new_dur,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *new_start;
                    l.duration_frames = *new_dur;
                }
            }
            Command::UpdateLineText {
                line_id,
                old_text,
                new_text,
            } => {
                project.update_line_text_preserving_sync_boxes(*line_id, old_text, new_text);
            }
            Command::SetLineKaraoke {
                line_id,
                new_karaoke,
                new_ratios,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.karaoke = *new_karaoke;
                    l.syllable_ratios = new_ratios.clone();
                }
            }
            Command::SetSyllableRatios {
                line_id,
                new_ratios,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.syllable_ratios = new_ratios.clone();
                }
            }
            Command::SetCharacter {
                line_id,
                new_name,
                new_color,
                new_voice_actor_names,
                ..
            } => {
                project.set_character_with_voice_actors(
                    *line_id,
                    new_name.clone(),
                    *new_color,
                    new_voice_actor_names.clone(),
                );
            }
            Command::SetCharacterColor {
                line_id, new_color, ..
            } => {
                project.set_line_character_color(*line_id, *new_color);
            }
            Command::RenameCharacter {
                changes,
                new_known_characters,
                ..
            } => {
                project.apply_character_name_changes(changes, true);
                project.set_known_characters(new_known_characters.clone());
            }
            Command::SetVoiceActors { changes } => {
                for change in changes {
                    project.set_line_voice_actor_names(
                        change.line_id,
                        change.new_voice_actor_names.clone(),
                    );
                }
            }
            Command::CreateVoiceActor { actor } => {
                project.upsert_voice_actor(actor.clone());
            }
            Command::AddMarker { marker, index } => {
                project.insert_marker(*index, marker.clone());
                // Already added during execute — for redo
            }
            Command::RemoveMarker { index, .. } => {
                let _ = project.remove_marker_at(*index);
            }
            Command::MoveMarker {
                index, new_frame, ..
            } => {
                project.move_marker(*index, *new_frame);
            }
            Command::UpdateLineNote {
                line_id, new_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = new_note.clone();
                }
            }
            Command::Detection { change } => {
                project.apply_detection_change(change, true);
            }
            Command::AddDrawingStroke { stroke } => {
                if project.drawing().get(stroke.id).is_none() {
                    project.add_drawing_stroke(stroke.clone());
                }
            }
            Command::EraseDrawingStrokes { strokes } => {
                let ids: Vec<u64> = strokes.iter().map(|stroke| stroke.id).collect();
                project.remove_drawing_strokes(&ids);
            }
            Command::TransformStrokes {
                stroke_ids,
                new_points,
                ..
            } => {
                project.set_drawing_strokes_points(stroke_ids, new_points);
            }
        }
    }

    pub(crate) fn unapply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { snapshot, .. } => {
                project.remove_line(snapshot.id);
            }
            Command::InsertLine { snapshot, .. } => {
                project.remove_line(snapshot.id);
            }
            Command::InsertLines { lines } => {
                for (snapshot, _) in lines {
                    project.remove_line(snapshot.id);
                }
            }
            Command::DeleteLine { snapshot, index } => {
                project.insert_line_at(*index, snapshot.clone());
            }
            Command::DeleteLines { lines } => {
                let mut lines = lines.clone();
                lines.sort_by_key(|(_, index)| *index);
                for (snapshot, index) in lines {
                    project.insert_line_at(index, snapshot);
                }
            }
            Command::SplitLine {
                old_line,
                old_index,
                second_line,
                ..
            } => {
                project.remove_line(second_line.id);
                project.upsert_line_at(*old_index, old_line.clone());
            }
            Command::AddDrawingStroke { stroke } => {
                if project.drawing().get(stroke.id).is_some() {
                    project.remove_drawing_stroke(stroke.id);
                }
            }
            Command::EraseDrawingStrokes { strokes } => {
                project.add_drawing_strokes(strokes);
            }
            Command::TransformStrokes {
                stroke_ids,
                old_points,
                ..
            } => {
                project.set_drawing_strokes_points(stroke_ids, old_points);
            }
            Command::MoveLine {
                line_id,
                old_start,
                old_y_slot,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *old_start;
                    l.y_slot = *old_y_slot;
                }
            }
            Command::MoveLines { moves } => {
                for movement in moves {
                    if let Some(l) = project.get_line_mut(movement.line_id) {
                        l.start_frame = movement.old_start;
                        l.y_slot = movement.old_y_slot;
                    }
                }
            }
            Command::ResizeLine {
                line_id,
                old_start,
                old_dur,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *old_start;
                    l.duration_frames = *old_dur;
                }
            }
            Command::UpdateLineText {
                line_id,
                old_text,
                new_text,
            } => {
                project.update_line_text_preserving_sync_boxes(*line_id, new_text, old_text);
            }
            Command::SetLineKaraoke {
                line_id,
                old_karaoke,
                old_ratios,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.karaoke = *old_karaoke;
                    l.syllable_ratios = old_ratios.clone();
                }
            }
            Command::SetSyllableRatios {
                line_id,
                old_ratios,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.syllable_ratios = old_ratios.clone();
                }
            }
            Command::SetCharacter {
                line_id,
                old_name,
                old_color,
                old_voice_actor_names,
                ..
            } => {
                project.set_character_with_voice_actors(
                    *line_id,
                    old_name.clone(),
                    *old_color,
                    old_voice_actor_names.clone(),
                );
            }
            Command::SetCharacterColor {
                line_id, old_color, ..
            } => {
                project.set_line_character_color(*line_id, *old_color);
            }
            Command::RenameCharacter {
                changes,
                old_known_characters,
                ..
            } => {
                project.apply_character_name_changes(changes, false);
                project.set_known_characters(old_known_characters.clone());
            }
            Command::SetVoiceActors { changes } => {
                for change in changes {
                    project.set_line_voice_actor_names(
                        change.line_id,
                        change.old_voice_actor_names.clone(),
                    );
                }
            }
            Command::CreateVoiceActor { actor } => {
                project.remove_voice_actor(&actor.name);
            }
            Command::AddMarker { index, .. } => {
                let _ = project.remove_marker_at(*index);
            }
            Command::RemoveMarker { marker, index } => {
                project.insert_marker(*index, marker.clone());
            }
            Command::MoveMarker {
                index, old_frame, ..
            } => {
                project.move_marker(*index, *old_frame);
            }
            Command::UpdateLineNote {
                line_id, old_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = old_note.clone();
                }
            }
            Command::Detection { change } => {
                project.apply_detection_change(change, false);
            }
        }
    }
}

pub struct CommandHistory {
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Push a command that has ALREADY been applied to the project.
    pub fn push(&mut self, cmd: Command) {
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
    }

    /// Update the last command's "new" state (for coalescing text edits / drag).
    pub fn update_last<F: FnOnce(&mut Command)>(&mut self, f: F) {
        if let Some(cmd) = self.undo_stack.last_mut() {
            f(cmd);
        }
    }

    /// Check if the last command matches a predicate (for coalescing).
    pub fn last_matches(&self, line_id: u64, kind: CommandKind) -> bool {
        self.undo_stack.last().is_some_and(|cmd| match (cmd, kind) {
            (Command::UpdateLineText { line_id: id, .. }, CommandKind::UpdateLineText) => {
                *id == line_id
            }
            (Command::UpdateLineNote { line_id: id, .. }, CommandKind::UpdateLineNote) => {
                *id == line_id
            }
            (Command::MoveLine { line_id: id, .. }, CommandKind::MoveLine) => *id == line_id,
            (Command::ResizeLine { line_id: id, .. }, CommandKind::ResizeLine) => *id == line_id,
            (Command::SetCharacter { line_id: id, .. }, CommandKind::SetCharacter) => {
                *id == line_id
            }
            (Command::SetCharacterColor { line_id: id, .. }, CommandKind::SetCharacterColor) => {
                *id == line_id
            }
            (Command::MoveMarker { index: idx, .. }, CommandKind::MoveMarker) => {
                *idx == line_id as usize
            }
            _ => false,
        })
    }

    pub fn last(&self) -> Option<&Command> {
        self.undo_stack.last()
    }

    pub fn last_matches_strokes(&self, stroke_ids: &[u64]) -> bool {
        self.undo_stack.last().is_some_and(|command| {
            matches!(command, Command::TransformStrokes { stroke_ids: ids, .. } if ids == stroke_ids)
        })
    }

    pub fn undo(&mut self, project: &mut Project) {
        if let Some(cmd) = self.undo_stack.pop() {
            cmd.unapply(project);
            self.redo_stack.push(cmd);
        }
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn redo(&mut self, project: &mut Project) {
        if let Some(cmd) = self.redo_stack.pop() {
            cmd.apply(project);
            self.undo_stack.push(cmd);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum CommandKind {
    UpdateLineText,
    UpdateLineNote,
    MoveLine,
    ResizeLine,
    SetCharacter,
    SetCharacterColor,
    MoveMarker,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project_with_line() -> (Project, u64) {
        let mut p = Project::new();
        let id = p.add_line_full(0, 48, 0.5, "test".into(), "Char".into(), [1.0; 4]);
        (p, id)
    }

    #[test]
    fn test_undo_redo_move() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        // Move
        project.get_line_mut(id).unwrap().start_frame = 100;
        project.get_line_mut(id).unwrap().y_slot = 0.75;
        history.push(Command::MoveLine {
            line_id: id,
            old_start: 0,
            old_y_slot: 0.5,
            new_start: 100,
            new_y_slot: 0.75,
        });

        assert_eq!(project.get_line(id).unwrap().start_frame, 100);

        // Undo
        history.undo(&mut project);
        assert_eq!(project.get_line(id).unwrap().start_frame, 0);
        assert_eq!(project.get_line(id).unwrap().y_slot, 0.5);

        // Redo
        history.redo(&mut project);
        assert_eq!(project.get_line(id).unwrap().start_frame, 100);
    }

    #[test]
    fn test_undo_redo_delete() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        let (snapshot, index) = project.remove_line(id).unwrap();
        history.push(Command::DeleteLine { snapshot, index });
        assert_eq!(project.line_count(), 0);

        // Undo restores the line
        history.undo(&mut project);
        assert_eq!(project.line_count(), 1);
        assert_eq!(project.get_line(id).unwrap().text, "test");

        // Redo removes it again
        history.redo(&mut project);
        assert_eq!(project.line_count(), 0);
    }

    #[test]
    fn test_undo_redo_delete_all_is_one_history_command() {
        let mut project = Project::new();
        let first = project.add_line_full(0, 10, 0.25, "first".into(), "A".into(), [1.0; 4]);
        let second = project.add_line_full(10, 10, 0.5, "second".into(), "B".into(), [1.0; 4]);
        let lines = vec![
            (project.get_line(first).unwrap().clone(), 0),
            (project.get_line(second).unwrap().clone(), 1),
        ];
        let mut history = CommandHistory::new();
        let command = Command::DeleteLines { lines };
        command.apply(&mut project);
        history.push(command);
        assert_eq!(project.line_count(), 0);

        history.undo(&mut project);
        assert_eq!(project.line_count(), 2);
        assert_eq!(
            project
                .lines()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );

        history.redo(&mut project);
        assert_eq!(project.line_count(), 0);
    }

    #[test]
    fn test_undo_redo_insert_multiple_is_one_history_command() {
        let mut source = Project::new();
        let first = source.add_line_full(0, 10, 0.25, "first".into(), "A".into(), [1.0; 4]);
        let second = source.add_line_full(10, 10, 0.5, "second".into(), "B".into(), [1.0; 4]);
        let lines = vec![
            (source.get_line(first).unwrap().clone(), 0),
            (source.get_line(second).unwrap().clone(), 1),
        ];
        let mut project = Project::new();
        let mut history = CommandHistory::new();
        let command = Command::InsertLines { lines };
        command.apply(&mut project);
        history.push(command);
        assert_eq!(project.line_count(), 2);

        history.undo(&mut project);
        assert_eq!(project.line_count(), 0);

        history.redo(&mut project);
        assert_eq!(project.line_count(), 2);
        assert_eq!(
            project
                .lines()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
    }

    #[test]
    fn test_undo_redo_text() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        project.get_line_mut(id).unwrap().text = "modified".into();
        history.push(Command::UpdateLineText {
            line_id: id,
            old_text: "test".into(),
            new_text: "modified".into(),
        });

        history.undo(&mut project);
        assert_eq!(project.get_line(id).unwrap().text, "test");
    }

    #[test]
    fn test_undo_redo_split_line() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();
        let old_line = project.get_line(id).unwrap().clone();
        let old_index = project.line_index(id).unwrap();

        let mut first_line = old_line.clone();
        first_line.text = "te-".into();
        first_line.duration_frames = 24;

        let mut second_line = old_line.clone();
        second_line.id = id + 1;
        second_line.text = "-st".into();
        second_line.start_frame = 24;
        second_line.duration_frames = 24;

        *project.get_line_mut(id).unwrap() = first_line.clone();
        project.insert_line_at(old_index + 1, second_line.clone());
        history.push(Command::SplitLine {
            old_line: old_line.clone(),
            old_index,
            first_line: first_line.clone(),
            second_line: second_line.clone(),
            second_index: old_index + 1,
        });

        assert_eq!(project.line_count(), 2);
        assert_eq!(project.get_line(id).unwrap().text, "te-");

        history.undo(&mut project);
        assert_eq!(project.line_count(), 1);
        assert_eq!(project.get_line(id).unwrap().text, "test");
        assert!(project.get_line(second_line.id).is_none());

        history.redo(&mut project);
        assert_eq!(project.line_count(), 2);
        assert_eq!(project.get_line(id).unwrap().text, "te-");
        assert_eq!(project.get_line(second_line.id).unwrap().text, "-st");
    }

    #[test]
    fn test_undo_redo_character_voice_actors() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        project.set_line_voice_actor_names(id, vec!["Old Actor".into()]);
        project.set_character_with_voice_actors(
            id,
            "New Char".into(),
            [0.0, 1.0, 0.0, 1.0],
            vec!["New Actor".into()],
        );
        history.push(Command::SetCharacter {
            line_id: id,
            old_name: "Char".into(),
            old_color: [1.0; 4],
            old_voice_actor_names: vec!["Old Actor".into()],
            new_name: "New Char".into(),
            new_color: [0.0, 1.0, 0.0, 1.0],
            new_voice_actor_names: vec!["New Actor".into()],
        });

        history.undo(&mut project);
        let line = project.get_line(id).unwrap();
        assert_eq!(line.character_name, "Char");
        assert_eq!(line.voice_actor_names, vec!["Old Actor"]);

        history.redo(&mut project);
        let line = project.get_line(id).unwrap();
        assert_eq!(line.character_name, "New Char");
        assert_eq!(line.voice_actor_names, vec!["New Actor"]);
    }

    #[test]
    fn test_undo_redo_rename_character() {
        let mut project = Project::new();
        let alice_1 = project.add_line_full(0, 48, 0.25, "hello".into(), "Alice".into(), [1.0; 4]);
        let alice_2 = project.add_line_full(48, 48, 0.50, "again".into(), "Alice".into(), [1.0; 4]);
        let bob = project.add_line_full(96, 48, 0.75, "world".into(), "Bob".into(), [0.0; 4]);
        let old_known_characters = vec![
            Character {
                name: "Alice".into(),
                color: [1.0; 4],
            },
            Character {
                name: "Bob".into(),
                color: [0.0; 4],
            },
        ];
        let new_known_characters = vec![
            Character {
                name: "Alicia".into(),
                color: [1.0; 4],
            },
            Character {
                name: "Bob".into(),
                color: [0.0; 4],
            },
        ];
        project.set_known_characters(old_known_characters.clone());
        let changes = vec![
            LineCharacterNameChange {
                line_id: alice_1,
                old_name: "Alice".into(),
                new_name: "Alicia".into(),
            },
            LineCharacterNameChange {
                line_id: alice_2,
                old_name: "Alice".into(),
                new_name: "Alicia".into(),
            },
        ];

        project.apply_character_name_changes(&changes, true);
        project.set_known_characters(new_known_characters.clone());
        let mut history = CommandHistory::new();
        history.push(Command::RenameCharacter {
            changes,
            old_known_characters,
            new_known_characters,
        });

        history.undo(&mut project);
        assert_eq!(project.get_line(alice_1).unwrap().character_name, "Alice");
        assert_eq!(project.get_line(alice_2).unwrap().character_name, "Alice");
        assert_eq!(project.get_line(bob).unwrap().character_name, "Bob");
        let known_names: Vec<_> = project
            .known_characters()
            .iter()
            .map(|character| character.name.as_str())
            .collect();
        assert_eq!(known_names, vec!["Alice", "Bob"]);

        history.redo(&mut project);
        assert_eq!(project.get_line(alice_1).unwrap().character_name, "Alicia");
        assert_eq!(project.get_line(alice_2).unwrap().character_name, "Alicia");
        assert_eq!(project.get_line(bob).unwrap().character_name, "Bob");
        let known_names: Vec<_> = project
            .known_characters()
            .iter()
            .map(|character| character.name.as_str())
            .collect();
        assert_eq!(known_names, vec!["Alicia", "Bob"]);
    }

    #[test]
    fn test_coalescing() {
        let (_, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        history.push(Command::UpdateLineText {
            line_id: id,
            old_text: "a".into(),
            new_text: "ab".into(),
        });
        assert!(history.last_matches(id, CommandKind::UpdateLineText));
        assert!(!history.last_matches(id, CommandKind::MoveLine));
        assert!(!history.last_matches(id + 1, CommandKind::UpdateLineText));
    }

    #[test]
    fn test_push_clears_redo() {
        let (mut project, id) = make_project_with_line();
        let mut history = CommandHistory::new();

        project.get_line_mut(id).unwrap().text = "v1".into();
        history.push(Command::UpdateLineText {
            line_id: id,
            old_text: "test".into(),
            new_text: "v1".into(),
        });
        history.undo(&mut project);

        // New command clears redo stack
        project.get_line_mut(id).unwrap().text = "v2".into();
        history.push(Command::UpdateLineText {
            line_id: id,
            old_text: "test".into(),
            new_text: "v2".into(),
        });
        // Redo should do nothing (stack cleared)
        history.redo(&mut project);
        assert_eq!(project.get_line(id).unwrap().text, "v2");
    }
}
