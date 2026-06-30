use hypher::{hyphenate, Lang};

/// Return syllable break positions as character indices within the full text.
/// Handles French contractions (l', d', j', etc.) as single syllable units.
/// Breaks at intra-word syllable boundaries and at word starts.
/// Spaces are absorbed into the preceding segment (attached to the word before).
pub fn syllable_breaks(text: &str, lang: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }

    let hypher_lang = match lang {
        "en-us" | "en" => Lang::English,
        _ => Lang::French,
    };
    let is_french = !matches!(lang, "en-us" | "en");

    let chars: Vec<char> = text.chars().collect();
    let total_chars = chars.len();

    // Step 1: Find word spans, treating apostrophes as part of words.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < total_chars {
        if is_word_char(chars[i]) {
            let start = i;
            while i < total_chars && is_word_char(chars[i]) {
                i += 1;
            }
            words.push((start, i));
        } else {
            i += 1;
        }
    }

    // Step 2: For each word, split into syllables.
    let mut raw_breaks = Vec::new();
    for &(word_start, word_end) in &words {
        let word: String = chars[word_start..word_end].iter().collect();

        // Check for French contraction
        if is_french {
            if let Some(apos_pos) = find_french_contraction(&word) {
                let contraction_end = word_start + apos_pos + 1;
                raw_breaks.push(contraction_end);

                // Hyphenate the remainder
                let remainder: String = chars[contraction_end..word_end].iter().collect();
                if remainder.len() > 1 {
                    hyphenate_word(
                        &remainder,
                        contraction_end,
                        hypher_lang,
                        is_french,
                        &mut raw_breaks,
                    );
                }
                continue;
            }
        }

        // Regular word
        hyphenate_word(&word, word_start, hypher_lang, is_french, &mut raw_breaks);
    }

    // Step 3: Add breaks at word starts (except the first word)
    for &(word_start, _) in words.iter().skip(1) {
        if !raw_breaks.contains(&word_start) {
            raw_breaks.push(word_start);
        }
    }

    raw_breaks.sort();
    raw_breaks.dedup();
    raw_breaks.retain(|&b| b > 0 && b < total_chars);
    raw_breaks
}

/// Hyphenate a single word and add break positions to raw_breaks.
/// Uses hypher first, then falls back to French CV rules if hypher returns no breaks.
fn hyphenate_word(
    word: &str,
    offset: usize,
    hypher_lang: Lang,
    is_french: bool,
    raw_breaks: &mut Vec<usize>,
) {
    // Strip apostrophes for hypher (it doesn't handle them)
    let clean: String = word.chars().filter(|c| c.is_alphabetic()).collect();
    if clean.len() <= 1 {
        return;
    }

    let syllables: Vec<&str> = hyphenate(&clean, hypher_lang).collect();
    if syllables.len() > 1 {
        // hypher found breaks — map back to original positions
        let mut pos = offset;
        for (si, syl) in syllables.iter().enumerate() {
            let syl_len = syl.chars().count();
            let mut counted = 0;
            while counted < syl_len && pos < offset + word.chars().count() {
                let ch = word.chars().nth(pos - offset).unwrap_or(' ');
                pos += 1;
                if ch.is_alphabetic() {
                    counted += 1;
                }
            }
            if si < syllables.len() - 1 {
                raw_breaks.push(pos);
            }
        }
    } else if is_french && clean.len() >= 3 {
        // Fallback: French CV-based syllable splitting
        let fb = french_syllable_breaks(&clean);
        for b in fb {
            raw_breaks.push(offset + b);
        }
    }
}

/// Simple French syllable rules as fallback when hypher returns no breaks.
/// Uses the principle: a consonant between two vowels starts a new syllable.
/// "adore" → [1, 3] (a|do|re)
fn french_syllable_breaks(word: &str) -> Vec<usize> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    if n < 3 {
        return Vec::new();
    }

    let is_vowel =
        |ch: char| "aeiouyàâäéèêëïîôùûüœæ".contains(ch.to_lowercase().next().unwrap_or(ch));

    let mut breaks = Vec::new();
    let mut i = 1;
    while i < n {
        // Rule: a single consonant between vowels starts a new syllable
        if !is_vowel(chars[i]) && i + 1 < n && is_vowel(chars[i + 1]) {
            if i > 0 && is_vowel(chars[i - 1]) {
                breaks.push(i);
                i += 2; // skip past the vowel after
                continue;
            }
        }
        // Rule: two consonants between vowels — split between them
        if !is_vowel(chars[i]) && i + 2 < n && !is_vowel(chars[i + 1]) && is_vowel(chars[i + 2]) {
            if i > 0 && is_vowel(chars[i - 1]) {
                // Exception: don't split "bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr",
                // "pl", "pr", "tr", "vr" — these stay together
                let c1 = chars[i].to_lowercase().next().unwrap_or(chars[i]);
                let c2 = chars[i + 1].to_lowercase().next().unwrap_or(chars[i + 1]);
                let inseparable = matches!(
                    (c1, c2),
                    ('b', 'l')
                        | ('b', 'r')
                        | ('c', 'l')
                        | ('c', 'r')
                        | ('c', 'h')
                        | ('d', 'r')
                        | ('f', 'l')
                        | ('f', 'r')
                        | ('g', 'l')
                        | ('g', 'r')
                        | ('p', 'l')
                        | ('p', 'r')
                        | ('p', 'h')
                        | ('t', 'r')
                        | ('t', 'h')
                        | ('v', 'r')
                );
                if inseparable {
                    breaks.push(i);
                } else {
                    breaks.push(i + 1);
                }
                i += 3;
                continue;
            }
        }
        i += 1;
    }

    breaks
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphabetic() || ch == '\'' || ch == '\u{2019}'
}

fn find_french_contraction(word: &str) -> Option<usize> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return None;
    }

    let first_lower = chars[0].to_lowercase().next().unwrap_or(chars[0]);
    if matches!(first_lower, 'j' | 'l' | 'd' | 'n' | 's' | 'c' | 'm' | 't')
        && is_apostrophe(chars[1])
        && chars[2].is_alphabetic()
    {
        return Some(1);
    }

    if chars.len() >= 4
        && chars[0].to_lowercase().next() == Some('q')
        && chars[1].to_lowercase().next() == Some('u')
        && is_apostrophe(chars[2])
        && chars[3].is_alphabetic()
    {
        return Some(2);
    }

    None
}

fn is_apostrophe(ch: char) -> bool {
    ch == '\'' || ch == '\u{2019}'
}

pub fn syllable_count(text: &str, lang: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    syllable_breaks(text, lang).len() + 1
}

/// Compute default ratios proportional to the character count of each segment.
/// This gives a visually correct initial distribution where longer syllables
/// get more space than shorter ones.
pub fn default_ratios_from_breaks(text: &str, breaks: &[usize]) -> Vec<f32> {
    if breaks.is_empty() {
        return Vec::new();
    }
    let total_chars = text.chars().count();
    if total_chars == 0 {
        return Vec::new();
    }
    let mut lengths = Vec::new();
    let mut prev = 0;
    for &b in breaks {
        lengths.push((b - prev) as f32);
        prev = b;
    }
    lengths.push((total_chars - prev) as f32);

    let sum: f32 = lengths.iter().sum();
    if sum <= 0.0 {
        let r = 1.0 / lengths.len() as f32;
        return vec![r; lengths.len()];
    }
    lengths.iter().map(|l| l / sum).collect()
}

/// Uniform ratios (legacy, used where char proportions are not available).
pub fn default_ratios(count: usize) -> Vec<f32> {
    if count <= 1 {
        return Vec::new();
    }
    let r = 1.0 / count as f32;
    vec![r; count]
}

pub fn timing_ratios(text: &str, saved_ratios: &[f32], lang: &str) -> Vec<f32> {
    if text.is_empty() {
        return Vec::new();
    }

    let breaks = syllable_breaks(text, lang);
    let count = breaks.len() + 1;
    if saved_ratios.len() == count
        && saved_ratios
            .iter()
            .all(|ratio| ratio.is_finite() && *ratio > 0.0)
    {
        return normalized_ratios(saved_ratios);
    }

    if breaks.is_empty() {
        vec![1.0]
    } else {
        default_ratios_from_breaks(text, &breaks)
    }
}

pub fn active_syllable_local_progress(ratios: &[f32], progress: f32) -> Option<f32> {
    if ratios.is_empty() {
        return None;
    }

    let ratios = normalized_ratios(ratios);
    let progress = progress.clamp(0.0, 1.0);
    let mut start = 0.0;
    for ratio in ratios {
        let end = (start + ratio).min(1.0);
        if progress <= end {
            let width = (end - start).max(f32::EPSILON);
            return Some(((progress - start) / width).clamp(0.0, 1.0));
        }
        start = end;
    }

    Some(1.0)
}

pub fn visual_progress_from_timing(
    text: &str,
    saved_ratios: &[f32],
    lang: &str,
    progress: f32,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let progress = progress.clamp(0.0, 1.0);
    let breaks = syllable_breaks(text, lang);
    if breaks.is_empty() {
        return progress;
    }

    let timing = timing_ratios(text, saved_ratios, lang);
    let visual = default_ratios_from_breaks(text, &breaks);
    if timing.len() != visual.len() || timing.is_empty() {
        return progress;
    }

    let timing = normalized_ratios(&timing);
    let visual = normalized_ratios(&visual);
    let mut timing_start = 0.0;
    let mut visual_start = 0.0;
    for (timing_ratio, visual_ratio) in timing.iter().zip(visual.iter()) {
        let timing_end = (timing_start + timing_ratio).min(1.0);
        if progress <= timing_end {
            let local = if timing_end > timing_start {
                (progress - timing_start) / (timing_end - timing_start)
            } else {
                0.0
            };
            return (visual_start + local.clamp(0.0, 1.0) * visual_ratio).clamp(0.0, 1.0);
        }
        timing_start = timing_end;
        visual_start = (visual_start + visual_ratio).min(1.0);
    }

    1.0
}

fn normalized_ratios(ratios: &[f32]) -> Vec<f32> {
    let sum: f32 = ratios
        .iter()
        .copied()
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .sum();
    if sum <= f32::EPSILON {
        return vec![1.0 / ratios.len().max(1) as f32; ratios.len()];
    }
    ratios.iter().map(|ratio| (*ratio).max(0.0) / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        assert!(syllable_breaks("", "fr-fr").is_empty());
    }

    #[test]
    fn test_french_fallback_adore() {
        let breaks = french_syllable_breaks("adore");
        // a|do|re → breaks at [1, 3]
        assert_eq!(
            breaks,
            vec![1, 3],
            "adore should split as a|do|re, got {:?}",
            breaks
        );
    }

    #[test]
    fn test_french_fallback_spaghettis() {
        let breaks = french_syllable_breaks("spaghettis");
        assert!(
            !breaks.is_empty(),
            "spaghettis should have breaks, got {:?}",
            breaks
        );
    }

    #[test]
    fn test_contraction_jadore() {
        let breaks = syllable_breaks("J'adore", "fr-fr");
        // J' | a | do | re → at least 3 breaks
        assert!(
            breaks.len() >= 2,
            "J'adore should have >=2 breaks, got {:?}",
            breaks
        );
        assert!(
            breaks.contains(&2),
            "Should break after J' (pos 2), got {:?}",
            breaks
        );
    }

    #[test]
    fn test_full_sentence() {
        let breaks = syllable_breaks("J'adore les spaghettis", "fr-fr");
        assert!(breaks.len() >= 4, "Expected >=4 breaks, got {:?}", breaks);
    }

    #[test]
    fn test_word_boundaries() {
        let breaks = syllable_breaks("un deux trois", "fr-fr");
        assert!(
            breaks.len() >= 2,
            "Expected >=2 word breaks, got {:?}",
            breaks
        );
    }

    #[test]
    fn test_default_ratios() {
        assert!(default_ratios(0).is_empty());
        assert!(default_ratios(1).is_empty());
        let r = default_ratios(3);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn test_proportional_ratios() {
        // "J'adore" with break at 2 → segments "J'" (2 chars) and "adore" (5 chars)
        let r = default_ratios_from_breaks("J'adore", &[2]);
        assert_eq!(r.len(), 2);
        // "J'" should get 2/7 ≈ 0.286, "adore" 5/7 ≈ 0.714
        assert!(r[0] < r[1], "J' should be smaller than adore: {:?}", r);
        let sum: f32 = r.iter().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_visual_progress_uses_timing_ratios() {
        let progress = visual_progress_from_timing("adore", &[0.8, 0.1, 0.1], "fr-fr", 0.4);
        assert!(
            progress < 0.2,
            "visual progress should linger on stretched first syllable, got {progress}"
        );
    }
}
