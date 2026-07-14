//! Project domain model and mutation helpers.
#![allow(clippy::too_many_arguments)]

use crate::constants::JS_MAX_SAFE_INTEGER;
use crate::rythmo_drawing::{DrawingStroke, RythmoDrawing};
use crate::rythmo_line::{RythmoLine, RythmoMarker};
use crate::voice_actor::VoiceActor;
use std::collections::{hash_map::Entry, HashMap};

const DEFAULT_COLORS: &[[f32; 4]] = &[
    [0.35, 0.55, 0.90, 1.0], // blue
    [0.90, 0.40, 0.35, 1.0], // red
    [0.35, 0.80, 0.45, 1.0], // green
    [0.90, 0.75, 0.30, 1.0], // yellow
    [0.70, 0.40, 0.85, 1.0], // purple
    [0.90, 0.55, 0.30, 1.0], // orange
    [0.40, 0.80, 0.80, 1.0], // cyan
    [0.85, 0.45, 0.65, 1.0], // pink
];

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrumental_audio_path: Option<String>,
    #[serde(default)]
    pub source_audio_offset_frames: i64,
    #[serde(default)]
    pub instrumental_audio_offset_frames: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineCharacterNameChange {
    pub line_id: u64,
    pub old_name: String,
    pub new_name: String,
}

pub struct Project {
    line_map: HashMap<u64, RythmoLine>,
    line_order: Vec<u64>,
    markers: Vec<RythmoMarker>,
    known_characters: Vec<Character>,
    voice_actors: Vec<VoiceActor>,
    drawing: RythmoDrawing,
    color_index: usize,
    revision: u64,
    settings: ProjectSettings,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

impl Project {
    pub fn new() -> Self {
        Self {
            line_map: HashMap::new(),
            line_order: Vec::new(),
            markers: Vec::new(),
            known_characters: Vec::new(),
            voice_actors: Vec::new(),
            drawing: RythmoDrawing::new(),
            color_index: 0,
            revision: 0,
            settings: ProjectSettings::default(),
        }
    }

    pub fn snapshot(&self) -> Self {
        Self {
            line_map: self.line_map.clone(),
            line_order: self.line_order.clone(),
            markers: self.markers.clone(),
            known_characters: self.known_characters.clone(),
            voice_actors: self.voice_actors.clone(),
            drawing: self.drawing.clone(),
            color_index: self.color_index,
            revision: self.revision,
            settings: self.settings.clone(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Read-only access to project collections. All changes go through the
    /// domain methods below so revision invalidation cannot be skipped.
    pub fn markers(&self) -> &[RythmoMarker] {
        &self.markers
    }

    pub fn marker(&self, index: usize) -> Option<&RythmoMarker> {
        self.markers.get(index)
    }

    pub fn marker_count(&self) -> usize {
        self.markers.len()
    }

    pub fn known_characters(&self) -> &[Character] {
        &self.known_characters
    }

    pub fn voice_actors(&self) -> &[VoiceActor] {
        &self.voice_actors
    }

    pub fn voice_actor(&self, index: usize) -> Option<&VoiceActor> {
        self.voice_actors.get(index)
    }

    pub fn drawing(&self) -> &RythmoDrawing {
        &self.drawing
    }

    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    // -- Line access (O(1) via HashMap) --

    pub fn get_line(&self, id: u64) -> Option<&RythmoLine> {
        self.line_map.get(&id)
    }

    pub fn get_line_mut(&mut self, id: u64) -> Option<&mut RythmoLine> {
        if self.line_map.contains_key(&id) {
            self.bump_revision();
        }
        self.line_map.get_mut(&id)
    }

    /// Iterate over lines in insertion order.
    pub fn lines(&self) -> impl Iterator<Item = &RythmoLine> {
        self.line_order
            .iter()
            .filter_map(move |id| self.line_map.get(id))
    }

    /// Collect all lines as a Vec (for serialization or cloning).
    pub fn lines_vec(&self) -> Vec<RythmoLine> {
        self.lines().cloned().collect()
    }

    pub fn line_count(&self) -> usize {
        self.line_map.len()
    }

    pub fn line_index(&self, id: u64) -> Option<usize> {
        self.line_order.iter().position(|&line_id| line_id == id)
    }

    pub fn generate_line_id(&self) -> u64 {
        loop {
            let id = rand::random::<u64>() % JS_MAX_SAFE_INTEGER;
            if !self.line_map.contains_key(&id) {
                return id;
            }
        }
    }

    // -- Line mutation --

    pub fn add_line(&mut self, start_frame: i64, duration_frames: i64, y_slot: f32) -> u64 {
        let id = rand::random::<u64>() % JS_MAX_SAFE_INTEGER;
        let color = self.next_color();

        // Find the last line on the same track (y_slot) that ends before this one starts
        let (char_name, char_color, voice_actor_names) = self
            .lines()
            .filter(|l| (l.y_slot - y_slot).abs() < 0.01 && l.end_frame() <= start_frame)
            .max_by_key(|l| l.end_frame())
            .map(|l| {
                (
                    l.character_name.clone(),
                    l.character_color,
                    l.voice_actor_names.clone(),
                )
            })
            .or_else(|| {
                // Fallback: any line on the same track
                self.lines()
                    .filter(|l| (l.y_slot - y_slot).abs() < 0.01)
                    .last()
                    .map(|l| {
                        (
                            l.character_name.clone(),
                            l.character_color,
                            l.voice_actor_names.clone(),
                        )
                    })
            })
            .or_else(|| {
                // Fallback: first known character
                self.known_characters
                    .first()
                    .map(|c| (c.name.clone(), c.color, Vec::new()))
            })
            .unwrap_or_else(|| ("Character".to_string(), color, Vec::new()));

        let line = RythmoLine {
            id,
            start_frame,
            duration_frames,
            y_slot,
            text: String::new(),
            character_name: char_name,
            character_color: char_color,
            voice_actor_names,
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
        };
        self.line_map.insert(id, line);
        self.line_order.push(id);
        self.bump_revision();
        id
    }

    pub fn add_line_full(
        &mut self,
        start_frame: i64,
        duration_frames: i64,
        y_slot: f32,
        text: String,
        character_name: String,
        character_color: [f32; 4],
    ) -> u64 {
        self.add_line_full_with_voice_actors(
            start_frame,
            duration_frames,
            y_slot,
            text,
            character_name,
            character_color,
            Vec::new(),
        )
    }

    pub fn add_line_full_with_voice_actors(
        &mut self,
        start_frame: i64,
        duration_frames: i64,
        y_slot: f32,
        text: String,
        character_name: String,
        character_color: [f32; 4],
        voice_actor_names: Vec<String>,
    ) -> u64 {
        let id = rand::random::<u64>() % JS_MAX_SAFE_INTEGER;
        let line = RythmoLine {
            id,
            start_frame,
            duration_frames,
            y_slot,
            text,
            character_name,
            character_color,
            voice_actor_names: Self::normalized_voice_actor_names(voice_actor_names),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
        };
        self.line_map.insert(id, line);
        self.line_order.push(id);
        self.bump_revision();
        id
    }

    pub fn duplicate_line_from(
        &mut self,
        snapshot: &RythmoLine,
        start_frame: i64,
    ) -> (RythmoLine, usize) {
        let mut line = snapshot.clone();
        line.id = rand::random::<u64>() % JS_MAX_SAFE_INTEGER;
        line.start_frame = start_frame;
        let index = self.line_order.len();
        self.insert_line(line.clone());
        (line, index)
    }

    /// Insert a line with a pre-existing ID (for network sync).
    pub fn insert_line(&mut self, line: RythmoLine) {
        let id = line.id;
        self.line_map.insert(id, line);
        if !self.line_order.contains(&id) {
            self.line_order.push(id);
        }
        self.bump_revision();
    }

    /// Insert a line at a specific position (for undo).
    pub fn insert_line_at(&mut self, index: usize, line: RythmoLine) {
        let id = line.id;
        self.line_map.insert(id, line);
        let idx = index.min(self.line_order.len());
        self.line_order.insert(idx, id);
        self.bump_revision();
    }

    pub fn upsert_line_at(&mut self, index: usize, line: RythmoLine) {
        let id = line.id;
        if let Entry::Occupied(mut entry) = self.line_map.entry(id) {
            entry.insert(line);
            self.bump_revision();
        } else {
            self.insert_line_at(index, line);
        }
    }

    /// Remove a line by ID. Returns the line and its index if found.
    pub fn remove_line(&mut self, id: u64) -> Option<(RythmoLine, usize)> {
        let line = self.line_map.remove(&id)?;
        let index = self.line_order.iter().position(|&i| i == id).unwrap_or(0);
        self.line_order.remove(index);
        self.bump_revision();
        Some((line, index))
    }

    /// Remove lines that don't match a predicate.
    pub fn retain_lines<F: Fn(&RythmoLine) -> bool>(&mut self, f: F) {
        self.line_order.retain(|id| {
            if let Some(line) = self.line_map.get(id) {
                if f(line) {
                    return true;
                }
            }
            false
        });
        self.line_map.retain(|_, line| f(line));
        self.bump_revision();
    }

    /// Clear all lines.
    pub fn clear_lines(&mut self) {
        self.line_map.clear();
        self.line_order.clear();
        self.bump_revision();
    }

    pub fn add_marker(&mut self, marker: RythmoMarker) -> usize {
        self.markers.push(marker);
        self.bump_revision();
        self.markers.len() - 1
    }

    pub fn insert_marker(&mut self, index: usize, marker: RythmoMarker) {
        let index = index.min(self.markers.len());
        self.markers.insert(index, marker);
        self.bump_revision();
    }

    pub fn remove_marker_at(&mut self, index: usize) -> Option<RythmoMarker> {
        if index >= self.markers.len() {
            return None;
        }
        let marker = self.markers.remove(index);
        self.bump_revision();
        Some(marker)
    }

    pub fn move_marker(&mut self, index: usize, frame: i64) -> bool {
        let Some(marker) = self.markers.get_mut(index) else {
            return false;
        };
        marker.frame = frame;
        self.bump_revision();
        true
    }

    pub fn retain_markers<F: FnMut(&RythmoMarker) -> bool>(&mut self, f: F) {
        self.markers.retain(f);
        self.bump_revision();
    }

    pub fn set_markers(&mut self, markers: Vec<RythmoMarker>) {
        self.markers = markers;
        self.bump_revision();
    }

    pub fn set_voice_actors(&mut self, voice_actors: Vec<VoiceActor>) {
        self.voice_actors = voice_actors;
        self.bump_revision();
    }

    /// Returns true if the project has no lines, no markers, and no characters.
    pub fn is_empty(&self) -> bool {
        self.line_map.is_empty()
            && self.markers.is_empty()
            && self.known_characters.is_empty()
            && self.voice_actors.is_empty()
    }

    /// Full reset: clear lines, markers, characters, and color index.
    pub fn reset(&mut self) {
        self.line_map.clear();
        self.line_order.clear();
        self.markers.clear();
        self.known_characters.clear();
        self.voice_actors.clear();
        self.settings = ProjectSettings::default();
        self.color_index = 0;
        self.bump_revision();
    }

    pub fn set_settings(&mut self, settings: ProjectSettings) {
        if self.settings != settings {
            self.settings = settings;
            self.bump_revision();
        }
    }

    pub fn add_drawing_stroke(&mut self, stroke: DrawingStroke) {
        self.drawing.add(stroke);
        self.bump_revision();
    }

    pub fn add_drawing_strokes(&mut self, strokes: &[DrawingStroke]) -> bool {
        let mut changed = false;
        for stroke in strokes {
            if self.drawing.get(stroke.id).is_none() {
                self.drawing.add(stroke.clone());
                changed = true;
            }
        }
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn remove_drawing_stroke(&mut self, id: u64) -> Option<DrawingStroke> {
        let removed = self.drawing.remove(id);
        if removed.is_some() {
            self.bump_revision();
        }
        removed
    }

    pub fn remove_drawing_strokes(&mut self, ids: &[u64]) -> bool {
        let mut changed = false;
        for id in ids {
            if self.drawing.remove(*id).is_some() {
                changed = true;
            }
        }
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn set_drawing_stroke_points(&mut self, id: u64, points: Vec<(f64, f32)>) -> bool {
        let Some(stroke) = self.drawing.get_mut(id) else {
            return false;
        };
        stroke.points = points;
        self.bump_revision();
        true
    }

    pub fn set_drawing_strokes_points(
        &mut self,
        ids: &[u64],
        points: &[Vec<(f64, f32)>],
    ) -> bool {
        let mut changed = false;
        for (index, id) in ids.iter().enumerate() {
            if let Some(new_points) = points.get(index) {
                if let Some(stroke) = self.drawing.get_mut(*id) {
                    stroke.points = new_points.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn set_drawing(&mut self, drawing: RythmoDrawing) {
        self.drawing = drawing;
        self.bump_revision();
    }

    pub fn adjust_source_audio_offset(&mut self, delta_frames: i64) {
        if delta_frames != 0 {
            self.settings.source_audio_offset_frames += delta_frames;
            self.bump_revision();
        }
    }

    pub fn adjust_instrumental_audio_offset(&mut self, delta_frames: i64) {
        if delta_frames != 0 {
            self.settings.instrumental_audio_offset_frames += delta_frames;
            self.bump_revision();
        }
    }

    // -- Character management --

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        self.upsert_known_character(&name, color);
        if let Some(line) = self.get_line_mut(line_id) {
            line.character_name = name;
            line.character_color = color;
        }
    }

    pub fn set_character_with_voice_actors(
        &mut self,
        line_id: u64,
        name: String,
        color: [f32; 4],
        voice_actor_names: Vec<String>,
    ) {
        self.upsert_known_character(&name, color);
        let voice_actor_names = Self::normalized_voice_actor_names(voice_actor_names);
        if let Some(line) = self.get_line_mut(line_id) {
            line.character_name = name;
            line.character_color = color;
            line.voice_actor_names = voice_actor_names;
        }
    }

    fn upsert_known_character(&mut self, name: &str, color: [f32; 4]) {
        // Update or add to known characters
        if !name.is_empty() {
            if let Some(existing) = self.known_characters.iter_mut().find(|c| c.name == name) {
                existing.color = color;
                self.bump_revision();
            } else {
                self.known_characters.push(Character {
                    name: name.to_string(),
                    color,
                });
                self.bump_revision();
            }
        }
    }

    pub fn find_character(&self, name: &str) -> Option<&Character> {
        self.known_characters.iter().find(|c| c.name == name)
    }

    pub fn set_known_characters(&mut self, known_characters: Vec<Character>) {
        self.known_characters = known_characters;
        self.bump_revision();
    }

    pub fn character_names_from_lines(&self) -> Vec<String> {
        let mut names = Vec::new();
        for line in self.lines() {
            if line.character_name.trim().is_empty() {
                continue;
            }
            if !names
                .iter()
                .any(|existing| existing == &line.character_name)
            {
                names.push(line.character_name.clone());
            }
        }
        names
    }

    pub fn apply_character_name_changes(
        &mut self,
        changes: &[LineCharacterNameChange],
        use_new: bool,
    ) {
        let mut changed = false;
        for change in changes {
            if let Some(line) = self.line_map.get_mut(&change.line_id) {
                let target_name = if use_new {
                    &change.new_name
                } else {
                    &change.old_name
                };
                if line.character_name != *target_name {
                    line.character_name = target_name.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.bump_revision();
        }
    }

    pub fn autocomplete(&self, prefix: &str) -> Vec<&Character> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let lower = prefix.to_lowercase();
        self.known_characters
            .iter()
            .filter(|c| {
                let cl = c.name.to_lowercase();
                cl.starts_with(&lower) && cl != lower // exclude exact match
            })
            .collect()
    }

    pub fn find_voice_actor(&self, name: &str) -> Option<&VoiceActor> {
        self.voice_actors.iter().find(|a| a.name == name)
    }

    pub fn add_voice_actor(&mut self, actor: VoiceActor) -> bool {
        if actor.name.trim().is_empty() || self.find_voice_actor(&actor.name).is_some() {
            return false;
        }
        self.voice_actors.push(actor);
        self.bump_revision();
        true
    }

    pub fn upsert_voice_actor(&mut self, actor: VoiceActor) {
        if let Some(existing) = self.voice_actors.iter_mut().find(|a| a.name == actor.name) {
            *existing = actor;
            self.bump_revision();
        } else if !actor.name.trim().is_empty() {
            self.voice_actors.push(actor);
            self.bump_revision();
        }
    }

    pub fn remove_voice_actor(&mut self, name: &str) {
        self.voice_actors.retain(|actor| actor.name != name);
        for line in self.line_map.values_mut() {
            line.voice_actor_names
                .retain(|actor_name| actor_name != name);
        }
        self.bump_revision();
    }

    pub fn set_line_voice_actor_names(&mut self, line_id: u64, names: Vec<String>) {
        if let Some(line) = self.get_line_mut(line_id) {
            line.voice_actor_names = Self::normalized_voice_actor_names(names);
        }
    }

    pub fn voice_actor_names_for_character(
        &self,
        character_name: &str,
        exclude_line_id: u64,
    ) -> Vec<String> {
        if character_name.trim().is_empty() {
            return Vec::new();
        }

        self.lines()
            .find(|line| line.id != exclude_line_id && line.character_name == character_name)
            .map(|line| line.voice_actor_names.clone())
            .unwrap_or_default()
    }

    pub fn normalized_voice_actor_names(names: Vec<String>) -> Vec<String> {
        let mut out = Vec::new();
        for name in names {
            let trimmed = name.trim();
            if !trimmed.is_empty() && !out.iter().any(|existing| existing == trimmed) {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    pub fn with_voice_actor_assignment(
        current: &[String],
        actor_name: &str,
        assign: bool,
    ) -> Vec<String> {
        let mut next = Self::normalized_voice_actor_names(current.to_vec());
        if assign {
            if !next.iter().any(|name| name == actor_name) && !actor_name.trim().is_empty() {
                next.push(actor_name.trim().to_string());
            }
        } else {
            next.retain(|name| name != actor_name);
        }
        next
    }

    pub fn has_voice_actor_assignments(&self) -> bool {
        self.lines().any(|line| !line.voice_actor_names.is_empty())
    }

    fn next_color(&mut self) -> [f32; 4] {
        let color = DEFAULT_COLORS[self.color_index % DEFAULT_COLORS.len()];
        self.color_index += 1;
        color
    }

    pub fn snap_y(y_ratio: f32) -> f32 {
        (y_ratio * 4.0).round() / 4.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_line() {
        let mut p = Project::new();
        let id = p.add_line(0, 48, 0.5);
        assert_eq!(p.line_count(), 1);
        let line = p.get_line(id).unwrap();
        assert_eq!(line.start_frame, 0);
        assert_eq!(line.duration_frames, 48);
        assert_eq!(line.y_slot, 0.5);
    }

    #[test]
    fn test_add_line_full() {
        let mut p = Project::new();
        let id = p.add_line_full(
            10,
            20,
            0.25,
            "hello".into(),
            "Alice".into(),
            [1.0, 0.0, 0.0, 1.0],
        );
        let line = p.get_line(id).unwrap();
        assert_eq!(line.text, "hello");
        assert_eq!(line.character_name, "Alice");
    }

    #[test]
    fn test_remove_line() {
        let mut p = Project::new();
        let id = p.add_line(0, 48, 0.5);
        assert_eq!(p.line_count(), 1);
        let (removed, index) = p.remove_line(id).unwrap();
        assert_eq!(removed.id, id);
        assert_eq!(index, 0);
        assert_eq!(p.line_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut p = Project::new();
        assert!(p.remove_line(999).is_none());
    }

    #[test]
    fn test_insert_line_at() {
        let mut p = Project::new();
        let id1 = p.add_line(0, 10, 0.25);
        let id2 = p.add_line(20, 10, 0.5);
        // Insert at position 1 (between id1 and id2)
        let line = crate::rythmo_line::RythmoLine {
            id: 42,
            start_frame: 10,
            duration_frames: 10,
            y_slot: 0.75,
            text: String::new(),
            character_name: String::new(),
            character_color: [1.0; 4],
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
        };
        p.insert_line_at(1, line);
        let ids: Vec<u64> = p.lines().map(|l| l.id).collect();
        assert_eq!(ids[0], id1);
        assert_eq!(ids[1], 42);
        assert_eq!(ids[2], id2);
    }

    #[test]
    fn test_retain_lines() {
        let mut p = Project::new();
        p.add_line_full(0, 10, 0.25, "keep".into(), "A".into(), [1.0; 4]);
        p.add_line_full(10, 10, 0.5, "drop".into(), "B".into(), [0.0; 4]);
        p.add_line_full(20, 10, 0.75, "keep".into(), "C".into(), [1.0; 4]);
        p.retain_lines(|l| l.text == "keep");
        assert_eq!(p.line_count(), 2);
    }

    #[test]
    fn test_get_line_mut() {
        let mut p = Project::new();
        let id = p.add_line(0, 48, 0.5);
        p.get_line_mut(id).unwrap().text = "modified".into();
        assert_eq!(p.get_line(id).unwrap().text, "modified");
    }

    #[test]
    fn test_revision_changes_on_mutations() {
        let mut p = Project::new();
        let initial = p.revision();
        let id = p.add_line(0, 48, 0.5);
        assert_ne!(p.revision(), initial);

        let after_add = p.revision();
        p.get_line_mut(id).unwrap().text = "modified".into();
        assert_ne!(p.revision(), after_add);

        let after_line_mut = p.revision();
        p.add_marker(crate::rythmo_line::RythmoMarker {
            kind: crate::rythmo_line::MarkerKind::Boucle,
            frame: 12,
        });
        assert_ne!(p.revision(), after_line_mut);

        let after_marker = p.revision();
        p.reset();
        assert_ne!(p.revision(), after_marker);
    }

    #[test]
    fn test_snapshot() {
        let mut p = Project::new();
        p.add_line(0, 10, 0.25);
        let snap = p.snapshot();
        assert_eq!(snap.line_count(), 1);
        // Modifying original doesn't affect snapshot
        p.add_line(10, 10, 0.5);
        assert_eq!(snap.line_count(), 1);
        assert_eq!(p.line_count(), 2);
    }

    #[test]
    fn test_set_character() {
        let mut p = Project::new();
        let id = p.add_line(0, 48, 0.5);
        p.set_character(id, "Alice".into(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(p.get_line(id).unwrap().character_name, "Alice");
        assert_eq!(p.known_characters.len(), 1);
        assert_eq!(p.known_characters[0].name, "Alice");
    }

    #[test]
    fn test_voice_actor_names_for_character() {
        let mut p = Project::new();
        let alice_id = p.add_line_full_with_voice_actors(
            0,
            48,
            0.25,
            "hello".into(),
            "Alice".into(),
            [1.0, 0.0, 0.0, 1.0],
            vec!["Alice Actor".into()],
        );
        let bob_id = p.add_line_full(
            48,
            48,
            0.25,
            "world".into(),
            "Bob".into(),
            [0.0, 1.0, 0.0, 1.0],
        );

        assert_eq!(
            p.voice_actor_names_for_character("Alice", bob_id),
            vec!["Alice Actor".to_string()]
        );
        assert!(p
            .voice_actor_names_for_character("Alice", alice_id)
            .is_empty());
        assert!(p
            .voice_actor_names_for_character("Unknown", bob_id)
            .is_empty());
    }

    #[test]
    fn test_autocomplete() {
        let mut p = Project::new();
        let id = p.add_line(0, 48, 0.5);
        p.set_character(id, "Alice".into(), [1.0; 4]);
        let results = p.autocomplete("al");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice");
        // Exact match excluded
        assert!(p.autocomplete("alice").is_empty());
    }

    #[test]
    fn test_snap_y() {
        assert_eq!(Project::snap_y(0.0), 0.0);
        assert_eq!(Project::snap_y(0.3), 0.25);
        assert_eq!(Project::snap_y(0.6), 0.5);
        assert_eq!(Project::snap_y(0.9), 1.0);
    }
}
