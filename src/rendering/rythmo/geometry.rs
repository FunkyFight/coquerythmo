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
        let viewport_width = viewport_width.max(0.0);
        let pixels_per_frame = pixels_per_frame.max(f32::EPSILON);
        let viewport_center_x = viewport_left + viewport_width * 0.5;
        let timeline_origin_x =
            viewport_center_x + crate::config::playhead_delta_pixels(viewport_width);
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

    #[inline]
    pub fn viewport_right(&self) -> f32 {
        self.viewport_left + self.viewport_width
    }

    #[inline]
    pub fn frame_x(&self, frame: f64, current_frame: f64) -> f32 {
        self.timeline_origin_x + (frame - current_frame) as f32 * self.pixels_per_frame
    }

    #[inline]
    pub fn centered_karaoke_x(&self, text_width: f32) -> f32 {
        self.viewport_center_x - text_width * 0.5
    }

    pub fn visible_frame_window(
        &self,
        current_frame: f64,
        margin_frames: i64,
    ) -> FrameWindow {
        let frames_before =
            ((self.timeline_origin_x - self.viewport_left) / self.pixels_per_frame).max(0.0);
        let frames_after =
            ((self.viewport_right() - self.timeline_origin_x) / self.pixels_per_frame).max(0.0);

        FrameWindow {
            first: (current_frame - frames_before as f64).floor() as i64 - margin_frames,
            last: (current_frame + frames_after as f64).ceil() as i64 + margin_frames,
        }
    }

    #[inline]
    pub fn timeline_origin_local_x(&self) -> f32 {
        self.timeline_origin_x - self.viewport_left
    }
}

#[inline]
pub fn normal_character_label_right(line_x: f32, pixels_per_frame: f32) -> f32 {
    line_x - 4.0 * pixels_per_frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry_with_delta(delta_pixels: f32) -> HorizontalRythmoGeometry {
        let viewport_left = 0.0;
        let viewport_width = 800.0;
        let viewport_center_x = 400.0;
        let timeline_origin_x = viewport_center_x + delta_pixels;
        HorizontalRythmoGeometry {
            viewport_left,
            viewport_width,
            viewport_center_x,
            timeline_origin_x,
            playhead_left_x: timeline_origin_x - 1.5,
            pixels_per_frame: 4.0,
        }
    }

    #[test]
    fn current_frame_is_always_under_playhead() {
        for delta in [0.0, -200.0, -400.0] {
            let geometry = geometry_with_delta(delta);
            assert_eq!(geometry.frame_x(120.0, 120.0), geometry.timeline_origin_x);
        }
    }

    #[test]
    fn karaoke_remains_centered_on_physical_viewport() {
        for delta in [0.0, -200.0, -400.0] {
            let geometry = geometry_with_delta(delta);
            let width = 180.0;
            assert_eq!(
                geometry.centered_karaoke_x(width) + width * 0.5,
                geometry.viewport_center_x
            );
        }
    }

    #[test]
    fn visible_window_is_asymmetric_when_playhead_is_left() {
        let geometry = geometry_with_delta(-200.0);
        let window = geometry.visible_frame_window(100.0, 0);
        let before = 100 - window.first;
        let after = window.last - 100;
        assert!(after > before);
        assert_eq!(window.first, 50);
        assert_eq!(window.last, 250);
    }

    #[test]
    fn normal_badge_gap_is_four_frames() {
        for ppf in [3.0, 6.0, 12.0] {
            let line_x = 500.0;
            let right = normal_character_label_right(line_x, ppf);
            assert_eq!(line_x - right, 4.0 * ppf);
        }
    }
}
