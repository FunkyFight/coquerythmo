use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use std::time::Instant;

const TOAST_W: f32 = 520.0;
const TOAST_PAD_Y: f32 = 10.0;
const TOAST_PAD_X: f32 = 16.0;
const TOAST_MARGIN: f32 = 12.0;
const TOAST_GAP: f32 = 6.0;
const LINE_HEIGHT: f32 = 16.0;
const FONT_SIZE: f32 = 12.0;
const CHAR_WIDTH: f32 = 6.5;

pub struct Toast {
    pub message: String,
    pub lines: Vec<String>,
    pub duration_secs: f32,
    pub created: Instant,
}

impl Toast {
    fn new(message: impl Into<String>, duration_secs: f32) -> Self {
        let message = message.into();
        let lines = word_wrap(&message, TOAST_W - TOAST_PAD_X * 2.0);
        Self {
            message,
            lines,
            duration_secs,
            created: Instant::now(),
        }
    }

    fn alpha(&self) -> f32 {
        let elapsed = self.created.elapsed().as_secs_f32();
        let fade_in = 0.3;
        let fade_out = 0.5;
        if elapsed < fade_in {
            elapsed / fade_in
        } else if elapsed < self.duration_secs - fade_out {
            1.0
        } else if elapsed < self.duration_secs {
            (self.duration_secs - elapsed) / fade_out
        } else {
            0.0
        }
    }

    fn expired(&self) -> bool {
        self.created.elapsed().as_secs_f32() >= self.duration_secs
    }

    fn height(&self) -> f32 {
        self.lines.len().max(1) as f32 * LINE_HEIGHT + TOAST_PAD_Y * 2.0
    }
}

pub struct ToastManager {
    toasts: Vec<Toast>,
    hovered: Option<usize>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            hovered: None,
        }
    }

    pub fn push(&mut self, message: impl Into<String>, duration_secs: f32) {
        self.toasts.push(Toast::new(message, duration_secs));
    }

    pub fn tick(&mut self) {
        self.toasts.retain(|t| !t.expired());
    }

    pub fn has_active(&self) -> bool {
        !self.toasts.is_empty()
    }

    pub fn handle_event(&mut self, event: &UiEvent, screen_w: f32, screen_h: f32) -> bool {
        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if let Some(i) = self.hit_test(*x, *y, screen_w, screen_h) {
                    self.toasts.remove(i);
                    self.hovered = None;
                    return true;
                }
            }
            UiEvent::MouseMove { x, y } => {
                let new_hover = self.hit_test(*x, *y, screen_w, screen_h);
                if new_hover != self.hovered {
                    self.hovered = new_hover;
                }
            }
            _ => {}
        }
        false
    }

    fn hit_test(&self, x: f32, y: f32, screen_w: f32, screen_h: f32) -> Option<usize> {
        let mut ty = screen_h - TOAST_MARGIN;
        for i in (0..self.toasts.len()).rev() {
            let h = self.toasts[i].height();
            ty -= h;
            let tx = (screen_w - TOAST_W) / 2.0;
            if (Rect {
                x: tx,
                y: ty,
                width: TOAST_W,
                height: h,
            })
            .contains(x, y)
            {
                return Some(i);
            }
            ty -= TOAST_GAP;
        }
        None
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let mut y = screen_h - TOAST_MARGIN;
        for (idx, toast) in self.toasts.iter().enumerate().rev() {
            let a = toast.alpha();
            if a <= 0.0 {
                continue;
            }

            let h = toast.height();
            y -= h;
            let x = (screen_w - TOAST_W) / 2.0;
            let is_hovered = self.hovered == Some(idx);

            // Background
            let (bg_top, bg_bot, border) = if is_hovered {
                (
                    [0.20, 0.20, 0.26, 0.95 * a],
                    [0.16, 0.16, 0.22, 0.95 * a],
                    [0.55, 0.50, 0.75, 0.8 * a],
                )
            } else {
                (
                    [0.14, 0.14, 0.18, 0.92 * a],
                    [0.10, 0.10, 0.14, 0.92 * a],
                    [0.40, 0.38, 0.55, 0.6 * a],
                )
            };
            overlay_quads.push(QuadInstance {
                rect: [x, y, TOAST_W, h],
                color: bg_top,
                color_bottom: bg_bot,
                border_color: border,
                border_width: 1.0,
                border_radius: 8.0,
                shadow_offset: [0.0, 2.0],
                shadow_color: [0.0, 0.0, 0.0, 0.4 * a],
                shadow_blur: 8.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            // Text lines
            let text_x = x + TOAST_PAD_X;
            let text_w = TOAST_W - TOAST_PAD_X * 2.0;
            let mut ly = y + TOAST_PAD_Y;
            for line in &toast.lines {
                labels.push(LabelInfo {
                    text: line,
                    bounds: Rect {
                        x: text_x,
                        y: ly,
                        width: text_w,
                        height: LINE_HEIGHT,
                    },
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(FONT_SIZE),
                    color_override: None,
                    font_family_override: None,
                });
                ly += LINE_HEIGHT;
            }

            y -= TOAST_GAP;
        }
    }
}

/// Word-wrap text to fit within `max_width` pixels (approximate).
fn word_wrap(text: &str, max_width: f32) -> Vec<String> {
    let max_chars = (max_width / CHAR_WIDTH).floor() as usize;
    if max_chars == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_chars {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
