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
    #[serde(default)]
    pub note: String,
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
            lines: project.lines().map(|l| LineData {
                start_frame: l.start_frame,
                duration_frames: l.duration_frames,
                y_slot: l.y_slot,
                text: l.text.clone(),
                character_name: l.character_name.clone(),
                character_color: l.character_color,
                note: l.note.clone(),
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
        if fps_ratio <= 0.0 {
            log::error!("Invalid fps_ratio {} (source_fps={}, target_fps={}), aborting import", fps_ratio, self.source_fps, target_fps);
            return;
        }
        project.clear_lines();
        project.markers.clear();
        project.known_characters.clear();

        for ch in &self.characters {
            project.known_characters.push(Character { name: ch.name.clone(), color: ch.color });
        }

        for l in &self.lines {
            let adjusted_start = (l.start_frame as f64 * fps_ratio) as i64;
            let adjusted_duration = (l.duration_frames as f64 * fps_ratio) as i64;
            if adjusted_duration <= 0 {
                log::warn!("Skipping line '{}': duration must be positive (got {})", l.text, adjusted_duration);
                continue;
            }
            if l.y_slot < 0.0 || l.y_slot > 1.0 {
                log::warn!("Skipping line '{}': y_slot out of range (got {})", l.text, l.y_slot);
                continue;
            }
            project.add_line_full(
                adjusted_start,
                adjusted_duration,
                l.y_slot,
                l.text.clone(), l.character_name.clone(), l.character_color,
            );
            // Apply note after creation
            if !l.note.is_empty() {
                if let Some(last_line) = project.lines().last() {
                    let note = l.note.clone();
                    let id = last_line.id;
                    if let Some(line) = project.get_line_mut(id) {
                        line.note = note;
                    }
                }
            }
        }

        for m in &self.markers {
            let kind = match m.kind.as_str() {
                "boucle" => MarkerKind::Boucle,
                "out" => MarkerKind::Out,
                "scene_change" => MarkerKind::SceneChange,
                "liaison_left" => MarkerKind::LiaisonLeft,
                "liaison_right" => MarkerKind::LiaisonRight,
                _ => {
                    log::warn!("Skipping unknown marker kind: {}", m.kind);
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_json() {
        let mut project = Project::new();
        project.add_line_full(0, 48, 0.5, "hello".into(), "Alice".into(), [1.0, 0.0, 0.0, 1.0]);
        project.markers.push(crate::rythmo_line::RythmoMarker {
            kind: crate::rythmo_line::MarkerKind::Boucle, frame: 100,
        });

        let data = ProjectData::from_project(&project, 24.0);
        let json = serde_json::to_string(&data).unwrap();
        let restored: ProjectData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.lines.len(), 1);
        assert_eq!(restored.lines[0].text, "hello");
        assert_eq!(restored.markers.len(), 1);
        assert_eq!(restored.source_fps, 24.0);
    }

    #[test]
    fn test_fps_conversion() {
        let data = ProjectData {
            source_fps: 24.0,
            lines: vec![LineData {
                start_frame: 24, duration_frames: 48, y_slot: 0.5,
                text: "test".into(), character_name: "A".into(),
                character_color: [1.0; 4],
            }],
            markers: vec![],
            characters: vec![],
        };

        let mut project = Project::new();
        data.apply_to_project(&mut project, 30.0);

        let line = project.lines().next().unwrap();
        // 24 frames at 24fps -> 1 second -> 30 frames at 30fps
        assert_eq!(line.start_frame, 30);
        // 48 frames at 24fps -> 2 seconds -> 60 frames at 30fps
        assert_eq!(line.duration_frames, 60);
    }

    #[test]
    fn test_apply_clears_existing() {
        let mut project = Project::new();
        project.add_line(0, 10, 0.25);
        project.add_line(10, 10, 0.5);
        assert_eq!(project.line_count(), 2);

        let data = ProjectData {
            source_fps: 24.0,
            lines: vec![LineData {
                start_frame: 0, duration_frames: 10, y_slot: 0.25,
                text: "new".into(), character_name: "X".into(),
                character_color: [1.0; 4],
            }],
            markers: vec![],
            characters: vec![],
        };
        data.apply_to_project(&mut project, 24.0);
        assert_eq!(project.line_count(), 1);
    }

    #[test]
    fn test_unknown_marker_skipped() {
        let data = ProjectData {
            source_fps: 24.0,
            lines: vec![],
            markers: vec![MarkerData { kind: "unknown_kind".into(), frame: 50 }],
            characters: vec![],
        };
        let mut project = Project::new();
        data.apply_to_project(&mut project, 24.0);
        assert_eq!(project.markers.len(), 0);
    }
}
