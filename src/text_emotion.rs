//! Persisted text-emotion ranges and render metadata.
//!
//! Ranges are expressed in extended grapheme clusters, never bytes or scalar
//! values. This keeps accents, emoji and ligatures together when an animation
//! is applied to only part of a dialogue.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

const RENDER_PREFIX: &str = "\u{e000}cqte:";
const RENDER_SEPARATOR: char = '\u{e001}';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

impl TextEmotion {
    pub const ALL: [Self; 9] = [
        Self::Pendulum,
        Self::Swing,
        Self::Yay,
        Self::Bounce,
        Self::Slide,
        Self::Oscillation,
        Self::Wave,
        Self::Shake,
        Self::Wiggle,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Pendulum => "Pendule",
            Self::Swing => "Balancement",
            Self::Yay => "YAY!!!",
            Self::Bounce => "Bounce",
            Self::Slide => "Glissade",
            Self::Oscillation => "Oscillation",
            Self::Wave => "Vague",
            Self::Shake => "Tremblement",
            Self::Wiggle => "Wiggle",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEmotionSpan {
    pub start_grapheme: usize,
    pub end_grapheme: usize,
    pub emotion: TextEmotion,
}

impl TextEmotionSpan {
    pub fn contains(&self, grapheme: usize) -> bool {
        grapheme >= self.start_grapheme && grapheme < self.end_grapheme
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineTextEmotions {
    #[serde(default)]
    pub source_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<TextEmotionSpan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEmotionDocument {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lines: BTreeMap<u64, LineTextEmotions>,
}

fn document() -> &'static RwLock<TextEmotionDocument> {
    static DOCUMENT: OnceLock<RwLock<TextEmotionDocument>> = OnceLock::new();
    DOCUMENT.get_or_init(|| RwLock::new(load_local_document().unwrap_or_default()))
}

fn animation_epoch() -> &'static Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now)
}

pub fn animation_seconds() -> f32 {
    animation_epoch().elapsed().as_secs_f32()
}

pub fn animation_phase_bucket() -> u64 {
    (animation_seconds() * 60.0).floor() as u64
}

pub fn snapshot() -> TextEmotionDocument {
    document().read().map(|value| value.clone()).unwrap_or_default()
}

pub fn replace_snapshot(value: TextEmotionDocument) {
    if let Ok(mut target) = document().write() {
        *target = value;
    }
    persist_local_document();
}

pub fn clear() {
    replace_snapshot(TextEmotionDocument::default());
}

pub fn has_any() -> bool {
    document()
        .read()
        .map(|value| value.lines.values().any(|line| !line.spans.is_empty()))
        .unwrap_or(false)
}

pub fn has_line(line_id: u64) -> bool {
    document()
        .read()
        .ok()
        .and_then(|value| value.lines.get(&line_id).cloned())
        .is_some_and(|line| !line.spans.is_empty())
}

pub fn has_line_for_text(line_id: u64, text: &str) -> bool {
    document()
        .read()
        .ok()
        .and_then(|value| value.lines.get(&line_id).cloned())
        .is_some_and(|line| line.source_text == text && !line.spans.is_empty())
}

pub fn spans_for_line(line_id: u64, text: &str) -> Vec<TextEmotionSpan> {
    let grapheme_count = text.graphemes(true).count();
    document()
        .read()
        .ok()
        .and_then(|value| value.lines.get(&line_id).cloned())
        .filter(|line| line.source_text == text)
        .map(|line| {
            line.spans
                .into_iter()
                .filter_map(|mut span| {
                    span.start_grapheme = span.start_grapheme.min(grapheme_count);
                    span.end_grapheme = span.end_grapheme.min(grapheme_count);
                    (span.start_grapheme < span.end_grapheme).then_some(span)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn emotion_at(line_id: u64, text: &str, grapheme: usize) -> Option<TextEmotion> {
    spans_for_line(line_id, text)
        .into_iter()
        .find(|span| span.contains(grapheme))
        .map(|span| span.emotion)
}

/// Apply one emotion to a grapheme range. `None` removes every emotion in the
/// range. Existing ranges are split as necessary so unaffected text keeps its
/// previous animation.
pub fn apply_range(
    line_id: u64,
    text: &str,
    start_grapheme: usize,
    end_grapheme: usize,
    emotion: Option<TextEmotion>,
) -> bool {
    let grapheme_count = text.graphemes(true).count();
    let start = start_grapheme.min(grapheme_count);
    let end = end_grapheme.min(grapheme_count);
    if start >= end {
        return false;
    }

    let mut changed = false;
    if let Ok(mut value) = document().write() {
        let line = value.lines.entry(line_id).or_default();
        if line.source_text != text {
            line.spans.clear();
        }
        line.source_text = text.to_string();
        let previous = line.spans.clone();
        let mut next = Vec::with_capacity(previous.len() + usize::from(emotion.is_some()));

        for span in previous {
            if span.end_grapheme <= start || span.start_grapheme >= end {
                next.push(span);
                continue;
            }
            if span.start_grapheme < start {
                next.push(TextEmotionSpan {
                    start_grapheme: span.start_grapheme,
                    end_grapheme: start,
                    emotion: span.emotion,
                });
            }
            if span.end_grapheme > end {
                next.push(TextEmotionSpan {
                    start_grapheme: end,
                    end_grapheme: span.end_grapheme,
                    emotion: span.emotion,
                });
            }
        }

        if let Some(emotion) = emotion {
            next.push(TextEmotionSpan {
                start_grapheme: start,
                end_grapheme: end,
                emotion,
            });
        }
        normalize_spans(&mut next);
        changed = line.spans != next;
        line.spans = next;
        if line.spans.is_empty() {
            value.lines.remove(&line_id);
        }
    }

    if changed {
        persist_local_document();
    }
    changed
}

pub fn remove_line(line_id: u64) {
    let changed = document()
        .write()
        .map(|mut value| value.lines.remove(&line_id).is_some())
        .unwrap_or(false);
    if changed {
        persist_local_document();
    }
}

pub fn rebase_after_text_edit(line_id: u64, old_text: &str, new_text: &str) {
    let old: Vec<&str> = old_text.graphemes(true).collect();
    let new: Vec<&str> = new_text.graphemes(true).collect();
    if old == new {
        return;
    }

    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_changed_end = old.len().saturating_sub(suffix);
    let new_changed_end = new.len().saturating_sub(suffix);
    let delta = new_changed_end as isize - old_changed_end as isize;

    if let Ok(mut value) = document().write() {
        let Some(line) = value.lines.get_mut(&line_id) else {
            return;
        };
        if line.source_text != old_text {
            return;
        }
        line.source_text = new_text.to_string();
        for span in &mut line.spans {
            span.start_grapheme = rebase_boundary(
                span.start_grapheme,
                prefix,
                old_changed_end,
                new_changed_end,
                delta,
            );
            span.end_grapheme = rebase_boundary(
                span.end_grapheme,
                prefix,
                old_changed_end,
                new_changed_end,
                delta,
            );
            span.start_grapheme = span.start_grapheme.min(new.len());
            span.end_grapheme = span.end_grapheme.min(new.len());
        }
        line.spans
            .retain(|span| span.start_grapheme < span.end_grapheme);
        normalize_spans(&mut line.spans);
        if line.spans.is_empty() {
            value.lines.remove(&line_id);
        }
    }
    persist_local_document();
}

fn rebase_boundary(
    boundary: usize,
    prefix: usize,
    old_changed_end: usize,
    new_changed_end: usize,
    delta: isize,
) -> usize {
    if boundary <= prefix {
        boundary
    } else if boundary >= old_changed_end {
        boundary.saturating_add_signed(delta)
    } else {
        new_changed_end
    }
}

fn normalize_spans(spans: &mut Vec<TextEmotionSpan>) {
    spans.sort_by_key(|span| (span.start_grapheme, span.end_grapheme));
    let mut merged: Vec<TextEmotionSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        if let Some(previous) = merged.last_mut() {
            if previous.emotion == span.emotion && previous.end_grapheme == span.start_grapheme {
                previous.end_grapheme = span.end_grapheme;
                continue;
            }
        }
        merged.push(span);
    }
    *spans = merged;
}

pub fn encode_render_text(line_id: u64, text: &str) -> String {
    if !has_line_for_text(line_id, text) {
        return text.to_string();
    }
    format!(
        "{RENDER_PREFIX}{line_id}:{};{RENDER_SEPARATOR}{text}",
        animation_phase_bucket()
    )
}

pub fn decode_render_text(value: &str) -> Option<(u64, u64, &str)> {
    let metadata = value.strip_prefix(RENDER_PREFIX)?;
    let (header, text) = metadata.split_once(RENDER_SEPARATOR)?;
    let header = header.strip_suffix(';')?;
    let (line_id, phase) = header.split_once(':')?;
    Some((line_id.parse().ok()?, phase.parse().ok()?, text))
}

pub fn plain_render_text(value: &str) -> &str {
    decode_render_text(value)
        .map(|(_, _, text)| text)
        .unwrap_or(value)
}

fn local_document_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("coquerythmo");
    path.push("text-emotions.json");
    Some(path)
}

fn load_local_document() -> Option<TextEmotionDocument> {
    let path = local_document_path()?;
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn persist_local_document() {
    let Some(path) = local_document_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(value) = document().read() else {
        return;
    };
    if let Ok(data) = serde_json::to_vec_pretty(&*value) {
        let _ = fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_split_and_merge_without_breaking_emoji() {
        clear();
        let text = "A👨‍👩‍👧‍👦BC";
        assert_eq!(text.graphemes(true).count(), 4);
        assert!(apply_range(1, text, 0, 4, Some(TextEmotion::Wave)));
        assert!(apply_range(1, text, 1, 3, Some(TextEmotion::Bounce)));
        assert_eq!(
            spans_for_line(1, text),
            vec![
                TextEmotionSpan {
                    start_grapheme: 0,
                    end_grapheme: 1,
                    emotion: TextEmotion::Wave,
                },
                TextEmotionSpan {
                    start_grapheme: 1,
                    end_grapheme: 3,
                    emotion: TextEmotion::Bounce,
                },
                TextEmotionSpan {
                    start_grapheme: 3,
                    end_grapheme: 4,
                    emotion: TextEmotion::Wave,
                },
            ]
        );
    }

    #[test]
    fn source_text_prevents_cross_project_id_collisions() {
        clear();
        apply_range(42, "Bonjour", 0, 7, Some(TextEmotion::Pendulum));
        assert!(has_line_for_text(42, "Bonjour"));
        assert!(!has_line_for_text(42, "Au revoir"));
        assert!(spans_for_line(42, "Au revoir").is_empty());
    }

    #[test]
    fn encoded_render_text_round_trips() {
        clear();
        apply_range(42, "Bonjour", 0, 7, Some(TextEmotion::Pendulum));
        let encoded = encode_render_text(42, "Bonjour");
        let (line, _, plain) = decode_render_text(&encoded).unwrap();
        assert_eq!(line, 42);
        assert_eq!(plain, "Bonjour");
    }
}
