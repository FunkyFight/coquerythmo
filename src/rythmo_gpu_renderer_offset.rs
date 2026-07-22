//! Export GPU facade.
//!
//! The native GPU renderer still assumes a centred playhead. Until it owns the
//! same fixed-karaoke compositing path as the CPU renderer, shifted exports are
//! deliberately routed through the offset-aware CPU fallback instead of
//! silently producing a different result.

#[path = "rythmo_gpu_renderer.rs"]
mod implementation;

pub use implementation::{GpuExportScene, GpuRenderStats};

pub struct GpuRenderer(implementation::GpuRenderer);

impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        if crate::config::playhead_offset_percent().abs() > f32::EPSILON {
            return Err(
                "shifted playhead export uses the offset-aware CPU compositor".to_string(),
            );
        }
        implementation::GpuRenderer::new().map(Self)
    }

    pub fn stats(&self) -> GpuRenderStats {
        self.0.stats()
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
        self.0.submit_render(
            scene,
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
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        padded_height: u32,
    ) {
        self.0.submit_render_nv12(
            scene,
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
        self.0.finish_render_into(width, height, out);
    }

    pub fn finish_render_nv12_into(&mut self, out: &mut Vec<u8>) {
        self.0.finish_render_nv12_into(out);
    }
}