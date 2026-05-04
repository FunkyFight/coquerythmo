use std::collections::HashMap;

use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, SwashContent, TextArea, TextAtlas, TextBounds, TextRenderer,
    Viewport, Wrap,
    cosmic_text::{Align, Ellipsize, EllipsizeHeightLimit},
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
    text_hash: u64,
    natural_width: u32,
    natural_height: u32,
    char_x_ratios: Vec<f32>, // ratio 0.0-1.0 for each char boundary (len = char_count + 1)
}

pub struct StretchedText {
    pub line_id: u64,
    pub text: String,
    pub dest_rect: Rect,
}

pub struct UiRenderer {
    quad_pipeline: wgpu::RenderPipeline,
    icon_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_group_for_icons: wgpu::BindGroup,

    pub icon_atlas: IconAtlas,
    nearest_sampler: wgpu::Sampler,

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
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 3 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 64, shader_location: 4 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 72, shader_location: 5 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 80, shader_location: 6 },
                        // shadow_blur + rotation packed as vec2
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 96, shader_location: 7 },
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
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 2 },
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

        // Nearest sampler for sharp stretched text
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
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
            nearest_sampler,
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

    pub fn texture_sampler(&self) -> &wgpu::Sampler {
        &self.icon_atlas.sampler
    }

    /// Rasterize text to CPU RGBA pixels, cache as GPU texture, return icon instances for stretched rendering.
    pub fn prepare_stretched_texts(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stretched: &[StretchedText],
    ) -> Vec<(IconInstance, u64)> {
        // Rasterize at 2x font size for sharper stretched text
        let font_size = crate::config::get().ui.font_size * 2.0;
        let mut result = Vec::new();

        for st in stretched {
            if st.text.is_empty() {
                continue;
            }

            let text_hash = Self::hash_text(&st.text);

            // Check cache
            let needs_update = match self.text_texture_cache.get(&st.line_id) {
                Some(cached) => cached.text_hash != text_hash,
                None => true,
            };

            if needs_update {
                let (pixels, w, h, char_x_ratios) = self.rasterize_text(&st.text, font_size);
                if w == 0 || h == 0 {
                    continue;
                }

                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Stretched Text"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1, sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });

                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture, mip_level: 0,
                        origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );

                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Stretched Text BG"),
                    layout: &self.icon_atlas.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.nearest_sampler) },
                    ],
                });

                self.text_texture_cache.insert(st.line_id, CachedTextTexture {
                    bind_group,
                    text_hash,
                    natural_width: w,
                    natural_height: h,
                    char_x_ratios,
                });
            }

            if let Some(_cached) = self.text_texture_cache.get(&st.line_id) {
                result.push((
                    IconInstance {
                        rect: [st.dest_rect.x, st.dest_rect.y, st.dest_rect.width, st.dest_rect.height],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0, 1.0, 1.0, 1.0],
                    },
                    st.line_id,
                ));
            }
        }

        result
    }

    fn hash_text(text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
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

    fn rasterize_text(&mut self, text: &str, font_size: f32) -> (Vec<u8>, u32, u32, Vec<f32>) {
        let rythmo_family = crate::config::get().ui.rythmo_font.clone();
        let family = match &rythmo_family {
            Some(name) => Family::Name(name),
            None => Family::SansSerif,
        };
        let line_height = (font_size * 1.4).ceil();
        let mut buffer = GlyphonBuffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(&mut self.font_system, Some(10000.0), Some(line_height));
        buffer.set_text(&mut self.font_system, text, &Attrs::new().family(family), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Measure text width and collect glyph boundaries
        let mut text_width = 0.0_f32;
        let mut glyph_ends: Vec<f32> = Vec::new(); // end x of each glyph
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let end = glyph.x + glyph.w;
                glyph_ends.push(end);
                if end > text_width {
                    text_width = end;
                }
            }
        }

        let w = (text_width.ceil() as u32).max(1);
        let h = line_height.ceil() as u32;
        let mut pixels = vec![0u8; (w * h * 4) as usize];

        // Rasterize each glyph
        for run in buffer.layout_runs() {
            let line_y = run.line_y;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                if let Some(image) = self.swash_cache.get_image_uncached(&mut self.font_system, physical.cache_key) {
                    let gx = physical.x as i32;
                    let gy = (line_y as i32) + physical.y as i32;

                    for iy in 0..image.placement.height as i32 {
                        for ix in 0..image.placement.width as i32 {
                            let px = gx + image.placement.left + ix;
                            let py = gy - image.placement.top + iy;
                            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                                continue;
                            }

                            let src_idx = (iy * image.placement.width as i32 + ix) as usize;
                            let dst_idx = ((py as u32 * w + px as u32) * 4) as usize;

                            match image.content {
                                SwashContent::Mask => {
                                    if src_idx < image.data.len() {
                                        let a = image.data[src_idx];
                                        if a > 0 && dst_idx + 3 < pixels.len() {
                                            // White text, premultiplied alpha
                                            pixels[dst_idx] = a;
                                            pixels[dst_idx + 1] = a;
                                            pixels[dst_idx + 2] = a;
                                            pixels[dst_idx + 3] = a;
                                        }
                                    }
                                }
                                SwashContent::Color => {
                                    let si = src_idx * 4;
                                    if si + 3 < image.data.len() && dst_idx + 3 < pixels.len() {
                                        pixels[dst_idx..dst_idx + 4].copy_from_slice(&image.data[si..si + 4]);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Build char boundary ratios: [0.0, end_of_char1/total, end_of_char2/total, ..., 1.0]
        let tw = if text_width > 0.0 { text_width } else { 1.0 };
        let mut char_x_ratios = vec![0.0_f32];
        for end in &glyph_ends {
            char_x_ratios.push(end / tw);
        }

        (pixels, w, h, char_x_ratios)
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        quads: &[QuadInstance],          // base layer (behind video)
        overlay_quads: &[QuadInstance],  // overlay layer (on top of video)
        icons: &[IconInstance],
        labels: &[LabelInfo],
        video_quad: Option<(&wgpu::BindGroup, IconInstance)>,
        stretched_quads: &[(IconInstance, u64)],
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

            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(fs, lh),
            );
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
                        left: i32::MIN, top: i32::MIN, right: i32::MAX, bottom: i32::MAX,
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
                device, queue, &mut self.font_system, &mut self.text_atlas,
                &self.viewport, text_areas, &mut self.swash_cache,
            )
            .unwrap();

        // Buffers
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Instance Buffer"),
            contents: bytemuck::cast_slice(quads),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let icon_buffer = if !icons.is_empty() {
            Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Icon Instance Buffer"),
                contents: bytemuck::cast_slice(icons),
                usage: wgpu::BufferUsages::VERTEX,
            }))
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
