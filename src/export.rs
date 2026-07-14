#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use crate::constants::Y_SLOTS;
use crate::project::{Character, Project, ProjectSettings};
use crate::rythmo_drawing::{DrawingStroke, RythmoDrawing};
use crate::rythmo_line::{MarkerKind, RythmoMarker};
use crate::voice_actor::VoiceActor;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    #[serde(default)]
    pub markers: Vec<MarkerData>,
    #[serde(default)]
    pub characters: Vec<CharacterData>,
    #[serde(default)]
    pub voice_actors: Vec<VoiceActorData>,
    #[serde(default)]
    pub settings: ProjectSettings,
    #[serde(default)]
    pub drawing: DrawingData,
}

fn default_fps() -> f64 {
    24.0
}

#[derive(Serialize, Deserialize)]
pub struct LineData {
    pub start_frame: i64,
    pub duration_frames: i64,
    pub y_slot: f32,
    pub text: String,
    pub character_name: String,
    pub character_color: [f32; 4],
    #[serde(default)]
    pub voice_actor_names: Vec<String>,
    #[serde(default)]
    pub syllable_ratios: Vec<f32>,
    #[serde(default)]
    pub karaoke: bool,
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

#[derive(Serialize, Deserialize)]
pub struct VoiceActorData {
    pub name: String,
    #[serde(default)]
    pub icon_path: String,
    #[serde(default)]
    pub icon_png_base64: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct DrawingData {
    #[serde(default)]
    pub strokes: Vec<StrokeData>,
}

#[derive(Serialize, Deserialize)]
pub struct StrokeData {
    pub id: u64,
    pub points: Vec<(f64, f32)>,
    pub color: [f32; 4],
    pub radius_frac: f32,
}

// -- Conversion --

impl ProjectData {
    pub fn from_project(project: &Project, fps: f64) -> Self {
        Self {
            source_fps: fps,
            lines: project
                .lines()
                .map(|l| LineData {
                    start_frame: l.start_frame,
                    duration_frames: l.duration_frames,
                    y_slot: l.y_slot,
                    text: l.text.clone(),
                    character_name: l.character_name.clone(),
                    character_color: l.character_color,
                    voice_actor_names: l.voice_actor_names.clone(),
                    syllable_ratios: l.syllable_ratios.clone(),
                    karaoke: l.karaoke,
                    note: l.note.clone(),
                })
                .collect(),
            markers: project
                .markers()
                .iter()
                .map(|m| MarkerData {
                    kind: match &m.kind {
                        MarkerKind::Boucle => "boucle",
                        MarkerKind::Out => "out",
                        MarkerKind::SceneChange => "scene_change",
                        MarkerKind::LiaisonLeft => "liaison_left",
                        MarkerKind::LiaisonRight => "liaison_right",
                    }
                    .to_string(),
                    frame: m.frame,
                })
                .collect(),
            characters: project
                .known_characters()
                .iter()
                .map(|c| CharacterData {
                    name: c.name.clone(),
                    color: c.color,
                })
                .collect(),
            voice_actors: project
                .voice_actors()
                .iter()
                .map(|actor| VoiceActorData {
                    name: actor.name.clone(),
                    icon_path: actor.icon_path.clone(),
                    icon_png_base64: actor.icon_png_base64.clone(),
                })
                .collect(),
            settings: project.settings().clone(),
            drawing: DrawingData {
                strokes: project
                    .drawing()
                    .strokes
                    .iter()
                    .map(|s| StrokeData {
                        id: s.id,
                        points: s.points.clone(),
                        color: s.color,
                        radius_frac: s.radius_frac,
                    })
                    .collect(),
            },
        }
    }

    pub fn clamp_to_total_frames(&mut self, total_frames: i64) -> (usize, usize) {
        if total_frames <= 0 {
            return (0, 0);
        }

        let mut clipped_lines = 0;
        let before_lines = self.lines.len();

        self.lines.retain_mut(|line| {
            let mut changed = false;
            if line.start_frame < 0 {
                line.duration_frames += line.start_frame;
                line.start_frame = 0;
                changed = true;
            }

            if line.start_frame >= total_frames || line.duration_frames <= 0 {
                return false;
            }

            let max_duration = total_frames - line.start_frame;
            if line.duration_frames > max_duration {
                line.duration_frames = max_duration;
                changed = true;
            }

            if changed {
                clipped_lines += 1;
            }
            true
        });

        (clipped_lines, before_lines - self.lines.len())
    }

    pub fn apply_to_project(&self, project: &mut Project, target_fps: f64) {
        let fps_ratio = if self.source_fps > 0.0 && target_fps > 0.0 {
            target_fps / self.source_fps
        } else {
            1.0
        };
        if fps_ratio <= 0.0 {
            log::error!(
                "Invalid fps_ratio {} (source_fps={}, target_fps={}), aborting import",
                fps_ratio,
                self.source_fps,
                target_fps
            );
            return;
        }
        project.clear_lines();
        project.set_markers(Vec::new());
        project.set_known_characters(Vec::new());
        project.set_voice_actors(Vec::new());
        let mut settings = self.settings.clone();
        settings.source_audio_offset_frames =
            (settings.source_audio_offset_frames as f64 * fps_ratio) as i64;
        settings.instrumental_audio_offset_frames =
            (settings.instrumental_audio_offset_frames as f64 * fps_ratio) as i64;
        project.set_settings(settings);

        let characters = self
            .characters
            .iter()
            .map(|ch| Character {
                name: ch.name.clone(),
                color: ch.color,
            })
            .collect();
        project.set_known_characters(characters);

        let voice_actors = self
            .voice_actors
            .iter()
            .map(|actor| VoiceActor {
                name: actor.name.clone(),
                icon_path: actor.icon_path.clone(),
                icon_png_base64: actor.icon_png_base64.clone(),
            })
            .collect();
        project.set_voice_actors(voice_actors);

        for l in &self.lines {
            let adjusted_start = (l.start_frame as f64 * fps_ratio) as i64;
            let adjusted_duration = (l.duration_frames as f64 * fps_ratio) as i64;
            if adjusted_duration <= 0 {
                log::warn!(
                    "Skipping line '{}': duration must be positive (got {})",
                    l.text,
                    adjusted_duration
                );
                continue;
            }
            if l.y_slot < 0.0 || l.y_slot > 1.0 {
                log::warn!(
                    "Skipping line '{}': y_slot out of range (got {})",
                    l.text,
                    l.y_slot
                );
                continue;
            }
            let line_id = project.add_line_full_with_voice_actors(
                adjusted_start,
                adjusted_duration,
                l.y_slot,
                l.text.clone(),
                l.character_name.clone(),
                l.character_color,
                l.voice_actor_names.clone(),
            );
            // Apply note after creation
            if !l.note.is_empty() {
                if let Some(line) = project.get_line_mut(line_id) {
                    line.note = l.note.clone();
                }
            }
            if let Some(line) = project.get_line_mut(line_id) {
                line.karaoke = l.karaoke;
                line.syllable_ratios = l.syllable_ratios.clone();
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
            project.add_marker(RythmoMarker {
                kind,
                frame: (m.frame as f64 * fps_ratio) as i64,
            });
        }

        // Restore drawing
        let mut drawing = RythmoDrawing::new();
        for s in &self.drawing.strokes {
            drawing.add(DrawingStroke {
                id: s.id,
                points: s.points.clone(),
                color: s.color,
                radius_frac: s.radius_frac,
            });
        }
        project.set_drawing(drawing);
    }
}

// -- JSON exporter/importer --

pub struct JsonExporter;

impl ProjectExporter for JsonExporter {
    fn extension(&self) -> &str {
        "json"
    }
    fn description(&self) -> &str {
        "Bande rythmo JSON"
    }

    fn export(&self, project: &Project, fps: f64, path: &Path) -> Result<(), String> {
        let data = ProjectData::from_project(project, fps);
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("JSON serialize error: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Write error: {e}"))?;
        log::info!("Project exported to {}", path.display());
        Ok(())
    }
}

pub struct JsonImporter;

impl ProjectImporter for JsonImporter {
    fn extension(&self) -> &str {
        "json"
    }
    fn description(&self) -> &str {
        "Bande rythmo JSON"
    }

    fn import(&self, path: &Path) -> Result<ProjectData, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("Read error: {e}"))?;
        let data: ProjectData =
            serde_json::from_str(&content).map_err(|e| format!("JSON parse error: {e}"))?;
        log::info!("Project imported from {}", path.display());
        Ok(data)
    }
}

// -- SubRip .srt importer --

fn srt_time_to_frames(time: &str, fps: f64) -> Result<i64, String> {
    let time = time.trim();
    let mut parts = time.split(':');
    let hours: f64 = parts
        .next()
        .ok_or_else(|| format!("Invalid SRT time: '{time}'"))?
        .parse()
        .map_err(|_| format!("Invalid hours in SRT time: '{time}'"))?;
    let minutes: f64 = parts
        .next()
        .ok_or_else(|| format!("Invalid SRT time: '{time}'"))?
        .parse()
        .map_err(|_| format!("Invalid minutes in SRT time: '{time}'"))?;
    let seconds_part = parts
        .next()
        .ok_or_else(|| format!("Invalid SRT time: '{time}'"))?;
    if parts.next().is_some() {
        return Err(format!("Invalid SRT time: '{time}'"));
    }
    let seconds_part = seconds_part.replace(',', ".");
    let seconds: f64 = seconds_part
        .parse()
        .map_err(|_| format!("Invalid seconds in SRT time: '{time}'"))?;

    Ok(((hours * 3600.0 + minutes * 60.0 + seconds) * fps).round() as i64)
}

fn parse_srt_timing(line: &str, fps: f64) -> Result<(i64, i64), String> {
    let (start, end_with_settings) = line
        .split_once("-->")
        .ok_or_else(|| format!("Invalid SRT timing line: '{line}'"))?;
    let end = end_with_settings
        .split_whitespace()
        .next()
        .ok_or_else(|| format!("Invalid SRT timing line: '{line}'"))?;
    let start_frame = srt_time_to_frames(start, fps)?;
    let end_frame = srt_time_to_frames(end, fps)?;
    Ok((start_frame, end_frame))
}

fn push_srt_block(block_lines: &[&str], fps: f64, lines: &mut Vec<LineData>) -> Result<(), String> {
    if block_lines.is_empty() {
        return Ok(());
    }

    let timing_index = block_lines
        .iter()
        .position(|line| line.contains("-->"))
        .ok_or_else(|| {
            format!(
                "Invalid SRT block, missing timing line: '{}'",
                block_lines.join(" ")
            )
        })?;
    let (start_frame, end_frame) = parse_srt_timing(block_lines[timing_index], fps)?;
    let text = block_lines[timing_index + 1..].join(" ");
    if text.is_empty() {
        return Ok(());
    }

    lines.push(LineData {
        start_frame,
        duration_frames: (end_frame - start_frame).max(1),
        y_slot: Y_SLOTS[1],
        text,
        character_name: "Character".to_string(),
        character_color: [1.0, 1.0, 1.0, 1.0],
        voice_actor_names: Vec::new(),
        syllable_ratios: Vec::new(),
        karaoke: false,
        note: String::new(),
    });

    Ok(())
}

/// Import a SubRip .srt file and convert it to ProjectData.
/// Requires the video FPS to convert subtitle timestamps to frames.
pub fn import_srt(path: &Path, fps: f64) -> Result<ProjectData, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Read error: {e}"))?;
    let content = content.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut lines = Vec::new();
    let mut block_lines = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            push_srt_block(&block_lines, fps, &mut lines)?;
            block_lines.clear();
        } else {
            block_lines.push(line);
        }
    }
    push_srt_block(&block_lines, fps, &mut lines)?;

    log::info!("SRT imported: {} lines", lines.len());

    Ok(ProjectData {
        source_fps: fps,
        lines,
        markers: Vec::new(),
        characters: vec![CharacterData {
            name: "Character".to_string(),
            color: [1.0, 1.0, 1.0, 1.0],
        }],
        voice_actors: Vec::new(),
        settings: ProjectSettings::default(),
        drawing: DrawingData::default(),
    })
}

// -- Cappela .detx importer --

/// Parse a timecode "HH:MM:SS:FF" into a total frame number at the given FPS.
fn timecode_to_frames(tc: &str, fps: f64) -> Result<i64, String> {
    let parts: Vec<&str> = tc.split(':').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Invalid timecode format: '{tc}', expected HH:MM:SS:FF"
        ));
    }
    let h: i64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid hours in timecode: '{}'", parts[0]))?;
    let m: i64 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid minutes in timecode: '{}'", parts[1]))?;
    let s: i64 = parts[2]
        .parse()
        .map_err(|_| format!("Invalid seconds in timecode: '{}'", parts[2]))?;
    let f: i64 = parts[3]
        .parse()
        .map_err(|_| format!("Invalid frames in timecode: '{}'", parts[3]))?;
    Ok((h * 3600 + m * 60 + s) * fps as i64 + f)
}

/// Parse a hex color "#RRGGBB" into RGBA [f32; 4] (0.0–1.0).
fn hex_color_to_rgba(hex: &str) -> [f32; 4] {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return [1.0, 1.0, 1.0, 1.0]; // fallback white
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f32 / 255.0;
    [r, g, b, 1.0]
}

/// Import a Cappela .detx file and convert it to ProjectData.
/// Requires the video FPS to correctly interpret timecodes.
pub fn import_cappela(path: &Path, fps: f64) -> Result<ProjectData, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let content = std::fs::read_to_string(path).map_err(|e| format!("Read error: {e}"))?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    // Roles map: id -> (name, color)
    let mut roles: std::collections::HashMap<String, (String, [f32; 4])> =
        std::collections::HashMap::new();
    let mut lines: Vec<LineData> = Vec::new();
    let mut markers: Vec<MarkerData> = Vec::new();
    let mut characters: Vec<CharacterData> = Vec::new();

    // State machine for parsing
    #[derive(PartialEq)]
    enum ParseState {
        Root,
        InHeader,
        InRoles,
        InBody,
        InLine {
            role_id: String,
            track: i32,
            voice_off: bool,
        },
    }
    let mut state = ParseState::Root;

    // Per-line accumulation
    let mut line_texts: Vec<String> = Vec::new();
    let mut line_first_tc: Option<String> = None;
    let mut line_last_tc: Option<String> = None;

    // Global offsets
    let mut video_offset_frames: Option<i64> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attrs: std::collections::HashMap<String, String> = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .map(|a| {
                        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&a.value).to_string();
                        (key, val)
                    })
                    .collect();

                match state {
                    ParseState::Root => {
                        if tag == "header" {
                            state = ParseState::InHeader;
                        } else if tag == "roles" {
                            state = ParseState::InRoles;
                        } else if tag == "body" {
                            state = ParseState::InBody;
                        }
                    }
                    ParseState::InHeader => {
                        if tag == "videofile" {
                            if let Some(tc) = attrs.get("timestamp") {
                                video_offset_frames = timecode_to_frames(tc, fps).ok();
                                log::info!(
                                    "Cappela video offset found: {} frames",
                                    video_offset_frames.unwrap_or(0)
                                );
                            }
                        }
                    }
                    ParseState::InRoles => {
                        if tag == "role" {
                            if let (Some(id), Some(name)) = (attrs.get("id"), attrs.get("name")) {
                                let color = attrs
                                    .get("color")
                                    .map(|c| hex_color_to_rgba(c))
                                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                                roles.insert(id.clone(), (name.clone(), color));
                            }
                        }
                    }
                    ParseState::InBody => {
                        if tag == "line" {
                            let role_id = attrs.get("role").cloned().unwrap_or_default();
                            let track: i32 =
                                attrs.get("track").and_then(|t| t.parse().ok()).unwrap_or(0);
                            let voice_off = attrs.get("voice").map(|v| v == "off").unwrap_or(false);
                            state = ParseState::InLine {
                                role_id,
                                track,
                                voice_off,
                            };
                            line_texts.clear();
                            line_first_tc = None;
                            line_last_tc = None;
                        } else if tag == "loop" {
                            if let Some(tc) = attrs.get("timecode") {
                                let frame = timecode_to_frames(tc, fps).unwrap_or(0);
                                markers.push(MarkerData {
                                    kind: "boucle".to_string(),
                                    frame,
                                });
                            }
                        } else if tag == "shot" {
                            if let Some(tc) = attrs.get("timecode") {
                                let frame = timecode_to_frames(tc, fps).unwrap_or(0);
                                markers.push(MarkerData {
                                    kind: "scene_change".to_string(),
                                    frame,
                                });
                            }
                        }
                    }
                    ParseState::InLine { .. } => {
                        if tag == "lipsync" {
                            if let Some(tc) = attrs.get("timecode") {
                                if line_first_tc.is_none() {
                                    line_first_tc = Some(tc.clone());
                                }
                                line_last_tc = Some(tc.clone());
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if matches!(state, ParseState::InLine { .. }) {
                    let text = e
                        .unescape()
                        .map_err(|e| format!("XML text decode error: {e}"))?;
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        line_texts.push(trimmed.to_string());
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match state {
                    ParseState::InHeader => {
                        if tag == "header" {
                            state = ParseState::Root;
                        }
                    }
                    ParseState::InRoles => {
                        if tag == "roles" {
                            state = ParseState::Root;
                            // Build characters from roles
                            for (name, color) in roles.values() {
                                characters.push(CharacterData {
                                    name: name.clone(),
                                    color: *color,
                                });
                            }
                        }
                    }
                    ParseState::InBody => {
                        if tag == "body" {
                            state = ParseState::Root;
                        }
                    }
                    ParseState::InLine {
                        ref role_id,
                        track,
                        voice_off,
                    } => {
                        if tag == "line" {
                            let full_text = line_texts.join(" "); // Ajout d'espaces entre les morceaux
                            let (char_name, char_color) = roles
                                .get(role_id)
                                .cloned()
                                .unwrap_or((role_id.clone(), [1.0, 1.0, 1.0, 1.0]));

                            let y_slot = {
                                let slots = [0.25, 0.5, 0.75, 1.0];
                                slots.get(track as usize).copied().unwrap_or(0.5)
                            };

                            let start_frame = line_first_tc
                                .as_ref()
                                .and_then(|tc| timecode_to_frames(tc, fps).ok())
                                .unwrap_or(0);
                            let end_frame = line_last_tc
                                .as_ref()
                                .and_then(|tc| timecode_to_frames(tc, fps).ok())
                                .unwrap_or(start_frame);
                            let duration = (end_frame - start_frame).max(1);

                            let note = if voice_off {
                                "Voix off".to_string()
                            } else {
                                String::new()
                            };

                            lines.push(LineData {
                                start_frame,
                                duration_frames: duration,
                                y_slot,
                                text: full_text,
                                character_name: char_name,
                                character_color: char_color,
                                voice_actor_names: Vec::new(),
                                syllable_ratios: Vec::new(),
                                karaoke: false,
                                note,
                            });

                            state = ParseState::InBody;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "XML parse error at position {}: {e}",
                    reader.error_position()
                ))
            }
            _ => {}
        }
        buf.clear();
    }

    // Si on a un offset global (ex: 01:00:00:00), on le soustrait partout.
    // Si on n'a pas cet offset explicitement dans le header, vérifions la première ligne.
    let offset = video_offset_frames.unwrap_or_else(|| {
        if let Some(first_line) = lines.first() {
            // Parfois, le timecode commence à 10:00:00:00 ou 01:00:00:00 dans le doublage sans header videofile
            let h = first_line.start_frame / (3600 * fps as i64);
            h * 3600 * fps as i64
        } else {
            0
        }
    });

    if offset > 0 {
        for l in &mut lines {
            l.start_frame -= offset;
            if l.start_frame < 0 {
                l.start_frame = 0;
            }
        }
        for m in &mut markers {
            m.frame -= offset;
            if m.frame < 0 {
                m.frame = 0;
            }
        }
    }

    log::info!(
        "Cappela .detx imported: {} lines, {} markers, {} characters | Video offset applied: {}",
        lines.len(),
        markers.len(),
        characters.len(),
        offset
    );

    Ok(ProjectData {
        source_fps: fps,
        lines,
        markers,
        characters,
        voice_actors: Vec::new(),
        settings: ProjectSettings::default(),
        drawing: DrawingData::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_json() {
        let mut project = Project::new();
        project.add_line_full(
            0,
            48,
            0.5,
            "hello".into(),
            "Alice".into(),
            [1.0, 0.0, 0.0, 1.0],
        );
        project.add_marker(crate::rythmo_line::RythmoMarker {
            kind: crate::rythmo_line::MarkerKind::Boucle,
            frame: 100,
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
                start_frame: 24,
                duration_frames: 48,
                y_slot: 0.5,
                text: "test".into(),
                character_name: "A".into(),
                character_color: [1.0; 4],
                voice_actor_names: Vec::new(),
                syllable_ratios: Vec::new(),
                karaoke: false,
                note: String::new(),
            }],
            markers: vec![],
            characters: vec![],
            voice_actors: vec![],
            settings: ProjectSettings::default(),
            drawing: DrawingData::default(),
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
    fn test_clamp_to_total_frames() {
        let mut data = ProjectData {
            source_fps: 24.0,
            lines: vec![
                LineData {
                    start_frame: 90,
                    duration_frames: 20,
                    y_slot: 0.5,
                    text: "clipped".into(),
                    character_name: "A".into(),
                    character_color: [1.0; 4],
                    voice_actor_names: Vec::new(),
                    syllable_ratios: Vec::new(),
                    karaoke: false,
                    note: String::new(),
                },
                LineData {
                    start_frame: 100,
                    duration_frames: 10,
                    y_slot: 0.5,
                    text: "skipped".into(),
                    character_name: "A".into(),
                    character_color: [1.0; 4],
                    voice_actor_names: Vec::new(),
                    syllable_ratios: Vec::new(),
                    karaoke: false,
                    note: String::new(),
                },
            ],
            markers: vec![],
            characters: vec![],
            voice_actors: vec![],
            settings: ProjectSettings::default(),
            drawing: DrawingData::default(),
        };

        let (clipped, skipped) = data.clamp_to_total_frames(100);
        assert_eq!((clipped, skipped), (1, 1));
        assert_eq!(data.lines.len(), 1);
        assert_eq!(data.lines[0].text, "clipped");
        assert_eq!(data.lines[0].duration_frames, 10);
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
                start_frame: 0,
                duration_frames: 10,
                y_slot: 0.25,
                text: "new".into(),
                character_name: "X".into(),
                character_color: [1.0; 4],
                voice_actor_names: Vec::new(),
                syllable_ratios: Vec::new(),
                karaoke: false,
                note: String::new(),
            }],
            markers: vec![],
            characters: vec![],
            voice_actors: vec![],
            settings: ProjectSettings::default(),
            drawing: DrawingData { strokes: vec![] },
        };
        data.apply_to_project(&mut project, 24.0);
        assert_eq!(project.line_count(), 1);
    }

    #[test]
    fn test_apply_preserves_karaoke_syllable_ratios() {
        let data = ProjectData {
            source_fps: 24.0,
            lines: vec![
                LineData {
                    start_frame: 0,
                    duration_frames: 24,
                    y_slot: 0.25,
                    text: "karaoke".into(),
                    character_name: "A".into(),
                    character_color: [1.0; 4],
                    voice_actor_names: Vec::new(),
                    syllable_ratios: vec![0.2, 0.8],
                    karaoke: true,
                    note: String::new(),
                },
                LineData {
                    start_frame: 24,
                    duration_frames: 24,
                    y_slot: 0.5,
                    text: "normal".into(),
                    character_name: "B".into(),
                    character_color: [1.0; 4],
                    voice_actor_names: Vec::new(),
                    syllable_ratios: vec![0.3, 0.7],
                    karaoke: false,
                    note: String::new(),
                },
            ],
            markers: vec![],
            characters: vec![],
            voice_actors: vec![],
            settings: ProjectSettings::default(),
            drawing: DrawingData { strokes: vec![] },
        };

        let mut project = Project::new();
        data.apply_to_project(&mut project, 24.0);

        let lines: Vec<_> = project.lines().collect();
        assert!(lines[0].karaoke);
        assert_eq!(lines[0].syllable_ratios, vec![0.2, 0.8]);
        assert_eq!(lines[1].syllable_ratios, vec![0.3, 0.7]);
    }

    #[test]
    fn test_unknown_marker_skipped() {
        let data = ProjectData {
            source_fps: 24.0,
            lines: vec![],
            markers: vec![MarkerData {
                kind: "unknown_kind".into(),
                frame: 50,
            }],
            characters: vec![],
            voice_actors: vec![],
            settings: ProjectSettings::default(),
            drawing: DrawingData { strokes: vec![] },
        };
        let mut project = Project::new();
        data.apply_to_project(&mut project, 24.0);
        assert_eq!(project.markers().len(), 0);
    }

    #[test]
    fn test_timecode_to_frames() {
        assert_eq!(timecode_to_frames("00:00:00:00", 24.0).unwrap(), 0);
        assert_eq!(timecode_to_frames("00:00:01:00", 24.0).unwrap(), 24);
        assert_eq!(timecode_to_frames("00:01:00:00", 24.0).unwrap(), 24 * 60);
        assert_eq!(timecode_to_frames("01:00:00:00", 24.0).unwrap(), 24 * 3600);
        assert_eq!(
            timecode_to_frames("01:00:08:19", 24.0).unwrap(),
            24 * 3600 + 8 * 24 + 19
        );
    }

    #[test]
    fn test_srt_time_to_frames() {
        assert_eq!(srt_time_to_frames("00:00:00,000", 24.0).unwrap(), 0);
        assert_eq!(srt_time_to_frames("00:00:01,000", 24.0).unwrap(), 24);
        assert_eq!(srt_time_to_frames("00:01:00.500", 24.0).unwrap(), 1452);
    }

    #[test]
    fn test_import_srt_basic() {
        let srt = "1\n00:00:01,000 --> 00:00:03,500\nBonjour\nle monde\n\n2\n00:00:04.000 --> 00:00:04.100 position:50%\nSalut\n";
        let dir = std::env::temp_dir().join("coquerythmo_test_srt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.srt");
        std::fs::write(&path, srt).unwrap();

        let data = import_srt(&path, 24.0).unwrap();
        assert_eq!(data.lines.len(), 2);
        assert_eq!(data.lines[0].start_frame, 24);
        assert_eq!(data.lines[0].duration_frames, 60);
        assert_eq!(data.lines[0].y_slot, 0.5);
        assert_eq!(data.lines[0].text, "Bonjour le monde");
        assert_eq!(data.lines[0].character_name, "Character");
        assert_eq!(data.lines[1].start_frame, 96);
        assert_eq!(data.lines[1].duration_frames, 2);
        assert_eq!(data.characters.len(), 1);
        assert_eq!(data.characters[0].name, "Character");
    }

    #[test]
    fn test_hex_color_to_rgba() {
        assert_eq!(hex_color_to_rgba("#FF0000"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(hex_color_to_rgba("#00FF00"), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(hex_color_to_rgba("#0000FF"), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn test_import_cappela_basic() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<detx>\n  <roles>\n    <role id=\"inspecteur\" name=\"Inspecteur\" color=\"#FF0000\"/>\n  </roles>\n  <body>\n    <line role=\"inspecteur\" track=\"0\">\n      <lipsync timecode=\"01:00:08:19\" type=\"in_open\"/>\n      <text>Restez où vous </text>\n      <lipsync timecode=\"01:00:09:14\" type=\"mpb\"/>\n      <text>êtes !</text>\n      <lipsync timecode=\"01:00:09:21\" type=\"out_open\"/>\n    </line>\n  </body>\n</detx>";
        let dir = std::env::temp_dir().join("coquerythmo_test_cappela");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.detx");
        std::fs::write(&path, xml).unwrap();

        let data = import_cappela(&path, 24.0).unwrap();
        assert_eq!(data.lines.len(), 1);
        assert_eq!(data.lines[0].text, "Restez où vous êtes !");
        assert_eq!(data.lines[0].character_name, "Inspecteur");
        assert_eq!(data.lines[0].character_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(data.lines[0].y_slot, 0.25); // track 0

        let start = 8 * 24 + 19;
        let end = 9 * 24 + 21;
        assert_eq!(data.lines[0].start_frame, start);
        assert_eq!(data.lines[0].duration_frames, end - start);

        assert_eq!(data.characters.len(), 1);
        assert_eq!(data.characters[0].name, "Inspecteur");
    }

    #[test]
    fn test_import_cappela_markers() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<detx>\n  <roles></roles>\n  <body>\n    <loop timecode=\"00:00:10:00\"/>\n    <shot timecode=\"00:00:20:00\"/>\n  </body>\n</detx>";
        let dir = std::env::temp_dir().join("coquerythmo_test_cappela2");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_markers.detx");
        std::fs::write(&path, xml).unwrap();

        let data = import_cappela(&path, 24.0).unwrap();
        assert_eq!(data.markers.len(), 2);
        assert_eq!(data.markers[0].kind, "boucle");
        assert_eq!(data.markers[0].frame, 10 * 24);
        assert_eq!(data.markers[1].kind, "scene_change");
        assert_eq!(data.markers[1].frame, 20 * 24);
    }

    #[test]
    fn test_import_cappela_voice_off() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<detx>\n  <roles>\n    <role id=\"narrator\" name=\"Narrateur\" color=\"#0000FF\"/>\n  </roles>\n  <body>\n    <line role=\"narrator\" track=\"1\" voice=\"off\">\n      <lipsync timecode=\"00:00:05:00\" type=\"in_open\"/>\n      <text>Il était une fois</text>\n      <lipsync timecode=\"00:00:08:00\" type=\"out_open\"/>\n    </line>\n  </body>\n</detx>";
        let dir = std::env::temp_dir().join("coquerythmo_test_cappela3");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_voiceoff.detx");
        std::fs::write(&path, xml).unwrap();

        let data = import_cappela(&path, 24.0).unwrap();
        assert_eq!(data.lines[0].note, "Voix off");
        assert_eq!(data.lines[0].y_slot, 0.5); // track 1
    }
}
