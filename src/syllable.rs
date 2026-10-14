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
        "es-419" | "es-es" | "es" => Lang::Spanish,
        _ => Lang::French,
    };
    let is_french = matches!(lang, "fr-fr" | "fr");

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

            if hypher_lang == Lang::English {
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
    } else if hypher_lang == Lang::English && clean_len >= 4 {
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
    if let Some(ratios) = adapted_ratios_for_breaks(text, saved_ratios, &breaks) {
        return ratios;
    }

    if breaks.is_empty() {
        vec![1.0]
    } else {
        default_ratios_from_breaks(text, &breaks)
    }
}

/// Saved karaoke timings expressed for the syllables of `text` in `lang`.
///
/// Changing a band's language (or its text) changes the syllable count, but
/// the timing work must survive: instead of falling back to the defaults, the
/// saved timings are carried over to the new syllables. Returns `None` when no
/// valid timing was saved or the text has a single syllable.
pub fn adapted_saved_ratios(text: &str, saved_ratios: &[f32], lang: &str) -> Option<Vec<f32>> {
    adapted_ratios_for_breaks(text, saved_ratios, &syllable_breaks(text, lang))
}

const SYLLABLE_LANGUAGE_CODES: [&str; 3] = ["fr-fr", "en-us", "es-419"];

fn adapted_ratios_for_breaks(
    text: &str,
    saved_ratios: &[f32],
    breaks: &[usize],
) -> Option<Vec<f32>> {
    if saved_ratios.is_empty()
        || !saved_ratios
            .iter()
            .all(|ratio| ratio.is_finite() && *ratio > 0.0)
    {
        return None;
    }
    let count = breaks.len() + 1;
    if saved_ratios.len() == count {
        return Some(normalized_ratios(saved_ratios));
    }
    let char_count = text.chars().count();
    if breaks.is_empty() || saved_ratios.len() < 2 || char_count == 0 {
        return None;
    }

    // Where the saved syllables started in the text: the syllabification of
    // another language when one matches (the band's language was changed
    // before the timings were converted), otherwise evenly spread over the
    // text (the text itself was changed).
    let source_breaks = SYLLABLE_LANGUAGE_CODES
        .iter()
        .map(|code| syllable_breaks(text, code))
        .find(|candidate| candidate.len() + 1 == saved_ratios.len())
        .unwrap_or_else(|| {
            (1..saved_ratios.len())
                .map(|index| index * char_count / saved_ratios.len())
                .collect()
        });
    warp_ratios_to_breaks(text, saved_ratios, &source_breaks, breaks)
}

/// Saved timings of `text` syllabified in `from`, expressed for its syllables
/// in `to`: every letter keeps its place in time, only the boundaries move.
/// `None` when nothing valid was saved for the `from` syllables.
pub fn ratios_for_other_language(
    text: &str,
    saved_ratios: &[f32],
    from: &str,
    to: &str,
) -> Option<Vec<f32>> {
    if from == to || text.is_empty() {
        return None;
    }
    let source_breaks = syllable_breaks(text, from);
    let target_breaks = syllable_breaks(text, to);
    if source_breaks == target_breaks {
        return None;
    }
    if saved_ratios.len() != source_breaks.len() + 1
        || !saved_ratios
            .iter()
            .all(|ratio| ratio.is_finite() && *ratio > 0.0)
    {
        return None;
    }
    if target_breaks.is_empty() {
        return Some(vec![1.0]);
    }
    warp_ratios_to_breaks(text, saved_ratios, &source_breaks, &target_breaks)
}

fn warp_ratios_to_breaks(
    text: &str,
    saved_ratios: &[f32],
    source_breaks: &[usize],
    breaks: &[usize],
) -> Option<Vec<f32>> {
    let char_count = text.chars().count();
    if char_count == 0 || saved_ratios.is_empty() {
        return None;
    }
    let count = breaks.len() + 1;

    // Piecewise-linear map from text position to timing position.
    let saved = normalized_ratios(saved_ratios);
    let mut controls = Vec::with_capacity(saved.len() + 1);
    controls.push((0.0_f32, 0.0_f32));
    let mut time = 0.0_f32;
    for (index, ratio) in saved.iter().enumerate() {
        time += ratio;
        let source = source_breaks
            .get(index)
            .map_or(1.0, |boundary| *boundary as f32 / char_count as f32);
        controls.push((source, time.min(1.0)));
    }
    let warp = |position: f32| -> f32 {
        for pair in controls.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            if position <= x1 {
                if x1 - x0 <= f32::EPSILON {
                    return y1;
                }
                return y0 + (position - x0) / (x1 - x0) * (y1 - y0);
            }
        }
        1.0
    };

    let mut ratios = Vec::with_capacity(count);
    let mut previous = 0.0_f32;
    for boundary in breaks
        .iter()
        .map(|boundary| warp(*boundary as f32 / char_count as f32))
        .chain(std::iter::once(1.0))
    {
        ratios.push((boundary - previous).max(0.001));
        previous = boundary.max(previous);
    }
    Some(normalized_ratios(&ratios))
}

/// A syllable slot kept empty after its text was erased, so that typing there
/// again takes the slot back as it was instead of carving a new one out of a
/// neighbour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReservedSlot {
    /// Index, in the current text, of the syllable the slot sits after.
    pub after_segment: usize,
    /// Share of the line the slot keeps.
    pub ratio: f32,
}

/// Saved karaoke timings after a text edit, and the slot the edit left empty.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditTimings {
    pub ratios: Vec<f32>,
    /// Slot of the syllables the edit erased (none typed in their place), or
    /// an earlier reservation the edit did not use, moved to its new place.
    pub reserved: Option<ReservedSlot>,
}

/// Char index of the end of syllable `index` of `text`.
pub fn syllable_end(text: &str, lang: &str, index: usize) -> usize {
    syllable_breaks(text, lang)
        .get(index)
        .copied()
        .unwrap_or_else(|| text.chars().count())
}

/// Saved karaoke timings after the text of a line changed from `old_text` to
/// `new_text`: see [`timings_after_text_edit`].
pub fn ratios_after_text_edit(
    old_text: &str,
    new_text: &str,
    saved_ratios: &[f32],
    lang: &str,
) -> Option<Vec<f32>> {
    timings_after_text_edit(old_text, new_text, saved_ratios, lang, None).map(|t| t.ratios)
}

/// Saved karaoke timings after the text of a line changed from `old_text` to
/// `new_text`.
///
/// Syllables the edit did not touch keep their exact timing; the syllables it
/// created or changed share the slot of the ones they replace (a word going
/// from one syllable to two is split inside its own slot), and brand-new
/// syllables are cut out of the slot of the syllable next to them. Erasing
/// whole syllables gives their slot to the syllable before, and reports it as
/// `reserved`: passed back on the next edit, typing there again takes the
/// whole slot back. Returns `None` when no timing was saved, so untouched
/// lines keep their default timing.
pub fn timings_after_text_edit(
    old_text: &str,
    new_text: &str,
    saved_ratios: &[f32],
    lang: &str,
    reserved: Option<ReservedSlot>,
) -> Option<TextEditTimings> {
    let old_breaks = syllable_breaks(old_text, lang);
    let old_ratios = adapted_ratios_for_breaks(old_text, saved_ratios, &old_breaks)
        .or_else(|| (old_breaks.is_empty() && !saved_ratios.is_empty()).then(|| vec![1.0]))?;
    if old_text == new_text {
        return Some(TextEditTimings {
            ratios: saved_ratios.to_vec(),
            reserved,
        });
    }
    let new_breaks = syllable_breaks(new_text, lang);
    if new_breaks.is_empty() {
        return Some(TextEditTimings {
            ratios: vec![1.0],
            reserved: None,
        });
    }
    let reserved = reserved.filter(|slot| slot.after_segment < old_ratios.len());

    let old_chars = old_text.chars().collect::<Vec<_>>();
    let new_chars = new_text.chars().collect::<Vec<_>>();
    let prefix = old_chars
        .iter()
        .zip(&new_chars)
        .take_while(|(old, new)| old == new)
        .count();
    let suffix = old_chars[prefix..]
        .iter()
        .rev()
        .zip(new_chars[prefix..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let old_edit_end = old_chars.len() - suffix;
    let shift = new_chars.len() as isize - old_chars.len() as isize;

    let segments = |breaks: &[usize], len: usize| {
        let mut bounds = Vec::with_capacity(breaks.len() + 2);
        bounds.push(0);
        bounds.extend_from_slice(breaks);
        bounds.push(len);
        bounds
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect::<Vec<_>>()
    };
    // Untouched old syllables, expressed in the new text.
    let old_segments = segments(&old_breaks, old_chars.len());
    let kept_old = old_segments
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(old_index, (start, end))| {
            if end <= prefix {
                Some((old_index, (start, end)))
            } else if start >= old_edit_end {
                let moved = |index: usize| (index as isize + shift) as usize;
                Some((old_index, (moved(start), moved(end))))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let new_segments = segments(&new_breaks, new_chars.len());
    let mut ratios: Vec<Option<f32>> = vec![None; new_segments.len()];
    // Old syllable each new syllable is the untouched copy of.
    let mut origin: Vec<Option<usize>> = vec![None; new_segments.len()];
    let mut used_old = vec![false; kept_old.len()];
    for (index, segment) in new_segments.iter().enumerate() {
        if let Some(kept_index) = kept_old
            .iter()
            .enumerate()
            .position(|(kept_index, (_, span))| !used_old[kept_index] && span == segment)
        {
            used_old[kept_index] = true;
            let old_index = kept_old[kept_index].0;
            origin[index] = Some(old_index);
            ratios[index] = Some(old_ratios[old_index]);
        }
    }
    let new_index_of = |old_index: usize| origin.iter().position(|o| *o == Some(old_index));

    // The edited syllables only share the time of the ones they replace, so
    // the rest of the line never moves: a syllable cut in two is split inside
    // its own slot.
    let kept_total: f32 = ratios.iter().flatten().sum();
    let replaced_total = (1.0 - kept_total).max(0.0);
    let segment_chars = |index: usize| {
        let (start, end) = new_segments[index];
        (end - start).max(1) as f32
    };
    let share_by_chars = |ratios: &mut Vec<Option<f32>>, members: &[usize], total: f32| {
        let chars: f32 = members.iter().map(|index| segment_chars(*index)).sum();
        for member in members {
            ratios[*member] = Some((total * segment_chars(*member) / chars).max(0.001));
        }
    };
    let changed = (0..ratios.len())
        .filter(|index| ratios[*index].is_none())
        .collect::<Vec<_>>();
    let mut next_reserved = None;
    if changed.is_empty() {
        // Erased syllables give their time to the syllable before the edit,
        // which keeps it in reserve for whatever is typed there next.
        if replaced_total > f32::EPSILON {
            let receiver = new_segments
                .iter()
                .rposition(|(start, _)| *start < prefix.max(1))
                .unwrap_or(0);
            if let Some(ratio) = ratios[receiver].as_mut() {
                *ratio += replaced_total;
            }
            let carried = reserved
                .filter(|slot| new_index_of(slot.after_segment) == Some(receiver))
                .map_or(0.0, |slot| slot.ratio);
            next_reserved = Some(ReservedSlot {
                after_segment: receiver,
                ratio: replaced_total + carried,
            });
        } else if let Some(slot) = reserved {
            next_reserved = new_index_of(slot.after_segment).map(|after_segment| ReservedSlot {
                after_segment,
                ..slot
            });
        }
    } else if replaced_total > 0.000_1 {
        // Syllables the edit touched share the slot of the ones they replace.
        // When the syllable holding a reserved slot is among them, what is
        // typed after it takes the reserved slot and the rest keeps its own.
        let donor_touched = reserved.filter(|slot| new_index_of(slot.after_segment).is_none());
        let (continuation, fresh): (Vec<usize>, Vec<usize>) = changed
            .iter()
            .copied()
            .partition(|index| new_segments[*index].0 < prefix);
        // Erasing whole syllables (nothing but spaces typed in their place)
        // keeps their slot in reserve after the syllable before them, even
        // when that one absorbed the space they left.
        let new_edit_end = new_chars.len() - suffix;
        let inserted_blank = new_chars[prefix..new_edit_end]
            .iter()
            .all(|character| character.is_whitespace());
        let erased_total: f32 = if inserted_blank {
            old_segments
                .iter()
                .enumerate()
                .filter(|(old_index, _)| !origin.contains(&Some(*old_index)))
                .filter(|(_, (start, end))| {
                    old_chars[*start..*end]
                        .iter()
                        .enumerate()
                        .all(|(offset, character)| {
                            character.is_whitespace()
                                || (prefix..old_edit_end).contains(&(start + offset))
                        })
                })
                .map(|(old_index, _)| old_ratios[old_index])
                .sum()
        } else {
            0.0
        };
        if erased_total > f32::EPSILON && fresh.is_empty() {
            share_by_chars(&mut ratios, &changed, replaced_total - erased_total);
            let receiver = continuation.last().copied().unwrap_or_else(|| {
                new_segments
                    .iter()
                    .rposition(|(start, _)| *start < prefix.max(1))
                    .unwrap_or(0)
            });
            if let Some(ratio) = ratios[receiver].as_mut() {
                *ratio += erased_total;
            }
            let carried = reserved
                .filter(|slot| {
                    donor_touched.is_some() || new_index_of(slot.after_segment) == Some(receiver)
                })
                .map_or(0.0, |slot| slot.ratio);
            next_reserved = Some(ReservedSlot {
                after_segment: receiver,
                ratio: erased_total + carried,
            });
        } else {
            match donor_touched {
                Some(slot) if !fresh.is_empty() && !continuation.is_empty() => {
                    let fresh_total = slot
                        .ratio
                        .min(replaced_total - 0.001 * continuation.len() as f32);
                    share_by_chars(&mut ratios, &continuation, replaced_total - fresh_total);
                    share_by_chars(&mut ratios, &fresh, fresh_total);
                }
                Some(slot) if !continuation.is_empty() => {
                    share_by_chars(&mut ratios, &changed, replaced_total);
                    next_reserved = Some(ReservedSlot {
                        after_segment: *continuation.last().unwrap_or(&0),
                        ratio: slot.ratio.min(replaced_total),
                    });
                }
                _ => {
                    share_by_chars(&mut ratios, &changed, replaced_total);
                    next_reserved = reserved.and_then(|slot| {
                        new_index_of(slot.after_segment).map(|after_segment| ReservedSlot {
                            after_segment,
                            ..slot
                        })
                    });
                }
            }
        }
    } else {
        // Brand-new syllables only: they take the slot reserved after their
        // neighbour when there is one, else they are cut out of the slot of
        // the neighbouring syllable (the one before, or after at the start).
        let mut reserved_left = reserved;
        let mut index = 0;
        while index < ratios.len() {
            if ratios[index].is_some() {
                index += 1;
                continue;
            }
            let run_end = (index..ratios.len())
                .find(|candidate| ratios[*candidate].is_some())
                .unwrap_or(ratios.len());
            let donor = if index > 0 {
                Some(index - 1)
            } else {
                (run_end < ratios.len()).then_some(run_end)
            };
            let fresh = (index..run_end).collect::<Vec<_>>();
            let reserved_here = reserved_left
                .filter(|slot| donor.and_then(|donor| origin[donor]) == Some(slot.after_segment));
            if let (Some(slot), Some(donor)) = (reserved_here, donor) {
                let donor_ratio = ratios[donor].unwrap_or(1.0);
                let taken = slot.ratio.min(donor_ratio - 0.001).max(0.0);
                ratios[donor] = Some(donor_ratio - taken);
                share_by_chars(&mut ratios, &fresh, taken);
                reserved_left = None;
            } else {
                let group = donor.into_iter().chain(fresh).collect::<Vec<_>>();
                let total = donor.and_then(|donor| ratios[donor]).unwrap_or(1.0);
                share_by_chars(&mut ratios, &group, total);
            }
            index = run_end;
        }
        next_reserved = reserved_left.and_then(|slot| {
            new_index_of(slot.after_segment).map(|after_segment| ReservedSlot {
                after_segment,
                ..slot
            })
        });
    }
    let ratios = normalized_ratios(
        &ratios
            .into_iter()
            .map(|ratio| ratio.unwrap_or(0.001))
            .collect::<Vec<_>>(),
    );
    let reserved = next_reserved.filter(|slot| {
        slot.ratio > 0.001
            && ratios
                .get(slot.after_segment)
                .is_some_and(|ratio| *ratio > slot.ratio)
    });
    Some(TextEditTimings { ratios, reserved })
}

/// Saved karaoke timings after the text of one section of a line was
/// replaced, without disturbing the rest of the line: the section ran from
/// character `section.0` to `section.1` of `old_text` and holds `inserted`
/// characters in `new_text`.
///
/// Syllables outside the section keep their exact timing. Inside it, the new
/// syllables take the slots of the old ones one for one when they are as many
/// (the usual case: a word replaced by a word of the same length in
/// syllables); otherwise they share the time of the section in proportion to
/// their length. Returns `None` when no timing was saved, so untouched lines
/// keep their default timing, and falls back on [`timings_after_text_edit`]
/// when the syllables around the section cannot be matched.
pub fn ratios_after_section_replace(
    old_text: &str,
    new_text: &str,
    section: (usize, usize),
    inserted: usize,
    saved_ratios: &[f32],
    lang: &str,
) -> Option<Vec<f32>> {
    let old_breaks = syllable_breaks(old_text, lang);
    let old_ratios = adapted_ratios_for_breaks(old_text, saved_ratios, &old_breaks)
        .or_else(|| (old_breaks.is_empty() && !saved_ratios.is_empty()).then(|| vec![1.0]))?;
    if old_text == new_text {
        return Some(saved_ratios.to_vec());
    }
    let fallback = || ratios_after_text_edit(old_text, new_text, saved_ratios, lang);
    let old_len = old_text.chars().count();
    let new_len = new_text.chars().count();
    let start = section.0.min(old_len);
    let end = section.1.clamp(start, old_len);
    let new_end = (start + inserted).min(new_len);
    if new_len + (end - start) != old_len + inserted {
        return fallback();
    }
    let new_breaks = syllable_breaks(new_text, lang);
    if new_breaks.is_empty() {
        return Some(vec![1.0]);
    }
    let segments = |breaks: &[usize], len: usize| {
        let mut bounds = Vec::with_capacity(breaks.len() + 2);
        bounds.push(0);
        bounds.extend_from_slice(breaks);
        bounds.push(len);
        bounds
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect::<Vec<_>>()
    };
    let old_segments = segments(&old_breaks, old_len);
    let new_segments = segments(&new_breaks, new_len);
    let overlaps = |(seg_start, seg_end): (usize, usize), (from, to): (usize, usize)| {
        seg_start < to && seg_end > from
    };
    let old_group = (0..old_segments.len())
        .filter(|index| overlaps(old_segments[*index], (start, end)))
        .collect::<Vec<_>>();
    let new_group = (0..new_segments.len())
        .filter(|index| overlaps(new_segments[*index], (start, new_end)))
        .collect::<Vec<_>>();
    let (Some(&first_old), Some(&last_old)) = (old_group.first(), old_group.last()) else {
        // Nothing replaced: the typed text is new, cut out of its neighbours.
        return fallback();
    };
    let before = first_old;
    let after = old_segments.len() - last_old - 1;
    let new_first = new_group.first().copied().unwrap_or(before);
    let new_after = new_group
        .last()
        .map_or(new_segments.len() - before, |last| {
            new_segments.len() - last - 1
        });
    if new_first != before || new_after != after {
        return fallback();
    }
    let total: f32 = old_group.iter().map(|index| old_ratios[*index]).sum();
    let mut ratios = Vec::with_capacity(new_segments.len());
    ratios.extend_from_slice(&old_ratios[..before]);
    if new_group.len() == old_group.len() {
        ratios.extend(old_group.iter().map(|index| old_ratios[*index]));
    } else if !new_group.is_empty() {
        let chars = new_group
            .iter()
            .map(|index| (new_segments[*index].1 - new_segments[*index].0).max(1) as f32)
            .collect::<Vec<_>>();
        let sum: f32 = chars.iter().sum();
        ratios.extend(chars.iter().map(|count| (total * count / sum).max(0.001)));
    }
    ratios.extend_from_slice(&old_ratios[old_segments.len() - after..]);
    if ratios.len() != new_segments.len() {
        return fallback();
    }
    Some(normalized_ratios(&ratios))
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

    fn close(left: &[f32], right: &[f32]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn replacing_a_section_by_as_many_syllables_keeps_every_slot() {
        // "bonjour" (bon|jour) "le" "monde" (mon|de): five syllables, custom
        // timings.
        let old = "bonjour le monde";
        assert_eq!(syllable_count(old, "fr-fr"), 5);
        let saved = [0.1, 0.3, 0.2, 0.25, 0.15];
        // The word "bonjour" (characters 0..7) becomes "salut" (sa|lut).
        let new = "salut le monde";
        assert_eq!(syllable_count(new, "fr-fr"), 5);
        let ratios = ratios_after_section_replace(old, new, (0, 7), 5, &saved, "fr-fr").unwrap();
        assert!(close(&ratios, &saved), "{ratios:?}");
        // "monde" (mon|de) becomes "la mer" (la|mer): the two slots stay.
        let new = "bonjour le la mer";
        assert_eq!(syllable_count(new, "fr-fr"), 5);
        let ratios = ratios_after_section_replace(old, new, (11, 16), 6, &saved, "fr-fr").unwrap();
        assert!(close(&ratios, &saved), "{ratios:?}");
    }

    #[test]
    fn replacing_a_section_by_another_syllable_count_shares_its_time() {
        let old = "bonjour le monde";
        let saved = [0.1, 0.3, 0.2, 0.25, 0.15];
        // "bonjour" (two slots, 0.4 in all) becomes "chat" (one syllable).
        let new = "chat le monde";
        assert_eq!(syllable_count(new, "fr-fr"), 4);
        let ratios = ratios_after_section_replace(old, new, (0, 7), 4, &saved, "fr-fr").unwrap();
        assert!(close(&ratios, &[0.4, 0.2, 0.25, 0.15]), "{ratios:?}");
        // "le" (one slot, 0.2) becomes "petit" (pe|tit): the slot is shared,
        // the other words keep their timing.
        let new = "bonjour petit monde";
        assert_eq!(syllable_count(new, "fr-fr"), 6);
        let ratios = ratios_after_section_replace(old, new, (8, 10), 5, &saved, "fr-fr").unwrap();
        assert_eq!(ratios.len(), 6);
        assert!((ratios[0] - 0.1).abs() < 1e-4 && (ratios[1] - 0.3).abs() < 1e-4);
        assert!((ratios[2] + ratios[3] - 0.2).abs() < 1e-4, "{ratios:?}");
        assert!((ratios[4] - 0.25).abs() < 1e-4 && (ratios[5] - 0.15).abs() < 1e-4);
    }

    #[test]
    fn replacing_a_section_without_saved_timings_changes_nothing() {
        assert_eq!(
            ratios_after_section_replace(
                "bonjour le monde",
                "salut le monde",
                (0, 7),
                5,
                &[],
                "fr-fr"
            ),
            None
        );
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
    fn test_spanish_uses_spanish_hyphenation() {
        let breaks = syllable_breaks("casa bonita", "es-419");
        assert!(
            breaks.contains(&5),
            "must preserve the word boundary: {breaks:?}"
        );
        assert!(
            breaks.len() >= 2,
            "Spanish words should be split: {breaks:?}"
        );
    }

    #[test]
    fn saved_timings_survive_a_language_change_with_another_syllable_count() {
        let text = "tambourine magnifique";
        let french = syllable_breaks(text, "fr-fr");
        let english = syllable_breaks(text, "en-us");
        assert_ne!(french.len(), english.len());

        // Calibrated in French: the first syllable is very long.
        let mut saved = vec![1.0; french.len() + 1];
        saved[0] = 10.0;
        let adapted = timing_ratios(text, &saved, "en-us");
        assert_eq!(adapted.len(), english.len() + 1);
        assert!((adapted.iter().sum::<f32>() - 1.0).abs() < 0.001);
        assert!(adapted.iter().all(|ratio| *ratio > 0.0));
        assert_ne!(adapted, default_ratios_from_breaks(text, &english));

        // Any syllable boundary shared by both languages keeps its timing.
        let saved = normalized_ratios(&saved);
        for (french_index, boundary) in french.iter().enumerate() {
            if let Some(english_index) = english.iter().position(|b| b == boundary) {
                let french_time: f32 = saved[..=french_index].iter().sum();
                let english_time: f32 = adapted[..=english_index].iter().sum();
                assert!((french_time - english_time).abs() < 0.001);
            }
        }

        // Back in French the exact calibration is used.
        assert_eq!(timing_ratios(text, &saved, "fr-fr"), saved);
    }

    #[test]
    fn saved_timings_survive_a_text_change_with_another_syllable_count() {
        let saved = [0.7, 0.1, 0.1, 0.1];
        let adapted = adapted_saved_ratios("la la la la la la", &saved, "fr-fr").unwrap();
        assert_eq!(adapted.len(), 6);
        assert!(adapted[0] > adapted[5]);
        assert!(adapted_saved_ratios("la la", &[], "fr-fr").is_none());
        assert!(adapted_saved_ratios("la", &saved, "fr-fr").is_none());
    }

    #[test]
    fn typing_a_new_word_adds_a_syllable_without_moving_the_others() {
        let old_text = "bonjour toi";
        let lang = "fr-fr";
        assert_eq!(syllable_breaks(old_text, lang).len() + 1, 3);
        let saved = [0.5, 0.2, 0.3];

        let new_text = "bonjour mon toi";
        let ratios = ratios_after_text_edit(old_text, new_text, &saved, lang).unwrap();
        assert_eq!(ratios.len(), syllable_breaks(new_text, lang).len() + 1);
        assert_eq!(ratios.len(), 4);
        // The other syllables do not move: the new one is cut out of the
        // slot of the syllable before it.
        assert!((ratios[0] - 0.5).abs() < 0.001, "{ratios:?}");
        assert!((ratios[1] + ratios[2] - 0.2).abs() < 0.001, "{ratios:?}");
        assert!((ratios[3] - 0.3).abs() < 0.001, "{ratios:?}");
    }

    #[test]
    fn a_word_going_from_one_syllable_to_two_is_split_inside_its_slot() {
        let lang = "fr-fr";
        let old_text = "le chat dort";
        let new_text = "le chateau dort";
        assert_eq!(syllable_breaks(old_text, lang).len() + 1, 3);
        assert_eq!(syllable_breaks(new_text, lang).len() + 1, 4);
        let saved = [0.2, 0.5, 0.3];

        let ratios = ratios_after_text_edit(old_text, new_text, &saved, lang).unwrap();
        assert_eq!(ratios.len(), 4);
        assert!((ratios[0] - 0.2).abs() < 0.001, "{ratios:?}");
        assert!((ratios[1] + ratios[2] - 0.5).abs() < 0.001, "{ratios:?}");
        assert!((ratios[3] - 0.3).abs() < 0.001, "{ratios:?}");

        // Typed letter by letter, the result is the same: nothing outside the
        // word ever moves.
        let mut text = old_text;
        let mut ratios = saved.to_vec();
        for next in ["le chate dort", "le chatea dort", new_text] {
            ratios = ratios_after_text_edit(text, next, &ratios, lang).unwrap();
            text = next;
            let count = ratios.len();
            assert_eq!(count, syllable_breaks(text, lang).len() + 1);
            assert!((ratios[0] - 0.2).abs() < 0.001, "{text}: {ratios:?}");
            assert!(
                (ratios[count - 1] - 0.3).abs() < 0.001,
                "{text}: {ratios:?}"
            );
        }
        assert_eq!(ratios.len(), 4);
        // Deleting the letters gives the slot back to the word.
        let ratios = ratios_after_text_edit(new_text, old_text, &ratios, lang).unwrap();
        assert_eq!(ratios.len(), 3);
        assert!((ratios[1] - 0.5).abs() < 0.001, "{ratios:?}");
    }

    #[test]
    fn erasing_a_word_reserves_its_slot_and_retyping_takes_it_back() {
        let lang = "en-us";
        let text = "Well I lost my phone";
        assert_eq!(syllable_breaks(text, lang).len() + 1, 5);
        let saved = [0.3, 0.05, 0.25, 0.1, 0.3];

        // Erase "lost" (one selection, one keystroke).
        let erased = timings_after_text_edit(text, "Well I  my phone", &saved, lang, None).unwrap();
        assert_eq!(erased.ratios.len(), 4);
        let slot = erased
            .reserved
            .expect("the erased word leaves a reserved slot");
        assert_eq!(slot.after_segment, 1);
        assert!((slot.ratio - 0.25).abs() < 0.001, "{slot:?}");
        assert!(
            (erased.ratios[1] - 0.30).abs() < 0.001,
            "{:?}",
            erased.ratios
        );
        assert!(
            (erased.ratios[2] - 0.10).abs() < 0.001,
            "{:?}",
            erased.ratios
        );

        // Type "perdu" letter by letter in the gap: it takes the slot back and
        // nothing else moves.
        let mut current = "Well I  my phone".to_string();
        let mut ratios = erased.ratios.clone();
        let mut reserved = erased.reserved;
        for word in ["p", "pe", "per", "perd", "perdu"] {
            let next = format!("Well I {word} my phone");
            let timings =
                timings_after_text_edit(&current, &next, &ratios, lang, reserved).unwrap();
            ratios = timings.ratios;
            reserved = timings.reserved;
            current = next;
            let count = ratios.len();
            assert!((ratios[0] - 0.3).abs() < 0.001, "{current}: {ratios:?}");
            assert!((ratios[1] - 0.05).abs() < 0.001, "{current}: {ratios:?}");
            assert!(
                (ratios[count - 2] - 0.1).abs() < 0.001,
                "{current}: {ratios:?}"
            );
            assert!(
                (ratios[count - 1] - 0.3).abs() < 0.001,
                "{current}: {ratios:?}"
            );
            assert!(reserved.is_none(), "{current}: {reserved:?}");
        }
        assert_eq!(ratios.len(), 6);
        assert!((ratios[2] + ratios[3] - 0.25).abs() < 0.001, "{ratios:?}");

        // Without the reservation the new word is cut out of its neighbour.
        let unreserved = timings_after_text_edit(
            "Well I  my phone",
            "Well I p my phone",
            &erased.ratios,
            lang,
            None,
        )
        .unwrap();
        assert!((unreserved.ratios[1] + unreserved.ratios[2] - 0.30).abs() < 0.001);
        assert!(unreserved.ratios[2] < 0.25);
    }

    #[test]
    fn typing_a_space_on_a_reserved_slot_keeps_it() {
        let lang = "fr-fr";
        let erased =
            timings_after_text_edit("le chat dort", "le  dort", &[0.2, 0.5, 0.3], lang, None)
                .unwrap();
        let slot = erased.reserved.unwrap();
        let spaced = timings_after_text_edit(
            "le  dort",
            "le   dort",
            &erased.ratios,
            lang,
            erased.reserved,
        )
        .unwrap();
        let kept = spaced.reserved.expect("a space does not spend the slot");
        assert_eq!(kept.after_segment, slot.after_segment);
        assert!((kept.ratio - slot.ratio).abs() < 0.001);
    }

    #[test]
    fn changing_the_syllable_language_keeps_every_letter_in_place() {
        let text = "tambourine magnifique";
        let french = syllable_breaks(text, "fr-fr");
        let english = syllable_breaks(text, "en-us");
        assert_ne!(french, english);
        let mut saved = vec![1.0; french.len() + 1];
        saved[0] = 10.0;
        let saved = normalized_ratios(&saved);

        let converted = ratios_for_other_language(text, &saved, "fr-fr", "en-us").unwrap();
        assert_eq!(converted.len(), english.len() + 1);
        // A boundary both languages share keeps its time; one the new language
        // adds falls where the letter was inside the old syllable.
        let char_count = text.chars().count() as f32;
        let time_at = |breaks: &[usize], ratios: &[f32], boundary: usize| -> f32 {
            let mut time = 0.0;
            let mut start = 0usize;
            for (index, ratio) in ratios.iter().enumerate() {
                let end = breaks.get(index).copied().unwrap_or(char_count as usize);
                if boundary <= end {
                    return time + ratio * (boundary - start) as f32 / (end - start).max(1) as f32;
                }
                time += ratio;
                start = end;
            }
            1.0
        };
        for boundary in &english {
            let before = time_at(&french, &saved, *boundary);
            let after = time_at(&english, &converted, *boundary);
            assert!(
                (before - after).abs() < 0.001,
                "{boundary}: {before} vs {after}"
            );
        }
        assert!(ratios_for_other_language(text, &saved, "fr-fr", "fr-fr").is_none());
        assert!(ratios_for_other_language(text, &[0.5, 0.5], "fr-fr", "en-us").is_none());
    }

    #[test]
    fn extending_a_syllable_keeps_its_time_and_the_others() {
        let saved = [0.5, 0.2, 0.3];
        let ratios =
            ratios_after_text_edit("bonjour toi", "bonjours toi", &saved, "fr-fr").unwrap();
        assert_eq!(ratios.len(), 3);
        for (ratio, saved) in ratios.iter().zip(saved) {
            assert!((ratio - saved).abs() < 0.001, "{ratios:?}");
        }
        assert!(ratios_after_text_edit("bonjour toi", "bonjour moi", &[], "fr-fr").is_none());
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
