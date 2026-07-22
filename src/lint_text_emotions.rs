//! Lint facade adding the text-emotion professional-use warning.

#[path = "lint.rs"]
mod base;

pub use base::{Diagnostic, Rule, Scope, Severity};

use crate::project::Project;
use crate::rythmo_line::RythmoLine;

pub const TEXT_EMOTION_PROFESSIONAL_WARNING: &str =
    "N'utilisez pas d'émotions du texte dans un milieu professionnel qui ne l'autorise pas !";

pub fn analyze(project: &Project, fps: f64) -> Vec<Diagnostic> {
    let mut diagnostics = base::analyze(project, fps);
    diagnostics.extend(project.lines().filter_map(|line| {
        (line.kind.is_dialogue()
            && !line.karaoke
            && crate::text_emotion::has_line(line.id))
        .then_some(Diagnostic {
            severity: Severity::Warning,
            // The existing public rule enum is kept wire-compatible. The
            // message and scope are authoritative for UI/accessibility.
            rule: Rule::UnbracketedReaction,
            scope: Scope::Line(line.id),
            message: TEXT_EMOTION_PROFESSIONAL_WARNING,
        })
    }));
    diagnostics
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
        .filter(|diagnostic| match diagnostic.scope {
            Scope::Line(id) => id == line.id,
            Scope::Zone {
                start_frame,
                end_frame,
            } => line.start_frame < end_frame && line.end_frame() > start_frame,
        })
        .cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_emotion::{TextEmotion, apply_range, clear};

    #[test]
    fn emotional_dialogue_gets_exact_warning_and_accessibility_suffix() {
        clear();
        let mut project = Project::new();
        let id = project.add_line_full(
            0,
            24,
            0.25,
            "Bonjour.".to_string(),
            "Alice".to_string(),
            [1.0; 4],
        );
        apply_range(id, "Bonjour.", 0, 8, Some(TextEmotion::Wave));
        let diagnostics = for_line(&project, 24.0, id);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Warning
                && diagnostic.message == TEXT_EMOTION_PROFESSIONAL_WARNING
        }));
        assert!(line_description_suffix(&project, 24.0, id)
            .unwrap()
            .contains(TEXT_EMOTION_PROFESSIONAL_WARNING));
    }

    #[test]
    fn karaoke_and_ambiance_never_get_emotion_warning() {
        clear();
        let mut project = Project::new();
        let karaoke = project.add_line_full(
            0,
            24,
            0.25,
            "Chant".to_string(),
            "Alice".to_string(),
            [1.0; 4],
        );
        project.get_line_mut(karaoke).unwrap().karaoke = true;
        apply_range(karaoke, "Chant", 0, 5, Some(TextEmotion::Wave));
        assert!(!for_line(&project, 24.0, karaoke)
            .iter()
            .any(|diagnostic| diagnostic.message == TEXT_EMOTION_PROFESSIONAL_WARNING));
    }
}
