//! Shared GPU text and primitive renderer.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, SyncSender};

use glyphon::{
    cosmic_text::{Align, Ellipsize, EllipsizeHeightLimit},
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Wrap,
};
use wgpu::MultisampleState;

use super::icons::IconAtlas;
use super::primitives::{HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    screen_size: [f32; 2],
}

struct CachedTextTexture {
    bind_group: wgpu::BindGroup,
    cache_hash: u64,
    char_x_ratios: Vec<f32>, // ratio 0.0-1.0 for each char boundary (len = char_count + 1)
}

struct TextRasterRequest {
    line_id: u64,
    cache_hash: u64,
    text: String,
    font_size: f32,
    width: u32,
    height: u32,
    stretch: bool,
}

struct TextRasterResult {
    line_id: u64,
    cache_hash: u64,
    rendered: Option<crate::vector_text::VectorTextPixmap>,
}

fn spawn_text_raster_worker() -> (SyncSender<TextRasterRequest>, Receiver<TextRasterResult>) {
    let (request_tx, request_rx) = std::sync::mpsc::sync_channel::<TextRasterRequest>(64);
    let (result_tx, result_rx) = std::sync::mpsc::channel::<TextRasterResult>();

    std::thread::Builder::new()
        .name("rythmo-text-raster".into())
        .spawn(move || {
            let mut font_system = FontSystem::new();
            while let Ok(request) = request_rx.recv() {
                let rendered = if request.stretch {
                    crate::vector_text::render_rythmo_text_with_ratios(
                        &mut font_system,
                        &request.text,
                        request.font_size,
                        request.width,
                        request.height,
                    )
                } else {
                    crate::vector_text::render_rythmo_text_natural(
                        &mut font_system,
                        &request.text,
                        request.font_size,
                        request.width,
                        request.height,
                    )
                };
                if result_tx
                    .send(TextRasterResult {
                        line_id: request.line_id,
                        cache_hash: request.cache_hash,
                        rendered,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .expect("failed to start rythmo text raster worker");

    (request_tx, result_rx)
}

struct DynamicBuffer {
    buffer: Option<wgpu::Buffer>,
    capacity_bytes: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct UiTextKey {
    text_hash: u64,
    text_len: usize,
    font_size_bits: u32,
    line_height_bits: u32,
    inner_width_bits: u32,
    height_bits: u32,
    h_align: u8,
    overflow: u8,
    font_family_hash: u64,
    font_family_len: usize,
}

struct CachedUiTextBuffer {
    buffer: GlyphonBuffer,
    text_height: f32,
    last_used_frame: u64,
}

impl DynamicBuffer {
    fn new() -> Self {
        Self {
            buffer: None,
            capacity_bytes: 0,
        }
    }

    fn upload<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        data: &[T],
    ) {
        if data.is_empty() {
            return;
        }

        let required = std::mem::size_of_val(data) as u64;
        if required > self.capacity_bytes {
            let capacity = required.next_power_of_two().max(1);
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.capacity_bytes = capacity;
        }

        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(data));
        }
    }

    fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }
}

pub struct StretchedText {
    pub line_id: u64,
    pub text: String,
    pub dest_rect: Rect,
    pub draw_rect: Rect,
    pub uv_rect: [f32; 4],
    pub tint: [f32; 4],
    pub stretch: bool,
    pub font_scale: f32,
    pub prewarm: bool,
}

impl StretchedText {
    pub fn new(line_id: u64, text: String, dest_rect: Rect) -> Self {
        Self {
            line_id,
            text,
            dest_rect,
            draw_rect: dest_rect,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            stretch: true,
            font_scale: 1.0,
            prewarm: false,
        }
    }

    pub fn natural(
        line_id: u64,
        text: String,
        dest_rect: Rect,
        font_scale: f32,
        tint: [f32; 4],
    ) -> Self {
        Self {
            line_id,
            text,
            dest_rect,
            draw_rect: dest_rect,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint,
            stretch: false,
            font_scale,
            prewarm: false,
        }
    }

    pub fn natural_prewarm(line_id: u64, text: String, dest_rect: Rect, font_scale: f32) -> Self {
        Self {
            line_id,
            text,
            dest_rect,
            draw_rect: Rect {
                x: dest_rect.x,
                y: dest_rect.y,
                width: 0.0,
                height: 0.0,
            },
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            stretch: false,
            font_scale,
            prewarm: true,
        }
    }

    pub fn natural_clipped(
        line_id: u64,
        text: String,
        dest_rect: Rect,
        clip_ratio: f32,
        font_scale: f32,
        tint: [f32; 4],
    ) -> Option<Self> {
        let clip_ratio = clip_ratio.clamp(0.0, 1.0);
        if clip_ratio <= 0.0 {
            return None;
        }

        let mut draw_rect = dest_rect;
        draw_rect.width *= clip_ratio;
        Some(Self {
            line_id,
            text,
            dest_rect,
            draw_rect,
            uv_rect: [0.0, 0.0, clip_ratio, 1.0],
            tint,
            stretch: false,
            font_scale,
            prewarm: false,
        })
    }
}

#[derive(Clone, Copy)]
enum TextLayer {
    Base,
    Overlay,
    Modal,
    ModalOverlay,
}

pub struct UiRenderer {
    quad_pipeline: wgpu::RenderPipeline,
    icon_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_group_for_icons: wgpu::BindGroup,

    pub icon_atlas: IconAtlas,

    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    overlay_text_renderer: TextRenderer,
    modal_text_renderer: TextRenderer,
    modal_overlay_text_renderer: TextRenderer,
    viewport: Viewport,

    text_texture_cache: HashMap<u64, CachedTextTexture>,
    text_raster_requests: SyncSender<TextRasterRequest>,
    text_raster_results: Receiver<TextRasterResult>,
    pending_text_rasters: HashMap<u64, u64>,
    ui_text_cache: HashMap<UiTextKey, CachedUiTextBuffer>,
    ui_text_frame: u64,
    quad_buffer: DynamicBuffer,
    icon_buffer: DynamicBuffer,
    video_quad_buffer: DynamicBuffer,
    stretched_text_buffer: DynamicBuffer,
    post_stretched_quad_buffer: DynamicBuffer,
    base_textured_buffer: DynamicBuffer,
    overlay_quad_buffer: DynamicBuffer,
    modal_quad_buffer: DynamicBuffer,
    modal_overlay_quad_buffer: DynamicBuffer,
    modal_textured_buffer: DynamicBuffer,
    extra_textured_buffer: DynamicBuffer,
    post_texture_quad_buffer: DynamicBuffer,
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // Shared uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("UI Uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Quad Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // == Quad pipeline ==
        let quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });

        let quad_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[Some(&uniform_bgl)],
            immediate_size: 0,
        });

        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Pipeline"),
            layout: Some(&quad_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &quad_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<QuadInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 64,
                            shader_location: 4,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 72,
                            shader_location: 5,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 80,
                            shader_location: 6,
                        },
                        // shadow_blur + rotation packed as vec2
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 96,
                            shader_location: 7,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &quad_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // == Icon atlas + pipeline ==
        let icon_atlas = IconAtlas::new(device, queue);

        let icon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Icon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("icon.wgsl").into()),
        });

        // Icon pipeline needs uniform BGL at group 0 and icon texture at group 1
        // We need a separate bind group for the uniform buffer using the same layout
        let uniform_bind_group_for_icons = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Icon Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let icon_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Icon Pipeline Layout"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&icon_atlas.bind_group_layout)],
            immediate_size: 0,
        });

        let icon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Icon Pipeline"),
            layout: Some(&icon_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &icon_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<IconInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 2,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &icon_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // == Text ==
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut text_atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, device, MultisampleState::default(), None);
        let overlay_text_renderer =
            TextRenderer::new(&mut text_atlas, device, MultisampleState::default(), None);
        let modal_text_renderer =
            TextRenderer::new(&mut text_atlas, device, MultisampleState::default(), None);
        let modal_overlay_text_renderer =
            TextRenderer::new(&mut text_atlas, device, MultisampleState::default(), None);
        let (text_raster_requests, text_raster_results) = spawn_text_raster_worker();

        Self {
            quad_pipeline,
            icon_pipeline,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_for_icons,
            icon_atlas,
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            overlay_text_renderer,
            modal_text_renderer,
            modal_overlay_text_renderer,
            viewport,
            text_texture_cache: HashMap::new(),
            text_raster_requests,
            text_raster_results,
            pending_text_rasters: HashMap::new(),
            ui_text_cache: HashMap::new(),
            ui_text_frame: 0,
            quad_buffer: DynamicBuffer::new(),
            icon_buffer: DynamicBuffer::new(),
            video_quad_buffer: DynamicBuffer::new(),
            stretched_text_buffer: DynamicBuffer::new(),
            post_stretched_quad_buffer: DynamicBuffer::new(),
            base_textured_buffer: DynamicBuffer::new(),
            overlay_quad_buffer: DynamicBuffer::new(),
            modal_quad_buffer: DynamicBuffer::new(),
            modal_overlay_quad_buffer: DynamicBuffer::new(),
            modal_textured_buffer: DynamicBuffer::new(),
            extra_textured_buffer: DynamicBuffer::new(),
            post_texture_quad_buffer: DynamicBuffer::new(),
        }
    }

    pub fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.icon_atlas.bind_group_layout
    }

    /// Returns the horizontal ratio (0.0-1.0) of cursor position within stretched text.
    pub fn cursor_x_ratio(&self, line_id: u64, cursor_pos: usize) -> f32 {
        if let Some(cached) = self.text_texture_cache.get(&line_id) {
            if cursor_pos < cached.char_x_ratios.len() {
                return cached.char_x_ratios[cursor_pos];
            }
            return 1.0;
        }
        0.0
    }

    /// Give an x ratio, returns the closest cursor index in text
    pub fn cursor_pos_from_x_ratio(&self, line_id: u64, x_ratio: f32) -> Option<usize> {
        if let Some(cached) = self.text_texture_cache.get(&line_id) {
            let mut closest = 0;
            let mut min_diff = f32::MAX;
            for (i, &r) in cached.char_x_ratios.iter().enumerate() {
                let diff = (r - x_ratio).abs();
                if diff < min_diff {
                    min_diff = diff;
                    closest = i;
                }
            }
            return Some(closest);
        }
        None
    }

    pub fn cursor_pos_from_segments(
        &self,
        segments: &[crate::workspaces::rythmo::view::CursorSegmentInfo],
        x_ratio: f32,
    ) -> Option<usize> {
        let mut closest = None;
        let mut min_diff = f32::MAX;

        for segment in segments {
            let Some(cached) = self.text_texture_cache.get(&segment.cache_id) else {
                continue;
            };
            for (local_idx, &local_ratio) in cached.char_x_ratios.iter().enumerate() {
                let global_ratio =
                    (segment.start_ratio + local_ratio * segment.width_ratio).clamp(0.0, 1.0);
                let diff = (global_ratio - x_ratio).abs();
                if diff < min_diff {
                    min_diff = diff;
                    closest = Some(segment.start_char + local_idx);
                }
            }
        }

        closest
    }

    pub fn texture_sampler(&self) -> &wgpu::Sampler {
        &self.icon_atlas.sampler
    }

    /// Render rythmo text at final size, cache as GPU texture, return icon instances.
    pub fn prepare_stretched_texts(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ui_scale: f32,
        stretched: &[StretchedText],
        async_misses: bool,
    ) -> Vec<(IconInstance, u64)> {
        self.upload_ready_text_rasters(device, queue, 2);

        let ui_scale = ui_scale.max(1.0);
        let font_size = crate::config::get().ui.font_size * 2.0 * ui_scale;
        let mut result = Vec::new();
        let mut remaining_prewarm_misses = 2usize;

        for st in stretched {
            if st.text.is_empty() {
                continue;
            }

            let effective_font_size = font_size * st.font_scale.max(0.1);
            let tex_w = (st.dest_rect.width.max(1.0) * ui_scale).ceil() as u32;
            let tex_h = (st.dest_rect.height.max(1.0) * ui_scale).ceil() as u32;
            let cache_hash =
                Self::hash_stretched_text(&st.text, effective_font_size, tex_w, tex_h, st.stretch);

            // Check cache
            let needs_update = match self.text_texture_cache.get(&st.line_id) {
                Some(cached) => cached.cache_hash != cache_hash,
                None => true,
            };

            if st.prewarm && needs_update {
                if remaining_prewarm_misses == 0 {
                    continue;
                }
                remaining_prewarm_misses -= 1;
            }

            if needs_update && async_misses {
                let already_pending =
                    self.pending_text_rasters.get(&st.line_id).copied() == Some(cache_hash);
                if !already_pending {
                    let request = TextRasterRequest {
                        line_id: st.line_id,
                        cache_hash,
                        text: st.text.clone(),
                        font_size: effective_font_size,
                        width: tex_w,
                        height: tex_h,
                        stretch: st.stretch,
                    };
                    if self.text_raster_requests.try_send(request).is_ok() {
                        self.pending_text_rasters.insert(st.line_id, cache_hash);
                    }
                }
            } else if needs_update {
                // Editing and paused views keep their immediate text response. Only
                // playback misses are rasterized away from the render thread.
                self.pending_text_rasters.remove(&st.line_id);
                let rendered = if st.stretch {
                    crate::vector_text::render_rythmo_text_with_ratios(
                        &mut self.font_system,
                        &st.text,
                        effective_font_size,
                        tex_w,
                        tex_h,
                    )
                } else {
                    crate::vector_text::render_rythmo_text_natural(
                        &mut self.font_system,
                        &st.text,
                        effective_font_size,
                        tex_w,
                        tex_h,
                    )
                };
                let Some(rendered) = rendered else {
                    continue;
                };
                let w = rendered.width;
                let h = rendered.height;
                if w == 0 || h == 0 {
                    continue;
                }
                self.upload_text_raster(device, queue, st.line_id, cache_hash, rendered);
            }

            if st.prewarm {
                continue;
            }

            if st.draw_rect.width <= 0.0 || st.draw_rect.height <= 0.0 {
                continue;
            }

            if self
                .text_texture_cache
                .get(&st.line_id)
                .is_some_and(|cached| cached.cache_hash == cache_hash)
            {
                result.push((
                    IconInstance {
                        rect: [
                            st.draw_rect.x,
                            st.draw_rect.y,
                            st.draw_rect.width,
                            st.draw_rect.height,
                        ],
                        uv_rect: st.uv_rect,
                        tint: st.tint,
                    },
                    st.line_id,
                ));
            }
        }

        result
    }

    fn upload_ready_text_rasters(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        max_uploads: usize,
    ) {
        let mut uploaded = 0;
        while uploaded < max_uploads {
            let Ok(result) = self.text_raster_results.try_recv() else {
                break;
            };
            if self.pending_text_rasters.get(&result.line_id).copied() != Some(result.cache_hash) {
                continue;
            }
            self.pending_text_rasters.remove(&result.line_id);
            let Some(rendered) = result.rendered else {
                continue;
            };
            if rendered.width == 0 || rendered.height == 0 {
                continue;
            }
            self.upload_text_raster(device, queue, result.line_id, result.cache_hash, rendered);
            uploaded += 1;
        }
    }

    fn upload_text_raster(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        line_id: u64,
        cache_hash: u64,
        rendered: crate::vector_text::VectorTextPixmap,
    ) {
        let w = rendered.width;
        let h = rendered.height;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vector Rythmo Text"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rendered.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vector Rythmo Text BG"),
            layout: &self.icon_atlas.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.icon_atlas.sampler),
                },
            ],
        });
        self.text_texture_cache.insert(
            line_id,
            CachedTextTexture {
                bind_group,
                cache_hash,
                char_x_ratios: rendered.char_x_ratios,
            },
        );
    }

    fn hash_stretched_text(
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
        stretch: bool,
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        "vector-rythmo".hash(&mut hasher);
        text.hash(&mut hasher);
        font_size.to_bits().hash(&mut hasher);
        dest_w.hash(&mut hasher);
        dest_h.hash(&mut hasher);
        stretch.hash(&mut hasher);
        crate::vector_text::rythmo_font_family_name().hash(&mut hasher);
        hasher.finish()
    }

    pub fn clear_text_cache(&mut self) {
        self.text_texture_cache.clear();
        self.pending_text_rasters.clear();
    }

    pub fn enumerate_font_families(&self) -> Vec<String> {
        let db = self.font_system.db();
        let mut families = std::collections::BTreeSet::new();
        for face in db.faces() {
            for (name, _) in face.families.iter() {
                families.insert(name.clone());
            }
        }
        families.into_iter().collect()
    }

    fn hash_text(value: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn h_align_key(align: HAlign) -> u8 {
        match align {
            HAlign::Left => 0,
            HAlign::Center => 1,
            HAlign::Right => 2,
        }
    }

    fn overflow_key(overflow: Overflow) -> u8 {
        match overflow {
            Overflow::Clip => 0,
            Overflow::Ellipsis => 1,
            Overflow::Visible => 2,
        }
    }

    fn ui_text_key(label: &LabelInfo, default_font_size: f32) -> (UiTextKey, f32, f32, f32) {
        let rect = &label.bounds;
        let padding = label.padding;
        let inner_width = (rect.width - padding * 2.0).max(0.0);
        let fs = label.font_size_override.unwrap_or(default_font_size);
        let lh = (fs * 1.3).ceil();
        let family = label.font_family_override.unwrap_or("");
        let key = UiTextKey {
            text_hash: Self::hash_text(label.text),
            text_len: label.text.len(),
            font_size_bits: fs.to_bits(),
            line_height_bits: lh.to_bits(),
            inner_width_bits: inner_width.to_bits(),
            height_bits: rect.height.max(0.0).to_bits(),
            h_align: Self::h_align_key(label.h_align),
            overflow: Self::overflow_key(label.overflow),
            font_family_hash: Self::hash_text(family),
            font_family_len: family.len(),
        };
        (key, inner_width, fs, lh)
    }

    fn build_ui_text_buffer(
        font_system: &mut FontSystem,
        label: &LabelInfo,
        inner_width: f32,
        fs: f32,
        lh: f32,
        frame: u64,
    ) -> CachedUiTextBuffer {
        let rect = &label.bounds;
        let mut buffer = GlyphonBuffer::new(font_system, Metrics::new(fs, lh));
        buffer.set_size(font_system, Some(inner_width), Some(rect.height));

        let cosmic_align = match label.h_align {
            HAlign::Left => Align::Left,
            HAlign::Center => Align::Center,
            HAlign::Right => Align::Right,
        };

        match label.overflow {
            Overflow::Clip | Overflow::Visible => {
                buffer.set_wrap(font_system, Wrap::None);
            }
            Overflow::Ellipsis => {
                buffer.set_wrap(font_system, Wrap::None);
                buffer.set_ellipsize(font_system, Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
            }
        }

        let label_family = match label.font_family_override {
            Some(name) => Family::Name(name),
            None => Family::SansSerif,
        };
        buffer.set_text(
            font_system,
            label.text,
            &Attrs::new().family(label_family),
            Shaping::Advanced,
            None,
        );

        for line in buffer.lines.iter_mut() {
            line.set_align(Some(cosmic_align));
        }
        buffer.shape_until_scroll(font_system, false);

        let mut text_height = 0.0_f32;
        for run in buffer.layout_runs() {
            text_height = run.line_top + run.line_height;
        }
        if text_height == 0.0 {
            text_height = lh;
        }

        CachedUiTextBuffer {
            buffer,
            text_height,
            last_used_frame: frame,
        }
    }

    fn evict_ui_text_cache(&mut self) {
        const MAX_UI_TEXT_CACHE_ENTRIES: usize = 2048;
        const UI_TEXT_CACHE_RETAIN_FRAMES: u64 = 180;

        if self.ui_text_cache.len() <= MAX_UI_TEXT_CACHE_ENTRIES {
            return;
        }

        let min_frame = self
            .ui_text_frame
            .saturating_sub(UI_TEXT_CACHE_RETAIN_FRAMES);
        self.ui_text_cache
            .retain(|_, cached| cached.last_used_frame >= min_frame);

        if self.ui_text_cache.len() > MAX_UI_TEXT_CACHE_ENTRIES {
            let remove_count = self.ui_text_cache.len() - MAX_UI_TEXT_CACHE_ENTRIES;
            let mut oldest: Vec<_> = self
                .ui_text_cache
                .iter()
                .map(|(key, cached)| (cached.last_used_frame, key.clone()))
                .collect();
            oldest.sort_unstable_by_key(|(last_used_frame, _)| *last_used_frame);
            for (_, key) in oldest.into_iter().take(remove_count) {
                self.ui_text_cache.remove(&key);
            }
        }
    }

    /// Build glyph geometry for the given labels and upload it to the text
    /// renderer. Each semantic layer owns a distinct glyphon `TextRenderer`,
    /// because a renderer has a single prepared vertex buffer. This preserves
    /// every label while enforcing base/overlay/modal ordering.
    fn prepare_text_layer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        labels: &[LabelInfo],
        ui_scale: f32,
        layer: TextLayer,
    ) {
        if labels.is_empty() {
            return;
        }

        let default_font_size = crate::config::get().ui.font_size;

        self.ui_text_frame = self.ui_text_frame.wrapping_add(1);
        let ui_text_frame = self.ui_text_frame;
        let mut text_keys = Vec::with_capacity(labels.len());
        for label in labels {
            let (key, inner_width, fs, lh) = Self::ui_text_key(label, default_font_size);
            match self.ui_text_cache.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().last_used_frame = ui_text_frame;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let cached = Self::build_ui_text_buffer(
                        &mut self.font_system,
                        label,
                        inner_width,
                        fs,
                        lh,
                        ui_text_frame,
                    );
                    entry.insert(cached);
                }
            }
            text_keys.push(key);
        }
        self.evict_ui_text_cache();

        let text_areas: Vec<TextArea> = text_keys
            .iter()
            .zip(labels.iter())
            .filter_map(|(key, label)| {
                let cached = self.ui_text_cache.get(key)?;
                let rect = &label.bounds;
                let padding = label.padding;
                let text_height = cached.text_height;

                let y_offset = match label.v_align {
                    VAlign::Top => 0.0,
                    VAlign::Center => (rect.height - text_height) / 2.0,
                    VAlign::Bottom => rect.height - text_height,
                };

                let bounds = match label.overflow {
                    Overflow::Visible => TextBounds {
                        left: i32::MIN,
                        top: i32::MIN,
                        right: i32::MAX,
                        bottom: i32::MAX,
                    },
                    _ => TextBounds {
                        left: (rect.x * ui_scale).floor() as i32,
                        top: (rect.y * ui_scale).floor() as i32,
                        right: ((rect.x + rect.width) * ui_scale).ceil() as i32,
                        bottom: ((rect.y + rect.height) * ui_scale).ceil() as i32,
                    },
                };

                let text_color = match label.color_override {
                    Some([r, g, b]) => GlyphonColor::rgb(r, g, b),
                    None => GlyphonColor::rgb(224, 224, 224),
                };

                Some(TextArea {
                    buffer: &cached.buffer,
                    left: (rect.x + padding) * ui_scale,
                    top: (rect.y + y_offset) * ui_scale,
                    scale: ui_scale,
                    bounds,
                    default_color: text_color,
                    custom_glyphs: &[],
                })
            })
            .collect();

        match layer {
            TextLayer::Base => self
                .text_renderer
                .prepare(
                    device,
                    queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .unwrap(),
            TextLayer::Overlay => self
                .overlay_text_renderer
                .prepare(
                    device,
                    queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .unwrap(),
            TextLayer::Modal => self
                .modal_text_renderer
                .prepare(
                    device,
                    queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .unwrap(),
            TextLayer::ModalOverlay => self
                .modal_overlay_text_renderer
                .prepare(
                    device,
                    queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .unwrap(),
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        ui_scale: f32,
        quads: &[QuadInstance],         // base layer (behind video)
        overlay_quads: &[QuadInstance], // overlay layer (on top of video)
        icons: &[IconInstance],
        labels: &[LabelInfo],         // text paired with the base layer
        overlay_labels: &[LabelInfo], // text paired with overlay quads
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        stretched_quads: &[(IconInstance, u64)],
        post_stretched_quads: &[QuadInstance],
        base_textured: &[(IconInstance, &wgpu::BindGroup)],
        extra_textured: &[(IconInstance, &wgpu::BindGroup)],
        post_texture_quads: &[QuadInstance], // drawn after textured quads (e.g. color picker indicators)
        modal_textured: &[(IconInstance, &wgpu::BindGroup)],
        modal_quads: &[QuadInstance], // modal backgrounds (above normal text)
        modal_labels: &[LabelInfo],   // modal text (above modal backgrounds)
        modal_overlay_quads: &[QuadInstance], // popups above modal content
        modal_overlay_labels: &[LabelInfo], // popup text above popup backgrounds
    ) {
        let ui_scale = ui_scale.max(1.0);
        let uniforms = Uniforms {
            screen_size: [
                screen_width as f32 / ui_scale,
                screen_height as f32 / ui_scale,
            ],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        self.viewport.update(
            queue,
            Resolution {
                width: screen_width,
                height: screen_height,
            },
        );

        self.prepare_text_layer(device, queue, labels, ui_scale, TextLayer::Base);
        self.prepare_text_layer(device, queue, overlay_labels, ui_scale, TextLayer::Overlay);

        // Upload dynamic instance buffers once per group. Draws that need different
        // bind groups reuse the same uploaded instance buffer with instance ranges.
        self.quad_buffer
            .upload(device, queue, "UI Quad Instance Buffer", quads);
        self.icon_buffer
            .upload(device, queue, "UI Icon Instance Buffer", icons);
        if let Some((_, instance)) = video_quad.as_ref() {
            self.video_quad_buffer.upload(
                device,
                queue,
                "UI Video Quad Buffer",
                std::slice::from_ref(instance),
            );
        }

        let stretched_instances: Vec<IconInstance> = stretched_quads
            .iter()
            .map(|(instance, _)| *instance)
            .collect();
        self.stretched_text_buffer.upload(
            device,
            queue,
            "UI Stretched Text Quad Buffer",
            &stretched_instances,
        );
        self.post_stretched_quad_buffer.upload(
            device,
            queue,
            "UI Post-Stretched Quad Buffer",
            post_stretched_quads,
        );
        let base_textured_instances: Vec<IconInstance> = base_textured
            .iter()
            .map(|(instance, _)| *instance)
            .collect();
        self.base_textured_buffer.upload(
            device,
            queue,
            "UI Base Textured Quad Buffer",
            &base_textured_instances,
        );
        self.overlay_quad_buffer
            .upload(device, queue, "UI Overlay Quad Buffer", overlay_quads);
        let extra_textured_instances: Vec<IconInstance> = extra_textured
            .iter()
            .map(|(instance, _)| *instance)
            .collect();
        self.extra_textured_buffer.upload(
            device,
            queue,
            "UI Extra Textured Quad Buffer",
            &extra_textured_instances,
        );
        self.post_texture_quad_buffer.upload(
            device,
            queue,
            "UI Post-Texture Quad Buffer",
            post_texture_quads,
        );
        self.modal_quad_buffer
            .upload(device, queue, "UI Modal Quad Buffer", modal_quads);
        self.modal_overlay_quad_buffer.upload(
            device,
            queue,
            "UI Modal Overlay Quad Buffer",
            modal_overlay_quads,
        );
        let modal_textured_instances: Vec<IconInstance> = modal_textured
            .iter()
            .map(|(instance, _)| *instance)
            .collect();
        self.modal_textured_buffer.upload(
            device,
            queue,
            "UI Modal Textured Quad Buffer",
            &modal_textured_instances,
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw quads
            if !quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..quads.len() as u32);
                }
            }

            // Draw video quad (before icons so icons render on top)
            if let Some((video_bg, video_instance)) = video_quad {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_bind_group(1, video_bg, &[]);
                if let Some(buffer) = self.video_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..1);
                }
                let _ = video_instance;
            }

            // Draw icons
            if !icons.is_empty() {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_bind_group(1, &self.icon_atlas.bind_group, &[]);
                if let Some(buffer) = self.icon_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..icons.len() as u32);
                }
            }

            // Draw stretched text textures (rythmo lines)
            if !stretched_quads.is_empty() {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                if let Some(buffer) = self.stretched_text_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                }
                for (index, (_, line_id)) in stretched_quads.iter().enumerate() {
                    let Some(cached) = self.text_texture_cache.get(line_id) else {
                        continue;
                    };
                    let index = index as u32;
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    pass.draw(0..6, index..index + 1);
                }
            }

            if !post_stretched_quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.post_stretched_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..post_stretched_quads.len() as u32);
                }
            }

            // Draw base textured quads before overlays (e.g. project actor icons)
            if !base_textured.is_empty() {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                if let Some(buffer) = self.base_textured_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    for (index, (_, bind_group)) in base_textured.iter().enumerate() {
                        let index = index as u32;
                        pass.set_bind_group(1, *bind_group, &[]);
                        pass.draw(0..6, index..index + 1);
                    }
                }
            }

            // Base text belongs to the workspace and is intentionally drawn
            // before overlay geometry. It can therefore never bleed through
            // panels, dropdowns, tooltips or context menus.
            if !labels.is_empty() {
                self.text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass)
                    .unwrap();
            }

            // Draw overlay quads (on top of video, icons, stretched text)
            if !overlay_quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.overlay_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..overlay_quads.len() as u32);
                }
            }

            // Draw extra textured quads (color picker gradients — after overlay background)
            if !extra_textured.is_empty() {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                if let Some(buffer) = self.extra_textured_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    for (index, (_, bind_group)) in extra_textured.iter().enumerate() {
                        let index = index as u32;
                        pass.set_bind_group(1, *bind_group, &[]);
                        pass.draw(0..6, index..index + 1);
                    }
                }
            }

            // Draw post-texture quads (color picker indicators on top of gradients)
            if !post_texture_quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.post_texture_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..post_texture_quads.len() as u32);
                }
            }

            // Overlay text is prepared independently and is drawn only after
            // every overlay background and texture in this layer.
            if !overlay_labels.is_empty() {
                self.overlay_text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass)
                    .unwrap();
            }
        }

        // Second pass: modals on top of everything (quads + text). LoadOp::Load
        // preserves the first pass's output.
        if !modal_quads.is_empty()
            || !modal_labels.is_empty()
            || !modal_overlay_quads.is_empty()
            || !modal_overlay_labels.is_empty()
        {
            self.prepare_text_layer(device, queue, modal_labels, ui_scale, TextLayer::Modal);
            self.prepare_text_layer(
                device,
                queue,
                modal_overlay_labels,
                ui_scale,
                TextLayer::ModalOverlay,
            );

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Modal Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw modal quads (dim + cards), above the normal text layer.
            if !modal_quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.modal_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..modal_quads.len() as u32);
                }
            }

            if !modal_textured.is_empty() {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                if let Some(buffer) = self.modal_textured_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    for (index, (_, bind_group)) in modal_textured.iter().enumerate() {
                        let index = index as u32;
                        pass.set_bind_group(1, *bind_group, &[]);
                        pass.draw(0..6, index..index + 1);
                    }
                }
            }

            // Draw modal text only when this frame prepared that layer; this
            // prevents glyphon's previous modal buffer from leaking forward.
            if !modal_labels.is_empty() {
                self.modal_text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass)
                    .unwrap();
            }

            if !modal_overlay_quads.is_empty() {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(buffer) = self.modal_overlay_quad_buffer.buffer() {
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..modal_overlay_quads.len() as u32);
                }
            }

            if !modal_overlay_labels.is_empty() {
                self.modal_overlay_text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass)
                    .unwrap();
            }
        }

        self.text_atlas.trim();
    }
}
