//! CPU export adapter enforcing shared rythmo geometry.
#![allow(clippy::too_many_arguments)]

#[path = "rythmo_cpu_renderer_legacy.rs"]
mod legacy;

use crate::project::Project;
use crate::rendering::rythmo::export_adapter;

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
        let geometry = export_adapter::export_geometry(width, br_scale);
        let shifted_frame = export_adapter::timeline_current_frame(&geometry, current_frame);
        let prepared = export_adapter::prepare_projects(project, current_frame);
        let ambiance_line_x: Vec<f32> = project
            .lines()
            .filter(|line| {
                matches!(
                    line.kind,
                    crate::rythmo_line::RythmoLineKind::AmbianceStart
                )
            })
            .map(|line| geometry.frame_x(line.start_frame as f64, current_frame))
            .collect();

        let mut output = crate::rythmo_layout::with_badge_render_context(
            false,
            &ambiance_line_x,
            || {
                self.timeline.render_br(
                    &prepared.timeline,
                    shifted_frame,
                    width,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                )
            },
        );
        let karaoke = crate::rythmo_layout::with_badge_render_context(true, &[], || {
            self.karaoke.render_br(
                &prepared.karaoke,
                current_frame,
                width,
                source_fps,
                br_scale,
                karaoke_text_scale,
            )
        });
        let karaoke_mask = crate::rythmo_layout::with_badge_render_context(true, &[], || {
            self.karaoke_mask.render_br(
                &prepared.karaoke_mask,
                current_frame,
                width,
                source_fps,
                br_scale,
                karaoke_text_scale,
            )
        });

        export_adapter::replace_changed_pixels(&mut output, &karaoke, &karaoke_mask);
        let height = output.len() / (width.max(1) as usize * 4);
        export_adapter::overlay_drawings(
            &mut output,
            project,
            &geometry,
            current_frame,
            width,
            height as u32,
        );
        output
    }
}
