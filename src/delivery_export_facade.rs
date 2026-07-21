//! Delivery-export facade for dialogue-only documents.
//!
//! Video exports intentionally bypass this module and render ambience lines.
//! Subtitle, interchange, cross-reference and presence documents receive an
//! ephemeral dialogue-only project with hidden metadata removed.

#[path = "delivery_export.rs"]
mod legacy;

pub use legacy::*;

use crate::project::Project;
use crate::rythmo_line_metadata::{decode, LinePresentation, LineSemanticKind};
use std::path::Path;

fn document_project(project: &Project) -> Project {
    let mut document = project.clone();
    let ambience_ids = document
        .lines()
        .filter(|line| decode(&line.note).0.kind != LineSemanticKind::Dialogue)
        .map(|line| line.id)
        .collect::<Vec<_>>();
    for line_id in ambience_ids {
        document.remove_line(line_id);
    }

    let line_ids = document.lines().map(|line| line.id).collect::<Vec<_>>();
    for line_id in line_ids {
        let Some(line) = document.get_line(line_id).cloned() else {
            continue;
        };
        let (metadata, user_note) = decode(&line.note);
        let visible_note = if metadata.presentation == LinePresentation::Off {
            if user_note.trim().is_empty() {
                "voice off".to_string()
            } else {
                format!("voice off\n{user_note}")
            }
        } else {
            user_note.to_string()
        };
        if let Some(line) = document.get_line_mut(line_id) {
            line.note = visible_note;
        }
    }

    // Production markers live in a reserved detection bucket. They are useful
    // only in the interactive editor and must never leak into delivery JSON.
    let mut settings = document.settings().clone();
    settings
        .detections
        .remove_line(crate::rythmo_special_markers::storage_line_id());
    document.set_settings(settings);
    document
}

pub fn export_subtitle(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
    format: SubtitleFormat,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_subtitle(&project, fps, output, language_name, format)
}

pub fn json_document(project: &Project, fps: f64) -> Result<String, String> {
    legacy::json_document(&document_project(project), fps)
}

pub fn export_json(project: &Project, fps: f64, output: &Path) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_json(&project, fps, output)
}

pub fn srt_document(project: &Project, fps: f64) -> Result<String, String> {
    legacy::srt_document(&document_project(project), fps)
}

pub fn export_srt(project: &Project, fps: f64, output: &Path) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_srt(&project, fps, output)
}

pub fn ass_document(project: &Project, fps: f64, language_name: &str) -> Result<String, String> {
    legacy::ass_document(&document_project(project), fps, language_name)
}

pub fn export_ass(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_ass(&project, fps, output, language_name)
}

pub fn detx_document(project: &Project, fps: f64, language_name: &str) -> Result<String, String> {
    legacy::detx_document(&document_project(project), fps, language_name)
}

pub fn export_detx(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_detx(&project, fps, output, language_name)
}

pub fn cross_reference_csv_document(
    project: &Project,
    fps: f64,
    language_name: &str,
) -> Result<String, String> {
    legacy::cross_reference_csv_document(&document_project(project), fps, language_name)
}

pub fn export_cross_reference_csv(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_cross_reference_csv(&project, fps, output, language_name)
}

pub fn cross_reference_pdf_bytes(
    project: &Project,
    fps: f64,
    language_name: &str,
) -> Result<Vec<u8>, String> {
    legacy::cross_reference_pdf_bytes(&document_project(project), fps, language_name)
}

pub fn export_cross_reference_pdf(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_cross_reference_pdf(&project, fps, output, language_name)
}

pub fn presence_grid_pdf_bytes(
    project: &Project,
    fps: f64,
    language_name: &str,
) -> Result<Vec<u8>, String> {
    legacy::presence_grid_pdf_bytes(&document_project(project), fps, language_name)
}

pub fn export_presence_grid_pdf(
    project: &Project,
    fps: f64,
    output: &Path,
    language_name: &str,
) -> Result<(), String> {
    let project = document_project(project);
    legacy::export_presence_grid_pdf(&project, fps, output, language_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{
        DetectionAddress, DetectionCue, DetectionCueId, DetectionKind, MediaTick,
    };
    use crate::rythmo_line_metadata::{with_kind, with_presentation};
    use crate::rythmo_special_markers::SpecialMarkerKind;

    #[test]
    fn ambience_is_absent_from_subtitles_and_cross_reference() {
        let mut project = Project::new();
        let dialogue = project.add_line(0, 24, 0.0);
        let ambience = project.add_line(30, 24, 0.25);
        project.get_line_mut(dialogue).unwrap().text = "Bonjour".to_string();
        let ambience_line = project.get_line_mut(ambience).unwrap();
        ambience_line.text = "Foule au loin".to_string();
        ambience_line.note = with_kind("", LineSemanticKind::AmbienceStart);

        let srt = srt_document(&project, 24.0).unwrap();
        let csv = cross_reference_csv_document(&project, 24.0, "fr").unwrap();
        assert!(srt.contains("Bonjour"));
        assert!(!srt.contains("Foule au loin"));
        assert!(csv.contains("Bonjour"));
        assert!(!csv.contains("Foule au loin"));
    }

    #[test]
    fn hidden_metadata_never_appears_in_delivery_json() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        project.get_line_mut(line_id).unwrap().text = "Bonjour".to_string();
        project.get_line_mut(line_id).unwrap().note =
            with_presentation("note humaine", LinePresentation::Back);
        let json = json_document(&project, 24.0).unwrap();
        assert!(json.contains("note humaine"));
        assert!(!json.contains("coquerythmo-line-v1"));
    }

    #[test]
    fn off_state_maps_to_detx_voice_off() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.0);
        let line = project.get_line_mut(line_id).unwrap();
        line.text = "Bonjour".to_string();
        line.note = with_presentation("", LinePresentation::Off);
        let detx = detx_document(&project, 24.0, "fr").unwrap();
        assert!(detx.contains("voice=\"off\""));
    }

    #[test]
    fn production_marker_bucket_is_removed_from_json() {
        let mut project = Project::new();
        let cue = DetectionCue {
            id: DetectionCueId(1),
            kind: DetectionKind::Reaction,
            media_tick: MediaTick::from_frame(10),
            target: SpecialMarkerKind::Start.target(),
        };
        assert!(project.detections_mut().insert_detection(
            DetectionAddress {
                line_id: crate::rythmo_special_markers::storage_line_id(),
                detection_id: cue.id,
            },
            cue,
        ));
        let json = json_document(&project, 24.0).unwrap();
        assert!(!json.contains(&crate::rythmo_special_markers::storage_line_id().to_string()));
    }
}
