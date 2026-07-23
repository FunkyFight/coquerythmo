//! GPU export adapter enforcing the same prepared geometry as the CPU path.
#![allow(clippy::too_many_arguments)]

#[path = "rythmo_gpu_renderer_legacy.rs"]
mod legacy;

use crate::project::Project;
use crate::rendering::rythmo::export_adapter;
use crate::rendering::rythmo::geometry::HorizontalRythmoGeometry;

pub use legacy::GpuRenderStats;

pub struct GpuExportScene<'a> {
    project: &'a Project,
}

impl<'a> GpuExportScene<'a> {
    pub fn new(project: &'a Project) -> Self {
        Self { project }
    }
}

struct PendingRender {
    project: Project,
    geometry: HorizontalRythmoGeometry,
    current_frame: f64,
    width: u32,
    height: u32,
    padded_height: Option<u32>,
}

pub struct GpuRenderer {
    timeline: legacy::GpuRenderer,
    karaoke: legacy::GpuRenderer,
    karaoke_mask: legacy::GpuRenderer,
    pending: Option<PendingRender>,
}

impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            timeline: legacy::GpuRenderer::new()?,
            karaoke: legacy::GpuRenderer::new()?,
            karaoke_mask: legacy::GpuRenderer::new()?,
            pending: None,
        })
    }

    pub fn stats(&self) -> GpuRenderStats {
        self.timeline.stats()
    }

    pub fn submit_render(
        &mut self,
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) {
        self.submit_render_inner(
            scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            None,
        );
    }

    pub fn submit_render_nv12(
        &mut self,
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        padded_height: u32,
    ) {
        self.submit_render_inner(
            scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            Some(padded_height),
        );
    }

    fn submit_render_inner(
        &mut self,
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        padded_height: Option<u32>,
    ) {
        let geometry = export_adapter::export_geometry(width, br_scale);
        let shifted_frame = export_adapter::timeline_current_frame(&geometry, current_frame);
        let prepared = export_adapter::prepare_projects(scene.project, current_frame);
        let timeline_scene = legacy::GpuExportScene::new(&prepared.timeline);
        let karaoke_scene = legacy::GpuExportScene::new(&prepared.karaoke);
        let karaoke_mask_scene = legacy::GpuExportScene::new(&prepared.karaoke_mask);
        let ambiance_line_x: Vec<f32> = scene
            .project
            .lines()
            .filter(|line| {
                matches!(
                    line.kind,
                    crate::rythmo_line::RythmoLineKind::AmbianceStart
                )
            })
            .map(|line| geometry.frame_x(line.start_frame as f64, current_frame))
            .collect();

        crate::rythmo_layout::with_badge_render_context(
            false,
            &ambiance_line_x,
            || {
                self.timeline.submit_render(
                    &timeline_scene,
                    shifted_frame,
                    width,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                );
            },
        );
        crate::rythmo_layout::with_badge_render_context(true, &[], || {
            self.karaoke.submit_render(
                &karaoke_scene,
                current_frame,
                width,
                fps,
                source_fps,
                br_scale,
                karaoke_text_scale,
            );
        });
        crate::rythmo_layout::with_badge_render_context(true, &[], || {
            self.karaoke_mask.submit_render(
                &karaoke_mask_scene,
                current_frame,
                width,
                fps,
                source_fps,
                br_scale,
                karaoke_text_scale,
            );
        });

        self.pending = Some(PendingRender {
            project: scene.project.snapshot(),
            geometry,
            current_frame,
            width,
            height: export_adapter::export_height(scene.project, width, br_scale),
            padded_height,
        });
    }

    pub fn finish_render_into(&mut self, width: u32, height: u32, output: &mut Vec<u8>) {
        let pending = self
            .pending
            .take()
            .expect("finish_render_into called without submit_render");
        let mut karaoke = Vec::new();
        let mut karaoke_mask = Vec::new();
        self.timeline.finish_render_into(width, height, output);
        self.karaoke
            .finish_render_into(width, height, &mut karaoke);
        self.karaoke_mask
            .finish_render_into(width, height, &mut karaoke_mask);
        export_adapter::replace_changed_pixels(output, &karaoke, &karaoke_mask);
        export_adapter::overlay_drawings(
            output,
            &pending.project,
            &pending.geometry,
            pending.current_frame,
            width,
            height,
        );
    }

    pub fn finish_render_nv12_into(&mut self, output: &mut Vec<u8>) {
        let pending = self
            .pending
            .take()
            .expect("finish_render_nv12_into called without submit_render_nv12");
        let mut rgba = Vec::new();
        let mut karaoke = Vec::new();
        let mut karaoke_mask = Vec::new();
        self.timeline
            .finish_render_into(pending.width, pending.height, &mut rgba);
        self.karaoke
            .finish_render_into(pending.width, pending.height, &mut karaoke);
        self.karaoke_mask.finish_render_into(
            pending.width,
            pending.height,
            &mut karaoke_mask,
        );
        export_adapter::replace_changed_pixels(&mut rgba, &karaoke, &karaoke_mask);
        export_adapter::overlay_drawings(
            &mut rgba,
            &pending.project,
            &pending.geometry,
            pending.current_frame,
            pending.width,
            pending.height,
        );
        export_adapter::rgba_to_nv12(
            &rgba,
            output,
            pending.width,
            pending.height,
            pending.padded_height.unwrap_or(pending.height),
        );
    }
}
