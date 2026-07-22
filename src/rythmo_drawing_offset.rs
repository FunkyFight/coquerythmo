#[path = "rythmo_drawing.rs"]
mod implementation;

pub use implementation::*;

fn offset_pixels(zone_width: f32) -> f32 {
    crate::config::playhead_delta_pixels(zone_width)
}

pub fn screen_to_drawing(
    x: f32,
    y: f32,
    zone_x: f32,
    zone_y: f32,
    zone_w: f32,
    zone_h: f32,
    current_frame: f64,
    ppf: f32,
) -> (f64, f32) {
    implementation::screen_to_drawing(
        x - offset_pixels(zone_w),
        y,
        zone_x,
        zone_y,
        zone_w,
        zone_h,
        current_frame,
        ppf,
    )
}

pub fn drawing_to_screen(
    frame: f64,
    y_frac: f32,
    zone_x: f32,
    zone_y: f32,
    zone_w: f32,
    zone_h: f32,
    current_frame: f64,
    ppf: f32,
) -> (f32, f32) {
    let (x, y) = implementation::drawing_to_screen(
        frame,
        y_frac,
        zone_x,
        zone_y,
        zone_w,
        zone_h,
        current_frame,
        ppf,
    );
    (x + offset_pixels(zone_w), y)
}

pub fn visible_frame_window(
    zone_w: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    let ppf = ppf.max(0.001) as f64;
    let playhead_local = (zone_w * 0.5 + offset_pixels(zone_w)).clamp(0.0, zone_w) as f64;
    let margin = margin_frames.max(0);
    let first = (current_frame - playhead_local / ppf).floor() as i64 - margin;
    let last = (current_frame + (zone_w as f64 - playhead_local) / ppf).ceil() as i64 + margin;
    (first, last.max(first))
}

pub fn rasterize_window(
    strokes: &[&DrawingStroke],
    zone_w: u32,
    zone_h: u32,
    current_frame: f64,
    ppf: f32,
) -> Vec<u8> {
    let adjusted_current_frame = current_frame
        - offset_pixels(zone_w as f32) as f64 / ppf.max(0.001) as f64;
    implementation::rasterize_window(strokes, zone_w, zone_h, adjusted_current_frame, ppf)
}

#[cfg(test)]
mod offset_tests {
    use super::*;

    #[test]
    fn asymmetric_window_keeps_more_future_frames_when_playhead_is_left() {
        let width = 800.0;
        let ppf = 4.0;
        let current = 1_000.0;
        let centered = implementation::visible_frame_window(width, current, ppf, 0);
        let shifted = visible_frame_window(width, current, ppf, 0);
        assert!(shifted.0 >= centered.0 || shifted.1 >= centered.1);
    }
}
