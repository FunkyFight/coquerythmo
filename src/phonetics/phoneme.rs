//! Internal phonetic inventory for Coquerythmo.
//!
//! One strongly typed [`Phoneme`] enum is shared by every supported language.
//! Phonemes that are acoustically equivalent across languages share a single
//! variant; language- or accent-specific realisations are distinct variants.
//! Serialization uses stable kebab-case ids (`serde rename_all`) so project
//! files remain valid across refactors.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Language handled by the phonetic engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    French,
    English,
    Spanish,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::French, Self::English, Self::Spanish];

    pub const fn code(self) -> &'static str {
        match self {
            Self::French => "fr",
            Self::English => "en",
            Self::Spanish => "es",
        }
    }

    pub const fn default_dialect(self) -> Dialect {
        match self {
            Self::French => Dialect::Generic,
            Self::English => Dialect::EnUs,
            Self::Spanish => Dialect::EsLatam,
        }
    }

    /// Regional variants selectable for this language. Order defines menu order.
    pub const fn dialects(self) -> &'static [Dialect] {
        match self {
            Self::French => &[Dialect::Generic],
            Self::English => &[Dialect::EnUs, Dialect::EnGb],
            Self::Spanish => &[Dialect::EsLatam, Dialect::EsSpain],
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "fr" | "fr-FR" | "fra" | "francais" | "français" => Some(Self::French),
            "en" | "en-US" | "en-GB" | "eng" => Some(Self::English),
            "es" | "es-ES" | "es-419" | "spa" | "espanol" | "español" => Some(Self::Spanish),
            _ => None,
        }
    }
}

/// Regional variant. `Generic` covers languages without an accent split (French).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    #[default]
    Generic,
    /// General American English.
    EnUs,
    /// Southern British English (RP-like).
    EnGb,
    /// Spain (distinción: c/z => θ).
    EsSpain,
    /// General Latin American (seseo).
    EsLatam,
}

impl Dialect {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::EnUs => "en-us",
            Self::EnGb => "en-gb",
            Self::EsSpain => "es-es",
            Self::EsLatam => "es-latam",
        }
    }
}

/// Internal phoneme. Variants intentionally carry no language prefix when the
/// sound is shared (e.g. `VoicelessBilabialPlosive` is /p/ in fr/en/es).
/// Language- or accent-specific sounds are suffixed: `*Fr`, `*EnUs`, …
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phoneme {
    // ── Shared vowels ────────────────────────────────────────────────────
    /// /a/ open central (fr "patte", es "casa").
    OpenCentral,
    /// /i/ (fr "si", en "see", es "sí").
    CloseFront,
    /// /e/ close-mid front (fr "été", es "mesa" first e).
    CloseMidFront,
    /// /ɛ/ open-mid front (fr "belle", en "bed", es "perro" first e region).
    OpenMidFront,
    /// /o/ close-mid back rounded (fr "eau", es "no").
    CloseMidBackRounded,
    /// /ɔ/ open-mid back rounded (fr "porte", en "lot" GB).
    OpenMidBackRounded,
    /// /u/ close back rounded (fr "tout", en "too", es "tú").
    CloseBackRounded,
    /// /y/ close front rounded (fr "tu").
    CloseFrontRounded,
    /// /ø/ close-mid front rounded (fr "peu").
    CloseMidFrontRounded,
    /// /œ/ open-mid front rounded (fr "sœur").
    OpenMidFrontRounded,
    /// /ə/ schwa (fr "le", en unstressed "about").
    Schwa,

    // ── French-specific ─────────────────────────────────────────────────
    /// /ɑ/ open back (fr marginal "pâte").
    OpenBackFr,
    /// /ɑ̃/ (fr "vent").
    NasalOpenBackFr,
    /// /ɛ̃/ (fr "vin").
    NasalOpenMidFrontFr,
    /// /ɔ̃/ (fr "bon").
    NasalOpenMidBackFr,
    /// /œ̃/ (fr "brun", merging with /ɛ̃/ in modern French).
    NasalOpenMidFrontRoundedFr,
    /// /ɥ/ labial-palatal approximant (fr "lui").
    LabialPalatalApproximant,

    // ── English vowels (accent-dependent) ───────────────────────────────
    /// /ɪ/ near-close near-front (en "bit").
    NearCloseFrontEn,
    /// /ʊ/ near-close near-back rounded (en "book").
    NearCloseBackRoundedEn,
    /// /ʌ/ open-mid back unrounded (en "strut").
    OpenMidBackEn,
    /// /æ/ near-open front (en "trap").
    NearOpenFrontEn,
    /// /ɒ/ open back rounded (en-GB "lot").
    OpenBackRoundedEnGb,
    /// /ɹ̩/ r-colored schwa (en-US "better" final).
    RColoredSchwaEnUs,
    /// /ɜː/ open-mid central (en-GB "nurse").
    OpenMidCentralEnGb,
    /// /ɜr/ r-colored open-mid central (en-US "nurse").
    RColoredOpenMidCentralEnUs,
    // English diphthongs.
    DiphthongFaceEn,   // /eɪ/
    DiphthongPriceEn,  // /aɪ/
    DiphthongChoiceEn, // /ɔɪ/
    DiphthongGoatEn,   // /oʊ/ US, /əʊ/ GB kept unified at mapping level
    DiphthongMouthEn,  // /aʊ/
    DiphthongNearEnGb, // /ɪə/
    DiphthongSquareEnGb, // /ɛə/
    DiphthongCureEnGb, // /ʊə/

    // ── Shared consonants ───────────────────────────────────────────────
    VoicelessBilabialPlosive, // p
    VoicedBilabialPlosive,    // b
    VoicelessAlveolarPlosive, // t
    VoicedAlveolarPlosive,    // d
    VoicelessVelarPlosive,    // k
    VoicedVelarPlosive,       // g
    BilabialNasal,            // m
    AlveolarNasal,            // n
    VoicelessLabiodentalFricative, // f
    VoicedLabiodentalFricative,    // v
    VoicelessAlveolarFricative,    // s
    VoicedAlveolarFricative,       // z
    VoicelessPostalveolarFricative, // ʃ
    VoicedPostalveolarFricative,    // ʒ
    VoicelessGlottalFricative,      // h
    AlveolarLateralApproximant,     // l
    PalatalApproximant,             // j
    LabialVelarApproximant,         // w
    VoicedUvularFricative,          // ʁ (standard French r)

    // ── French-specific consonants ──────────────────────────────────────
    PalatalNasal, // /ɲ/ (fr "agneau", es "niño")

    // ── English consonants ──────────────────────────────────────────────
    VoicelessDentalFricative,  // θ (en "think")
    VoicedDentalFricative,     // ð (en "this")
    VoicelessPostalveolarAffricate, // tʃ (en "church", es "mucho")
    VoicedPostalveolarAffricate,    // dʒ (en "judge")
    VelarNasal,                // ŋ (en "sing")
    AlveolarApproximant,       // ɹ (en-US/GB r onset)
    SyllabicL,                 // l̩ (en "bottle")
    SyllabicN,                 // n̩ (en "button")
    SyllabicM,                 // m̩ (en "rhythm")
    GlottalStop,               // ʔ
    VoicelessLabialVelarFricative, // ʍ (en "whine", conservative)

    // ── Spanish consonants ──────────────────────────────────────────────
    VoicelessDentalFricativeEsSpain, // θ (es-ES "zapato", "cena")
    VoicelessVelarFricative,         // x (es "jamón", "gente")
    AlveolarTap,                     // ɾ (es "pero")
    AlveolarTrill,                   // r (es "perro", initial "rojo")
    VoicedPalatalFricative,          // ʝ (es "yo", "llama" most dialects)
    VoicedPostalveolarFricativeEs,   // ʒ (es "yo/ll" Rioplatense/sheísmo)
    VoicelessPostalveolarFricativeEs, // ʃ (es "yo/ll" zheísmo)
    VoicedBilabialApproximant,       // β̞ (es intervocalic b/v)
    VoicedDentalApproximant,         // ð̞ (es intervocalic d)
    VoicedVelarApproximant,          // ɣ̞ (es intervocalic g)
    VoicelessAlveolarAffricate,      // ts (es-ES "ch"→tʃ handled as affricate variant; Mexican "tl")

    // ── Spanish vowels (hiatus/diphthong glides reuse shared set) ───────
    // Spanish uses CloseFront/CloseMidFront/OpenCentral/CloseMidBackRounded/
    // CloseBackRounded plus glides PalatalApproximant/LabialVelarApproximant.
}

impl Phoneme {
    /// Every declared phoneme. Tests iterate this list to assert exhaustive
    /// mapping coverage.
    pub const ALL: &'static [Phoneme] = &[
        Phoneme::OpenCentral,
        Phoneme::CloseFront,
        Phoneme::CloseMidFront,
        Phoneme::OpenMidFront,
        Phoneme::CloseMidBackRounded,
        Phoneme::OpenMidBackRounded,
        Phoneme::CloseBackRounded,
        Phoneme::CloseFrontRounded,
        Phoneme::CloseMidFrontRounded,
        Phoneme::OpenMidFrontRounded,
        Phoneme::Schwa,
        Phoneme::OpenBackFr,
        Phoneme::NasalOpenBackFr,
        Phoneme::NasalOpenMidFrontFr,
        Phoneme::NasalOpenMidBackFr,
        Phoneme::NasalOpenMidFrontRoundedFr,
        Phoneme::LabialPalatalApproximant,
        Phoneme::NearCloseFrontEn,
        Phoneme::NearCloseBackRoundedEn,
        Phoneme::OpenMidBackEn,
        Phoneme::NearOpenFrontEn,
        Phoneme::OpenBackRoundedEnGb,
        Phoneme::RColoredSchwaEnUs,
        Phoneme::OpenMidCentralEnGb,
        Phoneme::RColoredOpenMidCentralEnUs,
        Phoneme::DiphthongFaceEn,
        Phoneme::DiphthongPriceEn,
        Phoneme::DiphthongChoiceEn,
        Phoneme::DiphthongGoatEn,
        Phoneme::DiphthongMouthEn,
        Phoneme::DiphthongNearEnGb,
        Phoneme::DiphthongSquareEnGb,
        Phoneme::DiphthongCureEnGb,
        Phoneme::VoicelessBilabialPlosive,
        Phoneme::VoicedBilabialPlosive,
        Phoneme::VoicelessAlveolarPlosive,
        Phoneme::VoicedAlveolarPlosive,
        Phoneme::VoicelessVelarPlosive,
        Phoneme::VoicedVelarPlosive,
        Phoneme::BilabialNasal,
        Phoneme::AlveolarNasal,
        Phoneme::VoicelessLabiodentalFricative,
        Phoneme::VoicedLabiodentalFricative,
        Phoneme::VoicelessAlveolarFricative,
        Phoneme::VoicedAlveolarFricative,
        Phoneme::VoicelessPostalveolarFricative,
        Phoneme::VoicedPostalveolarFricative,
        Phoneme::VoicelessGlottalFricative,
        Phoneme::AlveolarLateralApproximant,
        Phoneme::PalatalApproximant,
        Phoneme::LabialVelarApproximant,
        Phoneme::VoicedUvularFricative,
        Phoneme::PalatalNasal,
        Phoneme::VoicelessDentalFricative,
        Phoneme::VoicedDentalFricative,
        Phoneme::VoicelessPostalveolarAffricate,
        Phoneme::VoicedPostalveolarAffricate,
        Phoneme::VelarNasal,
        Phoneme::AlveolarApproximant,
        Phoneme::SyllabicL,
        Phoneme::SyllabicN,
        Phoneme::SyllabicM,
        Phoneme::GlottalStop,
        Phoneme::VoicelessLabialVelarFricative,
        Phoneme::VoicelessDentalFricativeEsSpain,
        Phoneme::VoicelessVelarFricative,
        Phoneme::AlveolarTap,
        Phoneme::AlveolarTrill,
        Phoneme::VoicedPalatalFricative,
        Phoneme::VoicedPostalveolarFricativeEs,
        Phoneme::VoicelessPostalveolarFricativeEs,
        Phoneme::VoicedBilabialApproximant,
        Phoneme::VoicedDentalApproximant,
        Phoneme::VoicedVelarApproximant,
        Phoneme::VoicelessAlveolarAffricate,
    ];

    /// IPA symbol used in previews and documentation.
    pub const fn ipa(self) -> &'static str {
        use Phoneme::*;
        match self {
            OpenCentral => "a",
            CloseFront => "i",
            CloseMidFront => "e",
            OpenMidFront => "ɛ",
            CloseMidBackRounded => "o",
            OpenMidBackRounded => "ɔ",
            CloseBackRounded => "u",
            CloseFrontRounded => "y",
            CloseMidFrontRounded => "ø",
            OpenMidFrontRounded => "œ",
            Schwa => "ə",
            OpenBackFr => "ɑ",
            NasalOpenBackFr => "ɑ̃",
            NasalOpenMidFrontFr => "ɛ̃",
            NasalOpenMidBackFr => "ɔ̃",
            NasalOpenMidFrontRoundedFr => "œ̃",
            LabialPalatalApproximant => "ɥ",
            NearCloseFrontEn => "ɪ",
            NearCloseBackRoundedEn => "ʊ",
            OpenMidBackEn => "ʌ",
            NearOpenFrontEn => "æ",
            OpenBackRoundedEnGb => "ɒ",
            RColoredSchwaEnUs => "ɚ",
            OpenMidCentralEnGb => "ɜː",
            RColoredOpenMidCentralEnUs => "ɜr",
            DiphthongFaceEn => "eɪ",
            DiphthongPriceEn => "aɪ",
            DiphthongChoiceEn => "ɔɪ",
            DiphthongGoatEn => "oʊ",
            DiphthongMouthEn => "aʊ",
            DiphthongNearEnGb => "ɪə",
            DiphthongSquareEnGb => "ɛə",
            DiphthongCureEnGb => "ʊə",
            VoicelessBilabialPlosive => "p",
            VoicedBilabialPlosive => "b",
            VoicelessAlveolarPlosive => "t",
            VoicedAlveolarPlosive => "d",
            VoicelessVelarPlosive => "k",
            VoicedVelarPlosive => "g",
            BilabialNasal => "m",
            AlveolarNasal => "n",
            VoicelessLabiodentalFricative => "f",
            VoicedLabiodentalFricative => "v",
            VoicelessAlveolarFricative => "s",
            VoicedAlveolarFricative => "z",
            VoicelessPostalveolarFricative => "ʃ",
            VoicedPostalveolarFricative => "ʒ",
            VoicelessGlottalFricative => "h",
            AlveolarLateralApproximant => "l",
            PalatalApproximant => "j",
            LabialVelarApproximant => "w",
            VoicedUvularFricative => "ʁ",
            PalatalNasal => "ɲ",
            VoicelessDentalFricative => "θ",
            VoicedDentalFricative => "ð",
            VoicelessPostalveolarAffricate => "tʃ",
            VoicedPostalveolarAffricate => "dʒ",
            VelarNasal => "ŋ",
            AlveolarApproximant => "ɹ",
            SyllabicL => "l̩",
            SyllabicN => "n̩",
            SyllabicM => "m̩",
            GlottalStop => "ʔ",
            VoicelessLabialVelarFricative => "ʍ",
            VoicelessDentalFricativeEsSpain => "θ",
            VoicelessVelarFricative => "x",
            AlveolarTap => "ɾ",
            AlveolarTrill => "r",
            VoicedPalatalFricative => "ʝ",
            VoicedPostalveolarFricativeEs => "ʒ",
            VoicelessPostalveolarFricativeEs => "ʃ",
            VoicedBilabialApproximant => "β̞",
            VoicedDentalApproximant => "ð̞",
            VoicedVelarApproximant => "ɣ̞",
            VoicelessAlveolarAffricate => "ts",
        }
    }

    /// Short human description (used by generated docs and previews).
    pub const fn description(self) -> &'static str {
        use Phoneme::*;
        match self {
            OpenCentral => "voyelle ouverte centrale (a)",
            CloseFront => "voyelle fermée antérieure (i)",
            CloseMidFront => "voyelle mi-fermée antérieure (é)",
            OpenMidFront => "voyelle mi-ouverte antérieure (è)",
            CloseMidBackRounded => "voyelle mi-fermée postérieure arrondie (o)",
            OpenMidBackRounded => "voyelle mi-ouverte postérieure arrondie (ô ouvert)",
            CloseBackRounded => "voyelle fermée postérieure arrondie (ou)",
            CloseFrontRounded => "voyelle fermée antérieure arrondie (u)",
            CloseMidFrontRounded => "voyelle mi-fermée antérieure arrondie (eu fermé)",
            OpenMidFrontRounded => "voyelle mi-ouverte antérieure arrondie (eu ouvert)",
            Schwa => "schwa / e muet",
            OpenBackFr => "a postérieur (pâte)",
            NasalOpenBackFr => "voyelle nasale an/en",
            NasalOpenMidFrontFr => "voyelle nasale in/ain",
            NasalOpenMidBackFr => "voyelle nasale on",
            NasalOpenMidFrontRoundedFr => "voyelle nasale un",
            LabialPalatalApproximant => "semi-voyelle ɥ (lui)",
            NearCloseFrontEn => "ɪ anglais (bit)",
            NearCloseBackRoundedEn => "ʊ anglais (book)",
            OpenMidBackEn => "ʌ anglais (strut)",
            NearOpenFrontEn => "æ anglais (trap)",
            OpenBackRoundedEnGb => "ɒ anglais britannique (lot)",
            RColoredSchwaEnUs => "ɚ anglais américain (better)",
            OpenMidCentralEnGb => "ɜː anglais britannique (nurse)",
            RColoredOpenMidCentralEnUs => "ɜr anglais américain (nurse)",
            DiphthongFaceEn => "diphtongue eɪ (face)",
            DiphthongPriceEn => "diphtongue aɪ (price)",
            DiphthongChoiceEn => "diphtongue ɔɪ (choice)",
            DiphthongGoatEn => "diphtongue oʊ/əʊ (goat)",
            DiphthongMouthEn => "diphtongue aʊ (mouth)",
            DiphthongNearEnGb => "diphtongue ɪə (near, GB)",
            DiphthongSquareEnGb => "diphtongue ɛə (square, GB)",
            DiphthongCureEnGb => "diphtongue ʊə (cure, GB)",
            VoicelessBilabialPlosive => "p sourd",
            VoicedBilabialPlosive => "b sonore",
            VoicelessAlveolarPlosive => "t sourd",
            VoicedAlveolarPlosive => "d sonore",
            VoicelessVelarPlosive => "k sourd",
            VoicedVelarPlosive => "g sonore",
            BilabialNasal => "m",
            AlveolarNasal => "n",
            VoicelessLabiodentalFricative => "f",
            VoicedLabiodentalFricative => "v",
            VoicelessAlveolarFricative => "s sourd",
            VoicedAlveolarFricative => "z sonore (fr)/s sonorisé",
            VoicelessPostalveolarFricative => "ʃ (ch/tion)",
            VoicedPostalveolarFricative => "ʒ (j/ge)",
            VoicelessGlottalFricative => "h (anglais)",
            AlveolarLateralApproximant => "l",
            PalatalApproximant => "j (y/ill/i+ voyelle)",
            LabialVelarApproximant => "w (oi/watt)",
            VoicedUvularFricative => "r français (ʁ)",
            PalatalNasal => "ɲ (gn/ñ)",
            VoicelessDentalFricative => "θ anglais (think)",
            VoicedDentalFricative => "ð anglais (this)",
            VoicelessPostalveolarAffricate => "tʃ (church/mucho)",
            VoicedPostalveolarAffricate => "dʒ (judge)",
            VelarNasal => "ŋ (sing/parking)",
            AlveolarApproximant => "ɹ anglais (red)",
            SyllabicL => "l syllabique (bottle)",
            SyllabicN => "n syllabique (button)",
            SyllabicM => "m syllabique (rhythm)",
            GlottalStop => "coup de glotte",
            VoicelessLabialVelarFricative => "ʍ (wh conservateur)",
            VoicelessDentalFricativeEsSpain => "θ espanol d'Espagne (z/ce/ci)",
            VoicelessVelarFricative => "x espagnol (j/ge/gi)",
            AlveolarTap => "r battu (pero)",
            AlveolarTrill => "r roulé (perro/rojo)",
            VoicedPalatalFricative => "ʝ espagnol (yo/ll)",
            VoicedPostalveolarFricativeEs => "ʒ rioplatense (yo/ll)",
            VoicelessPostalveolarFricativeEs => "ʃ zheísmo (yo/ll)",
            VoicedBilabialApproximant => "β̞ espagnol (b/v intervocalique)",
            VoicedDentalApproximant => "ð̞ espagnol (d intervocalique)",
            VoicedVelarApproximant => "ɣ̞ espagnol (g intervocalique)",
            VoicelessAlveolarAffricate => "ts affriquée",
        }
    }
}

impl fmt::Display for Phoneme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ipa())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phoneme_has_ipa_and_description() {
        for phoneme in Phoneme::ALL {
            assert!(!phoneme.ipa().is_empty(), "{phoneme:?} missing ipa");
            assert!(
                !phoneme.description().is_empty(),
                "{phoneme:?} missing description"
            );
        }
    }

    #[test]
    fn phoneme_serde_ids_are_stable_kebab_case() {
        let json = serde_json::to_string(&Phoneme::VoicelessPostalveolarFricative).unwrap();
        assert_eq!(json, "\"voiceless-postalveolar-fricative\"");
        let back: Phoneme = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Phoneme::VoicelessPostalveolarFricative);
    }

    #[test]
    fn inventory_has_no_duplicates() {
        let mut sorted = Phoneme::ALL.to_vec();
        sorted.sort();
        let len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), len, "duplicate phoneme in ALL");
    }
}
