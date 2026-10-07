//! Band texture of the export layout preview, rendered by the CPU export
//! renderer (the one that draws the band of exported MP4s) and uploaded as a
//! UI texture. It is rendered again only when its inputs change.

use std::hash::{Hash, Hasher};

use crate::project::Project;
use crate::rythmo_cpu_renderer::CpuRenderer;

use super::export_layout_page::BandPreviewRequest;

struct BandTexture {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

#[derive(Default)]
pub struct ExportBandPreviewCache {
    /// Created when the layout step opens and dropped when it closes: its
    /// font system is heavy.
    renderer: Option<CpuRenderer>,
    key: Option<u64>,
    texture: Option<BandTexture>,
}

/// Everything that changes the pixels of the band. The layout does not: the
/// export stretches and moves the band rendered at the output width, and so
/// does the preview with this texture.
pub fn band_cache_key(
    request: &BandPreviewRequest,
    frame: f64,
    source_fps: f64,
    project_revision: u64,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.width.hash(&mut hasher);
    request.br_scale.to_bits().hash(&mut hasher);
    request.karaoke_text_scale.to_bits().hash(&mut hasher);
    let frame = if frame.is_finite() { frame } else { 0.0 };
    ((frame * 1000.0).round() as i64).hash(&mut hasher);
    source_fps.to_bits().hash(&mut hasher);
    project_revision.hash(&mut hasher);
    hasher.finish()
}

impl ExportBandPreviewCache {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        request: Option<BandPreviewRequest>,
        project: &Project,
        frame: f64,
        source_fps: f64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        let Some(request) = request else {
            self.renderer = None;
            self.key = None;
            self.texture = None;
            return;
        };
        let key = band_cache_key(&request, frame, source_fps, project.revision());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let renderer = self.renderer.get_or_insert_with(CpuRenderer::new);
        let width = request.width.max(1);
        let rgba = renderer.render_br(
            project,
            frame,
            width,
            source_fps,
            request.br_scale,
            request.karaoke_text_scale,
        );
        let height = (rgba.len() / (4 * width as usize)) as u32;
        if height == 0 {
            self.texture = None;
            return;
        }
        let reuse = self
            .texture
            .as_ref()
            .is_some_and(|texture| texture.width == width && texture.height == height);
        if !reuse {
            self.texture = Some(create_texture(
                width,
                height,
                device,
                bind_group_layout,
                sampler,
            ));
        }
        if let Some(texture) = &self.texture {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture._texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba[..(4 * width * height) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.texture.as_ref().map(|texture| &texture.bind_group)
    }
}

fn create_texture(
    width: u32,
    height: u32,
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> BandTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Export Layout Band Preview"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Export Layout Band Preview"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    BandTexture {
        _texture: texture,
        _view: view,
        bind_group,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> BandPreviewRequest {
        BandPreviewRequest {
            width: 960,
            br_scale: 0.5,
            karaoke_text_scale: 2.0,
        }
    }

    #[test]
    fn band_is_rendered_again_only_when_its_inputs_change() {
        let base = band_cache_key(&request(), 120.0, 24.0, 7);
        assert_eq!(base, band_cache_key(&request(), 120.0, 24.0, 7));
        assert_ne!(base, band_cache_key(&request(), 121.0, 24.0, 7));
        assert_ne!(base, band_cache_key(&request(), 120.0, 24.0, 8));
        let wider = BandPreviewRequest {
            width: 1280,
            ..request()
        };
        assert_ne!(base, band_cache_key(&wider, 120.0, 24.0, 7));
        let bigger = BandPreviewRequest {
            br_scale: 0.6,
            ..request()
        };
        assert_ne!(base, band_cache_key(&bigger, 120.0, 24.0, 7));
    }

    #[test]
    fn preview_band_has_the_proportions_of_the_exported_band() {
        // The renderer's band height follows its width, so the reduced
        // preview texture stretched to the composed rect looks like the
        // export's band stretched by ffmpeg.
        let project = Project::new();
        let request = request();
        let preview_h = crate::rythmo_cpu_renderer::br_height(&project, request.width, 0.5);
        let export_h = crate::rythmo_cpu_renderer::br_height(&project, 1920, 0.5);
        let ratio_preview = preview_h as f32 / request.width as f32;
        let ratio_export = export_h as f32 / 1920.0;
        assert!((ratio_preview - ratio_export).abs() < 2.0 / request.width as f32);
        let mut renderer = CpuRenderer::new();
        let rgba = renderer.render_br(&project, 0.0, request.width, 24.0, 0.5, 2.0);
        assert_eq!(rgba.len() as u32, 4 * request.width * preview_h);
    }
}
