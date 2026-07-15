#![allow(clippy::collapsible_if)]

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
    let clean_len = clean.chars().count();
    if clean_len <= 1 {
        return;
    }

    let mut clean_breaks = Vec::new();
    let syllables: Vec<&str> = hyphenate(&clean, hypher_lang).collect();
    if syllables.len() > 1 {
        // hypher found breaks. English dictionaries can leave large chunks like
        // "bourine", so refine each returned chunk before mapping positions back.
        let mut clean_pos = 0;
        for (si, syl) in syllables.iter().enumerate() {
            let syl_len = syl.chars().count();

            if !is_french {
                for inner_break in english_syllable_breaks(syl) {
                    clean_breaks.push(clean_pos + inner_break);
                }
            }

            clean_pos += syl_len;
            if si < syllables.len() - 1 {
                clean_breaks.push(clean_pos);
            }
        }
    } else if is_french && clean_len >= 3 {
        // Fallback: French CV-based syllable splitting
        clean_breaks.extend(french_syllable_breaks(&clean));
    } else if !is_french && clean_len >= 4 {
        clean_breaks.extend(english_syllable_breaks(&clean));
    }

    push_clean_breaks(word, offset, clean_len, clean_breaks, raw_breaks);
}

fn push_clean_breaks(
    word: &str,
    offset: usize,
    clean_len: usize,
    mut clean_breaks: Vec<usize>,
    raw_breaks: &mut Vec<usize>,
) {
    clean_breaks.sort_unstable();
    clean_breaks.dedup();
    clean_breaks.retain(|&b| b > 0 && b < clean_len);

    let mut breaks = clean_breaks.into_iter().peekable();
    if breaks.peek().is_none() {
        return;
    }

    let mut alpha_count = 0;
    for (idx, ch) in word.chars().enumerate() {
        if !ch.is_alphabetic() {
            continue;
        }

        alpha_count += 1;
        while breaks.peek().copied() == Some(alpha_count) {
            raw_breaks.push(offset + idx + 1);
            breaks.next();
        }
    }
}

/// Conservative English fallback: split only between separate vowel nuclei.
fn english_syllable_breaks(word: &str) -> Vec<usize> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    if n < 4 {
        return Vec::new();
    }

    let nuclei = english_vowel_nuclei(&chars);
    if nuclei.len() < 2 {
        return Vec::new();
    }

    let mut breaks = Vec::new();
    for pair in nuclei.windows(2) {
        let prev_end = pair[0].1;
        let next_start = pair[1].0;
        if next_start <= prev_end {
            continue;
        }

        let consonant_count = next_start - prev_end;
        let break_pos =
            if consonant_count <= 1 || is_english_onset_cluster(&chars[prev_end..next_start]) {
                prev_end
            } else {
                prev_end + 1
            };

        if break_pos > 0 && break_pos < n {
            breaks.push(break_pos);
        }
    }

    breaks
}

fn english_vowel_nuclei(chars: &[char]) -> Vec<(usize, usize)> {
    let mut nuclei = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_english_vowel_at(chars, i) {
            i += 1;
            continue;
        }

        let start = i;
        i += 1;
        while i < chars.len() && is_english_vowel_at(chars, i) {
            i += 1;
        }
        nuclei.push((start, i));
    }

    nuclei
}

fn is_english_vowel_at(chars: &[char], i: usize) -> bool {
    let ch = lower_ascii(chars[i]);
    if ch == 'e' && is_silent_final_e(chars, i) {
        return false;
    }

    if is_basic_english_vowel(ch) {
        return true;
    }

    if ch != 'y' || i == 0 {
        return false;
    }

    let prev_is_vowel = i > 0 && is_basic_english_vowel(lower_ascii(chars[i - 1]));
    let next_is_vowel = i + 1 < chars.len() && is_basic_english_vowel(lower_ascii(chars[i + 1]));
    !prev_is_vowel && !next_is_vowel
}

fn is_silent_final_e(chars: &[char], i: usize) -> bool {
    i + 1 == chars.len()
        && chars.len() > 2
        && lower_ascii(chars[i]) == 'e'
        && !is_basic_english_vowel(lower_ascii(chars[i - 1]))
        && chars[..i]
            .iter()
            .any(|&ch| is_basic_english_vowel(lower_ascii(ch)) || lower_ascii(ch) == 'y')
}

fn is_basic_english_vowel(ch: char) -> bool {
    matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn is_english_onset_cluster(cluster: &[char]) -> bool {
    let cluster: String = cluster.iter().map(|&ch| lower_ascii(ch)).collect();
    matches!(
        cluster.as_str(),
        "bl" | "br"
            | "ch"
            | "cl"
            | "cr"
            | "dr"
            | "fl"
            | "fr"
            | "gl"
            | "gr"
            | "ph"
            | "pl"
            | "pr"
            | "qu"
            | "sc"
            | "sh"
            | "sk"
            | "sl"
            | "sm"
            | "sn"
            | "sp"
            | "st"
            | "sw"
            | "th"
            | "tr"
            | "tw"
            | "wh"
            | "wr"
            | "scr"
            | "shr"
            | "spl"
            | "spr"
            | "str"
            | "thr"
    )
}

fn lower_ascii(ch: char) -> char {
    ch.to_ascii_lowercase()
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
        if !is_vowel(chars[i])
            && i > 0
            && i + 1 < n
            && is_vowel(chars[i - 1])
            && is_vowel(chars[i + 1])
        {
            breaks.push(i);
            i += 2; // skip past the vowel after
            continue;
        }
        // Rule: two consonants between vowels — split between them
        if !is_vowel(chars[i])
            && i > 0
            && i + 2 < n
            && is_vowel(chars[i - 1])
            && !is_vowel(chars[i + 1])
            && is_vowel(chars[i + 2])
        {
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

/// End of the highlighted prefix. The complete word crossing the reading line
/// is included so the highlight never cuts through a glyph or a word.
pub fn read_highlight_end(text: &str, progress: f32) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() || !(0.0..=1.0).contains(&progress) {
        return None;
    }
    let index = ((progress * chars.len() as f32).floor() as usize).min(chars.len() - 1);
    if chars[index].is_whitespace() {
        let mut end = index + 1;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }
        return Some(end);
    }
    let mut end = index + 1;
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    while end < chars.len() && chars[end].is_whitespace() {
        end += 1;
    }
    Some(end)
}

pub fn read_highlight_end_from_timing(
    text: &str,
    saved_ratios: &[f32],
    lang: &str,
    progress: f32,
) -> Option<usize> {
    if !(0.0..=1.0).contains(&progress) {
        return None;
    }
    let visual_progress = visual_progress_from_timing(text, saved_ratios, lang, progress);
    read_highlight_end(text, visual_progress)
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

/// Convert shaped character boundary positions into one width ratio per
/// syllable. This keeps untouched handles aligned with the rendered glyphs
/// instead of approximating their positions from character counts.
pub fn visual_ratios_from_char_positions(
    text: &str,
    breaks: &[usize],
    char_x_ratios: &[f32],
) -> Option<Vec<f32>> {
    let char_count = text.chars().count();
    if breaks.is_empty() || char_x_ratios.len() != char_count + 1 {
        return None;
    }

    let mut boundaries = Vec::with_capacity(breaks.len() + 2);
    boundaries.push(0.0_f32);
    for &char_index in breaks {
        if char_index == 0 || char_index >= char_count {
            return None;
        }
        boundaries.push(*char_x_ratios.get(char_index)?);
    }
    boundaries.push(1.0);

    let mut ratios = Vec::with_capacity(boundaries.len() - 1);
    for pair in boundaries.windows(2) {
        let ratio = pair[1] - pair[0];
        if !ratio.is_finite() || ratio <= f32::EPSILON {
            return None;
        }
        ratios.push(ratio);
    }

    let sum: f32 = ratios.iter().sum();
    if (sum - 1.0).abs() > 0.001 {
        return None;
    }
    Some(ratios)
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

#[derive(Debug, Clone, PartialEq)]
pub struct DialogueSplit {
    pub split_char: usize,
    pub split_progress: f32,
    pub first_text: String,
    pub second_text: String,
    pub first_ratios: Vec<f32>,
    pub second_ratios: Vec<f32>,
}

pub fn split_dialogue_at_syllable_progress(
    text: &str,
    saved_ratios: &[f32],
    lang: &str,
    progress: f32,
) -> Option<DialogueSplit> {
    let breaks = syllable_breaks(text, lang);
    let ratios = timing_ratios(text, saved_ratios, lang);
    let break_index = nearest_break_index_for_progress(&ratios, progress)?;
    split_dialogue_at_break_index(text, saved_ratios, lang, &breaks, break_index)
}

pub fn split_dialogue_at_syllable_cursor(
    text: &str,
    saved_ratios: &[f32],
    lang: &str,
    cursor_pos: usize,
) -> Option<DialogueSplit> {
    let breaks = syllable_breaks(text, lang);
    let break_index = breaks
        .iter()
        .enumerate()
        .min_by_key(|(_, split_char)| split_char.abs_diff(cursor_pos))
        .map(|(index, _)| index)?;
    split_dialogue_at_break_index(text, saved_ratios, lang, &breaks, break_index)
}

fn nearest_break_index_for_progress(ratios: &[f32], progress: f32) -> Option<usize> {
    if ratios.len() <= 1 {
        return None;
    }

    let progress = progress.clamp(0.0, 1.0);
    let ratios = normalized_ratios(ratios);
    let mut cumulative = 0.0;
    let mut best: Option<(usize, f32)> = None;
    for (index, ratio) in ratios.iter().take(ratios.len() - 1).enumerate() {
        cumulative = (cumulative + ratio).min(1.0);
        let distance = (progress - cumulative).abs();
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

fn split_dialogue_at_break_index(
    text: &str,
    saved_ratios: &[f32],
    lang: &str,
    breaks: &[usize],
    break_index: usize,
) -> Option<DialogueSplit> {
    let split_char = *breaks.get(break_index)?;
    let chars: Vec<char> = text.chars().collect();
    if split_char == 0 || split_char >= chars.len() {
        return None;
    }

    let ratios = timing_ratios(text, saved_ratios, lang);
    if ratios.len() != breaks.len() + 1 {
        return None;
    }

    let normalized = normalized_ratios(&ratios);
    let split_progress = normalized
        .iter()
        .take(break_index + 1)
        .sum::<f32>()
        .clamp(0.0, 1.0);

    let inside_word = split_is_inside_word(&chars, split_char);
    let mut first_text = chars[..split_char].iter().collect::<String>();
    let mut second_text = chars[split_char..].iter().collect::<String>();
    first_text = first_text.trim_end().to_string();
    second_text = second_text.trim_start().to_string();

    if inside_word {
        if !first_text.ends_with('-') {
            first_text.push('-');
        }
        if !second_text.starts_with('-') {
            second_text.insert(0, '-');
        }
    }

    if first_text.is_empty() || second_text.is_empty() {
        return None;
    }

    let first_ratios = ratios_for_split_piece(&first_text, &normalized[..=break_index], lang);
    let second_ratios = ratios_for_split_piece(&second_text, &normalized[break_index + 1..], lang);

    Some(DialogueSplit {
        split_char,
        split_progress,
        first_text,
        second_text,
        first_ratios,
        second_ratios,
    })
}

fn ratios_for_split_piece(text: &str, ratios: &[f32], lang: &str) -> Vec<f32> {
    if ratios.is_empty() {
        return Vec::new();
    }

    let ratios = normalized_ratios(ratios);
    if syllable_count(text, lang) == ratios.len() {
        ratios
    } else {
        Vec::new()
    }
}

fn split_is_inside_word(chars: &[char], split_char: usize) -> bool {
    split_char > 0
        && split_char < chars.len()
        && is_word_char(chars[split_char - 1])
        && is_word_char(chars[split_char])
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
    fn test_english_tambourine() {
        let breaks = syllable_breaks("tambourine", "en-us");
        assert_eq!(
            breaks,
            vec![3, 6],
            "tambourine should split as tam|bou|rine, got {:?}",
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
    fn visual_ratios_follow_shaped_character_positions() {
        let ratios = visual_ratios_from_char_positions(
            "iiWWWW",
            &[2],
            &[0.0, 0.05, 0.10, 0.30, 0.53, 0.76, 1.0],
        )
        .unwrap();

        assert_eq!(ratios, vec![0.10, 0.90]);
    }

    #[test]
    fn test_visual_progress_uses_timing_ratios() {
        let progress = visual_progress_from_timing("adore", &[0.8, 0.1, 0.1], "fr-fr", 0.4);
        assert!(
            progress < 0.2,
            "visual progress should linger on stretched first syllable, got {progress}"
        );
    }

    #[test]
    fn read_highlight_keeps_past_words_and_completes_the_current_word() {
        assert_eq!(read_highlight_end("un deux", 0.10), Some(3));
        assert_eq!(read_highlight_end("un deux", 0.30), Some(3));
        assert_eq!(read_highlight_end("un deux", 0.75), Some(7));
        assert_eq!(read_highlight_end("un deux", 1.20), None);
    }

    #[test]
    fn test_split_dialogue_adds_hyphens_inside_word() {
        let split = split_dialogue_at_syllable_progress("tambourine", &[], "en-us", 0.31)
            .expect("tambourine should split on a syllable");

        assert_eq!(split.first_text, "tam-");
        assert_eq!(split.second_text, "-bourine");
        assert_eq!(split.split_char, 3);
    }

    #[test]
    fn test_split_dialogue_sentence_boundary_has_no_hyphens() {
        let text = "Bonjour. Ça va";
        let cursor_pos = text.chars().position(|ch| ch == 'Ç').unwrap();
        let split = split_dialogue_at_syllable_cursor(text, &[], "fr-fr", cursor_pos)
            .expect("sentence boundary should split");

        assert_eq!(split.first_text, "Bonjour.");
        assert_eq!(split.second_text, "Ça va");
        assert!(!split.first_text.ends_with('-'));
        assert!(!split.second_text.starts_with('-'));
    }
}
