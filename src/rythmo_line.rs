use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RythmoLine {
    pub id: u64,
    pub start_frame: i64,
    pub duration_frames: i64,
    pub y_slot: f32,
    pub text: String,
    pub character_name: String,
    pub character_color: [f32; 4],
}

impl RythmoLine {
    pub fn end_frame(&self) -> i64 {
        self.start_frame + self.duration_frames
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MarkerKind {
    Boucle,
    Out,
    SceneChange,
    LiaisonLeft,
    LiaisonRight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RythmoMarker {
    pub kind: MarkerKind,
    pub frame: i64,
}
