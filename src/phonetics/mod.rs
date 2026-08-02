//! Text → phonemes → detection signs engine.
//!
//! Pipeline :
//! ```text
//! texte affiché
//! → normalisation (avec carte inverse vers le texte original)
//! → tokenisation (mots, nombres, acronymes, ponctuation)
//! → conversion graphème→phonème par langue (dictionnaire + règles + exceptions)
//! → mapping phonème→signe de détection (configurable)
//! → placement aux lettres (jamais à l'audio)
//! ```
//!
//! Aucune forme d'alignement audio (ASR, forced alignment) n'est utilisée.

pub mod english;
pub mod french;
pub mod g2p;
pub mod inventory_check;
pub mod mapping;
pub mod normalize;
pub mod phoneme;
pub mod sign_generation;
pub mod spanish;
pub mod types;

pub use english::EnglishG2P;
pub use french::FrenchG2P;
pub use g2p::{GraphemeToPhonemeConverter, G2PEngine};
pub use mapping::{DetectionMapping, PhonemeSignRule, SignMappingProfile};
pub use normalize::{normalize_line, NormalizedChar, NormalizedLine};
pub use phoneme::{Dialect, Language, Phoneme};
pub use spanish::SpanishG2P;
pub use types::*;

/// Version of the linguistic engine + embedded data. Bumped whenever rules,
/// dictionaries or the mapping change so stale generated signs can be
/// detected at load time.
pub const ENGINE_VERSION: u32 = 1;

/// Quick FNV-1a fingerprint used to detect that the source text of a line
/// changed after signs were generated.
pub fn text_fingerprint(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Build the converter for a language with its default dialect. Combines the
/// embedded dictionary, exceptions, contextual rules and fallback.
pub fn converter_for(language: Language) -> Box<dyn GraphemeToPhonemeConverter> {
    converter_for_dialect(language, language.default_dialect())
}

/// Build the converter for a language + regional variant.
pub fn converter_for_dialect(
    language: Language,
    dialect: Dialect,
) -> Box<dyn GraphemeToPhonemeConverter> {
    match language {
        Language::French => Box::new(FrenchG2P::new(dialect)),
        Language::English => Box::new(EnglishG2P::new(dialect)),
        Language::Spanish => Box::new(SpanishG2P::new(dialect)),
    }
}
