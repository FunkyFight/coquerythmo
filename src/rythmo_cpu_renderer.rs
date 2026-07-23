//! CPU renderer for the shared rythmo scene.
//!
//! Renderer entry points deliberately receive the complete render context so
//! CPU and GPU backends remain behaviorally interchangeable.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

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
use crate::ui::primitives::Rect;
use crate::voice_actor::{decode_icon_rgba, icon_hash, VoiceActor, VOICE_ACTOR_ICON_SIZE};
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};
use resvg::tiny_skia::{self, Pixmap};

// Local constants not shared with the UI
const BASE_TICK_WIDTH: f32 = 1.5;
const BASE_PLAYHEAD_WIDTH: f32 = 3.0;
const MAX_RYTHMO_TEXT_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RYTHMO_TEXT_CACHE_ENTRIES: usize = 512;

fn blit_playhead_segments(
    pixmap: &mut Pixmap,
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
            blit_rect(pixmap, x, y, width, skip_start - y, [255, 5, 13, 255]);
        }
        y = y.max(skip_end);
    }
    if y < height {
        blit_rect(pixmap, x, y, width, height - y, [255, 5, 13, 255]);
    }
}

struct CachedCpuRythmoText {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    bytes: usize,
    last_used: u64,
}

/// Persistent state for CPU text rasterization (reused across frames).
pub struct CpuRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    render_index: ProjectRenderIndex,
    rythmo_text_cache: HashMap<u64, CachedCpuRythmoText>,
    voice_actor_icon_cache: HashMap<u64, Vec<u8>>,
    rythmo_text_cache_bytes: usize,
    cache_tick: u64,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuRenderer {
    include!("rythmo_cpu_renderer_parts/impl_01.rs");
    include!("rythmo_cpu_renderer_parts/impl_02.rs");
    include!("rythmo_cpu_renderer_parts/impl_03.rs");
}
include!("rythmo_cpu_renderer_parts/tail_01.rs");
include!("rythmo_cpu_renderer_parts/tail_02.rs");
