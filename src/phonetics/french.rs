//! French grapheme→phoneme data. See `docs/phonemes_fr.md` (generated) for
//! the human-readable inventory; this module is the single source of truth.

pub mod data;

use crate::phonetics::g2p::rules::GraphemeRule;
use crate::phonetics::g2p::{G2PEngine, GraphemeToPhonemeConverter};
use crate::phonetics::phoneme::{Dialect, Language};
use crate::phonetics::types::*;

pub struct FrenchG2P {
    engine: G2PEngine,
}

impl Default for FrenchG2P {
    fn default() -> Self {
        Self::new(Dialect::Generic)
    }
}

fn identity(_dialect: Dialect, segments: Vec<GraphemeSegment>) -> Vec<GraphemeSegment> {
    segments
}

impl FrenchG2P {
    pub fn new(dialect: Dialect) -> Self {
        let rules: Vec<GraphemeRule> = data::RULES
            .iter()
            .copied()
            .filter(|rule| rule.applies_to(dialect))
            .collect();
        Self {
            engine: G2PEngine::new(
                Language::French,
                dialect,
                data::DICTIONARY,
                data::EXCEPTIONS,
                rules,
                data::PRONOUNCEABLE_ACRONYMS,
                identity,
            ),
        }
    }

    pub fn engine(&self) -> &G2PEngine {
        &self.engine
    }
}

impl GraphemeToPhonemeConverter for FrenchG2P {
    fn language(&self) -> Language {
        Language::French
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


