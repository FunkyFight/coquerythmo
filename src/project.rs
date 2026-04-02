use crate::rythmo_line::{RythmoLine, RythmoMarker};

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

pub struct Character {
    pub name: String,
    pub color: [f32; 4],
}

pub struct Project {
    pub lines: Vec<RythmoLine>,
    pub markers: Vec<RythmoMarker>,
    pub known_characters: Vec<Character>,
    next_id: u64,
    color_index: usize,
}

impl Project {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            markers: Vec::new(),
            known_characters: Vec::new(),
            next_id: 1,
            color_index: 0,
        }
    }

    pub fn add_line(&mut self, start_frame: i64, duration_frames: i64, y_slot: f32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let color = self.next_color();
        let (char_name, char_color) = if let Some(first) = self.known_characters.first() {
            (first.name.clone(), first.color)
        } else {
            ("Character".to_string(), color)
        };

        self.lines.push(RythmoLine {
            id,
            start_frame,
            duration_frames,
            y_slot,
            text: String::new(),
            character_name: char_name,
            character_color: char_color,
        });
        id
    }

    pub fn get_line(&self, id: u64) -> Option<&RythmoLine> {
        self.lines.iter().find(|l| l.id == id)
    }

    pub fn get_line_mut(&mut self, id: u64) -> Option<&mut RythmoLine> {
        self.lines.iter_mut().find(|l| l.id == id)
    }

    pub fn set_character(&mut self, line_id: u64, name: String, color: [f32; 4]) {
        // Update or add to known characters
        if !name.is_empty() {
            if let Some(existing) = self.known_characters.iter_mut().find(|c| c.name == name) {
                existing.color = color;
            } else {
                self.known_characters.push(Character { name: name.clone(), color });
            }
        }
        if let Some(line) = self.get_line_mut(line_id) {
            line.character_name = name;
            line.character_color = color;
        }
    }

    pub fn find_character(&self, name: &str) -> Option<&Character> {
        self.known_characters.iter().find(|c| c.name == name)
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

    fn next_color(&mut self) -> [f32; 4] {
        let color = DEFAULT_COLORS[self.color_index % DEFAULT_COLORS.len()];
        self.color_index += 1;
        color
    }

    pub fn snap_y(y_ratio: f32) -> f32 {
        (y_ratio * 4.0).round() / 4.0
    }
}
