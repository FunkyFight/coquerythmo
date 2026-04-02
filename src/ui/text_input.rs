use std::time::Instant;

pub enum TextInputAction {
    Changed(String),
    Finished,
}

pub struct TextInputState {
    pub active: bool,
    pub cursor_pos: usize,
    cursor_blink: Instant,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            active: false,
            cursor_pos: 0,
            cursor_blink: Instant::now(),
        }
    }

    pub fn activate(&mut self, text: &str) {
        self.active = true;
        self.cursor_pos = text.chars().count();
        self.cursor_blink = Instant::now();
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn cursor_visible(&self) -> bool {
        self.active && self.cursor_blink.elapsed().as_millis() % 1000 < 500
    }

    pub fn handle_key(&mut self, key_text: &str, current_text: &str) -> Option<TextInputAction> {
        if !self.active {
            return None;
        }

        self.cursor_blink = Instant::now();

        if key_text == "\x1b" || key_text == "\r" || key_text == "\n" {
            self.active = false;
            return Some(TextInputAction::Finished);
        }

        if key_text == "\x08" {
            // Backspace
            if self.cursor_pos > 0 {
                let byte_start = current_text.char_indices()
                    .nth(self.cursor_pos - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let byte_end = current_text.char_indices()
                    .nth(self.cursor_pos)
                    .map(|(i, _)| i)
                    .unwrap_or(current_text.len());
                let mut new_text = current_text.to_string();
                new_text.replace_range(byte_start..byte_end, "");
                self.cursor_pos -= 1;
                return Some(TextInputAction::Changed(new_text));
            }
            return None;
        }

        if !key_text.is_empty() {
            let byte_pos = current_text.char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(current_text.len());
            let mut new_text = current_text.to_string();
            new_text.insert_str(byte_pos, key_text);
            self.cursor_pos += key_text.chars().count();
            return Some(TextInputAction::Changed(new_text));
        }

        None
    }

    pub fn move_left(&mut self) {
        if self.active && self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.cursor_blink = Instant::now();
        }
    }

    pub fn move_right(&mut self, text: &str) {
        if self.active && self.cursor_pos < text.chars().count() {
            self.cursor_pos += 1;
            self.cursor_blink = Instant::now();
        }
    }

    /// Set text externally (e.g. autocomplete). Cursor goes to end.
    pub fn set_text_external(&mut self, text: &str) {
        self.cursor_pos = text.chars().count();
        self.cursor_blink = Instant::now();
    }
}
