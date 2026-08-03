//! Données linguistiques françaises : règles graphème→phonème,
//! dictionnaire de prononciation, exceptions, acronymes.
//!
//! Modèle de règles : balayage gauche→droite, correspondance la plus longue
//! d'abord ; à longueur égale, la première règle de la table gagne. Les
//! règles contextuelles doivent donc précéder les règles génériques de même
//! longueur. Les phonèmes `optional` représentent les liaisons/e caducs
//! proposés mais non générés par défaut.

use crate::phonetics::g2p::rules::{Ctx, Dialects, DictTable, GraphemeRule};
use crate::phonetics::phoneme::Phoneme;
use crate::phonetics::phoneme::Phoneme::*;

const A: Ctx = Ctx::Any;
const S: Ctx = Ctx::Start;
const E: Ctx = Ctx::End;
const V: Ctx = Ctx::Vowel;
const C: Ctx = Ctx::Consonant;
const FV: Ctx = Ctx::FrontVowel;
const BV: Ctx = Ctx::BackVowel;

const fn r(g: &'static str, l: Ctx, rt: Ctx, p: &'static [Phoneme]) -> GraphemeRule {
    GraphemeRule::rule(g, l, rt, p)
}
/// Règle produisant des phonèmes optionnels (liaisons proposées).
const fn ro(g: &'static str, l: Ctx, rt: Ctx, p: &'static [Phoneme]) -> GraphemeRule {
    GraphemeRule::optional(g, l, rt, p)
}

#[rustfmt::skip]
pub static RULES: &[GraphemeRule] = &[
    // ═══ Quadrigrammes et plus ═══════════════════════════════════════════
    r("aient", A, E, &[OpenMidFront]),
    r("oyen", A, E, &[LabialVelarApproximant, OpenCentral, PalatalApproximant, NasalOpenMidFrontFr]),
    r("eaux", A, E, &[CloseMidBackRounded]),
    r("tion", A, A, &[VoicelessAlveolarFricative, PalatalApproximant, NasalOpenMidBackFr]),
    r("stion", A, A, &[VoicelessAlveolarFricative, VoicelessAlveolarPlosive, PalatalApproximant, NasalOpenMidBackFr]),
    r("iens", A, E, &[PalatalApproximant, NasalOpenMidFrontFr]),
    r("ient", A, E, &[PalatalApproximant, NasalOpenMidFrontFr]),
    r("emps", A, E, &[NasalOpenBackFr]),
    r("uë", A, A, &[CloseFrontRounded]), // ambiguë, exiguë

    // ═══ Trigrammes vocaliques/nasaux ════════════════════════════════════
    r("eau", A, A, &[CloseMidBackRounded]),
    r("aon", A, A, &[NasalOpenBackFr]),
    r("aen", A, A, &[NasalOpenBackFr]),
    r("oin", A, A, &[LabialVelarApproximant, NasalOpenMidFrontFr]),
    r("oui", A, A, &[LabialVelarApproximant, CloseFront]),
    r("aîn", A, A, &[OpenMidFront, NasalOpenMidFrontFr]),
    r("eîn", A, A, &[OpenMidFront, NasalOpenMidFrontFr]),
    // Dénasalisation devant voyelle (aimable, reine, inutile, une, ananas)
    r("ain", A, V, &[OpenMidFront, AlveolarNasal]),
    r("aim", A, V, &[OpenMidFront, BilabialNasal]),
    r("ein", A, V, &[OpenMidFront, AlveolarNasal]),
    r("eim", A, V, &[OpenMidFront, BilabialNasal]),
    r("ien", A, V, &[CloseFront, OpenMidFront, AlveolarNasal]),
    r("yen", A, E, &[PalatalApproximant, NasalOpenMidFrontFr]),
    r("ain", A, A, &[NasalOpenMidFrontFr]),
    r("aim", A, A, &[NasalOpenMidFrontFr]),
    r("ein", A, A, &[NasalOpenMidFrontFr]),
    r("eim", A, A, &[NasalOpenMidFrontFr]),
    r("ien", A, E, &[PalatalApproximant, NasalOpenMidFrontFr]),
    // Voyelle+i/l finaux mouillés : travail, soleil, œil, cueil
    r("ail", A, E, &[OpenCentral, PalatalApproximant]),
    r("eil", A, E, &[OpenMidFront, PalatalApproximant]),
    r("euil", A, A, &[OpenMidFrontRounded, PalatalApproximant]),
    r("ueil", A, A, &[OpenMidFrontRounded, PalatalApproximant]),
    r("ill", V, A, &[PalatalApproximant]), // travaille, bouteille
    r("il", V, E, &[PalatalApproximant]),  // travail, fenouil
    r("aill", A, A, &[OpenCentral, PalatalApproximant]),   // taille, paille
    r("eill", A, A, &[OpenMidFront, PalatalApproximant]),  // vieille
    r("euill", A, A, &[OpenMidFrontRounded, PalatalApproximant]), // feuille
    r("ouill", A, A, &[CloseBackRounded, PalatalApproximant]),    // grenouille
    r("ueill", A, A, &[OpenMidFrontRounded, PalatalApproximant]), // cueille

    // ═══ e combiné : -er, -ez, -et, -ai, élidé/le syllabe initiale ═══════
    r("ent", A, E, &[]),              // 3e pers. plur. + adverbes -ent (dic pour -ent nominaux)
    r("ens", A, E, &[]),
    r("er", A, E, &[CloseMidFront]),
    r("ez", A, E, &[CloseMidFront]),
    r("ier", A, E, &[PalatalApproximant, CloseMidFront]),
    r("ers", A, E, &[OpenMidFront, VoicedUvularFricative]), // vers, enfers
    r("er", A, C, &[OpenMidFront, VoicedUvularFricative]),  // herbe, merci, perdre
    r("et", A, E, &[OpenMidFront]),
    r("ais", A, E, &[OpenMidFront]),
    r("ait", A, E, &[OpenMidFront]),
    r("ai", A, A, &[OpenMidFront]),
    r("aî", A, A, &[OpenMidFront]),
    r("ei", A, A, &[OpenMidFront]),
    r("ell", A, A, &[OpenMidFront, AlveolarLateralApproximant]), // belle
    r("ett", A, A, &[OpenMidFront, VoicelessAlveolarPlosive]),   // jette
    r("ess", A, A, &[OpenMidFront, VoicelessAlveolarFricative]), // presse
    r("enn", A, A, &[OpenMidFront, AlveolarNasal]),              // chienne (ié remonté)
    r("emm", A, A, &[OpenMidFront, BilabialNasal]),
    r("err", A, A, &[OpenMidFront, VoicedUvularFricative]),      // verre, erreur

    // ═══ Nasales digrammes : dénasalisation puis défaut nasal ════════════
    r("an", A, V, &[OpenCentral, AlveolarNasal]),
    r("am", A, V, &[OpenCentral, BilabialNasal]),
    r("en", A, V, &[OpenMidFront, AlveolarNasal]),
    r("em", A, V, &[OpenMidFront, BilabialNasal]),
    r("in", A, V, &[CloseFront, AlveolarNasal]),
    r("im", A, V, &[CloseFront, BilabialNasal]),
    r("yn", A, V, &[CloseFront, AlveolarNasal]),
    r("ym", A, V, &[CloseFront, BilabialNasal]),
    r("on", A, V, &[OpenMidBackRounded, AlveolarNasal]),
    r("om", A, V, &[OpenMidBackRounded, BilabialNasal]),
    r("un", A, V, &[CloseFrontRounded, AlveolarNasal]),
    r("um", A, V, &[CloseFrontRounded, BilabialNasal]),
    r("ann", A, A, &[OpenCentral, AlveolarNasal]),
    r("amm", A, A, &[OpenCentral, BilabialNasal]),
    r("enn", A, A, &[OpenMidFront, AlveolarNasal]),
    r("emm", A, A, &[OpenMidFront, BilabialNasal]),
    r("inn", A, A, &[CloseFront, AlveolarNasal]),
    r("imm", A, A, &[CloseFront, BilabialNasal]),
    r("ynn", A, A, &[CloseFront, AlveolarNasal]),
    r("omm", A, A, &[OpenMidBackRounded, BilabialNasal]),
    r("onn", A, A, &[OpenMidBackRounded, AlveolarNasal]),
    r("unn", A, A, &[CloseFrontRounded, AlveolarNasal]),
    r("an", A, A, &[NasalOpenBackFr]),
    r("am", A, A, &[NasalOpenBackFr]),
    r("en", A, A, &[NasalOpenBackFr]),
    r("em", A, A, &[NasalOpenBackFr]),
    r("in", A, A, &[NasalOpenMidFrontFr]),
    r("im", A, A, &[NasalOpenMidFrontFr]),
    r("yn", A, A, &[NasalOpenMidFrontFr]),
    r("ym", A, A, &[NasalOpenMidFrontFr]),
    r("un", A, A, &[NasalOpenMidFrontRoundedFr]),
    r("um", A, A, &[NasalOpenMidFrontRoundedFr]),
    r("on", A, A, &[NasalOpenMidBackFr]),
    r("om", A, A, &[NasalOpenMidBackFr]),

    // ═══ Digrammes vocaliques ════════════════════════════════════════════
    r("au", A, A, &[CloseMidBackRounded]),
    r("eu", A, E, &[CloseMidFrontRounded]),   // peu, deux
    r("eux", A, E, &[CloseMidFrontRounded]),  // heureux
    r("eu", A, A, &[OpenMidFrontRounded]),    // peur, neuf
    r("œu", A, A, &[OpenMidFrontRounded]),
    r("oeu", A, A, &[OpenMidFrontRounded]),
    r("ou", A, A, &[CloseBackRounded]),
    r("oû", A, A, &[CloseBackRounded]),
    r("oi", A, A, &[LabialVelarApproximant, OpenCentral]),
    r("oî", A, A, &[LabialVelarApproximant, OpenCentral]),
    r("oy", A, A, &[LabialVelarApproximant, OpenCentral, PalatalApproximant]),
    r("ui", A, A, &[LabialPalatalApproximant, CloseFront]),
    r("uî", A, A, &[LabialPalatalApproximant, CloseFront]),
    r("ie", C, E, &[CloseFront]), // vie, crie, lie
    r("ie", A, A, &[CloseFront]), // chérie, partie (approché)
    r("ée", A, A, &[CloseMidFront]),

    // ═══ Voyelles accentuées/simples ═════════════════════════════════════
    r("é", A, A, &[CloseMidFront]),
    r("è", A, A, &[OpenMidFront]),
    r("ê", A, A, &[OpenMidFront]),
    r("ë", A, A, &[OpenMidFront]),
    r("à", A, A, &[OpenCentral]),
    r("â", A, A, &[OpenBackFr]),
    r("î", A, A, &[CloseFront]),
    r("ï", A, A, &[CloseFront]),
    r("ô", A, A, &[CloseMidBackRounded]),
    r("û", A, A, &[CloseFrontRounded]),
    r("ù", A, A, &[CloseFrontRounded]),
    r("ü", A, A, &[CloseFrontRounded]),
    r("ÿ", A, A, &[CloseFront]),
    r("æ", A, A, &[OpenMidFront]),
    r("œ", A, A, &[OpenMidFrontRounded]),

    // ═══ Groupes consonantiques ══════════════════════════════════════════
    r("tch", A, A, &[VoicelessPostalveolarAffricate]), // match, sandwich
    r("dj", A, A, &[VoicedPostalveolarAffricate]), // emprunts: djembé, djihad
    r("sch", A, A, &[VoicelessPostalveolarFricative]),
    r("sh", A, A, &[VoicelessPostalveolarFricative]),
    r("ch", A, C, &[VoicelessVelarPlosive]),  // orchestre, technique, chrétien
    r("ch", A, A, &[VoicelessPostalveolarFricative]),
    r("ph", A, A, &[VoicelessLabiodentalFricative]),
    r("th", A, A, &[VoicelessAlveolarPlosive]),
    r("gn", A, A, &[PalatalNasal]),
    r("qu", A, A, &[VoicelessVelarPlosive]),
    r("gu", A, V, &[VoicedVelarPlosive]),
    r("gü", A, V, &[VoicedVelarPlosive]),
    r("sc", A, FV, &[VoicelessAlveolarFricative]),
    r("sc", A, A, &[VoicelessAlveolarFricative, VoicelessVelarPlosive]),
    r("ing", A, E, &[CloseFront, VelarNasal]),  // parking, shopping
    r("ng", A, E, &[VelarNasal]),
    r("ng", A, A, &[VelarNasal, VoicedVelarPlosive]), // angoisse
    r("xc", A, FV, &[VoicelessVelarPlosive, VoicelessAlveolarFricative]), // excellent
    r("ex", S, V, &[OpenMidFront, VoicedVelarPlosive, VoicedAlveolarFricative]), // examen, exister
    r("x", A, A, &[VoicelessVelarPlosive, VoicelessAlveolarFricative]),
    r("cc", A, FV, &[VoicelessVelarPlosive, VoicelessAlveolarFricative]), // accident
    r("cc", A, A, &[VoicelessVelarPlosive]),
    r("c", A, FV, &[VoicelessAlveolarFricative]),
    r("c", A, A, &[VoicelessVelarPlosive]),
    r("ç", A, A, &[VoicelessAlveolarFricative]),
    r("ge", A, BV, &[VoicedPostalveolarFricative]), // nous mangeons, mangea
    r("g", A, FV, &[VoicedPostalveolarFricative]),
    r("g", A, A, &[VoicedVelarPlosive]),
    r("j", A, A, &[VoicedPostalveolarFricative]),
    r("s", V, V, &[VoicedAlveolarFricative]),
    r("s", A, E, &[]) , // s final muet (liaison proposée via ro ci-dessous ? non : muet pur)
    r("s", A, A, &[VoicelessAlveolarFricative]),
    r("ps", S, A, &[VoicelessAlveolarFricative]), // psychologie
    r("h", A, A, &[]), // h muet / aspiré (l'aspiré bloque la liaison : info lexicale)
    r("y", A, V, &[PalatalApproximant]),
    r("y", A, A, &[CloseFront]),
    r("w", A, A, &[LabialVelarApproximant]),
    r("k", A, A, &[VoicelessVelarPlosive]),
    r("z", A, E, &[]), // chez, nez (rajeuni par dic pour nez… nez = muet ✓)
    r("z", A, A, &[VoicedAlveolarFricative]),
    r("i", C, V, &[PalatalApproximant]), // piano, social, bien tué
    r("ll", A, A, &[AlveolarLateralApproximant]),
    r("tt", A, A, &[VoicelessAlveolarPlosive]),
    r("pp", A, A, &[VoicelessBilabialPlosive]),
    r("mm", A, A, &[BilabialNasal]),
    r("nn", A, A, &[AlveolarNasal]),
    r("ff", A, A, &[VoicelessLabiodentalFricative]),
    r("ss", A, A, &[VoicelessAlveolarFricative]),
    r("rr", A, A, &[VoicedUvularFricative]),
    r("zz", A, A, &[VoicedAlveolarFricative]),
    r("dd", A, A, &[VoicedAlveolarPlosive]),
    r("bb", A, A, &[VoicedBilabialPlosive]),
    r("gg", A, A, &[VoicedVelarPlosive]),
    r("vv", A, A, &[VoicedLabiodentalFricative]),

    // ═══ Finales muettes / letter de liaison (optional) ══════════════════
    r("ds", A, E, &[]),
    r("ts", A, E, &[]),
    r("ps", A, E, &[]),
    r("bs", A, E, &[]),
    r("gs", A, E, &[]),
    ro("t", A, E, &[VoicelessAlveolarPlosive]),  // liaison : petit ‿ami
    ro("d", A, E, &[VoicelessAlveolarPlosive]),  // liaison durcie : quand ‿il
    ro("p", A, E, &[VoicelessBilabialPlosive]),  // trop ‿à faire
    ro("g", A, E, &[VoicelessVelarPlosive]),     // sang ‿impur (rare), long ‿été
    ro("x", A, E, &[VoicedAlveolarFricative]),   // deux ‿enfants
    r("b", A, E, &[]),   // plomb, aplomb
    r("f", A, E, &[VoicelessLabiodentalFricative]), // CaReFuL : neuf, soif
    r("l", A, E, &[AlveolarLateralApproximant]),
    r("r", A, E, &[VoicedUvularFricative]),
    r("m", A, E, &[BilabialNasal]), // album, forum…
    r("n", A, E, &[AlveolarNasal]),
    r("q", A, E, &[VoicelessVelarPlosive]), // cinq, coq
    r("c", A, E, &[VoicelessVelarPlosive]), // sac, sec, avec
    r("e", A, E, &[]),  // e final muet (monosyllabes dans le dictionnaire)
    r("es", A, E, &[]),

    // ═══ Lettres simples ═════════════════════════════════════════════════
    r("a", A, A, &[OpenCentral]),
    r("i", A, A, &[CloseFront]),
    r("o", A, E, &[CloseMidBackRounded]), // piano, repos, mot
    r("o", A, A, &[OpenMidBackRounded]),
    r("u", A, A, &[CloseFrontRounded]),
    r("e", A, A, &[Schwa]), // e interne (caduc) — défaut approché
    r("l", A, A, &[AlveolarLateralApproximant]),
    r("r", A, A, &[VoicedUvularFricative]),
    r("f", A, A, &[VoicelessLabiodentalFricative]),
    r("v", A, A, &[VoicedLabiodentalFricative]),
    r("p", A, A, &[VoicelessBilabialPlosive]),
    r("b", A, A, &[VoicedBilabialPlosive]),
    r("m", A, A, &[BilabialNasal]),
    r("n", A, A, &[AlveolarNasal]),
    r("t", A, A, &[VoicelessAlveolarPlosive]),
    r("d", A, A, &[VoicedAlveolarPlosive]),
    r("q", A, A, &[VoicelessVelarPlosive]),
];

// ═══════════════════════════ Dictionnaire ═══════════════════════════════════
// Mots fréquents dont la prononciation est irrégulière ou dont les segments
// diffèrent des règles, et mots-outils (monosyllabes à e prononcé, mots à
// lettre finale prononcée…). Les lettres doivent se concaténer exactement en
// la clé (vérifié par les tests).

const ALL: Dialects = Dialects::All;

#[rustfmt::skip]
pub static DICTIONARY: DictTable = &[
    // ── Mots-outils à e/est/et prononcés ─────────────────────────────────
    ("le", ALL, &[("l", &[AlveolarLateralApproximant]), ("e", &[Schwa])]),
    ("me", ALL, &[("m", &[BilabialNasal]), ("e", &[Schwa])]),
    ("te", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("e", &[Schwa])]),
    ("se", ALL, &[("s", &[VoicelessAlveolarFricative]), ("e", &[Schwa])]),
    ("ne", ALL, &[("n", &[AlveolarNasal]), ("e", &[Schwa])]),
    ("ce", ALL, &[("c", &[VoicelessAlveolarFricative]), ("e", &[Schwa])]),
    ("de", ALL, &[("d", &[VoicedAlveolarPlosive]), ("e", &[Schwa])]),
    ("je", ALL, &[("j", &[VoicedPostalveolarFricative]), ("e", &[Schwa])]),
    ("que", ALL, &[("qu", &[VoicelessVelarPlosive]), ("e", &[Schwa])]),
    ("est", ALL, &[("es", &[OpenMidFront]), ("t", &[])]),
    ("et", ALL, &[("e", &[CloseMidFront]), ("t", &[])]),
    ("es", ALL, &[("e", &[CloseMidFront]), ("s", &[])]),
    ("les", ALL, &[("l", &[AlveolarLateralApproximant]), ("es", &[CloseMidFront])]),
    ("des", ALL, &[("d", &[VoicedAlveolarPlosive]), ("es", &[CloseMidFront])]),
    ("mes", ALL, &[("m", &[BilabialNasal]), ("es", &[CloseMidFront])]),
    ("tes", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("es", &[CloseMidFront])],
    ),
    ("ses", ALL, &[("s", &[VoicelessAlveolarFricative]), ("es", &[CloseMidFront])]),
    ("ces", ALL, &[("c", &[VoicelessAlveolarFricative]), ("es", &[CloseMidFront])]),
    ("en", ALL, &[("en", &[NasalOpenBackFr])]),
    ("on", ALL, &[("on", &[NasalOpenMidBackFr])]),
    ("une", ALL, &[("u", &[CloseFrontRounded]), ("n", &[AlveolarNasal]), ("e", &[])]),
    ("dans", ALL, &[("d", &[VoicedAlveolarPlosive]), ("an", &[NasalOpenBackFr]), ("s", &[])]),
    ("sans", ALL, &[("s", &[VoicelessAlveolarFricative]), ("an", &[NasalOpenBackFr]), ("s", &[])]),
    ("temps", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("emps", &[NasalOpenBackFr])]),
    ("monsieur", ALL, &[("m", &[BilabialNasal]), ("o", &[Schwa]), ("n", &[]), ("s", &[]), ("i", &[PalatalApproximant]), ("eu", &[CloseMidFrontRounded]), ("r", &[VoicedUvularFricative])]),
    ("madame", ALL, &[("m", &[BilabialNasal]), ("a", &[OpenCentral]), ("d", &[VoicedAlveolarPlosive]), ("a", &[OpenCentral]), ("m", &[BilabialNasal]), ("e", &[])]),
    ("mademoiselle", ALL, &[("m", &[BilabialNasal]), ("a", &[OpenCentral]), ("d", &[VoicedAlveolarPlosive]), ("e", &[Schwa]), ("m", &[BilabialNasal]), ("oi", &[LabialVelarApproximant, OpenCentral]), ("s", &[VoicedAlveolarFricative]), ("ell", &[OpenMidFront, AlveolarLateralApproximant]), ("e", &[])]),

    // ── Lettres finales prononcées (extension de CaReFuL) ────────────────
    ("mars", ALL, &[("m", &[BilabialNasal]), ("a", &[OpenCentral]), ("r", &[VoicedUvularFricative]), ("s", &[VoicelessAlveolarFricative])]),
    ("ours", ALL, &[("ou", &[CloseBackRounded]), ("r", &[VoicedUvularFricative]), ("s", &[VoicelessAlveolarFricative])]),
    ("os", ALL, &[("o", &[OpenMidBackRounded]), ("s", &[])]),
    ("plus", ALL, &[("p", &[VoicelessBilabialPlosive]), ("l", &[AlveolarLateralApproximant]), ("u", &[CloseFrontRounded]), ("s", &[])]),
    ("tous", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("ou", &[CloseBackRounded]), ("s", &[])]),
    ("but", ALL, &[("b", &[VoicedBilabialPlosive]), ("u", &[CloseFrontRounded]), ("t", &[VoicelessAlveolarPlosive])]),
    ("sud", ALL, &[("s", &[VoicelessAlveolarFricative]), ("u", &[CloseFrontRounded]), ("d", &[VoicedAlveolarPlosive])]),
    ("bus", ALL, &[("b", &[VoicedBilabialPlosive]), ("u", &[CloseFrontRounded]), ("s", &[VoicelessAlveolarFricative])]),
    ("maïs", ALL, &[("m", &[BilabialNasal]), ("a", &[OpenCentral]), ("ï", &[CloseFront]), ("s", &[VoicelessAlveolarFricative])]),
    ("bonus", ALL, &[("b", &[VoicedBilabialPlosive]), ("o", &[OpenMidBackRounded]), ("n", &[AlveolarNasal]), ("u", &[CloseFrontRounded]), ("s", &[VoicelessAlveolarFricative])]),
    ("virus", ALL, &[("v", &[VoicedLabiodentalFricative]), ("i", &[CloseFront]), ("r", &[VoicedUvularFricative]), ("u", &[CloseFrontRounded]), ("s", &[VoicelessAlveolarFricative])]),
    ("avis", ALL, &[("a", &[OpenCentral]), ("v", &[VoicedLabiodentalFricative]), ("i", &[CloseFront]), ("s", &[])]),

    // ── Nombres ──────────────────────────────────────────────────────────
    ("zéro", ALL, &[("z", &[VoicedAlveolarFricative]), ("é", &[CloseMidFront]), ("r", &[VoicedUvularFricative]), ("o", &[CloseMidBackRounded])]),
    ("un", ALL, &[("un", &[NasalOpenMidFrontRoundedFr])]),
    ("deux", ALL, &[("d", &[VoicedAlveolarPlosive]), ("eu", &[CloseMidFrontRounded]), ("x", &[])]),
    ("trois", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("oi", &[LabialVelarApproximant, OpenCentral]), ("s", &[])]),
    ("quatre", ALL, &[("qu", &[VoicelessVelarPlosive]), ("a", &[OpenCentral]), ("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("e", &[])]),
    ("cinq", ALL, &[("c", &[VoicelessAlveolarFricative]), ("in", &[NasalOpenMidFrontFr]), ("q", &[VoicelessVelarPlosive])]),
    ("six", ALL, &[("s", &[VoicelessAlveolarFricative]), ("i", &[CloseFront]), ("x", &[VoicelessAlveolarFricative])]),
    ("sept", ALL, &[("s", &[VoicelessAlveolarFricative]), ("e", &[OpenMidFront]), ("pt", &[VoicelessAlveolarPlosive])]),
    ("huit", ALL, &[("hu", &[LabialPalatalApproximant]), ("i", &[CloseFront]), ("t", &[])]),
    ("neuf", ALL, &[("n", &[AlveolarNasal]), ("eu", &[OpenMidFrontRounded]), ("f", &[VoicelessLabiodentalFricative])]),
    ("dix", ALL, &[("d", &[VoicedAlveolarPlosive]), ("i", &[CloseFront]), ("x", &[VoicelessAlveolarFricative])]),
    ("onze", ALL, &[("on", &[NasalOpenMidBackFr]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("douze", ALL, &[("d", &[VoicedAlveolarPlosive]), ("ou", &[CloseBackRounded]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("treize", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("ei", &[OpenMidFront]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("quatorze", ALL, &[("qu", &[VoicelessVelarPlosive]), ("a", &[OpenCentral]), ("t", &[VoicelessAlveolarPlosive]), ("o", &[OpenMidBackRounded]), ("r", &[VoicedUvularFricative]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("quinze", ALL, &[("qu", &[VoicelessVelarPlosive]), ("in", &[NasalOpenMidFrontFr]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("seize", ALL, &[("s", &[VoicelessAlveolarFricative]), ("ei", &[OpenMidFront]), ("z", &[VoicedAlveolarFricative]), ("e", &[])]),
    ("vingt", ALL, &[("v", &[VoicedLabiodentalFricative]), ("in", &[NasalOpenMidFrontFr]), ("gt", &[])]),
    ("vingts", ALL, &[("v", &[VoicedLabiodentalFricative]), ("in", &[NasalOpenMidFrontFr]), ("gts", &[])]),
    ("trente", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("ent", &[NasalOpenBackFr]), ("e", &[])]),
    ("quarante", ALL, &[("qu", &[VoicelessVelarPlosive]), ("a", &[OpenCentral]), ("r", &[VoicedUvularFricative]), ("ant", &[NasalOpenBackFr]), ("e", &[])]),
    ("cinquante", ALL, &[("c", &[VoicelessAlveolarFricative]), ("in", &[NasalOpenMidFrontFr]), ("qu", &[VoicelessVelarPlosive]), ("ant", &[NasalOpenBackFr]), ("e", &[])]),
    ("soixante", ALL, &[("s", &[VoicelessAlveolarFricative]), ("oi", &[LabialVelarApproximant, OpenCentral]), ("x", &[VoicelessAlveolarFricative]), ("ant", &[NasalOpenBackFr]), ("e", &[])]),
    ("cent", ALL, &[("c", &[VoicelessAlveolarFricative]), ("en", &[NasalOpenBackFr]), ("t", &[])]),
    ("cents", ALL, &[("c", &[VoicelessAlveolarFricative]), ("en", &[NasalOpenBackFr]), ("ts", &[])]),
    ("mille", ALL, &[("m", &[BilabialNasal]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("e", &[])]),
    ("million", ALL, &[("m", &[BilabialNasal]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("i", &[CloseFront]), ("on", &[NasalOpenMidBackFr])]),
    ("milliard", ALL, &[("m", &[BilabialNasal]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("i", &[CloseFront, OpenCentral]), ("ar", &[OpenCentral, VoicedUvularFricative]), ("d", &[])]),
    ("centième", ALL, &[("c", &[VoicelessAlveolarFricative]), ("en", &[NasalOpenBackFr]), ("t", &[VoicelessAlveolarPlosive]), ("i", &[CloseFront]), ("è", &[OpenMidFront]), ("m", &[BilabialNasal]), ("e", &[])]),
    ("premier", ALL, &[("p", &[VoicelessBilabialPlosive]), ("r", &[VoicedUvularFricative]), ("e", &[Schwa]), ("m", &[BilabialNasal]), ("ier", &[PalatalApproximant, CloseMidFront])]),
    ("première", ALL, &[("p", &[VoicelessBilabialPlosive]), ("r", &[VoicedUvularFricative]), ("e", &[Schwa]), ("m", &[BilabialNasal]), ("i", &[PalatalApproximant]), ("è", &[OpenMidFront]), ("r", &[VoicedUvularFricative]), ("e", &[])]),
    ("deuxième", ALL, &[("d", &[VoicedAlveolarPlosive]), ("eu", &[CloseMidFrontRounded]), ("x", &[VoicedAlveolarFricative]), ("i", &[PalatalApproximant]), ("è", &[OpenMidFront]), ("m", &[BilabialNasal]), ("e", &[])]),
    ("troisième", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("oi", &[LabialVelarApproximant, OpenCentral]), ("s", &[VoicedAlveolarFricative]), ("i", &[PalatalApproximant]), ("è", &[OpenMidFront]), ("m", &[BilabialNasal]), ("e", &[])]),

    // ── Noms de lettres (pour acronymes) ─────────────────────────────────
    ("a", ALL, &[("a", &[OpenCentral])]),
    ("bé", ALL, &[("b", &[VoicedBilabialPlosive]), ("é", &[CloseMidFront])]),
    ("cé", ALL, &[("c", &[VoicelessAlveolarFricative]), ("é", &[CloseMidFront])]),
    ("dé", ALL, &[("d", &[VoicedAlveolarPlosive]), ("é", &[CloseMidFront])]),
    ("e", ALL, &[("e", &[Schwa])]),
    ("effe", ALL, &[("eff", &[OpenMidFront, VoicelessLabiodentalFricative]), ("e", &[])]),
    ("gé", ALL, &[("g", &[VoicedPostalveolarFricative]), ("é", &[CloseMidFront])]),
    ("ache", ALL, &[("a", &[OpenCentral]), ("ch", &[VoicelessPostalveolarFricative]), ("e", &[])]),
    ("i", ALL, &[("i", &[CloseFront])]),
    ("ji", ALL, &[("j", &[VoicedPostalveolarFricative]), ("i", &[CloseFront])]),
    ("ka", ALL, &[("k", &[VoicelessVelarPlosive]), ("a", &[OpenCentral])]),
    ("elle", ALL, &[("ell", &[OpenMidFront, AlveolarLateralApproximant]), ("e", &[])]),
    ("emme", ALL, &[("emm", &[OpenMidFront, BilabialNasal]), ("e", &[])]),
    ("enne", ALL, &[("enn", &[OpenMidFront, AlveolarNasal]), ("e", &[])]),
    ("o", ALL, &[("o", &[CloseMidBackRounded])]),
    ("pé", ALL, &[("p", &[VoicelessBilabialPlosive]), ("é", &[CloseMidFront])]),
    ("cu", ALL, &[("c", &[VoicelessVelarPlosive]), ("u", &[CloseFrontRounded])]),
    ("erre", ALL, &[("err", &[OpenMidFront, VoicedUvularFricative]), ("e", &[])]),
    ("esse", ALL, &[("ess", &[OpenMidFront, VoicelessAlveolarFricative]), ("e", &[])]),
    ("té", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("é", &[CloseMidFront])]),
    ("u", ALL, &[("u", &[CloseFrontRounded])]),
    ("vé", ALL, &[("v", &[VoicedLabiodentalFricative]), ("é", &[CloseMidFront])]),
    ("double-vé", ALL, &[("d", &[VoicedAlveolarPlosive]), ("ou", &[CloseBackRounded]), ("b", &[VoicedBilabialPlosive]), ("l", &[AlveolarLateralApproximant]), ("e", &[Schwa]), ("-", &[]), ("v", &[VoicedLabiodentalFricative]), ("é", &[CloseMidFront])]),
    ("ixe", ALL, &[("i", &[CloseFront]), ("x", &[VoicelessVelarPlosive, VoicelessAlveolarFricative]), ("e", &[])]),
    ("i-grec", ALL, &[("i", &[CloseFront]), ("-", &[]), ("g", &[VoicedVelarPlosive]), ("r", &[VoicedUvularFricative]), ("e", &[OpenMidFront]), ("c", &[VoicelessVelarPlosive])]),
    ("zède", ALL, &[("z", &[VoicedAlveolarFricative]), ("è", &[OpenMidFront]), ("d", &[VoicedAlveolarPlosive]), ("e", &[])]),
    ("y", ALL, &[("y", &[CloseFront])]),
];

// ═══════════════════════════ Exceptions ════════════════════════════════════
// Mots irréguliers fréquents non grabables par les règles (lettres finales
// prononcées surprenantes, prononciations héritées, h aspiré documenté par le
// fait de ne pas marquer de liaison sur le mot précédent…).

#[rustfmt::skip]
pub static EXCEPTIONS: DictTable = &[
    // "est" direction cardinale (ɛst) vs verbe (ɛ) : deux variantes, la
    // première lue par défaut ici est le verbe (le dictionnaire l'a déjà) ;
    // la variante cardinal est exposée comme seconde candidate dans le dic.
    ("chef", ALL, &[("ch", &[VoicelessPostalveolarFricative]), ("e", &[OpenMidFront]), ("f", &[VoicelessLabiodentalFricative])]),
    ("cerf", ALL, &[("c", &[VoicelessAlveolarFricative]), ("er", &[OpenMidFront, VoicedUvularFricative]), ("f", &[VoicelessLabiodentalFricative])]),
    ("soif", ALL, &[("s", &[VoicelessAlveolarFricative]), ("oi", &[LabialVelarApproximant, OpenCentral]), ("f", &[VoicelessLabiodentalFricative])]),
    ("fil", ALL, &[("f", &[VoicelessLabiodentalFricative]), ("i", &[CloseFront]), ("l", &[AlveolarLateralApproximant])]),
    ("fils", ALL, &[("f", &[VoicelessLabiodentalFricative]), ("i", &[CloseFront]), ("l", &[AlveolarLateralApproximant]), ("s", &[VoicelessAlveolarFricative])]),
    ("mal", ALL, &[("m", &[BilabialNasal]), ("a", &[OpenCentral]), ("l", &[AlveolarLateralApproximant])]),
    ("ville", ALL, &[("v", &[VoicedLabiodentalFricative]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("e", &[])]),
    ("mille", ALL, &[("m", &[BilabialNasal]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("e", &[])]),
    ("tranquille", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("r", &[VoicedUvularFricative]), ("an", &[NasalOpenBackFr]), ("qu", &[VoicelessVelarPlosive]), ("i", &[CloseFront]), ("ll", &[AlveolarLateralApproximant]), ("e", &[])]),
    ("août", ALL, &[("ao", &[OpenCentral, CloseBackRounded]), ("û", &[]), ("t", &[])]),
    ("tous", ALL, &[("t", &[VoicelessAlveolarPlosive]), ("ou", &[CloseBackRounded]), ("s", &[VoicelessAlveolarFricative])]),
];

// ═══════════════════════════ Acronymes prononçables ════════════════════════
pub static PRONOUNCEABLE_ACRONYMS: &[&str] = &[
    "nasa", "otan", "onu", "ovni", "sida", "unesco", "unicef", "radar", "laser", "smic", "rsa",
    "bts", "cap", "insee", "inria", "cnes", "hal", "samu", "taser", "gif", "ratp", "crs", "ens",
    "fnac",
];
