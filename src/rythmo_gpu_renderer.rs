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
use crate::rendering::rythmo::geometry::HorizontalRythmoGeometry;
use crate::rendering::rythmo::labels::{
    ambiance_character_label_x, centered_karaoke_character_label_x, character_label_metrics,
    character_label_rects, normal_character_label_x, PreparedLineGeometry,
};
use crate::rendering::rythmo::scene::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, RythmoScene, SceneOptions,
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
    lang: &str,
    current_frame: f64,
    x: f32,
    y: f32,
    width: f32,
    scale: f32,
) {
    let Some(progress) = line.karaoke_progress(current_frame) else {
        return;
    };
    let ratios = crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
    let local_progress = crate::syllable::active_syllable_local_progress(&ratios, progress)
        .unwrap_or(progress)
        .clamp(0.0, 1.0);
    let visual_progress = crate::syllable::visual_progress_from_timing(
        &line.text,
        &line.syllable_ratios,
        lang,
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
const BASE_PLAYHEAD_WIDTH: f32 = 3.0;

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
    include!("rythmo_gpu_renderer_parts/impl_01.rs");
    include!("rythmo_gpu_renderer_parts/impl_02.rs");
    include!("rythmo_gpu_renderer_parts/impl_03.rs");
    include!("rythmo_gpu_renderer_parts/impl_04.rs");
    include!("rythmo_gpu_renderer_parts/impl_05.rs");
    include!("rythmo_gpu_renderer_parts/impl_06.rs");
}
include!("rythmo_gpu_renderer_parts/tail_01.rs");
