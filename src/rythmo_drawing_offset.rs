#[path = "rythmo_drawing.rs"]
mod implementation;

pub use implementation::*;

fn configured_origin_local_x(zone_width: f32) -> f32 {
    zone_width * 0.5 + crate::config::playhead_delta_pixels(zone_width)
}

pub fn screen_to_drawing_with_origin(
    x: f32,
    y: f32,
    zone_x: f32,
    zone_y: f32,
    zone_w: f32,
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
    let x = zone_x + timeline_origin_local_x + (frame - current_frame) as f32 * ppf;
    let y = zone_y + y_frac * zone_h;
    (x, y)
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
        configured_origin_local_x(zone_w),
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
    drawing_to_screen_with_origin(
        frame,
        y_frac,
        zone_x,
        zone_y,
        zone_h,
        configured_origin_local_x(zone_w),
        current_frame,
        ppf,
    )
}

pub fn visible_frame_window_with_origin(
    zone_width: f32,
    timeline_origin_local_x: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    let zone_width = zone_width.max(0.0);
    let ppf = ppf.max(0.001) as f64;
    let origin = timeline_origin_local_x.clamp(0.0, zone_width) as f64;
    let margin = margin_frames.max(0);
    let first = (current_frame - origin / ppf).floor() as i64 - margin;
    let last = (current_frame + (zone_width as f64 - origin) / ppf).ceil() as i64 + margin;
    (first, last.max(first))
}

pub fn visible_frame_window(
    zone_w: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    visible_frame_window_with_origin(
        zone_w,
        configured_origin_local_x(zone_w),
        current_frame,
        ppf,
        margin_frames,
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
    let n = zone_width as usize * zone_height as usize * 4;
    let mut buffer = vec![0u8; n];
    let width = zone_width as f32;
    let height = zone_height as f32;

    for stroke in strokes {
        let radius = (stroke.radius_frac * height).max(1.0);
        let points: Vec<(f32, f32)> = stroke
            .points
            .iter()
            .map(|(frame, y_frac)| {
                (
                    timeline_origin_local_x + (*frame - current_frame) as f32 * ppf,
                    *y_frac * height,
                )
            })
            .collect();

        match points.as_slice() {
            [] => {}
            [point] => stamp_disk(
                &mut buffer,
                width,
                height,
                point.0,
                point.1,
                radius,
                stroke.color,
            ),
            _ => {
                for pair in points.windows(2) {
                    stamp_segment(
                        &mut buffer,
                        width,
                        height,
                        pair[0],
                        pair[1],
                        radius,
                        stroke.color,
                    );
                }
            }
        }
    }

    buffer
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
        configured_origin_local_x(zone_w as f32),
        current_frame,
        ppf,
    )
}

fn blend(buffer: &mut [u8], index: usize, color: [f32; 4], coverage: f32) {
    let source_alpha = (color[3] * coverage).clamp(0.0, 1.0);
    if source_alpha <= 0.0 || index + 3 >= buffer.len() {
        return;
    }
    let destination_alpha = buffer[index + 3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= 0.0 {
        return;
    }

    for channel in 0..3 {
        let source = color[channel].clamp(0.0, 1.0);
        let destination = buffer[index + channel] as f32 / 255.0;
        let output =
            (source * source_alpha + destination * destination_alpha * (1.0 - source_alpha))
                / output_alpha;
        buffer[index + channel] = (output * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    buffer[index + 3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn stamp_disk(
    buffer: &mut [u8],
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
    radius: f32,
    color: [f32; 4],
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let radius = radius.max(0.5);
    let x0 = (center_x - radius).floor().max(0.0) as i32;
    let x1 = (center_x + radius).ceil().min(width - 1.0) as i32;
    let y0 = (center_y - radius).floor().max(0.0) as i32;
    let y1 = (center_y + radius).ceil().min(height - 1.0) as i32;
    let radius_squared = radius * radius;

    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared <= radius_squared {
                let coverage = (1.0 - distance_squared.sqrt() / radius).clamp(0.0, 1.0);
                let index = (y as usize * width as usize + x as usize) * 4;
                blend(buffer, index, color, coverage);
            }
        }
    }
}

fn stamp_segment(
    buffer: &mut [u8],
    width: f32,
    height: f32,
    start: (f32, f32),
    end: (f32, f32),
    radius: f32,
    color: [f32; 4],
) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance <= 0.01 {
        stamp_disk(buffer, width, height, start.0, start.1, radius, color);
        return;
    }

    let steps = (distance / (radius * 0.5).max(0.25)).ceil() as usize;
    for index in 0..=steps {
        let t = index as f32 / steps.max(1) as f32;
        stamp_disk(
            buffer,
            width,
            height,
            start.0 + dx * t,
            start.1 + dy * t,
            radius,
            color,
        );
    }
}

#[cfg(test)]
mod offset_tests {
    use super::*;

    #[test]
    fn asymmetric_window_keeps_more_future_frames_when_playhead_is_left() {
        let (first, last) = visible_frame_window_with_origin(800.0, 200.0, 1_000.0, 4.0, 0);
        assert_eq!((first, last), (950, 1_150));
        assert!(last - 1_000 > 1_000 - first);
    }

    #[test]
    fn a_point_at_current_frame_is_rasterized_under_the_explicit_origin() {
        let mut stroke = DrawingStroke::new(1, [1.0, 1.0, 1.0, 1.0], 0.1);
        stroke.points.push((42.0, 0.5));
        let raster = rasterize_window_with_origin(&[&stroke], 100, 20, 25.0, 42.0, 4.0);
        let center_index = ((10 * 100 + 25) * 4 + 3) as usize;
        assert!(raster[center_index] > 0);
    }

    #[test]
    fn explicit_coordinate_conversion_round_trips() {
        let screen = drawing_to_screen_with_origin(120.5, 0.25, 10.0, 20.0, 80.0, 30.0, 100.0, 4.0);
        let drawing = screen_to_drawing_with_origin(
            screen.0,
            screen.1,
            10.0,
            20.0,
            200.0,
            80.0,
            30.0,
            100.0,
            4.0,
        );
        assert!((drawing.0 - 120.5).abs() < f64::EPSILON);
        assert!((drawing.1 - 0.25).abs() < f32::EPSILON);
    }
}
