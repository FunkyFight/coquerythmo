use serde::{Deserialize, Serialize};

use crate::project::Project;
use crate::rythmo_line::{MarkerKind, RythmoLine, RythmoMarker};

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
    Auth { password: String },
    CreateRoom { username: String },
    JoinRoom { code: String, username: String },
    LeaveRoom,
    Command { payload: CommandPayload },
    RequestSync,

    // Server → Client
    AuthOk,
    AuthFail { reason: String },
    RoomCreated { code: String },
    RoomJoined { code: String, role: String, members: Vec<String> },
    JoinError { reason: String },
    MemberJoined { username: String },
    MemberLeft { username: String },
    RemoteCommand { from: String, payload: CommandPayload },
    Sync { project: ProjectData },
    Error { message: String },
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
    MoveLine {
        line_id: u64,
        start_frame: i64,
        y_slot: f32,
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
    SetCharacter {
        line_id: u64,
        name: String,
        color: [f32; 4],
    },
    SetCharacterColor {
        line_id: u64,
        color: [f32; 4],
    },
    AddMarker {
        kind: MarkerKind,
        frame: i64,
    },
    RemoveMarker {
        kind: MarkerKind,
        frame: i64,
    },
    LoadVideo {
        filename: String,
        data_base64: String,
    },
}

// ---------------------------------------------------------------------------
// ProjectData — full project state for sync
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    pub lines: Vec<RythmoLine>,
    pub markers: Vec<RythmoMarker>,
    pub known_characters: Vec<CharacterData>,
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
            Command::CreateLine { line_id } => {
                let line = project.get_line(*line_id)
                    .expect("line must exist when converting to packet")
                    .clone();
                CommandPayload::CreateLine { line }
            }
            Command::DeleteLine { snapshot, .. } => {
                CommandPayload::DeleteLine { line_id: snapshot.id }
            }
            Command::MoveLine { line_id, new_start, new_y_slot, .. } => {
                CommandPayload::MoveLine {
                    line_id: *line_id,
                    start_frame: *new_start,
                    y_slot: *new_y_slot,
                }
            }
            Command::ResizeLine { line_id, new_start, new_dur, .. } => {
                CommandPayload::ResizeLine {
                    line_id: *line_id,
                    start_frame: *new_start,
                    duration_frames: *new_dur,
                }
            }
            Command::UpdateLineText { line_id, new_text, .. } => {
                CommandPayload::UpdateLineText {
                    line_id: *line_id,
                    text: new_text.clone(),
                }
            }
            Command::SetCharacter { line_id, new_name, new_color, .. } => {
                CommandPayload::SetCharacter {
                    line_id: *line_id,
                    name: new_name.clone(),
                    color: *new_color,
                }
            }
            Command::SetCharacterColor { line_id, new_color, .. } => {
                CommandPayload::SetCharacterColor {
                    line_id: *line_id,
                    color: *new_color,
                }
            }
            Command::AddMarker { index } => {
                let marker = &project.markers[*index];
                CommandPayload::AddMarker {
                    kind: marker.kind.clone(),
                    frame: marker.frame,
                }
            }
            Command::RemoveMarker { marker, .. } => {
                CommandPayload::RemoveMarker {
                    kind: marker.kind.clone(),
                    frame: marker.frame,
                }
            }
        };
        Packet::Command { payload }
    }
}

impl ProjectData {
    pub fn from_project(project: &Project) -> Self {
        Self {
            lines: project.lines.clone(),
            markers: project.markers.clone(),
            known_characters: project.known_characters.iter()
                .map(|c| CharacterData { name: c.name.clone(), color: c.color })
                .collect(),
        }
    }
}
