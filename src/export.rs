use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::project::{Character, Project};
use crate::rythmo_line::{MarkerKind, RythmoMarker};

/// Trait for project exporters. Implement for each format.
pub trait ProjectExporter {
    fn extension(&self) -> &str;
    fn description(&self) -> &str;
    fn export(&self, project: &Project, fps: f64, path: &Path) -> Result<(), String>;
}

/// Trait for project importers.
pub trait ProjectImporter {
    fn extension(&self) -> &str;
    fn description(&self) -> &str;
    fn import(&self, path: &Path) -> Result<ProjectData, String>;
}

// -- Serializable data structures --

#[derive(Serialize, Deserialize)]
pub struct ProjectData {
    #[serde(default = "default_fps")]
    pub source_fps: f64,
    pub lines: Vec<LineData>,
    pub markers: Vec<MarkerData>,
    pub characters: Vec<CharacterData>,
}

fn default_fps() -> f64 { 24.0 }

#[derive(Serialize, Deserialize)]
pub struct LineData {
    pub start_frame: i64,
    pub duration_frames: i64,
    pub y_slot: f32,
    pub text: String,
    pub character_name: String,
    pub character_color: [f32; 4],
}

#[derive(Serialize, Deserialize)]
pub struct MarkerData {
    pub kind: String,
    pub frame: i64,
}

#[derive(Serialize, Deserialize)]
pub struct CharacterData {
    pub name: String,
    pub color: [f32; 4],
}

// -- Conversion --

impl ProjectData {
    pub fn from_project(project: &Project, fps: f64) -> Self {
        Self {
            source_fps: fps,
            lines: project.lines.iter().map(|l| LineData {
                start_frame: l.start_frame,
                duration_frames: l.duration_frames,
                y_slot: l.y_slot,
                text: l.text.clone(),
                character_name: l.character_name.clone(),
                character_color: l.character_color,
            }).collect(),
            markers: project.markers.iter().map(|m| MarkerData {
                kind: match &m.kind {
                    MarkerKind::Boucle => "boucle",
                    MarkerKind::Out => "out",
                    MarkerKind::SceneChange => "scene_change",
                    MarkerKind::LiaisonLeft => "liaison_left",
                    MarkerKind::LiaisonRight => "liaison_right",
                }.to_string(),
                frame: m.frame,
            }).collect(),
            characters: project.known_characters.iter().map(|c| CharacterData {
                name: c.name.clone(),
                color: c.color,
            }).collect(),
        }
    }

    pub fn apply_to_project(&self, project: &mut Project, target_fps: f64) {
        let fps_ratio = if self.source_fps > 0.0 && target_fps > 0.0 {
            target_fps / self.source_fps
        } else {
            1.0
        };
        project.lines.clear();
        project.markers.clear();
        project.known_characters.clear();

        for ch in &self.characters {
            project.known_characters.push(Character { name: ch.name.clone(), color: ch.color });
        }

        for l in &self.lines {
            project.add_line_full(
                (l.start_frame as f64 * fps_ratio) as i64,
                (l.duration_frames as f64 * fps_ratio) as i64,
                l.y_slot,
                l.text.clone(), l.character_name.clone(), l.character_color,
            );
        }

        for m in &self.markers {
            let kind = match m.kind.as_str() {
                "boucle" => MarkerKind::Boucle,
                "out" => MarkerKind::Out,
                "scene_change" => MarkerKind::SceneChange,
                "liaison_left" => MarkerKind::LiaisonLeft,
                "liaison_right" => MarkerKind::LiaisonRight,
                _ => continue,
            };
            project.markers.push(RythmoMarker { kind, frame: (m.frame as f64 * fps_ratio) as i64 });
        }
    }
}

// -- JSON exporter/importer --

pub struct JsonExporter;

impl ProjectExporter for JsonExporter {
    fn extension(&self) -> &str { "json" }
    fn description(&self) -> &str { "Bande rythmo JSON" }

    fn export(&self, project: &Project, fps: f64, path: &Path) -> Result<(), String> {
        let data = ProjectData::from_project(project, fps);
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("JSON serialize error: {e}"))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Write error: {e}"))?;
        log::info!("Project exported to {}", path.display());
        Ok(())
    }
}

pub struct JsonImporter;

impl ProjectImporter for JsonImporter {
    fn extension(&self) -> &str { "json" }
    fn description(&self) -> &str { "Bande rythmo JSON" }

    fn import(&self, path: &Path) -> Result<ProjectData, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Read error: {e}"))?;
        let data: ProjectData = serde_json::from_str(&content)
            .map_err(|e| format!("JSON parse error: {e}"))?;
        log::info!("Project imported from {}", path.display());
        Ok(data)
    }
}
