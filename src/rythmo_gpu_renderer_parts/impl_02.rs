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
        timeline_origin_local_x: f32,
        ppf: f32,
    ) -> bool {
        let (first_frame, last_frame) = crate::rythmo_drawing::visible_frame_window_with_origin(
            width as f32,
            timeline_origin_local_x,
            current_frame,
            ppf,
            4,
        );
        let strokes: Vec<_> = scene
            .drawings
            .iter()
            .filter(|stroke| stroke.intersects_window(first_frame, last_frame))
            .collect();
        if strokes.is_empty() {
            return false;
        }

        let rgba = crate::rythmo_drawing::rasterize_window_with_origin(
            &strokes,
            width,
            height,
            timeline_origin_local_x,
            current_frame,
            ppf,
        );
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

