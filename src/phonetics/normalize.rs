//! Text normalization with an inverse map back to the original text.
//!
//! The displayed text is never modified: normalization produces an analysis
//! string plus, for every normalized character, the grapheme index of the
//! original character it came from.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// One normalized character and where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedChar {
    pub ch: char,
    /// Grapheme index in the original text.
    pub original_grapheme: usize,
    /// True when this character was inserted by normalization (e.g. the second
    /// half of an expanded ligature) and has no dedicated original letter.
    pub synthetic: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NormalizedLine {
    /// Original, untouched display text.
    pub original: String,
    /// Normalized analysis text (lowercased, apostrophes unified…).
    pub normalized: Vec<NormalizedChar>,
}

impl NormalizedLine {
    pub fn normalized_string(&self) -> String {
        self.normalized.iter().map(|c| c.ch).collect()
    }

    /// Map a normalized-char offset to the original grapheme index.
    pub fn original_grapheme(&self, normalized_index: usize) -> usize {
        self.normalized
            .get(normalized_index)
            .map(|c| c.original_grapheme)
            .unwrap_or(0)
    }

    /// Original grapheme count.
    pub fn grapheme_count(&self) -> usize {
        UnicodeSegmentation::graphemes(self.original.as_str(), true).count()
    }
}

/// Expand common ligatures and typographic variants into analysis characters.
/// Returns either one replacement char (same original index) or a small list
/// for expansions (Œ → oe, second char marked synthetic).
fn expanded_equiv(ch: char) -> &'static [char] {
    match ch {
        'œ' => &['œ'],
        'Œ' => &['œ'],
        'æ' => &['æ'],
        'Æ' => &['æ'],
        '’' | 'ʼ' | '‛' | '`' | '´' => &['\''],
        '‘' => &['\''],
        '“' | '”' | '«' | '»' | '‹' | '›' => &['"'],
        '‐' | '‑' | '‒' | '–' | '—' | '−' => &['-'],
        '…' => &['.', '.', '.'],
        'ﬁ' => &['f', 'i'],
        'ﬂ' => &['f', 'l'],
        'ﬀ' => &['f', 'f'],
        'ﬃ' => &['f', 'f', 'i'],
        'ﬄ' => &['f', 'f', 'l'],
        // Full-width ASCII.
        'Ａ'..='Ｚ' | 'ａ'..='ｚ' => &[],
        _ => &[],
    }
}

fn fold_fullwidth(ch: char) -> char {
    match ch {
        'Ａ'..='Ｚ' => char::from_u32((ch as u32) - ('Ａ' as u32) + ('a' as u32)).unwrap_or(ch),
        'ａ'..='ｚ' => char::from_u32((ch as u32) - ('ａ' as u32) + ('a' as u32)).unwrap_or(ch),
        _ => ch,
    }
}

/// Normalize a line of dialogue for phonetic analysis.
///
/// Operations: Unicode-NFKC-like handling of the characters above (the
/// project deliberately avoids pulling in a full ICU dependency), apostrophe
/// unification, dash unification, lowercasing, whitespace collapse. Every
/// normalized char remembers its source grapheme.
pub fn normalize_line(text: &str) -> NormalizedLine {
    let graphemes: Vec<&str> = UnicodeSegmentation::graphemes(text, true).collect();
    let mut normalized = Vec::with_capacity(graphemes.len());
    let mut grapheme_cursor = 0usize;
    let mut whitespace_pending: Option<usize> = None;

    for (gi, cluster) in graphemes.iter().enumerate() {
        grapheme_cursor = gi;
        let mut chars = cluster.chars();
        let Some(first) = chars.next() else { continue };
        // Multi-char clusters (flags, keycaps…): keep first char mapping.
        let lowered = fold_fullwidth(first).to_lowercase().next().unwrap_or(' ');
        let equiv = expanded_equiv(lowered);
        let write_ws = matches!(
            lowered,
            ' ' | '\t' | '\r' | '\n' | '\u{00A0}' | '\u{202F}' | '\u{2009}' | '\u{3000}'
        );
        if write_ws {
            // Collapse runs of whitespace to a single space, remembered as
            // coming from the first whitespace grapheme.
            if whitespace_pending.is_none() && !normalized.is_empty() {
                whitespace_pending = Some(gi);
            }
            continue;
        }
        if let Some(space_gi) = whitespace_pending.take() {
            normalized.push(NormalizedChar {
                ch: ' ',
                original_grapheme: space_gi,
                synthetic: false,
            });
        }
        if equiv.is_empty() {
            normalized.push(NormalizedChar {
                ch: lowered,
                original_grapheme: gi,
                synthetic: false,
            });
        } else {
            for (offset, &ch) in equiv.iter().enumerate() {
                normalized.push(NormalizedChar {
                    ch,
                    original_grapheme: gi,
                    synthetic: offset > 0,
                });
            }
        }
        // Append any remaining cluster chars (diacritics, ZWJ parts…).
        for extra in chars {
            normalized.push(NormalizedChar {
                ch: extra.to_lowercase().next().unwrap_or(extra),
                original_grapheme: gi,
                synthetic: false,
            });
        }
    }
    let _ = grapheme_cursor;

    // Trim a leading space that would come from leading whitespace.
    if matches!(normalized.first(), Some(c) if c.ch == ' ') {
        normalized.remove(0);
    }

    NormalizedLine {
        original: text.to_string(),
        normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(text: &str) -> (String, Vec<usize>) {
        let line = normalize_line(text);
        (
            line.normalized_string(),
            line.normalized
                .iter()
                .map(|c| c.original_grapheme)
                .collect(),
        )
    }

    #[test]
    fn lowercases_and_preserves_indices() {
        let (normalized, map) = norm("Chat");
        assert_eq!(normalized, "chat");
        assert_eq!(map, vec![0, 1, 2, 3]);
    }

    #[test]
    fn unifies_apostrophes() {
        for apostrophe in ["l’ami", "l'ami", "lʼami"] {
            let (normalized, _) = norm(apostrophe);
            assert_eq!(normalized, "l'ami");
        }
    }

    #[test]
    fn collapses_multiple_spaces_and_keeps_origin() {
        let (normalized, map) = norm("il  est   là");
        assert_eq!(normalized, "il est là");
        // Space after "il" must point at the first whitespace grapheme of the run.
        assert_eq!(map[2], 2);
        assert_eq!(normalized.chars().count(), map.len());
    }

    #[test]
    fn expands_ligature_oe_and_fi() {
        let (normalized, map) = norm("ﬁn");
        assert_eq!(normalized, "fin");
        assert_eq!(map, vec![0, 0, 1]);
    }

    #[test]
    fn unicode_dashes_and_ellipsis() {
        let (normalized, _) = norm("oui—non… peut-être");
        assert_eq!(normalized, "oui-non... peut-être");
    }

    #[test]
    fn accented_and_fullwidth_characters() {
        let (normalized, _) = norm("ÉＴÉ");
        assert_eq!(normalized, "été");
    }

    #[test]
    fn nbsp_and_narrow_nbsp_become_spaces() {
        let (normalized, _) = norm("c'est\u{00A0}sûr\u{202F}oui");
        assert_eq!(normalized, "c'est sûr oui");
    }
}
