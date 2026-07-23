//! Shared character-label measurements and semantic placement rules.

use crate::ui::primitives::Rect;

const NATURAL_LINE_HEIGHT_FACTOR: f32 = 1.4;
const WIDTH_EMPHASIS_FACTOR: f32 = 1.25;
const BASE_HORIZONTAL_PADDING: f32 = 6.0;
const BASE_ITALIC_OVERHANG_RESERVE: f32 = 12.0;
const BASE_MIN_WIDTH: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterLabelMetrics {
    pub font_size: f32,
    pub line_height: f32,
    pub width: f32,
    pub draw_height: f32,
    pub normal_gap: f32,
    pub centered_karaoke_gap: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterLabelRects {
    pub collision_rect: Rect,
    pub text_draw_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedLineGeometry {
    pub line_rect: Rect,
    pub badge_collision_rect: Option<Rect>,
    pub badge_text_rect: Option<Rect>,
    pub badge_font_size: f32,
    pub badge_scale: f32,
    pub badge_gap: f32,
}

pub fn character_label_metrics(
    name: &str,
    row_height: f32,
    scale: f32,
    pixels_per_frame: f32,
) -> CharacterLabelMetrics {
    let row_height = finite_non_negative(row_height);
    let scale = finite_non_negative(scale);
    let pixels_per_frame = finite_non_negative(pixels_per_frame);
    let font_size = (row_height / NATURAL_LINE_HEIGHT_FACTOR).max(1.0);
    let line_height = (font_size * NATURAL_LINE_HEIGHT_FACTOR).ceil();
    let draw_height = line_height.min(row_height.ceil().max(1.0));
    let measured = crate::vector_text::measure_rythmo_text_width_standalone(name, font_size)
        .unwrap_or_else(|| name.chars().count().max(1) as f32 * font_size * 0.62);
    let horizontal_padding = BASE_HORIZONTAL_PADDING * scale;
    let italic_overhang_reserve = BASE_ITALIC_OVERHANG_RESERVE * scale;
    let width = (measured * WIDTH_EMPHASIS_FACTOR
        + horizontal_padding * 2.0
        + italic_overhang_reserve)
        .max(BASE_MIN_WIDTH * scale.max(0.5));

    CharacterLabelMetrics {
        font_size,
        line_height,
        width,
        draw_height,
        normal_gap: 4.0 * pixels_per_frame,
        centered_karaoke_gap: crate::constants::BADGE_GAP * scale,
    }
}

pub fn normal_character_label_right(line_x: f32, pixels_per_frame: f32) -> f32 {
    line_x - 4.0 * finite_non_negative(pixels_per_frame)
}

pub fn normal_character_label_x(
    line_x: f32,
    badge_width: f32,
    pixels_per_frame: f32,
) -> f32 {
    normal_character_label_right(line_x, pixels_per_frame) - badge_width.max(0.0)
}

pub fn centered_karaoke_character_label_x(
    karaoke_rect: Rect,
    badge_width: f32,
    scale: f32,
) -> f32 {
    karaoke_rect.x
        - crate::constants::BADGE_GAP * finite_non_negative(scale)
        - badge_width.max(0.0)
}

pub fn ambiance_character_label_x(line_x: f32, badge_width: f32) -> f32 {
    line_x - badge_width.max(0.0)
}

pub fn character_label_rects(
    x: f32,
    row_y: f32,
    row_height: f32,
    metrics: CharacterLabelMetrics,
) -> CharacterLabelRects {
    let collision_rect = Rect {
        x,
        y: row_y,
        width: metrics.width,
        height: row_height.max(0.0),
    };
    let text_draw_rect = Rect {
        x,
        y: row_y,
        width: metrics.width,
        height: metrics.draw_height,
    };
    CharacterLabelRects {
        collision_rect,
        text_draw_rect,
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_gap_is_always_four_frames() {
        for (ppf, expected) in [(3.0, 12.0), (6.0, 24.0), (12.0, 48.0)] {
            assert_eq!(normal_character_label_right(100.0, ppf), 100.0 - expected);
            let metrics = character_label_metrics("Alex", 40.0, 1.0, ppf);
            assert_eq!(metrics.normal_gap, expected);
        }
    }

    #[test]
    fn natural_text_height_matches_the_row_without_vertical_stretch() {
        for name in ["Alex", "Twilight Sparkle", "Éléonore", "Jjjj ffff"] {
            let metrics = character_label_metrics(name, 40.0, 1.0, 6.0);
            assert!((metrics.line_height - 40.0).abs() <= 1.0);
            assert!((metrics.draw_height - metrics.line_height).abs() <= 1.0);
            assert!(metrics.width > 0.0);
        }
    }

    #[test]
    fn collision_and_text_rects_do_not_force_the_text_to_fill_the_row() {
        let metrics = character_label_metrics("Alex", 41.0, 1.0, 6.0);
        let rects = character_label_rects(10.0, 20.0, 80.0, metrics);
        assert_eq!(rects.collision_rect.height, 80.0);
        assert_eq!(rects.text_draw_rect.height, metrics.draw_height);
        assert!(rects.text_draw_rect.height < rects.collision_rect.height);
    }

    #[test]
    fn semantic_placement_rules_are_distinct() {
        let karaoke = Rect {
            x: 300.0,
            y: 0.0,
            width: 200.0,
            height: 40.0,
        };
        assert_eq!(normal_character_label_x(300.0, 100.0, 6.0), 176.0);
        assert_eq!(centered_karaoke_character_label_x(karaoke, 100.0, 1.0), 198.0);
        assert_eq!(ambiance_character_label_x(300.0, 100.0), 200.0);
    }
}
