//! Datos lingüísticos del español. Cobertura : distinción (Espagne) vs seseo
//! (Latinoamérica), yeísmo (ʝ), r/rr, j/g ante e/i, gü/qu, ch, ñ, h muda,
//! u muda en gue/gui/que/qui, diptongos/hiatus, aproximantes intervocálicas.

use crate::phonetics::g2p::rules::{Ctx, Dialects, DictTable, GraphemeRule};
use crate::phonetics::phoneme::Dialect;
use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::phoneme::Phoneme::*;

const A: Ctx = Ctx::Any;
const S: Ctx = Ctx::Start;
const E: Ctx = Ctx::End;
const V: Ctx = Ctx::Vowel;
const FV: Ctx = Ctx::FrontVowel;

const SPAIN: &[Dialect] = &[Dialect::EsSpain];
const LATAM: &[Dialect] = &[Dialect::EsLatam];

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
    // ═══ Dígrafos consonánticos ═════════════════════════════════════════
    r("ch", A, A, &[VoicelessPostalveolarAffricate]),
    r("rr", A, A, &[AlveolarTrill]),
    r("ll", A, A, &[VoicedPalatalFricative]),        // yeísmo (ʝ)
    r("qu", A, FV, &[VoicelessVelarPlosive]),         // que, qui
    r("qu", A, A, &[VoicelessVelarPlosive, LabialVelarApproximant]), // cuota
    r("gü", A, V, &[VoicedVelarPlosive]),             // pingüino: u prononcée → g + w géré below
    r("gu", A, V, &[VoicedVelarPlosive]),             // guerra, guiar
    r("r", S, A, &[AlveolarTrill]),            // r initiale : rojo
    r("r", Ctx::Letter("lns"), A, &[AlveolarTrill]),  // après l/n/s : enredo, Israel
    r("r", A, A, &[AlveolarTap]),              // pero
    r("y", A, V, &[VoicedPalatalFricative]),          // yo
    r("y", A, E, &[CloseFront]),                      // muy, soy, estoy
    r("y", E, E, &[CloseFront]),                      // "y" conjonction
    r("x", A, A, &[VoicelessVelarFricative]),         // México, examen (es)
    r("j", A, A, &[VoicelessVelarFricative]),         // jamón
    r("g", A, FV, &[VoicelessVelarFricative]),        // gente
    r("g", A, A, &[VoicedVelarPlosive]),              // gato
    rol("z", A, A, &[VoicelessDentalFricativeEsSpain], SPAIN),  // zapato (distinción)
    rol("z", A, A, &[VoicelessAlveolarFricative], LATAM),       // seseo
    rol("c", A, FV, &[VoicelessDentalFricativeEsSpain], SPAIN), // cena
    rol("c", A, FV, &[VoicelessAlveolarFricative], LATAM),
    r("c", A, A, &[VoicelessVelarPlosive]),           // casa
    r("h", A, A, &[]),                                // h muda
    r("ñ", A, A, &[PalatalNasal]),
    r("v", A, A, &[VoicedBilabialPlosive]),           // v = b en español
    r("b", A, A, &[VoicedBilabialPlosive]),
    r("d", A, E, &[VoicedDentalApproximant]),         // pared final, approché
    r("d", V, A, &[VoicedDentalApproximant]),         // nada
    r("d", A, A, &[VoicedAlveolarPlosive]),
    r("s", A, A, &[VoicelessAlveolarFricative]),
    r("f", A, A, &[VoicelessLabiodentalFricative]),
    r("p", A, A, &[VoicelessBilabialPlosive]),
    r("t", A, A, &[VoicelessAlveolarPlosive]),
    r("k", A, A, &[VoicelessVelarPlosive]),
    r("w", A, A, &[LabialVelarApproximant]),
    r("m", A, A, &[BilabialNasal]),
    r("n", A, A, &[AlveolarNasal]),
    r("l", A, A, &[AlveolarLateralApproximant]),

    // ═══ Vocales (5 systèmes) ═══════════════════════════════════════════
    r("á", A, A, &[OpenCentral]),
    r("é", A, A, &[CloseMidFront]),
    r("í", A, A, &[CloseFront]),
    r("ó", A, A, &[CloseMidBackRounded]),
    r("ú", A, A, &[CloseBackRounded]),
    r("ü", A, A, &[CloseBackRounded]),
    // Diptongos: u/i débiles + voyelle forte
    r("u", A, V, &[LabialVelarApproximant]),  // agua, bueno (weak u before strong vowel)
    r("i", A, V, &[PalatalApproximant]),      // piano, tierra
    r("a", A, A, &[OpenCentral]),
    r("e", A, A, &[CloseMidFront]),
    r("i", A, A, &[CloseFront]),
    r("o", A, A, &[CloseMidBackRounded]),
    r("u", A, A, &[CloseBackRounded]),
];


// ═══════════════════════════ Diccionario ══════════════════════════════════
const ALL: Dialects = Dialects::All;
const ONLY_SPAIN: Dialects = Dialects::Only(SPAIN);
const ONLY_LATAM: Dialects = Dialects::Only(LATAM);

#[rustfmt::skip]
pub static DICTIONARY: DictTable = &[
    // Nombres de letras (siglas) : a, be, ce, de, e, efe, ge, hache, i,
    // jota, ka, ele, eme, ene, eñe, o, pe, cu, ere, ese, te, u, uve,
    // uve-doble, equis, i-griega, zeta
    ("a", ALL, &[("a", &[OpenCentral])]),
    ("be", ALL, &[("b", &[VoicedBilabialPlosive]), ("e", &[CloseMidFront])]),
    ("ce", ONLY_LATAM, &[("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("ce", ONLY_SPAIN, &[("c", &[VoicelessDentalFricativeEsSpain]), ("e", &[CloseMidFront])]),
    ("de", ALL, &[("d", &[VoicedAlveolarPlosive]), ("e", &[CloseMidFront])]),
    ("e", ALL, &[("e", &[CloseMidFront])]),
    ("efe", ALL, &[("e", &[CloseMidFront]), ("f", &[VoicelessLabiodentalFricative]), ("e", &[CloseMidFront])]),
    ("ge", ALL, &[("g", &[VoicelessVelarFricative]), ("e", &[CloseMidFront])]),
    ("hache", ALL, &[("ha", &[OpenCentral]), ("ch", &[VoicelessPostalveolarAffricate]), ("e", &[CloseMidFront])]),
    ("i", ALL, &[("i", &[CloseFront])]),
    ("jota", ALL, &[("j", &[VoicelessVelarFricative]), ("o", &[CloseMidBackRounded]), ("t", &[VoicelessAlveolarPlosive]), ("a", &[OpenCentral])]),
    ("ka", ALL, &[("k", &[VoicelessVelarPlosive]), ("a", &[OpenCentral])]),
    ("ele", ALL, &[("e", &[CloseMidFront]), ("l", &[AlveolarLateralApproximant]), ("e", &[CloseMidFront])]),
    ("eme", ALL, &[("e", &[CloseMidFront]), ("m", &[BilabialNasal]), ("e", &[CloseMidFront])]),
    ("ene", ALL, &[("e", &[CloseMidFront]), ("n", &[AlveolarNasal]), ("e", &[CloseMidFront])]),
    ("eñe", ALL, &[("e", &[CloseMidFront]), ("ñ", &[PalatalNasal]), ("e", &[CloseMidFront])]),
    ("o", ALL, &[("o", &[CloseMidBackRounded])]),
    ("pe", ALL, &[("p", &[VoicelessBilabialPlosive]), ("e", &[CloseMidFront])]),
    ("cu", ALL, &[("c", &[VoicelessVelarPlosive]), ("u", &[CloseBackRounded])]),
    ("ere", ALL, &[("e", &[CloseMidFront]), ("r", &[AlveolarTap]), ("e", &[CloseMidFront])]),
    ("ese", ALL, &[("e", &[CloseMidFront]), ("s", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("te", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("e", &[CloseMidFront])]),
    ("u", ALL, &[("u", &[CloseBackRounded])]),
    ("uve", ALL, &[("u", &[CloseBackRounded]), ("v", &[VoicedBilabialPlosive]), ("e", &[CloseMidFront])]),
    ("uve-doble", ALL, &[("u", &[CloseBackRounded]), ("v", &[VoicedBilabialPlosive]), ("e", &[CloseMidFront]), ("-", &[]), ("d", &[VoicedAlveolarPlosive]), ("o", &[CloseMidBackRounded]), ("b", &[VoicedBilabialPlosive]), ("l", &[AlveolarLateralApproximant]), ("e", &[CloseMidFront])]),
    ("equis", ALL, &[("e", &[CloseMidFront]), ("qu", &[VoicelessVelarPlosive]), ("i", &[CloseFront]), ("s", &[VoicelessAlveolarFricative])]),
    ("i-griega", ALL, &[("i", &[CloseFront]), ("-", &[]), ("g", &[VoicedVelarPlosive]), ("r", &[AlveolarTap]), ("ie", &[PalatalApproximant, CloseMidFront]), ("ga", &[VoicedVelarApproximant, OpenCentral])]),
    ("zeta", ALL, &[("z", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront]), ("t", &[VoicelessAlveolarPlosive]), ("a", &[OpenCentral])]),

    // Números
    ("cero", ALL, &[("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront]), ("r", &[AlveolarTap]), ("o", &[CloseMidBackRounded])]),
    ("uno", ALL, &[("u", &[CloseBackRounded]), ("n", &[AlveolarNasal]), ("o", &[CloseMidBackRounded])]),
    ("dos", ALL, &[("d", &[VoicedAlveolarPlosive]), ("o", &[CloseMidBackRounded]), ("s", &[VoicelessAlveolarFricative])]),
    ("tres", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[AlveolarTap]), ("e", &[CloseMidFront]), ("s", &[VoicelessAlveolarFricative])]),
    ("cuatro", ALL, &[("c", &[VoicelessVelarPlosive]), ("u", &[LabialVelarApproximant]), ("a", &[OpenCentral]), ("t", &[VoicelessAlveolarPlosive]), ("r", &[AlveolarTap]), ("o", &[CloseMidBackRounded])]),
    ("cinco", ALL, &[("c", &[VoicelessAlveolarFricative]), ("i", &[CloseFront]), ("n", &[AlveolarNasal]), ("c", &[VoicelessVelarPlosive]), ("o", &[CloseMidBackRounded])]),
    ("seis", ALL, &[("s", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront]), ("i", &[PalatalApproximant]), ("s", &[VoicelessAlveolarFricative])]),
    ("siete", ALL, &[("s", &[VoicelessAlveolarFricative]), ("ie", &[PalatalApproximant, CloseMidFront]), ("t", &[VoicelessAlveolarPlosive]), ("e", &[CloseMidFront])]),
    ("ocho", ALL, &[("o", &[CloseMidBackRounded]), ("ch", &[VoicelessPostalveolarAffricate]), ("o", &[CloseMidBackRounded])]),
    ("nueve", ALL, &[("n", &[AlveolarNasal]), ("ue", &[LabialVelarApproximant, CloseMidFront]), ("v", &[VoicedBilabialApproximant]), ("e", &[CloseMidFront])]),
    ("diez", ALL, &[("d", &[VoicedAlveolarPlosive]), ("ie", &[PalatalApproximant, CloseMidFront]), ("z", &[VoicelessAlveolarFricative])]),
    ("once", ALL, &[("o", &[CloseMidBackRounded]), ("n", &[AlveolarNasal]), ("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("doce", ALL, &[("d", &[VoicedAlveolarPlosive]), ("o", &[CloseMidBackRounded]), ("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("trece", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[AlveolarTap]), ("e", &[CloseMidFront]), ("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("catorce", ALL, &[("c", &[VoicelessVelarPlosive]), ("a", &[OpenCentral]), ("t", &[VoicelessAlveolarPlosive]), ("o", &[CloseMidBackRounded]), ("r", &[AlveolarTap]), ("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("quince", ALL, &[("qu", &[VoicelessVelarPlosive]), ("i", &[CloseFront]), ("n", &[AlveolarNasal]), ("c", &[VoicelessAlveolarFricative]), ("e", &[CloseMidFront])]),
    ("veinte", ALL, &[("v", &[VoicedBilabialPlosive]), ("e", &[CloseMidFront]), ("i", &[PalatalApproximant]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive]), ("e", &[CloseMidFront])]),
    ("treinta", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[AlveolarTap]), ("e", &[CloseMidFront]), ("i", &[PalatalApproximant]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive]), ("a", &[OpenCentral])]),
    ("cien", ALL, &[("c", &[VoicelessAlveolarFricative]), ("ie", &[PalatalApproximant, CloseMidFront]), ("n", &[AlveolarNasal])]),
    ("ciento", ALL, &[("c", &[VoicelessAlveolarFricative]), ("ie", &[PalatalApproximant, CloseMidFront]), ("n", &[AlveolarNasal]), ("t", &[VoicelessAlveolarPlosive]), ("o", &[CloseMidBackRounded])]),
    ("mil", ALL, &[("m", &[BilabialNasal]), ("i", &[CloseFront]), ("l", &[AlveolarLateralApproximant])]),
    ("y", ALL, &[("y", &[CloseFront])]),
];

#[rustfmt::skip]
pub static EXCEPTIONS: DictTable = &[
    // Distinción España : z/ce/ci extraña los términos comunes. Mots
    // empruntés : x méxicaine ≈ x dans México, Oaxaca…
    ("méxico", ONLY_SPAIN, &[("m", &[BilabialNasal]), ("é", &[CloseMidFront]), ("x", &[VoicelessVelarFricative]), ("i", &[CloseFront]), ("c", &[VoicelessVelarPlosive]), ("o", &[CloseMidBackRounded])]),
    ("méxico", ONLY_LATAM, &[("m", &[BilabialNasal]), ("é", &[CloseMidFront]), ("x", &[VoicelessVelarFricative]), ("i", &[CloseFront]), ("c", &[VoicelessVelarPlosive]), ("o", &[CloseMidBackRounded])]),
    ("examen", ALL, &[("e", &[CloseMidFront]), ("x", &[VoicelessVelarPlosive, VoicelessAlveolarFricative]), ("a", &[OpenCentral]), ("m", &[BilabialNasal]), ("e", &[CloseMidFront]), ("n", &[AlveolarNasal])]),
];

pub static PRONOUNCEABLE_ACRONYMS: &[&str] = &[
    "onu", "otan", "unesco", "unicef", "ovni", "sida", "radar", "láser",
    "mercosur", "unam", "dni",
];
