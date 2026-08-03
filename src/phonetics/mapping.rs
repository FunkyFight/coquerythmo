//! Phoneme → detection sign mapping.
//!
//! Lives outside the linguistic converters so it stays configurable and
//! replaceable. A consecutive run of phonemes is matched against rules
//! ordered by priority (longest sequence first at equal priority). A phoneme
//! may produce zero, one or several signs; several phonemes may share one
//! sign; a sequence of phonemes may produce a compound sign.

use crate::detection::DetectionKind;
use crate::phonetics::phoneme::{Language, Phoneme};
use serde::{Deserialize, Serialize};

/// One mapping rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhonemeSignRule {
    /// Phoneme sequence matched consecutively within one token.
    pub phonemes: Vec<Phoneme>,
    /// Signs produced for the whole sequence. Empty = explicitly no sign.
    pub signs: Vec<DetectionKind>,
    /// Higher wins; ties broken by longer sequence.
    pub priority: u32,
}

/// A named mapping profile, scoped to a language (signs are shared).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionMapping {
    pub name: String,
    pub language: Language,
    pub rules: Vec<PhonemeSignRule>,
}

/// Fingerprint of a profile (name + rules), stored with generated signs.
pub fn mapping_fingerprint(mapping: &DetectionMapping) -> u64 {
    let mut key = mapping.name.clone();
    for rule in &mapping.rules {
        key.push('|');
        for phoneme in &rule.phonemes {
            key.push_str(phoneme.ipa());
            key.push(',');
        }
        key.push('>');
        for sign in &rule.signs {
            key.push_str(&format!("{sign:?}"));
            key.push(',');
        }
        key.push('@');
        key.push_str(&rule.priority.to_string());
    }
    crate::phonetics::text_fingerprint(&key)
}

/// Errors detected while validating a profile at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappingError {
    EmptySequence,
    ConflictingPriorities {
        a: String,
        b: String,
    },
    /// A declared phoneme has no rule and is not explicitly silenced.
    UnmappedPhoneme {
        phoneme: Phoneme,
        language: Language,
    },
}

/// Phonemes of `language` that may intentionally produce no sign. Everything
/// else must be covered by a rule (enforced by tests).
pub fn default_mapping(language: Language) -> DetectionMapping {
    use crate::detection::DetectionKind::*;
    use Phoneme::*;
    let r = |phonemes: &[Phoneme], signs: &[DetectionKind], priority: u32| PhonemeSignRule {
        phonemes: phonemes.to_vec(),
        signs: signs.to_vec(),
        priority,
    };
    let labial: &[DetectionKind] = &[Labial];
    let semi: &[DetectionKind] = &[SemiLabial];
    let open: &[DetectionKind] = &[MouthOpen];
    let closed: &[DetectionKind] = &[MouthClosed];
    let teeth: &[DetectionKind] = &[TeethVisible];
    let none: &[DetectionKind] = &[];

    let rules = match language {
        Language::French => vec![
            // Bilabial closure: p b m
            r(&[VoicelessBilabialPlosive], labial, 10),
            r(&[VoicedBilabialPlosive], labial, 10),
            r(&[BilabialNasal], labial, 10),
            // Labiodental: f v
            r(&[VoicelessLabiodentalFricative], semi, 10),
            r(&[VoicedLabiodentalFricative], semi, 10),
            // Rounded front vowels / labial approximant: y ø œ ɥ + u ou
            r(&[CloseFrontRounded], semi, 10),
            r(&[CloseMidFrontRounded], semi, 10),
            r(&[OpenMidFrontRounded], semi, 10),
            r(&[NasalOpenMidFrontRoundedFr], semi, 10),
            r(&[LabialPalatalApproximant], semi, 10),
            r(&[CloseBackRounded], semi, 10),
            // Teeth visible: s z ʃ ʒ t d n l θ-like fr
            r(&[VoicelessAlveolarFricative], teeth, 10),
            r(&[VoicedAlveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarFricative], teeth, 10),
            r(&[VoicedPostalveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarAffricate], teeth, 10),
            r(&[VoicedPostalveolarAffricate], teeth, 10),
            r(&[VelarNasal], teeth, 10),
            r(&[VoicelessAlveolarPlosive], teeth, 10),
            r(&[VoicedAlveolarPlosive], teeth, 10),
            r(&[AlveolarNasal], teeth, 10),
            r(&[AlveolarLateralApproximant], teeth, 10),
            // Velars/uvular/nasal stops: mouth mostly closed
            r(&[VoicelessVelarPlosive], closed, 10),
            r(&[VoicedVelarPlosive], closed, 10),
            r(&[VoicedUvularFricative], closed, 10),
            r(&[PalatalNasal], closed, 10),
            // Flap partagé (anglicismes US : better→bette[r])
            r(&[AlveolarTap], teeth, 10),
            // Open vowels
            r(&[OpenCentral], open, 10),
            r(&[OpenBackFr], open, 10),
            r(&[NasalOpenBackFr], open, 10),
            r(&[OpenMidFront], open, 10),
            r(&[NasalOpenMidFrontFr], open, 10),
            r(&[OpenMidBackRounded], open, 10),
            r(&[NasalOpenMidBackFr], open, 10),
            r(&[CloseMidBackRounded], open, 10),
            // Close vowels & glides
            r(&[CloseFront], closed, 10),
            r(&[CloseMidFront], closed, 10),
            r(&[PalatalApproximant], closed, 10),
            r(&[LabialVelarApproximant], semi, 10),
            r(&[Schwa], closed, 10),
        ],
        Language::English => vec![
            // The automatic result is one dominant mouth gesture per
            // syllable; these are the gestures that must survive that merge.
            r(&[VoicelessBilabialPlosive], labial, 10),
            r(&[VoicedBilabialPlosive], labial, 10),
            r(&[BilabialNasal], labial, 10),
            r(&[SyllabicM], labial, 10),
            r(&[VoicelessLabiodentalFricative], semi, 10),
            r(&[VoicedLabiodentalFricative], semi, 10),
            // Rounded/protruded vowels and glides.
            r(&[CloseMidBackRounded], &[ForwardWave], 10),
            r(&[OpenMidBackRounded], &[ForwardWave], 10),
            r(&[CloseBackRounded], &[ForwardWave], 10),
            r(&[NearCloseBackRoundedEn], &[ForwardWave], 10),
            r(&[OpenBackRoundedEnGb], &[ForwardWave], 10),
            r(&[CloseFrontRounded], &[ForwardWave], 10),
            r(&[CloseMidFrontRounded], &[ForwardWave], 10),
            r(&[OpenMidFrontRounded], &[ForwardWave], 10),
            r(&[NasalOpenMidFrontRoundedFr], &[ForwardWave], 10),
            r(&[LabialPalatalApproximant], &[ForwardWave], 10),
            r(&[LabialVelarApproximant], &[ForwardWave], 10),
            r(&[VoicelessLabialVelarFricative], &[ForwardWave], 10),
            r(&[DiphthongGoatEn], &[ForwardWave], 10),
            r(&[DiphthongChoiceEn], &[ForwardWave], 10),
            r(&[DiphthongMouthEn], &[ForwardWave], 10),
            r(&[DiphthongCureEnGb], &[ForwardWave], 10),
            r(&[VoicelessDentalFricative], teeth, 10),
            r(&[VoicedDentalFricative], teeth, 10),
            r(&[VoicelessAlveolarFricative], teeth, 10),
            r(&[VoicedAlveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarFricative], teeth, 10),
            r(&[VoicedPostalveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarAffricate], teeth, 10),
            r(&[VoicedPostalveolarAffricate], teeth, 10),
            r(&[VoicelessAlveolarPlosive], teeth, 10),
            r(&[VoicedAlveolarPlosive], teeth, 10),
            r(&[VoicelessVelarPlosive], teeth, 10),
            r(&[VoicedVelarPlosive], teeth, 10),
            r(&[AlveolarNasal], teeth, 10),
            r(&[VelarNasal], teeth, 10),
            r(&[AlveolarLateralApproximant], teeth, 10),
            r(&[SyllabicL], teeth, 10),
            r(&[SyllabicN], teeth, 10),
            r(&[AlveolarApproximant], teeth, 10),
            r(&[RColoredSchwaEnUs], &[OpeningWave], 10),
            r(&[RColoredOpenMidCentralEnUs], &[OpeningWave], 10),
            // Flap américain (better, water en-US).
            r(&[AlveolarTap], teeth, 10),
            r(&[VoicelessGlottalFricative], &[OpeningWave], 10),
            r(&[PalatalNasal], teeth, 10),
            r(&[GlottalStop], &[OpeningWave], 10),
            // Open or front vowels use an opening wave, never a literal
            // closed-mouth cue.
            r(&[OpenCentral], &[OpeningWave], 10),
            r(&[OpenBackFr], &[OpeningWave], 10),
            r(&[NasalOpenBackFr], &[OpeningWave], 10),
            r(&[OpenMidFront], &[OpeningWave], 10),
            r(&[NasalOpenMidFrontFr], &[OpeningWave], 10),
            r(&[NasalOpenMidBackFr], &[OpeningWave], 10),
            r(&[OpenMidBackEn], &[OpeningWave], 10),
            r(&[NearOpenFrontEn], &[OpeningWave], 10),
            r(&[OpenMidCentralEnGb], &[OpeningWave], 10),
            r(&[CloseFront], teeth, 10),
            r(&[CloseMidFront], teeth, 10),
            r(&[PalatalApproximant], teeth, 10),
            r(&[Schwa], &[OpeningWave], 10),
            r(&[NearCloseFrontEn], teeth, 10),
            r(&[DiphthongFaceEn], teeth, 10),
            r(&[DiphthongPriceEn], &[OpeningWave], 10),
            r(&[DiphthongNearEnGb], &[OpeningWave], 10),
            r(&[DiphthongSquareEnGb], &[OpeningWave], 10),
        ],
        Language::Spanish => vec![
            r(&[VoicelessBilabialPlosive], labial, 10),
            r(&[VoicedBilabialPlosive], labial, 10),
            r(&[VoicedBilabialApproximant], labial, 10),
            r(&[BilabialNasal], labial, 10),
            r(&[SyllabicM], labial, 10),
            r(&[VoicelessLabiodentalFricative], semi, 10),
            r(&[VoicedLabiodentalFricative], semi, 10),
            r(&[CloseBackRounded], semi, 10),
            r(&[NearCloseBackRoundedEn], semi, 10),
            r(&[LabialVelarApproximant], semi, 10),
            r(&[VoicelessLabialVelarFricative], semi, 10),
            r(&[DiphthongGoatEn], semi, 10),
            r(&[CloseFrontRounded], semi, 10),
            r(&[CloseMidFrontRounded], semi, 10),
            r(&[OpenMidFrontRounded], semi, 10),
            r(&[NasalOpenMidFrontRoundedFr], semi, 10),
            r(&[LabialPalatalApproximant], semi, 10),
            r(&[VoicelessDentalFricative], teeth, 10),
            r(&[VoicedDentalFricative], teeth, 10),
            r(&[VoicelessDentalFricativeEsSpain], teeth, 10),
            r(&[VoicelessAlveolarFricative], teeth, 10),
            r(&[VoicedAlveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarFricative], teeth, 10),
            r(&[VoicedPostalveolarFricative], teeth, 10),
            r(&[VoicelessPostalveolarAffricate], teeth, 10),
            r(&[VoicedPostalveolarAffricate], teeth, 10),
            r(&[VoicelessPostalveolarFricativeEs], teeth, 10),
            r(&[VoicedPostalveolarFricativeEs], teeth, 10),
            r(&[VoicelessAlveolarAffricate], teeth, 10),
            r(&[VoicelessAlveolarPlosive], teeth, 10),
            r(&[VoicedAlveolarPlosive], teeth, 10),
            r(&[VoicedDentalApproximant], teeth, 10),
            r(&[AlveolarNasal], teeth, 10),
            r(&[VelarNasal], teeth, 10),
            r(&[AlveolarLateralApproximant], teeth, 10),
            r(&[SyllabicL], teeth, 10),
            r(&[SyllabicN], teeth, 10),
            r(&[VoicedPalatalFricative], teeth, 10),
            r(&[VoicelessVelarFricative], closed, 10),
            r(&[VoicelessVelarPlosive], closed, 10),
            r(&[VoicedVelarPlosive], closed, 10),
            r(&[VoicedVelarApproximant], closed, 10),
            r(&[VoicelessGlottalFricative], closed, 10),
            r(&[PalatalNasal], closed, 10),
            r(&[GlottalStop], closed, 10),
            r(&[AlveolarApproximant], semi, 10),
            r(&[RColoredSchwaEnUs], semi, 10),
            r(&[RColoredOpenMidCentralEnUs], semi, 10),
            r(&[AlveolarTap], teeth, 10),
            r(&[AlveolarTrill], teeth, 10),
            r(&[OpenCentral], open, 10),
            r(&[OpenBackFr], open, 10),
            r(&[NasalOpenBackFr], open, 10),
            r(&[OpenMidFront], open, 10),
            r(&[NasalOpenMidFrontFr], open, 10),
            r(&[OpenMidBackRounded], open, 10),
            r(&[NasalOpenMidBackFr], open, 10),
            r(&[CloseMidBackRounded], open, 10),
            r(&[OpenMidBackEn], open, 10),
            r(&[NearOpenFrontEn], open, 10),
            r(&[OpenBackRoundedEnGb], open, 10),
            r(&[OpenMidCentralEnGb], open, 10),
            r(&[CloseFront], closed, 10),
            r(&[CloseMidFront], closed, 10),
            r(&[PalatalApproximant], closed, 10),
            r(&[Schwa], closed, 10),
            r(&[NearCloseFrontEn], closed, 10),
            r(&[DiphthongFaceEn], closed, 10),
            r(&[DiphthongPriceEn], open, 10),
            r(&[DiphthongChoiceEn], open, 10),
            r(&[DiphthongMouthEn], open, 10),
            r(&[DiphthongNearEnGb], closed, 10),
            r(&[DiphthongSquareEnGb], open, 10),
            r(&[DiphthongCureEnGb], semi, 10),
        ],
    };
    let _ = none;
    DetectionMapping {
        name: format!("default-{}", language.code()),
        language,
        rules,
    }
}

/// Phonemes the engine of each language can produce from its rules and
/// dictionaries. Only these must be covered by the language's mapping; the
/// rest of the shared inventory belongs to other languages.
pub fn relevant_phonemes(language: Language) -> &'static [Phoneme] {
    use Phoneme::*;
    match language {
        Language::French => &[
            OpenCentral,
            OpenBackFr,
            CloseFront,
            CloseMidFront,
            OpenMidFront,
            CloseMidBackRounded,
            OpenMidBackRounded,
            CloseBackRounded,
            CloseFrontRounded,
            CloseMidFrontRounded,
            OpenMidFrontRounded,
            Schwa,
            NasalOpenBackFr,
            NasalOpenMidFrontFr,
            NasalOpenMidBackFr,
            NasalOpenMidFrontRoundedFr,
            VoicelessBilabialPlosive,
            VoicedBilabialPlosive,
            VoicelessAlveolarPlosive,
            VoicedAlveolarPlosive,
            VoicelessVelarPlosive,
            VoicedVelarPlosive,
            BilabialNasal,
            AlveolarNasal,
            PalatalNasal,
            VoicelessLabiodentalFricative,
            VoicedLabiodentalFricative,
            VoicelessAlveolarFricative,
            VoicedAlveolarFricative,
            VoicelessPostalveolarFricative,
            VoicedPostalveolarFricative,
            AlveolarLateralApproximant,
            PalatalApproximant,
            LabialPalatalApproximant,
            LabialVelarApproximant,
            VoicedUvularFricative,
            // Présents via mots anglicisés (parking, match, week-end…)
            VoicelessPostalveolarAffricate,
            VoicedPostalveolarAffricate,
            VelarNasal,
        ],
        Language::English => &[
            OpenCentral,
            CloseFront,
            CloseMidFront,
            OpenMidFront,
            OpenMidBackRounded,
            CloseBackRounded,
            Schwa,
            NearCloseFrontEn,
            NearCloseBackRoundedEn,
            OpenMidBackEn,
            NearOpenFrontEn,
            OpenBackRoundedEnGb,
            RColoredSchwaEnUs,
            OpenMidCentralEnGb,
            RColoredOpenMidCentralEnUs,
            DiphthongFaceEn,
            DiphthongPriceEn,
            DiphthongChoiceEn,
            DiphthongGoatEn,
            DiphthongMouthEn,
            DiphthongNearEnGb,
            DiphthongSquareEnGb,
            DiphthongCureEnGb,
            VoicelessBilabialPlosive,
            VoicedBilabialPlosive,
            VoicelessAlveolarPlosive,
            VoicedAlveolarPlosive,
            VoicelessVelarPlosive,
            VoicedVelarPlosive,
            BilabialNasal,
            AlveolarNasal,
            VelarNasal,
            VoicelessLabiodentalFricative,
            VoicedLabiodentalFricative,
            VoicelessDentalFricative,
            VoicedDentalFricative,
            VoicelessAlveolarFricative,
            VoicedAlveolarFricative,
            VoicelessPostalveolarFricative,
            VoicedPostalveolarFricative,
            VoicelessPostalveolarAffricate,
            VoicedPostalveolarAffricate,
            VoicelessGlottalFricative,
            AlveolarLateralApproximant,
            AlveolarApproximant,
            PalatalApproximant,
            LabialVelarApproximant,
            SyllabicL,
            SyllabicN,
            SyllabicM,
            GlottalStop,
            VoicelessLabialVelarFricative,
            // Flap américain réutilise AlveolarTap (partagé avec l'espagnol).
            AlveolarTap,
        ],
        Language::Spanish => &[
            OpenCentral,
            CloseFront,
            CloseMidFront,
            CloseMidBackRounded,
            CloseBackRounded,
            VoicelessBilabialPlosive,
            VoicedBilabialPlosive,
            VoicedBilabialApproximant,
            VoicelessAlveolarPlosive,
            VoicedAlveolarPlosive,
            VoicedDentalApproximant,
            VoicelessVelarPlosive,
            VoicedVelarPlosive,
            VoicedVelarApproximant,
            BilabialNasal,
            AlveolarNasal,
            PalatalNasal,
            VoicelessLabiodentalFricative,
            VoicelessAlveolarFricative,
            VoicelessDentalFricativeEsSpain,
            VoicelessVelarFricative,
            VoicelessPostalveolarAffricate,
            VoicedPalatalFricative,
            VoicedPostalveolarFricativeEs,
            VoicelessPostalveolarFricativeEs,
            VoicelessAlveolarAffricate,
            AlveolarLateralApproximant,
            PalatalApproximant,
            LabialVelarApproximant,
            AlveolarTap,
            AlveolarTrill,
        ],
    }
}

/// Validate that every phoneme of the shared inventory has an explicit rule
/// for this language. Missing ones are listed, never silently ignored.
pub fn validate_mapping(mapping: &DetectionMapping) -> Result<(), Vec<MappingError>> {
    let mut errors = Vec::new();
    let mut seen_priorities: Vec<(String, u32)> = Vec::new();
    for rule in &mapping.rules {
        if rule.phonemes.is_empty() {
            errors.push(MappingError::EmptySequence);
        }
        let key = rule
            .phonemes
            .iter()
            .map(|p: &Phoneme| format!("{p:?}"))
            .collect::<Vec<_>>()
            .join("+");
        if let Some((existing, _)) = seen_priorities
            .iter()
            .find(|(k, priority)| *priority == rule.priority && *k == key)
        {
            errors.push(MappingError::ConflictingPriorities {
                a: existing.clone(),
                b: key.clone(),
            });
        }
        seen_priorities.push((key, rule.priority));
    }
    let relevant = relevant_phonemes(mapping.language);
    for &phoneme in relevant {
        let covered = mapping
            .rules
            .iter()
            .any(|rule| rule.phonemes.contains(&phoneme));
        if !covered {
            errors.push(MappingError::UnmappedPhoneme {
                phoneme,
                language: mapping.language,
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// One emitted sign for a matched phoneme span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInstance {
    pub kind: DetectionKind,
    /// Indices into the flattened phoneme sequence covered by the sign.
    pub first_phoneme: usize,
    pub last_phoneme: usize,
}

/// Match a phoneme sequence against the profile. Greedy: at each position,
/// pick the highest-priority then longest rule starting there.
pub fn map_sequence(mapping: &DetectionMapping, phonemes: &[Phoneme]) -> Vec<SignInstance> {
    let mut signs = Vec::new();
    let mut i = 0usize;
    while i < phonemes.len() {
        let mut best: Option<&PhonemeSignRule> = None;
        for rule in &mapping.rules {
            if i + rule.phonemes.len() > phonemes.len() {
                continue;
            }
            if phonemes[i..i + rule.phonemes.len()] != rule.phonemes[..] {
                continue;
            }
            let take = match best {
                None => true,
                Some(current) => {
                    (rule.priority, rule.phonemes.len())
                        > (current.priority, current.phonemes.len())
                }
            };
            if take {
                best = Some(rule);
            }
        }
        if let Some(rule) = best {
            for sign in &rule.signs {
                signs.push(SignInstance {
                    kind: *sign,
                    first_phoneme: i,
                    last_phoneme: i + rule.phonemes.len().saturating_sub(1),
                });
            }
            i += rule.phonemes.len();
        } else {
            // Unmapped phoneme in a valid profile should not happen; be loud
            // in debug, skip in release.
            debug_assert!(false, "unmapped phoneme {:?}", phonemes[i]);
            i += 1;
        }
    }
    signs
}

pub type SignMappingProfile = DetectionMapping;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phonetics::phoneme::Phoneme;

    #[test]
    fn default_profiles_cover_every_declared_phoneme() {
        for language in [Language::French, Language::English, Language::Spanish] {
            let mapping = default_mapping(language);
            validate_mapping(&mapping).unwrap_or_else(|errors| {
                panic!(
                    "language {:?}: {} mapping errors: {:?}",
                    language,
                    errors.len(),
                    errors
                )
            });
        }
    }

    #[test]
    fn sequence_rule_wins_over_unit_rules() {
        use Phoneme::*;
        let mapping = DetectionMapping {
            name: "test".into(),
            language: Language::French,
            rules: vec![
                PhonemeSignRule {
                    phonemes: vec![CloseFront],
                    signs: vec![DetectionKind::MouthClosed],
                    priority: 1,
                },
                PhonemeSignRule {
                    phonemes: vec![CloseFront, PalatalApproximant],
                    signs: vec![DetectionKind::MouthOpen],
                    priority: 5,
                },
            ],
        };
        let signs = map_sequence(&mapping, &[CloseFront, PalatalApproximant, CloseFront]);
        assert_eq!(signs.len(), 2);
        assert_eq!(signs[0].kind, DetectionKind::MouthOpen);
        assert_eq!((signs[0].first_phoneme, signs[0].last_phoneme), (0, 1));
        assert_eq!(signs[1].kind, DetectionKind::MouthClosed);
        assert_eq!((signs[1].first_phoneme, signs[1].last_phoneme), (2, 2));
    }

    #[test]
    fn phoneme_may_emit_no_sign_and_many_signs() {
        let mapping = DetectionMapping {
            name: "t".into(),
            language: Language::French,
            rules: vec![
                PhonemeSignRule {
                    phonemes: vec![Phoneme::Schwa],
                    signs: vec![],
                    priority: 1,
                },
                PhonemeSignRule {
                    phonemes: vec![Phoneme::BilabialNasal],
                    signs: vec![DetectionKind::Labial, DetectionKind::MouthClosed],
                    priority: 1,
                },
            ],
        };
        let signs = map_sequence(&mapping, &[Phoneme::Schwa, Phoneme::BilabialNasal]);
        assert_eq!(signs.len(), 2);
    }
}
