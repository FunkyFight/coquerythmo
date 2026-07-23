    fn push_rythmo_text_icons_emphasized(
        &mut self,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tint: [f32; 4],
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
            1.0,
            false,
            true,
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
        emphasized: bool,
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
            let hash = if emphasized {
                self.get_or_upload_rythmo_text_tile_emphasized(
                    text, font_size, full_w, full_h, tile_x, tile_w,
                )
            } else if stretch {
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

