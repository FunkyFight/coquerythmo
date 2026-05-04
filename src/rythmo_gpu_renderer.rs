use std::collections::HashMap;

use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping, SwashCache,
    SwashContent,
};
use crate::constants;
use crate::project::Project;
use crate::rythmo_line::MarkerKind;
use crate::ui::widget::{IconInstance, QuadInstance};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn quad(x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32, a: f32) -> QuadInstance {
    let c = [r, g, b, a];
    QuadInstance {
        rect: [x, y, w, h],
        color: c,
        color_bottom: c,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn rotated_line(
    cx: f32, cy: f32, length: f32, thickness: f32, angle: f32,
    r: f32, g: f32, b: f32, a: f32,
) -> QuadInstance {
    let mut q = quad(cx - length / 2.0, cy - thickness / 2.0, length, thickness, r, g, b, a);
    q.rotation = angle;
    q
}

fn text_hash(text: &str, font_size: f32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
    h.finish()
}

// ── Offscreen target with double-buffered staging ────────────────────────────

struct OffscreenTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    staging: [wgpu::Buffer; 2],
    current_staging: usize,
    width: u32,
    height: u32,
    padded_row_bytes: u32,
}

impl OffscreenTarget {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GPU Export Offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = width * 4;
        let padded_row_bytes = ((unpadded + align - 1) / align) * align;
        let buf_size = (padded_row_bytes * height) as u64;

        let make_staging = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: buf_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };

        Self {
            texture,
            view,
            staging: [make_staging("Staging A"), make_staging("Staging B")],
            current_staging: 0,
            width,
            height,
            padded_row_bytes,
        }
    }

    fn flip(&mut self) {
        self.current_staging = 1 - self.current_staging;
    }

    fn current_buf(&self) -> &wgpu::Buffer {
        &self.staging[self.current_staging]
    }
}

// ── Text cache ───────────────────────────────────────────────────────────────

struct CachedText {
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

// ── Icon draw range (batch) ──────────────────────────────────────────────────

struct IconBatch {
    hash: u64,
    start: u32,
    count: u32,
}

// ── GPU Renderer ─────────────────────────────────────────────────────────────

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BASE_TICK_WIDTH: f32 = 1.0;
const BASE_PLAYHEAD_WIDTH: f32 = 2.0;

const INITIAL_QUAD_CAP: usize = 512;
const INITIAL_ICON_CAP: usize = 256;

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    quad_pipeline: wgpu::RenderPipeline,
    icon_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_group_for_icons: wgpu::BindGroup,
    texture_bgl: wgpu::BindGroupLayout,
    nearest_sampler: wgpu::Sampler,
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_cache: HashMap<u64, CachedText>,
    offscreen: Option<OffscreenTarget>,
    // Pre-allocated GPU vertex buffers (reused across frames)
    quad_buf: wgpu::Buffer,
    quad_buf_cap: usize,
    icon_buf: wgpu::Buffer,
    icon_buf_cap: usize,
    // Reusable CPU-side pixel buffer
    pixel_buf: Vec<u8>,
}

fn create_vertex_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("No compatible GPU adapter for headless rendering: {e}"))?;

        let info = adapter.get_info();
        log::info!(
            "GPU export adapter: {} ({:?}, backend: {:?})",
            info.name, info.device_type, info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("GPU Export Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        ))
        .map_err(|e| format!("Failed to create GPU device: {e}"))?;

        // Uniform buffer + bind group layout
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export Uniforms"),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Export Uniform BGL"),
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
            label: Some("Export Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let uniform_bind_group_for_icons = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Export Icon Uniform BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Quad pipeline
        let quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Export Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui/quad.wgsl").into()),
        });

        let quad_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Export Quad PL"),
            bind_group_layouts: &[Some(&uniform_bgl)],
            immediate_size: 0,
        });

        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Export Quad Pipeline"),
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
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 96, shader_location: 7 },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &quad_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
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
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Texture bind group layout
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Export Texture BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Icon/text pipeline
        let icon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Export Icon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui/icon.wgsl").into()),
        });

        let icon_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Export Icon PL"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&texture_bgl)],
            immediate_size: 0,
        });

        let icon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Export Icon Pipeline"),
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
                    format: FORMAT,
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
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Pre-allocated vertex buffers
        let quad_buf = create_vertex_buffer(
            &device,
            "Export Quad VB",
            (INITIAL_QUAD_CAP * std::mem::size_of::<QuadInstance>()) as u64,
        );
        let icon_buf = create_vertex_buffer(
            &device,
            "Export Icon VB",
            (INITIAL_ICON_CAP * std::mem::size_of::<IconInstance>()) as u64,
        );

        Ok(Self {
            device,
            queue,
            quad_pipeline,
            icon_pipeline,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_for_icons,
            texture_bgl,
            nearest_sampler,
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            text_cache: HashMap::new(),
            offscreen: None,
            quad_buf,
            quad_buf_cap: INITIAL_QUAD_CAP,
            icon_buf,
            icon_buf_cap: INITIAL_ICON_CAP,
            pixel_buf: Vec::new(),
        })
    }

    // ── Vertex buffer management ─────────────────────────────────────────

    fn ensure_quad_buf(&mut self, count: usize) {
        if count > self.quad_buf_cap {
            let new_cap = count.next_power_of_two();
            self.quad_buf = create_vertex_buffer(
                &self.device,
                "Export Quad VB",
                (new_cap * std::mem::size_of::<QuadInstance>()) as u64,
            );
            self.quad_buf_cap = new_cap;
        }
    }

    fn ensure_icon_buf(&mut self, count: usize) {
        if count > self.icon_buf_cap {
            let new_cap = count.next_power_of_two();
            self.icon_buf = create_vertex_buffer(
                &self.device,
                "Export Icon VB",
                (new_cap * std::mem::size_of::<IconInstance>()) as u64,
            );
            self.icon_buf_cap = new_cap;
        }
    }

    // ── Text rasterization (CPU) ─────────────────────────────────────────

    fn rasterize_text(&mut self, text: &str, font_size: f32) -> (Vec<u8>, u32, u32) {
        let line_height = (font_size * 1.4).ceil();
        let mut buffer = GlyphonBuffer::new(
            &mut self.font_system,
            Metrics::new(font_size, line_height),
        );
        buffer.set_size(&mut self.font_system, Some(10000.0), Some(line_height));
        let rythmo_family = crate::config::get().ui.rythmo_font.clone();
        let family = match &rythmo_family {
            Some(name) => Family::Name(name),
            None => Family::SansSerif,
        };
        buffer.set_text(
            &mut self.font_system,
            text,
            &Attrs::new().family(family),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut text_width = 0.0_f32;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let end = glyph.x + glyph.w;
                if end > text_width {
                    text_width = end;
                }
            }
        }

        let w = (text_width.ceil() as u32).max(1);
        let h = line_height.ceil() as u32;
        let mut pixels = vec![0u8; (w * h * 4) as usize];

        for run in buffer.layout_runs() {
            let line_y = run.line_y;
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                if let Some(image) = self
                    .swash_cache
                    .get_image_uncached(&mut self.font_system, physical.cache_key)
                {
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
                                        pixels[dst_idx..dst_idx + 4]
                                            .copy_from_slice(&image.data[si..si + 4]);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        (pixels, w, h)
    }

    fn get_or_upload_text(&mut self, text: &str, font_size: f32) -> u64 {
        let hash = text_hash(text, font_size);
        if self.text_cache.contains_key(&hash) {
            return hash;
        }

        let (pixels, w, h) = self.rasterize_text(text, font_size);
        if w == 0 || h == 0 {
            return hash;
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Export Text Tex"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Export Text BG"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.nearest_sampler),
                },
            ],
        });

        self.text_cache.insert(hash, CachedText {
            bind_group,
            width: w,
            height: h,
        });

        hash
    }

    // ── Offscreen management ─────────────────────────────────────────────

    fn ensure_offscreen(&mut self, width: u32, height: u32) {
        let needs_create = match &self.offscreen {
            Some(o) => o.width != width || o.height != height,
            None => true,
        };
        if needs_create {
            self.offscreen = Some(OffscreenTarget::new(&self.device, width, height));
        }
    }

    /// Submit a frame for GPU rendering (non-blocking).
    /// Call `finish_render` to get the pixels.
    pub fn submit_render(
        &mut self,
        project: &Project,
        current_frame: f64,
        width: u32,
        _fps: f64,
        br_scale: f32,
    ) {
        // Build quads + icons using the same logic as render_br
        let s = width as f32 / constants::REF_WIDTH * br_scale;
        let used_slots = count_used_slots(project);
        let slot_count = used_slots.max(1) as f32;
        let slot_h = constants::SLOT_HEIGHT * s;
        let ruler_h = constants::RULER_HEIGHT * s;
        let ppf = constants::PIXELS_PER_FRAME * s * crate::config::scroll_speed();
        let tick_long = constants::TICK_LONG * s;
        let tick_short = constants::TICK_SHORT * s;
        let tick_w = BASE_TICK_WIDTH * s;
        let playhead_w = BASE_PLAYHEAD_WIDTH * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let badge_gap = constants::BADGE_GAP * s;
        let font_size = constants::RYTHMO_FONT_SIZE * s;
        let badge_font = constants::BADGE_FONT_SIZE * s;
        let badge_char_w = constants::BADGE_CHAR_W * s;
        let total_slot_h = slot_h + badge_h + badge_gap;
        let height = (ruler_h + slot_count * total_slot_h).ceil() as u32;

        self.ensure_offscreen(width, height);

        let w = width as f32;
        let h = height as f32;
        let center_x = w / 2.0;

        let mut quads: Vec<QuadInstance> = Vec::with_capacity(512);
        let mut all_icons: Vec<IconInstance> = Vec::with_capacity(128);
        let mut icon_batches: Vec<IconBatch> = Vec::with_capacity(32);

        // ── Ruler ticks ──
        let visible_frames = (w / ppf) as i64 + 4;
        let cf_i64 = current_frame as i64;
        let first_tick = ((cf_i64 - visible_frames / 2) / constants::TICK_GAP_FRAMES) * constants::TICK_GAP_FRAMES;
        let mut tf = first_tick;
        loop {
            let x = center_x + (tf as f64 - current_frame) as f32 * ppf;
            if x > w { break; }
            if x >= 0.0 {
                let tick_idx = tf / constants::TICK_GAP_FRAMES;
                let th = if tick_idx % 2 == 0 { tick_long } else { tick_short };
                quads.push(quad(x, 0.0, tick_w, th, 100.0/255.0, 100.0/255.0, 115.0/255.0, 128.0/255.0));
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // ── Playhead ──
        quads.push(quad(center_x - playhead_w/2.0, 0.0, playhead_w, h, 217.0/255.0, 38.0/255.0, 38.0/255.0, 1.0));

        // ── Lines ──
        for line in project.lines() {
            let x1 = center_x + (line.start_frame as f64 - current_frame) as f32 * ppf;
            let x2 = center_x + (line.end_frame() as f64 - current_frame) as f32 * ppf;
            let lw = (x2 - x1).max(2.0);
            if x1 + lw < 0.0 || x1 > w { continue; }

            let slot_idx = (line.y_slot * slot_count).round().min(slot_count - 1.0) as usize;
            let y_base = ruler_h + slot_idx as f32 * total_slot_h;

            let [cr, cg, cb, _] = line.character_color;
            let badge_w = (line.character_name.chars().count().max(1) as f32 * badge_char_w + 12.0 * s).max(16.0 * s);
            quads.push(quad(x1, y_base, badge_w, badge_h, cr, cg, cb, 1.0));

            if !line.character_name.is_empty() {
                let luminance = 0.299 * cr + 0.587 * cg + 0.114 * cb;
                let (tr, tg, tb) = if luminance > 0.55 { (0.0_f32, 0.0, 0.0) } else { (224.0/255.0, 224.0/255.0, 230.0/255.0) };
                let hash = self.get_or_upload_text(&line.character_name, badge_font);
                if let Some(cached) = self.text_cache.get(&hash) {
                    let tw = cached.width as f32;
                    let th = cached.height as f32;
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [x1 + (badge_w - tw) / 2.0, y_base + (badge_h - th) / 2.0, tw, th],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [tr, tg, tb, 1.0],
                    });
                    icon_batches.push(IconBatch { hash, start, count: 1 });
                }
            }

            let line_y = y_base + badge_h + badge_gap;

            if !line.text.is_empty() && line.text != "\u{2191}" && line.text != "\u{2193}" {
                let hash = self.get_or_upload_text(&line.text, font_size);
                if self.text_cache.contains_key(&hash) {
                    let start = all_icons.len() as u32;
                    if !line.syllable_ratios.is_empty() {
                        let lang = &crate::config::get().lang;
                        let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                        let syl_count = breaks.len() + 1;
                        if line.syllable_ratios.len() == syl_count && !breaks.is_empty() {
                            let total_chars = line.text.chars().count();
                            let char_positions: Vec<f32> = (0..=total_chars).map(|i| i as f32 / total_chars as f32).collect();
                            let mut seg_x = x1;
                            let mut prev_brk = 0;
                            for (i, &brk) in breaks.iter().enumerate() {
                                let seg_w = line.syllable_ratios.get(i).copied().unwrap_or(0.0) * lw;
                                let uv_start = char_positions.get(prev_brk).copied().unwrap_or(0.0);
                                let uv_end = char_positions.get(brk).copied().unwrap_or(1.0);
                                if seg_w > 0.5 && uv_end > uv_start {
                                    all_icons.push(IconInstance { rect: [seg_x, line_y, seg_w, slot_h], uv_rect: [uv_start, 0.0, uv_end, 1.0], tint: [1.0; 4] });
                                }
                                seg_x += seg_w;
                                prev_brk = brk;
                            }
                            let last_w = line.syllable_ratios.last().copied().unwrap_or(0.0) * lw;
                            let uv_start = char_positions.get(prev_brk).copied().unwrap_or(0.0);
                            if last_w > 0.5 && uv_start < 1.0 {
                                all_icons.push(IconInstance { rect: [seg_x, line_y, last_w, slot_h], uv_rect: [uv_start, 0.0, 1.0, 1.0], tint: [1.0; 4] });
                            }
                        } else {
                            all_icons.push(IconInstance { rect: [x1, line_y, lw, slot_h], uv_rect: [0.0, 0.0, 1.0, 1.0], tint: [1.0; 4] });
                        }
                    } else {
                        all_icons.push(IconInstance { rect: [x1, line_y, lw, slot_h], uv_rect: [0.0, 0.0, 1.0, 1.0], tint: [1.0; 4] });
                    }
                    let count = all_icons.len() as u32 - start;
                    if count > 0 { icon_batches.push(IconBatch { hash, start, count }); }
                }
            }

            if line.text == "\u{2191}" || line.text == "\u{2193}" {
                let up = line.text == "\u{2191}";
                let margin = 4.0;
                if lw > margin * 2.0 + 1.0 && slot_h > margin * 2.0 + 1.0 {
                    let dx = lw - margin * 2.0;
                    let dy = slot_h - margin * 2.0;
                    let length = (dx * dx + dy * dy).sqrt();
                    let cx = x1 + lw / 2.0;
                    let cy = line_y + slot_h / 2.0;
                    let angle = if up { (-dy).atan2(dx) } else { dy.atan2(dx) };
                    quads.push(rotated_line(cx, cy, length, 2.0*s, angle, 220.0/255.0, 220.0/255.0, 230.0/255.0, 230.0/255.0));
                }
            }

            // Note text (discrete, gray, at the bottom of the line)
            if !line.note.is_empty() {
                let note_font = badge_font * 0.9;
                let hash = self.get_or_upload_text(&line.note, note_font);
                if let Some(cached) = self.text_cache.get(&hash) {
                    let tw = cached.width as f32;
                    let _th = cached.height as f32;
                    let note_h = (note_font * 1.3).ceil();
                    let note_y = line_y + slot_h - note_h - 1.0;
                    let max_note_w = lw - 8.0 * s;
                    let draw_w = tw.min(max_note_w);
                    let uv_end = (draw_w / tw).min(1.0);
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [x1 + 4.0 * s, note_y, draw_w, note_h],
                        uv_rect: [0.0, 0.0, uv_end, 1.0],
                        tint: [160.0/255.0, 160.0/255.0, 170.0/255.0, 1.0],
                    });
                    icon_batches.push(IconBatch { hash, start, count: 1 });
                }
            }
        }

        // ── Markers ──
        for marker in &project.markers {
            let mx = center_x + (marker.frame as f64 - current_frame) as f32 * ppf;
            if mx < -10.0 * s || mx > w + 10.0 * s { continue; }
            match &marker.kind {
                MarkerKind::Boucle => {
                    quads.push(quad(mx - 1.0*s, 0.0, 2.0*s, h, 217.0/255.0, 38.0/255.0, 38.0/255.0, 230.0/255.0));
                    let cy = h / 2.0; let arm = 10.0 * s; let diag_len = arm * 2.0 * std::f32::consts::SQRT_2;
                    quads.push(rotated_line(mx, cy, diag_len, 2.5*s, std::f32::consts::FRAC_PI_4, 217.0/255.0, 38.0/255.0, 38.0/255.0, 230.0/255.0));
                    quads.push(rotated_line(mx, cy, diag_len, 2.5*s, -std::f32::consts::FRAC_PI_4, 217.0/255.0, 38.0/255.0, 38.0/255.0, 230.0/255.0));
                }
                MarkerKind::Out => {
                    quads.push(quad(mx - 1.0*s, 0.0, 2.0*s, h, 217.0/255.0, 115.0/255.0, 115.0/255.0, 180.0/255.0));
                    let cy = h / 2.0; let bh = h * 0.15;
                    for &offset in &[-5.0_f32, 5.0] {
                        let dx = bh * 0.3; let length = (dx*2.0_f32).hypot(bh*2.0); let angle = (bh*2.0).atan2(dx*2.0);
                        quads.push(rotated_line(mx + offset*s, cy, length, 2.0*s, angle, 217.0/255.0, 115.0/255.0, 115.0/255.0, 180.0/255.0));
                    }
                }
                MarkerKind::SceneChange => {
                    quads.push(quad(mx - 1.0*s, 0.0, 2.0*s, h, 230.0/255.0, 230.0/255.0, 240.0/255.0, 200.0/255.0));
                }
                MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => {
                    let is_left = matches!(marker.kind, MarkerKind::LiaisonLeft);
                    let ay = ruler_h / 2.0; let arm_x = if is_left { -3.0 } else { 3.0 } * s; let arm_y = 4.0 * s; let tip_x = mx + arm_x;
                    for &dy in &[-arm_y, arm_y] {
                        let sx = mx - arm_x; let length = ((tip_x - sx).powi(2) + dy.powi(2)).sqrt(); let angle = dy.atan2(tip_x - sx);
                        quads.push(rotated_line((sx+tip_x)/2.0, ay+dy/2.0, length, 1.5*s, angle, 180.0/255.0, 180.0/255.0, 190.0/255.0, 200.0/255.0));
                    }
                }
            }
        }

        // ── Submit GPU work (non-blocking) ──
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[width as f32, height as f32]));
        self.ensure_quad_buf(quads.len());
        self.ensure_icon_buf(all_icons.len().max(1));
        if !quads.is_empty() { self.queue.write_buffer(&self.quad_buf, 0, bytemuck::cast_slice(&quads)); }
        if !all_icons.is_empty() { self.queue.write_buffer(&self.icon_buf, 0, bytemuck::cast_slice(&all_icons)); }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Export Encoder") });

        {
            let offscreen = self.offscreen.as_ref().unwrap();
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Export Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &offscreen.view, resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 5.0/255.0, g: 5.0/255.0, b: 8.0/255.0, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
                });
                if !quads.is_empty() {
                    pass.set_pipeline(&self.quad_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.quad_buf.slice(..));
                    pass.draw(0..6, 0..quads.len() as u32);
                }
                if !all_icons.is_empty() {
                    pass.set_pipeline(&self.icon_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group_for_icons, &[]);
                    pass.set_vertex_buffer(0, self.icon_buf.slice(..));
                    for batch in &icon_batches {
                        if let Some(cached) = self.text_cache.get(&batch.hash) {
                            pass.set_bind_group(1, &cached.bind_group, &[]);
                            pass.draw(0..6, batch.start..batch.start + batch.count);
                        }
                    }
                }
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &offscreen.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo {
                    buffer: offscreen.current_buf(),
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(offscreen.padded_row_bytes), rows_per_image: Some(height) },
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // GPU is now working — caller can do I/O in parallel before calling finish_render
    }

    /// Wait for a previously submitted render and return the pixels.
    /// Caller must have called `submit_render` first.
    pub fn finish_render(&mut self, width: u32, height: u32) -> Vec<u8> {
        let offscreen = self.offscreen.as_mut().unwrap();
        let buf = offscreen.current_buf();
        let buffer_slice = buf.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });

        let data = buffer_slice.get_mapped_range();
        let unpadded_row = (width * 4) as usize;
        let padded_row = offscreen.padded_row_bytes as usize;
        let total = unpadded_row * height as usize;

        self.pixel_buf.clear();
        self.pixel_buf.reserve(total);
        if padded_row == unpadded_row {
            self.pixel_buf.extend_from_slice(&data[..total]);
        } else {
            for row in 0..height as usize {
                let start = row * padded_row;
                self.pixel_buf.extend_from_slice(&data[start..start + unpadded_row]);
            }
        }
        drop(data);
        buf.unmap();
        offscreen.flip();

        std::mem::take(&mut self.pixel_buf)
    }
}

fn count_used_slots(project: &Project) -> usize {
    let mut slots = std::collections::HashSet::new();
    for line in project.lines() {
        let idx = (line.y_slot * 4.0).round() as i32;
        slots.insert(idx);
    }
    slots.len()
}
