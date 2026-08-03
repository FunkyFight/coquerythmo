//! English grapheme→phoneme conversion, with American (GA) and British
//! (SSB/RP-like) variants.

pub mod data;

use crate::phonetics::g2p::rules::GraphemeRule;
use crate::phonetics::g2p::{G2PEngine, GraphemeToPhonemeConverter};
use crate::phonetics::phoneme::{Dialect, Language, Phoneme};
use crate::phonetics::types::*;

pub struct EnglishG2P {
    engine: G2PEngine,
}

impl Default for EnglishG2P {
    fn default() -> Self {
        Self::new(Dialect::EnUs)
    }
}

impl EnglishG2P {
    pub fn new(dialect: Dialect) -> Self {
        let rules: Vec<GraphemeRule> = data::RULES
            .iter()
            .copied()
            .filter(|rule| rule.applies_to(dialect))
            .collect();
        Self {
            engine: G2PEngine::new(
                Language::English,
                dialect,
                data::DICTIONARY,
                data::EXCEPTIONS,
                rules,
                data::PRONOUNCEABLE_ACRONYMS,
                post_process,
            ),
        }
    }

    pub fn engine(&self) -> &G2PEngine {
        &self.engine
    }
}

/// Language-specific fix-ups after rule application:
/// 1. "magic e": a final silent `e` upgrades the preceding vowel from its
///    lax checked value to its long/diphthong value (bit→bite, hat→hate,
///    not→note, cut→cute, pet→Pete).
/// 2. GB: non-rhoticity — drop coda `r` phonemes unless followed by a vowel.
fn post_process(dialect: Dialect, mut segments: Vec<GraphemeSegment>) -> Vec<GraphemeSegment> {
    apply_magic_e(&mut segments);
    if dialect == Dialect::EnGb {
        apply_non_rhoticity(&mut segments);
    }
    segments
}

fn lax_to_long(phoneme: Phoneme) -> Option<Phoneme> {
    use Phoneme::*;
    Some(match phoneme {
        NearOpenFrontEn => DiphthongFaceEn,   // æ→eɪ
        NearCloseFrontEn => DiphthongPriceEn, // ɪ→aɪ
        OpenMidFront => CloseFront,           // e→iː (mete)
        OpenMidBackEn => CloseBackRounded,    // ʌ→uː (cute)
        OpenMidBackRounded | OpenBackRoundedEnGb => DiphthongGoatEn, // ɔ/ɒ→oʊ
        NearCloseBackRoundedEn => OpenBackRoundedEnGb, // prominent "lure" shift avoided
        _ => return None,
    })
}

fn apply_magic_e(segments: &mut Vec<GraphemeSegment>) {
    // A silent single-letter "e" at the end with a consonant before it and a
    // vowel two slots back upgrades that vowel.
    let len = segments.len();
    if len < 3 {
        return;
    }
    let last_is_silent_e = segments[len - 1].grapheme == "e"
        && segments[len - 1].phonemes.is_empty()
        && segments[len - 1].silent;
    if !last_is_silent_e {
        return;
    }
    // Back up past the consonant cluster (usually one grapheme).
    let mut vowel_idx = None;
    for back in (0..len - 1).rev() {
        let segment = &segments[back];
        if segment.grapheme == "qu"
            || segment
                .grapheme
                .chars()
                .all(|c| "bcçdfghjklmnpqrstvwxz".contains(c))
        {
            continue; // skip consonant group
        }
        vowel_idx = Some(back);
        break;
    }
    if let Some(index) = vowel_idx {
        let target = &mut segments[index];
        if let Some(first) = target.phonemes.first().map(|occ| occ.phoneme) {
            if let Some(long) = lax_to_long(first) {
                for occ in &mut target.phonemes {
                    occ.phoneme = long;
                }
            }
        }
    }
}

fn apply_non_rhoticity(segments: &mut Vec<GraphemeSegment>) {
    // Drop AlveolarApproximant in coda position (not followed by a vowel
    // phoneme within the word). Linking r across words is out of scope here.
    for i in 0..segments.len() {
        let is_r = segments[i]
            .phonemes
            .first()
            .is_some_and(|occ| occ.phoneme == Phoneme::AlveolarApproximant);
        if !is_r {
            continue;
        }
        let followed_by_vowel = segments[i + 1..]
            .iter()
            .flat_map(|s| s.phonemes.iter())
            .next()
            .is_some_and(|occ| is_vowel_phoneme(occ.phoneme));
        if !followed_by_vowel {
            segments[i].phonemes.clear();
            segments[i].silent = true;
        }
    }
}

fn is_vowel_phoneme(phoneme: Phoneme) -> bool {
    use Phoneme::*;
    matches!(
        phoneme,
        OpenCentral
            | CloseFront
            | CloseMidFront
            | OpenMidFront
            | CloseMidBackRounded
            | OpenMidBackRounded
            | CloseBackRounded
            | CloseFrontRounded
            | CloseMidFrontRounded
            | OpenMidFrontRounded
            | Schwa
            | NearCloseFrontEn
            | NearCloseBackRoundedEn
            | OpenMidBackEn
            | NearOpenFrontEn
            | OpenBackRoundedEnGb
            | OpenMidCentralEnGb
            | DiphthongFaceEn
            | DiphthongPriceEn
            | DiphthongChoiceEn
            | DiphthongGoatEn
            | DiphthongMouthEn
            | DiphthongNearEnGb
            | DiphthongSquareEnGb
            | DiphthongCureEnGb
    )
}

impl GraphemeToPhonemeConverter for EnglishG2P {
    fn language(&self) -> Language {
        Language::English
    }
    fn convert(&self, text: &str) -> PhoneticLine {
        crate::phonetics::g2p::convert_line(&self.engine, text)
    }
    fn dialect(&self) -> Dialect {
        self.engine.dialect
    }
    fn convert_word(&self, word: &str) -> Vec<PronunciationCandidate> {
        self.engine.resolve_word(word)
    }
}
