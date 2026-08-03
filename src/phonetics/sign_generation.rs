//! Convert a [`PhoneticLine`] into detection cues positioned on its written
//! syllables.
//!
//! **Never** derives timing from audio: positions come from grapheme ranges
//! only (evenly distributed over the line duration, warped by existing sync
//! points). The user then adjusts signs by hand.

use crate::detection::{DetectionCue, DetectionCueId, DetectionKind, MediaTick, TextAnchor};
use crate::phonetics::mapping::{map_sequence, DetectionMapping, SignInstance};
use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::types::{PhoneticLine, TextRange};
use unicode_segmentation::UnicodeSegmentation;

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
struct FlatPhoneme {
    phoneme: Phoneme,
    optional: bool,
    range: TextRange,
}

struct GeneratedSign {
    kind: DetectionKind,
    range: TextRange,
    optional: bool,
}

fn syllable_ranges(line: &PhoneticLine, grapheme_count: usize) -> Vec<TextRange> {
    if grapheme_count == 0 {
        return Vec::new();
    }
    let mut edges = vec![0];
    for char_break in crate::syllable::syllable_breaks(&line.original_text, line.language.code()) {
        let grapheme_break =
            char_break_to_grapheme(&line.original_text, char_break).min(grapheme_count);
        if grapheme_break > 0 && grapheme_break < grapheme_count {
            edges.push(grapheme_break);
        }
    }
    edges.push(grapheme_count);
    edges.sort_unstable();
    edges.dedup();
    edges
        .windows(2)
        .map(|pair| TextRange::new(pair[0], pair[1]))
        .filter(|range| !range.is_empty())
        .collect()
}

fn char_break_to_grapheme(text: &str, char_break: usize) -> usize {
    let mut chars = 0;
    for (index, grapheme) in UnicodeSegmentation::graphemes(text, true).enumerate() {
        chars += grapheme.chars().count();
        if chars >= char_break {
            return index + 1;
        }
    }
    UnicodeSegmentation::graphemes(text, true).count()
}

fn syllable_index(ranges: &[TextRange], grapheme_index: usize) -> usize {
    ranges
        .iter()
        .position(|range| grapheme_index < range.end)
        .unwrap_or_else(|| ranges.len().saturating_sub(1))
}

/// Keep the one gesture that best describes a syllable. Lip closures win over
/// vowels, while a rounded vowel wins over incidental consonant articulation.
fn dominant_sign(instances: &[SignInstance]) -> Option<DetectionKind> {
    instances
        .iter()
        .map(|instance| instance.kind)
        .min_by_key(|kind| match kind {
            DetectionKind::Labial => 0,
            DetectionKind::SemiLabial => 1,
            DetectionKind::Pucker => 2,
            DetectionKind::ForwardWave => 3,
            DetectionKind::OpeningWave => 4,
            DetectionKind::MouthOpen => 5,
            DetectionKind::TeethVisible => 6,
            DetectionKind::MouthClosed => 7,
            DetectionKind::Breath | DetectionKind::Reaction => 8,
            DetectionKind::TextSyncPoint => 9,
        })
}

/// Convert one analyzed line into cues.
pub fn generate_cues(line: &PhoneticLine, options: &GenerationOptions) -> GenerationResult {
    // 1) Flatten the phoneme stream with ranges, honoring optionals.
    let mut flat: Vec<FlatPhoneme> = Vec::new();
    let syllables = syllable_ranges(line, options.grapheme_count);
    let mut generated_signs = Vec::new();
    for token in &line.tokens {
        let candidate = match token.selected() {
            Some(c) => c,
            None => continue,
        };
        let token_start = flat.len();
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
                });
            }
        }

        if options.placement == SignPlacement::Distributed {
            let token_phonemes: Vec<Phoneme> = flat[token_start..]
                .iter()
                .map(|phoneme| phoneme.phoneme)
                .collect();
            for instance in map_sequence(&options.mapping, &token_phonemes) {
                let first = &flat[token_start + instance.first_phoneme];
                let last = &flat[token_start + instance.last_phoneme];
                generated_signs.push(GeneratedSign {
                    kind: instance.kind,
                    range: TextRange::new(first.range.start, last.range.end),
                    optional: first.optional || last.optional,
                });
            }
            continue;
        }

        // Map each syllable independently, then keep its dominant gesture.
        // This prevents a consonant and its vowel from creating contradictory
        // cues such as "mouth closed" followed by "mouth open".
        let mut group_start = token_start;
        for cursor in token_start..=flat.len() {
            let at_group_end = cursor == flat.len()
                || (cursor > group_start
                    && syllable_index(&syllables, flat[cursor - 1].range.start)
                        != syllable_index(&syllables, flat[cursor].range.start));
            if !at_group_end {
                continue;
            }
            let group_end = cursor;
            if group_start < group_end {
                let phonemes: Vec<Phoneme> = flat[group_start..group_end]
                    .iter()
                    .map(|phoneme| phoneme.phoneme)
                    .collect();
                let instances = map_sequence(&options.mapping, &phonemes);
                if let Some(kind) = dominant_sign(&instances) {
                    let first = &flat[group_start];
                    let last = &flat[group_end - 1];
                    let range = syllables
                        .get(syllable_index(&syllables, first.range.start))
                        .copied()
                        .map(|syllable| {
                            TextRange::new(
                                syllable.start.max(token.range.start),
                                syllable.end.min(token.range.end),
                            )
                        })
                        .filter(|range| !range.is_empty())
                        .unwrap_or_else(|| TextRange::new(first.range.start, last.range.end));
                    generated_signs.push(GeneratedSign {
                        kind,
                        range,
                        optional: first.optional || last.optional,
                    });
                }
            }
            group_start = group_end;
        }
    }

    // 2) Emit one cue per syllable. The cue spans the written syllable so a
    //    silent final letter remains covered by the generated gesture.
    let mut cues = Vec::new();
    let mut next_id = options.first_id.0;
    let mut optional_cues = Vec::new();
    for instance in generated_signs {
        let range = instance.range;
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
            duration: duration_for_range(range, options),
            target,
        });
        if instance.optional {
            optional_cues.push(id);
        }
    }

    GenerationResult {
        cues,
        optional_cues,
        unknown_ranges: line.unknown_tokens().map(|token| token.range).collect(),
        ambiguous_ranges: line.ambiguous_tokens().map(|token| token.range).collect(),
    }
}

enum MatchPoint {
    Center,
}

/// Temporal position of a grapheme range. Uses character progress data when
/// available (warped by sync points), else spreads uniformly over the line.
fn tick_for_range(range: TextRange, options: &GenerationOptions, point: MatchPoint) -> MediaTick {
    let progress = |grapheme_index: usize| progress_at(grapheme_index, options);
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

fn progress_at(grapheme_index: usize, options: &GenerationOptions) -> f64 {
    if let Some(progress) = &options.progress {
        let clamped = grapheme_index.min(progress.len().saturating_sub(1));
        return f64::from(progress[clamped]).clamp(0.0, 1.0);
    }
    let count = options.grapheme_count.max(1) as f64;
    (grapheme_index as f64 / count).clamp(0.0, 1.0)
}

fn duration_for_range(range: TextRange, options: &GenerationOptions) -> MediaTick {
    let start = progress_at(range.start, options);
    let end = progress_at(range.end.min(options.grapheme_count), options);
    MediaTick::from_frame_position(
        ((end - start).max(0.0) * options.duration_frames.max(1) as f64).max(1.0),
    )
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
    fn nicole_uses_its_english_pronunciation_and_one_sign_per_syllable() {
        let converter = converter_for(Language::English);
        let line = converter.convert("Nicole");
        let phonemes: Vec<_> = line.tokens[0]
            .selected()
            .unwrap()
            .phonemes()
            .map(|occurrence| occurrence.phoneme)
            .collect();
        assert_eq!(
            phonemes,
            vec![
                Phoneme::AlveolarNasal,
                Phoneme::NearCloseFrontEn,
                Phoneme::VoicelessVelarPlosive,
                Phoneme::DiphthongGoatEn,
                Phoneme::AlveolarLateralApproximant,
            ]
        );
        let result = generate_cues(
            &line,
            &GenerationOptions {
                mapping: default_mapping(Language::English),
                placement: SignPlacement::Center,
                include_optional: false,
                start_frame: 0,
                duration_frames: 100,
                progress: None,
                grapheme_count: 6,
                first_id: DetectionCueId(1),
            },
        );
        assert_eq!(
            result.cues.iter().map(|cue| cue.kind).collect::<Vec<_>>(),
            vec![DetectionKind::TeethVisible, DetectionKind::ForwardWave]
        );
        assert_eq!(
            result
                .cues
                .iter()
                .map(|cue| cue.target.clone())
                .collect::<Vec<_>>(),
            vec![
                TextAnchor::GraphemeRange { start: 0, end: 2 },
                TextAnchor::GraphemeRange { start: 2, end: 6 },
            ]
        );
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

    #[test]
    fn sequence_rules_do_not_cross_word_boundaries() {
        use crate::detection::DetectionKind;
        use crate::phonetics::mapping::PhonemeSignRule;
        use crate::phonetics::phoneme::Phoneme;
        use crate::phonetics::types::{
            GraphemeSegment, PhonemeOccurrence, PhoneticToken, PronunciationCandidate,
        };

        let candidate =
            |grapheme: &str, range: TextRange, phoneme: Phoneme| PronunciationCandidate {
                source: crate::phonetics::PronunciationSource::Rule,
                confidence: 1.0,
                segments: vec![GraphemeSegment {
                    grapheme: grapheme.to_string(),
                    range,
                    phonemes: vec![PhonemeOccurrence::new(phoneme)],
                    silent: false,
                }],
            };
        let line = PhoneticLine {
            original_text: "a y".into(),
            language: Language::French,
            dialect: crate::phonetics::Dialect::Generic,
            tokens: vec![
                PhoneticToken {
                    text: "a".into(),
                    range: TextRange::new(0, 1),
                    kind: crate::phonetics::TokenKind::Word,
                    candidates: vec![candidate("a", TextRange::new(0, 1), Phoneme::CloseFront)],
                    selected_candidate: 0,
                    unknown: false,
                },
                PhoneticToken {
                    text: "y".into(),
                    range: TextRange::new(2, 3),
                    kind: crate::phonetics::TokenKind::Word,
                    candidates: vec![candidate(
                        "y",
                        TextRange::new(2, 3),
                        Phoneme::PalatalApproximant,
                    )],
                    selected_candidate: 0,
                    unknown: false,
                },
            ],
        };
        let mapping = DetectionMapping {
            name: "boundary-test".into(),
            language: Language::French,
            rules: vec![
                PhonemeSignRule {
                    phonemes: vec![Phoneme::CloseFront],
                    signs: vec![DetectionKind::MouthClosed],
                    priority: 1,
                },
                PhonemeSignRule {
                    phonemes: vec![Phoneme::PalatalApproximant],
                    signs: vec![DetectionKind::TeethVisible],
                    priority: 1,
                },
                PhonemeSignRule {
                    phonemes: vec![Phoneme::CloseFront, Phoneme::PalatalApproximant],
                    signs: vec![DetectionKind::MouthOpen],
                    priority: 5,
                },
            ],
        };
        let mut options = options(SignPlacement::Center, 3);
        options.mapping = mapping;
        let result = generate_cues(&line, &options);
        assert_eq!(
            result.cues.iter().map(|cue| cue.kind).collect::<Vec<_>>(),
            vec![DetectionKind::MouthClosed, DetectionKind::TeethVisible]
        );
    }

    #[test]
    fn expanded_unicode_graphemes_keep_their_original_anchor() {
        let converter = converter_for(Language::French);
        let line = converter.convert("ﬁn chat");
        let first = &line.tokens[0];
        assert_eq!(first.range, TextRange::new(0, 2));
        assert!(first
            .segments()
            .iter()
            .all(|segment| segment.range.end <= first.range.end));
        assert!(first
            .segments()
            .iter()
            .any(|segment| segment.range == TextRange::new(0, 1)));
        assert_eq!(line.tokens[1].range, TextRange::new(3, 7));
    }

    #[test]
    fn every_supported_language_generates_anchored_cues() {
        for (language, text) in [
            (Language::French, "Bonjour, le chat."),
            (Language::English, "Hello, the cat."),
            (Language::Spanish, "Hola, el gato."),
        ] {
            let converter = converter_for(language);
            let line = converter.convert(text);
            let result = generate_cues(
                &line,
                &GenerationOptions {
                    mapping: crate::phonetics::mapping::default_mapping(language),
                    placement: SignPlacement::Center,
                    include_optional: false,
                    start_frame: 0,
                    duration_frames: 120,
                    progress: None,
                    grapheme_count: text.chars().count(),
                    first_id: DetectionCueId(1),
                },
            );
            assert!(!result.cues.is_empty(), "{language:?} produced no cues");
            for cue in result.cues {
                let start = match cue.target {
                    TextAnchor::Grapheme { index } => index,
                    TextAnchor::GraphemeRange { start, .. } => start,
                    TextAnchor::BeforeText | TextAnchor::AfterText => continue,
                } as usize;
                assert!(start < text.chars().count(), "{language:?} anchor {start}");
            }
        }
    }
}
