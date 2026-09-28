//! Bubble text layout shared by the studio preview and the video export.
//!
//! Text is laid out once in a reference space where the page is 1080 pixels
//! tall, then scaled to the preview or export size. Both outputs therefore
//! wrap the same words on the same lines.

use crate::comic_dubs::{Bubble, Point, TextAlignment};
use unicode_segmentation::UnicodeSegmentation;

/// Page height in which `Bubble::font_size` is expressed.
pub const REFERENCE_HEIGHT: f32 = 1_080.0;

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<String>,
    /// Font size in reference pixels.
    pub font_px: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
    /// Measured width of each line, in reference pixels.
    pub widths: Vec<f32>,
    /// Normalized text box `(x, y, width, height)`.
    pub bounds: [f32; 4],
    /// Reference width of the whole page.
    pub page_width: f32,
}

impl TextLayout {
    /// Normalized top-left corner of line `index`.
    pub fn line_origin(&self, index: usize, alignment: TextAlignment) -> Point {
        let box_width = self.bounds[2] * self.page_width;
        let width = self
            .widths
            .get(index)
            .copied()
            .unwrap_or(0.0)
            .min(box_width);
        let x = match alignment {
            TextAlignment::Left => 0.0,
            TextAlignment::Center => (box_width - width) * 0.5,
            TextAlignment::Right => box_width - width,
        };
        let block = self.line_height * self.lines.len() as f32;
        let top = (self.bounds[3] * REFERENCE_HEIGHT - block) * 0.5;
        Point {
            x: self.bounds[0] + x / self.page_width,
            y: self.bounds[1] + (top + index as f32 * self.line_height) / REFERENCE_HEIGHT,
        }
    }

    /// Normalized center of the whole text block.
    pub fn center(&self) -> Point {
        Point {
            x: self.bounds[0] + self.bounds[2] * 0.5,
            y: self.bounds[1] + self.bounds[3] * 0.5,
        }
    }
}

/// Lays out `text` (the bubble text by default) inside `points`.
pub fn layout(
    bubble: &Bubble,
    text: &str,
    points: &[Point],
    page_width: u32,
    page_height: u32,
    font_family: Option<&str>,
) -> TextLayout {
    let page_width = REFERENCE_HEIGHT * page_width.max(1) as f32 / page_height.max(1) as f32;
    let bounds = text_box(points);
    let padding = 6.0;
    let width = (bounds[2] * page_width - padding * 2.0).max(1.0);
    let bounds = [
        bounds[0] + padding / page_width,
        bounds[1],
        width / page_width,
        bounds[3],
    ];
    let height = (bounds[3] * REFERENCE_HEIGHT).max(1.0);
    let bold = bubble.bold;
    let (lines, font_px) = fit_text(
        text,
        width,
        height,
        bubble.font_size,
        bubble.letter_spacing,
        bubble.line_spacing,
        font_family,
        bold,
    );
    let widths = lines
        .iter()
        .map(|line| measure(line, font_px, bubble.letter_spacing, font_family, bold))
        .collect();
    TextLayout {
        lines,
        font_px,
        line_height: font_px * bubble.line_spacing,
        letter_spacing: bubble.letter_spacing,
        widths,
        bounds,
        page_width,
    }
}

/// Largest axis-aligned box (on a 6×6 grid) fully inside the polygon, so text
/// stays inside triangles and concave shapes.
pub fn text_box(points: &[Point]) -> [f32; 4] {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    let mut best: Option<([f32; 4], f32)> = None;
    // An 8-cell search keeps tails and scalloped outlines from shrinking the box
    // too much; layouts are cached by their callers.
    const GRID: usize = 8;
    for left in 0..GRID {
        for right in left + 1..=GRID {
            for top in 0..GRID {
                for bottom in top + 1..=GRID {
                    let x1 = min_x + (max_x - min_x) * left as f32 / GRID as f32;
                    let x2 = min_x + (max_x - min_x) * right as f32 / GRID as f32;
                    let y1 = min_y + (max_y - min_y) * top as f32 / GRID as f32;
                    let y2 = min_y + (max_y - min_y) * bottom as f32 / GRID as f32;
                    let inset_x = (x2 - x1) * 0.06;
                    let inset_y = (y2 - y1) * 0.06;
                    let candidate = [x1 + inset_x, y1 + inset_y, x2 - inset_x, y2 - inset_y];
                    let inside = [
                        (candidate[0], candidate[1]),
                        (candidate[2], candidate[1]),
                        (candidate[0], candidate[3]),
                        (candidate[2], candidate[3]),
                        (
                            (candidate[0] + candidate[2]) * 0.5,
                            (candidate[1] + candidate[3]) * 0.5,
                        ),
                    ]
                    .into_iter()
                    .all(|(x, y)| point_in_polygon(Point { x, y }, points));
                    let area = (candidate[2] - candidate[0]) * (candidate[3] - candidate[1]);
                    if inside && best.is_none_or(|(_, best_area)| area > best_area) {
                        best = Some((candidate, area));
                    }
                }
            }
        }
    }
    match best {
        Some(([x1, y1, x2, y2], _)) => [x1, y1, x2 - x1, y2 - y1],
        None => [min_x, min_y, max_x - min_x, max_y - min_y],
    }
}

pub fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let mut previous = polygon.last().copied().unwrap_or(point);
    for current in polygon {
        if (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}

/// Picks the largest font (at most `preferred_font_size`) whose greedy,
/// width-measured wrapping fits the box. Word widths are measured once and
/// scaled, so the search stays cheap.
#[allow(clippy::too_many_arguments)]
pub fn fit_text(
    text: &str,
    width: f32,
    height: f32,
    preferred_font_size: f32,
    letter_spacing: f32,
    line_spacing: f32,
    font_family: Option<&str>,
    bold: bool,
) -> (Vec<String>, f32) {
    const BASE: f32 = 100.0;
    let tokens = words(text);
    let token_widths = tokens
        .iter()
        .map(|token| measure(token, BASE, 0.0, font_family, bold))
        .collect::<Vec<_>>();
    let space = (measure("a a", BASE, 0.0, font_family, bold)
        - measure("aa", BASE, 0.0, font_family, bold))
    .max(BASE * 0.2);
    let maximum = preferred_font_size.clamp(6.0, 72.0).floor() as u32;
    for font in (6..=maximum).rev().map(|size| size as f32) {
        let scale = font / BASE;
        let token_width = |index: usize| {
            token_widths[index] * scale
                + letter_spacing * tokens[index].chars().count().saturating_sub(1) as f32
        };
        if (0..tokens.len()).any(|index| token_width(index) > width) {
            continue;
        }
        let mut lines: Vec<String> = Vec::new();
        let mut line_width = 0.0;
        for (index, token) in tokens.iter().enumerate() {
            let advance = token_width(index);
            match lines.last_mut() {
                Some(line) if line_width + (space * scale + letter_spacing) + advance <= width => {
                    line.push(' ');
                    line.push_str(token);
                    line_width += space * scale + letter_spacing + advance;
                }
                _ => {
                    lines.push(token.clone());
                    line_width = advance;
                }
            }
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        if lines.len() as f32 * font * line_spacing <= height
            && lines
                .iter()
                .all(|line| measure(line, font, letter_spacing, font_family, bold) <= width + 0.5)
        {
            return (lines, font);
        }
    }

    // Last resort: cut long words at the smallest size and ellipsize.
    let font = 6.0;
    let max_chars = (width / (font * 0.56 + letter_spacing)).floor().max(1.0) as usize;
    let max_lines = (height / (font * line_spacing)).floor().max(1.0) as usize;
    let mut lines = wrap_text(text, max_chars);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while last.chars().count() >= max_chars {
                last.pop();
            }
            last.push('…');
        }
    }
    (lines, font)
}

pub fn measure(
    line: &str,
    font_size: f32,
    letter_spacing: f32,
    font_family: Option<&str>,
    bold: bool,
) -> f32 {
    let width =
        crate::vector_text::measure_text_width_with_family_standalone(line, font_size, font_family)
            .unwrap_or(0.0);
    let width = if bold { width * 1.06 } else { width };
    width + letter_spacing * line.graphemes(true).count().saturating_sub(1) as f32
}

/// Splits on whitespace but keeps French high punctuation (« ! », « ? »,
/// « : », « ; », « » ») attached to the previous word so it never starts a line.
fn words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let punctuation = word
            .chars()
            .all(|character| matches!(character, '!' | '?' | ':' | ';' | '»' | '…' | '.'));
        match words.last_mut() {
            Some(last) if punctuation => {
                last.push('\u{a0}');
                last.push_str(word);
            }
            _ => words.push(word.to_string()),
        }
    }
    words
}

pub fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words(text) {
        let characters: Vec<_> = word.chars().collect();
        if characters.len() > max_chars {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for chunk in characters.chunks(max_chars) {
                let chunk: String = chunk.iter().collect();
                if chunk.chars().count() == max_chars {
                    lines.push(chunk);
                } else {
                    current = chunk;
                }
            }
            continue;
        }
        if !current.is_empty() && current.chars().count() + 1 + characters.len() > max_chars {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineReveal {
    Full,
    /// Only this prefix of the line is visible, drawn in place.
    Partial(String),
    Hidden,
}

/// Splits a typewriter reveal (graphemes of the trimmed text) across the
/// wrapped lines. Whitespace is ignored so wrapping never shifts the count.
pub fn line_reveals(lines: &[String], text: &str, reveal: Option<usize>) -> Vec<LineReveal> {
    let Some(count) = reveal else {
        return vec![LineReveal::Full; lines.len()];
    };
    let visible = crate::comic_dubs_timeline::revealed_prefix(text.trim(), Some(count))
        .graphemes(true)
        .filter(|grapheme| !grapheme.chars().all(char::is_whitespace))
        .count();
    let mut remaining = visible;
    lines
        .iter()
        .map(|line| {
            let ink = line
                .graphemes(true)
                .filter(|grapheme| !grapheme.chars().all(char::is_whitespace))
                .count();
            if remaining >= ink {
                remaining -= ink;
                return if ink == 0 && visible == 0 {
                    LineReveal::Hidden
                } else {
                    LineReveal::Full
                };
            }
            if remaining == 0 {
                return LineReveal::Hidden;
            }
            let mut seen = 0;
            let mut prefix = String::new();
            for grapheme in line.graphemes(true) {
                if !grapheme.chars().all(char::is_whitespace) {
                    if seen == remaining {
                        break;
                    }
                    seen += 1;
                }
                prefix.push_str(grapheme);
            }
            remaining = 0;
            LineReveal::Partial(prefix.trim_end().to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_box_stays_inside_a_triangle() {
        let triangle = [
            Point { x: 0.5, y: 0.1 },
            Point { x: 0.9, y: 0.9 },
            Point { x: 0.1, y: 0.9 },
        ];
        let [x, y, width, height] = text_box(&triangle);
        for (px, py) in [
            (x, y),
            (x + width, y),
            (x, y + height),
            (x + width, y + height),
        ] {
            assert!(point_in_polygon(Point { x: px, y: py }, &triangle));
        }
    }

    #[test]
    fn fitted_lines_use_the_real_font_width() {
        let (lines, font) = fit_text("WWWW WWWW", 120.0, 80.0, 34.0, 2.0, 1.2, None, false);
        assert!(lines
            .iter()
            .all(|line| measure(line, font, 2.0, None, false) <= 120.0));
        let text = "Une traduction assez longue doit rester lisible dans la bulle";
        let (lines, font) = fit_text(text, 300.0, 200.0, 34.0, 0.0, 1.18, None, false);
        assert_eq!(lines.join(" "), text);
        assert!(lines.len() as f32 * font * 1.18 <= 200.0);
    }

    #[test]
    fn french_punctuation_stays_with_its_word() {
        assert_eq!(wrap_text("Viens ici !", 6), vec!["Viens", "ici\u{a0}!"]);
    }

    #[test]
    fn typewriter_reveal_spreads_across_wrapped_lines() {
        let lines = vec!["Salut".to_string(), "à toi".to_string()];
        assert_eq!(
            line_reveals(&lines, "Salut à toi", Some(7)),
            vec![LineReveal::Full, LineReveal::Partial("à".into())]
        );
        assert_eq!(
            line_reveals(&lines, "Salut à toi", Some(0)),
            vec![LineReveal::Hidden, LineReveal::Hidden]
        );
        assert_eq!(
            line_reveals(&lines, "Salut à toi", None),
            vec![LineReveal::Full, LineReveal::Full]
        );
    }
}
