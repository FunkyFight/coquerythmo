include!("ui/mod.rs");

impl Ui {
    pub fn text_emotion_screen_size(&self) -> (f32, f32) {
        (self.screen_w, self.screen_h)
    }

    pub fn open_text_emotion_accessibility_scope(&mut self) {
        let nodes = crate::text_emotion_foreground::accessible_nodes();
        if nodes.is_empty() {
            return;
        }
        if self.focus.active_scope_id() == Some("text-emotions") {
            let _ = self.focus.pop_scope();
        }
        self.focus.push_scope("text-emotions", nodes);
    }

    pub fn refresh_text_emotion_accessibility_scope(&mut self) {
        if self.focus.active_scope_id() != Some("text-emotions") {
            return;
        }
        let _ = self.focus.pop_scope();
        let nodes = crate::text_emotion_foreground::accessible_nodes();
        if !nodes.is_empty() {
            self.focus.push_scope("text-emotions", nodes);
        }
    }

    pub fn close_text_emotion_accessibility_scope(&mut self) {
        if self.focus.active_scope_id() == Some("text-emotions") {
            let _ = self.focus.pop_scope();
        }
    }
}
