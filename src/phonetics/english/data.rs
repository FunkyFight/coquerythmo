//! English grapheme→phoneme data for GA (en-US) and SSB (en-GB).

use crate::phonetics::g2p::rules::{Ctx, Dialects, DictTable, GraphemeRule};
use crate::phonetics::phoneme::Dialect;
use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::phoneme::Phoneme::*;

const A: Ctx = Ctx::Any;
const S: Ctx = Ctx::Start;
const E: Ctx = Ctx::End;
const V: Ctx = Ctx::Vowel;

const US: &[Dialect] = &[Dialect::EnUs];
const GB: &[Dialect] = &[Dialect::EnGb];
const BOTH: &[Dialect] = &[Dialect::EnUs, Dialect::EnGb];

const fn r(g: &'static str, l: Ctx, rt: Ctx, p: &'static [Phoneme]) -> GraphemeRule {
    GraphemeRule::rule(g, l, rt, p)
}
const fn rol(
    g: &'static str,
    l: Ctx,
    rt: Ctx,
    p: &'static [Phoneme],
    dialects: &'static [Dialect],
) -> GraphemeRule {
    GraphemeRule::only(g, l, rt, p, dialects)
}

#[rustfmt::skip]
pub static RULES: &[GraphemeRule] = &[
    // ═══ Long vowel digraphs & diphthongs (checked before trigraphs) ════
    r("eigh", A, A, &[DiphthongFaceEn]),         // eight, weight
    r("ough", A, E, &[DiphthongGoatEn]),         // though, dough
    r("augh", A, A, &[OpenMidBackRounded]),      // caught, daughter
    r("tion", A, A, &[VoicelessPostalveolarFricative, Schwa, AlveolarNasal]),
    r("sion", A, A, &[VoicedPostalveolarFricative, Schwa, AlveolarNasal]),
    r("ture", A, E, &[VoicelessPostalveolarAffricate, Schwa, AlveolarApproximant]),
    r("sure", A, E, &[VoicedPostalveolarFricative, Schwa, AlveolarApproximant]),
    r("igh", A, A, &[DiphthongPriceEn]),         // high, light
    r("igh", A, E, &[DiphthongPriceEn]),
    r("ind", A, E, &[DiphthongPriceEn, AlveolarNasal, VoicedAlveolarPlosive]), // find
    r("ild", A, E, &[DiphthongPriceEn, AlveolarLateralApproximant, VoicedAlveolarPlosive]),
    r("ign", A, E, &[DiphthongPriceEn, AlveolarNasal]), // sign
    r("ould", A, E, &[NearCloseBackRoundedEn, AlveolarLateralApproximant, VoicedAlveolarPlosive]), // could
    r("ear", A, A, &[NearCloseFrontEn, AlveolarApproximant]), // hear
    r("air", A, A, &[OpenMidFront, AlveolarApproximant]),     // hair
    r("eer", A, A, &[NearCloseFrontEn, AlveolarApproximant]),
    r("ure", A, E, &[NearCloseBackRoundedEn, AlveolarApproximant]), // pure

    // ═══ Digraphs ═══════════════════════════════════════════════════════
    r("ee", A, A, &[CloseFront]),
    r("ea", A, A, &[CloseFront]),                // sea, read (present)
    r("ai", A, A, &[DiphthongFaceEn]),           // rain
    r("ay", A, A, &[DiphthongFaceEn]),           // day
    r("ey", A, E, &[CloseFront]),                // money, key
    r("oa", A, A, &[DiphthongGoatEn]),           // boat
    r("oe", A, E, &[DiphthongGoatEn]),           // toe
    r("ow", A, E, &[DiphthongGoatEn]),           // low (vs cow: dictionary)
    r("oo", A, A, &[CloseBackRounded]),          // too, moon (book: dic)
    r("oi", A, A, &[DiphthongChoiceEn]),
    r("oy", A, A, &[DiphthongChoiceEn]),
    r("ou", A, A, &[DiphthongMouthEn]),          // out, loud
    r("ow", A, A, &[DiphthongMouthEn]),
    r("au", A, A, &[OpenMidBackRounded]),        // author
    r("aw", A, A, &[OpenMidBackRounded]),        // law
    r("or", A, E, &[OpenMidBackRounded, AlveolarApproximant]), // for
    r("ar", A, E, &[OpenCentral, AlveolarApproximant]),        // car
    r("er", A, E, &[Schwa, AlveolarApproximant]),              // better (US)
    r("ir", A, A, &[OpenMidCentralEnGb, AlveolarApproximant]), // bird → US handled by post? no: use variants below
    r("ur", A, A, &[OpenMidCentralEnGb, AlveolarApproximant]),
    r("ew", A, A, &[CloseBackRounded]),          // new (GB also juː)

    // ═══ Consonant digraphs ═════════════════════════════════════════════
    r("tch", A, A, &[VoicelessPostalveolarAffricate]),
    r("dge", A, A, &[VoicedPostalveolarAffricate]),
    r("ch", A, A, &[VoicelessPostalveolarAffricate]),
    r("sh", A, A, &[VoicelessPostalveolarFricative]),
    r("th", A, A, &[VoicedDentalFricative]),     // the (default; θ words in dic)
    r("ph", A, A, &[VoicelessLabiodentalFricative]),
    r("wh", A, A, &[LabialVelarApproximant]),    // what (conservative ʍ in dic)
    r("wr", A, A, &[AlveolarApproximant]),
    r("kn", A, A, &[AlveolarNasal]),
    r("gn", S, A, &[AlveolarNasal]),             // gnaw
    r("gn", A, A, &[VoicedVelarPlosive, AlveolarNasal]), // signal
    r("mb", A, E, &[BilabialNasal]),             // lamb
    r("ng", A, A, &[VelarNasal]),
    r("nk", A, A, &[VelarNasal, VoicelessVelarPlosive]),
    r("qu", A, A, &[VoicelessVelarPlosive, LabialVelarApproximant]),
    r("ck", A, A, &[VoicelessVelarPlosive]),
    r("dg", A, A, &[VoicedPostalveolarAffricate]),
    r("gh", A, E, &[]),                          // though, through
    r("gh", S, A, &[VoicedVelarPlosive]),        // ghost
    r("gh", A, A, &[VoicelessLabiodentalFricative]), // laugh, enough
    r("gu", S, A, &[VoicedVelarPlosive]),        // guess, guide
    r("gue", A, E, &[VoicedVelarPlosive]),       // league
    r("ue", A, E, &[CloseBackRounded]),          // blue, true
    r("ue", A, A, &[CloseBackRounded]),          // cruel
    r("ui", A, A, &[CloseBackRounded]),          // fruit, build
    r("le", A, E, &[SyllabicL]),                 // bottle (after consonant)
    r("le", A, A, &[AlveolarLateralApproximant, CloseMidFront]),

    // ═══ Contextual c/g/x/y ═════════════════════════════════════════════
    r("c", A, Ctx::FrontVowel, &[VoicelessAlveolarFricative]), // city
    r("c", A, A, &[VoicelessVelarPlosive]),
    r("g", A, Ctx::FrontVowel, &[VoicedPostalveolarAffricate]), // gem, giraffe
    r("g", A, A, &[VoicedVelarPlosive]),
    r("x", A, A, &[VoicelessVelarPlosive, VoicelessAlveolarFricative]),
    r("y", S, A, &[PalatalApproximant]),         // yes, you
    r("y", A, E, &[CloseFront]),                 // happy, very
    r("y", A, A, &[DiphthongPriceEn]),           // my, fly
    r("se", A, E, &[VoicedAlveolarFricative]),   // rose, house(vb) — approx
    r("ss", A, A, &[VoicelessAlveolarFricative]),
    r("s", V, V, &[VoicedAlveolarFricative]),    // easy
    r("s", A, A, &[VoicelessAlveolarFricative]),

    // ═══ Finals ═════════════════════════════════════════════════════════
    r("e", A, E, &[]),                           // silent final e (magic-e in post)
    r("es", A, E, &[]),
    r("ed", A, E, &[VoicedAlveolarPlosive]),     // walked (default; dic for id/t)

    // ═══ Single letters ═════════════════════════════════════════════════
    r("i", A, A, &[NearCloseFrontEn]),
    r("a", A, A, &[NearOpenFrontEn]),
    r("e", A, A, &[OpenMidFront]),
    r("o", A, A, &[OpenMidBackRounded]),         // hot US; GB post? kept via dialect rules below
    r("u", A, A, &[OpenMidBackEn]),
    r("h", A, A, &[VoicelessGlottalFricative]),
    r("j", A, A, &[VoicedPostalveolarAffricate]),
    r("w", A, A, &[LabialVelarApproximant]),
    r("v", A, A, &[VoicedLabiodentalFricative]),
    r("z", A, A, &[VoicedAlveolarFricative]),
    r("r", A, A, &[AlveolarApproximant]),
    r("l", A, A, &[AlveolarLateralApproximant]),
    r("m", A, A, &[BilabialNasal]),
    r("n", A, A, &[AlveolarNasal]),
    r("p", A, A, &[VoicelessBilabialPlosive]),
    r("b", A, A, &[VoicedBilabialPlosive]),
    r("t", A, A, &[VoicelessAlveolarPlosive]),
    r("d", A, A, &[VoicedAlveolarPlosive]),
    r("k", A, A, &[VoicelessVelarPlosive]),
    r("f", A, A, &[VoicelessLabiodentalFricative]),
];

const ALL: Dialects = Dialects::All;
const ONLY_US: Dialects = Dialects::Only(US);
const ONLY_GB: Dialects = Dialects::Only(GB);

#[rustfmt::skip]
pub static DICTIONARY: DictTable = &[
    // Letter names (acronyms): a ay, b bee, c cee…
    ("a", ALL, &[("a", &[DiphthongFaceEn])]),
    ("bee", ALL, &[("b", &[VoicedBilabialPlosive]), ("ee", &[CloseFront])]),
    ("cee", ALL, &[("c", &[VoicelessAlveolarFricative]), ("ee", &[CloseFront])]),
    ("dee", ALL, &[("d", &[VoicedAlveolarPlosive]), ("ee", &[CloseFront])]),
    ("e", ALL, &[("e", &[CloseFront])]),
    ("eff", ALL, &[("e", &[OpenMidFront]), ("ff", &[VoicelessLabiodentalFricative])]),
    ("gee", ALL, &[("g", &[VoicedPostalveolarAffricate]), ("ee", &[CloseFront])]),
    ("aitch", ALL, &[("a", &[DiphthongFaceEn]), ("i", &[]), ("tch", &[VoicelessPostalveolarAffricate])]),
    ("i", ALL, &[("i", &[DiphthongPriceEn])]),
    ("jay", ALL, &[("j", &[VoicedPostalveolarAffricate]), ("ay", &[DiphthongFaceEn])]),
    ("kay", ALL, &[("k", &[VoicelessVelarPlosive]), ("ay", &[DiphthongFaceEn])]),
    ("ell", ALL, &[("e", &[OpenMidFront]), ("ll", &[AlveolarLateralApproximant])]),
    ("em", ALL, &[("e", &[OpenMidFront]), ("m", &[BilabialNasal])]),
    ("en", ALL, &[("e", &[OpenMidFront]), ("n", &[AlveolarNasal])]),
    ("o", ALL, &[("o", &[DiphthongGoatEn])]),
    ("pee", ALL, &[("p", &[VoicelessBilabialPlosive]), ("ee", &[CloseFront])]),
    ("cue", ALL, &[("c", &[VoicelessVelarPlosive]), ("ue", &[CloseBackRounded])]),
    ("ar", ALL, &[("a", &[OpenCentral]), ("r", &[AlveolarApproximant])]),
    ("ess", ALL, &[("e", &[OpenMidFront]), ("ss", &[VoicelessAlveolarFricative])]),
    ("tee", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("ee", &[CloseFront])]),
    ("u", ALL, &[("u", &[PalatalApproximant, CloseBackRounded])]),
    ("vee", ALL, &[("v", &[VoicedLabiodentalFricative]), ("ee", &[CloseFront])]),
    ("double-u", ALL, &[("d", &[VoicedAlveolarPlosive]), ("o", &[OpenMidBackEn]), ("u", &[OpenMidBackEn]), ("b", &[VoicedBilabialPlosive]), ("l", &[SyllabicL]), ("e", &[]), ("-", &[]), ("u", &[PalatalApproximant, CloseBackRounded])]),
    ("ex", ALL, &[("e", &[OpenMidFront]), ("x", &[VoicelessVelarPlosive, VoicelessAlveolarFricative])]),
    ("wy", ALL, &[("w", &[LabialVelarApproximant]), ("y", &[DiphthongPriceEn])]),
    ("zee", ALL, &[("z", &[VoicedAlveolarFricative]), ("ee", &[CloseFront])]),
    ("zed", ALL, &[("z", &[VoicedAlveolarFricative]), ("e", &[OpenMidFront]), ("d", &[VoicedAlveolarPlosive])]),

    // Heterophones 2 candidates (default first) :
    ("read", ALL, &[("r", &[AlveolarApproximant]), ("ea", &[CloseFront]), ("d", &[VoicedAlveolarPlosive])]),
    ("read", ALL, &[("r", &[AlveolarApproximant]), ("ea", &[OpenMidFront]), ("d", &[VoicedAlveolarPlosive])]),
    ("lead", ALL, &[("l", &[AlveolarLateralApproximant]), ("ea", &[CloseFront]), ("d", &[VoicedAlveolarPlosive])]),
    ("lead", ALL, &[("l", &[AlveolarLateralApproximant]), ("ea", &[OpenMidFront]), ("d", &[VoicedAlveolarPlosive])]),
    ("wind", ALL, &[("w", &[LabialVelarApproximant]), ("i", &[NearCloseFrontEn]), ("nd", &[AlveolarNasal, VoicedAlveolarPlosive])]),
    ("wind", ALL, &[("w", &[LabialVelarApproximant]), ("ind", &[DiphthongPriceEn, AlveolarNasal, VoicedAlveolarPlosive])]),
    ("live", ALL, &[("l", &[AlveolarLateralApproximant]), ("i", &[NearCloseFrontEn]), ("v", &[VoicedLabiodentalFricative]), ("e", &[])]),
    ("live", ALL, &[("l", &[AlveolarLateralApproximant]), ("i", &[DiphthongPriceEn]), ("v", &[VoicedLabiodentalFricative]), ("e", &[])]),
    ("close", ALL, &[("cl", &[VoicelessVelarPlosive, AlveolarLateralApproximant]), ("o", &[DiphthongGoatEn]), ("s", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("close", ALL, &[("cl", &[VoicelessVelarPlosive, AlveolarLateralApproximant]), ("o", &[DiphthongGoatEn]), ("s", &[VoicelessAlveolarFricative]), ("e", &[])]),
    ("record", ALL, &[("r", &[AlveolarApproximant]), ("e", &[NearCloseFrontEn]), ("c", &[VoicelessVelarPlosive]), ("or", &[OpenMidBackRounded, AlveolarApproximant]), ("d", &[VoicedAlveolarPlosive])]),
    ("present", ALL, &[("p", &[VoicelessBilabialPlosive]), ("r", &[AlveolarApproximant]), ("e", &[OpenMidFront]), ("s", &[VoicedAlveolarFricative]), ("e", &[Schwa]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive])]),
    ("object", ALL, &[("o", &[Schwa]), ("b", &[VoicedBilabialPlosive]), ("j", &[VoicedPostalveolarAffricate]), ("e", &[OpenMidFront]), ("c", &[VoicelessVelarPlosive]), ("t", &[VoicelessAlveolarPlosive])]),
    ("content", ALL, &[("c", &[VoicelessVelarPlosive]), ("o", &[Schwa]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive]), ("e", &[OpenMidFront]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive])]),

    // Common function words (weak forms as variants)
    ("the", ALL, &[("th", &[VoicedDentalFricative]), ("e", &[Schwa])]),
    ("to", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("o", &[Schwa])]),
    ("of", ALL, &[("o", &[Schwa]), ("f", &[VoicedLabiodentalFricative])]),
    ("and", ALL, &[("a", &[Schwa]), ("n", &[AlveolarNasal]), ("d", &[VoicedAlveolarPlosive])]),
    ("you", ALL, &[("y", &[PalatalApproximant]), ("ou", &[CloseBackRounded])]),
    ("was", ALL, &[("w", &[LabialVelarApproximant]), ("a", &[OpenMidBackRounded]), ("s", &[VoicedAlveolarFricative])]),
    ("were", ALL, &[("w", &[LabialVelarApproximant]), ("ere", &[OpenMidCentralEnGb, AlveolarApproximant])]),
    ("said", ALL, &[("s", &[VoicelessAlveolarFricative]), ("ai", &[OpenMidFront]), ("d", &[VoicedAlveolarPlosive])]),
    ("says", ALL, &[("s", &[VoicelessAlveolarFricative]), ("ay", &[OpenMidFront]), ("s", &[VoicedAlveolarFricative])]),
    ("two", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("w", &[]), ("o", &[CloseBackRounded])]),
];

#[rustfmt::skip]
pub static EXCEPTIONS: DictTable = &[
    ("one", ALL, &[("o", &[OpenMidBackRounded]), ("ne", &[AlveolarNasal])]), // wʌn via variante ci-dessous
    ("one", ALL, &[("o", &[LabialVelarApproximant, OpenMidBackEn]), ("ne", &[AlveolarNasal])]),
    ("once", ALL, &[("o", &[LabialVelarApproximant, OpenMidBackEn]), ("n", &[AlveolarNasal]), ("ce", &[VoicelessAlveolarFricative])]),
    ("could", ALL, &[("c", &[VoicelessVelarPlosive]), ("ould", &[NearCloseBackRoundedEn, AlveolarLateralApproximant, VoicedAlveolarPlosive])]),
    ("would", ALL, &[("w", &[LabialVelarApproximant]), ("ould", &[NearCloseBackRoundedEn, AlveolarLateralApproximant, VoicedAlveolarPlosive])]),
    ("should", ALL, &[("sh", &[VoicelessPostalveolarFricative]), ("ould", &[NearCloseBackRoundedEn, AlveolarLateralApproximant, VoicedAlveolarPlosive])]),
    ("women", ALL, &[("w", &[LabialVelarApproximant]), ("o", &[NearCloseFrontEn]), ("m", &[BilabialNasal]), ("e", &[NearCloseFrontEn]), ("n", &[AlveolarNasal])]),
    ("though", ALL, &[("th", &[VoicedDentalFricative]), ("ough", &[DiphthongGoatEn])]),
    ("through", ALL, &[("th", &[VoicelessDentalFricative]), ("rough", &[AlveolarApproximant, CloseBackRounded])]),
    ("thought", ALL, &[("th", &[VoicelessDentalFricative]), ("ought", &[OpenMidBackRounded, VoicelessAlveolarPlosive])]),
    ("know", ALL, &[("kn", &[AlveolarNasal]), ("ow", &[DiphthongGoatEn])]),
    ("knife", ALL, &[("kn", &[AlveolarNasal]), ("i", &[DiphthongPriceEn]), ("f", &[VoicelessLabiodentalFricative]), ("e", &[])]),
    ("write", ALL, &[("wr", &[AlveolarApproximant]), ("i", &[DiphthongPriceEn]), ("t", &[VoicelessAlveolarPlosive]), ("e", &[])]),
    ("who", ALL, &[("wh", &[VoicelessGlottalFricative]), ("o", &[CloseBackRounded])]),
    ("island", ALL, &[("i", &[DiphthongPriceEn]), ("s", &[]), ("l", &[AlveolarLateralApproximant]), ("a", &[Schwa]), ("n", &[AlveolarNasal]), ("d", &[VoicedAlveolarPlosive])]),
    ("debt", ALL, &[("d", &[VoicedAlveolarPlosive]), ("e", &[OpenMidFront]), ("b", &[]), ("t", &[VoicelessAlveolarPlosive])]),
    ("doubt", ALL, &[("d", &[VoicedAlveolarPlosive]), ("ou", &[DiphthongMouthEn]), ("b", &[]), ("t", &[VoicelessAlveolarPlosive])]),
    ("subtle", ALL, &[("s", &[VoicelessAlveolarFricative]), ("u", &[OpenMidBackEn]), ("b", &[]), ("t", &[VoicelessAlveolarPlosive]), ("le", &[SyllabicL])]),
    ("receipt", ALL, &[("r", &[AlveolarApproximant]), ("e", &[CloseFront]), ("cei", &[CloseFront]), ("p", &[]), ("t", &[VoicelessAlveolarPlosive])]),
    ("fast", ONLY_GB, &[("f", &[VoicelessLabiodentalFricative]), ("a", &[OpenCentral]), ("s", &[VoicelessAlveolarFricative]), ("t", &[VoicelessAlveolarPlosive])]),
    ("can't", ONLY_GB, &[("c", &[VoicelessVelarPlosive]), ("a", &[OpenCentral]), ("n", &[]), ("'", &[]), ("t", &[VoicelessAlveolarPlosive])]),
    ("dance", ONLY_GB, &[("d", &[VoicedAlveolarPlosive]), ("a", &[OpenCentral]), ("n", &[AlveolarNasal]), ("c", &[VoicelessAlveolarFricative]), ("e", &[])]),
    ("path", ONLY_GB, &[("p", &[VoicelessBilabialPlosive]), ("a", &[OpenCentral]), ("th", &[VoicelessDentalFricative])]),
    ("grass", ONLY_GB, &[("g", &[VoicedVelarPlosive]), ("r", &[]), ("a", &[OpenCentral]), ("ss", &[VoicelessAlveolarFricative])]),
    ("better", ONLY_US, &[("b", &[VoicedBilabialPlosive]), ("e", &[OpenMidFront]), ("tt", &[AlveolarTap]), ("er", &[RColoredSchwaEnUs])]),
    ("water", ONLY_US, &[("w", &[LabialVelarApproximant]), ("a", &[OpenMidBackRounded]), ("t", &[AlveolarTap]), ("er", &[RColoredSchwaEnUs])]),
    ("water", ONLY_GB, &[("w", &[LabialVelarApproximant]), ("a", &[OpenMidBackRounded]), ("t", &[VoicelessAlveolarPlosive]), ("er", &[Schwa])]),
    ("nurse", ONLY_US, &[("n", &[AlveolarNasal]), ("ur", &[RColoredOpenMidCentralEnUs]), ("s", &[VoicelessAlveolarFricative]), ("e", &[])]),
    ("nurse", ONLY_GB, &[("n", &[AlveolarNasal]), ("ur", &[OpenMidCentralEnGb]), ("s", &[VoicelessAlveolarFricative]), ("e", &[])]),
    ("bird", ONLY_US, &[("b", &[VoicedBilabialPlosive]), ("ir", &[RColoredOpenMidCentralEnUs]), ("d", &[VoicedAlveolarPlosive])]),
    ("bird", ONLY_GB, &[("b", &[VoicedBilabialPlosive]), ("ir", &[OpenMidCentralEnGb]), ("d", &[VoicedAlveolarPlosive])]),
    ("car", ONLY_US, &[("c", &[VoicelessVelarPlosive]), ("ar", &[OpenCentral, AlveolarApproximant])]),
    ("car", ONLY_GB, &[("c", &[VoicelessVelarPlosive]), ("ar", &[OpenCentral])]),
    ("near", ONLY_US, &[("n", &[AlveolarNasal]), ("ear", &[NearCloseFrontEn, AlveolarApproximant])]),
    ("near", ONLY_GB, &[("n", &[AlveolarNasal]), ("ear", &[DiphthongNearEnGb])]),
    ("square", ONLY_US, &[("squ", &[VoicelessAlveolarFricative, VoicelessVelarPlosive, LabialVelarApproximant]), ("are", &[OpenMidFront, AlveolarApproximant])]),
    ("square", ONLY_GB, &[("squ", &[VoicelessAlveolarFricative, VoicelessVelarPlosive, LabialVelarApproximant]), ("are", &[DiphthongSquareEnGb])]),
    ("cure", ONLY_US, &[("c", &[VoicelessVelarPlosive]), ("ure", &[NearCloseBackRoundedEn, AlveolarApproximant])]),
    ("cure", ONLY_GB, &[("c", &[VoicelessVelarPlosive]), ("ure", &[DiphthongCureEnGb])]),
    ("lot", ONLY_GB, &[("l", &[AlveolarLateralApproximant]), ("o", &[OpenBackRoundedEnGb]), ("t", &[VoicelessAlveolarPlosive])]),
    ("hot", ONLY_GB, &[("h", &[VoicelessGlottalFricative]), ("o", &[OpenBackRoundedEnGb]), ("t", &[VoicelessAlveolarPlosive])]),
];

pub static PRONOUNCEABLE_ACRONYMS: &[&str] = &[
    "nasa", "nato", "unesco", "unicef", "laser", "radar", "scuba", "aids",
    "ovni", "gif", "jpeg", "sim", "pin", "vat", "opec", "fifa", "uefa",
];
