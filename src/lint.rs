//! Diagnostics non persistants des conventions de la bande rythmo.

use crate::{
    project::Project,
    rythmo_layout::track_index_for_y_slot,
    rythmo_line::{MarkerKind, RythmoLine},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    ReactionInAngles,
    UnbracketedReaction,
    LongLoop,
    TooLongLoop,
    CharacterOnMultipleTracks,
    MixedVoicePresence,
    MissingFinalPunctuation,
    AmbianceParentheses,
    TextEmotion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Line(u64),
    Zone { start_frame: i64, end_frame: i64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub rule: Rule,
    pub scope: Scope,
    pub message: &'static str,
}

impl Diagnostic {
    pub fn spoken(&self) -> String {
        format!(
            "{} : {}",
            if self.severity == Severity::Error {
                "Erreur"
            } else {
                "Avertissement"
            },
            self.message
        )
    }

    pub fn label(&self) -> &'static str {
        match self.severity {
            Severity::Warning => "Avertissement :",
            Severity::Error => "Non conforme :",
        }
    }
}

const KNOWN_REACTIONS: &[&str] = &[
    "x", "mts", "tsc", "ah", "oh", "ih", "mhm", "hm", "ptt", "pff", "unh", "hun", "psst",
];

pub fn analyze(project: &Project, fps: f64) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let lines: Vec<_> = project.lines().collect();
    for line in &lines {
        lint_text(line, &mut out);
    }
    lint_character_tracks(&lines, &mut out);
    lint_voice_presence(&lines, &mut out);
    lint_loops(project, fps, &mut out);
    out
}

pub fn for_line(project: &Project, fps: f64, line_id: u64) -> Vec<Diagnostic> {
    let Some(line) = project.get_line(line_id) else {
        return Vec::new();
    };
    let diagnostics = analyze(project, fps);
    for_line_in(&diagnostics, line)
}

pub fn for_line_in(diagnostics: &[Diagnostic], line: &RythmoLine) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .cloned()
        .filter(|d| match d.scope {
            Scope::Line(id) => id == line.id,
            Scope::Zone {
                start_frame,
                end_frame,
            } => line.start_frame < end_frame && line.end_frame() > start_frame,
        })
        .collect()
}

pub fn line_description_suffix(project: &Project, fps: f64, line_id: u64) -> Option<String> {
    let diagnostics = for_line(project, fps, line_id);
    (!diagnostics.is_empty()).then(|| {
        diagnostics
            .iter()
            .map(Diagnostic::spoken)
            .collect::<Vec<_>>()
            .join(". ")
    })
}

fn add(out: &mut Vec<Diagnostic>, id: u64, severity: Severity, rule: Rule, message: &'static str) {
    out.push(Diagnostic {
        severity,
        rule,
        scope: Scope::Line(id),
        message,
    });
}

fn lint_text(line: &RythmoLine, out: &mut Vec<Diagnostic>) {
    if !line.text_emotions.is_empty() {
        add(
            out,
            line.id,
            Severity::Warning,
            Rule::TextEmotion,
            "N'utilisez pas d'émotions du texte dans un milieu professionnel qui ne l'autorise pas !",
        );
    }
    let text = line.text.trim();
    if text.is_empty() {
        return;
    }
    if line.kind.is_ambiance() {
        if !(text.starts_with('(') && text.ends_with(')')) {
            add(
                out,
                line.id,
                Severity::Error,
                Rule::AmbianceParentheses,
                "le contenu doit être entre parenthèses.",
            );
        }
        return;
    }
    if contains_angle_description(text) {
        add(out, line.id, Severity::Error, Rule::ReactionInAngles, "les descriptifs de réaction doivent être écrits entre crochets et parenthèses, pas entre piquants.");
    }
    if let Some(inner) = text.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        let normalized = inner.trim().to_lowercase();
        if !(inner.trim().starts_with('[') && inner.trim().ends_with(']'))
            && !KNOWN_REACTIONS.contains(&normalized.as_str())
        {
            add(out, line.id, Severity::Warning, Rule::UnbracketedReaction, "ce descriptif semble être une réaction ; écrivez-le entre crochets et parenthèses, par exemple ([Pleurs]).");
        }
        return;
    }
    if text.chars().any(char::is_alphabetic) && !has_final_punctuation(text) {
        add(
            out,
            line.id,
            Severity::Warning,
            Rule::MissingFinalPunctuation,
            "une phrase normale doit se terminer par un signe de ponctuation.",
        );
    }
}

fn lint_character_tracks(lines: &[&RythmoLine], out: &mut Vec<Diagnostic>) {
    let mut tracks: HashMap<String, HashSet<usize>> = HashMap::new();
    for line in lines.iter().copied().filter(|line| line.kind.is_dialogue()) {
        let name = line.character_name.trim().to_lowercase();
        if !name.is_empty() {
            tracks
                .entry(name)
                .or_default()
                .insert(track_index_for_y_slot(line.y_slot));
        }
    }
    for line in lines.iter().copied().filter(|line| line.kind.is_dialogue()) {
        let name = line.character_name.trim().to_lowercase();
        if !name.is_empty() && tracks.get(&name).is_some_and(|set| set.len() > 1) {
            add(out, line.id, Severity::Warning, Rule::CharacterOnMultipleTracks, "ce personnage apparaît sur plusieurs lignes ; gardez-lui la même ligne dans la mesure du possible.");
        }
    }
}

fn lint_voice_presence(lines: &[&RythmoLine], out: &mut Vec<Diagnostic>) {
    let mut modes: HashMap<String, (bool, bool)> = HashMap::new();
    for line in lines.iter().copied().filter(|line| line.kind.is_dialogue()) {
        let name = line.character_name.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        let mode = modes.entry(name).or_default();
        if is_voice_off(&line.note) {
            mode.0 = true
        } else {
            mode.1 = true
        }
    }
    for line in lines.iter().copied().filter(|line| line.kind.is_dialogue()) {
        let name = line.character_name.trim().to_lowercase();
        if !name.is_empty() && modes.get(&name) == Some(&(true, true)) {
            add(out, line.id, Severity::Warning, Rule::MixedVoicePresence, "ce personnage possède des répliques ON et OFF ; créez un personnage distinct pour la voice-over, par exemple « John V.O ».");
        }
    }
}

fn lint_loops(project: &Project, fps: f64, out: &mut Vec<Diagnostic>) {
    if !fps.is_finite() || fps <= 0.0 {
        return;
    }
    let mut starts: Vec<_> = project
        .markers()
        .iter()
        .filter(|m| m.kind == MarkerKind::Boucle)
        .map(|m| m.frame)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let content_end = project
        .lines()
        .map(RythmoLine::end_frame)
        .chain(project.markers().iter().map(|marker| marker.frame))
        .max()
        .unwrap_or(0);
    for (index, start) in starts.iter().copied().enumerate() {
        let next_loop = starts.get(index + 1).copied();
        let next_out = project
            .markers()
            .iter()
            .filter(|marker| marker.kind == MarkerKind::Out && marker.frame > start)
            .map(|marker| marker.frame)
            .min();
        let end = next_loop
            .into_iter()
            .chain(next_out)
            .min()
            .unwrap_or(content_end);
        if end <= start {
            continue;
        }
        let seconds = (end - start) as f64 / fps;
        let values = if seconds > 90.0 {
            Some((Severity::Error, Rule::TooLongLoop, "cette boucle dépasse une minute trente ; elle n’est pas conforme et doit être réduite."))
        } else if seconds > 60.0 {
            Some((Severity::Warning, Rule::LongLoop, "cette boucle dépasse une minute ; une boucle devrait idéalement durer environ une minute."))
        } else {
            None
        };
        if let Some((severity, rule, message)) = values {
            out.push(Diagnostic {
                severity,
                rule,
                scope: Scope::Zone {
                    start_frame: start,
                    end_frame: end,
                },
                message,
            });
        }
    }
}

fn contains_angle_description(text: &str) -> bool {
    let mut open = false;
    let mut content = false;
    for ch in text.chars() {
        match ch {
            '<' => {
                open = true;
                content = false;
            }
            '>' if open => {
                if content {
                    return true;
                }
                open = false;
            }
            _ if open && !ch.is_whitespace() => content = true,
            _ => {}
        }
    }
    false
}
fn is_voice_off(note: &str) -> bool {
    let n = note.to_lowercase();
    n.contains("voix off")
        || n.contains("voice off")
        || n.contains("voice-over")
        || n.contains("voice over")
}
fn has_final_punctuation(text: &str) -> bool {
    text.trim_end_matches(|c: char| matches!(c, '"' | '\'' | '»' | '”' | ')' | ']'))
        .ends_with(|c: char| matches!(c, '.' | ',' | '!' | '?' | ':' | ';' | '…'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::RythmoMarker;
    fn line(id: u64, text: &str, character: &str, y: f32, note: &str) -> RythmoLine {
        RythmoLine {
            id,
            start_frame: 0,
            duration_frames: 30,
            y_slot: y,
            text: text.into(),
            character_name: character.into(),
            character_color: [1.0; 4],
            kind: crate::rythmo_line::RythmoLineKind::Dialogue,
            voice_actor_names: vec![],
            syllable_ratios: vec![],
            karaoke: false,
            note: note.into(),
            presence: crate::rythmo_line::LinePresence::On,
            text_emotions: vec![],
        }
    }
    #[test]
    fn text_rules() {
        let mut d = vec![];
        lint_text(&line(1, "Bonjour <pleure>", "A", 0.25, ""), &mut d);
        assert!(d
            .iter()
            .any(|d| d.rule == Rule::ReactionInAngles && d.severity == Severity::Error));
        lint_text(&line(2, "(Pleurs)", "A", 0.25, ""), &mut d);
        assert!(d.iter().any(|d| d.rule == Rule::UnbracketedReaction));
        lint_text(&line(3, "([Pleurs])", "A", 0.25, ""), &mut d);
        assert!(!d.iter().any(|d| d.scope == Scope::Line(3)));
        lint_text(&line(4, "(mhm)", "A", 0.25, ""), &mut d);
        assert!(!d.iter().any(|d| d.scope == Scope::Line(4)));
    }
    #[test]
    fn ambiance_has_only_parentheses_rule() {
        let mut invalid = line(10, "pluie", "Extérieur", 0.25, "");
        invalid.kind = crate::rythmo_line::RythmoLineKind::AmbianceStart;
        let mut diagnostics = Vec::new();
        lint_text(&invalid, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, Rule::AmbianceParentheses);

        invalid.text = "(pluie)".into();
        diagnostics.clear();
        lint_text(&invalid, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }
    #[test]
    fn loop_thresholds_are_strict() {
        let mut p = Project::new();
        for frame in [0, 1801, 4502] {
            p.add_marker(RythmoMarker {
                kind: MarkerKind::Boucle,
                frame,
            });
        }
        let d = analyze(&p, 30.0);
        assert_eq!(d.iter().filter(|d| d.rule == Rule::LongLoop).count(), 1);
        assert_eq!(d.iter().filter(|d| d.rule == Rule::TooLongLoop).count(), 1);
    }
    #[test]
    fn loop_ends_at_out_marker() {
        let mut p = Project::new();
        p.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 0,
        });
        p.add_marker(RythmoMarker {
            kind: MarkerKind::Out,
            frame: 2701,
        });
        let d = analyze(&p, 30.0);
        assert!(d.iter().any(|d| d.rule == Rule::TooLongLoop
            && d.scope
                == Scope::Zone {
                    start_frame: 0,
                    end_frame: 2701
                }));
    }
    #[test]
    fn final_loop_uses_content_end_without_out() {
        let mut p = Project::new();
        let mut dialogue = line(1, "Une phrase.", "A", 0.25, "");
        dialogue.duration_frames = 2701;
        p.insert_line(dialogue);
        p.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 0,
        });
        assert!(analyze(&p, 30.0)
            .iter()
            .any(|d| d.rule == Rule::TooLongLoop));
    }
    #[test]
    fn project_wide_rules_affect_all_lines() {
        let mut p = Project::new();
        p.insert_line(line(1, "Oui.", "John", 0.25, "Voix off"));
        p.insert_line(line(2, "Non.", "john", 0.75, ""));
        for id in [1, 2] {
            let d = for_line(&p, 30.0, id);
            assert!(d.iter().any(|d| d.rule == Rule::CharacterOnMultipleTracks));
            assert!(d.iter().any(|d| d.rule == Rule::MixedVoicePresence));
        }
    }

    #[test]
    fn text_emotion_has_the_professional_context_warning() {
        let mut emotional = line(9, "Yay !", "A", 0.25, "");
        emotional
            .text_emotions
            .push(crate::rythmo_line::TextEmotionSpan {
                start: 0,
                end: 3,
                emotion: crate::rythmo_line::TextEmotion::Yay,
            });
        let mut diagnostics = Vec::new();
        lint_text(&emotional, &mut diagnostics);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.rule == Rule::TextEmotion)
            .unwrap();
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(
            diagnostic.message,
            "N'utilisez pas d'émotions du texte dans un milieu professionnel qui ne l'autorise pas !"
        );
    }
}
