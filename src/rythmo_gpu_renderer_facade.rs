//! GPU export renderer facade using the same semantic project as the CPU path.

#[path = "rythmo_gpu_renderer.rs"]
mod legacy;

use crate::project::Project;

pub use legacy::GpuRenderStats;

pub struct GpuExportScene {
    project: Project,
}

impl GpuExportScene {
    pub fn new(project: &Project) -> Self {
        Self {
            project: crate::rythmo_export_project::normalize_for_video(project),
        }
    }
}

pub struct GpuRenderer {
    inner: legacy::GpuRenderer,
}

impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        legacy::GpuRenderer::new().map(|inner| Self { inner })
    }

    pub fn stats(&self) -> GpuRenderStats {
        self.inner.stats()
    }

    pub fn submit_render(
        &mut self,
        scene: &GpuExportScene,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) {
        let legacy_scene = legacy::GpuExportScene::new(&scene.project);
        self.inner.submit_render(
            &legacy_scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
    }

    pub fn submit_render_nv12(
        &mut self,
        scene: &GpuExportScene,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        padded_height: u32,
    ) {
        let legacy_scene = legacy::GpuExportScene::new(&scene.project);
        self.inner.submit_render_nv12(
            &legacy_scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            padded_height,
        );
    }

    pub fn finish_render_into(&mut self, width: u32, height: u32, out: &mut Vec<u8>) {
        self.inner.finish_render_into(width, height, out);
    }

    pub fn finish_render_nv12_into(&mut self, out: &mut Vec<u8>) {
        self.inner.finish_render_nv12_into(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line_metadata::{with_kind, LineSemanticKind};

    #[test]
    fn gpu_scene_owns_an_export_only_semantic_copy() {
        let mut project = Project::new();
        let id = project.add_line(0, 24, 0.0);
        project.get_line_mut(id).unwrap().note =
            with_kind("", LineSemanticKind::AmbienceStart);
        let scene = GpuExportScene::new(&project);
        assert!(scene.project.line_count() > project.line_count());
        assert_eq!(
            crate::rythmo_line_metadata::decode(&project.get_line(id).unwrap().note).0.kind,
            LineSemanticKind::AmbienceStart
        );
    }
}
