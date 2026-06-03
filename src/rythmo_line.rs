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
    #[serde(default)]
    pub voice_actor_names: Vec<String>,
    #[serde(default)]
    pub syllable_ratios: Vec<f32>,
    #[serde(default)]
    pub note: String,
}

impl RythmoLine {
    pub fn end_frame(&self) -> i64 {
        self.start_frame + self.duration_frames
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.duration_frames <= 0 {
            return Err(format!(
                "Line {}: duration must be positive (got {})",
                self.id, self.duration_frames
            ));
        }
        if self.y_slot < 0.0 || self.y_slot > 1.0 {
            return Err(format!(
                "Line {}: y_slot must be 0.0-1.0 (got {})",
                self.id, self.y_slot
            ));
        }
        for (i, &c) in self.character_color.iter().enumerate() {
            if c < 0.0 || c > 1.0 {
                return Err(format!(
                    "Line {}: color channel {} out of range (got {})",
                    self.id, i, c
                ));
            }
        }
        Ok(())
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
