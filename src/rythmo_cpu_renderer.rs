use std::collections::HashMap;

use crate::constants;
use crate::project::Project;
use crate::rythmo_line::MarkerKind;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};
use resvg::tiny_skia::{self, Paint, PathBuilder, Pixmap, Rect as SkRect, Transform};

// Local constants not shared with the UI
const BASE_TICK_WIDTH: f32 = 1.0;
const BASE_PLAYHEAD_WIDTH: f32 = 2.0;
const MAX_RYTHMO_TEXT_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RYTHMO_TEXT_CACHE_ENTRIES: usize = 512;

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
    rythmo_text_cache: HashMap<u64, CachedCpuRythmoText>,
    rythmo_text_cache_bytes: usize,
    cache_tick: u64,
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            rythmo_text_cache: HashMap::new(),
            rythmo_text_cache_bytes: 0,
            cache_tick: 0,
        }
    }

    fn rythmo_text_cache_key(text: &str, font_size: f32, dest_w: u32, dest_h: u32) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        font_size.to_bits().hash(&mut h);
        dest_w.hash(&mut h);
        dest_h.hash(&mut h);
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
        self.cache_tick = self.cache_tick.wrapping_add(1);
        let key = Self::rythmo_text_cache_key(text, font_size, dest_w, dest_h);
        if let Some(cached) = self.rythmo_text_cache.get_mut(&key) {
            cached.last_used = self.cache_tick;
            return Some(key);
        }

        let rendered = crate::vector_text::render_rythmo_text(
            &mut self.font_system,
            text,
            font_size,
            dest_w,
            dest_h,
        )?;
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
        let line_height = (font_size * 1.4).ceil();
        let mut buffer =
            GlyphonBuffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
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

    /// Render vector rythmo text at final size and blit it without horizontal resampling.
    fn blit_rythmo_text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        dest_w: f32,
        dest_h: f32,
        font_size: f32,
    ) {
        let tex_w = dest_w.max(1.0).ceil() as u32;
        let tex_h = dest_h.max(1.0).ceil() as u32;
        let Some(cache_key) = self.get_or_render_rythmo_text(text, font_size, tex_w, tex_h) else {
            return;
        };
        let Some(rendered) = self.rythmo_text_cache.get(&cache_key) else {
            return;
        };
        if rendered.width == 0 || rendered.height == 0 {
            return;
        }

        let pm_w = pixmap.width() as i32;
        let pm_h = pixmap.height() as i32;
        let xi = x as i32;
        let yi = y as i32;
        let start_dx = (-xi).max(0).min(rendered.width as i32) as u32;
        let end_dx = (pm_w - xi).max(0).min(rendered.width as i32) as u32;
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
                    let src = rendered.pixels[src_idx + c] as u32;
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
        _fps: f64,
        br_scale: f32,
    ) -> Vec<u8> {
        let s = width as f32 / constants::REF_WIDTH * br_scale; // export BR scale factor
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
        let height = (ruler_h + slot_count * (slot_h + badge_h + badge_gap)).ceil() as u32;

        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(5, 5, 8, 255));

        let w = width as f32;
        let h = height as f32;
        let center_x = w / 2.0;

        // -- Ruler ticks --
        let mut tick_paint = Paint::default();
        tick_paint.set_color_rgba8(100, 100, 115, 128);
        let visible_frames = (w / ppf) as i64 + 4;
        let first_tick = ((current_frame - visible_frames / 2) / constants::TICK_GAP_FRAMES)
            * constants::TICK_GAP_FRAMES;
        let mut tf = first_tick;
        loop {
            let x = center_x + (tf - current_frame) as f32 * ppf;
            if x > w {
                break;
            }
            if x >= 0.0 {
                let tick_idx = tf / constants::TICK_GAP_FRAMES;
                let th = if tick_idx % 2 == 0 {
                    tick_long
                } else {
                    tick_short
                };
                if let Some(rect) = SkRect::from_xywh(x, 0.0, tick_w, th) {
                    pixmap.fill_rect(rect, &tick_paint, Transform::identity(), None);
                }
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // -- Playhead --
        let mut playhead_paint = Paint::default();
        playhead_paint.set_color_rgba8(217, 38, 38, 255);
        if let Some(rect) = SkRect::from_xywh(center_x - playhead_w / 2.0, 0.0, playhead_w, h) {
            pixmap.fill_rect(rect, &playhead_paint, Transform::identity(), None);
        }

        // -- Lines (no handles, no border — clean export) --
        let total_slot_h = slot_h + badge_h + badge_gap;

        for line in project.lines() {
            let x1 = center_x + (line.start_frame - current_frame) as f32 * ppf;
            let x2 = center_x + (line.end_frame() - current_frame) as f32 * ppf;
            let lw = (x2 - x1).max(2.0);
            if x1 + lw < 0.0 || x1 > w {
                continue;
            }

            let slot_idx = (line.y_slot * slot_count).round().min(slot_count - 1.0) as usize;
            let y_base = ruler_h + slot_idx as f32 * total_slot_h;

            // Badge
            let [cr, cg, cb, _] = line.character_color;
            let mut badge_paint = Paint::default();
            badge_paint.set_color_rgba8(
                (cr * 255.0) as u8,
                (cg * 255.0) as u8,
                (cb * 255.0) as u8,
                255,
            );
            let badge_w = (line.character_name.chars().count().max(1) as f32 * badge_char_w
                + 12.0 * s)
                .max(16.0 * s);
            if let Some(rect) = SkRect::from_xywh(x1, y_base, badge_w, badge_h) {
                pixmap.fill_rect(rect, &badge_paint, Transform::identity(), None);
            }

            // Badge text
            if !line.character_name.is_empty() {
                let luminance = 0.299 * cr + 0.587 * cg + 0.114 * cb;
                let bf = badge_font;
                let (tex, tw, th) = self.rasterize_text(&line.character_name, bf);
                if tw > 0 && th > 0 {
                    let tx = x1 + (badge_w - tw as f32) / 2.0;
                    let ty = y_base + (badge_h - th as f32) / 2.0;
                    // Blit with color tint
                    let pm_w = pixmap.width() as i32;
                    let pm_h = pixmap.height() as i32;
                    let pm_data = pixmap.data_mut();
                    let (tr, tg, tb) = if luminance > 0.55 {
                        (0u8, 0, 0)
                    } else {
                        (224, 224, 230)
                    };
                    for py in 0..th {
                        for px in 0..tw {
                            let dx = tx as i32 + px as i32;
                            let dy = ty as i32 + py as i32;
                            if dx < 0 || dy < 0 || dx >= pm_w || dy >= pm_h {
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
                            pm_data[di] = ((tr as u32 * a + pm_data[di] as u32 * inv) / 255) as u8;
                            pm_data[di + 1] =
                                ((tg as u32 * a + pm_data[di + 1] as u32 * inv) / 255) as u8;
                            pm_data[di + 2] =
                                ((tb as u32 * a + pm_data[di + 2] as u32 * inv) / 255) as u8;
                            pm_data[di + 3] = (a + (pm_data[di + 3] as u32 * inv) / 255) as u8;
                        }
                    }
                }
            }

            // Line body
            let line_y = y_base + badge_h + badge_gap;

            // Rythmo text, rendered vectorially at final size.
            if !line.text.is_empty() && line.text != "↑" && line.text != "↓" {
                let lang = &crate::config::get().lang;
                let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                let use_segments =
                    !breaks.is_empty() && line.syllable_ratios.len() == breaks.len() + 1;
                if use_segments {
                    let chars: Vec<char> = line.text.chars().collect();
                    let mut seg_x = x1;
                    let mut prev_break = 0usize;
                    for (i, &ratio) in line.syllable_ratios.iter().enumerate() {
                        let seg_w = ratio * lw;
                        let end_break = if i < breaks.len() {
                            breaks[i]
                        } else {
                            chars.len()
                        };
                        let segment: String = chars[prev_break..end_break].iter().collect();
                        if !segment.is_empty() && seg_w > 0.5 {
                            self.blit_rythmo_text(
                                &mut pixmap,
                                &segment,
                                seg_x,
                                line_y,
                                seg_w,
                                slot_h,
                                font_size,
                            );
                        }
                        seg_x += seg_w;
                        prev_break = end_break;
                    }
                } else {
                    self.blit_rythmo_text(
                        &mut pixmap,
                        &line.text,
                        x1,
                        line_y,
                        lw,
                        slot_h,
                        font_size,
                    );
                }
            }

            // Breath arrows
            if line.text == "↑" || line.text == "↓" {
                let up = line.text == "↑";
                let mut arrow_paint = Paint::default();
                arrow_paint.set_color_rgba8(220, 220, 230, 230);
                arrow_paint.anti_alias = true;
                let margin = 4.0;
                let mut pb = PathBuilder::new();
                if up {
                    pb.move_to(x1 + margin, line_y + slot_h - margin);
                    pb.line_to(x1 + lw - margin, line_y + margin);
                } else {
                    pb.move_to(x1 + margin, line_y + margin);
                    pb.line_to(x1 + lw - margin, line_y + slot_h - margin);
                }
                if let Some(path) = pb.finish() {
                    let stroke = tiny_skia::Stroke {
                        width: 2.0,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, &arrow_paint, &stroke, Transform::identity(), None);
                }
            }

            // Note text (discrete, at the bottom of the line)
            if !line.note.is_empty() {
                let note_font = badge_font * 0.9;
                let note_h = (note_font * 1.3).ceil();
                let note_y = line_y + slot_h - note_h - 1.0;
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
                            let sr = (160u32 * a / 255) as u32;
                            let sg = (160u32 * a / 255) as u32;
                            let sb = (170u32 * a / 255) as u32;
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

        // -- Markers --
        for marker in &project.markers {
            let mx = center_x + (marker.frame - current_frame) as f32 * ppf;
            if mx < -10.0 * s || mx > w + 10.0 * s {
                continue;
            }

            match &marker.kind {
                MarkerKind::Boucle => {
                    let mut p = Paint::default();
                    p.set_color_rgba8(217, 38, 38, 230);
                    if let Some(rect) = SkRect::from_xywh(mx - 1.0 * s, 0.0, 2.0 * s, h) {
                        pixmap.fill_rect(rect, &p, Transform::identity(), None);
                    }
                    p.anti_alias = true;
                    let cy = h / 2.0;
                    let arm = 10.0 * s;
                    let mut pb = PathBuilder::new();
                    pb.move_to(mx - arm, cy - arm);
                    pb.line_to(mx + arm, cy + arm);
                    pb.move_to(mx - arm, cy + arm);
                    pb.line_to(mx + arm, cy - arm);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(
                            &path,
                            &p,
                            &tiny_skia::Stroke {
                                width: 2.5 * s,
                                ..Default::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }
                }
                MarkerKind::Out => {
                    let mut p = Paint::default();
                    p.set_color_rgba8(217, 115, 115, 180);
                    if let Some(rect) = SkRect::from_xywh(mx - 1.0 * s, 0.0, 2.0 * s, h) {
                        pixmap.fill_rect(rect, &p, Transform::identity(), None);
                    }
                    p.anti_alias = true;
                    let cy = h / 2.0;
                    let bh = h * 0.15;
                    for offset in &[-5.0_f32, 5.0] {
                        let mut pb = PathBuilder::new();
                        pb.move_to(mx + offset - bh * 0.3, cy - bh);
                        pb.line_to(mx + offset + bh * 0.3, cy + bh);
                        if let Some(path) = pb.finish() {
                            pixmap.stroke_path(
                                &path,
                                &p,
                                &tiny_skia::Stroke {
                                    width: 2.0 * s,
                                    ..Default::default()
                                },
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }
                MarkerKind::SceneChange => {
                    let mut p = Paint::default();
                    p.set_color_rgba8(230, 230, 240, 200);
                    if let Some(rect) = SkRect::from_xywh(mx - 1.0 * s, 0.0, 2.0 * s, h) {
                        pixmap.fill_rect(rect, &p, Transform::identity(), None);
                    }
                }
                MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => {
                    let mut p = Paint::default();
                    p.set_color_rgba8(180, 180, 190, 200);
                    p.anti_alias = true;
                    let is_left = matches!(marker.kind, MarkerKind::LiaisonLeft);
                    let ay = ruler_h / 2.0;
                    let mut pb = PathBuilder::new();
                    if is_left {
                        pb.move_to(mx + 5.0 * s, ay - 4.0 * s);
                        pb.line_to(mx - 3.0 * s, ay);
                        pb.line_to(mx + 5.0 * s, ay + 4.0 * s);
                    } else {
                        pb.move_to(mx - 5.0 * s, ay - 4.0 * s);
                        pb.line_to(mx + 3.0 * s, ay);
                        pb.line_to(mx - 5.0 * s, ay + 4.0 * s);
                    }
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(
                            &path,
                            &p,
                            &tiny_skia::Stroke {
                                width: 1.5 * s,
                                ..Default::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        pixmap.data().to_vec()
    }
}

/// Calculate the BR height in pixels based on used slots.
pub fn br_height(project: &Project, width: u32, br_scale: f32) -> u32 {
    let s = width as f32 / constants::REF_WIDTH * br_scale;
    let used = count_used_slots(project);
    let slot_count = used.max(1) as f32;
    (constants::RULER_HEIGHT * s
        + slot_count
            * (constants::SLOT_HEIGHT * s + constants::BADGE_HEIGHT * s + constants::BADGE_GAP * s))
        .ceil() as u32
}

fn count_used_slots(project: &Project) -> usize {
    let mut slots = std::collections::HashSet::new();
    for line in project.lines() {
        let idx = (line.y_slot * 4.0).round() as i32;
        slots.insert(idx);
    }
    slots.len()
}
