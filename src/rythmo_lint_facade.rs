//! Extended bande-rythmo lint facade.

#[path = "rythmo_lint.rs"]
mod legacy;

pub use legacy::*;

use crate::project::Project;
use crate::rythmo_line_metadata::{decode, LinePresentation};
use std::collections::BTreeMap;

const CHARACTER_MIXED_ON_OFF_MESSAGE: &str =
    "Ce personnage possède des répliques ON et OFF. Créez un personnage distinct pour la voix hors caméra, par exemple John V.O.";

fn append_mixed_on_off(project: &Project, diagnostics: &mut Vec<LintDiagnostic>) {
    let mut states = BTreeMap::<String, (bool, bool)>::new();
    for line in project.lines() {
        let character = line.character_name.trim().to_lowercase();
        if character.is_empty() {
            continue;
        }
        let presentation = decode(&line.note).0.presentation;
        let state = states.entry(character).or_default();
        if presentation == LinePresentation::Off {
            state.1 = true;
        } else {
            state.0 = true;
        }
    }

    for line in project.lines() {
        let character = line.character_name.trim().to_lowercase();
        if character.is_empty() || states.get(&character) != Some(&(true, true)) {
            continue;
        }
        diagnostics.push(LintDiagnostic {
            code: "character.mixed_on_off",
            severity: LintSeverity::Warning,
            scope: LintScope::Line {
                line_id: line.id,
                start_char: 0,
                end_char: line.text.chars().count().max(1),
            },
            message: CHARACTER_MIXED_ON_OFF_MESSAGE,
        });
    }
}

pub fn lint_project(project: &Project) -> Vec<LintDiagnostic> {
    let mut diagnostics = legacy::lint_project(project);
    append_mixed_on_off(project, &mut diagnostics);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line_metadata::{with_presentation, LinePresentation};

    #[test]
    fn mixed_on_off_character_warns_all_affected_lines() {
        let mut project = Project::new();
        let on = project.add_line(0, 24, 0.0);
        let off = project.add_line(30, 24, 0.0);
        for id in [on, off] {
            let line = project.get_line_mut(id).unwrap();
            line.character_name = "John".to_string();
            line.text = "Texte".to_string();
        }
        project.get_line_mut(off).unwrap().note =
            with_presentation("", LinePresentation::Off);

        let diagnostics = lint_project(&project);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "character.mixed_on_off")
                .count(),
            2
        );
    }
}
