use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

use crate::constants;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEmotion {
    Pendulum,
    Swing,
    Yay,
    Bounce,
    Slide,
    Oscillation,
    Wave,
    Shake,
    Wiggle,
    AngerHeavy,
    AngerContained,
    JoySoft,
    JoyBurst,
    FearPanic,
    SadnessDeep,
    LoveTender,
    AngerSoft,
    AngerExtreme,
    JoyStrong,
    JoyExtreme,
    FearSoft,
    FearStrong,
    FearExtreme,
    SadnessSoft,
    SadnessStrong,
    SadnessExtreme,
    TendernessSoft,
    TendernessStrong,
    TendernessExtreme,
    DisgustSoft,
    Disgust,
    DisgustStrong,
    DisgustExtreme,
    DoubtSoft,
    Doubt,
    DoubtStrong,
    DoubtExtreme,
    QuestionSoft,
    Question,
    QuestionStrong,
    QuestionExtreme,
    QuestionFast,
    ExclamationSoft,
    Exclamation,
    ExclamationStrong,
    ExclamationExtreme,
    ExclamationHuge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEmotionTransform {
    pub offset: [f32; 2],
    /// Rotation, horizontal skew, normalized pivot x/y.
    pub transform: [f32; 4],
    pub tint: [f32; 4],
}

thread_local! {
    static EMOTION_RATIOS_CACHE: RefCell<HashMap<u64, Vec<f32>>> = RefCell::new(HashMap::new());
}

pub fn text_emotion_char_ratios(text: &str, font_size: f32) -> Option<Vec<f32>> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    font_size.to_bits().hash(&mut hasher);
    crate::vector_text::rythmo_font_family_name().hash(&mut hasher);
    let key = hasher.finish();
    EMOTION_RATIOS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(ratios) = cache.get(&key) {
            return Some(ratios.clone());
        }
        let ratios =
            crate::vector_text::measure_rythmo_text_char_ratios_standalone(text, font_size)?;
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, ratios.clone());
        Some(ratios)
    })
}

pub fn text_emotion_transform(
    emotion: TextEmotion,
    grapheme_index: usize,
    grapheme_count: usize,
    seconds: f32,
) -> TextEmotionTransform {
    let i = grapheme_index as f32;
    let count = grapheme_count.max(1) as f32;
    let phase = seconds * 4.0 + i * 0.42;
    let mut value = TextEmotionTransform {
        offset: [0.0; 2],
        transform: [0.0, 0.0, 0.5, 0.5],
        tint: [1.0; 4],
    };
    match emotion {
        TextEmotion::Pendulum => {
            value.transform = [phase.sin() * 0.22, 0.0, 0.5, 0.0];
        }
        TextEmotion::Swing => {
            value.transform = [phase.sin() * 0.18, phase.cos() * 0.18, 0.5, 0.0];
            value.offset[0] = phase.cos() * 1.5;
        }
        TextEmotion::Yay => {
            value.tint = rainbow((seconds * 0.22 + i / count).fract());
        }
        TextEmotion::Bounce => {
            let cycle = seconds.rem_euclid(1.0) * count;
            let active = cycle.floor();
            if active == i {
                value.offset[1] = -((cycle - active) * std::f32::consts::PI).sin() * 7.0;
            }
        }
        TextEmotion::Slide => {
            value.transform[1] = phase.sin() * 0.28;
        }
        TextEmotion::Oscillation => {
            value.transform[0] = phase.sin() * 0.20;
        }
        TextEmotion::Wave => {
            value.offset[1] = phase.sin() * 4.5;
        }
        TextEmotion::Shake => {
            value.offset = [(phase * 3.7).sin() * 2.0, (phase * 5.1 + 1.7).sin() * 2.0];
            value.transform[0] = (phase * 4.3).sin() * 0.035;
        }
        TextEmotion::Wiggle => {
            value.offset = [phase.sin() * 3.2, (phase * 0.73).cos() * 1.4];
            value.transform[0] = (phase * 0.8).sin() * 0.055;
        }
        TextEmotion::AngerHeavy => {
            value.offset = [(phase * 3.7).sin() * 5.5, (phase * 5.1 + 1.7).sin() * 5.5];
            value.transform[0] = (phase * 4.3).sin() * 0.09;
        }
        TextEmotion::AngerContained => {
            value.offset = [(phase * 3.7).sin() * 0.8, (phase * 5.1 + 1.7).sin() * 0.8];
            value.transform[0] = (phase * 4.3).sin() * 0.02;
        }
        TextEmotion::JoySoft => {
            value.tint = rainbow((seconds * 0.12 + i / count).fract());
        }
        TextEmotion::JoyBurst => {
            let cycle = (seconds * 10.0).rem_euclid(count);
            let active = cycle.floor();
            if active == i {
                value.offset[1] = -((cycle - active) * std::f32::consts::PI).sin() * 11.0;
            }
        }
        TextEmotion::FearPanic => {
            value.offset = [(phase * 6.0).sin() * 4.5, (phase * 8.0 + 1.7).sin() * 2.0];
            value.transform[0] = (phase * 7.0).sin() * 0.08;
        }
        TextEmotion::SadnessDeep => {
            value.transform = [(seconds * 2.0 + i * 0.42).sin() * 0.12, 0.0, 0.5, 0.0];
            value.offset[1] = (seconds * 2.0 + i * 0.42).sin() * 2.0;
        }
        TextEmotion::LoveTender => {
            value.transform = [phase.sin() * 0.08, phase.cos() * 0.08, 0.5, 0.0];
            value.offset[1] = phase.sin() * 1.5;
        }
        TextEmotion::AngerSoft | TextEmotion::ExclamationSoft => {
            value.offset = [(phase * 5.0).sin() * 1.2, (phase * 6.0).cos() * 1.2];
            value.transform[0] = (phase * 5.0).sin() * 0.025;
        }
        TextEmotion::AngerExtreme | TextEmotion::ExclamationExtreme => {
            value.offset = [(phase * 3.0).sin() * 8.0, (phase * 4.0).cos() * 8.0];
            value.transform[0] = (phase * 4.0).sin() * 0.13;
        }
        TextEmotion::JoyStrong => value.offset[1] = (phase * 1.4).sin() * 8.0,
        TextEmotion::JoyExtreme => value.offset[1] = (phase * 2.0).sin() * 13.0,
        TextEmotion::FearSoft => {
            value.offset = [(phase * 4.0).sin() * 1.5, (phase * 5.0).cos() * 0.8];
        }
        TextEmotion::FearStrong => {
            value.offset = [(phase * 7.0).sin() * 5.5, (phase * 9.0).cos() * 2.5];
            value.transform[0] = (phase * 8.0).sin() * 0.1;
        }
        TextEmotion::FearExtreme => {
            value.offset = [(phase * 10.0).sin() * 8.0, (phase * 12.0).cos() * 4.0];
            value.transform[0] = (phase * 11.0).sin() * 0.16;
        }
        TextEmotion::SadnessSoft => value.offset[1] = (phase * 0.7).sin() * 1.0,
        TextEmotion::SadnessStrong => value.offset[1] = (phase * 1.3).sin() * 3.5,
        TextEmotion::SadnessExtreme => value.offset[1] = (phase * 1.8).sin() * 6.0,
        TextEmotion::TendernessSoft => value.transform[0] = phase.sin() * 0.04,
        TextEmotion::TendernessStrong => value.transform[0] = phase.sin() * 0.14,
        TextEmotion::TendernessExtreme => value.transform[0] = phase.sin() * 0.24,
        TextEmotion::DisgustSoft => value.transform[1] = phase.sin() * 0.08,
        TextEmotion::Disgust => value.transform[1] = phase.sin() * 0.22,
        TextEmotion::DisgustStrong => value.transform[1] = phase.sin() * 0.38,
        TextEmotion::DisgustExtreme => value.transform[1] = phase.sin() * 0.6,
        TextEmotion::DoubtSoft => value.transform[0] = phase.sin() * 0.06,
        TextEmotion::Doubt => value.transform[0] = phase.sin() * 0.2,
        TextEmotion::DoubtStrong => value.transform[0] = phase.sin() * 0.34,
        TextEmotion::DoubtExtreme => value.transform[0] = phase.sin() * 0.55,
        TextEmotion::QuestionSoft => value.offset[1] = phase.sin() * 2.0,
        TextEmotion::Question => value.offset[1] = phase.sin() * 4.5,
        TextEmotion::QuestionStrong => value.offset[1] = phase.sin() * 6.5,
        TextEmotion::QuestionExtreme => value.offset[1] = phase.sin() * 10.0,
        TextEmotion::QuestionFast => value.offset[1] = (phase * 2.0).sin() * 7.0,
        TextEmotion::Exclamation | TextEmotion::ExclamationStrong => {
            value.offset = [(phase * 3.7).sin() * 3.8, (phase * 5.1).sin() * 3.8];
            value.transform[0] = (phase * 4.3).sin() * 0.06;
        }
        TextEmotion::ExclamationHuge => {
            value.offset = [(phase * 2.5).sin() * 10.0, (phase * 3.5).sin() * 10.0];
            value.transform[0] = (phase * 3.5).sin() * 0.16;
        }
    }
    value
}

fn rainbow(hue: f32) -> [f32; 4] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h.rem_euclid(2.0) - 1.0).abs();
    let rgb = match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    [rgb[0], rgb[1], rgb[2], 1.0]
}

impl TextEmotion {
    pub const ALL: [Self; 47] = [
        Self::Pendulum,
        Self::Swing,
        Self::Yay,
        Self::Bounce,
        Self::Slide,
        Self::Oscillation,
        Self::Wave,
        Self::Shake,
        Self::Wiggle,
        Self::AngerHeavy,
        Self::AngerContained,
        Self::JoySoft,
        Self::JoyBurst,
        Self::FearPanic,
        Self::SadnessDeep,
        Self::LoveTender,
        Self::AngerSoft,
        Self::AngerExtreme,
        Self::JoyStrong,
        Self::JoyExtreme,
        Self::FearSoft,
        Self::FearStrong,
        Self::FearExtreme,
        Self::SadnessSoft,
        Self::SadnessStrong,
        Self::SadnessExtreme,
        Self::TendernessSoft,
        Self::TendernessStrong,
        Self::TendernessExtreme,
        Self::DisgustSoft,
        Self::Disgust,
        Self::DisgustStrong,
        Self::DisgustExtreme,
        Self::DoubtSoft,
        Self::Doubt,
        Self::DoubtStrong,
        Self::DoubtExtreme,
        Self::QuestionSoft,
        Self::Question,
        Self::QuestionStrong,
        Self::QuestionExtreme,
        Self::QuestionFast,
        Self::ExclamationSoft,
        Self::Exclamation,
        Self::ExclamationStrong,
        Self::ExclamationExtreme,
        Self::ExclamationHuge,
    ];

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Pendulum => "text_emotion.pendulum",
            Self::Swing => "text_emotion.swing",
            Self::Yay => "text_emotion.yay",
            Self::Bounce => "text_emotion.bounce",
            Self::Slide => "text_emotion.slide",
            Self::Oscillation => "text_emotion.oscillation",
            Self::Wave => "text_emotion.wave",
            Self::Shake => "text_emotion.shake",
            Self::Wiggle => "text_emotion.wiggle",
            Self::AngerHeavy => "text_emotion.anger_heavy",
            Self::AngerContained => "text_emotion.anger_contained",
            Self::JoySoft => "text_emotion.joy_soft",
            Self::JoyBurst => "text_emotion.joy_burst",
            Self::FearPanic => "text_emotion.fear_panic",
            Self::SadnessDeep => "text_emotion.sadness_deep",
            Self::LoveTender => "text_emotion.love_tender",
            Self::AngerSoft => "text_emotion.anger_soft",
            Self::AngerExtreme => "text_emotion.anger_extreme",
            Self::JoyStrong => "text_emotion.joy_strong",
            Self::JoyExtreme => "text_emotion.joy_extreme",
            Self::FearSoft => "text_emotion.fear_soft",
            Self::FearStrong => "text_emotion.fear_strong",
            Self::FearExtreme => "text_emotion.fear_extreme",
            Self::SadnessSoft => "text_emotion.sadness_soft",
            Self::SadnessStrong => "text_emotion.sadness_strong",
            Self::SadnessExtreme => "text_emotion.sadness_extreme",
            Self::TendernessSoft => "text_emotion.tenderness_soft",
            Self::TendernessStrong => "text_emotion.tenderness_strong",
            Self::TendernessExtreme => "text_emotion.tenderness_extreme",
            Self::DisgustSoft => "text_emotion.disgust_soft",
            Self::Disgust => "text_emotion.disgust",
            Self::DisgustStrong => "text_emotion.disgust_strong",
            Self::DisgustExtreme => "text_emotion.disgust_extreme",
            Self::DoubtSoft => "text_emotion.doubt_soft",
            Self::Doubt => "text_emotion.doubt",
            Self::DoubtStrong => "text_emotion.doubt_strong",
            Self::DoubtExtreme => "text_emotion.doubt_extreme",
            Self::QuestionSoft => "text_emotion.question_soft",
            Self::Question => "text_emotion.question",
            Self::QuestionStrong => "text_emotion.question_strong",
            Self::QuestionExtreme => "text_emotion.question_extreme",
            Self::QuestionFast => "text_emotion.question_fast",
            Self::ExclamationSoft => "text_emotion.exclamation_soft",
            Self::Exclamation => "text_emotion.exclamation",
            Self::ExclamationStrong => "text_emotion.exclamation_strong",
            Self::ExclamationExtreme => "text_emotion.exclamation_extreme",
            Self::ExclamationHuge => "text_emotion.exclamation_huge",
        }
    }
}

/// Half-open character range. The editor cursor already uses Unicode scalar
/// boundaries; rendering maps these ranges to grapheme clusters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEmotionSpan {
    pub start: u32,
    pub end: u32,
    pub emotion: TextEmotion,
}

pub fn rebase_text_emotions(
    spans: &[TextEmotionSpan],
    old_text: &str,
    new_text: &str,
) -> Vec<TextEmotionSpan> {
    let old: Vec<char> = old_text.chars().collect();
    let new: Vec<char> = new_text.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let delta = new_end as i64 - old_end as i64;

    let mut out = Vec::new();
    for span in spans.iter().copied() {
        let start = span.start as usize;
        let end = span.end as usize;
        if old_end == prefix && start < prefix && end > prefix {
            out.push(TextEmotionSpan {
                end: (end as i64 + delta).max(span.start as i64) as u32,
                ..span
            });
        } else if end <= prefix {
            out.push(span);
        } else if start >= old_end {
            out.push(TextEmotionSpan {
                start: (start as i64 + delta).max(0) as u32,
                end: (end as i64 + delta).max(0) as u32,
                ..span
            });
        } else {
            if start < prefix {
                out.push(TextEmotionSpan {
                    end: prefix as u32,
                    ..span
                });
            }
            if end > old_end {
                out.push(TextEmotionSpan {
                    start: new_end as u32,
                    end: (end as i64 + delta).max(new_end as i64) as u32,
                    ..span
                });
            }
        }
    }
    out.retain(|span| span.start < span.end);
    out.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<TextEmotionSpan> = Vec::with_capacity(out.len());
    for span in out {
        if let Some(previous) = merged
            .last_mut()
            .filter(|previous| previous.end == span.start && previous.emotion == span.emotion)
        {
            previous.end = span.end;
        } else {
            merged.push(span);
        }
    }
    merged
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_emotions: Vec<TextEmotionSpan>,
}

impl LinePresence {
    pub fn is_on(&self) -> bool {
        matches!(self, Self::On)
    }
}

impl RythmoLine {
    pub fn can_have_text_emotions(&self) -> bool {
        self.kind.is_dialogue() && !self.karaoke
    }

    pub fn set_text_emotion(&mut self, start: usize, end: usize, emotion: Option<TextEmotion>) {
        if !self.can_have_text_emotions() {
            return;
        }
        let len = self.text.chars().count();
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut start = start.min(len);
        let mut end = end.min(len);
        let mut cursor = 0;
        for grapheme in self.text.graphemes(true) {
            let next = cursor + grapheme.chars().count();
            if cursor < start && start < next {
                start = cursor;
            }
            if cursor < end && end < next {
                end = next;
            }
            cursor = next;
        }
        let start = start as u32;
        let end = end as u32;
        if start >= end {
            return;
        }
        let mut spans = Vec::new();
        for span in self.text_emotions.drain(..) {
            if span.end <= start || span.start >= end {
                spans.push(span);
            } else {
                if span.start < start {
                    spans.push(TextEmotionSpan { end: start, ..span });
                }
                if span.end > end {
                    spans.push(TextEmotionSpan { start: end, ..span });
                }
            }
        }
        if let Some(emotion) = emotion {
            spans.push(TextEmotionSpan {
                start,
                end,
                emotion,
            });
        }
        spans.sort_by_key(|span| (span.start, span.end));
        self.text_emotions = spans;
    }

    pub fn emotion_at_char(&self, index: usize) -> Option<TextEmotion> {
        self.text_emotions
            .iter()
            .find(|span| span.start as usize <= index && index < span.end as usize)
            .map(|span| span.emotion)
    }

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
        reading_bar_offset_frames: f64,
    ) -> (f32, f32) {
        let x1 = center_x
            + (self.start_frame as f64 - current_frame - reading_bar_offset_frames) as f32
                * pixels_per_frame;
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

    #[test]
    fn emotion_spans_follow_text_edits() {
        let span = TextEmotionSpan {
            start: 1,
            end: 4,
            emotion: TextEmotion::Wave,
        };
        assert_eq!(
            rebase_text_emotions(&[span], "abcde", "abXcde"),
            vec![TextEmotionSpan { end: 5, ..span }]
        );
        assert_eq!(
            rebase_text_emotions(&[span], "abcde", "abde"),
            vec![TextEmotionSpan { end: 3, ..span }]
        );
    }

    #[test]
    fn emotion_ranges_replace_only_the_requested_text() {
        let mut line = RythmoLine {
            id: 1,
            start_frame: 0,
            duration_frames: 24,
            y_slot: 0.0,
            text: "Bonjour".into(),
            character_name: "A".into(),
            character_color: [1.0; 4],
            kind: RythmoLineKind::Dialogue,
            voice_actor_names: vec![],
            syllable_ratios: vec![],
            karaoke: false,
            note: String::new(),
            presence: LinePresence::On,
            text_emotions: vec![],
        };
        line.set_text_emotion(0, 7, Some(TextEmotion::Wave));
        line.set_text_emotion(2, 5, Some(TextEmotion::Bounce));
        assert_eq!(
            line.text_emotions,
            vec![
                TextEmotionSpan {
                    start: 0,
                    end: 2,
                    emotion: TextEmotion::Wave,
                },
                TextEmotionSpan {
                    start: 2,
                    end: 5,
                    emotion: TextEmotion::Bounce,
                },
                TextEmotionSpan {
                    start: 5,
                    end: 7,
                    emotion: TextEmotion::Wave,
                },
            ]
        );
        line.set_text_emotion(2, 5, None);
        assert_eq!(line.text_emotions.len(), 2);
        line.karaoke = true;
        line.set_text_emotion(0, 7, Some(TextEmotion::Shake));
        assert!(line
            .text_emotions
            .iter()
            .all(|span| span.emotion != TextEmotion::Shake));

        line.karaoke = false;
        line.text = "e\u{301}!".into();
        line.text_emotions.clear();
        line.set_text_emotion(1, 2, Some(TextEmotion::Wave));
        assert_eq!(
            (line.text_emotions[0].start, line.text_emotions[0].end),
            (0, 2)
        );
    }

    #[test]
    fn every_emotion_produces_finite_animation_data() {
        for emotion in TextEmotion::ALL {
            let animation = text_emotion_transform(emotion, 2, 8, 1.25);
            assert!(animation
                .offset
                .into_iter()
                .chain(animation.transform)
                .chain(animation.tint)
                .all(f32::is_finite));
        }
    }

    #[test]
    fn bounce_traverses_any_text_length_in_one_second() {
        for grapheme_count in [1, 2, 8, 32] {
            for grapheme_index in 0..grapheme_count {
                let seconds = (grapheme_index as f32 + 0.5) / grapheme_count as f32;
                let animation = text_emotion_transform(
                    TextEmotion::Bounce,
                    grapheme_index,
                    grapheme_count,
                    seconds,
                );
                assert!(animation.offset[1] < 0.0);
            }
        }
    }
}
