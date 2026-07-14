//! Drawing primitives and serialization.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::items_after_test_module)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DrawingStroke {
    pub id: u64,
    pub points: Vec<(f64, f32)>,
    pub color: [f32; 4],
    pub radius_frac: f32,
}

impl DrawingStroke {
    pub fn new(id: u64, color: [f32; 4], radius_frac: f32) -> Self {
        Self {
            id,
            points: Vec::new(),
            color,
            radius_frac,
        }
    }

    pub fn bbox_frames(&self) -> (i64, i64) {
        if self.points.is_empty() {
            return (0, 0);
        }
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for (f, _) in &self.points {
            lo = lo.min(*f);
            hi = hi.max(*f);
        }
        (lo.floor() as i64, hi.ceil() as i64)
    }

    pub fn intersects_window(&self, first_frame: i64, last_frame: i64) -> bool {
        let (lo, hi) = self.bbox_frames();
        hi >= first_frame && lo <= last_frame
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RythmoDrawing {
    pub strokes: Vec<DrawingStroke>,
    next_id: u64,
}

impl RythmoDrawing {
    pub fn new() -> Self {
        Self {
            strokes: Vec::new(),
            next_id: 1,
        }
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    pub fn peek_id(&self) -> u64 {
        self.next_id
    }

    pub fn add(&mut self, mut stroke: DrawingStroke) {
        if stroke.id == 0 {
            stroke.id = self.next_id();
        } else {
            self.next_id = self.next_id.max(stroke.id + 1);
        }
        self.strokes.push(stroke);
    }

    pub fn remove(&mut self, id: u64) -> Option<DrawingStroke> {
        if let Some(pos) = self.strokes.iter().position(|s| s.id == id) {
            Some(self.strokes.remove(pos))
        } else {
            None
        }
    }

    pub fn get(&self, id: u64) -> Option<&DrawingStroke> {
        self.strokes.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut DrawingStroke> {
        self.strokes.iter_mut().find(|s| s.id == id)
    }

    pub fn query_window(&self, first_frame: i64, last_frame: i64) -> Vec<&DrawingStroke> {
        self.strokes
            .iter()
            .filter(|s| s.intersects_window(first_frame, last_frame))
            .collect()
    }

    pub fn strokes_within_radius(
        &self,
        frame: f64,
        y_frac: f32,
        ppf: f32,
        zone_h: f32,
        radius_frac: f32,
    ) -> Vec<u64> {
        let mut ids = Vec::new();
        for s in &self.strokes {
            let combined_r = radius_frac.max(s.radius_frac * 0.5);
            for (f, yf) in &s.points {
                let norm_dx = (f - frame) * ppf as f64 / zone_h as f64;
                let norm_dy = (yf - y_frac) as f64;
                let dist = (norm_dx * norm_dx + norm_dy * norm_dy).sqrt() as f32;
                if dist <= combined_r {
                    ids.push(s.id);
                    break;
                }
            }
        }
        ids
    }

    /// Returns IDs of strokes whose bounding boxes intersect the given rectangle in frame-space.
    /// Rect is (min_frame, min_y_frac, max_frame, max_y_frac).
    pub fn strokes_in_rect(
        &self,
        min_frame: f64,
        min_y: f32,
        max_frame: f64,
        max_y: f32,
    ) -> Vec<u64> {
        let mut ids = Vec::new();
        for s in &self.strokes {
            let (s_min_f, s_max_f) = s.bbox_frames();
            let s_min_f = s_min_f as f64;
            let s_max_f = s_max_f as f64;
            // Quick bbox reject
            if s_max_f < min_frame || s_min_f > max_frame {
                continue;
            }
            // Check y overlap by scanning points (since y varies per point)
            let mut y_overlaps = false;
            for (_, yf) in &s.points {
                if *yf >= min_y && *yf <= max_y {
                    y_overlaps = true;
                    break;
                }
            }
            if y_overlaps {
                ids.push(s.id);
            }
        }
        ids
    }
}

pub fn ppf_for_scale(scale: f32) -> f32 {
    crate::constants::PIXELS_PER_FRAME * scale * crate::config::scroll_speed()
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
    let center_x = zone_x + zone_w / 2.0;
    let frame = current_frame + (x - center_x) as f64 / ppf as f64;
    let y_frac = ((y - zone_y) / zone_h).clamp(0.0, 1.0);
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
    let center_x = zone_x + zone_w / 2.0;
    let x = center_x + (frame - current_frame) as f32 * ppf;
    let y = zone_y + y_frac * zone_h;
    (x, y)
}

pub fn visible_frame_window(
    zone_w: f32,
    current_frame: f64,
    ppf: f32,
    margin_frames: i64,
) -> (i64, i64) {
    let half = zone_w as f64 / ppf as f64 / 2.0;
    let first = (current_frame - half).floor() as i64 - margin_frames;
    let last = (current_frame + half).ceil() as i64 + margin_frames;
    (first, last.max(first))
}

fn blend(buf: &mut [u8], i: usize, r: u8, g: u8, b: u8, a: u8) {
    let sa = a as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = buf[i + 3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        return;
    }
    let sr = r as f32 / 255.0;
    let sg = g as f32 / 255.0;
    let sb = b as f32 / 255.0;
    let dr = buf[i] as f32 / 255.0;
    let dg = buf[i + 1] as f32 / 255.0;
    let db = buf[i + 2] as f32 / 255.0;
    let or_ = (sr * sa + dr * da * (1.0 - sa)) / oa;
    let og_ = (sg * sa + dg * da * (1.0 - sa)) / oa;
    let ob_ = (sb * sa + db * da * (1.0 - sa)) / oa;
    buf[i] = (or_ * 255.0).round().clamp(0.0, 255.0) as u8;
    buf[i + 1] = (og_ * 255.0).round().clamp(0.0, 255.0) as u8;
    buf[i + 2] = (ob_ * 255.0).round().clamp(0.0, 255.0) as u8;
    buf[i + 3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn stamp_disk(buf: &mut [u8], zw: f32, zh: f32, cx: f32, cy: f32, r: f32, color: [f32; 4]) {
    let r = r.max(0.5);
    let x0 = (cx - r).floor().max(0.0) as i32;
    let x1 = (cx + r).ceil().min(zw - 1.0) as i32;
    let y0 = (cy - r).floor().max(0.0) as i32;
    let y1 = (cy + r).ceil().min(zh - 1.0) as i32;
    let r2 = r * r;
    let cr = (color[0] * 255.0) as u8;
    let cg = (color[1] * 255.0) as u8;
    let cb = (color[2] * 255.0) as u8;
    let ca = (color[3] * 255.0) as u8;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 <= r2 {
                let dist = d2.sqrt();
                let cov = (1.0 - dist / r).clamp(0.0, 1.0);
                let a = (ca as f32 * cov) as u8;
                let i = (y as usize * zw as usize + x as usize) * 4;
                blend(buf, i, cr, cg, cb, a);
            }
        }
    }
}

fn stamp_segment(
    buf: &mut [u8],
    zw: f32,
    zh: f32,
    p0: (f32, f32),
    p1: (f32, f32),
    r: f32,
    color: [f32; 4],
) {
    let dx = p1.0 - p0.0;
    let dy = p1.1 - p0.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= 0.01 {
        stamp_disk(buf, zw, zh, p0.0, p0.1, r, color);
        return;
    }
    let steps = (dist / (r * 0.5)).ceil() as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = p0.0 + dx * t;
        let y = p0.1 + dy * t;
        stamp_disk(buf, zw, zh, x, y, r, color);
    }
}

pub fn rasterize_window(
    strokes: &[&DrawingStroke],
    zone_w: u32,
    zone_h: u32,
    current_frame: f64,
    ppf: f32,
) -> Vec<u8> {
    let n = (zone_w as usize) * (zone_h as usize) * 4;
    let mut buf = vec![0u8; n];
    let center_x = zone_w as f32 / 2.0;
    let zw = zone_w as f32;
    let zh = zone_h as f32;
    for stroke in strokes {
        let r_px = (stroke.radius_frac * zh).max(1.0);
        let pts: Vec<(f32, f32)> = stroke
            .points
            .iter()
            .map(|(f, yf)| {
                let x = center_x + (f - current_frame) as f32 * ppf;
                let y = yf * zh;
                (x, y)
            })
            .collect();
        if pts.is_empty() {
            continue;
        }
        if pts.len() == 1 {
            stamp_disk(&mut buf, zw, zh, pts[0].0, pts[0].1, r_px, stroke.color);
        } else {
            for i in 0..pts.len() - 1 {
                stamp_segment(&mut buf, zw, zh, pts[i], pts[i + 1], r_px, stroke.color);
            }
        }
    }
    buf
}

/// Alpha-composite a transparent drawing raster over an RGBA render target.
/// Both buffers must describe the same pixel dimensions.
pub fn composite_rgba_over(dst: &mut [u8], src: &[u8]) {
    assert_eq!(
        dst.len(),
        src.len(),
        "RGBA buffers must have matching sizes"
    );
    for i in (0..dst.len()).step_by(4) {
        blend(dst, i, src[i], src[i + 1], src[i + 2], src[i + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_rgba_over_blends_drawing_pixels() {
        let mut dst = vec![10, 20, 30, 255, 1, 2, 3, 255];
        let src = vec![210, 20, 30, 128, 0, 0, 0, 0];

        composite_rgba_over(&mut dst, &src);

        assert_eq!(&dst[..4], &[110, 20, 30, 255]);
        assert_eq!(&dst[4..], &[1, 2, 3, 255]);
    }

    #[test]
    fn screen_rotation_does_not_skew_under_anisotropic_projection() {
        let points = [(0.0, 0.0), (1.0, 0.0)];
        let rotated = transformed_points_in_screen_space(
            &points,
            (0.0, 0.0),
            (0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
            1.0,
            10.0,
            100.0,
        );

        // One frame is 10 px wide, so after a quarter-turn it is 10 px tall,
        // i.e. 0.1 of a 100 px drawing zone.
        assert!(rotated[1].0.abs() < 1e-6);
        assert!((rotated[1].1 - 0.1).abs() < 1e-6);
    }
}

/// Compute the bounding box of multiple strokes in frame-space.
/// Returns (min_frame, min_y, max_frame, max_y).
pub fn strokes_bbox(strokes: &[&DrawingStroke]) -> Option<(f64, f32, f64, f32)> {
    let mut min_f = f64::INFINITY;
    let mut max_f = f64::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    for s in strokes {
        for (f, y) in &s.points {
            any = true;
            min_f = min_f.min(*f);
            max_f = max_f.max(*f);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
    }
    if any {
        Some((min_f, min_y, max_f, max_y))
    } else {
        None
    }
}

/// Transform strokes by translating, rotating, and scaling around a center point in frame-space.
/// All coordinates are in frame-space (frame, y_frac).
pub fn transformed_points(
    points: &[(f64, f32)],
    center: (f64, f32),
    translate: (f64, f32),
    rotate: f32,
    scale: f32,
) -> Vec<(f64, f32)> {
    let (cx, cy) = center;
    let cos_a = rotate.cos() as f64;
    let sin_a = rotate.sin() as f64;
    let scale_f64 = scale as f64;
    points
        .iter()
        .map(|(frame, y)| {
            let local_f = (*frame - cx) * scale_f64;
            let local_y = (*y - cy) * scale;
            let rot_f = local_f * cos_a - local_y as f64 * sin_a;
            let rot_y = local_f * sin_a + local_y as f64 * cos_a;
            (
                rot_f + cx + translate.0,
                (rot_y + cy as f64 + translate.1 as f64) as f32,
            )
        })
        .collect()
}

/// Transform points as a user sees them on screen, then convert them back to
/// frame/y coordinates. A frame and a normalized y unit have different pixel
/// sizes, so rotating directly in frame-space makes a screen rotation look
/// skewed. The explicit projection scales keep the operation truly rigid.
pub fn transformed_points_in_screen_space(
    points: &[(f64, f32)],
    center: (f64, f32),
    translate: (f64, f32),
    rotate: f32,
    scale: f32,
    pixels_per_frame: f32,
    zone_height: f32,
) -> Vec<(f64, f32)> {
    let (cx, cy) = center;
    let x_scale = pixels_per_frame.max(f32::EPSILON) as f64;
    let y_scale = zone_height.max(f32::EPSILON) as f64;
    let cos_a = rotate.cos() as f64;
    let sin_a = rotate.sin() as f64;
    let scale = scale.max(0.0) as f64;
    points
        .iter()
        .map(|(frame, y)| {
            let local_x = (*frame - cx) * x_scale * scale;
            let local_y = (*y - cy) as f64 * y_scale * scale;
            let rotated_x = local_x * cos_a - local_y * sin_a;
            let rotated_y = local_x * sin_a + local_y * cos_a;
            (
                rotated_x / x_scale + cx + translate.0,
                (rotated_y / y_scale + cy as f64 + translate.1 as f64) as f32,
            )
        })
        .collect()
}

pub fn transform_strokes(
    strokes: &mut [&mut DrawingStroke],
    center: (f64, f32),
    translate: (f64, f32),
    rotate: f32,
    scale: f32,
) {
    for s in strokes {
        s.points = transformed_points(&s.points, center, translate, rotate, scale);
    }
}

/// Get mutable references to strokes by IDs for transformation.
pub fn get_strokes_mut<'a>(
    drawing: &'a mut RythmoDrawing,
    ids: &[u64],
) -> Vec<&'a mut DrawingStroke> {
    drawing
        .strokes
        .iter_mut()
        .filter(|s| ids.contains(&s.id))
        .collect()
}
