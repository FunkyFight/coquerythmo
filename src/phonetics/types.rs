//! Structured phonetic representations. Every unit keeps an exact reference
//! (grapheme indices in the *original* displayed text) so detection signs can
//! be placed on the letters that produced them.

use crate::phonetics::phoneme::{Dialect, Language, Phoneme};
use serde::{Deserialize, Serialize};

/// Half-open grapheme-index range into the original line text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: usize,
    /// End-exclusive.
    pub end: usize,
}

impl TextRange {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// What produced a pronunciation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PronunciationSource {
    /// Correction stored on this exact line occurrence.
    LocalOverride,
    /// Project-level user correction.
    ProjectOverride,
    /// Global user dictionary.
    UserDictionary,
    /// Embedded pronunciation dictionary / exception list.
    #[default]
    Dictionary,
    /// Automatic grapheme-to-phoneme rules.
    Rule,
    /// Last-resort letter-by-letter fallback (unknown word).
    Fallback,
}

impl PronunciationSource {
    /// Priority used by the hybrid resolver. Higher wins.
    pub const fn priority(self) -> u8 {
        match self {
            Self::LocalOverride => 6,
            Self::ProjectOverride => 5,
            Self::UserDictionary => 4,
            Self::Dictionary => 3,
            Self::Rule => 2,
            Self::Fallback => 1,
        }
    }
}

/// One phoneme instance with flags.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhonemeOccurrence {
    pub phoneme: Phoneme,
    /// Pronounceable in careful speech but commonly dropped (French e muet,
    /// liaison consonants, optional linking r).
    #[serde(default)]
    pub optional: bool,
    /// Free-form variant note (e.g. "liaison", "regional").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl PhonemeOccurrence {
    pub fn new(phoneme: Phoneme) -> Self {
        Self {
            phoneme,
            optional: false,
            variant: None,
        }
    }

    pub fn optional(phoneme: Phoneme) -> Self {
        Self {
            phoneme,
            optional: true,
            variant: None,
        }
    }
}

/// A written unit (grapheme) and the phonemes it produces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphemeSegment {
    /// Letters as written (from the original text).
    pub grapheme: String,
    /// Range in the original text, in grapheme indices.
    pub range: TextRange,
    pub phonemes: Vec<PhonemeOccurrence>,
    /// True when the letters are written but silent (French final "t"…).
    #[serde(default)]
    pub silent: bool,
}

impl GraphemeSegment {
    pub fn letter(grapheme: impl Into<String>, range: TextRange, phoneme: Phoneme) -> Self {
        Self {
            grapheme: grapheme.into(),
            range,
            phonemes: vec![PhonemeOccurrence::new(phoneme)],
            silent: false,
        }
    }

    pub fn silent_letter(grapheme: impl Into<String>, range: TextRange) -> Self {
        Self {
            grapheme: grapheme.into(),
            range,
            phonemes: Vec::new(),
            silent: true,
        }
    }
}

/// Onset/nucleus split is not needed for placement, but keeping the segment
/// list ordered guarantees deterministic sign ordering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PronunciationCandidate {
    pub source: PronunciationSource,
    /// 0.0..=1.0. Dictionary entries are 1.0, ambiguous rules below.
    pub confidence: f32,
    pub segments: Vec<GraphemeSegment>,
}

impl PronunciationCandidate {
    pub fn phonemes(&self) -> impl Iterator<Item = &PhonemeOccurrence> {
        self.segments.iter().flat_map(|segment| segment.phonemes.iter())
    }

    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|segment| segment.phonemes.is_empty())
    }
}

/// Kind of lexical token after tokenization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    #[default]
    Word,
    Number,
    /// All-caps short token pronounced letter-by-letter (SNCF, BBC).
    Acronym,
    /// All-caps token pronounced as a word (NASA, OTAN).
    AcronymWord,
    /// Symbol read aloud (%, €, &, @, #…).
    Symbol,
    /// Punctuation that influences segmentation/prosody.
    Punctuation,
    /// Elided contraction head (l', d', j'…).
    ElidedPrefix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhoneticToken {
    /// Surface form from the original text.
    pub text: String,
    pub range: TextRange,
    pub kind: TokenKind,
    /// Possible pronunciations, best first.
    #[serde(default)]
    pub candidates: Vec<PronunciationCandidate>,
    /// Index into `candidates` chosen by the user (0 = default).
    #[serde(default)]
    pub selected_candidate: usize,
    /// True when the word could not be resolved with confidence.
    #[serde(default)]
    pub unknown: bool,
}

impl PhoneticToken {
    pub fn selected(&self) -> Option<&PronunciationCandidate> {
        self.candidates
            .get(self.selected_candidate)
            .or_else(|| self.candidates.first())
    }

    /// All segments of the selected pronunciation.
    pub fn segments(&self) -> &[GraphemeSegment] {
        self.selected()
            .map(|candidate| candidate.segments.as_slice())
            .unwrap_or(&[])
    }
}

/// Phonetic analysis of one full line of dialogue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhoneticLine {
    pub original_text: String,
    pub language: Language,
    pub dialect: Dialect,
    pub tokens: Vec<PhoneticToken>,
}

impl PhoneticLine {
    /// Every segment of every token, in reading order.
    pub fn segments(&self) -> impl Iterator<Item = &GraphemeSegment> {
        self.tokens.iter().flat_map(PhoneticToken::segments)
    }

    /// Tokens that failed to resolve.
    pub fn unknown_tokens(&self) -> impl Iterator<Item = &PhoneticToken> {
        self.tokens.iter().filter(|token| token.unknown)
    }

    /// Tokens exposing more than one pronunciation.
    pub fn ambiguous_tokens(&self) -> impl Iterator<Item = &PhoneticToken> {
        self.tokens
            .iter()
            .filter(|token| token.candidates.len() > 1)
    }
}

/// Metadata recorded next to the detection signs produced by one generation.
/// Stored inside `LineDetectionData` so it persists with the project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSignsInfo {
    /// Unique id of the generation run (monotonic per line).
    pub generation_id: u32,
    /// [`ENGINE_VERSION`](crate::phonetics::ENGINE_VERSION) used.
    pub engine_version: u32,
    pub language: Language,
    pub dialect: Dialect,
    /// Fingerprint of the source text at generation time
    /// ([`text_fingerprint`](crate::phonetics::text_fingerprint)).
    pub text_fingerprint: u64,
    /// Fingerprint of the mapping profile used.
    pub mapping_fingerprint: u64,
    /// Ids of cues created by this run, in insertion order.
    pub cue_ids: Vec<u64>,
}

impl GeneratedSignsInfo {
    /// True when the line text changed since signs were generated.
    pub fn is_stale_for(&self, current_text: &str) -> bool {
        self.text_fingerprint != crate::phonetics::text_fingerprint(current_text)
    }
}
