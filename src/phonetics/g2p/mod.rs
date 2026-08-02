//! Grapheme-to-phoneme conversion: trait, tokenization, hybrid
//! dictionary+rules resolver and per-word cache.

pub mod numbers;
pub mod rules;
pub mod tokenizer;

use crate::phonetics::normalize::normalize_line;
use crate::phonetics::phoneme::{Dialect, Language};
use crate::phonetics::types::*;
use std::collections::HashMap;

pub use tokenizer::tokenize;

/// Per-language converter contract.
pub trait GraphemeToPhonemeConverter: Send + Sync {
    fn language(&self) -> Language;
    /// Convert the *displayed* text. Ranges in the result index graphemes of
    /// `text` exactly.
    fn convert(&self, text: &str) -> PhoneticLine;
    fn dialect(&self) -> Dialect {
        self.language().default_dialect()
    }
    /// Convert a single lowercase normalized word. Used by the preview to
    /// re-resolve one word after a user correction.
    fn convert_word(&self, word: &str) -> Vec<PronunciationCandidate>;
}

/// Full-word pronunciation dictionary. Multiple entries per key = variants.
pub struct WordDict {
    pub entries: HashMap<&'static str, Vec<rules::WordEntry>>,
}

impl WordDict {
    pub fn from_tables(table: rules::DictTable, dialect: Dialect) -> Self {
        let mut entries: HashMap<&'static str, Vec<rules::WordEntry>> = HashMap::new();
        for (key, dialects, entry) in table {
            let applies = match dialects {
                rules::Dialects::All => true,
                rules::Dialects::Only(list) => list.contains(&dialect),
            };
            if applies {
                entries.entry(key).or_default().push(entry);
            }
        }
        Self { entries }
    }

    pub fn lookup(&self, word: &str) -> Option<&Vec<rules::WordEntry>> {
        self.entries.get(word)
    }

    pub fn entry_count(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }
}

/// Shared engine: rule table, dictionaries and cache for one language.
/// Concrete converters (`FrenchG2P`…) only provide data.
pub struct G2PEngine {
    pub language: Language,
    pub dialect: Dialect,
    pub dictionary: WordDict,
    pub exceptions: WordDict,
    /// Grapheme rules filtered for `dialect`.
    pub rules: Vec<rules::GraphemeRule>,
    /// Lowercase acronyms pronounced as words (NASA→AcronymWord).
    pub pronounceable_acronyms: &'static [&'static str],
    /// Language-specific post-pass on rule-produced segments (English magic-e,
    /// GB non-rhoticity…).
    pub post: fn(Dialect, Vec<GraphemeSegment>) -> Vec<GraphemeSegment>,
    cache: std::sync::Mutex<HashMap<String, Vec<PronunciationCandidate>>>,
}

impl G2PEngine {
    pub fn new(
        language: Language,
        dialect: Dialect,
        dictionary: rules::DictTable,
        exceptions: rules::DictTable,
        rules: Vec<rules::GraphemeRule>,
        pronounceable_acronyms: &'static [&'static str],
        post: fn(Dialect, Vec<GraphemeSegment>) -> Vec<GraphemeSegment>,
    ) -> Self {
        Self {
            language,
            dialect,
            dictionary: WordDict::from_tables(dictionary, dialect),
            exceptions: WordDict::from_tables(exceptions, dialect),
            rules,
            pronounceable_acronyms,
            post,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Resolve a token into pronunciation candidates, honouring the hybrid
    /// order: dictionary → exceptions → rules → fallback. `word` must be
    /// lowercase and language-normalized.
    pub fn resolve_word(&self, word: &str) -> Vec<PronunciationCandidate> {
        if let Some(hit) = self.cache.lock().unwrap().get(word) {
            return hit.clone();
        }

        let mut candidates: Vec<PronunciationCandidate> = Vec::new();

        if let Some(entries) = self.dictionary.lookup(word) {
            for entry in entries {
                candidates.push(PronunciationCandidate {
                    source: PronunciationSource::Dictionary,
                    confidence: 1.0,
                    segments: rules::expand_word(word, entry),
                });
            }
        }
        if let Some(entries) = self.exceptions.lookup(word) {
            for entry in entries {
                candidates.push(PronunciationCandidate {
                    source: PronunciationSource::Dictionary,
                    confidence: 0.95,
                    segments: rules::expand_word(word, entry),
                });
            }
        }
        if candidates.is_empty() {
            let segments = (self.post)(self.dialect, rules::apply_rules(&self.rules, word));
            candidates.push(PronunciationCandidate {
                source: PronunciationSource::Rule,
                confidence: 0.85,
                segments,
            });
        }
        self.cache
            .lock()
            .unwrap()
            .insert(word.to_string(), candidates.clone());
        candidates
    }

    /// Cache key components: language + dialect + engine version.
    pub fn cache_key(&self) -> u64 {
        let base = format!(
            "{}:{}:{}",
            self.language.code(),
            self.dialect.code(),
            crate::phonetics::ENGINE_VERSION
        );
        crate::phonetics::text_fingerprint(&base)
    }
}

/// Full line conversion shared by all converters: normalize, tokenize,
/// resolve each token.
pub fn convert_line(engine: &G2PEngine, text: &str) -> PhoneticLine {
    let normalized = normalize_line(text);
    let tokens = tokenize(&normalized, engine.language);
    let mut out_tokens = Vec::with_capacity(tokens.len());
    let graphemes: Vec<&str> =
        unicode_segmentation::UnicodeSegmentation::graphemes(text, true).collect();
    let total_graphemes = graphemes.len();

    for token in tokens {
        let start = token.start_grapheme.min(total_graphemes);
        let end = token.end_grapheme.min(total_graphemes).max(start);
        let range = TextRange::new(start, end);
        let surface: String = graphemes
            .get(start..end)
            .map(|slice| slice.concat())
            .unwrap_or_default();

        let mut out = PhoneticToken {
            text: surface.clone(),
            range,
            kind: token.kind,
            candidates: Vec::new(),
            selected_candidate: 0,
            unknown: false,
        };

        match token.kind {
            TokenKind::Punctuation => {}
            TokenKind::Symbol => {
                if let Some(reading) = numbers::symbol_reading(engine.language, &token.text) {
                    for word in reading.split_whitespace() {
                        out.candidates
                            .extend(remap_multi(engine, word, &token, range));
                    }
                }
            }
            TokenKind::Number => {
                let words = numbers::spell_number(engine.language, &token.text);
                let segments = number_segments(engine, &words, range);
                out.candidates = vec![PronunciationCandidate {
                    source: PronunciationSource::Dictionary,
                    confidence: 1.0,
                    segments,
                }];
            }
            TokenKind::Acronym | TokenKind::AcronymWord | TokenKind::Word
            | TokenKind::ElidedPrefix => {
                // Re-detect all-caps acronyms from the original casing (the
                // tokenizer sees lowercase).
                let acronym = tokenizer::acronym_kind(
                    text,
                    token.start_grapheme,
                    token.end_grapheme,
                    engine.pronounceable_acronyms,
                    if token.kind == TokenKind::Word { 1 } else { 0 },
                );
                match acronym {
                    Some(TokenKind::Acronym) => {
                        out.kind = TokenKind::Acronym;
                        let count = token.text.chars().count();
                        let segments = acronym_segments(engine, &token.text, range, count);
                        out.candidates = vec![PronunciationCandidate {
                            source: PronunciationSource::Dictionary,
                            confidence: 1.0,
                            segments,
                        }];
                        out_tokens.push(out);
                        continue;
                    }
                    Some(TokenKind::AcronymWord) => {
                        out.kind = TokenKind::AcronymWord;
                    }
                    _ => {}
                }
                let resolved = engine.resolve_word(&token.text);
                // A word resolved purely by rules that leaves letters with no
                // phoneme and not marked silent = unhandled letters → unknown.
                out.unknown = resolved.first().is_some_and(|candidate| {
                    candidate.source == PronunciationSource::Rule
                        && candidate
                            .segments
                            .iter()
                            .any(|segment| segment.phonemes.is_empty() && !segment.silent)
                });
                out.candidates = remap_ranges(resolved, &token, range);
            }
        }
        out_tokens.push(out);
    }

    PhoneticLine {
        original_text: text.to_string(),
        language: engine.language,
        dialect: engine.dialect,
        tokens: out_tokens,
    }
}

/// Resolve one extra word (symbol reading) and attach the token range.
fn remap_multi(
    engine: &G2PEngine,
    word: &str,
    token: &tokenizer::RawToken,
    range: TextRange,
) -> Vec<PronunciationCandidate> {
    let mut candidates = engine.resolve_word(word);
    for candidate in &mut candidates {
        for segment in &mut candidate.segments {
            segment.range = range;
        }
    }
    let _ = token;
    candidates
}

/// Rule/dictionary results use token-relative char indices; this remaps them
/// to original grapheme indices. For pure words without ligature expansion,
/// char index == grapheme index.
fn remap_ranges(
    mut candidates: Vec<PronunciationCandidate>,
    token: &tokenizer::RawToken,
    original_range: TextRange,
) -> Vec<PronunciationCandidate> {
    let token_len = token.text.chars().count().max(1);
    let span = original_range.len().max(1);
    for candidate in &mut candidates {
        for segment in &mut candidate.segments {
            let rel_start = segment.range.start.min(token_len);
            let rel_end = segment.range.end.min(token_len).max(rel_start);
            let start = original_range.start + rel_start * span / token_len;
            let mut end = original_range.start + rel_end * span / token_len;
            if rel_end > rel_start && end <= start {
                end = start + 1;
            }
            segment.range = TextRange::new(start, end);
        }
    }
    candidates
}

/// Numbers: resolve each spoken word and give every produced segment the
/// full digit span as its range (the digits *are* the letters).
fn number_segments(
    engine: &G2PEngine,
    spoken_words: &[String],
    range: TextRange,
) -> Vec<GraphemeSegment> {
    let mut segments = Vec::new();
    for word in spoken_words {
        for part in word.split('-') {
            if let Some(candidate) = engine.resolve_word(part).into_iter().next() {
                for mut segment in candidate.segments {
                    segment.range = range;
                    segments.push(segment);
                }
            }
        }
    }
    segments
}

/// Acronym: one segment per letter. Each letter's dictionary reading
/// (e.g. "bé", "cee") is itself resolved.
fn acronym_segments(
    engine: &G2PEngine,
    letters: &str,
    range: TextRange,
    count: usize,
) -> Vec<GraphemeSegment> {
    let mut segments = Vec::new();
    let count = count.max(1);
    for (i, letter) in letters.chars().enumerate() {
        let letter_name = numbers::letter_name(engine.language, letter);
        let sub_range = TextRange::new(
            range.start + (range.len() * i) / count,
            range.start + (range.len() * (i + 1)) / count,
        );
        if let Some(candidate) = engine.resolve_word(&letter_name).into_iter().next() {
            for mut segment in candidate.segments {
                segment.range = sub_range;
                segments.push(segment);
            }
        }
    }
    segments
}
