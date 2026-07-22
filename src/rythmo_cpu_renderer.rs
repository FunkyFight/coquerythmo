//! CPU renderer for the shared rythmo scene.
//!
//! Renderer entry points deliberately receive the complete render context so
//! CPU and GPU backends remain behaviorally interchangeable.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use crate::constants;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rendering::rythmo::scene::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, RythmoScene, SceneLine, SceneOptions,
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
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            render_index: ProjectRenderIndex::new(),
            rythmo_text_cache: HashMap::new(),
            voice_actor_icon_cache: HashMap::new(),
            rythmo_text_cache_bytes: 0,
            cache_tick: 0,
        }
    }

    fn rythmo_text_cache_key(
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
        stretch: bool,
        emphasized: bool,
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        font_size.to_bits().hash(&mut h);
        dest_w.hash(&mut h);
        dest_h.hash(&mut h);
        stretch.hash(&mut h);
        emphasized.hash(&mut h);
        crate::vector_text::rythmo_font_family_name().hash(&mut h);
        h.finish()
    }

    fn get_or_render_rythmo_text(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<u64> {
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, true, false)
    }

    fn get_or_render_rythmo_text_natural(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<u64> {
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, false, false)
    }

    fn get_or_render_rythmo_text_natural_emphasized(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<u64> {
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, false, true)
    }

    fn get_or_render_rythmo_text_with_mode(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
        stretch: bool,
        emphasized: bool,
    ) -> Option<u64> {
        self.cache_tick = self.cache_tick.wrapping_add(1);
        let key = Self::rythmo_text_cache_key(text, font_size, dest_w, dest_h, stretch, emphasized);
        if let Some(cached) = self.rythmo_text_cache.get_mut(&key) {
            cached.last_used = self.cache_tick;
            return Some(key);
        }

        let rendered = if emphasized {
            crate::vector_text::render_rythmo_text_natural_emphasized(
                &mut self.font_system,
                text,
                font_size,
                dest_w,
                dest_h,
            )?
        } else if stretch {
            crate::vector_text::render_rythmo_text(
                &mut self.font_system,
                text,
                font_size,
                dest_w,
                dest_h,
            )?
        } else {
            crate::vector_text::render_rythmo_text_natural(
                &mut self.font_system,
                text,
                font_size,
                dest_w,
                dest_h,
            )?
        };
        let bytes = rendered.pixels.len();
        self.rythmo_text_cache_bytes += bytes;
        self.rythmo_text_cache.insert(
            key,
            CachedCpuRythmoText {
                pixels: rendered.pixels,
                width: rendered.width,
                height: rendered.height,
                bytes,
                last_used: self.cache_tick,
            },
        );
        self.evict_rythmo_text_cache();

        Some(key)
    }

    fn evict_rythmo_text_cache(&mut self) {
        while self.rythmo_text_cache.len() > 1
            && (self.rythmo_text_cache.len() > MAX_RYTHMO_TEXT_CACHE_ENTRIES
                || self.rythmo_text_cache_bytes > MAX_RYTHMO_TEXT_CACHE_BYTES)
        {
            let Some(oldest_key) = self
                .rythmo_text_cache
                .iter()
                .min_by_key(|(_, cached)| cached.last_used)
                .map(|(&key, _)| key)
            else {
                break;
            };
            if let Some(removed) = self.rythmo_text_cache.remove(&oldest_key) {
                self.rythmo_text_cache_bytes =
                    self.rythmo_text_cache_bytes.saturating_sub(removed.bytes);
            }
        }
    }

    /// Rasterize text into RGBA pixels at natural size, returns (pixels, width, height).
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

    fn cached_voice_actor_icon(&mut self, actor: &VoiceActor) -> Option<&[u8]> {
        let icon = actor.icon_png_base64.as_deref()?;
        let hash = icon_hash(icon);
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.voice_actor_icon_cache.entry(hash)
        {
            let rgba = decode_icon_rgba(icon).ok()?;
            entry.insert(rgba);
        }
        self.voice_actor_icon_cache
            .get(&hash)
            .map(|data| data.as_slice())
    }

    fn render_voice_actor_icons(
        &mut self,
        pixmap: &mut Pixmap,
        project: &Project,
        line: &crate::rythmo_line::RythmoLine,
        x: f32,
        y: f32,
        _badge_w: f32,
        icon_size: f32,
        scale: f32,
    ) {
        if line.karaoke || line.voice_actor_names.is_empty() {
            return;
        }

        let icon_size = icon_size.max(1.0);
        // The badge ends immediately before the line body. Keep actor icons
        // on the outer side of the badge so they cannot cover the line text.
        let mut icon_x = x - 3.0 * scale - icon_size;

        for actor_name in &line.voice_actor_names {
            if icon_x > pixmap.width() as f32 {
                break;
            }
            blit_rect(pixmap, icon_x, y, icon_size, icon_size, [10, 10, 14, 235]);

            if let Some(actor) = project.find_voice_actor(actor_name) {
                if let Some(icon) = self.cached_voice_actor_icon(actor) {
                    blit_actor_icon(pixmap, icon, icon_x, y, icon_size);
                } else {
                    self.blit_actor_fallback(pixmap, &actor.name, icon_x, y, icon_size);
                }
            } else {
                self.blit_actor_fallback(pixmap, actor_name, icon_x, y, icon_size);
            }
            icon_x -= icon_size + 3.0 * scale;
        }
    }

    fn blit_actor_fallback(&mut self, pixmap: &mut Pixmap, text: &str, x: f32, y: f32, size: f32) {
        let (tex, tw, th) = self.rasterize_text(text, size * 0.55);
        if tw == 0 || th == 0 {
            return;
        }
        let tx = x + (size - tw as f32) / 2.0;
        let ty = y + (size - th as f32) / 2.0;
        let pm_w = pixmap.width() as i32;
        let pm_h = pixmap.height() as i32;
        let pm_data = pixmap.data_mut();
        for py in 0..th {
            for px in 0..tw {
                let dx = tx as i32 + px as i32;
                let dy = ty as i32 + py as i32;
                if dx < 0 || dy < 0 || dx >= pm_w || dy >= pm_h || px as f32 >= size {
                    continue;
                }
                let si = ((py * tw + px) * 4) as usize;
                let di = ((dy as u32 * pm_w as u32 + dx as u32) * 4) as usize;
                if si + 3 >= tex.len() || di + 3 >= pm_data.len() {
                    continue;
                }
                let a = tex[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let inv = 255 - a;
                pm_data[di] = ((230u32 * a + pm_data[di] as u32 * inv) / 255) as u8;
                pm_data[di + 1] = ((230u32 * a + pm_data[di + 1] as u32 * inv) / 255) as u8;
                pm_data[di + 2] = ((238u32 * a + pm_data[di + 2] as u32 * inv) / 255) as u8;
                pm_data[di + 3] = (a + (pm_data[di + 3] as u32 * inv) / 255) as u8;
            }
        }
    }

    fn blit_read_word_text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        segment_start: usize,
        highlight_end: Option<usize>,
        base_tint: [u8; 3],
    ) {
        let count = text.chars().count();
        let Some(highlight_end) = highlight_end else {
            self.blit_rythmo_text_tinted_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0,
            );
            return;
        };
        if count == 0 || highlight_end <= segment_start {
            self.blit_rythmo_text_tinted_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0,
            );
            return;
        }
        let end_ratio = ((highlight_end - segment_start) as f32 / count as f32).min(1.0);
        if end_ratio < 1.0 {
            self.blit_rythmo_text_tinted_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0,
            );
        }
        self.blit_rythmo_text_tinted_clipped(
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            [255, 209, 20],
            end_ratio,
        );
    }

    fn blit_rythmo_text_tinted_clipped(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        tint: [u8; 3],
        clip_ratio: f32,
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode(
            pixmap, text, x, y, dest_w, dest_h, font_size, tint, clip_ratio, true, false,
        );
    }

    fn blit_rythmo_text_natural_tinted_clipped(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        tint: [u8; 3],
        clip_ratio: f32,
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode(
            pixmap, text, x, y, dest_w, dest_h, font_size, tint, clip_ratio, false, false,
        );
    }

    fn blit_rythmo_text_natural_emphasized_tinted(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        tint: [u8; 3],
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode(
            pixmap, text, x, y, dest_w, dest_h, font_size, tint, 1.0, false, true,
        );
    }

    fn blit_rythmo_text_tinted_clipped_with_mode(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        tint: [u8; 3],
        clip_ratio: f32,
        stretch: bool,
        emphasized: bool,
    ) {
        let tex_w = dest_w.max(1.0).ceil() as u32;
        let tex_h = dest_h.max(1.0).ceil() as u32;
        let cache_key = if emphasized {
            self.get_or_render_rythmo_text_natural_emphasized(text, font_size, tex_w, tex_h)
        } else if stretch {
            self.get_or_render_rythmo_text(text, font_size, tex_w, tex_h)
        } else {
            self.get_or_render_rythmo_text_natural(text, font_size, tex_w, tex_h)
        };
        let Some(cache_key) = cache_key else {
            return;
        };
        let Some(rendered) = self.rythmo_text_cache.get(&cache_key) else {
            return;
        };
        if rendered.width == 0 || rendered.height == 0 {
            return;
        }
        let clip_width = (rendered.width as f32 * clip_ratio.clamp(0.0, 1.0)).ceil() as u32;
        if clip_width == 0 {
            return;
        }

        let pm_w = pixmap.width() as i32;
        let pm_h = pixmap.height() as i32;
        let xi = x as i32;
        let yi = y as i32;
        let start_dx = (-xi).max(0).min(rendered.width as i32) as u32;
        let end_dx = (pm_w - xi)
            .max(0)
            .min(rendered.width as i32)
            .min(clip_width as i32) as u32;
        let start_dy = (-yi).max(0).min(rendered.height as i32) as u32;
        let end_dy = (pm_h - yi).max(0).min(rendered.height as i32) as u32;

        if start_dx >= end_dx || start_dy >= end_dy {
            return;
        }

        let pm_data = pixmap.data_mut();

        for dy in start_dy..end_dy {
            let py = yi + dy as i32;

            for dx in start_dx..end_dx {
                let px = xi + dx as i32;

                let src_idx = ((dy * rendered.width + dx) * 4) as usize;
                let dst_idx = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;

                if src_idx + 3 >= rendered.pixels.len() || dst_idx + 3 >= pm_data.len() {
                    continue;
                }

                let sa = rendered.pixels[src_idx + 3] as u32;
                if sa == 0 {
                    continue;
                }

                let inv_a = 255 - sa;
                for c in 0..3 {
                    let src = (rendered.pixels[src_idx + c] as u32 * tint[c] as u32) / 255;
                    let dst = pm_data[dst_idx + c] as u32;
                    pm_data[dst_idx + c] = (src + (dst * inv_a) / 255).min(255) as u8;
                }
                pm_data[dst_idx + 3] = (sa + (pm_data[dst_idx + 3] as u32 * inv_a) / 255) as u8;
            }
        }
    }

    /// Render the bande rythmo for a given frame. All sizes scale with width.
    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: i64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        let s = width as f32 / constants::REF_WIDTH * br_scale; // export BR scale factor
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
        self.render_index.refresh(project);
        let visible_frames = (width as f32 / ppf) as i64 + 4;
        let render_margin_frames = ((source_fps.max(1.0) * 10.0).round() as i64)
            .max(karaoke_adjacent_max_gap_frames(source_fps))
            .max(karaoke_count_in_frames(source_fps))
            .saturating_add(self.render_index.max_duration_frames());
        let scene = RythmoScene::build(
            project,
            &self.render_index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: current_frame
                        .saturating_sub(visible_frames / 2)
                        .saturating_sub(render_margin_frames),
                    last: current_frame
                        .saturating_add(visible_frames / 2)
                        .saturating_add(render_margin_frames),
                },
                current_frame: current_frame as f64,
                source_fps,
                normal_body_height: normal_slot_h,
                slot_header_height: slot_header_h,
                badge_gap,
                scale: s,
                dynamic_track_layout: false,
            },
        );
        let track_layouts = &scene.tracks;
        let height = (ruler_h + rythmo_layout::total_tracks_height(track_layouts)).ceil() as u32;

        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(5, 5, 8, 255));

        let w = width as f32;
        let h = height as f32;
        let center_x = w / 2.0;

        // -- Ruler ticks --
        let first_tick_frame = current_frame - visible_frames / 2;
        let first_tick =
            first_tick_frame.div_euclid(constants::TICK_GAP_FRAMES) * constants::TICK_GAP_FRAMES;
        let mut tf = first_tick;
        loop {
            let x = center_x + (tf - current_frame) as f32 * ppf;
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
                blit_rect(&mut pixmap, x, 0.0, tick_w, th, [100, 100, 115, 128]);
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // -- Playhead, split around active karaoke lines --
        let playhead_gaps = scene.active_karaoke_skip_ranges(ruler_h, slot_header_h, badge_gap, s);
        blit_playhead_segments(
            &mut pixmap,
            center_x - playhead_w / 2.0,
            playhead_w,
            h,
            &playhead_gaps,
        );

        // -- Lines (no handles, no border -- clean export) --
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
                line.visual_x_width(current_frame as f64, center_x, ppf, w, s)
            };
            let badge_w = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart)
            {
                rythmo_layout::scaled_character_badge_width(
                    &crate::rythmo_line::ambiance_label(&line.character_name),
                    s,
                )
                .max(150.0 * s)
            } else {
                rythmo_layout::scaled_character_badge_width(&line.character_name, s)
            };
            let label_gap = if scene_line.karaoke_should_be_centered() {
                constants::BADGE_GAP * s
            } else {
                4.0 * ppf
            };
            let badge_x = x1 - badge_w - label_gap;
            let show_badge =
                line.kind.is_dialogue() && (!line.karaoke || scene_line.character_label_visible);
            let has_leading_label = show_badge
                || matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
            let leading_visual = has_leading_label.then(|| {
                rythmo_layout::leading_visual_bounds(
                    badge_x,
                    badge_w,
                    if !line.karaoke {
                        line.voice_actor_names.len()
                    } else {
                        0
                    },
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
        for scene_line in &scene.lines {
            if let Some(r) = compute_line_rect(scene_line) {
                let line = &scene_line.line;
                line_rects.insert(line.id, (r, line.character_name.clone()));
            }
        }
        for scene_line in &scene.lines {
            let line = &scene_line.line;
            let karaoke_count_in = scene_line.karaoke_count_in_progress.is_some();
            if line.karaoke && !scene_line.karaoke_should_be_visible() {
                continue;
            }

            let (x1, lw) = if scene_line.karaoke_should_be_centered() {
                let width = self.karaoke_text_width(&line.text, font_size, karaoke_text_scale);
                (center_x - width / 2.0, width)
            } else {
                line.visual_x_width(current_frame as f64, center_x, ppf, w, s)
            };
            let badge_w = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart)
            {
                rythmo_layout::scaled_character_badge_width(
                    &crate::rythmo_line::ambiance_label(&line.character_name),
                    s,
                )
                .max(150.0 * s)
            } else {
                rythmo_layout::scaled_character_badge_width(&line.character_name, s)
            };
            let badge_x = rythmo_layout::leading_character_badge_x(x1, badge_w, s);
            let show_badge =
                line.kind.is_dialogue() && (!line.karaoke || scene_line.character_label_visible);
            let has_leading_label = show_badge
                || matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
            let leading_visual = has_leading_label.then(|| {
                rythmo_layout::leading_visual_bounds(
                    badge_x,
                    badge_w,
                    if !line.karaoke {
                        line.voice_actor_names.len()
                    } else {
                        0
                    },
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

            // Calculate badge position/size for later drawing (on top of text)
            // Rectangular, top-aligned, with right edge a few px left of the line's left edge.
            let badge_h = body_h;
            let [cr, cg, cb, _] = line.character_color;
            let badge_y = line_y;

            if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                let ambiance_label = crate::rythmo_line::ambiance_label(&line.character_name);
                let underline_x = badge_x + font_size * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &ambiance_label,
                    font_size,
                )
                .unwrap_or(badge_w)
                .min((badge_x + badge_w - underline_x).max(0.0));
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    &ambiance_label,
                    badge_x,
                    badge_y,
                    badge_w,
                    badge_h,
                    font_size,
                    [51, 140, 255],
                );
                blit_rect(
                    &mut pixmap,
                    underline_x,
                    badge_y + badge_h - 2.0 * s,
                    underline_w,
                    1.5 * s,
                    [51, 140, 255, 255],
                );
                blit_rect(
                    &mut pixmap,
                    underline_x,
                    badge_y + badge_h - 5.5 * s,
                    underline_w,
                    1.5 * s,
                    [51, 140, 255, 255],
                );
            }

            // Rythmo text, rendered vectorially at final size.
            if !line.text.is_empty() && line.text != "↑" && line.text != "↓" {
                let read_highlight_end = if project.settings().highlight_read_word && !line.karaoke
                {
                    let progress = (current_frame as f64 - line.start_frame as f64)
                        / line.duration_frames.max(1) as f64;
                    crate::syllable::read_highlight_end_from_timing(
                        &line.text,
                        &line.syllable_ratios,
                        scene.syllable_language.code(),
                        progress as f32,
                    )
                } else {
                    None
                };
                let scrolling_text_tint = if line.kind.is_ambiance() {
                    [242, 31, 41]
                } else if project.settings().scrolling_text_uses_character_color {
                    [
                        color_channel(line.character_color[0]),
                        color_channel(line.character_color[1]),
                        color_channel(line.character_color[2]),
                    ]
                } else {
                    [255; 3]
                };
                if line.kind.is_ambiance() {
                    let reserve = (54.0 * s).min(lw);
                    let (text_x, text_w) =
                        if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                            (x1 + reserve, (lw - reserve).max(1.0))
                        } else {
                            (x1, (lw - reserve).max(1.0))
                        };
                    self.blit_rythmo_text_tinted_clipped(
                        &mut pixmap,
                        &line.text,
                        text_x,
                        line_y,
                        text_w,
                        body_h,
                        font_size,
                        scrolling_text_tint,
                        1.0,
                    );
                } else if line.karaoke {
                    let karaoke_font_size =
                        font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
                    self.blit_rythmo_text_natural_tinted_clipped(
                        &mut pixmap,
                        &line.text,
                        x1,
                        line_y,
                        lw,
                        body_h,
                        karaoke_font_size,
                        [255, 255, 255],
                        1.0,
                    );
                    if let Some(progress) = scene_line.karaoke_progress {
                        let visual_progress = crate::syllable::visual_progress_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            scene.syllable_language.code(),
                            progress,
                        );
                        self.blit_rythmo_text_natural_tinted_clipped(
                            &mut pixmap,
                            &line.text,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            karaoke_font_size,
                            [
                                color_channel(line.character_color[0]),
                                color_channel(line.character_color[1]),
                                color_channel(line.character_color[2]),
                            ],
                            visual_progress,
                        );
                    }
                } else {
                    let lang = scene.syllable_language.code();
                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                    let base_ratios =
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
                    let ratios = project.detections().warped_ratios(
                        line.id,
                        &line.text,
                        &breaks,
                        &base_ratios,
                        line.start_frame,
                        line.duration_frames,
                    );
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
                                self.blit_read_word_text(
                                    &mut pixmap,
                                    &segment,
                                    seg_x,
                                    line_y,
                                    seg_w,
                                    body_h,
                                    font_size,
                                    prev_break,
                                    read_highlight_end,
                                    scrolling_text_tint,
                                );
                            }
                            seg_x += seg_w;
                            prev_break = end_break;
                        }
                    } else {
                        self.blit_read_word_text(
                            &mut pixmap,
                            &line.text,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            font_size,
                            0,
                            read_highlight_end,
                            scrolling_text_tint,
                        );
                    }
                }
            }

            if !line.presence.is_on() && !line.text.is_empty() {
                let underline_y = line_y + body_h - (3.0 * s).max(1.0);
                let thickness = (1.5 * s).max(1.0);
                if line.presence == crate::rythmo_line::LinePresence::Off {
                    blit_rect(
                        &mut pixmap,
                        x1,
                        underline_y,
                        lw,
                        thickness,
                        [255, 255, 255, 255],
                    );
                } else {
                    let (dash, gap) = ((8.0 * s).max(2.0), (5.0 * s).max(2.0));
                    let mut x = x1;
                    while x < x1 + lw {
                        blit_rect(
                            &mut pixmap,
                            x,
                            underline_y,
                            dash.min(x1 + lw - x),
                            thickness,
                            [255, 255, 255, 255],
                        );
                        x += dash + gap;
                    }
                }
            }

            if line.kind.is_ambiance() {
                let at_start =
                    matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart);
                let gutter = (46.0 * s).min(lw);
                let gx = if at_start { x1 } else { x1 + lw - gutter };
                let dir = if at_start { 1.0 } else { -1.0 };
                let cy = line_y + body_h * 0.5;
                let tip_x = if at_start {
                    gx + gutter - 5.0 * s
                } else {
                    gx + 5.0 * s
                };
                let base_x = tip_x - dir * 15.0 * s;
                for dy in [-10.0 * s, 10.0 * s] {
                    blit_thick_line(
                        &mut pixmap,
                        base_x,
                        cy + dy,
                        tip_x,
                        cy,
                        5.0 * s,
                        [255, 255, 255, 255],
                    );
                }
                blit_thick_line(
                    &mut pixmap,
                    gx + 5.0 * s,
                    cy,
                    gx + gutter - 5.0 * s,
                    cy,
                    5.0 * s,
                    [255, 255, 255, 255],
                );
                let bar_x = if at_start {
                    gx + 3.0 * s
                } else {
                    gx + gutter - 3.0 * s
                };
                blit_thick_line(
                    &mut pixmap,
                    bar_x,
                    cy - 13.0 * s,
                    bar_x,
                    cy + 13.0 * s,
                    5.0 * s,
                    [255, 255, 255, 255],
                );
            }

            // Overlap detection vs OTHER lines: hide if same character, 60% opacity if different
            let mut badge_hidden = false;
            let mut badge_overlap_alpha = 255u8;
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
                        badge_overlap_alpha =
                            (255.0 * constants::CHARACTER_BADGE_COLLISION_OPACITY) as u8;
                    }
                }
            }

            // Same emphasized typography as ambiance labels, tinted with the
            // character colour and deliberately left without an underline.
            if show_badge && !badge_hidden {
                let underline_x = badge_x + font_size * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &line.character_name,
                    font_size,
                )
                .unwrap_or(badge_w)
                .min((badge_x + badge_w - underline_x).max(0.0));
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    &line.character_name,
                    badge_x,
                    badge_y,
                    badge_w,
                    badge_h,
                    font_size,
                    [color_channel(cr), color_channel(cg), color_channel(cb)],
                );
                for y_offset in [2.0, 5.5] {
                    blit_rect(
                        &mut pixmap,
                        underline_x,
                        badge_y + badge_h - y_offset * s,
                        underline_w,
                        1.5 * s,
                        [
                            color_channel(cr),
                            color_channel(cg),
                            color_channel(cb),
                            badge_overlap_alpha,
                        ],
                    );
                }

                self.render_voice_actor_icons(
                    &mut pixmap,
                    project,
                    line,
                    badge_x,
                    badge_y,
                    badge_w,
                    actor_icon_size,
                    s,
                );
            }

            // Breath arrows
            if line.text == "↑" || line.text == "↓" {
                let up = line.text == "↑";
                let margin = 4.0 * s.max(1.0);
                if lw > margin * 2.0 + 1.0 && body_h > margin * 2.0 + 1.0 {
                    let (y0, y1) = if up {
                        (line_y + body_h - margin, line_y + margin)
                    } else {
                        (line_y + margin, line_y + body_h - margin)
                    };
                    blit_thick_line(
                        &mut pixmap,
                        x1 + margin,
                        y0,
                        x1 + lw - margin,
                        y1,
                        2.0 * s,
                        [220, 220, 230, 230],
                    );
                }
            }

            if karaoke_count_in {
                blit_karaoke_count_in_dot(
                    &mut pixmap,
                    line,
                    x1,
                    line_y,
                    scene_line.karaoke_count_in_progress,
                    s,
                );
            } else {
                blit_karaoke_dot(
                    &mut pixmap,
                    line,
                    scene.syllable_language.code(),
                    current_frame as f64,
                    x1,
                    line_y,
                    lw,
                    s,
                );
            }

            // Note text (discrete, at the bottom of the line)
            if !line.note.is_empty() {
                let note_font = badge_font * 0.9;
                let note_h = (note_font * 1.3).ceil();
                let note_y = line_y + body_h - note_h - 1.0;
                let (tex, tw, th) = self.rasterize_text(&line.note, note_font);
                if tw > 0 && th > 0 {
                    let max_note_w = lw - 8.0 * s;
                    let blit_w = (tw as f32).min(max_note_w);
                    let pm_w = pixmap.width() as i32;
                    let pm_h = pixmap.height() as i32;
                    let pm_data = pixmap.data_mut();
                    for py in 0..th {
                        for px in 0..tw {
                            let dx = (x1 + 4.0 * s) as i32 + px as i32;
                            let dy = note_y as i32 + py as i32;
                            if dx < 0 || dy < 0 || dx >= pm_w || dy >= pm_h {
                                continue;
                            }
                            if px as f32 >= blit_w {
                                break;
                            }
                            let si = ((py * tw + px) * 4) as usize;
                            let di = ((dy as u32 * pm_w as u32 + dx as u32) * 4) as usize;
                            if si + 3 >= tex.len() || di + 3 >= pm_data.len() {
                                continue;
                            }
                            let a = tex[si + 3] as u32;
                            if a == 0 {
                                continue;
                            }
                            // Tint: gray (160, 160, 170)
                            let sr = 160u32 * a / 255;
                            let sg = 160u32 * a / 255;
                            let sb = 170u32 * a / 255;
                            let inv = 255 - a;
                            pm_data[di] = ((sr + pm_data[di] as u32 * inv) / 255) as u8;
                            pm_data[di + 1] = ((sg + pm_data[di + 1] as u32 * inv) / 255) as u8;
                            pm_data[di + 2] = ((sb + pm_data[di + 2] as u32 * inv) / 255) as u8;
                            pm_data[di + 3] = (a + (pm_data[di + 3] as u32 * inv) / 255) as u8;
                        }
                    }
                }
            }
        }

        // Drawings are an overlay in the editor, so composite them last in the
        // exported BR as well (above lines, labels and markers).
        let (first_frame, last_frame) =
            crate::rythmo_drawing::visible_frame_window(width as f32, current_frame as f64, ppf, 4);
        let strokes: Vec<_> = scene
            .drawings
            .iter()
            .filter(|stroke| stroke.intersects_window(first_frame, last_frame))
            .collect();
        if !strokes.is_empty() {
            let drawing = crate::rythmo_drawing::rasterize_window(
                &strokes,
                width,
                height,
                current_frame as f64,
                ppf,
            );
            crate::rythmo_drawing::composite_rgba_over(pixmap.data_mut(), &drawing);
        }

        pixmap.data().to_vec()
    }
}

fn color_channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn blit_karaoke_dot(
    pixmap: &mut Pixmap,
    line: &crate::rythmo_line::RythmoLine,
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
    let cx = if width > size {
        x + size / 2.0 + visual_progress.clamp(0.0, 1.0) * (width - size)
    } else {
        x + width / 2.0
    };
    let cy = y + 3.0 * scale.max(0.5) + size / 2.0
        - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
    blit_circle(
        pixmap,
        cx,
        cy,
        size / 2.0 + 1.5 * scale.max(0.5),
        [0, 0, 0, 90],
    );
    blit_circle(
        pixmap,
        cx,
        cy,
        size / 2.0,
        [
            color_channel(line.character_color[0]),
            color_channel(line.character_color[1]),
            color_channel(line.character_color[2]),
            255,
        ],
    );
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

fn blit_karaoke_count_in_dot(
    pixmap: &mut Pixmap,
    line: &crate::rythmo_line::RythmoLine,
    x: f32,
    y: f32,
    count_in_progress: Option<f32>,
    scale: f32,
) {
    let Some(count_in_progress) = count_in_progress else {
        return;
    };

    let (dx, dy, size) = karaoke_count_in_dot_rect(x, y, count_in_progress, scale);
    blit_circle(
        pixmap,
        dx + size / 2.0,
        dy + size / 2.0,
        size / 2.0 + 1.5 * scale.max(0.5),
        [0, 0, 0, 90],
    );
    blit_circle(
        pixmap,
        dx + size / 2.0,
        dy + size / 2.0,
        size / 2.0,
        [
            color_channel(line.character_color[0]),
            color_channel(line.character_color[1]),
            color_channel(line.character_color[2]),
            255,
        ],
    );
}

fn blit_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, color: [u8; 4]) {
    if !cx.is_finite() || !cy.is_finite() || !radius.is_finite() || radius <= 0.0 || color[3] == 0 {
        return;
    }

    let pm_w = pixmap.width() as i32;
    let pm_h = pixmap.height() as i32;
    let min_x = (cx - radius - 1.0).floor() as i32;
    let max_x = (cx + radius + 1.0).ceil() as i32;
    let min_y = (cy - radius - 1.0).floor() as i32;
    let max_y = (cy + radius + 1.0).ceil() as i32;
    let data = pixmap.data_mut();

    for py in min_y.max(0)..max_y.min(pm_h) {
        for px in min_x.max(0)..max_x.min(pm_w) {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let coverage = (radius + 1.0 - dist).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let alpha = (color[3] as f32 * coverage).round() as u32;
            if alpha == 0 {
                continue;
            }
            let inv = 255 - alpha;
            let di = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;
            data[di] = ((color[0] as u32 * alpha + data[di] as u32 * inv) / 255) as u8;
            data[di + 1] = ((color[1] as u32 * alpha + data[di + 1] as u32 * inv) / 255) as u8;
            data[di + 2] = ((color[2] as u32 * alpha + data[di + 2] as u32 * inv) / 255) as u8;
            data[di + 3] = (alpha + (data[di + 3] as u32 * inv) / 255).min(255) as u8;
        }
    }
}

fn blit_rect(pixmap: &mut Pixmap, x: f32, y: f32, width: f32, height: f32, color: [u8; 4]) {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return;
    }

    let pm_w = pixmap.width() as i32;
    let pm_h = pixmap.height() as i32;
    if pm_w <= 0 || pm_h <= 0 {
        return;
    }

    let min_x = (x.floor() as i32).clamp(0, pm_w);
    let max_x = ((x + width).ceil() as i32).clamp(0, pm_w);
    let min_y = (y.floor() as i32).clamp(0, pm_h);
    let max_y = ((y + height).ceil() as i32).clamp(0, pm_h);
    if min_x >= max_x || min_y >= max_y || color[3] == 0 {
        return;
    }

    let alpha = color[3] as u32;
    let inv = 255 - alpha;
    let data = pixmap.data_mut();
    for py in min_y..max_y {
        for px in min_x..max_x {
            let di = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;
            if di + 3 >= data.len() {
                continue;
            }
            data[di] = ((color[0] as u32 * alpha + data[di] as u32 * inv) / 255) as u8;
            data[di + 1] = ((color[1] as u32 * alpha + data[di + 1] as u32 * inv) / 255) as u8;
            data[di + 2] = ((color[2] as u32 * alpha + data[di + 2] as u32 * inv) / 255) as u8;
            data[di + 3] = (alpha + (data[di + 3] as u32 * inv) / 255).min(255) as u8;
        }
    }
}

fn blit_thick_line(
    pixmap: &mut Pixmap,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    color: [u8; 4],
) {
    if !x0.is_finite()
        || !y0.is_finite()
        || !x1.is_finite()
        || !y1.is_finite()
        || !width.is_finite()
        || width <= 0.0
    {
        return;
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return;
    }

    let pm_w = pixmap.width() as i32;
    let pm_h = pixmap.height() as i32;
    if pm_w <= 0 || pm_h <= 0 {
        return;
    }

    let half = width.max(1.0) * 0.5;
    let aa = 1.0;
    let min_x = ((x0.min(x1) - half - aa).floor() as i32).clamp(0, pm_w);
    let max_x = ((x0.max(x1) + half + aa).ceil() as i32).clamp(0, pm_w);
    let min_y = ((y0.min(y1) - half - aa).floor() as i32).clamp(0, pm_h);
    let max_y = ((y0.max(y1) + half + aa).ceil() as i32).clamp(0, pm_h);
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    let data = pixmap.data_mut();
    for py in min_y..max_y {
        let fy = py as f32 + 0.5;
        for px in min_x..max_x {
            let fx = px as f32 + 0.5;
            let t = (((fx - x0) * dx + (fy - y0) * dy) / len_sq).clamp(0.0, 1.0);
            let cx = x0 + t * dx;
            let cy = y0 + t * dy;
            let dist = ((fx - cx) * (fx - cx) + (fy - cy) * (fy - cy)).sqrt();
            let coverage = (half + aa - dist).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }

            let alpha = (color[3] as f32 * coverage).round().clamp(0.0, 255.0) as u32;
            if alpha == 0 {
                continue;
            }
            let inv = 255 - alpha;
            let di = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;
            if di + 3 >= data.len() {
                continue;
            }
            data[di] = ((color[0] as u32 * alpha + data[di] as u32 * inv) / 255) as u8;
            data[di + 1] = ((color[1] as u32 * alpha + data[di + 1] as u32 * inv) / 255) as u8;
            data[di + 2] = ((color[2] as u32 * alpha + data[di + 2] as u32 * inv) / 255) as u8;
            data[di + 3] = (alpha + (data[di + 3] as u32 * inv) / 255).min(255) as u8;
        }
    }
}

fn blit_actor_icon(pixmap: &mut Pixmap, icon: &[u8], x: f32, y: f32, size: f32) {
    let dest_size = size.max(1.0).round() as i32;
    let xi = x.round() as i32;
    let yi = y.round() as i32;
    let pm_w = pixmap.width() as i32;
    let pm_h = pixmap.height() as i32;
    let pm_data = pixmap.data_mut();
    let src_size = VOICE_ACTOR_ICON_SIZE as i32;

    for dy in 0..dest_size {
        let py = yi + dy;
        if py < 0 || py >= pm_h {
            continue;
        }
        for dx in 0..dest_size {
            let px = xi + dx;
            if px < 0 || px >= pm_w {
                continue;
            }

            let sx = (dx * src_size / dest_size).clamp(0, src_size - 1);
            let sy = (dy * src_size / dest_size).clamp(0, src_size - 1);
            let si = ((sy as u32 * VOICE_ACTOR_ICON_SIZE + sx as u32) * 4) as usize;
            let di = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;
            if si + 3 >= icon.len() || di + 3 >= pm_data.len() {
                continue;
            }
            let a = icon[si + 3] as u32;
            if a == 0 {
                continue;
            }
            let inv = 255 - a;
            pm_data[di] = ((icon[si] as u32 * a + pm_data[di] as u32 * inv) / 255) as u8;
            pm_data[di + 1] =
                ((icon[si + 1] as u32 * a + pm_data[di + 1] as u32 * inv) / 255) as u8;
            pm_data[di + 2] =
                ((icon[si + 2] as u32 * a + pm_data[di + 2] as u32 * inv) / 255) as u8;
            pm_data[di + 3] = (a + (pm_data[di + 3] as u32 * inv) / 255) as u8;
        }
    }
}

/// Calculate the BR height in pixels based on used slots.
pub fn br_height(project: &Project, width: u32, br_scale: f32) -> u32 {
    let s = width as f32 / constants::REF_WIDTH * br_scale;
    let normal_slot_h = constants::SLOT_HEIGHT * s;
    let badge_h = constants::BADGE_HEIGHT * s;
    let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
    let slot_header_h = badge_h.max(actor_icon_size);
    let badge_gap = constants::BADGE_GAP * s;
    let track_indices = rythmo_layout::used_track_indices(project);
    let track_layouts = rythmo_layout::build_track_layouts(
        project,
        &track_indices,
        normal_slot_h,
        slot_header_h,
        badge_gap,
        s,
    );
    (constants::RULER_HEIGHT * s + rythmo_layout::total_tracks_height(&track_layouts)).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::{MarkerKind, RythmoMarker};

    #[test]
    fn br_height_doubles_only_tracks_with_karaoke() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;

        let width = constants::REF_WIDTH as u32;
        let br_scale = 1.0;
        let s = width as f32 / constants::REF_WIDTH * br_scale;
        let normal_body_h = constants::SLOT_HEIGHT * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
        let slot_header_h = badge_h.max(actor_icon_size);
        let badge_gap = constants::BADGE_GAP * s;
        let normal_total_h = normal_body_h + slot_header_h + badge_gap;
        let karaoke_total_h =
            rythmo_layout::karaoke_track_body_height(normal_body_h, s) + slot_header_h + badge_gap;
        let expected =
            (constants::RULER_HEIGHT * s + normal_total_h + karaoke_total_h).ceil() as u32;

        assert_eq!(br_height(&project, width, br_scale), expected);
    }

    #[test]
    fn cpu_export_count_in_dot_moves_from_left_onto_text() {
        let x = 300.0;
        let y = 80.0;
        let (start_x, _, start_size) = karaoke_count_in_dot_rect(x, y, 0.0, 1.0);
        let (mid_x, _, _) = karaoke_count_in_dot_rect(x, y, 0.5, 1.0);
        let (end_x, _, _) = karaoke_count_in_dot_rect(x, y, 1.0, 1.0);

        assert!(start_x + start_size <= x);
        assert!(mid_x > start_x);
        assert!(mid_x < x);
        assert!((end_x - x).abs() < 0.01);
    }

    #[test]
    fn cpu_export_karaoke_island_after_normal_line_continues_alternating_rows() {
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

    #[test]
    fn cpu_render_handles_marker_and_breath_lines() {
        crate::config::init();
        let mut project = Project::new();
        project.add_line_full(0, 24, 0.0, "↑".into(), "Alice".into(), [0.8, 0.2, 0.2, 1.0]);
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 0,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Out,
            frame: 1,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::LiaisonLeft,
            frame: 2,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::LiaisonRight,
            frame: 3,
        });

        let width = 320;
        let br_scale = 0.5;
        let height = br_height(&project, width, br_scale);
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render_br(&project, 0, width, 24.0, br_scale, 1.0);

        assert_eq!(pixels.len(), width as usize * height as usize * 4);
    }
}
