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

/// Typographic style of a run of non-karaoke dialogue text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStyleKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
}

impl TextStyleKind {
    pub const ALL: [Self; 4] = [
        Self::Bold,
        Self::Italic,
        Self::Underline,
        Self::Strikethrough,
    ];

    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Bold => "text_style.bold",
            Self::Italic => "text_style.italic",
            Self::Underline => "text_style.underline",
            Self::Strikethrough => "text_style.strikethrough",
        }
    }
}

impl TextStyle {
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    pub fn has(&self, kind: TextStyleKind) -> bool {
        match kind {
            TextStyleKind::Bold => self.bold,
            TextStyleKind::Italic => self.italic,
            TextStyleKind::Underline => self.underline,
            TextStyleKind::Strikethrough => self.strikethrough,
        }
    }

    pub fn with(mut self, kind: TextStyleKind, on: bool) -> Self {
        match kind {
            TextStyleKind::Bold => self.bold = on,
            TextStyleKind::Italic => self.italic = on,
            TextStyleKind::Underline => self.underline = on,
            TextStyleKind::Strikethrough => self.strikethrough = on,
        }
        self
    }
}

/// Half-open character range sharing one style. Spans never overlap and plain
/// text has no span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextStyleSpan {
    pub start: u32,
    pub end: u32,
    pub style: TextStyle,
}

fn char_styles(spans: &[TextStyleSpan], len: usize) -> Vec<TextStyle> {
    let mut styles = vec![TextStyle::default(); len];
    for span in spans {
        let end = (span.end as usize).min(len);
        for style in styles.iter_mut().take(end).skip(span.start as usize) {
            *style = span.style;
        }
    }
    styles
}

fn spans_from_char_styles(styles: &[TextStyle]) -> Vec<TextStyleSpan> {
    let mut spans: Vec<TextStyleSpan> = Vec::new();
    for (index, style) in styles.iter().enumerate() {
        if style.is_plain() {
            continue;
        }
        match spans.last_mut() {
            Some(last) if last.end as usize == index && last.style == *style => {
                last.end += 1;
            }
            _ => spans.push(TextStyleSpan {
                start: index as u32,
                end: index as u32 + 1,
                style: *style,
            }),
        }
    }
    spans
}

/// Follows an edit of the text. Inserted text takes the style of the text it
/// replaces, or of the character before it, as in a word processor.
pub fn rebase_text_styles(
    spans: &[TextStyleSpan],
    old_text: &str,
    new_text: &str,
) -> Vec<TextStyleSpan> {
    if spans.is_empty() {
        return Vec::new();
    }
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
    let old_styles = char_styles(spans, old.len());
    let inserted = if old_end > prefix {
        old_styles[prefix]
    } else if prefix > 0 {
        old_styles[prefix - 1]
    } else {
        TextStyle::default()
    };
    let mut styles = Vec::with_capacity(new.len());
    styles.extend_from_slice(&old_styles[..prefix]);
    styles.extend(std::iter::repeat(inserted).take(new_end - prefix));
    styles.extend_from_slice(&old_styles[old_end..]);
    spans_from_char_styles(&styles)
}

/// Styled runs inside the character range `start..end`, as ranges relative to
/// `start`.
pub fn text_style_runs(
    spans: &[TextStyleSpan],
    start: usize,
    end: usize,
) -> Vec<(usize, usize, TextStyle)> {
    spans
        .iter()
        .filter_map(|span| {
            let run_start = (span.start as usize).max(start);
            let run_end = (span.end as usize).min(end);
            (run_start < run_end).then(|| (run_start - start, run_end - start, span.style))
        })
        .collect()
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
    /// Words of the line (see [`crate::syllable::split_word_key`]) whose pin
    /// is on: they are cut into all their written syllables ("mons|tres").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syllable_split_words: Vec<String>,
    /// Liaisons marked on the line: made, or expressly not made. A place
    /// where a liaison is possible and that is not listed shows nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub liaison_marks: Vec<LiaisonMark>,
    #[serde(default)]
    pub karaoke: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "LinePresence::is_on")]
    pub presence: LinePresence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_emotions: Vec<TextEmotionSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_styles: Vec<TextStyleSpan>,
}

impl LinePresence {
    pub fn is_on(&self) -> bool {
        matches!(self, Self::On)
    }
}

/// A liaison marked between two words of a line ("les amis", "rien à").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiaisonMark {
    /// Key of the two words (see [`crate::syllable::liaison_pair_key`]):
    /// every place of the line where they follow each other shares it.
    pub pair: String,
    /// The liaison is made (an arrow from the consonant to the vowel), or
    /// expressly not made (the same arrow, struck through).
    pub made: bool,
}

/// Where the karaoke dot is on a line, and on which syllable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KaraokeDotState {
    /// How far the dot is into the syllable being sung (0 to 1): it lands
    /// at 0 and at 1.
    pub local: f32,
    /// Place of the dot along the drawn text (0 to 1).
    pub position: f32,
    /// Start and end of the syllable being sung, along the drawn text.
    pub span: (f32, f32),
    /// Share of the duration of the line this syllable lasts.
    pub share: f32,
}

impl RythmoLine {
    /// How the text of this line is cut into syllables in `lang`: the
    /// language of the band and the words whose pin is on.
    pub fn syllable_rules<'a>(&'a self, lang: &'a str) -> crate::syllable::SyllableRules<'a> {
        crate::syllable::SyllableRules::new(lang, &self.syllable_split_words)
    }

    /// How long each syllable lasts and how wide it is drawn, as shares of
    /// the line: what the editor shows as handles. A line whose handles were
    /// never moved is timed by the width of its syllables as drawn, since
    /// that is where its handles stand. `None` for a one-syllable line.
    pub fn karaoke_syllable_layout(&self, lang: &str) -> Option<(Vec<f32>, Vec<f32>)> {
        let rules = self.syllable_rules(lang);
        let breaks = crate::syllable::syllable_breaks(&self.text, rules);
        if breaks.is_empty() {
            return None;
        }
        let drawn = if crate::config::is_initialized() {
            let font_size = crate::config::get().ui.font_size * 2.0;
            text_emotion_char_ratios(&self.text, font_size).and_then(|positions| {
                crate::syllable::visual_ratios_from_char_positions(&self.text, &breaks, &positions)
            })
        } else {
            None
        };
        let visual = drawn
            .unwrap_or_else(|| crate::syllable::default_ratios_from_breaks(&self.text, &breaks));
        let timing =
            crate::syllable::adapted_saved_ratios(&self.text, &self.syllable_ratios, rules)
                .filter(|ratios| ratios.len() == visual.len())
                .unwrap_or_else(|| visual.clone());
        Some((timing, visual))
    }

    /// What is marked for the liaison of `pair`: made, not made, or nothing.
    pub fn liaison_state(&self, pair: &str) -> Option<bool> {
        self.liaison_marks
            .iter()
            .find(|mark| mark.pair == pair)
            .map(|mark| mark.made)
    }

    /// Liaisons marked on the line, for the band: the middle of the
    /// consonant and of the vowel as shares of the width of the drawn text,
    /// and whether the liaison is made.
    pub fn liaison_marks_layout(&self, lang: &str) -> Vec<(f32, f32, bool)> {
        if self.liaison_marks.is_empty() {
            return Vec::new();
        }
        let sites = crate::syllable::liaison_sites(&self.text, lang);
        if sites.is_empty() {
            return Vec::new();
        }
        let char_count = self.text.chars().count().max(1);
        let drawn = if crate::config::is_initialized() {
            let font_size = crate::config::get().ui.font_size * 2.0;
            text_emotion_char_ratios(&self.text, font_size)
                .filter(|positions| positions.len() == char_count + 1)
        } else {
            None
        };
        let position = |character: usize| -> f32 {
            match &drawn {
                Some(positions) => positions[character.min(char_count)],
                None => character.min(char_count) as f32 / char_count as f32,
            }
        };
        let middle = |character: usize| (position(character) + position(character + 1)) / 2.0;
        sites
            .into_iter()
            .filter_map(|site| {
                let made = self.liaison_state(&site.pair)?;
                Some((middle(site.from), middle(site.to), made))
            })
            .collect()
    }

    /// Mute syllables to pronounce: for every word whose pin is on, the
    /// centre of the syllable the pin adds ("tres" in "monstres"), as a share
    /// of the width of the drawn text. Marked under the line during playback.
    pub fn pronounced_syllable_marks(&self, lang: &str) -> Vec<f32> {
        if self.syllable_split_words.is_empty() {
            return Vec::new();
        }
        let pins = crate::syllable::syllable_split_pins(&self.text, self.syllable_rules(lang));
        if !pins.iter().any(|pin| pin.active) {
            return Vec::new();
        }
        let char_count = self.text.chars().count().max(1);
        let drawn = if crate::config::is_initialized() {
            let font_size = crate::config::get().ui.font_size * 2.0;
            text_emotion_char_ratios(&self.text, font_size)
                .filter(|positions| positions.len() == char_count + 1)
        } else {
            None
        };
        let position = |character: usize| -> f32 {
            match &drawn {
                Some(positions) => positions[character.min(char_count)],
                None => character.min(char_count) as f32 / char_count as f32,
            }
        };
        pins.into_iter()
            .filter(|pin| pin.active)
            .map(|pin| ((position(pin.split_at) + position(pin.end)) / 2.0).clamp(0.0, 1.0))
            .collect()
    }

    /// The karaoke dot at `progress` (0 to 1) of the line: how far it is
    /// into the syllable being sung (it lands on every syllable boundary,
    /// where the handles are), and its place along the drawn text.
    pub fn karaoke_dot_progress(&self, lang: &str, progress: f32) -> (f32, f32) {
        let state = self.karaoke_dot_state(lang, progress);
        (state.local, state.position)
    }

    /// The karaoke dot at `progress` (0 to 1) of the line, with the syllable
    /// it is on.
    pub fn karaoke_dot_state(&self, lang: &str, progress: f32) -> KaraokeDotState {
        let progress = progress.clamp(0.0, 1.0);
        let Some((timing, visual)) = self.karaoke_syllable_layout(lang) else {
            return KaraokeDotState {
                local: progress,
                position: progress,
                span: (0.0, 1.0),
                share: 1.0,
            };
        };
        // Syllable being sung: the first one whose end is not passed yet.
        let mut index = timing.len() - 1;
        let mut time_start = 0.0_f32;
        for (candidate, ratio) in timing.iter().enumerate() {
            if progress <= (time_start + ratio).min(1.0) || candidate == timing.len() - 1 {
                index = candidate;
                break;
            }
            time_start += ratio;
        }
        let share = timing[index].max(f32::EPSILON);
        let local = ((progress - time_start) / share).clamp(0.0, 1.0);
        let span_start: f32 = visual[..index].iter().sum();
        let span_end = (span_start + visual[index]).min(1.0);
        KaraokeDotState {
            local,
            position: (span_start + local * visual[index]).clamp(0.0, 1.0),
            span: (span_start.clamp(0.0, 1.0), span_end),
            share,
        }
    }

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

    pub fn can_have_text_styles(&self) -> bool {
        self.can_have_text_emotions()
    }

    /// Character range snapped outward to grapheme boundaries. An empty range
    /// stands for the whole line.
    fn style_range(&self, start: usize, end: usize) -> (usize, usize) {
        let len = self.text.chars().count();
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        if start == end {
            return (0, len);
        }
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
        (start, end)
    }

    /// Whether every character of the range carries `kind`. An empty range
    /// stands for the whole line.
    pub fn text_style_active(&self, start: usize, end: usize, kind: TextStyleKind) -> bool {
        let (start, end) = self.style_range(start, end);
        if start >= end {
            return false;
        }
        let styles = char_styles(&self.text_styles, self.text.chars().count());
        styles[start..end].iter().all(|style| style.has(kind))
    }

    /// Adds `kind` to the range, or removes it when the whole range has it.
    pub fn toggle_text_style(&mut self, start: usize, end: usize, kind: TextStyleKind) {
        if !self.can_have_text_styles() {
            return;
        }
        let on = !self.text_style_active(start, end, kind);
        let (start, end) = self.style_range(start, end);
        let mut styles = char_styles(&self.text_styles, self.text.chars().count());
        for style in &mut styles[start..end] {
            *style = style.with(kind, on);
        }
        self.text_styles = spans_from_char_styles(&styles);
    }

    pub fn style_at_char(&self, index: usize) -> TextStyle {
        self.text_styles
            .iter()
            .find(|span| span.start as usize <= index && index < span.end as usize)
            .map(|span| span.style)
            .unwrap_or_default()
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

    fn styled_line(text: &str) -> RythmoLine {
        RythmoLine {
            id: 1,
            start_frame: 0,
            duration_frames: 24,
            y_slot: 0.0,
            text: text.into(),
            character_name: String::new(),
            character_color: [1.0; 4],
            kind: RythmoLineKind::Dialogue,
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            syllable_split_words: Vec::new(),
            liaison_marks: Vec::new(),
            karaoke: false,
            note: String::new(),
            presence: LinePresence::On,
            text_emotions: Vec::new(),
            text_styles: Vec::new(),
        }
    }

    #[test]
    fn marked_liaisons_are_laid_out_from_the_consonant_to_the_vowel() {
        let mut line = styled_line("les amis");
        assert!(line.liaison_marks_layout("fr-fr").is_empty());
        line.liaison_marks = vec![LiaisonMark {
            pair: "les amis".to_string(),
            made: true,
        }];
        let layout = line.liaison_marks_layout("fr-fr");
        assert_eq!(layout.len(), 1);
        let (from, to, made) = layout[0];
        // From the "s" (third letter of eight) to the "a" (fifth).
        assert!(made && from > 0.2 && from < 0.45 && to > from && to < 0.7);
        assert_eq!(line.liaison_state("les amis"), Some(true));
        assert_eq!(line.liaison_state("mes amis"), None);
        // No liaison outside French, nor for a pair that left the text.
        assert!(line.liaison_marks_layout("en-us").is_empty());
        line.text = "les chats".to_string();
        assert!(line.liaison_marks_layout("fr-fr").is_empty());
    }

    #[test]
    fn a_pin_that_is_on_marks_its_syllable_during_playback() {
        let mut line = styled_line("les monstres rient");
        line.karaoke = true;
        assert!(line.pronounced_syllable_marks("fr-fr").is_empty());
        line.syllable_split_words = vec!["monstres".to_string()];
        let marks = line.pronounced_syllable_marks("fr-fr");
        // One mark, under "tres" (characters 8 to 12 of 18): its exact place
        // follows the width of the letters as drawn.
        assert_eq!(marks.len(), 1);
        assert!(marks[0] > 0.45 && marks[0] < 0.67, "{marks:?}");
        // A word that left the text marks nothing.
        line.syllable_split_words = vec!["tables".to_string()];
        assert!(line.pronounced_syllable_marks("fr-fr").is_empty());
    }

    #[test]
    fn the_karaoke_dot_lands_on_every_syllable_handle() {
        let mut line = styled_line("la la la la");
        line.karaoke = true;
        // Handles dragged to 50 %, 60 % and 90 % of the line.
        line.syllable_ratios = vec![0.5, 0.1, 0.3, 0.1];
        for boundary in [0.0, 0.5, 0.6, 0.9, 1.0] {
            let (local, _) = line.karaoke_dot_progress("fr-fr", boundary);
            let bounce = (local * std::f32::consts::PI).sin();
            assert!(bounce.abs() < 1e-3, "{boundary}: {local}");
        }
        // The syllable it is on, and how long that syllable lasts.
        let state = line.karaoke_dot_state("fr-fr", 0.55);
        let (_, visual) = line.karaoke_syllable_layout("fr-fr").unwrap();
        assert!((state.span.0 - visual[0]).abs() < 1e-4);
        assert!((state.span.1 - visual[0] - visual[1]).abs() < 1e-4);
        assert!((state.share - 0.1).abs() < 1e-4 && (state.local - 0.5).abs() < 1e-3);
        // Halfway through a syllable it is at the top of its jump, above the
        // middle of that syllable in the text.
        let (local, position) = line.karaoke_dot_progress("fr-fr", 0.25);
        assert!((local - 0.5).abs() < 1e-4);
        let (_, visual) = line.karaoke_syllable_layout("fr-fr").unwrap();
        assert!((position - visual[0] / 2.0).abs() < 1e-4);

        // A line whose handles were never moved: they stand where the
        // syllables are drawn, and the dot lands there too.
        line.syllable_ratios.clear();
        let (timing, visual) = line.karaoke_syllable_layout("fr-fr").unwrap();
        assert_eq!(timing, visual);
        let mut boundary = 0.0;
        for ratio in &timing[..timing.len() - 1] {
            boundary += ratio;
            let (local, position) = line.karaoke_dot_progress("fr-fr", boundary);
            assert!((local * std::f32::consts::PI).sin().abs() < 1e-3);
            assert!((position - boundary).abs() < 1e-4);
        }
    }

    #[test]
    fn toggling_a_style_twice_restores_plain_text() {
        let mut line = styled_line("bonjour");
        line.toggle_text_style(0, 3, TextStyleKind::Bold);
        assert_eq!(line.text_styles.len(), 1);
        assert!(line.style_at_char(2).bold);
        assert!(!line.style_at_char(3).bold);
        line.toggle_text_style(0, 3, TextStyleKind::Bold);
        assert!(line.text_styles.is_empty());
    }

    #[test]
    fn partially_styled_range_gets_the_style_everywhere() {
        let mut line = styled_line("bonjour");
        line.toggle_text_style(0, 2, TextStyleKind::Italic);
        line.toggle_text_style(0, 5, TextStyleKind::Italic);
        assert!(line.text_style_active(0, 5, TextStyleKind::Italic));
        line.toggle_text_style(2, 4, TextStyleKind::Bold);
        assert_eq!(line.text_styles.len(), 3);
    }

    #[test]
    fn empty_range_styles_the_whole_line() {
        let mut line = styled_line("salut");
        line.toggle_text_style(2, 2, TextStyleKind::Underline);
        assert!(line.text_style_active(0, 5, TextStyleKind::Underline));
    }

    #[test]
    fn karaoke_lines_cannot_be_styled() {
        let mut line = styled_line("salut");
        line.karaoke = true;
        line.toggle_text_style(0, 5, TextStyleKind::Bold);
        assert!(line.text_styles.is_empty());
    }

    #[test]
    fn typing_inside_a_styled_run_extends_it() {
        let bold = TextStyle::default().with(TextStyleKind::Bold, true);
        let spans = vec![TextStyleSpan {
            start: 0,
            end: 3,
            style: bold,
        }];
        let rebased = rebase_text_styles(&spans, "bonjour", "bonnjour");
        assert_eq!(
            rebased,
            vec![TextStyleSpan {
                start: 0,
                end: 4,
                style: bold
            }]
        );
        let shifted = rebase_text_styles(&spans, "bonjour", "Xbonjour");
        assert_eq!(shifted[0].start, 1);
        assert_eq!(shifted[0].end, 4);
        let deleted = rebase_text_styles(&spans, "bonjour", "bjour");
        assert_eq!(deleted[0].end, 1);
    }

    #[test]
    fn style_runs_are_relative_to_the_segment() {
        let bold = TextStyle::default().with(TextStyleKind::Bold, true);
        let spans = vec![TextStyleSpan {
            start: 2,
            end: 6,
            style: bold,
        }];
        assert_eq!(text_style_runs(&spans, 4, 8), vec![(0, 2, bold)]);
        assert!(text_style_runs(&spans, 6, 8).is_empty());
    }

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
            syllable_split_words: Vec::new(),
            liaison_marks: Vec::new(),
            karaoke: false,
            note: String::new(),
            presence: LinePresence::On,
            text_emotions: vec![],
            text_styles: Vec::new(),
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
        line.kind = RythmoLineKind::AmbianceStart;
        line.text_emotions.clear();
        line.set_text_emotion(0, 7, Some(TextEmotion::Wave));
        assert!(line.text_emotions.is_empty());

        line.kind = RythmoLineKind::Dialogue;
        line.text = "e\u{301}!".into();
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
