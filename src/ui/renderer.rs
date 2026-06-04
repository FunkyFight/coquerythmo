use std::collections::HashMap;

use glyphon::{
    cosmic_text::{Align, Ellipsize, EllipsizeHeightLimit},
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Wrap,
};
use wgpu::util::DeviceExt;
use wgpu::MultisampleState;

use super::icons::IconAtlas;
use super::widget::{HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

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
    viewport: Viewport,

    text_texture_cache: HashMap<u64, CachedTextTexture>,
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
            viewport,
            text_texture_cache: HashMap::new(),
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
        segments: &[crate::ui::rythmo::CursorSegmentInfo],
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
        stretched: &[StretchedText],
    ) -> Vec<(IconInstance, u64)> {
        let font_size = crate::config::get().ui.font_size * 2.0;
        let mut result = Vec::new();
        let mut remaining_prewarm_misses = 2usize;

        for st in stretched {
            if st.text.is_empty() {
                continue;
            }

            let effective_font_size = font_size * st.font_scale.max(0.1);
            let tex_w = st.dest_rect.width.max(1.0).ceil() as u32;
            let tex_h = st.dest_rect.height.max(1.0).ceil() as u32;
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

            if needs_update {
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
                    st.line_id,
                    CachedTextTexture {
                        bind_group,
                        cache_hash,
                        char_x_ratios: rendered.char_x_ratios,
                    },
                );
            }

            if st.prewarm {
                continue;
            }

            if st.draw_rect.width <= 0.0 || st.draw_rect.height <= 0.0 {
                continue;
            }

            if let Some(_cached) = self.text_texture_cache.get(&st.line_id) {
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

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        quads: &[QuadInstance],         // base layer (behind video)
        overlay_quads: &[QuadInstance], // overlay layer (on top of video)
        icons: &[IconInstance],
        labels: &[LabelInfo],
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        stretched_quads: &[(IconInstance, u64)],
        post_stretched_quads: &[QuadInstance],
        base_textured: &[(IconInstance, &wgpu::BindGroup)],
        extra_textured: &[(IconInstance, &wgpu::BindGroup)],
        post_texture_quads: &[QuadInstance], // drawn after textured quads (e.g. color picker indicators)
    ) {
        let uniforms = Uniforms {
            screen_size: [screen_width as f32, screen_height as f32],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        self.viewport.update(
            queue,
            Resolution {
                width: screen_width,
                height: screen_height,
            },
        );

        let default_font_size = crate::config::get().ui.font_size;

        // Prepare text buffers
        let mut text_buffers: Vec<GlyphonBuffer> = Vec::new();
        for label in labels {
            let rect = &label.bounds;
            let padding = label.padding;
            let inner_width = (rect.width - padding * 2.0).max(0.0);
            let fs = label.font_size_override.unwrap_or(default_font_size);
            let lh = (fs * 1.3).ceil();

            let mut buffer = GlyphonBuffer::new(&mut self.font_system, Metrics::new(fs, lh));
            buffer.set_size(&mut self.font_system, Some(inner_width), Some(rect.height));

            let cosmic_align = match label.h_align {
                HAlign::Left => Align::Left,
                HAlign::Center => Align::Center,
                HAlign::Right => Align::Right,
            };

            match label.overflow {
                Overflow::Clip | Overflow::Visible => {
                    buffer.set_wrap(&mut self.font_system, Wrap::None);
                }
                Overflow::Ellipsis => {
                    buffer.set_wrap(&mut self.font_system, Wrap::None);
                    buffer.set_ellipsize(
                        &mut self.font_system,
                        Ellipsize::End(EllipsizeHeightLimit::Lines(1)),
                    );
                }
            }

            let label_family = match label.font_family_override {
                Some(name) => Family::Name(name),
                None => Family::SansSerif,
            };
            buffer.set_text(
                &mut self.font_system,
                label.text,
                &Attrs::new().family(label_family),
                Shaping::Advanced,
                None,
            );

            for line in buffer.lines.iter_mut() {
                line.set_align(Some(cosmic_align));
            }
            buffer.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buffer);
        }

        let text_areas: Vec<TextArea> = text_buffers
            .iter()
            .zip(labels.iter())
            .map(|(buffer, label)| {
                let rect = &label.bounds;
                let padding = label.padding;

                let mut text_height = 0.0_f32;
                for run in buffer.layout_runs() {
                    text_height = run.line_top + run.line_height;
                }
                if text_height == 0.0 {
                    let fs = label.font_size_override.unwrap_or(default_font_size);
                    text_height = (fs * 1.3).ceil();
                }

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
                        left: rect.x as i32,
                        top: rect.y as i32,
                        right: (rect.x + rect.width) as i32,
                        bottom: (rect.y + rect.height) as i32,
                    },
                };

                let text_color = match label.color_override {
                    Some([r, g, b]) => GlyphonColor::rgb(r, g, b),
                    None => GlyphonColor::rgb(224, 224, 224),
                };

                TextArea {
                    buffer,
                    left: rect.x + padding,
                    top: rect.y + y_offset,
                    scale: 1.0,
                    bounds,
                    default_color: text_color,
                    custom_glyphs: &[],
                }
            })
            .collect();

        self.text_renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();

        // Buffers
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Instance Buffer"),
            contents: bytemuck::cast_slice(quads),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let icon_buffer = if !icons.is_empty() {
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Icon Instance Buffer"),
                    contents: bytemuck::cast_slice(icons),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            )
        } else {
            None
        };

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
                pass.set_vertex_buffer(0, quad_buffer.slice(..));
                pass.draw(0..6, 0..quads.len() as u32);
            }

            // Draw video quad (before icons so icons render on top)
            if let Some((video_bg, video_instance)) = video_quad {
                let video_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Video Quad Buffer"),
                    contents: bytemuck::cast_slice(&[video_instance]),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_bind_group(1, video_bg, &[]);
                pass.set_vertex_buffer(0, video_buf.slice(..));
                pass.draw(0..6, 0..1);
            }

            // Draw icons
            if let Some(buf) = &icon_buffer {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_bind_group(1, &self.icon_atlas.bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..icons.len() as u32);
            }

            // Draw stretched text textures (rythmo lines)
            for (instance, line_id) in stretched_quads {
                if let Some(cached) = self.text_texture_cache.get(line_id) {
                    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Stretched Text Quad"),
                        contents: bytemuck::cast_slice(&[*instance]),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    pass.set_pipeline(&self.icon_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, 0..1);
                }
            }

            if !post_stretched_quads.is_empty() {
                let post_stretched_buf =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Post-Stretched Quad Buffer"),
                        contents: bytemuck::cast_slice(post_stretched_quads),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, post_stretched_buf.slice(..));
                pass.draw(0..6, 0..post_stretched_quads.len() as u32);
            }

            // Draw base textured quads before overlays (e.g. project actor icons)
            let base_textured_buffer = if base_textured.is_empty() {
                None
            } else {
                let instances: Vec<_> = base_textured
                    .iter()
                    .map(|(instance, _)| *instance)
                    .collect();
                Some(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Base Textured Quad Buffer"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                )
            };
            if let Some(buf) = &base_textured_buffer {
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
            }
            for (index, (_, bind_group)) in base_textured.iter().enumerate() {
                let index = index as u32;
                pass.set_bind_group(1, *bind_group, &[]);
                pass.draw(0..6, index..index + 1);
            }

            // Draw overlay quads (on top of video, icons, stretched text)
            if !overlay_quads.is_empty() {
                let overlay_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Overlay Quad Buffer"),
                    contents: bytemuck::cast_slice(overlay_quads),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, overlay_buf.slice(..));
                pass.draw(0..6, 0..overlay_quads.len() as u32);
            }

            // Draw extra textured quads (color picker gradients — after overlay background)
            for (instance, bind_group) in extra_textured {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Extra Textured Quad"),
                    contents: bytemuck::cast_slice(&[*instance]),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&self.icon_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                pass.set_bind_group(1, *bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..1);
            }

            // Draw post-texture quads (color picker indicators on top of gradients)
            if !post_texture_quads.is_empty() {
                let pt_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Post-Texture Quad Buffer"),
                    contents: bytemuck::cast_slice(post_texture_quads),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, pt_buf.slice(..));
                pass.draw(0..6, 0..post_texture_quads.len() as u32);
            }

            // Draw text
            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut pass)
                .unwrap();
        }

        self.text_atlas.trim();
    }
}
