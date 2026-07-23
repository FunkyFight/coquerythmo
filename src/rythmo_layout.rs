//! Shared rythmo layout facade.

#[path = "rythmo_layout_legacy.rs"]
mod legacy;

pub use legacy::{
    active_karaoke_tracks, all_track_indices, build_track_layouts,
    build_track_layouts_at_frame, karaoke_mode_tracks, karaoke_stack_gap,
    karaoke_track_body_height, karaoke_tracks, leading_visual_bounds,
    line_or_badge_intersects_viewport, total_tracks_height, track_count,
    track_for_index, track_for_y_slot, track_has_active_karaoke,
    track_has_karaoke, track_index_for_y_slot, used_track_indices,
    y_slot_for_track_index, TrackLayout,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterLabelMetrics {
    pub font_size: f32,
    pub line_height: f32,
    pub width: f32,
    pub draw_height: f32,
    pub normal_gap: f32,
    pub centered_karaoke_gap: f32,
}

pub fn character_label_metrics(
    name: &str,
    row_height: f32,
    scale: f32,
    pixels_per_frame: f32,
) -> CharacterLabelMetrics {
    let row_height = row_height.max(1.0);
    let scale = scale.max(0.0);
    let font_size = row_height / 1.4;
    let line_height = (font_size * 1.4).ceil();
    let draw_height = line_height.min(row_height.ceil());
    let measured = crate::vector_text::measure_rythmo_text_width_standalone(name, font_size)
        .unwrap_or_else(|| name.chars().count().max(1) as f32 * font_size * 0.62);
    let horizontal_padding = 12.0 * scale;
    let italic_overhang_reserve = 12.0 * scale;
    let width = (measured * 1.25 + horizontal_padding + italic_overhang_reserve)
        .max(16.0 * scale);
    CharacterLabelMetrics {
        font_size,
        line_height,
        width,
        draw_height,
        normal_gap: 4.0 * pixels_per_frame,
        centered_karaoke_gap: crate::constants::BADGE_GAP * scale,
    }
}

pub fn scaled_character_badge_width(character_name: &str, scale: f32) -> f32 {
    let row_height = crate::constants::SLOT_HEIGHT * scale.max(0.0);
    let ppf = crate::constants::PIXELS_PER_FRAME
        * scale.max(0.0)
        * crate::config::scroll_speed();
    character_label_metrics(character_name, row_height, scale, ppf).width
}

/// Compatibility entry point for normal dialogue labels. The implementation is
/// intentionally semantic: normal labels always end four frames before the
/// line, and no longer depend on BADGE_GAP.
pub fn leading_character_badge_x(line_x: f32, badge_width: f32, scale: f32) -> f32 {
    let ppf = crate::constants::PIXELS_PER_FRAME
        * scale.max(0.0)
        * crate::config::scroll_speed();
    normal_character_badge_x(line_x, badge_width, ppf)
}

#[inline]
pub fn normal_character_label_right(line_x: f32, pixels_per_frame: f32) -> f32 {
    line_x - 4.0 * pixels_per_frame
}

#[inline]
pub fn normal_character_badge_x(
    line_x: f32,
    badge_width: f32,
    pixels_per_frame: f32,
) -> f32 {
    normal_character_label_right(line_x, pixels_per_frame) - badge_width
}

#[inline]
pub fn centered_karaoke_badge_x(
    karaoke_x: f32,
    badge_width: f32,
    scale: f32,
) -> f32 {
    karaoke_x - crate::constants::BADGE_GAP * scale.max(0.0) - badge_width
}

#[inline]
pub fn ambiance_badge_x(line_x: f32, badge_width: f32) -> f32 {
    line_x - badge_width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_gap_scales_with_pixels_per_frame() {
        for ppf in [3.0, 6.0, 12.0] {
            let line_x = 500.0;
            let badge_width = 100.0;
            let badge_x = normal_character_badge_x(line_x, badge_width, ppf);
            assert_eq!(line_x - (badge_x + badge_width), 4.0 * ppf);
        }
    }

    #[test]
    fn label_height_preserves_natural_metrics() {
        for name in ["Alex", "Twilight Sparkle", "Éléonore", "Jjjj ffff"] {
            let metrics = character_label_metrics(name, 40.0, 1.0, 6.0);
            assert!((metrics.line_height - 40.0).abs() <= 1.0);
            assert!((metrics.draw_height - (metrics.font_size * 1.4).ceil()).abs() <= 1.0);
        }
    }
}
