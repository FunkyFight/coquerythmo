//! CPU export adapter enforcing shared rythmo geometry.
#![allow(clippy::too_many_arguments)]

#[path = "rythmo_cpu_renderer_legacy.rs"]
mod legacy;

use crate::project::Project;
use crate::rendering::rythmo::geometry::HorizontalRythmoGeometry;

pub struct CpuRenderer {
    timeline: legacy::CpuRenderer,
    karaoke: legacy::CpuRenderer,
    karaoke_mask: legacy::CpuRenderer,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self {
            timeline: legacy::CpuRenderer::new(),
            karaoke: legacy::CpuRenderer::new(),
            karaoke_mask: legacy::CpuRenderer::new(),
        }
    }

    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        let current_frame = finite_frame(current_frame);
        let scale = width as f32 / crate::constants::REF_WIDTH * br_scale;
        let ppf = crate::constants::PIXELS_PER_FRAME
            * scale
            * crate::config::scroll_speed();
        let playhead_width = 3.0 * scale;
        let geometry = HorizontalRythmoGeometry::new(
            0.0,
            width as f32,
            playhead_width,
            ppf,
        );
        let shifted_frame = current_frame
            - (geometry.timeline_origin_x - geometry.viewport_center_x) as f64
                / geometry.pixels_per_frame as f64;

        let mut timeline_project = project.snapshot();
        clear_drawings(&mut timeline_project);
        suppress_karaoke_visuals(&mut timeline_project, current_frame);

        let mut karaoke_project = project.snapshot();
        clear_drawings(&mut karaoke_project);

        let mut karaoke_mask_project = karaoke_project.snapshot();
        suppress_karaoke_visuals(&mut karaoke_mask_project, current_frame);

        let mut output = self.timeline.render_br(
            &timeline_project,
            shifted_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
        let karaoke = self.karaoke.render_br(
            &karaoke_project,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
        let karaoke_mask = self.karaoke_mask.render_br(
            &karaoke_mask_project,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );

        replace_changed_pixels(&mut output, &karaoke, &karaoke_mask);

        let height = output.len() / (width as usize * 4);
        if height > 0 {
            let margin_frames = 4;
            let (first_frame, last_frame) =
                crate::rythmo_drawing::visible_frame_window_with_origin(
                    width as f32,
                    geometry.timeline_origin_local_x(),
                    current_frame,
                    geometry.pixels_per_frame,
                    margin_frames,
                );
            let strokes: Vec<_> = project
                .drawing()
                .query_window(first_frame, last_frame);
            if !strokes.is_empty() {
                let drawing = crate::rythmo_drawing::rasterize_window_with_origin(
                    &strokes,
                    width,
                    height as u32,
                    geometry.timeline_origin_local_x(),
                    current_frame,
                    geometry.pixels_per_frame,
                );
                crate::rythmo_drawing::composite_rgba_over(&mut output, &drawing);
            }
        }

        output
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

fn replace_changed_pixels(output: &mut [u8], foreground: &[u8], mask: &[u8]) {
    let pixel_count = output.len().min(foreground.len()).min(mask.len()) / 4;
    for pixel in 0..pixel_count {
        let index = pixel * 4;
        if foreground[index..index + 4] != mask[index..index + 4] {
            output[index..index + 4].copy_from_slice(&foreground[index..index + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_pixels_replace_only_karaoke_layer() {
        let mut output = vec![1, 2, 3, 255, 4, 5, 6, 255];
        let foreground = vec![9, 9, 9, 255, 8, 8, 8, 255];
        let mask = vec![0, 0, 0, 255, 8, 8, 8, 255];
        replace_changed_pixels(&mut output, &foreground, &mask);
        assert_eq!(output, vec![9, 9, 9, 255, 4, 5, 6, 255]);
    }
}
