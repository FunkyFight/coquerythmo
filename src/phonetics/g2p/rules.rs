//! Contextual grapheme→phoneme rule engine.
//!
//! Rules are applied left-to-right with longest match first. Each rule
//! matches a grapheme (1..=4 letters) optionally guarded by left/right
//! context patterns, and produces zero (silent), one or several phonemes.
//! Everything is `const`-friendly so the data lives in compile-time tables
//! validated by unit tests.

use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::types::{GraphemeSegment, PhonemeOccurrence, TextRange};

/// Right-context matcher on the following letters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctx {
    /// Anything.
    Any,
    /// Start/end of word.
    Start,
    End,
    /// One of the given ASCII lowercase letters.
    Letter(&'static str),
    /// Not one of these letters (but a letter).
    NotLetter(&'static str),
    Vowel,      // a e i o u y é è ê ë à â î ï ô û ù ü ÿ (FR vowels incl.)
    Consonant,  // non-vowel letter
    FrontVowel, // e i é è ê ë î ï y
    BackVowel,  // a o u à â ô û ù
}

impl Ctx {
    fn is_vowel_char(ch: char) -> bool {
        matches!(
            ch,
            'a' | 'e'
                | 'i'
                | 'o'
                | 'u'
                | 'y'
                | 'é'
                | 'è'
                | 'ê'
                | 'ë'
                | 'à'
                | 'â'
                | 'î'
                | 'ï'
                | 'ô'
                | 'û'
                | 'ù'
                | 'á'
                | 'í'
                | 'ó'
                | 'ú'
                | 'ü'
                | 'ñ'
                | 'æ'
                | 'œ'
        )
    }

    pub fn matches(&self, text: &[char], pos: usize, from_left: bool) -> bool {
        match self {
            Ctx::Any => true,
            Ctx::Start => pos == 0,
            Ctx::End => pos >= text.len(),
            Ctx::Letter(set) => {
                // Set of single candidate chars, immediately before (from_left)
                // or after the matched grapheme.
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                match text.get(idx) {
                    Some(&ch) => set.contains(ch),
                    None => false,
                }
            }
            Ctx::NotLetter(set) => {
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                match text.get(idx) {
                    Some(&ch) => ch.is_alphabetic() && !set.contains(ch),
                    None => false,
                }
            }
            Ctx::Vowel => {
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                text.get(idx).is_some_and(|&ch| Self::is_vowel_char(ch))
            }
            Ctx::Consonant => {
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                text.get(idx)
                    .is_some_and(|&ch| ch.is_alphabetic() && !Self::is_vowel_char(ch))
            }
            Ctx::FrontVowel => {
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                text.get(idx)
                    .is_some_and(|&ch| matches!(ch, 'e' | 'i' | 'é' | 'è' | 'ê' | 'ë' | 'î' | 'ï' | 'y'))
            }
            Ctx::BackVowel => {
                let idx = if from_left { pos.wrapping_sub(1) } else { pos };
                text.get(idx)
                    .is_some_and(|&ch| matches!(ch, 'a' | 'o' | 'u' | 'à' | 'â' | 'ô' | 'û' | 'ù'))
            }
        }
    }
}

/// Dialect filter for rules and dictionary entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialects {
    All,
    /// Only these dialects.
    Only(&'static [crate::phonetics::phoneme::Dialect]),
}

/// One grapheme rule.
#[derive(Clone, Copy, Debug)]
pub struct GraphemeRule {
    /// Letters matched, lowercase, ASCII or accented (e.g. "ch", "eau", "ill").
    pub grapheme: &'static str,
    pub left: Ctx,
    pub right: Ctx,
    /// Produced phonemes in order. Empty = silent letter(s).
    pub phonemes: &'static [Phoneme],
    /// Phonemes optional (liaison, e muet…).
    pub optional: bool,
    /// Dialect restriction.
    pub dialects: Dialects,
}

impl GraphemeRule {
    pub const fn rule(
        grapheme: &'static str,
        left: Ctx,
        right: Ctx,
        phonemes: &'static [Phoneme],
    ) -> Self {
        Self {
            grapheme,
            left,
            right,
            phonemes,
            optional: false,
            dialects: Dialects::All,
        }
    }

    pub const fn only(
        grapheme: &'static str,
        left: Ctx,
        right: Ctx,
        phonemes: &'static [Phoneme],
        dialects: &'static [crate::phonetics::phoneme::Dialect],
    ) -> Self {
        Self {
            grapheme,
            left,
            right,
            phonemes,
            optional: false,
            dialects: Dialects::Only(dialects),
        }
    }

    pub const fn optional(
        grapheme: &'static str,
        left: Ctx,
        right: Ctx,
        phonemes: &'static [Phoneme],
    ) -> Self {
        Self {
            grapheme,
            left,
            right,
            phonemes,
            optional: true,
            dialects: Dialects::All,
        }
    }

    pub fn applies_to(&self, dialect: crate::phonetics::phoneme::Dialect) -> bool {
        match self.dialects {
            Dialects::All => true,
            Dialects::Only(list) => list.contains(&dialect),
        }
    }
}

/// Apply `rules` to one lowercased word, longest-match-first. Deterministic.
/// Unknown letters produce a zero-phoneme non-silent segment flagged by the
/// caller for the "unknown word" diagnostic.
pub fn apply_rules(rules: &[GraphemeRule], word: &str) -> Vec<GraphemeSegment> {
    let chars: Vec<char> = word.chars().collect();
    let mut segments = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        // Longest grapheme first.
        let mut best: Option<&GraphemeRule> = None;
        for rule in rules {
            let g: Vec<char> = rule.grapheme.chars().collect();
            if g.is_empty() || i + g.len() > chars.len() {
                continue;
            }
            if chars[i..i + g.len()] != g[..] {
                continue;
            }
            if !rule.left.matches(&chars, i, true) {
                continue;
            }
            if !rule.right.matches(&chars, i + g.len(), false) {
                continue;
            }
            match best {
                None => best = Some(rule),
                Some(current) => {
                    let current_len = current.grapheme.chars().count();
                    if g.len() > current_len {
                        best = Some(rule);
                    }
                }
            }
        }
        if let Some(rule) = best {
            let glen = rule.grapheme.chars().count();
            let range = TextRange::new(i, i + glen);
            let phonemes: Vec<PhonemeOccurrence> = rule
                .phonemes
                .iter()
                .map(|&phoneme| PhonemeOccurrence {
                    phoneme,
                    optional: rule.optional,
                    variant: None,
                })
                .collect();
            segments.push(GraphemeSegment {
                grapheme: rule.grapheme.to_string(),
                range,
                silent: phonemes.is_empty(),
                phonemes,
            });
            i += glen;
        } else {
            // No rule matched: keep letter, no phoneme, flagged not-silent so
            // callers can detect unhandled letters.
            segments.push(GraphemeSegment {
                grapheme: chars[i].to_string(),
                range: TextRange::new(i, i + 1),
                phonemes: Vec::new(),
                silent: false,
            });
            i += 1;
        }
    }
    segments
}

// ── Dictionaries ────────────────────────────────────────────────────────────

/// A dictionary entry is a compact segment spec: (grapheme letters, phonemes).
/// Ranges are computed from the letters by `expand_word` so dictionary tables
/// stay terse. All grapheme strings must concatenate to the key exactly
/// (validated in tests).
pub type SegmentSpec = (&'static str, &'static [Phoneme]);
pub type WordEntry = &'static [SegmentSpec];
/// Dictionary table row: (word, dialect filter, segments).
pub type DictRow = (&'static str, Dialects, WordEntry);
/// A dictionary table is iterated once at startup to build per-dialect maps.
pub type DictTable = &'static [DictRow];

/// Build segments from a compact dictionary entry, computing ranges by
/// consuming the word's letters. Returns `Vec<GraphemeSegment>` whose `range`
/// fields are in *word char indices* (converted later to grapheme indices).
pub fn expand_word(word: &str, entry: WordEntry) -> Vec<GraphemeSegment> {
    let chars: Vec<char> = word.chars().collect();
    let mut cursor = 0usize;
    let mut segments = Vec::with_capacity(entry.len());
    for &(letters, phonemes) in entry {
        let len = letters.chars().count();
        let range = TextRange::new(cursor, cursor + len);
        cursor += len;
        segments.push(GraphemeSegment {
            grapheme: letters.to_string(),
            range,
            phonemes: phonemes
                .iter()
                .map(|&phoneme| PhonemeOccurrence::new(phoneme))
                .collect(),
            silent: phonemes.is_empty(),
        });
    }
    debug_assert_eq!(
        cursor,
        chars.len(),
        "dictionary entry letters must cover the whole key {word}"
    );
    segments
}
