use serde::{Deserialize, Serialize};

use crate::project::{LineCharacterNameChange, Project};
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_line::{MarkerKind, RythmoLine, RythmoMarker};
use crate::voice_actor::{LineVoiceActorsChange, VoiceActor};

// ---------------------------------------------------------------------------
// Packetable trait — implemented by Command
// ---------------------------------------------------------------------------

pub trait Packetable {
    fn to_packet(&self, project: &Project) -> Packet;
}

// ---------------------------------------------------------------------------
// Packet — all network message types (client ↔ server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Packet {
    // Client → Server
    Auth {
        password: String,
    },
    CreateRoom {
        username: String,
    },
    JoinRoom {
        code: String,
        username: String,
    },
    LeaveRoom,
    Command {
        payload: CommandPayload,
    },
    RequestSync,

    // Server → Client
    AuthOk,
    AuthFail {
        reason: String,
    },
    RoomCreated {
        code: String,
    },
    RoomJoined {
        code: String,
        role: String,
        members: Vec<String>,
    },
    JoinError {
        reason: String,
    },
    MemberJoined {
        username: String,
    },
    MemberLeft {
        username: String,
    },
    RemoteCommand {
        from: String,
        payload: CommandPayload,
    },
    Sync {
        project: ProjectData,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// CommandPayload — serializable form of Command (forward-only, no undo data)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum CommandPayload {
    CreateLine {
        line: RythmoLine,
    },
    DeleteLine {
        line_id: u64,
    },
    SplitLine {
        first_line: RythmoLine,
        second_line: RythmoLine,
        second_index: usize,
    },
    MoveLine {
        line_id: u64,
        start_frame: i64,
        y_slot: f32,
    },
    MoveLines {
        lines: Vec<MoveLinePayload>,
    },
    ResizeLine {
        line_id: u64,
        start_frame: i64,
        duration_frames: i64,
    },
    UpdateLineText {
        line_id: u64,
        text: String,
    },
    UpdateLineNote {
        line_id: u64,
        note: String,
    },
    SetLineKaraoke {
        line_id: u64,
        karaoke: bool,
        syllable_ratios: Vec<f32>,
    },
    SetSyllableRatios {
        line_id: u64,
        ratios: Vec<f32>,
    },
    SetCharacter {
        line_id: u64,
        name: String,
        color: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice_actor_names: Option<Vec<String>>,
    },
    SetCharacterColor {
        line_id: u64,
        color: [f32; 4],
    },
    RenameCharacter {
        changes: Vec<LineCharacterNameChange>,
        known_characters: Vec<CharacterData>,
    },
    SetVoiceActors {
        changes: Vec<LineVoiceActorsChange>,
    },
    CreateVoiceActor {
        actor: VoiceActor,
    },
    AddMarker {
        kind: MarkerKind,
        frame: i64,
    },
    RemoveMarker {
        kind: MarkerKind,
        frame: i64,
    },
    MoveMarker {
        kind: MarkerKind,
        old_frame: i64,
        new_frame: i64,
    },
    AddDrawingStroke {
        stroke: DrawingStroke,
    },
EraseDrawingStrokes {
        strokes: Vec<DrawingStroke>,
    },
    TransformStrokes {
        stroke_ids: Vec<u64>,
        new_points: Vec<Vec<(f64, f32)>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveLinePayload {
    pub line_id: u64,
    pub start_frame: i64,
    pub y_slot: f32,
}

// ---------------------------------------------------------------------------
// ProjectData — full project state for sync
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    pub lines: Vec<RythmoLine>,
    pub markers: Vec<RythmoMarker>,
    pub known_characters: Vec<CharacterData>,
    #[serde(default)]
    pub voice_actors: Vec<VoiceActor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    pub name: String,
    pub color: [f32; 4],
}

// ---------------------------------------------------------------------------
// impl Packetable for Command
// ---------------------------------------------------------------------------

use crate::command::Command;

impl Packetable for Command {
    fn to_packet(&self, project: &Project) -> Packet {
        let payload = match self {
            Command::CreateLine { snapshot, .. } => CommandPayload::CreateLine {
                line: snapshot.clone(),
            },
            Command::InsertLine { snapshot, .. } => CommandPayload::CreateLine {
                line: snapshot.clone(),
            },
            Command::DeleteLine { snapshot, .. } => CommandPayload::DeleteLine {
                line_id: snapshot.id,
            },
            Command::SplitLine {
                first_line,
                second_line,
                second_index,
                ..
            } => CommandPayload::SplitLine {
                first_line: first_line.clone(),
                second_line: second_line.clone(),
                second_index: *second_index,
            },
            Command::MoveLine {
                line_id,
                new_start,
                new_y_slot,
                ..
            } => CommandPayload::MoveLine {
                line_id: *line_id,
                start_frame: *new_start,
                y_slot: *new_y_slot,
            },
            Command::MoveLines { moves } => CommandPayload::MoveLines {
                lines: moves
                    .iter()
                    .map(|movement| MoveLinePayload {
                        line_id: movement.line_id,
                        start_frame: movement.new_start,
                        y_slot: movement.new_y_slot,
                    })
                    .collect(),
            },
            Command::ResizeLine {
                line_id,
                new_start,
                new_dur,
                ..
            } => CommandPayload::ResizeLine {
                line_id: *line_id,
                start_frame: *new_start,
                duration_frames: *new_dur,
            },
            Command::UpdateLineText {
                line_id, new_text, ..
            } => CommandPayload::UpdateLineText {
                line_id: *line_id,
                text: new_text.clone(),
            },
            Command::UpdateLineNote {
                line_id, new_note, ..
            } => CommandPayload::UpdateLineNote {
                line_id: *line_id,
                note: new_note.clone(),
            },
            Command::SetLineKaraoke {
                line_id,
                new_karaoke,
                new_ratios,
                ..
            } => CommandPayload::SetLineKaraoke {
                line_id: *line_id,
                karaoke: *new_karaoke,
                syllable_ratios: new_ratios.clone(),
            },
            Command::SetSyllableRatios {
                line_id,
                new_ratios,
                ..
            } => CommandPayload::SetSyllableRatios {
                line_id: *line_id,
                ratios: new_ratios.clone(),
            },
            Command::SetCharacter {
                line_id,
                new_name,
                new_color,
                new_voice_actor_names,
                ..
            } => CommandPayload::SetCharacter {
                line_id: *line_id,
                name: new_name.clone(),
                color: *new_color,
                voice_actor_names: Some(new_voice_actor_names.clone()),
            },
            Command::SetCharacterColor {
                line_id, new_color, ..
            } => CommandPayload::SetCharacterColor {
                line_id: *line_id,
                color: *new_color,
            },
            Command::RenameCharacter {
                changes,
                new_known_characters,
                ..
            } => CommandPayload::RenameCharacter {
                changes: changes.clone(),
                known_characters: new_known_characters
                    .iter()
                    .map(|c| CharacterData {
                        name: c.name.clone(),
                        color: c.color,
                    })
                    .collect(),
            },
            Command::SetVoiceActors { changes } => CommandPayload::SetVoiceActors {
                changes: changes.clone(),
            },
            Command::CreateVoiceActor { actor } => CommandPayload::CreateVoiceActor {
                actor: actor.clone(),
            },
            Command::AddMarker { marker, .. } => {
                CommandPayload::AddMarker {
                    kind: marker.kind.clone(),
                    frame: marker.frame,
                }
            }
            Command::RemoveMarker { marker, .. } => CommandPayload::RemoveMarker {
                kind: marker.kind.clone(),
                frame: marker.frame,
            },
            Command::MoveMarker {
                index,
                old_frame,
                new_frame,
            } => {
                let kind = project
                    .markers
                    .get(*index)
                    .map(|m| m.kind.clone())
                    .unwrap_or(crate::rythmo_line::MarkerKind::Boucle);
                CommandPayload::MoveMarker {
                    kind,
                    old_frame: *old_frame,
                    new_frame: *new_frame,
                }
            }
            Command::AddDrawingStroke { stroke } => CommandPayload::AddDrawingStroke {
                stroke: stroke.clone(),
            },
Command::EraseDrawingStrokes { strokes } => CommandPayload::EraseDrawingStrokes {
                strokes: strokes.clone(),
            },
Command::TransformStrokes { stroke_ids, new_points, .. } => {
                CommandPayload::TransformStrokes {
                    stroke_ids: stroke_ids.clone(),
                    new_points: new_points.clone(),
                }
            }
        };
        Packet::Command { payload }
    }
}

impl ProjectData {
    pub fn from_project(project: &Project) -> Self {
        Self {
            lines: project.lines_vec(),
            markers: project.markers.clone(),
            known_characters: project
                .known_characters
                .iter()
                .map(|c| CharacterData {
                    name: c.name.clone(),
                    color: c.color,
                })
                .collect(),
            voice_actors: project.voice_actors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_payload_roundtrip() {
        let payload = CommandPayload::CreateLine {
            line: RythmoLine {
                id: 42,
                start_frame: 10,
                duration_frames: 20,
                y_slot: 0.5,
                text: "test".into(),
                character_name: "Alice".into(),
                character_color: [1.0, 0.0, 0.0, 1.0],
                voice_actor_names: Vec::new(),
                syllable_ratios: Vec::new(),
                karaoke: false,
                note: String::new(),
            },
        };
        let json = serde_json::to_string(&payload).unwrap();
        let restored: CommandPayload = serde_json::from_str(&json).unwrap();
        match restored {
            CommandPayload::CreateLine { line } => {
                assert_eq!(line.id, 42);
                assert_eq!(line.text, "test");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_project_data_roundtrip() {
        let mut project = Project::new();
        project.add_line_full(
            0,
            48,
            0.5,
            "hello".into(),
            "Bob".into(),
            [0.0, 1.0, 0.0, 1.0],
        );
        let data = ProjectData::from_project(&project);
        let json = serde_json::to_string(&data).unwrap();
        let restored: ProjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.lines.len(), 1);
        assert_eq!(restored.lines[0].text, "hello");
    }

    #[test]
    fn test_packet_serde() {
        let packet = Packet::RoomCreated {
            code: "ABC123".into(),
        };
        let json = serde_json::to_string(&packet).unwrap();
        assert!(json.contains("ABC123"));
        let restored: Packet = serde_json::from_str(&json).unwrap();
        match restored {
            Packet::RoomCreated { code } => assert_eq!(code, "ABC123"),
            _ => panic!("wrong variant"),
        }
    }
}
