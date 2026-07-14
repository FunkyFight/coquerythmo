//! Text input state and rendering.
#![allow(clippy::too_many_arguments)]

use std::time::{Duration, Instant};

use super::primitives::{HAlign, QuadInstance, Rect};

pub enum TextInputAction {
    Changed(String),
    Finished,
}

struct TextEditSnapshot {
    text: String,
    cursor_pos: usize,
    selection: Option<(usize, usize)>,
}

pub struct TextInputState {
    pub active: bool,
    pub cursor_pos: usize,
    cursor_blink: Instant,
    pub selection: Option<(usize, usize)>,
    undo_stack: Vec<TextEditSnapshot>,
}

impl Default for TextInputState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TextInputMetrics {
    pub padding_x: f32,
    pub font_size: f32,
    pub h_align: HAlign,
}

impl TextInputMetrics {
    pub const fn new(font_size: f32, padding_x: f32, h_align: HAlign) -> Self {
        Self {
            padding_x,
            font_size,
            h_align,
        }
    }

    pub const fn left(font_size: f32, padding_x: f32) -> Self {
        Self::new(font_size, padding_x, HAlign::Left)
    }

    pub const fn center(font_size: f32, padding_x: f32) -> Self {
        Self::new(font_size, padding_x, HAlign::Center)
    }
}

impl TextInputState {
    const MAX_UNDO: usize = 100;

    pub fn new() -> Self {
        Self {
            active: false,
            cursor_pos: 0,
            cursor_blink: Instant::now(),
            selection: None,
            undo_stack: Vec::new(),
        }
    }

    pub fn activate(&mut self, text: &str) {
        self.active = true;
        self.cursor_pos = text.chars().count();
        self.cursor_blink = Instant::now();
        self.selection = None;
        self.undo_stack.clear();
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        self.selection = None;
        self.undo_stack.clear();
    }

    pub fn cursor_visible(&self) -> bool {
        self.active && self.cursor_blink.elapsed().as_millis() % 1000 < 500
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<Instant> {
        if !self.active {
            return None;
        }

        let elapsed_ms = self
            .cursor_blink
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let next_ms = (elapsed_ms / 500 + 1) * 500;
        self.cursor_blink
            .checked_add(Duration::from_millis(next_ms))
            .or_else(|| Some(Instant::now()))
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection.map(|(s, e)| (s.min(e), s.max(e)))
    }

    pub fn select_range(&mut self, start: usize, end: usize) {
        self.selection = if start == end {
            None
        } else {
            Some((start, end))
        };
        self.cursor_pos = end;
        self.cursor_blink = Instant::now();
    }

    pub fn selected_text(&self, text: &str) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let byte_start = text
            .char_indices()
            .nth(start)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        let byte_end = text
            .char_indices()
            .nth(end)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        Some(text[byte_start..byte_end].to_string())
    }

    pub fn select_word_at(&mut self, text: &str, pos: usize) {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return;
        }
        let mut idx = pos.min(chars.len().saturating_sub(1));
        if idx > 0 && is_word_separator(chars[idx]) {
            idx -= 1;
        }
        if is_word_separator(chars[idx]) {
            self.set_cursor_pos(pos.min(chars.len()));
            return;
        }
        let mut start = idx;
        while start > 0 && !is_word_separator(chars[start - 1]) {
            start -= 1;
        }
        let mut end = idx + 1;
        while end < chars.len() && !is_word_separator(chars[end]) {
            end += 1;
        }
        self.select_range(start, end);
    }

    pub fn handle_key(&mut self, key_text: &str, current_text: &str) -> Option<TextInputAction> {
        if !self.active {
            return None;
        }

        self.cursor_blink = Instant::now();

        if key_text == "\x1b" || key_text == "\r" || key_text == "\n" {
            self.active = false;
            self.selection = None;
            return Some(TextInputAction::Finished);
        }

        if key_text == "\x08" || key_text == "\x7f" {
            // Delete / Backspace with selection
            if let Some((start, end)) = self.selection_range() {
                self.push_undo_snapshot(current_text);
                self.selection = None;
                let byte_start = current_text
                    .char_indices()
                    .nth(start)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let byte_end = current_text
                    .char_indices()
                    .nth(end)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let mut new_text = current_text.to_string();
                new_text.replace_range(byte_start..byte_end, "");
                self.cursor_pos = start;
                return Some(TextInputAction::Changed(new_text));
            }

            // Standard backspace
            if key_text == "\x08" && self.cursor_pos > 0 {
                self.push_undo_snapshot(current_text);
                let byte_start = current_text
                    .char_indices()
                    .nth(self.cursor_pos - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let byte_end = current_text
                    .char_indices()
                    .nth(self.cursor_pos)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let mut new_text = current_text.to_string();
                new_text.replace_range(byte_start..byte_end, "");
                self.cursor_pos -= 1;
                return Some(TextInputAction::Changed(new_text));
            }
            // Standard delete
            if key_text == "\x7f" && self.cursor_pos < current_text.chars().count() {
                self.push_undo_snapshot(current_text);
                let byte_start = current_text
                    .char_indices()
                    .nth(self.cursor_pos)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let byte_end = current_text
                    .char_indices()
                    .nth(self.cursor_pos + 1)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let mut new_text = current_text.to_string();
                new_text.replace_range(byte_start..byte_end, "");
                return Some(TextInputAction::Changed(new_text));
            }
            return None;
        }

        if !key_text.is_empty() {
            // If has selection, replace it
            if let Some((start, end)) = self.selection_range() {
                self.push_undo_snapshot(current_text);
                self.selection = None;
                let byte_start = current_text
                    .char_indices()
                    .nth(start)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let byte_end = current_text
                    .char_indices()
                    .nth(end)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let mut new_text = current_text.to_string();
                new_text.replace_range(byte_start..byte_end, key_text);
                self.cursor_pos = start + key_text.chars().count();
                return Some(TextInputAction::Changed(new_text));
            }
            // Normal insert
            let byte_pos = current_text
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(current_text.len());
            self.push_undo_snapshot(current_text);
            let mut new_text = current_text.to_string();
            new_text.insert_str(byte_pos, key_text);
            self.cursor_pos += key_text.chars().count();
            return Some(TextInputAction::Changed(new_text));
        }

        None
    }

    pub fn move_left(&mut self) {
        if self.active {
            self.selection = None;
            if self.cursor_pos > 0 {
                self.cursor_pos -= 1;
                self.cursor_blink = Instant::now();
            }
        }
    }

    pub fn move_right(&mut self, text: &str) {
        if self.active {
            self.selection = None;
            if self.cursor_pos < text.chars().count() {
                self.cursor_pos += 1;
                self.cursor_blink = Instant::now();
            }
        }
    }

    /// Move left while extending selection
    pub fn move_left_shift(&mut self) {
        if self.active && self.cursor_pos > 0 {
            let anchor = self.selection.map(|(a, _)| a).unwrap_or(self.cursor_pos);
            self.cursor_pos -= 1;
            if anchor == self.cursor_pos {
                self.selection = None;
            } else {
                self.selection = Some((anchor, self.cursor_pos));
            }
            self.cursor_blink = Instant::now();
        }
    }

    /// Move right while extending selection
    pub fn move_right_shift(&mut self, text: &str) {
        if self.active && self.cursor_pos < text.chars().count() {
            let anchor = self.selection.map(|(a, _)| a).unwrap_or(self.cursor_pos);
            self.cursor_pos += 1;
            if anchor == self.cursor_pos {
                self.selection = None;
            } else {
                self.selection = Some((anchor, self.cursor_pos));
            }
            self.cursor_blink = Instant::now();
        }
    }

    /// Set cursor position from mouse click (clears selection)
    pub fn set_cursor_pos(&mut self, pos: usize) {
        self.cursor_pos = pos;
        self.cursor_blink = Instant::now();
        self.selection = None;
    }

    /// Start a new mouse selection
    pub fn start_selection(&mut self, pos: usize) {
        self.cursor_pos = pos;
        self.selection = Some((pos, pos));
        self.cursor_blink = Instant::now();
    }

    /// Update selection during mouse drag
    pub fn update_selection(&mut self, pos: usize) {
        if self.selection.is_none() {
            self.selection = Some((self.cursor_pos, self.cursor_pos));
        }
        if let Some((anchor, _)) = self.selection {
            self.cursor_pos = pos;
            if anchor == pos {
                self.selection = None;
            } else {
                self.selection = Some((anchor, pos));
            }
            self.cursor_blink = Instant::now();
        }
    }

    /// Select all text (Ctrl+A).
    pub fn select_all(&mut self, text: &str) {
        if self.active && !text.is_empty() {
            let count = text.chars().count();
            self.selection = Some((0, count));
            self.cursor_pos = count;
            self.cursor_blink = Instant::now();
        }
    }

    pub fn set_text_external(&mut self, text: &str) {
        self.cursor_pos = text.chars().count();
        self.cursor_blink = Instant::now();
        self.selection = None;
        self.undo_stack.clear();
    }

    pub fn undo(&mut self, current_text: &str) -> Option<String> {
        if !self.active {
            return None;
        }

        let snapshot = self.undo_stack.pop()?;
        let len = snapshot.text.chars().count();
        self.cursor_pos = snapshot.cursor_pos.min(len);
        self.selection = snapshot
            .selection
            .map(|(start, end)| (start.min(len), end.min(len)));
        self.cursor_blink = Instant::now();

        if snapshot.text == current_text {
            None
        } else {
            Some(snapshot.text)
        }
    }

    fn push_undo_snapshot(&mut self, current_text: &str) {
        if self.undo_stack.len() >= Self::MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(TextEditSnapshot {
            text: current_text.to_string(),
            cursor_pos: self.cursor_pos,
            selection: self.selection,
        });
    }
}

fn is_word_separator(ch: char) -> bool {
    ch.is_whitespace() || ch.is_ascii_punctuation()
}

pub fn cursor_pos_from_x(value: &str, rect: Rect, x: f32, metrics: TextInputMetrics) -> usize {
    let target = x - text_start_x(value, rect, metrics);
    let mut width = 0.0;
    for (index, ch) in value.chars().enumerate() {
        let next = width + char_advance(ch, metrics.font_size);
        if target < (width + next) * 0.5 {
            return index;
        }
        width = next;
    }
    value.chars().count()
}

pub fn cursor_x(value: &str, char_pos: usize, rect: Rect, metrics: TextInputMetrics) -> f32 {
    text_start_x(value, rect, metrics) + text_width_until(value, char_pos, metrics.font_size)
}

pub fn text_width_until(value: &str, char_pos: usize, font_size: f32) -> f32 {
    value
        .chars()
        .take(char_pos.min(value.chars().count()))
        .map(|ch| char_advance(ch, font_size))
        .sum()
}

pub fn text_width(value: &str, font_size: f32) -> f32 {
    value.chars().map(|ch| char_advance(ch, font_size)).sum()
}

pub fn render_selection_and_cursor(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    value: &str,
    input: &TextInputState,
    focused: bool,
    metrics: TextInputMetrics,
    selection_inset_y: f32,
    cursor_inset_y: f32,
    selection_color: [f32; 4],
    cursor_color: [f32; 4],
) {
    if !focused {
        return;
    }

    let content_left = rect.x + metrics.padding_x;
    let content_right = rect.x + rect.width - metrics.padding_x;

    if let Some((start, end)) = input.selection_range() {
        let x1 = cursor_x(value, start, rect, metrics);
        let x2 = cursor_x(value, end, rect, metrics);
        let left = x1.min(x2).clamp(content_left, content_right);
        let right = x1.max(x2).clamp(content_left, content_right);
        if right - left > 1.0 {
            quads.push(plain_quad(
                Rect {
                    x: left,
                    y: rect.y + selection_inset_y,
                    width: right - left,
                    height: rect.height - selection_inset_y * 2.0,
                },
                selection_color,
                2.0,
            ));
        }
    }

    if input.cursor_visible() {
        let x = cursor_x(value, input.cursor_pos, rect, metrics).clamp(content_left, content_right);
        quads.push(plain_quad(
            Rect {
                x,
                y: rect.y + cursor_inset_y,
                width: 1.5,
                height: rect.height - cursor_inset_y * 2.0,
            },
            cursor_color,
            0.0,
        ));
    }
}

fn text_start_x(value: &str, rect: Rect, metrics: TextInputMetrics) -> f32 {
    let content_left = rect.x + metrics.padding_x;
    let content_width = (rect.width - metrics.padding_x * 2.0).max(0.0);
    let width = text_width(value, metrics.font_size);
    match metrics.h_align {
        HAlign::Left => content_left,
        HAlign::Center => content_left + (content_width - width) * 0.5,
        HAlign::Right => content_left + content_width - width,
    }
}

fn char_advance(ch: char, font_size: f32) -> f32 {
    let ratio = match ch {
        'i' | 'l' | 'I' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' | '`' => 0.30,
        'j' | 'f' | 'r' | 't' | ' ' => 0.40,
        'm' | 'w' | 'M' | 'W' => 0.82,
        '0'..='9' => 0.55,
        '\u{0000}'..='\u{007f}' => 0.52,
        _ => 0.62,
    };
    font_size * ratio
}

fn plain_quad(rect: Rect, color: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 100.0,
            y: 0.0,
            width: 120.0,
            height: 24.0,
        }
    }

    fn assert_approx_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn cursor_x_uses_variable_character_widths() {
        let metrics = TextInputMetrics::left(10.0, 8.0);
        assert_approx_eq(cursor_x("iii", 3, rect(), metrics), 117.0);
        assert_approx_eq(cursor_x("www", 3, rect(), metrics), 132.6);
    }

    #[test]
    fn cursor_pos_from_x_matches_cursor_geometry() {
        let metrics = TextInputMetrics::left(10.0, 8.0);
        let value = "mi.w";
        for pos in 0..=value.chars().count() {
            let x = cursor_x(value, pos, rect(), metrics);
            assert_eq!(cursor_pos_from_x(value, rect(), x, metrics), pos);
        }
    }

    #[test]
    fn centered_cursor_uses_centered_text_start() {
        let metrics = TextInputMetrics::center(10.0, 8.0);
        let centered_start = 100.0 + 8.0 + ((120.0 - 16.0) - text_width("ii", 10.0)) * 0.5;
        assert_approx_eq(cursor_x("ii", 0, rect(), metrics), centered_start);
    }
}
