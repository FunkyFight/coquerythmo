//! Convert a [`PhoneticLine`] into detection cues positioned on the letters.
//!
//! **Never** derives timing from audio: positions come from grapheme ranges
//! only (evenly distributed over the line duration, warped by existing sync
//! points). The user then adjusts signs by hand.

use crate::detection::{DetectionCue, DetectionCueId, MediaTick, TextAnchor};
use crate::phonetics::mapping::{map_sequence, DetectionMapping};
use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::types::{PhoneticLine, TextRange};

/// Where on its grapheme group a sign is placed (start|center fraction).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SignPlacement {
    /// At the beginning of the group.
    Start,
    /// At the center of the group. Default: visually centered under letters.
    #[default]
    Center,
    /// Distributed evenly when several phonemes come from the same group.
    Distributed,
}

/// Options controlling conversion of one line.
#[derive(Clone, Debug)]
pub struct GenerationOptions {
    pub mapping: DetectionMapping,
    pub placement: SignPlacement,
    /// Include optional phonemes (liaison proposals).
    pub include_optional: bool,
    /// Line frame span.
    pub start_frame: i64,
    pub duration_frames: i64,
    /// Character progress positions within the line (0..=1 per character
    /// boundary). `None` → uniform over grapheme count.
    pub progress: Option<Vec<f32>>,
    /// Grapheme count in the displayed text (for uniform fallback).
    pub grapheme_count: usize,
    /// First cue id to allocate (from `LineDetectionData::next_detection_id`).
    pub first_id: DetectionCueId,
}

/// What happened during generation — for preview display.
#[derive(Clone, Debug)]
pub struct GenerationResult {
    pub cues: Vec<DetectionCue>,
    /// Ids of cues produced from optional phonemes (liaison proposals).
    pub optional_cues: Vec<DetectionCueId>,
    /// Ranges of words marked unknown (highlight in preview).
    pub unknown_ranges: Vec<TextRange>,
    /// Tokens with more than one candidate (shown as ambiguous).
    pub ambiguous_ranges: Vec<TextRange>,
}

/// Flattened phoneme with its source range.
struct FlatPhoneme<'a> {
    phoneme: Phoneme,
    optional: bool,
    range: TextRange,
    _private: std::marker::PhantomData<&'a ()>,
}

/// Convert one analyzed line into cues.
pub fn generate_cues(line: &PhoneticLine, options: &GenerationOptions) -> GenerationResult {
    // 1) Flatten the phoneme stream with ranges, honoring optionals.
    let mut flat: Vec<FlatPhoneme> = Vec::new();
    for token in &line.tokens {
        let candidate = match token.selected() {
            Some(c) => c,
            None => continue,
        };
        for segment in &candidate.segments {
            let count = segment.phonemes.len();
            for (idx, occurrence) in segment.phonemes.iter().enumerate() {
                if occurrence.optional && !options.include_optional {
                    continue;
                }
                // Distribute multiple phonemes from the same group over its span.
                let sub_range = match options.placement {
                    SignPlacement::Distributed if count > 1 => {
                        let len = segment.range.len();
                        let start = segment.range.start + len * idx / count;
                        let end = segment.range.start + len * (idx + 1) / count;
                        TextRange::new(start, end.max(start + 1))
                    }
                    _ => segment.range,
                };
                flat.push(FlatPhoneme {
                    phoneme: occurrence.phoneme,
                    optional: occurrence.optional,
                    range: sub_range,
                    _private: std::marker::PhantomData,
                });
            }
        }
    }

    // 2) Map phoneme sequences to sign instances.
    let phoneme_seq: Vec<Phoneme> = flat.iter().map(|p| p.phoneme).collect();
    let sign_instances = map_sequence(&options.mapping, &phoneme_seq);

    // 3) Emit cues. A sign covering several phonemes spans from the first
    //    phoneme range start to the last phoneme range end.
    let mut cues = Vec::new();
    let mut next_id = options.first_id.0;
    let mut optional_cues = Vec::new();
    for instance in sign_instances {
        let first = &flat[instance.first_phoneme];
        let last = &flat[instance.last_phoneme];
        let range = TextRange::new(first.range.start, last.range.end);
        let tick = tick_for_range(range, options, MatchPoint::Center);
        let target = if range.len() <= 1 {
            TextAnchor::Grapheme {
                index: range.start as u32,
            }
        } else {
            TextAnchor::GraphemeRange {
                start: range.start as u32,
                end: range.end as u32,
            }
        };
        let id = DetectionCueId(next_id);
        next_id += 1;
        cues.push(DetectionCue {
            id,
            kind: instance.kind,
            media_tick: tick,
            duration: MediaTick::ZERO,
            target,
        });
        if first.optional || last.optional {
            optional_cues.push(id);
        }
    }

    GenerationResult {
        cues,
        optional_cues,
        unknown_ranges: line
            .unknown_tokens()
            .map(|token| token.range)
            .collect(),
        ambiguous_ranges: line
            .ambiguous_tokens()
            .map(|token| token.range)
            .collect(),
    }
}

enum MatchPoint {
    Center,
}

/// Temporal position of a grapheme range. Uses character progress data when
/// available (warped by sync points), else spreads uniformly over the line.
fn tick_for_range(range: TextRange, options: &GenerationOptions, point: MatchPoint) -> MediaTick {
    let progress = |grapheme_index: usize| -> f64 {
        if let Some(progress) = &options.progress {
            let clamped = grapheme_index.min(progress.len().saturating_sub(1));
            return f64::from(progress[clamped]).clamp(0.0, 1.0);
        }
        let count = options.grapheme_count.max(1) as f64;
        (grapheme_index as f64 / count).clamp(0.0, 1.0)
    };
    let frac = match point {
        MatchPoint::Center => {
            let start = progress(range.start);
            let end = progress(range.end.min(options.grapheme_count));
            if end >= start {
                (start + end) / 2.0
            } else {
                start
            }
        }
    };
    let frame = options.start_frame as f64 + frac * options.duration_frames.max(1) as f64;
    MediaTick::from_frame_position(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonetics::converter_for;
    use crate::phonetics::mapping::default_mapping;
    use crate::phonetics::phoneme::Language;

    fn options(placement: SignPlacement, grapheme_count: usize) -> GenerationOptions {
        GenerationOptions {
            mapping: default_mapping(Language::French),
            placement,
            include_optional: false,
            start_frame: 0,
            duration_frames: 100,
            progress: None,
            grapheme_count,
            first_id: DetectionCueId(1),
        }
    }

    #[test]
    fn chat_produces_signs_anchored_to_letters() {
        let converter = converter_for(Language::French);
        let line = converter.convert("chat"); // graphemes: c h a t
        let result = generate_cues(&line, &options(SignPlacement::Center, 4));
        assert!(!result.cues.is_empty());
        // Tout signe doit avoir une ancre grapheme valide dans "chat" (0..4)
        for cue in &result.cues {
            let index = cue.target.grapheme_index().unwrap();
            assert!(index < 4, "anchor {index} out of range");
        }
    }

    #[test]
    fn phonemes_from_same_group_distribute() {
        // "ci" → c [s] i [i] : 2 signes, ordre temporel croissant.
        let converter = converter_for(Language::French);
        let line = converter.convert("chose"); // ch = 1 signe, o, s, e muet
        let result = generate_cues(&line, &options(SignPlacement::Distributed, 5));
        let mut last_tick = MediaTick::ZERO;
        for cue in &result.cues {
            assert!(cue.media_tick >= last_tick, "sign order must be monotonic");
            last_tick = cue.media_tick;
        }
    }
}
