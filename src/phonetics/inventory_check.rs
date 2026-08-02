//! Compile/load-time validation of linguistic data and generation of the
//! coverage report. Called by tests; the report is what powers the generated
//! documentation.

use crate::phonetics::g2p::rules::{Dialects, DictTable};
use crate::phonetics::mapping::{default_mapping, validate_mapping};
use crate::phonetics::phoneme::{Dialect, Language, Phoneme};
use crate::phonetics::converter_for_dialect;
use std::collections::{BTreeMap, BTreeSet};

/// Per-language coverage statistics.
#[derive(Clone, Debug, Default)]
pub struct CoverageReport {
    pub language: &'static str,
    pub phonemes_declared: usize,
    pub phonemes_mapped: usize,
    pub phonemes_explicitly_silent: usize,
    pub grapheme_rules: usize,
    pub dictionary_words: usize,
    pub exception_words: usize,
    pub phonemes_without_test: Vec<Phoneme>,
    pub rules_without_test: Vec<&'static str>,
}

/// For every supported language, check its mapping covers the whole phoneme
/// inventory and run a smoke conversion. Test builds call this and fail on
/// any error.
pub fn check_language(language: Language) -> Result<(), String> {
    let mapping = default_mapping(language);
    validate_mapping(&mapping).map_err(|errors| {
        format!(
            "{}: {} erreurs de mapping: {:?}",
            language.code(),
            errors.len(),
            errors
        )
    })?;

    // Smoke test: a basic phrase converts without unknown letters.
    for dialect in language.dialects() {
        let converter = converter_for_dialect(language, *dialect);
        let sample = sample_sentence(language);
        let line = converter.convert(sample);
        let unknowns: Vec<_> = line
            .tokens
            .iter()
            .filter(|t| t.unknown)
            .map(|t| t.text.clone())
            .collect();
        if !unknowns.is_empty() {
            return Err(format!(
                "{}({}): mots inconnus dans la phrase de test: {:?}",
                language.code(),
                dialect.code(),
                unknowns
            ));
        }
    }
    Ok(())
}

fn sample_sentence(language: Language) -> &'static str {
    match language {
        Language::French => "Bonjour, le chat mange une pomme !",
        Language::English => "Hello, the quick brown fox jumps over the lazy dog.",
        Language::Spanish => "Hola, el perro come una manzana y bebe agua.",
    }
}

/// Build the full coverage report used by docs and tests.
pub fn build_report(language: Language) -> CoverageReport {
    let mapping = default_mapping(language);
    let inventory = Phoneme::ALL;
    let mut mapped = BTreeSet::new();
    let mut silenced = BTreeSet::new();
    for rule in &mapping.rules {
        for phoneme in &rule.phonemes {
            if rule.signs.is_empty() {
                silenced.insert(*phoneme);
            } else {
                mapped.insert(*phoneme);
            }
        }
    }
    let _ = (mapped.len(), silenced.len());

    // Smoke: probe each phoneme by converting crafted probe words that should
    // contain it (curated probe list below).
    let mut missing_test: Vec<Phoneme> = Vec::new();
    for &phoneme in inventory {
        if probe_words(language, phoneme).is_none() {
            missing_test.push(phoneme);
        }
    }

    CoverageReport {
        language: language.code(),
        phonemes_declared: inventory.len(),
        phonemes_mapped: mapped.len(),
        phonemes_explicitly_silent: silenced.len(),
        grapheme_rules: rule_count(language),
        dictionary_words: dict_len(language, true),
        exception_words: dict_len(language, false),
        phonemes_without_test: missing_test,
        rules_without_test: Vec::new(),
    }
}

fn rule_count(language: Language) -> usize {
    match language {
        Language::French => crate::phonetics::french::data::RULES.len(),
        Language::English => crate::phonetics::english::data::RULES.len(),
        Language::Spanish => crate::phonetics::spanish::data::RULES.len(),
    }
}

fn dict_len(language: Language, main: bool) -> usize {
    let table: DictTable = match (language, main) {
        (Language::French, true) => crate::phonetics::french::data::DICTIONARY,
        (Language::French, false) => crate::phonetics::french::data::EXCEPTIONS,
        (Language::English, true) => crate::phonetics::english::data::DICTIONARY,
        (Language::English, false) => crate::phonetics::english::data::EXCEPTIONS,
        (Language::Spanish, true) => crate::phonetics::spanish::data::DICTIONARY,
        (Language::Spanish, false) => crate::phonetics::spanish::data::EXCEPTIONS,
    };
    table.len()
}

/// Curated probe words per phoneme: if a word exists, tests verify the
/// phoneme appears in its conversion. Popupated exhaustively in tests.
fn probe_words(language: Language, phoneme: Phoneme) -> Option<&'static str> {
    use Language::*;
    use Phoneme::*;
    Some(match (language, phoneme) {
        (French, VoicelessPostalveolarFricative) => "chat",
        (French, OpenCentral) => "chat",
        (French, AlveolarNasal) => "nous",
        (French, CloseBackRounded) => "nous",
        (French, VoicedPostalveolarFricative) => "jour",
        (French, VoicelessBilabialPlosive) => "papa",
        (French, VoicedBilabialPlosive) => "bébé",
        (French, BilabialNasal) => "maman",
        (French, VoicelessLabiodentalFricative) => "fou",
        (French, VoicedLabiodentalFricative) => "vous",
        (French, VoicelessAlveolarFricative) => "soleil",
        (French, VoicedAlveolarFricative) => "rose",
        (French, VoicelessAlveolarPlosive) => "table",
        (French, VoicedAlveolarPlosive) => "donne",
        (French, VoicelessVelarPlosive) => "cou",
        (French, VoicedVelarPlosive) => "gare",
        (French, VoicedUvularFricative) => "rue",
        (French, AlveolarLateralApproximant) => "lune",
        (French, PalatalApproximant) => "yeux",
        (French, PalatalNasal) => "agneau",
        (French, LabialVelarApproximant) => "moi",
        (French, LabialPalatalApproximant) => "lui",
        (French, CloseFront) => "lit",
        (French, CloseMidFront) => "été",
        (French, OpenMidFront) => "belle",
        (French, CloseFrontRounded) => "tu",
        (French, CloseMidFrontRounded) => "peu",
        (French, OpenMidFrontRounded) => "peur",
        (French, Schwa) => "le",
        (French, CloseMidBackRounded) => "eau",
        (French, OpenMidBackRounded) => "porte",
        (French, NasalOpenBackFr) => "vent",
        (French, NasalOpenMidFrontFr) => "vin",
        (French, NasalOpenMidBackFr) => "bon",
        (French, NasalOpenMidFrontRoundedFr) => "brun",
        (French, OpenBackFr) => "pâte",

        (English, VoicelessDentalFricative) => "think",
        (English, VoicedDentalFricative) => "this",
        (English, CloseFront) => "see",
        (English, NearCloseFrontEn) => "bit",
        (English, OpenMidFront) => "bed",
        (English, NearOpenFrontEn) => "cat",
        (English, OpenCentral) => "palm",
        (English, OpenMidBackEn) => "strut",
        (English, CloseBackRounded) => "too",
        (English, NearCloseBackRoundedEn) => "book",
        (English, OpenBackRoundedEnGb) => "lot",
        (English, CloseMidBackRounded) => "thought",
        (English, DiphthongFaceEn) => "face",
        (English, DiphthongPriceEn) => "price",
        (English, DiphthongChoiceEn) => "choice",
        (English, DiphthongGoatEn) => "goat",
        (English, DiphthongMouthEn) => "mouth",
        (English, DiphthongNearEnGb) => "near",
        (English, DiphthongSquareEnGb) => "square",
        (English, DiphthongCureEnGb) => "cure",
        (English, Schwa) => "about",
        (English, RColoredSchwaEnUs) => "better",
        (English, RColoredOpenMidCentralEnUs) => "nurse",
        (English, OpenMidCentralEnGb) => "nurse",
        (English, VoicelessBilabialPlosive) => "pen",
        (English, VoicedBilabialPlosive) => "bad",
        (English, VoicelessAlveolarPlosive) => "ten",
        (English, VoicedAlveolarPlosive) => "dog",
        (English, VoicelessVelarPlosive) => "key",
        (English, VoicedVelarPlosive) => "go",
        (English, VoicelessPostalveolarAffricate) => "church",
        (English, VoicedPostalveolarAffricate) => "judge",
        (English, VoicelessLabiodentalFricative) => "fool",
        (English, VoicedLabiodentalFricative) => "voice",
        (English, VoicelessAlveolarFricative) => "see",
        (English, VoicedAlveolarFricative) => "zoo",
        (English, VoicelessPostalveolarFricative) => "she",
        (English, VoicedPostalveolarFricative) => "measure",
        (English, VoicelessGlottalFricative) => "hat",
        (English, BilabialNasal) => "man",
        (English, AlveolarNasal) => "no",
        (English, VelarNasal) => "sing",
        (English, AlveolarLateralApproximant) => "leg",
        (English, AlveolarApproximant) => "red",
        (English, LabialVelarApproximant) => "we",
        (English, PalatalApproximant) => "yes",
        (English, SyllabicL) => "bottle",
        (English, SyllabicN) => "button",
        (English, SyllabicM) => "rhythm",
        (English, GlottalStop) => "uh-oh",
        (English, VoicelessLabialVelarFricative) => "why",

        (Spanish, AlveolarTap) => "pero",
        (Spanish, AlveolarTrill) => "perro",
        (Spanish, VoicelessVelarFricative) => "jamón",
        (Spanish, VoicelessDentalFricativeEsSpain) => "zapato",
        (Spanish, VoicelessPostalveolarAffricate) => "mucho",
        (Spanish, VoicedPalatalFricative) => "yo",
        (Spanish, PalatalNasal) => "niño",
        (Spanish, OpenCentral) => "casa",
        (Spanish, CloseMidFront) => "mesa",
        (Spanish, CloseFront) => "sí",
        (Spanish, CloseMidBackRounded) => "no",
        (Spanish, CloseBackRounded) => "tú",
        (Spanish, VoicelessBilabialPlosive) => "padre",
        (Spanish, VoicedBilabialPlosive) => "bomba",
        (Spanish, VoicedBilabialApproximant) => "sabe",
        (Spanish, VoicelessAlveolarPlosive) => "toda",
        (Spanish, VoicedAlveolarPlosive) => "donde",
        (Spanish, VoicedDentalApproximant) => "nada",
        (Spanish, VoicelessVelarPlosive) => "cosa",
        (Spanish, VoicedVelarPlosive) => "gato",
        (Spanish, VoicedVelarApproximant) => "lago",
        (Spanish, VoicelessAlveolarFricative) => "casa",
        (Spanish, VoicelessLabiodentalFricative) => "foco",
        (Spanish, AlveolarLateralApproximant) => "lado",
        (Spanish, BilabialNasal) => "mano",
        (Spanish, AlveolarNasal) => "nada",
        (Spanish, PalatalApproximant) => "aire",
        (Spanish, LabialVelarApproximant) => "agua",
        _ => return None,
    })
}

/// Verify every phoneme produced by each language probe actually appears in
/// the conversion of its probe word. Called from tests.
pub fn verify_probe(language: Language, phoneme: Phoneme) -> Result<bool, String> {
    let Some(word) = probe_words(language, phoneme) else {
        return Ok(false); // no probe: reported as "without test"
    };
    let dialect = match language {
        Language::English => Dialect::EnUs,
        Language::Spanish => Dialect::EsSpain,
        Language::French => Dialect::Generic,
    };
    let converter = converter_for_dialect(language, dialect);
    let line = converter.convert(word);
    let found = line
        .tokens
        .iter()
        .flat_map(|token| token.candidates.iter())
        .flat_map(|candidate| candidate.phonemes())
        .any(|occurrence| occurrence.phoneme == phoneme);
    if found {
        Ok(true)
    } else {
        Err(format!(
            "{}: le mot sonde '{}' ne produit pas le phonème {}",
            language.code(),
            word,
            phoneme.ipa()
        ))
    }
}

/// Extra sanity: rule tables reference only declared phonemes and never an
/// empty grapheme; dictionaries cover their keys exactly; dictionary
/// graphemes branding is checked in the per-language test modules.
pub fn validate_rules_table(
    table: &[crate::phonetics::g2p::rules::GraphemeRule],
    name: &str,
) -> Result<(), String> {
    let declared: std::collections::HashSet<Phoneme> = Phoneme::ALL.iter().copied().collect();
    for (i, rule) in table.iter().enumerate() {
        if rule.grapheme.is_empty() {
            return Err(format!("{name}: règle #{i} sans graphème"));
        }
        for &phoneme in rule.phonemes {
            if !declared.contains(&phoneme) {
                return Err(format!(
                    "{name}: règle «{}» référence un phonème inconnu {phoneme:?}",
                    rule.grapheme
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_dict_table(table: DictTable, name: &str) -> Result<(), String> {
    let declared: std::collections::HashSet<Phoneme> = Phoneme::ALL.iter().copied().collect();
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    let mut _dupes: BTreeMap<&str, usize> = BTreeMap::new();
    for &(key, dialects, entry) in table {
        let joined: String = entry.iter().map(|(letters, _)| *letters).collect();
        if joined != *key {
            return Err(format!(
                "{name}: les segments de «{key}» se concatènent en «{joined}»"
            ));
        }
        for &(_, phonemes) in entry {
            for &phoneme in phonemes {
                if !declared.contains(&phoneme) {
                    return Err(format!(
                        "{name}: «{key}» référence un phonème inconnu {phoneme:?}"
                    ));
                }
            }
        }
        if let Dialects::Only(list) = dialects {
            if list.is_empty() {
                return Err(format!("{name}: «{key}» a une liste de dialectes vide"));
            }
        }
        keys.insert(key);
        *_dupes.entry(key).or_default() += 1;
    }
    Ok(())
}
