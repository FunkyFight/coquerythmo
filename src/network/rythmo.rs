//! Entity snapshots for complete bande-rythmo replication.

use crate::project::{Character, LanguageSnapshot, Project, ProjectLanguage, ProjectSettings};
use crate::rythmo_drawing::{DrawingStroke, RythmoDrawing};
use crate::rythmo_line::{RythmoLine, RythmoMarker};
use crate::voice_actor::VoiceActor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub type Records = BTreeMap<String, Value>;
pub type Changes = BTreeMap<String, Option<Value>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub project_huuid: String,
    pub active_language_id: u64,
    pub language_order: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transport {
    pub frame: i64,
    pub playing: bool,
    pub fps: f64,
    pub instrumental: bool,
    /// Playback follows the DA in the bande rythmo. Recording has its own
    /// control owner, which may be a Co-DA.
    pub rythmo: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectorView {
    pub selection: Option<crate::workspaces::rythmo::view::Selection>,
    pub compact_empty_tracks: bool,
    pub active_stroke: Option<DrawingStroke>,
    pub font_family: String,
}

pub struct Band {
    language: ProjectLanguage,
    lines: Vec<RythmoLine>,
    markers: Vec<RythmoMarker>,
    characters: Vec<Character>,
    actors: Vec<VoiceActor>,
    drawing: RythmoDrawing,
    settings: ProjectSettings,
}

pub struct RythmoDocument {
    pub manifest: Manifest,
    bands: Vec<Band>,
}

impl RythmoDocument {
    /// Build all bands before replacing the live document. Media filenames
    /// belong to the recipient's extracted archive and stay local.
    pub fn apply(&self, project: &mut Project) -> Result<(), String> {
        let mut snapshots = Vec::with_capacity(self.bands.len());
        for band in &self.bands {
            let mut replacement = Project::new();
            let mut settings = band.settings.clone();
            settings.instrumental_audio_path =
                project.language_instrumental_audio_path(band.language.id);
            replacement.replace_lines(band.lines.clone());
            replacement.set_markers(band.markers.clone());
            replacement.set_known_characters(band.characters.clone());
            let mut actors = band.actors.clone();
            if let Some(local) = project.project_for_language(band.language.id) {
                for actor in &mut actors {
                    actor.icon_path = local
                        .voice_actors()
                        .iter()
                        .find(|previous| previous.name == actor.name)
                        .map(|previous| previous.icon_path.clone())
                        .unwrap_or_default();
                }
            }
            replacement.set_voice_actors(actors);
            replacement.set_drawing(band.drawing.clone());
            replacement.set_settings(settings);
            snapshots.push(LanguageSnapshot {
                language: band.language.clone(),
                project: replacement,
            });
        }
        if !project.replace_language_snapshots(snapshots, self.manifest.active_language_id) {
            return Err("invalid replicated language collection".into());
        }
        Ok(())
    }
}

fn put<T: Serialize>(
    records: &mut Records,
    key: impl Into<String>,
    value: &T,
) -> Result<(), String> {
    records.insert(
        key.into(),
        serde_json::to_value(value).map_err(|error| error.to_string())?,
    );
    Ok(())
}

pub fn document_records(project: &Project, huuid: &str) -> Result<Records, String> {
    let mut records = Records::new();
    put(
        &mut records,
        "manifest",
        &Manifest {
            project_huuid: huuid.to_owned(),
            active_language_id: project.active_language_id(),
            language_order: project
                .languages()
                .iter()
                .map(|language| language.id)
                .collect(),
        },
    )?;
    for snapshot in project.language_snapshots() {
        let prefix = format!("lang/{}", snapshot.language.id);
        let band = snapshot.project;
        put(&mut records, format!("{prefix}/meta"), &snapshot.language)?;
        put(
            &mut records,
            format!("{prefix}/line_order"),
            &band.lines().map(|line| line.id).collect::<Vec<_>>(),
        )?;
        for line in band.lines() {
            put(&mut records, format!("{prefix}/line/{}", line.id), line)?;
        }
        put(&mut records, format!("{prefix}/markers"), &band.markers())?;
        put(
            &mut records,
            format!("{prefix}/characters"),
            &band.known_characters(),
        )?;
        let mut actors = band.voice_actors().to_vec();
        for actor in &mut actors {
            actor.icon_path.clear();
        }
        put(&mut records, format!("{prefix}/actors"), &actors)?;
        put(&mut records, format!("{prefix}/drawing"), &band.drawing())?;
        let mut settings = band.settings().clone();
        settings.instrumental_audio_path = None;
        put(&mut records, format!("{prefix}/settings"), &settings)?;
    }
    Ok(records)
}

fn record<T: for<'a> Deserialize<'a>>(records: &Records, key: &str) -> Result<T, String> {
    let value = records
        .get(key)
        .ok_or_else(|| format!("missing replicated record: {key}"))?;
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid replicated record {key}: {error}"))
}

pub fn decode_document(records: &Records) -> Result<RythmoDocument, String> {
    let manifest: Manifest = record(records, "manifest")?;
    let mut language_ids = HashSet::new();
    if manifest.language_order.is_empty()
        || manifest.language_order.len() > 256
        || !manifest
            .language_order
            .contains(&manifest.active_language_id)
    {
        return Err("invalid replicated language manifest".into());
    }
    let mut bands = Vec::new();
    for id in &manifest.language_order {
        if !language_ids.insert(*id) {
            return Err("duplicate replicated language".into());
        }
        let prefix = format!("lang/{id}");
        let language: ProjectLanguage = record(records, &format!("{prefix}/meta"))?;
        if language.id != *id || language.name.trim().is_empty() {
            return Err("invalid replicated language identity".into());
        }
        let order: Vec<u64> = record(records, &format!("{prefix}/line_order"))?;
        let mut line_ids = HashSet::new();
        let mut lines = Vec::with_capacity(order.len());
        for line_id in order {
            let line: RythmoLine = record(records, &format!("{prefix}/line/{line_id}"))?;
            if line.id != line_id
                || !line_ids.insert(line_id)
                || line.duration_frames < 0
                || !line.y_slot.is_finite()
            {
                return Err("invalid replicated line".into());
            }
            lines.push(line);
        }
        bands.push(Band {
            language,
            lines,
            markers: record(records, &format!("{prefix}/markers"))?,
            characters: record(records, &format!("{prefix}/characters"))?,
            actors: record(records, &format!("{prefix}/actors"))?,
            drawing: record(records, &format!("{prefix}/drawing"))?,
            settings: record(records, &format!("{prefix}/settings"))?,
        });
    }
    Ok(RythmoDocument { manifest, bands })
}

pub fn diff_records(before: &Records, after: &Records) -> Changes {
    let mut changes = Changes::new();
    for (key, value) in after {
        if before.get(key) != Some(value) {
            changes.insert(key.clone(), Some(value.clone()));
        }
    }
    for key in before.keys() {
        if !after.contains_key(key) {
            changes.insert(key.clone(), None);
        }
    }
    changes
}

pub fn apply_changes(records: &mut Records, changes: Changes) {
    for (key, value) in changes {
        if let Some(value) = value {
            records.insert(key, value);
        } else {
            records.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_every_band_line_order_and_local_media_paths() {
        let mut source = Project::new();
        let first = source.active_language_id();
        let id = source.add_line(10, 30, 0.25);
        source.get_line_mut(id).unwrap().note = "note".into();
        let mut settings = source.settings().clone();
        settings.instrumental_audio_path = Some("director-only.flac".into());
        settings.scroll_speed = 1.5;
        source.set_settings(settings);
        let second = source.create_language_named("English");
        source.select_language(second);
        source.add_line(40, 50, 0.5);
        let records = document_records(&source, "project-id").unwrap();
        assert!(!serde_json::to_string(&records)
            .unwrap()
            .contains("director-only.flac"));
        let mut target = source.snapshot();
        target.set_language_instrumental_audio_path(first, Some("actor-local.flac".into()));
        let document = decode_document(&records).unwrap();
        document.apply(&mut target).unwrap();
        assert_eq!(target.active_language_id(), second);
        assert_eq!(target.language_count(), 2);
        assert_eq!(
            target
                .project_for_language(first)
                .unwrap()
                .get_line(id)
                .unwrap()
                .note,
            "note"
        );
        assert_eq!(
            target.language_instrumental_audio_path(first).as_deref(),
            Some("actor-local.flac")
        );
        assert_eq!(target.settings().scroll_speed, 1.5);
    }

    #[test]
    fn one_text_edit_only_sends_the_changed_line() {
        let mut source = Project::new();
        let id = source.add_line(0, 50, 0.2);
        source.add_line(60, 20, 0.4);
        let before = document_records(&source, "project").unwrap();
        source.get_line_mut(id).unwrap().text = "nouveau texte".into();
        let after = document_records(&source, "project").unwrap();
        let changes = diff_records(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(changes
            .keys()
            .next()
            .unwrap()
            .ends_with(&format!("/line/{id}")));
    }

    #[test]
    fn deletions_and_undo_replace_the_correct_entities() {
        let mut source = Project::new();
        let id = source.add_line(10, 40, 0.3);
        let original = document_records(&source, "project").unwrap();
        source.remove_line(id);
        let removed = document_records(&source, "project").unwrap();
        let mut replicated = original.clone();
        apply_changes(&mut replicated, diff_records(&original, &removed));
        assert_eq!(replicated, removed);
        apply_changes(&mut replicated, diff_records(&removed, &original));
        assert_eq!(replicated, original);
    }

    #[test]
    fn missing_or_duplicated_line_records_reject_the_snapshot() {
        let mut source = Project::new();
        let id = source.add_line(0, 10, 0.5);
        let mut records = document_records(&source, "project").unwrap();
        records.remove(&format!("lang/{}/line/{id}", source.active_language_id()));
        assert!(decode_document(&records).is_err());
    }

    #[test]
    fn drawings_detections_cast_markers_and_settings_survive_replication_and_undo() {
        use crate::detection::{track_storage_line_id, DetectionKind, MediaTick, TextAnchor};
        let mut source = Project::new();
        let line = source.add_line(120, 60, 0.5);
        let before = document_records(&source, "project").unwrap();
        let mut settings = source.settings().clone();
        settings
            .detections
            .add_detection(
                track_storage_line_id(0),
                DetectionKind::Labial,
                MediaTick(1250),
                TextAnchor::BeforeText,
            )
            .unwrap();
        settings.highlight_read_word = true;
        settings.source_audio_offset_frames = 12;
        settings.reading_bar_offset_percent = -10.0;
        source.set_settings(settings);
        source.add_marker(RythmoMarker {
            kind: crate::rythmo_line::MarkerKind::Boucle,
            frame: 125,
        });
        let mut drawing = RythmoDrawing::new();
        let mut stroke = DrawingStroke::new(1, [1.0, 0.0, 0.0, 1.0], 0.02);
        stroke.points = vec![(120.0, 0.2), (150.0, 0.3)];
        drawing.add(stroke);
        source.set_drawing(drawing);
        source.set_known_characters(vec![Character {
            name: "Ada".into(),
            color: [0.0, 1.0, 0.0, 1.0],
        }]);
        source.set_voice_actors(vec![VoiceActor {
            name: "Camille".into(),
            icon_path: "DA/private/icon.png".into(),
            icon_png_base64: Some("embedded-icon".into()),
        }]);
        source.get_line_mut(line).unwrap().text = "Une réplique modifiée".into();
        let after = document_records(&source, "project").unwrap();
        assert!(!serde_json::to_string(&after)
            .unwrap()
            .contains("DA/private"));
        let mut replica = before.clone();
        apply_changes(&mut replica, diff_records(&before, &after));
        let mut target = Project::new();
        decode_document(&before)
            .unwrap()
            .apply(&mut target)
            .unwrap();
        target.set_voice_actors(vec![VoiceActor {
            name: "Camille".into(),
            icon_path: "local/icon.png".into(),
            icon_png_base64: None,
        }]);
        decode_document(&replica)
            .unwrap()
            .apply(&mut target)
            .unwrap();
        assert_eq!(target.voice_actors()[0].icon_path, "local/icon.png");
        assert_eq!(document_records(&target, "project").unwrap(), after);
        apply_changes(&mut replica, diff_records(&after, &before));
        decode_document(&replica)
            .unwrap()
            .apply(&mut target)
            .unwrap();
        assert_eq!(document_records(&target, "project").unwrap(), before);
    }
}
