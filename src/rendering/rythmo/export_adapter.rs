//! Shared preparation and compositing for CPU and GPU export adapters.

use crate::project::Project;
use crate::rendering::rythmo::geometry::HorizontalRythmoGeometry;

pub struct PreparedExportProjects {
    pub timeline: Project,
    pub karaoke: Project,
    pub karaoke_mask: Project,
}

pub fn export_geometry(width: u32, br_scale: f32) -> HorizontalRythmoGeometry {
    let scale = width as f32 / crate::constants::REF_WIDTH * br_scale;
    let ppf = crate::constants::PIXELS_PER_FRAME
        * scale
        * crate::config::scroll_speed();
    HorizontalRythmoGeometry::new(0.0, width as f32, 3.0 * scale, ppf)
}

pub fn timeline_current_frame(
    geometry: &HorizontalRythmoGeometry,
    current_frame: f64,
) -> f64 {
    finite_frame(current_frame)
        - (geometry.timeline_origin_x - geometry.viewport_center_x) as f64
            / geometry.pixels_per_frame as f64
}

pub fn prepare_projects(project: &Project, current_frame: f64) -> PreparedExportProjects {
    let current_frame = finite_frame(current_frame);

    let mut timeline = project.snapshot();
    clear_drawings(&mut timeline);
    suppress_karaoke_visuals(&mut timeline, current_frame);

    let mut karaoke = project.snapshot();
    clear_drawings(&mut karaoke);

    let mut karaoke_mask = karaoke.snapshot();
    suppress_karaoke_visuals(&mut karaoke_mask, current_frame);

    PreparedExportProjects {
        timeline,
        karaoke,
        karaoke_mask,
    }
}

pub fn replace_changed_pixels(output: &mut [u8], foreground: &[u8], mask: &[u8]) {
    let pixel_count = output.len().min(foreground.len()).min(mask.len()) / 4;
    for pixel in 0..pixel_count {
        let index = pixel * 4;
        if foreground[index..index + 4] != mask[index..index + 4] {
            output[index..index + 4].copy_from_slice(&foreground[index..index + 4]);
        }
    }
}

pub fn overlay_drawings(
    output: &mut [u8],
    project: &Project,
    geometry: &HorizontalRythmoGeometry,
    current_frame: f64,
    width: u32,
    height: u32,
) {
    if width == 0 || height == 0 || output.is_empty() {
        return;
    }
    let (first_frame, last_frame) = crate::rythmo_drawing::visible_frame_window_with_origin(
        width as f32,
        geometry.timeline_origin_local_x(),
        finite_frame(current_frame),
        geometry.pixels_per_frame,
        4,
    );
    let strokes = project.drawing().query_window(first_frame, last_frame);
    if strokes.is_empty() {
        return;
    }
    let drawing = crate::rythmo_drawing::rasterize_window_with_origin(
        &strokes,
        width,
        height,
        geometry.timeline_origin_local_x(),
        finite_frame(current_frame),
        geometry.pixels_per_frame,
    );
    if drawing.len() == output.len() {
        crate::rythmo_drawing::composite_rgba_over(output, &drawing);
    }
}

pub fn export_height(project: &Project, width: u32, br_scale: f32) -> u32 {
    let scale = width as f32 / crate::constants::REF_WIDTH * br_scale;
    let normal_body_height = crate::constants::SLOT_HEIGHT * scale;
    let ruler_height = crate::constants::RULER_HEIGHT * scale;
    let badge_height = crate::constants::BADGE_HEIGHT * scale;
    let actor_icon_size = crate::constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * scale;
    let slot_header_height = badge_height.max(actor_icon_size);
    let badge_gap = crate::constants::BADGE_GAP * scale;
    let tracks = crate::rythmo_layout::build_track_layouts(
        project,
        &crate::rythmo_layout::used_track_indices(project),
        normal_body_height,
        slot_header_height,
        badge_gap,
        scale,
    );
    (ruler_height + crate::rythmo_layout::total_tracks_height(&tracks)).ceil() as u32
}

pub fn rgba_to_nv12(
    rgba: &[u8],
    output: &mut Vec<u8>,
    width: u32,
    visible_height: u32,
    padded_height: u32,
) {
    let width = width as usize;
    let visible_height = visible_height as usize;
    let padded_height = padded_height as usize;
    let frame_size = width * padded_height * 3 / 2;
    output.clear();
    output.resize(frame_size, 0);
    let uv_offset = width * padded_height;

    for y in 0..visible_height.min(padded_height) {
        for x in 0..width {
            let source = (y * width + x) * 4;
            if source + 2 >= rgba.len() {
                continue;
            }
            let r = rgba[source] as i32;
            let g = rgba[source + 1] as i32;
            let b = rgba[source + 2] as i32;
            output[y * width + x] =
                (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
        }
    }
    for y in visible_height.min(padded_height)..padded_height {
        output[y * width..(y + 1) * width].fill(16);
    }

    for cy in 0..padded_height / 2 {
        for cx in 0..width / 2 {
            let mut r = 0i32;
            let mut g = 0i32;
            let mut b = 0i32;
            let mut count = 0i32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let y = cy * 2 + dy;
                    let x = cx * 2 + dx;
                    if y >= visible_height || y >= padded_height {
                        continue;
                    }
                    let source = (y * width + x) * 4;
                    if source + 2 >= rgba.len() {
                        continue;
                    }
                    r += rgba[source] as i32;
                    g += rgba[source + 1] as i32;
                    b += rgba[source + 2] as i32;
                    count += 1;
                }
            }
            if count == 0 {
                output[uv_offset + cy * width + cx * 2] = 128;
                output[uv_offset + cy * width + cx * 2 + 1] = 128;
                continue;
            }
            r /= count;
            g /= count;
            b /= count;
            output[uv_offset + cy * width + cx * 2] =
                (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            output[uv_offset + cy * width + cx * 2 + 1] =
                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
}

fn finite_frame(frame: f64) -> f64 {
    if frame.is_finite() { frame } else { 0.0 }
}

fn clear_drawings(project: &mut Project) {
    let ids: Vec<u64> = project.drawing().strokes.iter().map(|stroke| stroke.id).collect();
    project.remove_drawing_strokes(&ids);
}

fn suppress_karaoke_visuals(project: &mut Project, current_frame: f64) {
    let hidden_start = current_frame
        .ceil()
        .clamp(i64::MIN as f64, i64::MAX as f64 - 2_000_000.0) as i64
        + 1_000_000;
    let ids: Vec<u64> = project
        .lines()
        .filter(|line| line.karaoke)
        .map(|line| line.id)
        .collect();
    for id in ids {
        if let Some(line) = project.get_line_mut(id) {
            line.start_frame = hidden_start;
            line.duration_frames = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_pixels_replace_only_foreground_delta() {
        let mut output = vec![1, 2, 3, 255, 4, 5, 6, 255];
        let foreground = vec![9, 9, 9, 255, 8, 8, 8, 255];
        let mask = vec![0, 0, 0, 255, 8, 8, 8, 255];
        replace_changed_pixels(&mut output, &foreground, &mask);
        assert_eq!(output, vec![9, 9, 9, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn nv12_size_matches_ffmpeg_contract() {
        let rgba = vec![0; 8 * 4 * 4];
        let mut nv12 = Vec::new();
        rgba_to_nv12(&rgba, &mut nv12, 8, 4, 6);
        assert_eq!(nv12.len(), 8 * 6 * 3 / 2);
    }
}
