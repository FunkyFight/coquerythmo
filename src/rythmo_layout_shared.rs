//! Shared-label facade for the historical vertical rythmo layout module.

#[path = "rythmo_layout.rs"]
mod implementation;

#[allow(unused_imports)]
pub use implementation::*;

/// Compatibility entry point using the shared final-size measurement. New code
/// should prefer `character_label_metrics` and keep its collision/text rects.
pub fn scaled_character_badge_width(character_name: &str, scale: f32) -> f32 {
    let row_height = crate::constants::SLOT_HEIGHT * scale.max(0.0);
    let ppf = crate::constants::PIXELS_PER_FRAME
        * scale.max(0.0)
        * crate::config::scroll_speed();
    crate::rendering::rythmo::labels::character_label_metrics(
        character_name,
        row_height,
        scale,
        ppf,
    )
    .width
}

pub fn normal_character_label_right(line_x: f32, pixels_per_frame: f32) -> f32 {
    crate::rendering::rythmo::labels::normal_character_label_right(line_x, pixels_per_frame)
}

pub fn normal_character_label_x(
    line_x: f32,
    badge_width: f32,
    pixels_per_frame: f32,
) -> f32 {
    crate::rendering::rythmo::labels::normal_character_label_x(
        line_x,
        badge_width,
        pixels_per_frame,
    )
}

pub fn centered_karaoke_character_label_x(
    karaoke_rect: crate::ui::primitives::Rect,
    badge_width: f32,
    scale: f32,
) -> f32 {
    crate::rendering::rythmo::labels::centered_karaoke_character_label_x(
        karaoke_rect,
        badge_width,
        scale,
    )
}

pub fn ambiance_character_label_x(line_x: f32, badge_width: f32) -> f32 {
    crate::rendering::rythmo::labels::ambiance_character_label_x(line_x, badge_width)
}

/// Kept only for source compatibility with code outside the render paths. Its
/// old `BADGE_GAP` semantics are deliberately gone: the value now follows the
/// canonical four-frame dialogue rule.
#[deprecated(note = "use normal_character_label_x or a semantic karaoke/ambiance helper")]
pub fn leading_character_badge_x(line_x: f32, badge_width: f32, scale: f32) -> f32 {
    let ppf = crate::constants::PIXELS_PER_FRAME
        * scale.max(0.0)
        * crate::config::scroll_speed();
    normal_character_label_x(line_x, badge_width, ppf)
}

#[cfg(test)]
mod shared_label_tests {
    use super::*;

    #[test]
    fn compatibility_helper_no_longer_uses_badge_gap() {
        crate::config::init();
        let scale = 1.0;
        let ppf = crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed();
        #[allow(deprecated)]
        let x = leading_character_badge_x(200.0, 50.0, scale);
        assert_eq!(x, 200.0 - 4.0 * ppf - 50.0);
    }
}
