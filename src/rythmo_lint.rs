//! Pure convention linting for bande-rythmo projects.
//!
//! The linter deliberately has no dependency on UI, rendering, playback or
//! export adapters. Editor overlays and accessibility descriptions consume the
//! same diagnostics, while export paths simply never ask for them.

use crate::project::Project;
use crate::rythmo_line::MarkerKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Error,
}

impl LintSeverity {
    pub const fn accessibility_label(self) -> &'static str {
        match self {
            Self::Warning => "Avertissement",
            Self::Error => "Erreur",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LintScope {
    Line {
        line_id: u64,
        start_char: usize,
        end_char: usize,
    },
    Zone {
        start_frame: i64,
        end_frame: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintDiagnostic {
    pub code: &'static str,
    pub severity: LintSeverity,
    pub scope: LintScope,
    pub message: &'static str,
}

impl LintDiagnostic {
    pub fn applies_to_line(&self, line_id: u64, start_frame: i64, end_frame: i64) -> bool {
        match self.scope {
            LintScope::Line {
                line_id: diagnostic_line,
                ..
            } => diagnostic_line == line_id,
            LintScope::Zone {
                start_frame: zone_start,
                end_frame: zone_end,
            } => start_frame < zone_end && end_frame > zone_start,
        }
    }
}

const REACTION_DESCRIPTOR_MESSAGE: &str =
    "Les descriptifs de réaction doivent être écrits entre piquants, pas entre chevrons.";
const PARENTHESIZED_REACTION_MESSAGE: &str =
    "Cette réaction isolée devrait être écrite entre crochets puis parenthèses, par exemple ([Pleurs]).";
const LOOP_LONG_MESSAGE: &str =
    "Cette boucle dépasse une minute. Dans l’idéal, une boucle dure environ une minute.";
const LOOP_TOO_LONG_MESSAGE: &str =
    "Cette boucle dépasse une minute trente. Elle n’est pas conforme et doit être réduite.";
const CHARACTER_MULTIPLE_TRACKS_MESSAGE: &str =
    "Ce personnage apparaît sur plusieurs lignes de la bande. Gardez autant que possible une ligne dédiée au personnage.";

/// Compute every currently supported convention diagnostic.
pub fn lint_project(project: &Project) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    lint_reaction_notation(project, &mut diagnostics);
    lint_loop_lengths(project, &mut diagnostics);
    lint_character_tracks(project, &mut diagnostics);
    diagnostics.sort_by_key(diagnostic_sort_key);
    diagnostics
}

pub fn line_diagnostics(project: &Project, line_id: u64) -> Vec<LintDiagnostic> {
    let Some(line) = project.get_line(line_id) else {
        return Vec::new();
    };
    lint_project(project)
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.applies_to_line(line.id, line.start_frame, line.end_frame())
        })
        .collect()
}

/// Sentence appended to the semantic description of a line.
pub fn line_accessibility_suffix(project: &Project, line_id: u64) -> String {
    let diagnostics = line_diagnostics(project, line_id);
    if diagnostics.is_empty() {
        return String::new();
    }
    let mut text = String::from(" Conventions :");
    for diagnostic in diagnostics {
        text.push(' ');
        text.push_str(diagnostic.severity.accessibility_label());
        text.push_str(" : ");
        text.push_str(diagnostic.message);
    }
    text
}

fn diagnostic_sort_key(diagnostic: &LintDiagnostic) -> (i64, usize, u8, &'static str) {
    let (frame, character) = match diagnostic.scope {
        LintScope::Line {
            line_id,
            start_char,
            ..
        } => (line_id.min(i64::MAX as u64) as i64, start_char),
        LintScope::Zone { start_frame, .. } => (start_frame, 0),
    };
    let severity = match diagnostic.severity {
        LintSeverity::Error => 0,
        LintSeverity::Warning => 1,
    };
    (frame, character, severity, diagnostic.code)
}

fn lint_reaction_notation(project: &Project, diagnostics: &mut Vec<LintDiagnostic>) {
    for line in project.lines() {
        let characters = line.text.chars().collect::<Vec<_>>();
        let mut open = None;
        for (index, character) in characters.iter().copied().enumerate() {
            match character {
                '<' if open.is_none() => open = Some(index),
                '>' => {
                    if let Some(start_char) = open.take() {
                        if index > start_char + 1 {
                            diagnostics.push(LintDiagnostic {
                                code: "reaction.angle_brackets",
                                severity: LintSeverity::Error,
                                scope: LintScope::Line {
                                    line_id: line.id,
                                    start_char,
                                    end_char: index + 1,
                                },
                                message: REACTION_DESCRIPTOR_MESSAGE,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        let trimmed = line.text.trim();
        if !is_parenthesized_only(trimmed) || is_bracketed_parenthesized(trimmed) {
            continue;
        }
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner.is_empty() || is_known_reaction(inner) {
            continue;
        }
        diagnostics.push(LintDiagnostic {
            code: "reaction.parentheses_without_brackets",
            severity: LintSeverity::Warning,
            scope: LintScope::Line {
                line_id: line.id,
                start_char: 0,
                end_char: line.text.chars().count(),
            },
            message: PARENTHESIZED_REACTION_MESSAGE,
        });
    }
}

fn is_parenthesized_only(text: &str) -> bool {
    text.len() >= 2 && text.starts_with('(') && text.ends_with(')')
}

fn is_bracketed_parenthesized(text: &str) -> bool {
    text.starts_with("([") && text.ends_with("])")
}

fn normalized_reaction(text: &str) -> String {
    text.trim()
        .trim_matches(|character: char| character == '[' || character == ']')
        .trim()
        .to_lowercase()
}

fn is_known_reaction(text: &str) -> bool {
    matches!(
        normalized_reaction(text).as_str(),
        "rire"
            | "rires"
            | "rit"
            | "pleure"
            | "pleurs"
            | "sanglot"
            | "sanglots"
            | "soupir"
            | "soupire"
            | "respiration"
            | "souffle"
            | "cri"
            | "crie"
            | "toux"
            | "tousse"
            | "éternuement"
            | "éternue"
            | "halètement"
            | "halète"
            | "gémissement"
            | "gémit"
            | "grognement"
            | "grogne"
            | "laugh"
            | "laughs"
            | "crying"
            | "cries"
            | "sigh"
            | "sighs"
            | "breath"
            | "breathes"
            | "cough"
            | "coughs"
            | "grunt"
            | "grunts"
    )
}

fn lint_loop_lengths(project: &Project, diagnostics: &mut Vec<LintDiagnostic>) {
    let mut starts = project
        .markers()
        .iter()
        .filter(|marker| matches!(marker.kind, MarkerKind::Boucle))
        .map(|marker| marker.frame)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let fps = project.settings().export_configuration.fps;
    let fps = if fps.is_finite() && fps > 0.0 { fps } else { 24.0 };
    let warning_frames = (60.0 * fps).round() as i64;
    let error_frames = (90.0 * fps).round() as i64;

    for pair in starts.windows(2) {
        let start_frame = pair[0];
        let end_frame = pair[1];
        let duration = end_frame.saturating_sub(start_frame);
        let (severity, message, code) = if duration > error_frames {
            (
                LintSeverity::Error,
                LOOP_TOO_LONG_MESSAGE,
                "loop.too_long",
            )
        } else if duration > warning_frames {
            (LintSeverity::Warning, LOOP_LONG_MESSAGE, "loop.long")
        } else {
            continue;
        };
        diagnostics.push(LintDiagnostic {
            code,
            severity,
            scope: LintScope::Zone {
                start_frame,
                end_frame,
            },
            message,
        });
    }
}

fn lint_character_tracks(project: &Project, diagnostics: &mut Vec<LintDiagnostic>) {
    let mut tracks_by_character: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for line in project.lines() {
        let character = line.character_name.trim().to_lowercase();
        if character.is_empty() {
            continue;
        }
        tracks_by_character
            .entry(character)
            .or_default()
            .insert(crate::rythmo_layout::track_index_for_y_slot(line.y_slot));
    }

    for line in project.lines() {
        let character = line.character_name.trim().to_lowercase();
        if character.is_empty()
            || !tracks_by_character
                .get(&character)
                .is_some_and(|tracks| tracks.len() > 1)
        {
            continue;
        }
        diagnostics.push(LintDiagnostic {
            code: "character.multiple_tracks",
            severity: LintSeverity::Warning,
            scope: LintScope::Line {
                line_id: line.id,
                start_char: 0,
                end_char: line.text.chars().count().max(1),
            },
            message: CHARACTER_MULTIPLE_TRACKS_MESSAGE,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::RythmoMarker;

    fn line_with_text(project: &mut Project, text: &str) -> u64 {
        let id = project.add_line(0, 48, 0.0);
        project.get_line_mut(id).unwrap().text = text.to_string();
        id
    }

    #[test]
    fn angle_bracket_descriptor_is_an_error_with_exact_span() {
        let mut project = Project::new();
        let line_id = line_with_text(&mut project, "Salut <angry grunt> !");
        let diagnostic = lint_project(&project)
            .into_iter()
            .find(|diagnostic| diagnostic.code == "reaction.angle_brackets")
            .expect("reaction descriptor should be linted");
        assert_eq!(diagnostic.severity, LintSeverity::Error);
        assert_eq!(
            diagnostic.scope,
            LintScope::Line {
                line_id,
                start_char: 6,
                end_char: 19,
            }
        );
    }

    #[test]
    fn unknown_parenthesized_reaction_warns_but_bracketed_form_does_not() {
        let mut project = Project::new();
        let warned = line_with_text(&mut project, "(Pleurs chauds)");
        let safe = project.add_line(60, 48, 0.25);
        project.get_line_mut(safe).unwrap().text = "([Pleurs chauds])".to_string();
        let diagnostics = lint_project(&project);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "reaction.parentheses_without_brackets"
                && matches!(diagnostic.scope, LintScope::Line { line_id, .. } if line_id == warned)
        }));
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "reaction.parentheses_without_brackets"
                && matches!(diagnostic.scope, LintScope::Line { line_id, .. } if line_id == safe)
        }));
    }

    #[test]
    fn loop_length_has_warning_then_error_thresholds() {
        let mut project = Project::new();
        let fps = project.settings().export_configuration.fps;
        for seconds in [0.0, 75.0, 170.0] {
            project.add_marker(RythmoMarker {
                kind: MarkerKind::Boucle,
                frame: (seconds * fps).round() as i64,
            });
        }
        let diagnostics = lint_project(&project);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "loop.long" && diagnostic.severity == LintSeverity::Warning
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "loop.too_long" && diagnostic.severity == LintSeverity::Error
        }));
    }

    #[test]
    fn character_on_multiple_tracks_warns_every_affected_line() {
        let mut project = Project::new();
        let first = project.add_line(0, 48, 0.0);
        let second = project.add_line(60, 48, 1.0);
        for id in [first, second] {
            let line = project.get_line_mut(id).unwrap();
            line.character_name = "John".to_string();
            line.text = "Texte".to_string();
        }
        let diagnostics = lint_project(&project);
        let affected = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "character.multiple_tracks")
            .count();
        assert_eq!(affected, 2);
    }

    #[test]
    fn accessibility_suffix_mentions_line_and_containing_zone_errors() {
        let mut project = Project::new();
        let line_id = project.add_line(10, 48, 0.0);
        project.get_line_mut(line_id).unwrap().text = "<cri>".to_string();
        let fps = project.settings().export_configuration.fps;
        for seconds in [0.0, 100.0] {
            project.add_marker(RythmoMarker {
                kind: MarkerKind::Boucle,
                frame: (seconds * fps).round() as i64,
            });
        }
        let suffix = line_accessibility_suffix(&project, line_id);
        assert!(suffix.contains("Erreur"));
        assert!(suffix.contains("piquants"));
        assert!(suffix.contains("minute trente"));
    }
}
