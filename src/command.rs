use crate::project::Project;
use crate::rythmo_line::{RythmoLine, RythmoMarker};

/// Each command stores before/after state for reversibility.
pub enum Command {
    CreateLine {
        line_id: u64,
    },
    DeleteLine {
        snapshot: RythmoLine,
        index: usize,
    },
    MoveLine {
        line_id: u64,
        old_start: i64, old_y_slot: f32,
        new_start: i64, new_y_slot: f32,
    },
    ResizeLine {
        line_id: u64,
        old_start: i64, old_dur: i64,
        new_start: i64, new_dur: i64,
    },
    UpdateLineText {
        line_id: u64,
        old_text: String,
        new_text: String,
    },
    SetCharacter {
        line_id: u64,
        old_name: String, old_color: [f32; 4],
        new_name: String, new_color: [f32; 4],
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
}

impl Command {
    fn apply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { line_id } => {
                // Line was already added by project.add_line — nothing to re-apply
                // For redo: re-insert if deleted
            }
            Command::DeleteLine { snapshot, .. } => {
                project.lines.retain(|l| l.id != snapshot.id);
            }
            Command::MoveLine { line_id, new_start, new_y_slot, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *new_start;
                    l.y_slot = *new_y_slot;
                }
            }
            Command::ResizeLine { line_id, new_start, new_dur, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *new_start;
                    l.duration_frames = *new_dur;
                }
            }
            Command::UpdateLineText { line_id, new_text, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.text = new_text.clone();
                }
            }
            Command::SetCharacter { line_id, new_name, new_color, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_name = new_name.clone();
                    l.character_color = *new_color;
                }
            }
            Command::SetCharacterColor { line_id, new_color, .. } => {
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
        }
    }

    fn unapply(&self, project: &mut Project) {
        match self {
            Command::CreateLine { line_id } => {
                project.lines.retain(|l| l.id != *line_id);
            }
            Command::DeleteLine { snapshot, index } => {
                let idx = (*index).min(project.lines.len());
                project.lines.insert(idx, RythmoLine {
                    id: snapshot.id,
                    start_frame: snapshot.start_frame,
                    duration_frames: snapshot.duration_frames,
                    y_slot: snapshot.y_slot,
                    text: snapshot.text.clone(),
                    character_name: snapshot.character_name.clone(),
                    character_color: snapshot.character_color,
                });
            }
            Command::MoveLine { line_id, old_start, old_y_slot, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *old_start;
                    l.y_slot = *old_y_slot;
                }
            }
            Command::ResizeLine { line_id, old_start, old_dur, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.start_frame = *old_start;
                    l.duration_frames = *old_dur;
                }
            }
            Command::UpdateLineText { line_id, old_text, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.text = old_text.clone();
                }
            }
            Command::SetCharacter { line_id, old_name, old_color, .. } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.character_name = old_name.clone();
                    l.character_color = *old_color;
                }
            }
            Command::SetCharacterColor { line_id, old_color, .. } => {
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
        }
    }
}

pub struct CommandHistory {
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new() }
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
        self.undo_stack.last().map_or(false, |cmd| match (cmd, kind) {
            (Command::UpdateLineText { line_id: id, .. }, CommandKind::UpdateLineText) => *id == line_id,
            (Command::MoveLine { line_id: id, .. }, CommandKind::MoveLine) => *id == line_id,
            (Command::ResizeLine { line_id: id, .. }, CommandKind::ResizeLine) => *id == line_id,
            (Command::SetCharacter { line_id: id, .. }, CommandKind::SetCharacter) => *id == line_id,
            (Command::SetCharacterColor { line_id: id, .. }, CommandKind::SetCharacterColor) => *id == line_id,
            _ => false,
        })
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
    MoveLine,
    ResizeLine,
    SetCharacter,
    SetCharacterColor,
}
