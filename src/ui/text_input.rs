use std::time::Instant;

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
