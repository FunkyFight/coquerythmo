
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

