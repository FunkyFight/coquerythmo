//! Shared horizontal geometry for the rythmo UI and export renderers.

use super::scene::FrameWindow;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalRythmoGeometry {
    pub viewport_left: f32,
    pub viewport_width: f32,
    pub viewport_center_x: f32,
    pub timeline_origin_x: f32,
    pub playhead_left_x: f32,
    pub pixels_per_frame: f32,
}

impl HorizontalRythmoGeometry {
    pub fn new(
        viewport_left: f32,
        viewport_width: f32,
        playhead_width: f32,
        pixels_per_frame: f32,
    ) -> Self {
        Self::new_with_offset_pixels(
            viewport_left,
            viewport_width,
            playhead_width,
            pixels_per_frame,
            crate::config::playhead_delta_pixels(viewport_width),
        )
    }

    pub fn new_with_offset_percent(
        viewport_left: f32,
        viewport_width: f32,
        playhead_width: f32,
        pixels_per_frame: f32,
        offset_percent: f32,
    ) -> Self {
        let finite_percent = if offset_percent.is_finite() {
            offset_percent
        } else {
            0.0
        };
        Self::new_with_offset_pixels(
            viewport_left,
            viewport_width,
            playhead_width,
            pixels_per_frame,
            viewport_width.max(0.0) * finite_percent / 100.0,
        )
    }

    fn new_with_offset_pixels(
        viewport_left: f32,
        viewport_width: f32,
        playhead_width: f32,
        pixels_per_frame: f32,
        offset_pixels: f32,
    ) -> Self {
        let viewport_left = finite_or(viewport_left, 0.0);
        let viewport_width = finite_or(viewport_width, 0.0).max(0.0);
        let playhead_width = finite_or(playhead_width, 0.0).max(0.0);
        let pixels_per_frame = finite_or(pixels_per_frame, 0.001).max(0.001);
        let viewport_center_x = viewport_left + viewport_width * 0.5;
        let timeline_origin_x = viewport_center_x + finite_or(offset_pixels, 0.0);
        let playhead_left_x = timeline_origin_x - playhead_width * 0.5;

        Self {
            viewport_left,
            viewport_width,
            viewport_center_x,
            timeline_origin_x,
            playhead_left_x,
            pixels_per_frame,
        }
    }

    pub fn viewport_right(&self) -> f32 {
        self.viewport_left + self.viewport_width
    }

    pub fn frame_x(&self, frame: f64, current_frame: f64) -> f32 {
        let frame = finite_f64_or(frame, 0.0);
        let current_frame = finite_f64_or(current_frame, 0.0);
        self.timeline_origin_x + (frame - current_frame) as f32 * self.pixels_per_frame
    }

    pub fn centered_karaoke_x(&self, text_width: f32) -> f32 {
        self.viewport_center_x - finite_or(text_width, 0.0).max(0.0) * 0.5
    }

    pub fn visible_frame_window(
        &self,
        current_frame: f64,
        margin_frames: i64,
    ) -> FrameWindow {
        let current_frame = finite_f64_or(current_frame, 0.0);
        let frames_before =
            (self.timeline_origin_x - self.viewport_left) as f64 / self.pixels_per_frame as f64;
        let frames_after =
            (self.viewport_right() - self.timeline_origin_x) as f64 / self.pixels_per_frame as f64;
        let margin = margin_frames.max(0);
        let first = floor_to_i64(current_frame - frames_before).saturating_sub(margin);
        let last = ceil_to_i64(current_frame + frames_after).saturating_add(margin);

        FrameWindow {
            first,
            last: last.max(first),
        }
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn finite_f64_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn floor_to_i64(value: f64) -> i64 {
    if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.floor() as i64
    }
}

fn ceil_to_i64(value: f64) -> i64 {
    if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.ceil() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(offset_percent: f32) -> HorizontalRythmoGeometry {
        HorizontalRythmoGeometry::new_with_offset_percent(
            0.0,
            800.0,
            4.0,
            4.0,
            offset_percent,
        )
    }

    #[test]
    fn offset_positions_match_the_export_contract() {
        let centered = geometry(0.0);
        assert_eq!(centered.viewport_center_x, 400.0);
        assert_eq!(centered.timeline_origin_x, 400.0);

        let quarter_left = geometry(-25.0);
        assert_eq!(quarter_left.viewport_center_x, 400.0);
        assert_eq!(quarter_left.timeline_origin_x, 200.0);

        let fully_left = geometry(-50.0);
        assert_eq!(fully_left.viewport_center_x, 400.0);
        assert_eq!(fully_left.timeline_origin_x, 0.0);
    }

    #[test]
    fn current_frame_is_always_under_the_playhead() {
        for offset in [0.0, -12.5, -25.0, -50.0] {
            let geometry = geometry(offset);
            assert_eq!(geometry.frame_x(321.25, 321.25), geometry.timeline_origin_x);
        }
    }

    #[test]
    fn centered_karaoke_stays_at_the_physical_center() {
        for offset in [0.0, -12.5, -25.0, -50.0] {
            let geometry = geometry(offset);
            let width = 123.0;
            assert_eq!(
                geometry.centered_karaoke_x(width) + width * 0.5,
                geometry.viewport_center_x
            );
        }
    }

    #[test]
    fn shifted_playhead_produces_an_asymmetric_visible_window() {
        let geometry = geometry(-25.0);
        let window = geometry.visible_frame_window(1_000.0, 0);
        let before = 1_000 - window.first;
        let after = window.last - 1_000;
        assert!(after > before);
        assert_eq!(window.first, 950);
        assert_eq!(window.last, 1_150);
    }

    #[test]
    fn visible_window_includes_the_right_edge_and_margin() {
        let geometry = geometry(-25.0);
        let window = geometry.visible_frame_window(1_000.25, 4);
        assert_eq!(window.first, 946);
        assert_eq!(window.last, 1_155);
    }
}
