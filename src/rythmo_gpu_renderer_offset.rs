//! Offset-aware GPU export facade.
//!
//! Shifted exports stay on the GPU: one GPU pass renders the fixed karaoke
//! overlay and another renders a wider scrolling timeline that is cropped
//! around the configured playhead.

#[path = "rythmo_gpu_renderer.rs"]
mod implementation;
#[path = "rythmo_export_offset_geometry.rs"]
mod offset_geometry;
#[path = "rythmo_export_offset_nv12.rs"]
mod offset_nv12;

use crate::project::Project;
use offset_geometry::{
    copy_rgba_intersection, copy_rgba_rect, crop_rgba, intersects, restore_playhead_rgba,
    ExportPlan, KaraokeOverlay, PixelRect,
};

pub use implementation::GpuRenderStats;

pub struct GpuExportScene<'a> {
    project: &'a Project,
    inner: implementation::GpuExportScene<'a>,
}

impl<'a> GpuExportScene<'a> {
    pub fn new(project: &'a Project) -> Self {
        Self {
            project,
            inner: implementation::GpuExportScene::new(project),
        }
    }
}

pub struct GpuRenderer {
    overlay: implementation::GpuRenderer,
    timeline: implementation::GpuRenderer,
    pending: Option<PendingFrame>,
}

impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            overlay: implementation::GpuRenderer::new()?,
            timeline: implementation::GpuRenderer::new()?,
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
        self.submit(
            scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            Output::Rgba,
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
        self.submit(
            scene,
            current_frame,
            width,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            Output::Nv12 { padded_height },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn submit(
        &mut self,
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        output: Output,
    ) {
        let shifted = crate::config::playhead_offset_percent().abs() > f32::EPSILON && width > 0;
        if !shifted {
            match output {
                Output::Rgba => self.timeline.submit_render(
                    &scene.inner,
                    current_frame,
                    width,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                ),
                Output::Nv12 { padded_height } => self.timeline.submit_render_nv12(
                    &scene.inner,
                    current_frame,
                    width,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                    padded_height,
                ),
            }
            self.pending = Some(PendingFrame::passthrough(output));
            return;
        }

        let plan = ExportPlan::new(
            scene.project,
            current_frame,
            width,
            source_fps,
            br_scale,
            karaoke_text_scale,
        );
        let overlay_project = filtered_project(scene.project, false);
        let overlay_scene = implementation::GpuExportScene::new(&overlay_project);
        let timeline_project = filtered_project(scene.project, true);
        let timeline_scene = implementation::GpuExportScene::new(&timeline_project);

        match output {
            Output::Rgba => {
                self.overlay.submit_render(
                    &overlay_scene,
                    current_frame,
                    width,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                );
                self.timeline.submit_render(
                    &timeline_scene,
                    current_frame,
                    plan.virtual_width,
                    fps,
                    source_fps,
                    plan.virtual_scale,
                    karaoke_text_scale,
                );
            }
            Output::Nv12 { padded_height } => {
                self.overlay.submit_render_nv12(
                    &overlay_scene,
                    current_frame,
                    width,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                    padded_height,
                );
                self.timeline.submit_render_nv12(
                    &timeline_scene,
                    current_frame,
                    plan.virtual_width,
                    fps,
                    source_fps,
                    plan.virtual_scale,
                    karaoke_text_scale,
                    padded_height,
                );
            }
        }
        self.pending = Some(PendingFrame::shifted(output, plan));
    }

    pub fn finish_render_into(&mut self, width: u32, height: u32, out: &mut Vec<u8>) {
        let Some(frame) = self.pending.take() else {
            self.timeline.finish_render_into(width, height, out);
            return;
        };
        if !frame.shifted {
            self.timeline.finish_render_into(width, height, out);
            return;
        }

        let mut overlay = Vec::new();
        self.overlay
            .finish_render_into(frame.width, frame.height, &mut overlay);
        let mut wide = Vec::new();
        self.timeline
            .finish_render_into(frame.virtual_width, frame.height, &mut wide);
        let mut result = crop_rgba(
            &wide,
            frame.virtual_width,
            frame.width,
            frame.height,
            frame.crop_left,
        );
        composite_rgba(&overlay, &mut result, &frame);
        out.clear();
        out.extend_from_slice(&result);
    }

    pub fn finish_render_nv12_into(&mut self, out: &mut Vec<u8>) {
        let Some(frame) = self.pending.take() else {
            self.timeline.finish_render_nv12_into(out);
            return;
        };
        if !frame.shifted {
            self.timeline.finish_render_nv12_into(out);
            return;
        }
        let Output::Nv12 { padded_height } = frame.output else {
            out.clear();
            return;
        };
        let mut overlay = Vec::new();
        self.overlay.finish_render_nv12_into(&mut overlay);
        let mut wide = Vec::new();
        self.timeline.finish_render_nv12_into(&mut wide);
        let crop_left = frame.crop_left.div_euclid(2) * 2;
        let mut result = offset_nv12::crop(
            &wide,
            frame.virtual_width,
            frame.width,
            padded_height,
            crop_left,
        );
        composite_nv12(&overlay, &mut result, &frame, padded_height);
        out.clear();
        out.extend_from_slice(&result);
    }
}

fn filtered_project(project: &Project, remove_karaoke: bool) -> Project {
    let mut filtered = project.snapshot();
    let ids: Vec<u64> = filtered
        .lines()
        .filter(|line| line.karaoke == remove_karaoke)
        .map(|line| line.id)
        .collect();
    for id in ids {
        let Some(line) = filtered.get_line_mut(id) else {
            continue;
        };
        line.karaoke = false;
        line.text.clear();
        line.character_name.clear();
        line.voice_actor_names.clear();
        line.syllable_ratios.clear();
        line.note.clear();
        line.presence = crate::rythmo_line::LinePresence::On;
    }
    filtered
}

fn composite_rgba(overlay: &[u8], result: &mut [u8], frame: &PendingFrame) {
    if overlay.len() != result.len() {
        return;
    }
    let timeline = result.to_vec();
    for item in &frame.overlays {
        copy_rgba_rect(
            overlay,
            result,
            frame.width,
            frame.height,
            item.copy_rect,
        );
        restore_playhead_rgba(
            &timeline,
            result,
            frame.width,
            frame.height,
            item.copy_rect,
            frame.centered_playhead,
        );
        if !intersects(item.text_rect, frame.shifted_playhead) {
            copy_rgba_intersection(
                &timeline,
                result,
                frame.width,
                frame.height,
                item.copy_rect,
                frame.shifted_playhead,
            );
        }
    }
}

fn composite_nv12(
    overlay: &[u8],
    result: &mut [u8],
    frame: &PendingFrame,
    padded_height: u32,
) {
    if overlay.len() != result.len() {
        return;
    }
    let timeline = result.to_vec();
    for item in &frame.overlays {
        offset_nv12::copy_rect(
            overlay,
            result,
            frame.width,
            frame.height,
            padded_height,
            item.copy_rect,
        );
        offset_nv12::restore_playhead(
            &timeline,
            result,
            frame.width,
            frame.height,
            padded_height,
            item.copy_rect,
            frame.centered_playhead,
        );
        if !intersects(item.text_rect, frame.shifted_playhead) {
            offset_nv12::copy_intersection(
                &timeline,
                result,
                frame.width,
                frame.height,
                padded_height,
                item.copy_rect,
                frame.shifted_playhead,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum Output {
    Rgba,
    Nv12 { padded_height: u32 },
}

struct PendingFrame {
    shifted: bool,
    output: Output,
    width: u32,
    height: u32,
    virtual_width: u32,
    crop_left: i64,
    overlays: Vec<KaraokeOverlay>,
    centered_playhead: PixelRect,
    shifted_playhead: PixelRect,
}

impl PendingFrame {
    fn passthrough(output: Output) -> Self {
        Self {
            shifted: false,
            output,
            width: 0,
            height: 0,
            virtual_width: 0,
            crop_left: 0,
            overlays: Vec::new(),
            centered_playhead: PixelRect::default(),
            shifted_playhead: PixelRect::default(),
        }
    }

    fn shifted(output: Output, plan: ExportPlan) -> Self {
        Self {
            shifted: true,
            output,
            width: plan.width,
            height: plan.height,
            virtual_width: plan.virtual_width,
            crop_left: plan.crop_left,
            overlays: plan.overlays,
            centered_playhead: plan.centered_playhead,
            shifted_playhead: plan.shifted_playhead,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn karaoke_never_enters_the_scrolling_gpu_pass() {
        let mut project = Project::new();
        let id = project.add_line(0, 24, 0.25);
        let line = project.get_line_mut(id).unwrap();
        line.karaoke = true;
        line.text = "karaoke".into();
        let timeline = filtered_project(&project, true);
        assert!(!timeline.get_line(id).unwrap().karaoke);
        assert!(timeline.get_line(id).unwrap().text.is_empty());
    }
}
