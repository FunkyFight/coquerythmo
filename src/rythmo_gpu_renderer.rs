//! GPU renderer for the shared rythmo scene.
//!
//! Backend signatures expose the complete render context used by export and
//! preview paths.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::constants;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rendering::rythmo::scene::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, RythmoScene, SceneLine, SceneOptions,
};
use crate::rythmo_layout;
use crate::rythmo_line::{MarkerKind, RythmoLine};
use crate::ui::primitives::{IconInstance, QuadInstance, Rect};
use crate::voice_actor::{decode_icon_rgba, VoiceActor, VOICE_ACTOR_ICON_SIZE};
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};

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

fn quad_rounded(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
    border_radius: f32,
) -> QuadInstance {
    let mut q = quad(x, y, w, h, r, g, b, a);
    q.border_radius = border_radius;
    q
}

fn rotated_line(
    cx: f32,
    cy: f32,
    length: f32,
    thickness: f32,
    angle: f32,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> QuadInstance {
    let mut q = quad(
        cx - length / 2.0,
        cy - thickness / 2.0,
        length,
        thickness,
        r,
        g,
        b,
        a,
    );
    q.rotation = angle;
    q
}

fn push_playhead_segments(
    quads: &mut Vec<QuadInstance>,
    x: f32,
    width: f32,
    height: f32,
    skip_ranges: &[(f32, f32)],
) {
    let mut ranges: Vec<(f32, f32)> = skip_ranges
        .iter()
        .map(|(start, end)| (start.max(0.0), end.min(height)))
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut y = 0.0;
    for (skip_start, skip_end) in ranges {
        if skip_start > y {
            quads.push(quad(
                x,
                y,
                width,
                skip_start - y,
                217.0 / 255.0,
                38.0 / 255.0,
                38.0 / 255.0,
                1.0,
            ));
        }
        y = y.max(skip_end);
    }
    if y < height {
        quads.push(quad(
            x,
            y,
            width,
            height - y,
            217.0 / 255.0,
            38.0 / 255.0,
            38.0 / 255.0,
            1.0,
        ));
    }
}

fn push_karaoke_dot(
    quads: &mut Vec<QuadInstance>,
    line: &RythmoLine,
    current_frame: f64,
    x: f32,
    y: f32,
    width: f32,
    scale: f32,
) {
    let Some(progress) = line.karaoke_progress(current_frame) else {
        return;
    };
    let ratios = crate::syllable::timing_ratios(
        &line.text,
        &line.syllable_ratios,
        &crate::config::get().lang,
    );
    let local_progress = crate::syllable::active_syllable_local_progress(&ratios, progress)
        .unwrap_or(progress)
        .clamp(0.0, 1.0);
    let visual_progress = crate::syllable::visual_progress_from_timing(
        &line.text,
        &line.syllable_ratios,
        &crate::config::get().lang,
        progress,
    );
    let bounce = (local_progress * std::f32::consts::PI).sin().max(0.0);
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let center_x = if width > size {
        x + size / 2.0 + visual_progress.clamp(0.0, 1.0) * (width - size)
    } else {
        x + width / 2.0
    };
    let dx = center_x - size / 2.0;
    let dy = y + 3.0 * scale.max(0.5) - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;

    let mut shadow = quad(
        dx - 1.5,
        dy - 1.5,
        size + 3.0,
        size + 3.0,
        0.0,
        0.0,
        0.0,
        0.35,
    );
    shadow.border_radius = (size + 3.0) / 2.0;
    quads.push(shadow);

    let mut dot = quad(
        dx,
        dy,
        size,
        size,
        line.character_color[0].clamp(0.0, 1.0),
        line.character_color[1].clamp(0.0, 1.0),
        line.character_color[2].clamp(0.0, 1.0),
        1.0,
    );
    dot.color_bottom = dot.color;
    dot.border_color = [1.0, 1.0, 1.0, 0.85];
    dot.border_width = 1.0;
    dot.border_radius = size / 2.0;
    quads.push(dot);
}

fn karaoke_count_in_dot_rect(
    x: f32,
    y: f32,
    count_in_progress: f32,
    scale: f32,
) -> (f32, f32, f32) {
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let progress = count_in_progress.clamp(0.0, 1.0);
    let bounce_progress = (progress * constants::KARAOKE_COUNT_IN_BOUNCES).fract();
    let bounce = (bounce_progress * std::f32::consts::PI).sin().max(0.0);
    let travel = constants::KARAOKE_NEXT_PREVIEW_GAP * 4.0 * scale + size * 2.0;
    let dx = x - travel + travel * progress;
    let dy = y + 3.0 * scale.max(0.5) - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
    (dx, dy, size)
}

fn push_karaoke_count_in_dot(
    quads: &mut Vec<QuadInstance>,
    line: &RythmoLine,
    x: f32,
    y: f32,
    count_in_progress: Option<f32>,
    scale: f32,
) {
    let Some(count_in_progress) = count_in_progress else {
        return;
    };

    let (dx, dy, size) = karaoke_count_in_dot_rect(x, y, count_in_progress, scale);
    let mut shadow = quad(
        dx - 1.5,
        dy - 1.5,
        size + 3.0,
        size + 3.0,
        0.0,
        0.0,
        0.0,
        0.35,
    );
    shadow.border_radius = (size + 3.0) / 2.0;
    quads.push(shadow);

    let mut dot = quad(
        dx,
        dy,
        size,
        size,
        line.character_color[0].clamp(0.0, 1.0),
        line.character_color[1].clamp(0.0, 1.0),
        line.character_color[2].clamp(0.0, 1.0),
        1.0,
    );
    dot.color_bottom = dot.color;
    dot.border_color = [1.0, 1.0, 1.0, 0.85];
    dot.border_width = 1.0;
    dot.border_radius = size / 2.0;
    quads.push(dot);
}

fn text_hash(
    kind: &str,
    text: &str,
    font_size: f32,
    width: Option<u32>,
    height: Option<u32>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
    width.hash(&mut h);
    height.hash(&mut h);
    crate::vector_text::rythmo_font_family_name().hash(&mut h);
    h.finish()
}

fn text_tile_hash(
    kind: &str,
    text: &str,
    font_size: f32,
    full_width: u32,
    height: u32,
    tile_x: u32,
    tile_width: u32,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    text.hash(&mut h);
    font_size.to_bits().hash(&mut h);
    full_width.hash(&mut h);
    height.hash(&mut h);
    tile_x.hash(&mut h);
    tile_width.hash(&mut h);
    crate::vector_text::rythmo_font_family_name().hash(&mut h);
    h.finish()
}

fn voice_actor_icon_texture_hash(icon_png_base64: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "voice-actor-icon".hash(&mut h);
    icon_png_base64.hash(&mut h);
    h.finish()
}

fn export_backends() -> wgpu::Backends {
    #[cfg(target_os = "windows")]
    {
        wgpu::Backends::DX12
    }

    #[cfg(not(target_os = "windows"))]
    {
        wgpu::Backends::PRIMARY
    }
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
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = width * 4;
        let padded_row_bytes = unpadded.div_ceil(align) * align;
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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Nv12Params {
    width: u32,
    height: u32,
    padded_height: u32,
    total_bytes: u32,
}

struct Nv12Target {
    storage: wgpu::Buffer,
    staging: [wgpu::Buffer; 2],
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    current_staging: usize,
    width: u32,
    height: u32,
    padded_height: u32,
    frame_size: usize,
    buffer_size: u64,
}

impl Nv12Target {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        source_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        padded_height: u32,
    ) -> Self {
        let frame_size = width as usize * padded_height as usize * 3 / 2;
        let buffer_size = ((frame_size + 3) & !3) as u64;
        let storage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export NV12 Storage"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let make_staging = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export NV12 Params"),
            size: std::mem::size_of::<Nv12Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Export NV12 BG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: storage.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            storage,
            staging: [
                make_staging("NV12 Staging A"),
                make_staging("NV12 Staging B"),
            ],
            params_buffer,
            bind_group,
            current_staging: 0,
            width,
            height,
            padded_height,
            frame_size,
            buffer_size,
        }
    }

    fn flip(&mut self) {
        self.current_staging = 1 - self.current_staging;
    }

    fn current_buf(&self) -> &wgpu::Buffer {
        &self.staging[self.current_staging]
    }

    fn word_count(&self) -> u32 {
        (self.buffer_size / 4) as u32
    }
}

#[derive(Clone, Debug, Default)]
pub struct GpuRenderStats {
    pub frames_submitted: u64,
    pub draw_calls: u64,
    pub text_uploads: u64,
    pub icon_uploads: u64,
    pub texture_creations: u64,
    pub bind_groups_created: u64,
    pub last_frame_quads: usize,
    pub last_frame_icons: usize,
    pub last_frame_icon_batches: usize,
    pub last_frame_draw_calls: u64,
    pub last_readback_bytes: usize,
    pub total_readback_bytes: u64,
    pub text_upload_time: Duration,
    pub icon_upload_time: Duration,
}

pub struct GpuExportScene<'a> {
    project: &'a Project,
    render_index: ProjectRenderIndex,
    voice_actors_by_name: HashMap<&'a str, &'a VoiceActor>,
}

impl<'a> GpuExportScene<'a> {
    pub fn new(project: &'a Project) -> Self {
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        let voice_actors_by_name = project
            .voice_actors()
            .iter()
            .map(|actor| (actor.name.as_str(), actor))
            .collect();
        Self {
            project,
            render_index,
            voice_actors_by_name,
        }
    }

    fn voice_actor(&self, name: &str) -> Option<&'a VoiceActor> {
        self.voice_actors_by_name.get(name).copied()
    }
}

// ── Text cache ───────────────────────────────────────────────────────────────

struct CachedText {
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

struct CachedActorIconRef {
    hash: u64,
    icon_ptr: usize,
    icon_len: usize,
}

struct FailedActorIconRef {
    hash: u64,
    icon_ptr: usize,
    icon_len: usize,
}

struct DrawingOverlayTexture {
    texture: wgpu::Texture,
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
const BASE_TICK_WIDTH: f32 = 1.5;
const BASE_PLAYHEAD_WIDTH: f32 = 2.0;

const INITIAL_QUAD_CAP: usize = 512;
const INITIAL_ICON_CAP: usize = 256;
const MAX_TEXT_TEXTURE_DIMENSION: u32 = 8192;

#[derive(Clone, Copy)]
enum ReadbackMode {
    Rgba,
    Nv12 { padded_height: u32 },
}

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    quad_pipeline: wgpu::RenderPipeline,
    icon_pipeline: wgpu::RenderPipeline,
    nv12_pipeline: wgpu::ComputePipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_group_for_icons: wgpu::BindGroup,
    texture_bgl: wgpu::BindGroupLayout,
    nv12_bgl: wgpu::BindGroupLayout,
    nearest_sampler: wgpu::Sampler,
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_cache: HashMap<u64, CachedText>,
    actor_icon_cache: HashMap<String, CachedActorIconRef>,
    failed_actor_icon_cache: HashMap<String, FailedActorIconRef>,
    drawing_overlay: Option<DrawingOverlayTexture>,
    offscreen: Option<OffscreenTarget>,
    nv12: Option<Nv12Target>,
    // Pre-allocated GPU vertex buffers (reused across frames)
    quad_buf: wgpu::Buffer,
    quad_buf_cap: usize,
    icon_buf: wgpu::Buffer,
    icon_buf_cap: usize,
    quads: Vec<QuadInstance>,
    all_icons: Vec<IconInstance>,
    icon_batches: Vec<IconBatch>,
    stats: GpuRenderStats,
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
        let backends = export_backends();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        log::info!("GPU export backend preference: {:?}", backends);

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("No compatible GPU adapter for headless rendering: {e}"))?;

        let info = adapter.get_info();
        log::info!(
            "GPU export adapter: {} ({:?}, backend: {:?})",
            info.name,
            info.device_type,
            info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("GPU Export Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
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

        let nv12_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Export NV12 BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let nv12_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Export RGBA to NV12 Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui/rgba_to_nv12.wgsl").into()),
        });
        let nv12_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Export NV12 PL"),
            bind_group_layouts: &[Some(&nv12_bgl)],
            immediate_size: 0,
        });
        let nv12_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Export RGBA to NV12 Pipeline"),
            layout: Some(&nv12_pipeline_layout),
            module: &nv12_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
            nv12_pipeline,
            uniform_buffer,
            uniform_bind_group,
            uniform_bind_group_for_icons,
            texture_bgl,
            nv12_bgl,
            nearest_sampler,
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            text_cache: HashMap::new(),
            actor_icon_cache: HashMap::new(),
            failed_actor_icon_cache: HashMap::new(),
            drawing_overlay: None,
            offscreen: None,
            nv12: None,
            quad_buf,
            quad_buf_cap: INITIAL_QUAD_CAP,
            icon_buf,
            icon_buf_cap: INITIAL_ICON_CAP,
            quads: Vec::with_capacity(INITIAL_QUAD_CAP),
            all_icons: Vec::with_capacity(INITIAL_ICON_CAP),
            icon_batches: Vec::with_capacity(64),
            stats: GpuRenderStats::default(),
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

    fn prepare_drawing_overlay(
        &mut self,
        scene: &RythmoScene,
        current_frame: f64,
        width: u32,
        height: u32,
        ppf: f32,
    ) -> bool {
        let (first_frame, last_frame) =
            crate::rythmo_drawing::visible_frame_window(width as f32, current_frame, ppf, 4);
        let strokes: Vec<_> = scene
            .drawings
            .iter()
            .filter(|stroke| stroke.intersects_window(first_frame, last_frame))
            .collect();
        if strokes.is_empty() {
            return false;
        }

        let rgba =
            crate::rythmo_drawing::rasterize_window(&strokes, width, height, current_frame, ppf);
        let needs_create = self
            .drawing_overlay
            .as_ref()
            .is_none_or(|overlay| overlay.width != width || overlay.height != height);
        if needs_create {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Export Drawing Overlay"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Export Drawing Overlay BG"),
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
            self.drawing_overlay = Some(DrawingOverlayTexture {
                texture,
                bind_group,
                width,
                height,
            });
            self.stats.texture_creations += 1;
            self.stats.bind_groups_created += 1;
        }

        let overlay = self.drawing_overlay.as_ref().unwrap();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &overlay.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        true
    }

    // ── Text rasterization (CPU) ─────────────────────────────────────────

    fn rasterize_text(&mut self, text: &str, font_size: f32) -> (Vec<u8>, u32, u32) {
        crate::vector_text::prepare_font_system(&mut self.font_system);
        let line_height = (font_size * 1.4).ceil();
        let mut buffer =
            GlyphonBuffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
        buffer.set_size(&mut self.font_system, Some(10000.0), Some(line_height));
        let rythmo_family = crate::vector_text::rythmo_font_family_name();
        let family = if rythmo_family == "sans-serif" {
            Family::SansSerif
        } else {
            Family::Name(&rythmo_family)
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
                    let gx = physical.x;
                    let gy = line_y as i32 + physical.y;
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
        let hash = text_hash("glyphon", text, font_size, None, None);
        if self.text_cache.contains_key(&hash) {
            return hash;
        }

        let upload_start = Instant::now();
        let (pixels, w, h) = self.rasterize_text(text, font_size);
        if w == 0 || h == 0 {
            return hash;
        }
        if w > MAX_TEXT_TEXTURE_DIMENSION || h > MAX_TEXT_TEXTURE_DIMENSION {
            log::warn!(
                "Skipping oversized export text texture {}x{} for text '{}...'",
                w,
                h,
                text.chars().take(24).collect::<String>()
            );
            return hash;
        }

        self.stats.texture_creations += 1;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Export Text Tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
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
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.stats.bind_groups_created += 1;
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

        self.text_cache.insert(
            hash,
            CachedText {
                bind_group,
                width: w,
                height: h,
            },
        );

        self.stats.text_uploads += 1;
        self.stats.text_upload_time += upload_start.elapsed();

        hash
    }

    fn get_or_upload_voice_actor_icon(&mut self, actor: &VoiceActor) -> Option<u64> {
        let icon_data = actor.icon_png_base64.as_deref()?;
        let icon_ptr = icon_data.as_ptr() as usize;
        let icon_len = icon_data.len();
        if let Some(cached) = self.actor_icon_cache.get(&actor.name) {
            if cached.icon_ptr == icon_ptr
                && cached.icon_len == icon_len
                && self.text_cache.contains_key(&cached.hash)
            {
                return Some(cached.hash);
            }
        }
        if self
            .failed_actor_icon_cache
            .get(&actor.name)
            .is_some_and(|failed| failed.icon_ptr == icon_ptr && failed.icon_len == icon_len)
        {
            return None;
        }

        let hash = voice_actor_icon_texture_hash(icon_data);
        if self.text_cache.contains_key(&hash) {
            self.actor_icon_cache.insert(
                actor.name.clone(),
                CachedActorIconRef {
                    hash,
                    icon_ptr,
                    icon_len,
                },
            );
            return Some(hash);
        }
        if let Some(failed) = self.failed_actor_icon_cache.get_mut(&actor.name) {
            if failed.hash == hash {
                failed.icon_ptr = icon_ptr;
                failed.icon_len = icon_len;
                return None;
            }
        }

        let upload_start = Instant::now();
        let rgba = match decode_icon_rgba(icon_data) {
            Ok(rgba) => rgba,
            Err(e) => {
                log::warn!("Failed to decode voice actor icon '{}': {e}", actor.name);
                self.actor_icon_cache.remove(&actor.name);
                self.failed_actor_icon_cache.insert(
                    actor.name.clone(),
                    FailedActorIconRef {
                        hash,
                        icon_ptr,
                        icon_len,
                    },
                );
                return None;
            }
        };

        self.stats.texture_creations += 1;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Export Voice Actor Icon Tex"),
            size: wgpu::Extent3d {
                width: VOICE_ACTOR_ICON_SIZE,
                height: VOICE_ACTOR_ICON_SIZE,
                depth_or_array_layers: 1,
            },
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
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * VOICE_ACTOR_ICON_SIZE),
                rows_per_image: Some(VOICE_ACTOR_ICON_SIZE),
            },
            wgpu::Extent3d {
                width: VOICE_ACTOR_ICON_SIZE,
                height: VOICE_ACTOR_ICON_SIZE,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.stats.bind_groups_created += 1;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Export Voice Actor Icon BG"),
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

        self.text_cache.insert(
            hash,
            CachedText {
                bind_group,
                width: VOICE_ACTOR_ICON_SIZE,
                height: VOICE_ACTOR_ICON_SIZE,
            },
        );

        self.actor_icon_cache.insert(
            actor.name.clone(),
            CachedActorIconRef {
                hash,
                icon_ptr,
                icon_len,
            },
        );
        self.failed_actor_icon_cache.remove(&actor.name);

        self.stats.icon_uploads += 1;
        self.stats.icon_upload_time += upload_start.elapsed();

        Some(hash)
    }

    fn get_or_upload_rythmo_text_tile(
        &mut self,
        text: &str,
        font_size: f32,
        full_w: u32,
        dest_h: u32,
        tile_x: u32,
        tile_w: u32,
    ) -> u64 {
        self.get_or_upload_rythmo_text_tile_with_mode(
            text, font_size, full_w, dest_h, tile_x, tile_w, true,
        )
    }

    fn get_or_upload_rythmo_text_tile_natural(
        &mut self,
        text: &str,
        font_size: f32,
        full_w: u32,
        dest_h: u32,
        tile_x: u32,
        tile_w: u32,
    ) -> u64 {
        self.get_or_upload_rythmo_text_tile_with_mode(
            text, font_size, full_w, dest_h, tile_x, tile_w, false,
        )
    }

    fn get_or_upload_rythmo_text_tile_with_mode(
        &mut self,
        text: &str,
        font_size: f32,
        full_w: u32,
        dest_h: u32,
        tile_x: u32,
        tile_w: u32,
        stretch: bool,
    ) -> u64 {
        let tile_w = tile_w.min(full_w.saturating_sub(tile_x)).max(1);
        let kind = if stretch {
            "vector-rythmo-tile"
        } else {
            "vector-rythmo-tile-natural"
        };
        let hash = text_tile_hash(kind, text, font_size, full_w, dest_h, tile_x, tile_w);
        if self.text_cache.contains_key(&hash) {
            return hash;
        }

        if tile_w > MAX_TEXT_TEXTURE_DIMENSION || dest_h > MAX_TEXT_TEXTURE_DIMENSION {
            log::warn!(
                "Skipping oversized export rythmo text texture {}x{} for text '{}...'",
                tile_w,
                dest_h,
                text.chars().take(24).collect::<String>()
            );
            return hash;
        }

        let upload_start = Instant::now();
        let rendered = if stretch {
            crate::vector_text::render_rythmo_text_tile(
                &mut self.font_system,
                text,
                font_size,
                full_w,
                dest_h,
                tile_x,
                tile_w,
            )
        } else {
            crate::vector_text::render_rythmo_text_tile_natural(
                &mut self.font_system,
                text,
                font_size,
                full_w,
                dest_h,
                tile_x,
                tile_w,
            )
        };
        let Some(rendered) = rendered else {
            return hash;
        };
        if rendered.width == 0 || rendered.height == 0 {
            return hash;
        }

        self.stats.texture_creations += 1;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Export Vector Rythmo Text Tex"),
            size: wgpu::Extent3d {
                width: rendered.width,
                height: rendered.height,
                depth_or_array_layers: 1,
            },
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
            &rendered.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * rendered.width),
                rows_per_image: Some(rendered.height),
            },
            wgpu::Extent3d {
                width: rendered.width,
                height: rendered.height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.stats.bind_groups_created += 1;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Export Vector Rythmo Text BG"),
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

        self.text_cache.insert(
            hash,
            CachedText {
                bind_group,
                width: rendered.width,
                height: rendered.height,
            },
        );

        self.stats.text_uploads += 1;
        self.stats.text_upload_time += upload_start.elapsed();

        hash
    }

    fn push_rythmo_text_icons(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        self.push_rythmo_text_icons_tinted_clipped(
            text,
            font_size,
            x,
            y,
            w,
            h,
            [1.0, 1.0, 1.0, 1.0],
            1.0,
            all_icons,
            icon_batches,
        );
    }

    fn push_read_word_text_icons(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        segment_start: usize,
        highlight_end: Option<usize>,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        let count = text.chars().count();
        let Some(highlight_end) = highlight_end else {
            self.push_rythmo_text_icons(text, font_size, x, y, w, h, all_icons, icon_batches);
            return;
        };
        if count == 0 || highlight_end <= segment_start {
            self.push_rythmo_text_icons(text, font_size, x, y, w, h, all_icons, icon_batches);
            return;
        }
        let end_ratio = ((highlight_end - segment_start) as f32 / count as f32).min(1.0);
        if end_ratio < 1.0 {
            self.push_rythmo_text_icons(text, font_size, x, y, w, h, all_icons, icon_batches);
        }
        self.push_rythmo_text_icons_tinted_clipped(
            text,
            font_size,
            x,
            y,
            w,
            h,
            [1.0, 0.82, 0.08, 1.0],
            end_ratio,
            all_icons,
            icon_batches,
        );
    }

    fn push_rythmo_text_icons_tinted_clipped(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tint: [f32; 4],
        clip_ratio: f32,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        self.push_rythmo_text_icons_tinted_clipped_with_mode(
            text,
            font_size,
            x,
            y,
            w,
            h,
            tint,
            clip_ratio,
            true,
            all_icons,
            icon_batches,
        );
    }

    fn push_rythmo_text_icons_natural_tinted_clipped(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tint: [f32; 4],
        clip_ratio: f32,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        self.push_rythmo_text_icons_tinted_clipped_with_mode(
            text,
            font_size,
            x,
            y,
            w,
            h,
            tint,
            clip_ratio,
            false,
            all_icons,
            icon_batches,
        );
    }

    fn push_rythmo_text_icons_tinted_clipped_with_mode(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tint: [f32; 4],
        clip_ratio: f32,
        stretch: bool,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        let full_w = w.max(1.0).ceil() as u32;
        let full_h = h.max(1.0).ceil() as u32;
        let clip_px = (full_w as f32 * clip_ratio.clamp(0.0, 1.0)).ceil() as u32;
        if clip_px == 0 {
            return;
        }
        if full_h > MAX_TEXT_TEXTURE_DIMENSION {
            log::warn!(
                "Skipping export rythmo text taller than GPU texture limit: {}px",
                full_h
            );
            return;
        }

        let mut tile_x = 0;
        while tile_x < full_w {
            let tile_w = (full_w - tile_x).min(MAX_TEXT_TEXTURE_DIMENSION);
            if tile_x >= clip_px {
                break;
            }
            let visible_tile_w = tile_w.min(clip_px - tile_x).max(1);
            let hash = if stretch {
                self.get_or_upload_rythmo_text_tile(text, font_size, full_w, full_h, tile_x, tile_w)
            } else {
                self.get_or_upload_rythmo_text_tile_natural(
                    text, font_size, full_w, full_h, tile_x, tile_w,
                )
            };
            if self.text_cache.contains_key(&hash) {
                let draw_x = x + (tile_x as f32 / full_w as f32) * w;
                let draw_w = (visible_tile_w as f32 / full_w as f32) * w;
                let uv_end = (visible_tile_w as f32 / tile_w as f32).clamp(0.0, 1.0);
                let start = all_icons.len() as u32;
                all_icons.push(IconInstance {
                    rect: [draw_x, y, draw_w, h],
                    uv_rect: [0.0, 0.0, uv_end, 1.0],
                    tint,
                });
                icon_batches.push(IconBatch {
                    hash,
                    start,
                    count: 1,
                });
            }
            tile_x += tile_w;
        }
    }

    fn karaoke_text_width(&mut self, text: &str, font_size: f32, karaoke_text_scale: f32) -> f32 {
        let font_size = font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
        crate::vector_text::measure_rythmo_text_width(&mut self.font_system, text, font_size)
            .map(|width| width.ceil() + 1.0)
            .unwrap_or_else(|| {
                let char_count = text.chars().count().max(1) as f32;
                char_count * font_size * 0.62 + font_size * 0.7
            })
            .max(2.0)
    }

    fn push_actor_fallback_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        let font_size = (size * 0.55).max(1.0);
        let hash = self.get_or_upload_text(text, font_size);
        let Some(cached) = self.text_cache.get(&hash) else {
            return;
        };
        let tw = cached.width as f32;
        let th = cached.height as f32;
        if tw <= 0.0 || th <= 0.0 {
            return;
        }

        let draw_w = tw.min(size);
        let draw_h = th.min(size);
        let start = all_icons.len() as u32;
        all_icons.push(IconInstance {
            rect: [
                x + (size - draw_w) * 0.5,
                y + (size - draw_h) * 0.5,
                draw_w,
                draw_h,
            ],
            uv_rect: [0.0, 0.0, draw_w / tw, draw_h / th],
            tint: [230.0 / 255.0, 230.0 / 255.0, 238.0 / 255.0, 1.0],
        });
        icon_batches.push(IconBatch {
            hash,
            start,
            count: 1,
        });
    }

    fn push_voice_actor_icons(
        &mut self,
        scene: &GpuExportScene<'_>,
        line: &RythmoLine,
        x: f32,
        y: f32,
        _badge_w: f32,
        icon_size: f32,
        scale: f32,
        surface_w: f32,
        quads: &mut Vec<QuadInstance>,
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        if line.karaoke || line.voice_actor_names.is_empty() {
            return;
        }

        let icon_size = icon_size.max(1.0);
        let gap = 3.0 * scale;
        // The badge ends immediately before the line body. Keep actor icons
        // on the outer side of the badge so they cannot cover the line text.
        let mut icon_x = x - gap - icon_size;

        for actor_name in &line.voice_actor_names {
            if icon_x > surface_w {
                break;
            }

            quads.push(quad(
                icon_x,
                y,
                icon_size,
                icon_size,
                10.0 / 255.0,
                10.0 / 255.0,
                14.0 / 255.0,
                235.0 / 255.0,
            ));

            let mut drew_icon = false;
            if let Some(actor) = scene.voice_actor(actor_name) {
                if let Some(hash) = self.get_or_upload_voice_actor_icon(actor) {
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [icon_x, y, icon_size, icon_size],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [1.0; 4],
                    });
                    icon_batches.push(IconBatch {
                        hash,
                        start,
                        count: 1,
                    });
                    drew_icon = true;
                }

                if !drew_icon {
                    self.push_actor_fallback_text(
                        &actor.name,
                        icon_x,
                        y,
                        icon_size,
                        all_icons,
                        icon_batches,
                    );
                }
            } else {
                self.push_actor_fallback_text(
                    actor_name,
                    icon_x,
                    y,
                    icon_size,
                    all_icons,
                    icon_batches,
                );
            }

            icon_x -= icon_size + gap;
        }
    }

    // ── Offscreen management ─────────────────────────────────────────────

    fn ensure_offscreen(&mut self, width: u32, height: u32) {
        let needs_create = match &self.offscreen {
            Some(o) => o.width != width || o.height != height,
            None => true,
        };
        if needs_create {
            self.offscreen = Some(OffscreenTarget::new(&self.device, width, height));
            self.nv12 = None;
        }
    }

    fn ensure_nv12(&mut self, width: u32, height: u32, padded_height: u32) {
        let needs_create = match &self.nv12 {
            Some(target) => {
                target.width != width
                    || target.height != height
                    || target.padded_height != padded_height
            }
            None => true,
        };
        if needs_create {
            let offscreen = self.offscreen.as_ref().unwrap();
            self.nv12 = Some(Nv12Target::new(
                &self.device,
                &self.nv12_bgl,
                &offscreen.view,
                width,
                height,
                padded_height,
            ));
        }

        let target = self.nv12.as_ref().unwrap();
        let params = Nv12Params {
            width,
            height,
            padded_height,
            total_bytes: target.frame_size as u32,
        };
        self.queue
            .write_buffer(&target.params_buffer, 0, bytemuck::bytes_of(&params));
    }

    fn coalesce_icon_batches(icon_batches: &mut Vec<IconBatch>) {
        if icon_batches.len() < 2 {
            return;
        }

        let mut write_idx = 0;
        for read_idx in 1..icon_batches.len() {
            let current = IconBatch {
                hash: icon_batches[read_idx].hash,
                start: icon_batches[read_idx].start,
                count: icon_batches[read_idx].count,
            };
            let previous = &mut icon_batches[write_idx];
            if previous.hash == current.hash && previous.start + previous.count == current.start {
                previous.count += current.count;
            } else {
                write_idx += 1;
                icon_batches[write_idx] = current;
            }
        }
        icon_batches.truncate(write_idx + 1);
    }

    pub fn stats(&self) -> GpuRenderStats {
        self.stats.clone()
    }

    /// Submit a frame for GPU rendering (non-blocking).
    /// Call `finish_render_into` to get RGBA pixels.
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
            ReadbackMode::Rgba,
        );
    }

    /// Submit a frame and convert it to NV12 on the GPU before readback.
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
            ReadbackMode::Nv12 { padded_height },
        );
    }

    fn submit_render_inner(
        &mut self,
        scene: &GpuExportScene<'_>,
        current_frame: f64,
        width: u32,
        _fps: f64,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        readback: ReadbackMode,
    ) {
        // Build quads + icons using the same logic as render_br
        let s = width as f32 / constants::REF_WIDTH * br_scale;
        let normal_slot_h = constants::SLOT_HEIGHT * s;
        let ruler_h = constants::RULER_HEIGHT * s;
        let ppf = constants::PIXELS_PER_FRAME * s * crate::config::scroll_speed();
        let tick_long = constants::TICK_LONG * s;
        let tick_short = constants::TICK_SHORT * s;
        let tick_w = BASE_TICK_WIDTH * s;
        let playhead_w = BASE_PLAYHEAD_WIDTH * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let badge_gap = constants::BADGE_GAP * s;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
        let slot_header_h = badge_h.max(actor_icon_size);
        let font_size = constants::RYTHMO_FONT_SIZE * s;
        let badge_font = constants::BADGE_FONT_SIZE * s;
        let visible_frames = (width as f32 / ppf) as i64 + 4;
        let render_margin_frames = ((source_fps.max(1.0) * 10.0).round() as i64)
            .max(karaoke_adjacent_max_gap_frames(source_fps))
            .max(karaoke_count_in_frames(source_fps))
            .saturating_add(scene.render_index.max_duration_frames());
        let common_scene = RythmoScene::build(
            scene.project,
            &scene.render_index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: (current_frame.floor() as i64)
                        .saturating_sub(visible_frames / 2)
                        .saturating_sub(render_margin_frames),
                    last: (current_frame.ceil() as i64)
                        .saturating_add(visible_frames / 2)
                        .saturating_add(render_margin_frames),
                },
                current_frame,
                source_fps,
                normal_body_height: normal_slot_h,
                slot_header_height: slot_header_h,
                badge_gap,
                scale: s,
            },
        );
        let track_layouts = &common_scene.tracks;
        let height = (ruler_h + rythmo_layout::total_tracks_height(track_layouts)).ceil() as u32;

        self.ensure_offscreen(width, height);

        let w = width as f32;
        let h = height as f32;
        let center_x = w / 2.0;

        let mut quads = std::mem::take(&mut self.quads);
        let mut all_icons = std::mem::take(&mut self.all_icons);
        let mut icon_batches = std::mem::take(&mut self.icon_batches);
        quads.clear();
        all_icons.clear();
        icon_batches.clear();

        // ── Ruler ticks ──
        let cf_i64 = if current_frame.is_finite() {
            current_frame.floor() as i64
        } else {
            0
        };
        let first_tick_frame = cf_i64 - visible_frames / 2;
        let first_tick =
            first_tick_frame.div_euclid(constants::TICK_GAP_FRAMES) * constants::TICK_GAP_FRAMES;
        let mut tf = first_tick;
        loop {
            let x = center_x + (tf as f64 - current_frame) as f32 * ppf;
            if x > w {
                break;
            }
            if x >= 0.0 {
                let tick_idx = tf.div_euclid(constants::TICK_GAP_FRAMES);
                let th = if tick_idx % 2 == 0 {
                    tick_long
                } else {
                    tick_short
                };
                quads.push(quad(
                    x,
                    0.0,
                    tick_w,
                    th,
                    100.0 / 255.0,
                    100.0 / 255.0,
                    115.0 / 255.0,
                    128.0 / 255.0,
                ));
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // ── Playhead, split around active karaoke lines ──
        let playhead_gaps =
            common_scene.active_karaoke_skip_ranges(ruler_h, slot_header_h, badge_gap, s);
        push_playhead_segments(
            &mut quads,
            center_x - playhead_w / 2.0,
            playhead_w,
            h,
            &playhead_gaps,
        );

        // ── Lines ──
        // Precompute every visible line's rect + character name so a badge can be tested
        // against OTHER lines (same char → hide, different char → 60% opacity).
        let mut compute_line_rect = |scene_line: &SceneLine| -> Option<Rect> {
            let line = &scene_line.line;
            if line.karaoke && !scene_line.karaoke_should_be_visible() {
                return None;
            }
            let (x1, lw) = if scene_line.karaoke_should_be_centered() {
                let width = self.karaoke_text_width(&line.text, font_size, karaoke_text_scale);
                (center_x - width / 2.0, width)
            } else {
                line.visual_x_width(current_frame, center_x, ppf, w, s)
            };
            let badge_w = rythmo_layout::scaled_character_badge_width(&line.character_name, s);
            let badge_x = rythmo_layout::leading_character_badge_x(x1, badge_w, s);
            let show_badge = !line.karaoke || scene_line.character_label_visible;
            let leading_visual = show_badge.then(|| {
                rythmo_layout::leading_visual_bounds(
                    badge_x,
                    badge_w,
                    (!line.karaoke)
                        .then_some(line.voice_actor_names.len())
                        .unwrap_or(0),
                    actor_icon_size,
                    3.0 * s,
                )
            });
            if !rythmo_layout::line_or_badge_intersects_viewport(x1, lw, leading_visual, 0.0, w) {
                return None;
            }
            let track = rythmo_layout::track_for_y_slot(track_layouts, line.y_slot)?;
            let y_base = ruler_h + track.top;
            let body_y = y_base + slot_header_h + badge_gap;
            let mut line_y = body_y;
            let mut body_h = normal_slot_h;
            if line.karaoke {
                line_y = karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, s);
                body_h = karaoke_stack_height(track.body_h, s);
            }
            Some(Rect {
                x: x1,
                y: line_y,
                width: lw,
                height: body_h,
            })
        };
        let mut line_rects: HashMap<u64, (Rect, String)> = HashMap::new();
        for scene_line in &common_scene.lines {
            if let Some(r) = compute_line_rect(scene_line) {
                let line = &scene_line.line;
                line_rects.insert(line.id, (r, line.character_name.clone()));
            }
        }
        for scene_line in &common_scene.lines {
            let line = &scene_line.line;
            let karaoke_count_in = scene_line.karaoke_count_in_progress.is_some();
            if line.karaoke && !scene_line.karaoke_should_be_visible() {
                continue;
            }

            let (x1, lw) = if scene_line.karaoke_should_be_centered() {
                let width = self.karaoke_text_width(&line.text, font_size, karaoke_text_scale);
                (center_x - width / 2.0, width)
            } else {
                line.visual_x_width(current_frame, center_x, ppf, w, s)
            };
            let badge_w = rythmo_layout::scaled_character_badge_width(&line.character_name, s);
            let badge_x = rythmo_layout::leading_character_badge_x(x1, badge_w, s);
            let show_badge = !line.karaoke || scene_line.character_label_visible;
            let leading_visual = show_badge.then(|| {
                rythmo_layout::leading_visual_bounds(
                    badge_x,
                    badge_w,
                    (!line.karaoke)
                        .then_some(line.voice_actor_names.len())
                        .unwrap_or(0),
                    actor_icon_size,
                    3.0 * s,
                )
            });
            if !rythmo_layout::line_or_badge_intersects_viewport(x1, lw, leading_visual, 0.0, w) {
                continue;
            }

            let Some(track) = rythmo_layout::track_for_y_slot(track_layouts, line.y_slot) else {
                continue;
            };
            let y_base = ruler_h + track.top;
            let body_y = y_base + slot_header_h + badge_gap;
            let mut line_y = body_y;
            let mut body_h = normal_slot_h;
            if line.karaoke {
                line_y = karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, s);
                body_h = karaoke_stack_height(track.body_h, s);
            }

            let [cr, cg, cb, _] = line.character_color;
            let badge_h = body_h * constants::BADGE_OVERLAP_HEIGHT_RATIO;
            // Rectangular, top-aligned, right edge a few px left of the line's left edge.
            let badge_y = line_y;

            // Overlap detection vs OTHER lines: hide if same character, 60% opacity if different
            let mut badge_hidden = false;
            let mut badge_overlap_alpha = 1.0_f32;
            for (&oid, (other_rect, other_name)) in &line_rects {
                if oid == line.id {
                    continue;
                }
                let overlap = badge_x < other_rect.x + other_rect.width
                    && badge_x + badge_w > other_rect.x
                    && badge_y < other_rect.y + other_rect.height
                    && badge_y + badge_h > other_rect.y;
                if overlap {
                    if other_name == &line.character_name {
                        badge_hidden = true;
                        break;
                    } else {
                        badge_overlap_alpha = constants::CHARACTER_BADGE_COLLISION_OPACITY;
                    }
                }
            }

            // Store badge info for later drawing (after text)
            let badge_info = if show_badge && !badge_hidden && !line.character_name.is_empty() {
                let luminance = 0.299 * cr + 0.587 * cg + 0.114 * cb;
                let (tr, tg, tb) = if luminance > 0.55 {
                    (0.0_f32, 0.0, 0.0)
                } else {
                    (224.0 / 255.0, 224.0 / 255.0, 230.0 / 255.0)
                };
                let hash = self.get_or_upload_text(&line.character_name, badge_font);
                Some((
                    badge_x,
                    badge_y,
                    badge_w,
                    badge_h,
                    cr,
                    cg,
                    cb,
                    badge_overlap_alpha,
                    hash,
                    tr,
                    tg,
                    tb,
                ))
            } else {
                None
            };

            if !line.text.is_empty() && line.text != "\u{2191}" && line.text != "\u{2193}" {
                let read_highlight_end =
                    if scene.project.settings().highlight_read_word && !line.karaoke {
                        let progress = (current_frame - line.start_frame as f64)
                            / line.duration_frames.max(1) as f64;
                        crate::syllable::read_highlight_end_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            &crate::config::get().lang,
                            progress as f32,
                        )
                    } else {
                        None
                    };
                if line.karaoke {
                    let karaoke_font_size =
                        font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
                    self.push_rythmo_text_icons_natural_tinted_clipped(
                        &line.text,
                        karaoke_font_size,
                        x1,
                        line_y,
                        lw,
                        body_h,
                        [1.0, 1.0, 1.0, 1.0],
                        1.0,
                        &mut all_icons,
                        &mut icon_batches,
                    );
                    if let Some(progress) = scene_line.karaoke_progress {
                        let visual_progress = crate::syllable::visual_progress_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            &crate::config::get().lang,
                            progress,
                        );
                        self.push_rythmo_text_icons_natural_tinted_clipped(
                            &line.text,
                            karaoke_font_size,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            [
                                line.character_color[0].clamp(0.0, 1.0),
                                line.character_color[1].clamp(0.0, 1.0),
                                line.character_color[2].clamp(0.0, 1.0),
                                1.0,
                            ],
                            visual_progress,
                            &mut all_icons,
                            &mut icon_batches,
                        );
                    }
                } else {
                    let lang = &crate::config::get().lang;
                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                    let ratios =
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);

                    if !ratios.is_empty() {
                        let chars: Vec<char> = line.text.chars().collect();
                        let mut seg_x = x1;
                        let mut prev_break = 0usize;
                        for (i, &ratio) in ratios.iter().enumerate() {
                            let seg_w = ratio * lw;
                            let end_break = if i < breaks.len() {
                                breaks[i]
                            } else {
                                chars.len()
                            };
                            let segment: String = chars[prev_break..end_break].iter().collect();
                            if !segment.is_empty() && seg_w > 0.5 {
                                self.push_read_word_text_icons(
                                    &segment,
                                    font_size,
                                    seg_x,
                                    line_y,
                                    seg_w,
                                    body_h,
                                    prev_break,
                                    read_highlight_end,
                                    &mut all_icons,
                                    &mut icon_batches,
                                );
                            }
                            seg_x += seg_w;
                            prev_break = end_break;
                        }
                    } else {
                        self.push_read_word_text_icons(
                            &line.text,
                            font_size,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            0,
                            read_highlight_end,
                            &mut all_icons,
                            &mut icon_batches,
                        );
                    }
                }
            }

            // Draw badge AFTER text so it appears on top
            if let Some((badge_x, badge_y, badge_w, badge_h, cr, cg, cb, ba, hash, tr, tg, tb)) =
                badge_info
            {
                let badge_radius = 0.0; // rectangular badge (no rounding)
                quads.push(quad_rounded(
                    badge_x,
                    badge_y,
                    badge_w,
                    badge_h,
                    cr,
                    cg,
                    cb,
                    ba,
                    badge_radius,
                ));

                if let Some(cached) = self.text_cache.get(&hash) {
                    let tw = cached.width as f32;
                    let th = cached.height as f32;
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [
                            badge_x + (badge_w - tw) / 2.0,
                            badge_y + (badge_h - th) / 2.0,
                            tw,
                            th,
                        ],
                        uv_rect: [0.0, 0.0, 1.0, 1.0],
                        tint: [tr, tg, tb, ba],
                    });
                    icon_batches.push(IconBatch {
                        hash,
                        start,
                        count: 1,
                    });
                }

                self.push_voice_actor_icons(
                    scene,
                    line,
                    badge_x,
                    badge_y,
                    badge_w,
                    actor_icon_size,
                    s,
                    w,
                    &mut quads,
                    &mut all_icons,
                    &mut icon_batches,
                );
            }

            if line.text == "\u{2191}" || line.text == "\u{2193}" {
                let up = line.text == "\u{2191}";
                let margin = 4.0;
                if lw > margin * 2.0 + 1.0 && body_h > margin * 2.0 + 1.0 {
                    let dx = lw - margin * 2.0;
                    let dy = body_h - margin * 2.0;
                    let length = (dx * dx + dy * dy).sqrt();
                    let cx = x1 + lw / 2.0;
                    let cy = line_y + body_h / 2.0;
                    let angle = if up { (-dy).atan2(dx) } else { dy.atan2(dx) };
                    quads.push(rotated_line(
                        cx,
                        cy,
                        length,
                        2.0 * s,
                        angle,
                        220.0 / 255.0,
                        220.0 / 255.0,
                        230.0 / 255.0,
                        230.0 / 255.0,
                    ));
                }
            }

            if karaoke_count_in {
                push_karaoke_count_in_dot(
                    &mut quads,
                    line,
                    x1,
                    line_y,
                    scene_line.karaoke_count_in_progress,
                    s,
                );
            } else {
                push_karaoke_dot(&mut quads, line, current_frame, x1, line_y, lw, s);
            }

            // Note text (discrete, gray, at the bottom of the line)
            if !line.note.is_empty() {
                let note_font = badge_font * 0.9;
                let hash = self.get_or_upload_text(&line.note, note_font);
                if let Some(cached) = self.text_cache.get(&hash) {
                    let tw = cached.width as f32;
                    let _th = cached.height as f32;
                    let note_h = (note_font * 1.3).ceil();
                    let note_y = line_y + body_h - note_h - 1.0;
                    let max_note_w = lw - 8.0 * s;
                    let draw_w = tw.min(max_note_w);
                    let uv_end = (draw_w / tw).min(1.0);
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [x1 + 4.0 * s, note_y, draw_w, note_h],
                        uv_rect: [0.0, 0.0, uv_end, 1.0],
                        tint: [160.0 / 255.0, 160.0 / 255.0, 170.0 / 255.0, 1.0],
                    });
                    icon_batches.push(IconBatch {
                        hash,
                        start,
                        count: 1,
                    });
                }
            }
        }

        // ── Markers ──
        let marker_margin_frames = (10.0 * s / ppf).ceil() as i64 + 1;
        let first_marker_frame =
            cf_i64.saturating_sub((w / ppf / 2.0).ceil() as i64 + marker_margin_frames);
        let last_marker_frame =
            cf_i64.saturating_add((w / ppf / 2.0).ceil() as i64 + marker_margin_frames);
        for marker in &common_scene.markers {
            if marker.frame < first_marker_frame || marker.frame > last_marker_frame {
                continue;
            }
            let mx = center_x + (marker.frame as f64 - current_frame) as f32 * ppf;
            if mx < -10.0 * s || mx > w + 10.0 * s {
                continue;
            }
            match &marker.kind {
                MarkerKind::Boucle => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        217.0 / 255.0,
                        38.0 / 255.0,
                        38.0 / 255.0,
                        230.0 / 255.0,
                    ));
                    let cy = h / 2.0;
                    let arm = 10.0 * s;
                    let diag_len = arm * 2.0 * std::f32::consts::SQRT_2;
                    quads.push(rotated_line(
                        mx,
                        cy,
                        diag_len,
                        2.5 * s,
                        std::f32::consts::FRAC_PI_4,
                        217.0 / 255.0,
                        38.0 / 255.0,
                        38.0 / 255.0,
                        230.0 / 255.0,
                    ));
                    quads.push(rotated_line(
                        mx,
                        cy,
                        diag_len,
                        2.5 * s,
                        -std::f32::consts::FRAC_PI_4,
                        217.0 / 255.0,
                        38.0 / 255.0,
                        38.0 / 255.0,
                        230.0 / 255.0,
                    ));
                }
                MarkerKind::Out => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        217.0 / 255.0,
                        115.0 / 255.0,
                        115.0 / 255.0,
                        180.0 / 255.0,
                    ));
                    let cy = h / 2.0;
                    let bh = h * 0.15;
                    for &offset in &[-5.0_f32, 5.0] {
                        let dx = bh * 0.3;
                        let length = (dx * 2.0_f32).hypot(bh * 2.0);
                        let angle = (bh * 2.0).atan2(dx * 2.0);
                        quads.push(rotated_line(
                            mx + offset * s,
                            cy,
                            length,
                            2.0 * s,
                            angle,
                            217.0 / 255.0,
                            115.0 / 255.0,
                            115.0 / 255.0,
                            180.0 / 255.0,
                        ));
                    }
                }
                MarkerKind::SceneChange => {
                    quads.push(quad(
                        mx - 1.0 * s,
                        0.0,
                        2.0 * s,
                        h,
                        230.0 / 255.0,
                        230.0 / 255.0,
                        240.0 / 255.0,
                        200.0 / 255.0,
                    ));
                }
                MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => {
                    let is_left = matches!(marker.kind, MarkerKind::LiaisonLeft);
                    let ay = ruler_h / 2.0;
                    let arm_x = if is_left { -3.0 } else { 3.0 } * s;
                    let arm_y = 4.0 * s;
                    let tip_x = mx + arm_x;
                    for &dy in &[-arm_y, arm_y] {
                        let sx = mx - arm_x;
                        let length = ((tip_x - sx).powi(2) + dy.powi(2)).sqrt();
                        let angle = dy.atan2(tip_x - sx);
                        quads.push(rotated_line(
                            (sx + tip_x) / 2.0,
                            ay + dy / 2.0,
                            length,
                            1.5 * s,
                            angle,
                            180.0 / 255.0,
                            180.0 / 255.0,
                            190.0 / 255.0,
                            200.0 / 255.0,
                        ));
                    }
                }
            }
        }

        // Match the editor's layer order: drawings cover the rendered BR and
        // are themselves free of editing handles or selection UI.
        let drawing_icon_index =
            if self.prepare_drawing_overlay(&common_scene, current_frame, width, height, ppf) {
                let index = all_icons.len() as u32;
                all_icons.push(IconInstance {
                    rect: [0.0, 0.0, width as f32, height as f32],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
                Some(index)
            } else {
                None
            };

        Self::coalesce_icon_batches(&mut icon_batches);

        // ── Submit GPU work (non-blocking) ──
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32]),
        );
        self.ensure_quad_buf(quads.len());
        self.ensure_icon_buf(all_icons.len().max(1));
        if !quads.is_empty() {
            self.queue
                .write_buffer(&self.quad_buf, 0, bytemuck::cast_slice(&quads));
        }
        if !all_icons.is_empty() {
            self.queue
                .write_buffer(&self.icon_buf, 0, bytemuck::cast_slice(&all_icons));
        }
        if let ReadbackMode::Nv12 { padded_height } = readback {
            self.ensure_nv12(width, height, padded_height);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Export Encoder"),
            });

        {
            let offscreen = self.offscreen.as_ref().unwrap();
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Export Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &offscreen.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 5.0 / 255.0,
                                g: 5.0 / 255.0,
                                b: 8.0 / 255.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
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
                    if let (Some(index), Some(overlay)) =
                        (drawing_icon_index, self.drawing_overlay.as_ref())
                    {
                        pass.set_bind_group(1, &overlay.bind_group, &[]);
                        pass.draw(0..6, index..index + 1);
                    }
                }
            }
            match readback {
                ReadbackMode::Rgba => {
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture: &offscreen.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: offscreen.current_buf(),
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(offscreen.padded_row_bytes),
                                rows_per_image: Some(height),
                            },
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                ReadbackMode::Nv12 { .. } => {
                    let nv12 = self.nv12.as_ref().unwrap();
                    {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("Export NV12 Compute Pass"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.nv12_pipeline);
                        pass.set_bind_group(0, &nv12.bind_group, &[]);
                        pass.dispatch_workgroups(nv12.word_count().div_ceil(256), 1, 1);
                    }
                    encoder.copy_buffer_to_buffer(
                        &nv12.storage,
                        0,
                        nv12.current_buf(),
                        0,
                        nv12.buffer_size,
                    );
                }
            }
        }

        let quad_draws = u64::from(!quads.is_empty());
        let icon_draws = icon_batches
            .iter()
            .filter(|batch| self.text_cache.contains_key(&batch.hash))
            .count() as u64
            + u64::from(drawing_icon_index.is_some());
        let frame_draws = quad_draws + icon_draws;
        self.stats.frames_submitted += 1;
        self.stats.draw_calls += frame_draws;
        self.stats.last_frame_quads = quads.len();
        self.stats.last_frame_icons = all_icons.len();
        self.stats.last_frame_icon_batches = icon_batches.len();
        self.stats.last_frame_draw_calls = frame_draws;

        self.queue.submit(std::iter::once(encoder.finish()));
        self.quads = quads;
        self.all_icons = all_icons;
        self.icon_batches = icon_batches;
        // GPU is now working — caller can do I/O in parallel before calling finish_render
    }

    /// Wait for a previously submitted RGBA render and copy pixels into `out`.
    /// Caller must have called `submit_render` first.
    pub fn finish_render_into(&mut self, width: u32, height: u32, out: &mut Vec<u8>) {
        let offscreen = self.offscreen.as_mut().unwrap();
        let buf = offscreen.current_buf();
        let buffer_slice = buf.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        let data = buffer_slice.get_mapped_range();
        let unpadded_row = (width * 4) as usize;
        let padded_row = offscreen.padded_row_bytes as usize;
        let total = unpadded_row * height as usize;

        out.clear();
        out.reserve(total);
        if padded_row == unpadded_row {
            out.extend_from_slice(&data[..total]);
        } else {
            for row in 0..height as usize {
                let start = row * padded_row;
                out.extend_from_slice(&data[start..start + unpadded_row]);
            }
        }
        drop(data);
        buf.unmap();
        offscreen.flip();

        self.stats.last_readback_bytes = total;
        self.stats.total_readback_bytes += total as u64;
    }

    /// Wait for a previously submitted NV12 render and copy the exact ffmpeg frame into `out`.
    /// Caller must have called `submit_render_nv12` first.
    pub fn finish_render_nv12_into(&mut self, out: &mut Vec<u8>) {
        let nv12 = self.nv12.as_mut().unwrap();
        let frame_size = nv12.frame_size;
        let buf = nv12.current_buf();
        let buffer_slice = buf.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        let data = buffer_slice.get_mapped_range();
        out.clear();
        out.reserve(frame_size);
        out.extend_from_slice(&data[..frame_size]);
        drop(data);
        buf.unmap();
        nv12.flip();

        self.stats.last_readback_bytes = frame_size;
        self.stats.total_readback_bytes += frame_size as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_export_karaoke_island_after_normal_line_continues_alternating_rows() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.25);
        let first_karaoke_id = project.add_line(24 * 2, 24, 0.25);
        let second_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(first_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(second_karaoke_id).unwrap().karaoke = true;

        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);
        let scene = RythmoScene::build(
            &project,
            &index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: 0,
                    last: 120,
                },
                current_frame: 48.0,
                source_fps: 24.0,
                ..SceneOptions::default()
            },
        );
        assert_eq!(
            scene
                .lines
                .iter()
                .find(|line| line.line.id == first_karaoke_id)
                .unwrap()
                .karaoke_stack_row,
            1
        );
        assert_eq!(
            scene
                .lines
                .iter()
                .find(|line| line.line.id == second_karaoke_id)
                .unwrap()
                .karaoke_stack_row,
            0
        );
    }
}
