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
use unicode_segmentation::UnicodeSegmentation;

// Local constants not shared with the UI
const BASE_TICK_WIDTH: f32 = 1.5;
const MAX_RYTHMO_TEXT_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RYTHMO_TEXT_CACHE_ENTRIES: usize = 512;

fn blit_playhead_segments(
    pixmap: &mut Pixmap,
    x: f32,
    width: f32,
    height: f32,
    skip_ranges: &[(f32, f32)],
    color: [u8; 4],
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
            blit_rect(pixmap, x, y, width, skip_start - y, color);
        }
        y = y.max(skip_end);
    }
    if y < height {
        blit_rect(pixmap, x, y, width, height - y, color);
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
    karaoke_dot_sprites: crate::karaoke_dot::CpuDotSprites,
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
            karaoke_dot_sprites: crate::karaoke_dot::CpuDotSprites::new(),
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
        text_styles: &[crate::vector_text::TextStyleRun],
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        if !text_styles.is_empty() {
            text_styles.hash(&mut h);
        }
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
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, true, false, &[])
    }

    fn get_or_render_rythmo_text_natural(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<u64> {
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, false, false, &[])
    }

    fn get_or_render_rythmo_text_natural_emphasized(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
    ) -> Option<u64> {
        self.get_or_render_rythmo_text_with_mode(text, font_size, dest_w, dest_h, false, true, &[])
    }

    fn get_or_render_rythmo_text_with_mode(
        &mut self,
        text: &str,
        font_size: f32,
        dest_w: u32,
        dest_h: u32,
        stretch: bool,
        emphasized: bool,
        text_styles: &[crate::vector_text::TextStyleRun],
    ) -> Option<u64> {
        self.cache_tick = self.cache_tick.wrapping_add(1);
        let key = Self::rythmo_text_cache_key(
            text,
            font_size,
            dest_w,
            dest_h,
            stretch,
            emphasized,
            text_styles,
        );
        if let Some(cached) = self.rythmo_text_cache.get_mut(&key) {
            cached.last_used = self.cache_tick;
            return Some(key);
        }

        let rendered = if !text_styles.is_empty() && !emphasized {
            crate::vector_text::render_rythmo_text_styled(
                &mut self.font_system,
                text,
                font_size,
                dest_w,
                dest_h,
                false,
                stretch,
                false,
                text_styles,
            )?
        } else if emphasized {
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

    /// Voice actor icons beside the character label `badge`. `frames`
    /// draws their backgrounds (under the text, like the editor's quads),
    /// otherwise their pictures or names (over it).
    fn render_voice_actor_icons(
        &mut self,
        pixmap: &mut Pixmap,
        project: &Project,
        line: &crate::rythmo_line::RythmoLine,
        badge: Rect,
        icon_size: f32,
        scale: f32,
        frames: bool,
    ) {
        use crate::band_visuals as visuals;
        if line.karaoke || line.voice_actor_names.is_empty() {
            return;
        }

        let icon_size = icon_size.max(1.0);
        let gap = visuals::ACTOR_ICON_GAP * scale;
        // The badge ends immediately before the line body. Keep actor icons
        // on the outer side of the badge so they cannot cover the line text.
        let mut icon_x = badge.x - gap - icon_size;
        let y = badge.y + (badge.height - icon_size) * 0.5;

        for actor_name in &line.voice_actor_names {
            if icon_x > pixmap.width() as f32 {
                break;
            }
            if frames {
                blit_quad(
                    pixmap,
                    Rect {
                        x: icon_x,
                        y,
                        width: icon_size,
                        height: icon_size,
                    },
                    visuals::ACTOR_ICON_BG_TOP,
                    visuals::ACTOR_ICON_BG_BOTTOM,
                    visuals::ACTOR_ICON_BORDER,
                    visuals::ACTOR_ICON_BORDER_WIDTH * scale,
                    visuals::ACTOR_ICON_RADIUS * scale,
                );
            } else if let Some(actor) = project.find_voice_actor(actor_name) {
                if let Some(icon) = self.cached_voice_actor_icon(actor) {
                    blit_actor_icon(pixmap, icon, icon_x, y, icon_size);
                } else {
                    self.blit_actor_fallback(pixmap, &actor.name, icon_x, y, icon_size);
                }
            } else {
                self.blit_actor_fallback(pixmap, actor_name, icon_x, y, icon_size);
            }
            icon_x -= icon_size + gap;
        }
    }

    fn blit_actor_fallback(&mut self, pixmap: &mut Pixmap, text: &str, x: f32, y: f32, size: f32) {
        let font_size = size * crate::band_visuals::ACTOR_FALLBACK_FONT_RATIO;
        let (tex, tw, th) = self.rasterize_text(text, font_size);
        if tw == 0 || th == 0 {
            return;
        }
        let tx = x + (size - tw as f32) / 2.0;
        let ty = y + (size - th as f32) / 2.0;
        Self::blit_text_mask(
            pixmap,
            &tex,
            tw,
            th,
            tx,
            ty,
            size,
            crate::band_visuals::ACTOR_FALLBACK_TEXT_COLOR,
        );
    }

    /// Draws a small marker label (loop number, "out") at `font_size` with
    /// its top-left at `x`, `y`.
    fn blit_marker_label(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        color: [u8; 3],
    ) {
        let (tex, tw, th) = self.rasterize_text(text, font_size.max(1.0));
        if tw == 0 || th == 0 {
            return;
        }
        Self::blit_text_mask(pixmap, &tex, tw, th, x, y, f32::INFINITY, color);
    }

    /// Blends the coverage of a rasterized text `tex` (`tw` x `th`, at most
    /// `max_width` pixels wide) tinted with `color`.
    fn blit_text_mask(
        pixmap: &mut Pixmap,
        tex: &[u8],
        tw: u32,
        th: u32,
        tx: f32,
        ty: f32,
        max_width: f32,
        color: [u8; 3],
    ) {
        let size = max_width;
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
                pm_data[di] = ((color[0] as u32 * a + pm_data[di] as u32 * inv) / 255) as u8;
                pm_data[di + 1] =
                    ((color[1] as u32 * a + pm_data[di + 1] as u32 * inv) / 255) as u8;
                pm_data[di + 2] =
                    ((color[2] as u32 * a + pm_data[di + 2] as u32 * inv) / 255) as u8;
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
        line_styles: &[crate::rythmo_line::TextStyleSpan],
    ) {
        let count = text.chars().count();
        let styles = if line_styles.is_empty() {
            Vec::new()
        } else {
            crate::rythmo_line::text_style_runs(line_styles, segment_start, segment_start + count)
        };
        let Some(highlight_end) = highlight_end else {
            self.blit_rythmo_text_styled_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0, &styles,
            );
            return;
        };
        if count == 0 || highlight_end <= segment_start {
            self.blit_rythmo_text_styled_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0, &styles,
            );
            return;
        }
        let end_ratio = ((highlight_end - segment_start) as f32 / count as f32).min(1.0);
        if end_ratio < 1.0 {
            self.blit_rythmo_text_styled_clipped(
                pixmap, text, x, y, dest_w, dest_h, font_size, base_tint, 1.0, &styles,
            );
        }
        self.blit_rythmo_text_styled_clipped(
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            [255, 209, 20],
            end_ratio,
            &styles,
        );
    }

    /// Stretched band text carrying per-character styles; empty `text_styles`
    /// draws exactly like `blit_rythmo_text_tinted_clipped`.
    fn blit_rythmo_text_styled_clipped(
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
        text_styles: &[crate::vector_text::TextStyleRun],
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode(
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            tint,
            clip_ratio,
            true,
            false,
            text_styles,
            255,
        );
    }

    /// Like `blit_rythmo_text_styled_clipped`, with an opacity.
    fn blit_rythmo_text_styled_clipped_alpha(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
        tint: [u8; 3],
        alpha: u8,
        text_styles: &[crate::vector_text::TextStyleRun],
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode(
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            tint,
            1.0,
            true,
            false,
            text_styles,
            alpha,
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
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            tint,
            clip_ratio,
            true,
            false,
            &[],
            255,
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
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            tint,
            clip_ratio,
            false,
            false,
            &[],
            255,
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
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            tint,
            1.0,
            false,
            true,
            &[],
            255,
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
        text_styles: &[crate::vector_text::TextStyleRun],
        alpha: u8,
    ) {
        if alpha == 0 {
            return;
        }
        let tex_w = dest_w.max(1.0).ceil() as u32;
        let tex_h = dest_h.max(1.0).ceil() as u32;
        let cache_key = if !text_styles.is_empty() && !emphasized {
            self.get_or_render_rythmo_text_with_mode(
                text,
                font_size,
                tex_w,
                tex_h,
                stretch,
                false,
                text_styles,
            )
        } else if emphasized {
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

                // Premultiplied source, like the editor's band text shader.
                let sa = rendered.pixels[src_idx + 3] as u32 * alpha as u32 / 255;
                if sa == 0 {
                    continue;
                }

                let inv_a = 255 - sa;
                for c in 0..3 {
                    let src = rendered.pixels[src_idx + c] as u32 * tint[c] as u32 / 255
                        * alpha as u32
                        / 255;
                    let dst = pm_data[dst_idx + c] as u32;
                    pm_data[dst_idx + c] = (src + (dst * inv_a) / 255).min(255) as u8;
                }
                pm_data[dst_idx + 3] = (sa + (pm_data[dst_idx + 3] as u32 * inv_a) / 255) as u8;
            }
        }
    }

    fn blit_emotional_text(
        &mut self,
        pixmap: &mut Pixmap,
        line: &crate::rythmo_line::RythmoLine,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        seconds: f32,
        scale: f32,
        tint: [u8; 3],
        sync_positions: Option<&[f32]>,
        show_lane: bool,
    ) {
        let graphemes: Vec<&str> = line.text.graphemes(true).collect();
        let char_count = line.text.chars().count().max(1);
        let ratios = sync_positions
            .filter(|ratios| ratios.len() == char_count + 1)
            .map(<[f32]>::to_vec)
            .or_else(|| {
                crate::rythmo_line::text_emotion_char_ratios(&line.text, font_size)
                    .filter(|ratios| ratios.len() == char_count + 1)
            });
        let mut char_start = 0;
        for (index, grapheme) in graphemes.iter().enumerate() {
            let char_end = char_start + grapheme.chars().count();
            let start_ratio = ratios
                .as_ref()
                .map(|ratios| ratios[char_start])
                .unwrap_or(char_start as f32 / char_count as f32);
            let end_ratio = ratios
                .as_ref()
                .map(|ratios| ratios[char_end])
                .unwrap_or(char_end as f32 / char_count as f32);
            let gx = x + start_ratio * width;
            let gw = ((end_ratio - start_ratio) * width).max(0.5);
            let style = if line.can_have_text_styles() {
                line.style_at_char(char_start)
            } else {
                crate::rythmo_line::TextStyle::default()
            };
            let style_runs = if style.is_plain() {
                Vec::new()
            } else {
                vec![(0, char_end - char_start, style)]
            };
            if let Some(emotion) = line.emotion_at_char(char_start) {
                let animation = crate::rythmo_line::text_emotion_transform(
                    emotion,
                    index,
                    graphemes.len(),
                    seconds,
                );
                let animated_tint = if emotion == crate::rythmo_line::TextEmotion::Yay {
                    [
                        (255.0 * animation.tint[0]).round() as u8,
                        (255.0 * animation.tint[1]).round() as u8,
                        (255.0 * animation.tint[2]).round() as u8,
                    ]
                } else {
                    [
                        (tint[0] as f32 * animation.tint[0]).round() as u8,
                        (tint[1] as f32 * animation.tint[1]).round() as u8,
                        (tint[2] as f32 * animation.tint[2]).round() as u8,
                    ]
                };
                let key = self.get_or_render_rythmo_text_with_mode(
                    grapheme,
                    font_size,
                    gw.ceil() as u32,
                    height.ceil() as u32,
                    true,
                    false,
                    &style_runs,
                );
                if let Some(rendered) = key.and_then(|key| self.rythmo_text_cache.get(&key)) {
                    Self::blit_cached_transformed(
                        pixmap,
                        rendered,
                        gx + animation.offset[0],
                        y + animation.offset[1] - height * 0.08,
                        animation.transform,
                        animated_tint,
                        color_channel(animation.tint[3]),
                    );
                }
                if show_lane {
                    let (copy_y, copy_height) =
                        rythmo_layout::text_emotion_copy_rect(y, height, scale);
                    self.blit_rythmo_text_styled_clipped_alpha(
                        pixmap,
                        grapheme,
                        gx,
                        copy_y,
                        gw,
                        copy_height,
                        font_size * 0.68,
                        tint,
                        color_channel(crate::band_visuals::TEXT_EMOTION_LANE_ALPHA),
                        &style_runs,
                    );
                }
            } else {
                self.blit_rythmo_text_styled_clipped(
                    pixmap,
                    grapheme,
                    gx,
                    y,
                    gw,
                    height,
                    font_size,
                    tint,
                    1.0,
                    &style_runs,
                );
            }
            char_start = char_end;
        }
    }

    fn blit_cached_transformed(
        pixmap: &mut Pixmap,
        rendered: &CachedCpuRythmoText,
        x: f32,
        y: f32,
        transform: [f32; 4],
        tint: [u8; 3],
        opacity: u8,
    ) {
        if opacity == 0 {
            return;
        }
        let [angle, skew, pivot_x, pivot_y] = transform;
        let (sin, cos) = angle.sin_cos();
        let pivot = [
            pivot_x * rendered.width as f32,
            pivot_y * rendered.height as f32,
        ];
        let radius = rendered.width.max(rendered.height) as i32;
        let left = (x as i32 - radius).max(0);
        let top = (y as i32 - radius).max(0);
        let right = (x as i32 + rendered.width as i32 + radius).min(pixmap.width() as i32);
        let bottom = (y as i32 + rendered.height as i32 + radius).min(pixmap.height() as i32);
        let pm_w = pixmap.width();
        let data = pixmap.data_mut();
        for py in top..bottom {
            for px in left..right {
                let rx = px as f32 - x - pivot[0];
                let ry = py as f32 - y - pivot[1];
                let unrotated_x = rx * cos + ry * sin;
                let source_y = -rx * sin + ry * cos + pivot[1];
                let source_x = unrotated_x - skew * (source_y - pivot[1]) + pivot[0];
                let sx = source_x.round() as i32;
                let sy = source_y.round() as i32;
                if sx < 0 || sy < 0 || sx >= rendered.width as i32 || sy >= rendered.height as i32 {
                    continue;
                }
                let source = ((sy as u32 * rendered.width + sx as u32) * 4) as usize;
                let destination = ((py as u32 * pm_w + px as u32) * 4) as usize;
                let alpha = rendered.pixels[source + 3] as u32 * opacity as u32 / 255;
                if alpha == 0 {
                    continue;
                }
                let inverse_alpha = 255 - alpha;
                for channel in 0..3 {
                    let source_channel = rendered.pixels[source + channel] as u32
                        * tint[channel] as u32
                        / 255
                        * opacity as u32
                        / 255;
                    data[destination + channel] = (source_channel
                        + data[destination + channel] as u32 * inverse_alpha / 255)
                        .min(255) as u8;
                }
                data[destination + 3] =
                    (alpha + data[destination + 3] as u32 * inverse_alpha / 255).min(255) as u8;
            }
        }
    }

    /// Render the bande rythmo for a fractional source-frame position.
    ///
    /// Integer frame bounds are used only for visibility queries; every visual
    /// position keeps the fractional component so a 24 fps project can scroll
    /// smoothly in a 60 fps export.
    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        let current_frame = if current_frame.is_finite() {
            current_frame
        } else {
            0.0
        };
        let current_frame_floor = current_frame.floor() as i64;
        let current_frame_ceil = current_frame.ceil() as i64;
        let s = width as f32 / constants::REF_WIDTH * br_scale; // export BR scale factor
        let normal_slot_h = constants::SLOT_HEIGHT * s;
        let ruler_h = constants::RULER_HEIGHT * s;
        let ppf = constants::PIXELS_PER_FRAME * s * project.settings().scroll_speed;
        let tick_long = constants::TICK_LONG * s;
        let tick_short = constants::TICK_SHORT * s;
        let tick_w = BASE_TICK_WIDTH * s;
        let playhead_w = project.settings().band_style.playhead_width * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let badge_gap = constants::BADGE_GAP * s;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
        let slot_header_h = badge_h.max(actor_icon_size);
        let font_size = constants::RYTHMO_FONT_SIZE * s;
        let character_label_font = constants::CHARACTER_LABEL_FONT_SIZE * s;
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
                    first: current_frame_floor
                        .saturating_sub(visible_frames / 2)
                        .saturating_sub(render_margin_frames),
                    last: current_frame_ceil
                        .saturating_add(visible_frames / 2)
                        .saturating_add(render_margin_frames),
                },
                current_frame,
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
        let [bg_r, bg_g, bg_b, _] =
            crate::band_style::to_rgba8(project.settings().band_style.background);
        pixmap.fill(tiny_skia::Color::from_rgba8(bg_r, bg_g, bg_b, 255));

        let w = width as f32;
        let h = height as f32;
        let center_x = w / 2.0;
        let offset_frames = crate::rythmo_layout::reading_bar_offset_seconds(
            project.settings().reading_bar_offset_percent,
            w,
            source_fps,
            ppf,
        ) * source_fps;

        // -- Ruler ticks --
        let first_tick_frame = current_frame_floor - visible_frames / 2;
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
                blit_rect(&mut pixmap, x, 0.0, tick_w, th, [100, 100, 115, 128]);
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // -- Playhead, split around active karaoke lines (only when overlapping) --
        let playhead_x = center_x - playhead_w / 2.0 - offset_frames as f32 * ppf;
        let playhead_gaps: Vec<(f32, f32)> = scene
            .lines
            .iter()
            .filter(|scene_line| scene_line.karaoke_active)
            .filter_map(|scene_line| {
                let track =
                    crate::rythmo_layout::track_for_index(&scene.tracks, scene_line.track_index)?;
                let body_y = ruler_h + track.top + slot_header_h + badge_gap;
                let line_y = karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, s);
                let y_range = (line_y, line_y + karaoke_stack_height(track.body_h, s));

                let karaoke_width =
                    self.karaoke_text_width(&scene_line.line.text, font_size, karaoke_text_scale);
                let karaoke_left = center_x - karaoke_width / 2.0;
                let karaoke_right = center_x + karaoke_width / 2.0;

                if playhead_x + playhead_w > karaoke_left && playhead_x < karaoke_right {
                    Some(y_range)
                } else {
                    None
                }
            })
            .collect();
        blit_playhead_segments(
            &mut pixmap,
            playhead_x,
            playhead_w,
            h,
            &playhead_gaps,
            crate::band_style::to_rgba8(project.settings().band_style.playhead),
        );

        // -- Markers, under the lines like the editor's band quads --
        self.render_markers(
            &mut pixmap,
            &scene.markers,
            current_frame,
            center_x,
            ppf,
            offset_frames,
            w,
            h,
            s,
            false,
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
                line.visual_x_width(current_frame, center_x, ppf, w, s, offset_frames)
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
                badge_gap
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
            let track = rythmo_layout::track_for_y_slot(track_layouts, line.y_slot)?;
            let y_base = ruler_h + track.top;
            let body_y = y_base + slot_header_h + badge_gap;
            let mut line_y = body_y;
            let mut body_h = rythmo_layout::line_body_height(line, track, normal_slot_h);
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
                line.visual_x_width(current_frame, center_x, ppf, w, s, offset_frames)
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
            let badge_x = rythmo_layout::leading_character_badge_x(
                x1,
                badge_w,
                s,
                Some(if scene_line.karaoke_should_be_centered() {
                    badge_gap
                } else {
                    4.0 * ppf
                }),
            );
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
            let mut body_h = rythmo_layout::line_body_height(line, track, normal_slot_h);
            if line.karaoke {
                line_y = karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, s);
                body_h = karaoke_stack_height(track.body_h, s);
            }

            // Calculate badge position/size.
            // Rectangular, top-aligned, with right edge a few px left of the line's left edge.
            let badge_h = body_h;
            let [cr, cg, cb, _] = line.character_color;
            let badge_y = line_y;
            let character_rgb = [color_channel(cr), color_channel(cg), color_channel(cb)];
            let scrolling_text_tint = if line.kind.is_ambiance() {
                [242, 31, 41]
            } else if project.settings().scrolling_text_uses_character_color {
                character_rgb
            } else {
                [255; 3]
            };
            let karaoke_dot = project
                .settings()
                .band_style
                .dot_for_character(&line.character_name);

            // Against OTHER line bodies: hidden over the same character,
            // shrunk to fit (fully opaque) over another one, like the editor.
            let badge_info = if show_badge && !line.character_name.is_empty() {
                let fit = crate::band_visuals::fit_character_badge_among_lines(
                    Rect {
                        x: badge_x,
                        y: badge_y,
                        width: badge_w,
                        height: badge_h,
                    },
                    x1,
                    badge_gap,
                    line.id,
                    &line.character_name,
                    &line_rects,
                );
                (!fit.hidden).then_some((fit.rect, fit.scale))
            } else {
                None
            };

            // -- Geometry, drawn under the text like the editor's band quads --
            if karaoke_dot.is_circle() {
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
                        current_frame,
                        x1,
                        line_y,
                        lw,
                        s,
                    );
                }
            }

            let ambiance_label = matches!(
                line.kind,
                crate::rythmo_line::RythmoLineKind::AmbianceStart
            )
            .then(|| crate::rythmo_line::ambiance_label(&line.character_name));
            if let Some(ambiance_label) = &ambiance_label {
                let underline_x = badge_x + character_label_font * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    ambiance_label,
                    character_label_font,
                )
                .unwrap_or(badge_w)
                .min((badge_x + badge_w - underline_x).max(0.0));
                for y_offset in [2.0, 5.5] {
                    blit_rect(
                        &mut pixmap,
                        underline_x,
                        badge_y + badge_h - y_offset * s,
                        underline_w,
                        1.5 * s,
                        [51, 140, 255, 255],
                    );
                }
            }

            if !line.presence.is_on() && !line.text.is_empty() {
                // Same underline as the editor, in the scrolling text colour.
                use crate::band_visuals::{
                    PRESENCE_DASH_LENGTH, PRESENCE_DASH_PERIOD, PRESENCE_UNDERLINE_BOTTOM_OFFSET,
                    PRESENCE_UNDERLINE_THICKNESS,
                };
                let underline_y = line_y + body_h - PRESENCE_UNDERLINE_BOTTOM_OFFSET * s;
                let thickness = PRESENCE_UNDERLINE_THICKNESS * s;
                let [ur, ug, ub] = scrolling_text_tint;
                if line.presence == crate::rythmo_line::LinePresence::Off {
                    blit_rect(
                        &mut pixmap,
                        x1,
                        underline_y,
                        lw,
                        thickness,
                        [ur, ug, ub, 255],
                    );
                } else {
                    let dash = PRESENCE_DASH_LENGTH * s;
                    let period = (PRESENCE_DASH_PERIOD * s).max(0.5);
                    let mut x = x1;
                    while x < x1 + lw {
                        blit_rect(
                            &mut pixmap,
                            x,
                            underline_y,
                            dash.min(x1 + lw - x),
                            thickness,
                            [ur, ug, ub, 255],
                        );
                        x += period;
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

            // Breath arrows, with their arrowhead like in the editor.
            if line.text == "↑" || line.text == "↓" {
                use crate::band_visuals::{
                    breath_arrow_bars, rgba8, rotated_bar_ends, BREATH_ARROW_COLOR,
                    BREATH_ARROW_HEAD_LENGTH, BREATH_ARROW_MARGIN, BREATH_ARROW_THICKNESS,
                };
                let up = line.text == "↑";
                let margin = BREATH_ARROW_MARGIN * s;
                if lw > margin * 2.0 + 1.0 && body_h > margin * 2.0 + 1.0 {
                    let body = Rect {
                        x: x1,
                        y: line_y,
                        width: lw,
                        height: body_h,
                    };
                    for (cx, cy, length, angle) in
                        breath_arrow_bars(body, up, margin, BREATH_ARROW_HEAD_LENGTH * s)
                    {
                        let [(x0, y0), (x1, y1)] = rotated_bar_ends(cx, cy, length, angle);
                        blit_thick_line(
                            &mut pixmap,
                            x0,
                            y0,
                            x1,
                            y1,
                            BREATH_ARROW_THICKNESS * s,
                            rgba8(BREATH_ARROW_COLOR),
                        );
                    }
                }
            }

            if let Some((badge, badge_scale)) = badge_info {
                let label_font = character_label_font * badge_scale;
                let underline_x = badge.x + label_font * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &line.character_name,
                    label_font,
                )
                .unwrap_or(badge.width)
                .min((badge.x + badge.width - underline_x).max(0.0));
                for y_offset in [2.0, 5.5] {
                    blit_rect(
                        &mut pixmap,
                        underline_x,
                        badge.y + badge.height - y_offset * s * badge_scale,
                        underline_w,
                        1.5 * s * badge_scale,
                        [character_rgb[0], character_rgb[1], character_rgb[2], 255],
                    );
                }
                self.render_voice_actor_icons(
                    &mut pixmap,
                    project,
                    line,
                    badge,
                    actor_icon_size * badge_scale,
                    s,
                    true,
                );
            }

            // -- Text --
            if let Some(ambiance_label) = &ambiance_label {
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    ambiance_label,
                    badge_x,
                    badge_y,
                    badge_w,
                    badge_h,
                    character_label_font,
                    [51, 140, 255],
                );
            }

            // Rythmo text, rendered vectorially at final size.
            if !line.text.is_empty() && line.text != "↑" && line.text != "↓" {
                let read_highlight_end = if project.settings().highlight_read_word && !line.karaoke
                {
                    let progress = (current_frame - line.start_frame as f64)
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
                } else if !line.text_emotions.is_empty() {
                    let sync_positions = project.detections().warped_character_positions(
                        line.id,
                        &line.text,
                        line.start_frame,
                        line.duration_frames,
                    );
                    self.blit_emotional_text(
                        &mut pixmap,
                        line,
                        x1,
                        line_y,
                        lw,
                        body_h,
                        font_size,
                        (current_frame / source_fps.max(1.0)) as f32,
                        s,
                        scrolling_text_tint,
                        sync_positions.as_deref(),
                        project.settings().show_text_emotion_lanes,
                    );
                } else {
                    let line_styles: &[crate::rythmo_line::TextStyleSpan] =
                        if line.can_have_text_styles() {
                            &line.text_styles
                        } else {
                            &[]
                        };
                    let lang = scene.syllable_language.code();
                    let base_breaks = crate::syllable::syllable_breaks(&line.text, lang);
                    let base_ratios =
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
                    let (breaks, ratios) = project.detections().warped_segments(
                        line.id,
                        &line.text,
                        &base_breaks,
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
                                    line_styles,
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
                            line_styles,
                        );
                    }
                }
            }

            // Same emphasized typography as ambiance labels, tinted with the
            // character colour.
            if let Some((badge, badge_scale)) = badge_info {
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    &line.character_name,
                    badge.x,
                    badge.y,
                    badge.width,
                    badge.height,
                    character_label_font * badge_scale,
                    character_rgb,
                );
            }

            // -- Textures drawn over the text, like the editor --
            if let Some((badge, badge_scale)) = badge_info {
                self.render_voice_actor_icons(
                    &mut pixmap,
                    project,
                    line,
                    badge,
                    actor_icon_size * badge_scale,
                    s,
                    false,
                );
            }

            if !karaoke_dot.is_circle() {
                let dot_rect = if karaoke_count_in {
                    scene_line
                        .karaoke_count_in_progress
                        .map(|progress| karaoke_count_in_dot_rect(x1, line_y, progress, s))
                } else {
                    karaoke_dot_center(
                        line,
                        scene.syllable_language.code(),
                        current_frame,
                        x1,
                        line_y,
                        lw,
                        s,
                    )
                    .map(|(cx, cy, size)| (cx - size / 2.0, cy - size / 2.0, size))
                };
                if let Some((dx, dy, size)) = dot_rect {
                    self.karaoke_dot_sprites.draw(
                        &mut pixmap,
                        karaoke_dot,
                        crate::karaoke_dot::face_for_dot(line_y, dy, size, s),
                        dx,
                        dy,
                        size,
                        [
                            line.character_color[0],
                            line.character_color[1],
                            line.character_color[2],
                            1.0,
                        ],
                        s,
                    );
                }
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
        // exported BR as well (above lines and markers).
        let (first_frame, last_frame) = crate::rythmo_drawing::visible_frame_window(
            width as f32,
            current_frame,
            ppf,
            4,
            source_fps,
            offset_frames / source_fps,
        );
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
                current_frame,
                ppf,
                source_fps,
                offset_frames / source_fps,
            );
            crate::rythmo_drawing::composite_rgba_over(pixmap.data_mut(), &drawing);
        }
        // The editor draws marker labels with the other UI text, last.
        self.render_markers(
            &mut pixmap,
            &scene.markers,
            current_frame,
            center_x,
            ppf,
            offset_frames,
            w,
            h,
            s,
            true,
        );

        pixmap.data().to_vec()
    }

    /// Draws the markers: their bars, or their labels when `labels` is set.
    fn render_markers(
        &mut self,
        pixmap: &mut Pixmap,
        markers: &[crate::rendering::rythmo::scene::SceneMarker],
        current_frame: f64,
        center_x: f32,
        ppf: f32,
        reading_bar_offset_frames: f64,
        w: f32,
        h: f32,
        s: f32,
        labels: bool,
    ) {
        use crate::band_visuals::{self as visuals, rgba8, rotated_bar_ends};
        use crate::rythmo_line::MarkerKind;
        let bar_w = visuals::MARKER_BAR_WIDTH * s;
        let cy = h / 2.0;
        for marker in markers {
            let mx = rythmo_layout::export_timeline_x(
                marker.frame,
                current_frame,
                center_x,
                ppf,
                reading_bar_offset_frames,
            );
            if mx < -10.0 * s || mx > w + 10.0 * s {
                continue;
            }
            match &marker.kind {
                MarkerKind::Boucle if labels => {
                    if let Some(number) = marker.loop_number {
                        self.blit_marker_label(
                            pixmap,
                            &number.to_string(),
                            visuals::LOOP_NUMBER_FONT_SIZE * s,
                            mx + visuals::LOOP_NUMBER_OFFSET[0] * s,
                            cy + visuals::LOOP_NUMBER_OFFSET[1] * s,
                            visuals::LOOP_NUMBER_COLOR,
                        );
                    }
                }
                MarkerKind::Boucle => {
                    let color = rgba8(visuals::LOOP_MARKER_COLOR);
                    blit_rect(pixmap, mx - bar_w / 2.0, 0.0, bar_w, h, color);
                    for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
                        let [(x0, y0), (x1, y1)] = rotated_bar_ends(
                            mx,
                            cy,
                            visuals::LOOP_MARKER_X_BAR_LENGTH * s,
                            angle,
                        );
                        blit_thick_line(
                            pixmap,
                            x0,
                            y0,
                            x1,
                            y1,
                            visuals::LOOP_MARKER_X_THICKNESS * s,
                            color,
                        );
                    }
                }
                MarkerKind::Out if labels => {
                    let font = visuals::OUT_LABEL_FONT_SIZE * s;
                    // Vertically centred on the band, like the editor label.
                    let label_h = (font * 1.4).ceil();
                    self.blit_marker_label(
                        pixmap,
                        visuals::OUT_LABEL,
                        font,
                        mx + visuals::OUT_LABEL_OFFSET_X * s,
                        cy - label_h / 2.0,
                        visuals::OUT_LABEL_COLOR,
                    );
                }
                MarkerKind::Out => {
                    let color = rgba8(visuals::OUT_MARKER_COLOR);
                    blit_rect(pixmap, mx - bar_w / 2.0, 0.0, bar_w, h, color);
                    for offset in visuals::OUT_MARKER_BAR_OFFSETS {
                        let [(x0, y0), (x1, y1)] = rotated_bar_ends(
                            mx + offset * s,
                            cy,
                            h * visuals::OUT_MARKER_BAR_LENGTH_RATIO,
                            visuals::OUT_MARKER_BAR_ANGLE,
                        );
                        blit_thick_line(
                            pixmap,
                            x0,
                            y0,
                            x1,
                            y1,
                            visuals::OUT_MARKER_BAR_THICKNESS * s,
                            color,
                        );
                    }
                }
                _ if labels => {}
                MarkerKind::SceneChange => {
                    blit_rect(
                        pixmap,
                        mx - bar_w / 2.0,
                        0.0,
                        bar_w,
                        h,
                        rgba8(visuals::SCENE_CHANGE_COLOR),
                    );
                }
                MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => {
                    let is_left = matches!(marker.kind, MarkerKind::LiaisonLeft);
                    let ay = constants::RULER_HEIGHT * s / 2.0;
                    let arm_x = if is_left { -3.0 } else { 3.0 } * s;
                    let arm_y = 4.0 * s;
                    // Two full arms from the vertex to the tips, like the GPU
                    // export.
                    for &dy in &[-arm_y, arm_y] {
                        blit_thick_line(
                            pixmap,
                            mx - arm_x,
                            ay,
                            mx + arm_x,
                            ay + dy,
                            1.5 * s,
                            rgba8(visuals::LIAISON_MARKER_TINT),
                        );
                    }
                }
            }
        }
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
    let Some((cx, cy, size)) = karaoke_dot_center(line, lang, current_frame, x, y, width, scale)
    else {
        return;
    };
    blit_karaoke_circle(
        pixmap,
        cx - size / 2.0,
        cy - size / 2.0,
        size,
        line.character_color,
        scale,
    );
}

/// Round karaoke dot like the editor: a soft shadow, the character colour
/// and a white rim.
fn blit_karaoke_circle(pixmap: &mut Pixmap, x: f32, y: f32, size: f32, color: [f32; 4], scale: f32) {
    let unit = scale.max(0.5);
    let expand = 1.5 * unit;
    let shadow = crate::karaoke_dot::SHADOW_TINT;
    blit_quad(
        pixmap,
        Rect {
            x: x - expand,
            y: y - expand,
            width: size + expand * 2.0,
            height: size + expand * 2.0,
        },
        shadow,
        shadow,
        [0.0; 4],
        0.0,
        size / 2.0 + expand,
    );
    let color = [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
        1.0,
    ];
    blit_quad(
        pixmap,
        Rect {
            x,
            y,
            width: size,
            height: size,
        },
        color,
        color,
        crate::karaoke_dot::RIM_TINT,
        unit,
        size / 2.0,
    );
}

/// Centre and size of the bouncing karaoke dot of `line`.
fn karaoke_dot_center(
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    current_frame: f64,
    x: f32,
    y: f32,
    width: f32,
    scale: f32,
) -> Option<(f32, f32, f32)> {
    let progress = line.karaoke_progress(current_frame)?;
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
    Some((cx, cy, size))
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
    blit_karaoke_circle(pixmap, dx, dy, size, line.character_color, scale);
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sdf_rounded_rect(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = x.abs() - half_w + radius;
    let qy = y.abs() - half_h + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Draws a band quad the way `ui/quad.wgsl` does (vertical gradient from
/// `top` to `bottom`, inner border, rounded corners), so CPU exports share
/// the editor's shapes. Colours are straight-alpha sRGB.
fn blit_quad(
    pixmap: &mut Pixmap,
    rect: Rect,
    top: [f32; 4],
    bottom: [f32; 4],
    border: [f32; 4],
    border_width: f32,
    radius: f32,
) {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return;
    }
    let pm_w = pixmap.width() as i32;
    let pm_h = pixmap.height() as i32;
    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;
    let radius = radius.min(half_w.min(half_h)).max(0.0);
    let center_x = rect.x + half_w;
    let center_y = rect.y + half_h;
    let min_x = ((rect.x - 1.0).floor() as i32).clamp(0, pm_w);
    let max_x = ((rect.x + rect.width + 1.0).ceil() as i32).clamp(0, pm_w);
    let min_y = ((rect.y - 1.0).floor() as i32).clamp(0, pm_h);
    let max_y = ((rect.y + rect.height + 1.0).ceil() as i32).clamp(0, pm_h);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let data = pixmap.data_mut();
    for py in min_y..max_y {
        let local_y = py as f32 + 0.5 - center_y;
        let t = ((local_y + half_h) / rect.height).clamp(0.0, 1.0);
        for px in min_x..max_x {
            let local_x = px as f32 + 0.5 - center_x;
            let dist = sdf_rounded_rect(local_x, local_y, half_w, half_h, radius);
            if dist > 0.5 {
                continue;
            }
            let inner = sdf_rounded_rect(
                local_x,
                local_y,
                half_w - border_width,
                half_h - border_width,
                (radius - border_width).max(0.0),
            );
            let border_mask = smoothstep(-0.5, 0.5, inner);
            let aa = 1.0 - smoothstep(-0.5, 0.5, dist);
            let mut color = [0.0; 4];
            for (channel, value) in color.iter_mut().enumerate() {
                let background = mix(top[channel], bottom[channel], t);
                *value = mix(background, border[channel], border_mask);
            }
            let alpha = (color[3] * aa).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let di = ((py as u32 * pm_w as u32 + px as u32) * 4) as usize;
            for channel in 0..3 {
                let source = color[channel].clamp(0.0, 1.0) * 255.0;
                let destination = data[di + channel] as f32;
                data[di + channel] =
                    (source * alpha + destination * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8;
            }
            let destination_alpha = data[di + 3] as f32;
            data[di + 3] = (alpha * 255.0 + destination_alpha * (1.0 - alpha))
                .round()
                .clamp(0.0, 255.0) as u8;
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
    fn rythmo_text_cache_key_follows_text_styles() {
        crate::config::init();
        let bold = crate::rythmo_line::TextStyle {
            bold: true,
            ..Default::default()
        };
        let italic = crate::rythmo_line::TextStyle {
            italic: true,
            ..Default::default()
        };
        let key = |styles: &[crate::vector_text::TextStyleRun]| {
            CpuRenderer::rythmo_text_cache_key("Salut", 24.0, 120, 34, true, false, styles)
        };
        assert_ne!(key(&[]), key(&[(0, 2, bold)]));
        assert_ne!(key(&[(0, 2, bold)]), key(&[(0, 2, italic)]));
        assert_ne!(key(&[(0, 2, bold)]), key(&[(0, 3, bold)]));
        assert_eq!(key(&[(0, 2, bold)]), key(&[(0, 2, bold)]));
    }

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
    fn cpu_karaoke_circle_dot_has_the_editor_white_rim() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        blit_karaoke_circle(&mut pixmap, 10.0, 10.0, 20.0, [0.0, 0.0, 1.0, 1.0], 1.0);
        let pixel = |x: u32, y: u32| {
            let i = ((y * 40 + x) * 4) as usize;
            let data = pixmap.data();
            [data[i], data[i + 1], data[i + 2]]
        };
        assert_eq!(pixel(20, 20), [0, 0, 255]);
        let rim = pixel(10, 20);
        assert!(rim[0] > 150 && rim[1] > 150, "rim pixel {rim:?}");
        // The soft shadow lies just outside the dot.
        let shadow = pixel(8, 20);
        assert!(shadow[2] < 20 && shadow[0] < 20, "shadow pixel {shadow:?}");
    }

    #[test]
    fn cpu_markers_are_drawn_under_the_line_text() {
        crate::config::init();
        let mut project = Project::new();
        project.add_line_full(
            0,
            96,
            0.0,
            "IIIIIIIIIIIIIIIIIIIIIIII".into(),
            String::new(),
            [1.0, 1.0, 1.0, 1.0],
        );
        for frame in 0..96 {
            project.add_marker(RythmoMarker {
                kind: MarkerKind::Boucle,
                frame,
            });
        }

        let width = constants::REF_WIDTH as u32;
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render_br(&project, 48.0, width, 24.0, 1.0, 1.0);
        let height = pixels.len() / 4 / width as usize;
        let pixel = |x: usize, y: usize| {
            let i = (y * width as usize + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        // Columns crossed by a loop bar are red at the top of the ruler.
        let marker_columns: Vec<usize> = (0..width as usize)
            .filter(|&x| {
                let [r, g, _] = pixel(x, 1);
                r > 150 && g < 90
            })
            .collect();
        assert!(!marker_columns.is_empty());
        // Where the white text covers a bar, the text wins, like in the
        // editor (bars drawn over the text would tint every pixel red).
        let text_over_marker = marker_columns
            .iter()
            .any(|&x| (0..height).any(|y| pixel(x, y).iter().all(|&c| c >= 245)));
        assert!(text_over_marker);
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
        let pixels = renderer.render_br(&project, 0.0, width, 24.0, br_scale, 1.0);

        assert_eq!(pixels.len(), width as usize * height as usize * 4);
    }
}
