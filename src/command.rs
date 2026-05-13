use crate::project::Project;
use crate::rythmo_line::{RythmoLine, RythmoMarker};

/// Each command stores before/after state for reversibility.
pub enum Command {
    CreateLine {
        line_id: u64,
    },
    InsertLine {
        snapshot: RythmoLine,
        index: usize,
    },
    DeleteLine {
        snapshot: RythmoLine,
        index: usize,
    },
    MoveLine {
        line_id: u64,
        old_start: i64,
        old_y_slot: f32,
        new_start: i64,
        new_y_slot: f32,
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
    SetCharacter {
        line_id: u64,
        old_name: String,
        old_color: [f32; 4],
        new_name: String,
        new_color: [f32; 4],
    },
    SetCharacterColor {
        line_id: u64,
        old_color: [f32; 4],
        new_color: [f32; 4],
    },
    AddMarker {
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
}

impl Command {
    fn apply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { line_id: _ } => {
                // Line was already added — nothing to re-apply for redo
            }
            Command::InsertLine { snapshot, index } => {
                project.insert_line_at(*index, snapshot.clone());
            }
            Command::DeleteLine { snapshot, .. } => {
                project.remove_line(snapshot.id);
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
                line_id, new_text, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.text = new_text.clone();
                }
            }
            Command::SetCharacter {
                line_id,
                new_name,
                new_color,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_name = new_name.clone();
                    l.character_color = *new_color;
                }
            }
            Command::SetCharacterColor {
                line_id, new_color, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_color = *new_color;
                }
            }
            Command::AddMarker { .. } => {
                // Already added during execute — for redo
            }
            Command::RemoveMarker { index, .. } => {
                if *index < project.markers.len() {
                    project.markers.remove(*index);
                }
            }
            Command::MoveMarker {
                index, new_frame, ..
            } => {
                if let Some(m) = project.markers.get_mut(*index) {
                    m.frame = *new_frame;
                }
            }
            Command::UpdateLineNote {
                line_id, new_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = new_note.clone();
                }
            }
        }
    }

    fn unapply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { line_id } => {
                project.remove_line(*line_id);
            }
            Command::InsertLine { snapshot, .. } => {
                project.remove_line(snapshot.id);
            }
            Command::DeleteLine { snapshot, index } => {
                project.insert_line_at(*index, snapshot.clone());
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
                line_id, old_text, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.text = old_text.clone();
                }
            }
            Command::SetCharacter {
                line_id,
                old_name,
                old_color,
                ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_name = old_name.clone();
                    l.character_color = *old_color;
                }
            }
            Command::SetCharacterColor {
                line_id, old_color, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_color = *old_color;
                }
            }
            Command::AddMarker { index } => {
                if *index < project.markers.len() {
                    project.markers.remove(*index);
                }
            }
            Command::RemoveMarker { marker, index } => {
                let idx = (*index).min(project.markers.len());
                project.markers.insert(idx, marker.clone());
            }
            Command::MoveMarker {
                index, old_frame, ..
            } => {
                if let Some(m) = project.markers.get_mut(*index) {
                    m.frame = *old_frame;
                }
            }
            Command::UpdateLineNote {
                line_id, old_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = old_note.clone();
                }
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
        self.undo_stack
            .last()
            .map_or(false, |cmd| match (cmd, kind) {
                (Command::UpdateLineText { line_id: id, .. }, CommandKind::UpdateLineText) => {
                    *id == line_id
                }
                (Command::UpdateLineNote { line_id: id, .. }, CommandKind::UpdateLineNote) => {
                    *id == line_id
                }
                (Command::MoveLine { line_id: id, .. }, CommandKind::MoveLine) => *id == line_id,
                (Command::ResizeLine { line_id: id, .. }, CommandKind::ResizeLine) => {
                    *id == line_id
                }
                (Command::SetCharacter { line_id: id, .. }, CommandKind::SetCharacter) => {
                    *id == line_id
                }
                (
                    Command::SetCharacterColor { line_id: id, .. },
                    CommandKind::SetCharacterColor,
                ) => *id == line_id,
                (Command::MoveMarker { index: idx, .. }, CommandKind::MoveMarker) => {
                    *idx == line_id as usize
                }
                _ => false,
            })
    }

    pub fn last(&self) -> Option<&Command> {
        self.undo_stack.last()
    }

    pub fn undo(&mut self, project: &mut Project) {
        if let Some(cmd) = self.undo_stack.pop() {
            cmd.unapply(project);
            self.redo_stack.push(cmd);
        }
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
