//! Tokenization of a normalized line into words, numbers, acronyms, symbols
//! and punctuation, preserving exact original grapheme ranges for each token.

use crate::phonetics::normalize::NormalizedLine;
use crate::phonetics::phoneme::Language;
use crate::phonetics::types::TokenKind;

#[derive(Clone, Debug)]
pub struct RawToken {
    /// Lowercased normalized surface (may include accents).
    pub text: String,
    /// Half-open offsets in `NormalizedLine::normalized`.
    pub normalized_start: usize,
    pub normalized_end: usize,
    pub start_grapheme: usize,
    /// End-exclusive.
    pub end_grapheme: usize,
    pub kind: TokenKind,
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '-')
}

fn is_symbol(ch: char) -> bool {
    matches!(
        ch,
        '%' | '€' | '$' | '£' | '&' | '@' | '#' | '+' | '=' | '°' | '<' | '>' | '/' | '*' | '^'
    )
}

/// French elisions keep the elided head as its own token (`l'`, `d'`, `j'`,
/// `qu'`, `n'`, `s'`, `t'`, `c'`, `m'`, `jusqu'`, `lorsqu'`, `puisqu'`).
const FR_ELIDABLE: &[&str] = &[
    "jusqu", "lorsqu", "puisqu", "quoiqu", "qu", "l", "d", "j", "n", "s", "t", "c", "m",
];

/// Spanish contractions as tokens ("al", "del"), English clitics are kept
/// attached to the word (`don't` …). We only split where pronunciation is
/// compositional.
pub fn tokenize(line: &NormalizedLine, language: Language) -> Vec<RawToken> {
    let chars = &line.normalized;
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i].ch;
        if ch == ' ' {
            i += 1;
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            let mut has_dot_or_comma = false;
            while i < chars.len() {
                let c = chars[i].ch;
                if c.is_ascii_digit() {
                    i += 1;
                    continue;
                }
                // Space between digits = thousands separator ("1 200").
                if c == ' '
                    && i > start
                    && chars.get(i + 1).is_some_and(|n| n.ch.is_ascii_digit())
                    && chars
                        .get(i.wrapping_sub(1))
                        .is_some_and(|p| p.ch.is_ascii_digit())
                {
                    i += 1;
                    continue;
                }
                if matches!(c, '.' | ',') {
                    // Decimal only with digits both sides.
                    let next_is_digit = chars.get(i + 1).is_some_and(|n| n.ch.is_ascii_digit());
                    if i > start && next_is_digit && !has_dot_or_comma {
                        has_dot_or_comma = true;
                        i += 1;
                        continue;
                    }
                    break;
                }
                if matches!(c, '\'') {
                    i += 1;
                    continue;
                }
                break;
            }
            let text: String = chars[start..i].iter().map(|c| c.ch).collect();
            let mut text = text
                .replace('\u{00A0}', "")
                .replace('\'', "")
                .replace(' ', "");
            if has_dot_or_comma && language == Language::French {
                text = text.replace(',', ".");
            }
            tokens.push(RawToken {
                text,
                normalized_start: start,
                normalized_end: i,
                start_grapheme: chars[start].original_grapheme,
                end_grapheme: chars[i - 1].original_grapheme + 1,
                kind: TokenKind::Number,
            });
            continue;
        }

        if is_word_char(ch) {
            let start = i;
            let mut has_apostrophe = false;
            while i < chars.len() && (is_word_char(chars[i].ch) || chars[i].ch == '\'') {
                if chars[i].ch == '\'' {
                    has_apostrophe = true;
                    // French: cut "l'" type prefixes into their own token when
                    // followed by a letter and the head is elidable.
                    if language == Language::French {
                        let head: String = chars[start..i].iter().map(|c| c.ch).collect();
                        let next_is_letter = chars.get(i + 1).is_some_and(|c| c.ch.is_alphabetic());
                        if next_is_letter && FR_ELIDABLE.contains(&head.as_str()) {
                            // Emit prefix token and continue with the rest.
                            tokens.push(RawToken {
                                text: head,
                                normalized_start: start,
                                normalized_end: i,
                                start_grapheme: chars[start].original_grapheme,
                                end_grapheme: chars[i].original_grapheme,
                                kind: TokenKind::ElidedPrefix,
                            });
                            i += 1; // skip apostrophe
                            let rest_start = i;
                            while i < chars.len()
                                && (is_word_char(chars[i].ch) || chars[i].ch == '\'')
                            {
                                i += 1;
                            }
                            let rest_text: String =
                                chars[rest_start..i].iter().map(|c| c.ch).collect();
                            tokens.push(RawToken {
                                text: rest_text.clone(),
                                normalized_start: rest_start,
                                normalized_end: i,
                                start_grapheme: chars[rest_start].original_grapheme,
                                end_grapheme: chars[i - 1].original_grapheme + 1,
                                kind: classify_word(&rest_text),
                            });
                            break;
                        }
                    }
                }
                i += 1;
            }
            if i > start
                && tokens
                    .last()
                    .is_none_or(|t| t.end_grapheme != chars[i - 1].original_grapheme + 1)
            {
                let text: String = chars[start..i].iter().map(|c| c.ch).collect();
                if !text.is_empty() {
                    // English/Spanish contractions: keep whole, apostrophes inside.
                    let _ = has_apostrophe;
                    let kind = classify_word(&text);
                    tokens.push(RawToken {
                        text,
                        normalized_start: start,
                        normalized_end: i,
                        start_grapheme: chars[start].original_grapheme,
                        end_grapheme: chars[i - 1].original_grapheme + 1,
                        kind,
                    });
                }
            }
            continue;
        }

        if is_symbol(ch) {
            tokens.push(RawToken {
                text: ch.to_string(),
                normalized_start: i,
                normalized_end: i + 1,
                start_grapheme: chars[i].original_grapheme,
                end_grapheme: chars[i].original_grapheme + 1,
                kind: TokenKind::Symbol,
            });
            i += 1;
            continue;
        }

        if ch.is_ascii_punctuation() || matches!(ch, '"' | '…' | '¡' | '¿') {
            tokens.push(RawToken {
                text: ch.to_string(),
                normalized_start: i,
                normalized_end: i + 1,
                start_grapheme: chars[i].original_grapheme,
                end_grapheme: chars[i].original_grapheme + 1,
                kind: TokenKind::Punctuation,
            });
            i += 1;
            continue;
        }
        i += 1;
    }
    tokens
}

/// All-caps 2..=6 letters → acronym. Check against the pronounceable list
/// done later at resolution; here we tag all-caps words, defaulting to
/// letter-by-letter if short.
fn classify_word(text: &str) -> TokenKind {
    let letters: Vec<char> = text.chars().collect();
    if letters.len() >= 2 && letters.len() <= 7 && letters.iter().all(|c| c.is_alphabetic()) {
        // Tokenizer works on lowercase; acronyms were detected before
        // lowercasing. Rethink: normalization lowercases everything. So
        // acronym detection must happen on the *original* text. We therefore
        // cannot decide here. Mark candidate: resolver decides.
    }
    let _ = letters;
    TokenKind::Word
}

/// Detect all-caps acronym candidates from the original text. Called per
/// word by converters wanting casing info. Returns Some(kind) if the original
/// surface of [start, end) is an acronym.
pub fn acronym_kind(
    original: &str,
    start_grapheme: usize,
    end_grapheme: usize,
    pronounceable: &[&str],
    letter_names_hint_uppercase_len: usize,
) -> Option<TokenKind> {
    use unicode_segmentation::UnicodeSegmentation;
    let graphemes: Vec<&str> = UnicodeSegmentation::graphemes(original, true).collect();
    if end_grapheme > graphemes.len() || start_grapheme >= end_grapheme {
        return None;
    }
    let surface: String = graphemes[start_grapheme..end_grapheme].concat();
    let letters: Vec<char> = surface.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 2 || !letters.iter().all(|c| c.is_uppercase()) {
        return None;
    }
    let lower = surface.to_lowercase();
    if pronounceable.iter().any(|word| *word == lower) {
        Some(TokenKind::AcronymWord)
    } else if letter_names_hint_uppercase_len > 0 {
        Some(TokenKind::Acronym)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonetics::normalize::normalize_line;

    fn tokens(text: &str, language: Language) -> Vec<(String, TokenKind, (usize, usize))> {
        let line = normalize_line(text);
        tokenize(&line, language)
            .into_iter()
            .map(|t| (t.text, t.kind, (t.start_grapheme, t.end_grapheme)))
            .collect()
    }

    #[test]
    fn french_elision_splits_prefix_and_word() {
        let result = tokens("l'ami d'Éric", Language::French);
        assert!(result
            .iter()
            .any(|(text, kind, _)| text == "l" && *kind == TokenKind::ElidedPrefix));
        assert!(result.iter().any(|(text, _, _)| text == "ami"));
        assert!(result
            .iter()
            .any(|(text, kind, _)| text == "d" && *kind == TokenKind::ElidedPrefix));
        assert!(result.iter().any(|(text, _, _)| text == "éric"));
    }

    #[test]
    fn numbers_with_spaces_groups_and_decimals() {
        let result = tokens("il y a 1 200,5 euros", Language::French);
        let number = result
            .iter()
            .find(|(_, kind, _)| *kind == TokenKind::Number)
            .expect("number token");
        assert_eq!(number.0, "1200.5");
        assert!(result.iter().any(|(text, _, _)| text == "euros"));
    }

    #[test]
    fn punctuation_becomes_own_token() {
        let result = tokens("oui, non !", Language::French);
        assert!(result
            .iter()
            .any(|(text, kind, _)| text == "," && *kind == TokenKind::Punctuation));
        assert!(result
            .iter()
            .any(|(text, kind, _)| text == "!" && *kind == TokenKind::Punctuation));
        assert_eq!(
            result
                .iter()
                .filter(|(_, kind, _)| *kind == TokenKind::Word)
                .count(),
            2
        );
    }

    #[test]
    fn english_contraction_stays_whole() {
        let result = tokens("don't stop", Language::English);
        assert!(result.iter().any(|(text, _, _)| text == "don't"));
    }

    #[test]
    fn original_ranges_point_into_display_text() {
        let original = "C'est ÇA!";
        let result = tokens(original, Language::French);
        for (_, _, (start, end)) in &result {
            assert!(end > start, "empty range");
        }
        // "ça" must cover grapheme indices 6..8 in "C'est ÇA!"
        let ca = result
            .iter()
            .find(|(text, _, _)| text == "ça")
            .expect("ça token");
        assert_eq!(ca.2, (6, 8));
    }
}
