use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

const TOOLTIP_PADDING_H: f32 = 12.0;
const TOOLTIP_PADDING_V: f32 = 6.0;
const TOOLTIP_OFFSET_Y: f32 = 20.0;
const TOOLTIP_RADIUS: f32 = 4.0;
const LINT_TOOLTIP_MAX_WIDTH: f32 = 460.0;
const LINT_TOOLTIP_LINE_GAP: f32 = 3.0;

pub struct TooltipState {
    pub text: String,
    pub cursor_x: f32,
    pub cursor_y: f32,
}

pub struct LintTooltipEntry {
    pub severity: crate::lint::Severity,
    pub lines: Vec<String>,
}

pub struct LintTooltipState {
    pub entries: Vec<LintTooltipEntry>,
    pub cursor_x: f32,
    pub cursor_y: f32,
}

impl LintTooltipState {
    pub fn new(diagnostics: &[crate::lint::Diagnostic], cursor_x: f32, cursor_y: f32) -> Self {
        let font_size = crate::config::get().ui.font_size;
        let entries = diagnostics.iter().map(|diagnostic| LintTooltipEntry {
            severity: diagnostic.severity,
            lines: wrap_message(diagnostic.message, font_size, diagnostic.severity),
        }).collect();
        Self { entries, cursor_x, cursor_y }
    }

    fn rect(&self, screen_w: f32, font_size: f32) -> Rect {
        let char_width = font_size * 0.56;
        let longest = self.entries.iter().flat_map(|entry| entry.lines.iter().enumerate().map(move |(index, line)| {
            line.chars().count() + if index == 0 { if entry.severity == crate::lint::Severity::Error { 16 } else { 17 } } else { 0 }
        })).max().unwrap_or(20) as f32;
        let width = (longest * char_width + TOOLTIP_PADDING_H * 2.0)
            .clamp(220.0, LINT_TOOLTIP_MAX_WIDTH.min(screen_w - 8.0));
        let line_count = self.entries.iter().map(|entry| entry.lines.len()).sum::<usize>() + self.entries.len().saturating_sub(1);
        let height = line_count.max(1) as f32 * (font_size + LINT_TOOLTIP_LINE_GAP)
            + TOOLTIP_PADDING_V * 2.0;
        let x = (self.cursor_x - width / 2.0).clamp(4.0, screen_w - width - 4.0);
        Rect { x, y: self.cursor_y + TOOLTIP_OFFSET_Y, width, height }
    }

    pub fn render_quads(&self, screen_w: f32) -> Vec<QuadInstance> {
        let r = self.rect(screen_w, crate::config::get().ui.font_size);
        vec![QuadInstance {
            rect: [r.x, r.y, r.width, r.height], color: [0.18, 0.18, 0.20, 0.97],
            color_bottom: [0.14, 0.14, 0.16, 0.97], border_color: [0.30, 0.30, 0.36, 0.7],
            border_width: 1.0, border_radius: TOOLTIP_RADIUS, shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.4], shadow_blur: 6.0, rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    pub fn render_labels(&self, screen_w: f32) -> Vec<LabelInfo<'_>> {
        let font_size = crate::config::get().ui.font_size;
        let rect = self.rect(screen_w, font_size);
        let mut labels = Vec::new();
        let mut entry_line = 0usize;
        for (index, entry) in self.entries.iter().enumerate() {
            for (line_index, line) in entry.lines.iter().enumerate() {
                let y = rect.y + TOOLTIP_PADDING_V + entry_line as f32 * (font_size + LINT_TOOLTIP_LINE_GAP);
                if line_index == 0 {
                    let prefix = match entry.severity {
                        crate::lint::Severity::Warning => "Avertissement :",
                        crate::lint::Severity::Error => "Non conforme :",
                    };
                    let prefix_width = prefix.chars().count() as f32 * font_size * 0.56 + 7.0;
                    labels.push(LabelInfo { text: prefix, bounds: Rect { x: rect.x + TOOLTIP_PADDING_H, y, width: prefix_width, height: font_size }, h_align: HAlign::Left, v_align: VAlign::Center, overflow: Overflow::Visible, padding: 0.0, font_size_override: None, color_override: Some(if entry.severity == crate::lint::Severity::Error { [255, 92, 92] } else { [255, 196, 54] }), font_family_override: None });
                    labels.push(LabelInfo { text: line, bounds: Rect { x: rect.x + TOOLTIP_PADDING_H + prefix_width, y, width: rect.width - TOOLTIP_PADDING_H * 2.0 - prefix_width, height: font_size }, h_align: HAlign::Left, v_align: VAlign::Center, overflow: Overflow::Visible, padding: 0.0, font_size_override: None, color_override: None, font_family_override: None });
                } else {
                    labels.push(LabelInfo { text: line, bounds: Rect { x: rect.x + TOOLTIP_PADDING_H, y, width: rect.width - TOOLTIP_PADDING_H * 2.0, height: font_size }, h_align: HAlign::Left, v_align: VAlign::Center, overflow: Overflow::Visible, padding: 0.0, font_size_override: None, color_override: None, font_family_override: None });
                }
                entry_line += 1;
            }
            if index + 1 < self.entries.len() { entry_line += 1; }
        }
        labels
    }
}

fn wrap_message(message: &str, font_size: f32, severity: crate::lint::Severity) -> Vec<String> {
    let max_chars = ((LINT_TOOLTIP_MAX_WIDTH - TOOLTIP_PADDING_H * 2.0) / (font_size * 0.56)).floor() as usize;
    let prefix_len = if severity == crate::lint::Severity::Error { 16 } else { 17 };
    let mut result = Vec::new();
    let mut words = message.split_whitespace().peekable();
    let mut first = true;
    while words.peek().is_some() {
        let capacity = if first { max_chars.saturating_sub(prefix_len).max(12) } else { max_chars };
        let mut line = String::new();
        while let Some(word) = words.peek().copied() {
            let needed = line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if !line.is_empty() && needed > capacity { break; }
            if !line.is_empty() { line.push(' '); }
            line.push_str(word); words.next();
        }
        result.push(line); first = false;
    }
    result
}

impl TooltipState {
    fn rect(&self, screen_w: f32, font_size: f32) -> Rect {
        let char_count = self.text.chars().count() as f32;
        // Approximate char width as ~60% of font size for sans-serif
        let char_w = font_size * 0.6;
        let text_w = char_count * char_w;
        let w = (text_w + TOOLTIP_PADDING_H * 2.0).clamp(40.0, screen_w - 8.0);
        let h = font_size + TOOLTIP_PADDING_V * 2.0;
        let x = (self.cursor_x - w / 2.0).clamp(4.0, screen_w - w - 4.0);
        let y = self.cursor_y + TOOLTIP_OFFSET_Y;
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn render_quads(&self, screen_w: f32) -> Vec<QuadInstance> {
        let font_size = crate::config::get().ui.font_size;
        let r = self.rect(screen_w, font_size);
        vec![QuadInstance {
            rect: [r.x, r.y, r.width, r.height],
            color: [0.18, 0.18, 0.20, 0.95],
            color_bottom: [0.14, 0.14, 0.16, 0.95],
            border_color: [0.30, 0.30, 0.36, 0.6],
            border_width: 1.0,
            border_radius: TOOLTIP_RADIUS,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.4],
            shadow_blur: 6.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    pub fn render_labels(&self, screen_w: f32) -> Vec<LabelInfo<'_>> {
        let font_size = crate::config::get().ui.font_size;
        let r = self.rect(screen_w, font_size);
        vec![LabelInfo {
            text: &self.text,
            bounds: r,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Visible,
            padding: TOOLTIP_PADDING_H,
            font_size_override: None,
            color_override: None,
            font_family_override: None,
        }]
    }
}
