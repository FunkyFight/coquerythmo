use serde::{Deserialize, Serialize};

use crate::constants;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinePresence {
    #[default]
    On,
    Off,
    Back,
}

/// Semantic role of a line on the rythmo band. Ambiance lines are visual
/// annotations for the video band and are deliberately excluded from dialogue
/// and reference-document exports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RythmoLineKind {
    #[default]
    Dialogue,
    AmbianceStart,
    AmbianceEnd,
}

impl RythmoLineKind {
    pub fn is_dialogue(&self) -> bool {
        matches!(self, Self::Dialogue)
    }
    pub fn is_ambiance(&self) -> bool {
        !self.is_dialogue()
    }
}

pub const AMBIANCE_LABEL_PREFIX: &str = "amb.";

/// Build the immutable-prefix label shown for ambiance starts. The editable
/// model stores only the ambiance name; legacy values that already contain
/// the prefix are normalized to avoid displaying it twice.
pub fn ambiance_name(name: &str) -> &str {
    let without_leading_space = name.trim_start();
    without_leading_space
        .get(..AMBIANCE_LABEL_PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(AMBIANCE_LABEL_PREFIX))
        .map(|_| without_leading_space[AMBIANCE_LABEL_PREFIX.len()..].trim_start())
        // This function runs after every editing keystroke. Preserve ordinary
        // whitespace so a just-typed space survives until the next word.
        .unwrap_or(name)
}

pub fn ambiance_label(name: &str) -> String {
    format!("{AMBIANCE_LABEL_PREFIX}{}", ambiance_name(name))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RythmoLine {
    pub id: u64,
    pub start_frame: i64,
    pub duration_frames: i64,
    pub y_slot: f32,
    pub text: String,
    pub character_name: String,
    pub character_color: [f32; 4],
    #[serde(default, skip_serializing_if = "RythmoLineKind::is_dialogue")]
    pub kind: RythmoLineKind,
    #[serde(default)]
    pub voice_actor_names: Vec<String>,
    #[serde(default)]
    pub syllable_ratios: Vec<f32>,
    #[serde(default)]
    pub karaoke: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "LinePresence::is_on")]
    pub presence: LinePresence,
}

impl LinePresence {
    pub fn is_on(&self) -> bool {
        matches!(self, Self::On)
    }
}

impl RythmoLine {
    pub fn end_frame(&self) -> i64 {
        self.start_frame + self.duration_frames
    }

    pub fn karaoke_progress(&self, current_frame: f64) -> Option<f32> {
        if !self.karaoke || self.duration_frames <= 0 {
            return None;
        }

        let start = self.start_frame as f64;
        let end = self.end_frame() as f64;
        if current_frame < start || current_frame > end {
            return None;
        }

        Some(((current_frame - start) / self.duration_frames as f64).clamp(0.0, 1.0) as f32)
    }

    pub fn karaoke_active(&self, current_frame: f64) -> bool {
        self.karaoke_progress(current_frame).is_some()
    }

    pub fn visual_x_width(
        &self,
        current_frame: f64,
        center_x: f32,
        pixels_per_frame: f32,
        _available_width: f32,
        scale: f32,
    ) -> (f32, f32) {
        let x1 = center_x + (self.start_frame as f64 - current_frame) as f32 * pixels_per_frame;
        // Width must not depend on the moving viewport position. Computing it as
        // `x2 - x1` makes f32 rounding alternate around whole pixels during
        // sub-frame scrolling. The text texture cache rounds that width up, so
        // the oscillation continuously invalidates and rebuilds visible text.
        let width = (self.duration_frames as f32 * pixels_per_frame).max(2.0);

        if self.karaoke_active(current_frame) {
            let width =
                karaoke_text_visual_width_for_font(&self.text, constants::RYTHMO_FONT_SIZE * scale);
            (center_x - width / 2.0, width)
        } else {
            (x1, width)
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.duration_frames <= 0 {
            return Err(format!(
                "Line {}: duration must be positive (got {})",
                self.id, self.duration_frames
            ));
        }
        if !(0.0..=1.0).contains(&self.y_slot) {
            return Err(format!(
                "Line {}: y_slot must be 0.0-1.0 (got {})",
                self.id, self.y_slot
            ));
        }
        for (i, &c) in self.character_color.iter().enumerate() {
            if !(0.0..=1.0).contains(&c) {
                return Err(format!(
                    "Line {}: color channel {} out of range (got {})",
                    self.id, i, c
                ));
            }
        }
        Ok(())
    }
}

pub fn karaoke_text_visual_width_for_font(text: &str, font_size: f32) -> f32 {
    let font_size = (font_size * constants::KARAOKE_TEXT_FONT_SCALE).max(1.0);
    let char_count = text.chars().count().max(1) as f32;
    let avg_char_width = font_size * 0.62;
    let padding = font_size * 0.7;
    (char_count * avg_char_width + padding).max(2.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiance_prefix_is_permanent_and_never_duplicated() {
        assert_eq!(ambiance_label(""), "amb.");
        assert_eq!(ambiance_label("bureaux"), "amb.bureaux");
        assert_eq!(ambiance_label("Amb. bureaux"), "amb.bureaux");
        assert_eq!(ambiance_name("amb.pluie"), "pluie");
        assert_eq!(ambiance_label("bruit de "), "amb.bruit de ");
    }
}
