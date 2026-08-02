//! Conversor español: reglas grafema→fonema y diccionarios.

pub mod data;

use crate::phonetics::g2p::rules::GraphemeRule;
use crate::phonetics::g2p::{G2PEngine, GraphemeToPhonemeConverter};
use crate::phonetics::phoneme::{Dialect, Language};
use crate::phonetics::types::*;

pub struct SpanishG2P {
    engine: G2PEngine,
}

impl Default for SpanishG2P {
    fn default() -> Self {
        Self::new(Dialect::EsLatam)
    }
}

impl SpanishG2P {
    pub fn new(dialect: Dialect) -> Self {
        let rules: Vec<GraphemeRule> = data::RULES
            .iter()
            .copied()
            .filter(|rule| rule.applies_to(dialect))
            .collect();
        Self {
            engine: G2PEngine::new(
                Language::Spanish,
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

fn identity(_dialect: Dialect, segments: Vec<GraphemeSegment>) -> Vec<GraphemeSegment> {
    segments
}

impl GraphemeToPhonemeConverter for SpanishG2P {
    fn language(&self) -> Language {
        Language::Spanish
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
