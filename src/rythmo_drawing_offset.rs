#[path = "rythmo_drawing.rs"]
mod implementation;

pub use implementation::{
    composite_rgba_over, get_strokes_mut, ppf_for_scale, strokes_bbox, transform_strokes,
    transformed_points, transformed_points_in_screen_space, DrawingStroke, RythmoDrawing,
};

#[inline]
fn configured_origin(zone_width: f32) -> f32 {
    zone_width * 0.5 + crate::config::playhead_delta_pixels(zone_width)
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
    screen_to_drawing_with_origin(
        x,
        y,
        zone_x,
        zone_y,
        zone_w,
        zone_h,
        configured_origin(zone_w),
        current_frame,
        ppf,
    )
}

pub fn screen_to_drawing_with_origin(
    x: f32,
    y: f32,
    zone_x: f32,
    zone_y: f32,
    _zone_w: f32,
    zone_h: f32,
    timeline_origin_local_x: f32,
    current_frame: f64,
    ppf: f32,
) -> (f64, f32) {
    let ppf = ppf.max(0.001);
    let origin_x = zone_x + timeline_origin_local_x;
    let frame = current_frame + (x - origin_x) as f64 / ppf as f64;
    let y_frac = ((y - zone_y) / zone_h.max(0.001)).clamp(0.0, 1.0);
    (frame, y_frac)
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
    drawing_to_screen_with_origin(
        frame,
        y_frac,
        zone_x,
        zone_y,
        zone_h,
        configured_origin(zone_w),
        current_frame,
        ppf,
    )
}

pub fn drawing_to_screen_with_origin(
    frame: f64,
    y_frac: f32,
    zone_x: f32,
    zone_y: f32,
    zone_h: f32,
    timeline_origin_local_x: f32,
    current_frame: f64,
    ppf: f32,
) -> (f32, f32) {
    (
        zone_x + timeline_origin_local_x + (frame - current_frame) as f32 * ppf,
        zone_y + y_frac * zone_h,
    )
}

pub fn visible_frame_window(
    zone_w: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    visible_frame_window_with_origin(
        zone_w,
        configured_origin(zone_w),
        current_frame,
        ppf,
        margin_frames,
    )
}

pub fn visible_frame_window_with_origin(
    zone_width: f32,
    timeline_origin_local_x: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    let ppf = ppf.max(0.001) as f64;
    let origin = timeline_origin_local_x.clamp(0.0, zone_width.max(0.0)) as f64;
    let margin = margin_frames.max(0);
    let first = (current_frame - origin / ppf).floor() as i64 - margin;
    let last = (current_frame + (zone_width as f64 - origin) / ppf).ceil() as i64 + margin;
    (first, last.max(first))
}

pub fn rasterize_window(
    strokes: &[&DrawingStroke],
    zone_w: u32,
    zone_h: u32,
    current_frame: f64,
    ppf: f32,
) -> Vec<u8> {
    rasterize_window_with_origin(
        strokes,
        zone_w,
        zone_h,
        configured_origin(zone_w as f32),
        current_frame,
        ppf,
    )
}

pub fn rasterize_window_with_origin(
    strokes: &[&DrawingStroke],
    zone_width: u32,
    zone_height: u32,
    timeline_origin_local_x: f32,
    current_frame: f64,
    ppf: f32,
) -> Vec<u8> {
    let centered_origin = zone_width as f32 * 0.5;
    let adjusted_current_frame = current_frame
        - (timeline_origin_local_x - centered_origin) as f64 / ppf.max(0.001) as f64;
    implementation::rasterize_window(
        strokes,
        zone_width,
        zone_height,
        adjusted_current_frame,
        ppf,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_at_current_frame_is_under_explicit_origin() {
        let (x, _) = drawing_to_screen_with_origin(
            120.0, 0.5, 0.0, 0.0, 200.0, 175.0, 120.0, 4.0,
        );
        assert_eq!(x, 175.0);
    }

    #[test]
    fn explicit_window_is_asymmetric() {
        let (first, last) = visible_frame_window_with_origin(800.0, 200.0, 100.0, 4.0, 0);
        assert_eq!((first, last), (50, 250));
    }
}
