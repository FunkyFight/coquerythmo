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
            text, font_size, full_w, dest_h, tile_x, tile_w, true, false,
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
            text, font_size, full_w, dest_h, tile_x, tile_w, false, false,
        )
    }
    fn get_or_upload_rythmo_text_tile_emphasized(
        &mut self,
        text: &str,
        font_size: f32,
        full_w: u32,
        dest_h: u32,
        tile_x: u32,
        tile_w: u32,
    ) -> u64 {
        self.get_or_upload_rythmo_text_tile_with_mode(
            text, font_size, full_w, dest_h, tile_x, tile_w, false, true,
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
        emphasized: bool,
    ) -> u64 {
        let tile_w = tile_w.min(full_w.saturating_sub(tile_x)).max(1);
        let kind = if emphasized {
            "vector-rythmo-tile-emphasized"
        } else if stretch {
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
        let rendered = if emphasized {
            // Ambiance labels are short; they fit one tile. Rendering the
            // complete styled SVG preserves the selected user font.
            crate::vector_text::render_rythmo_text_natural_emphasized(
                &mut self.font_system,
                text,
                font_size,
                full_w,
                dest_h,
            )
        } else if stretch {
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
        base_tint: [f32; 4],
        all_icons: &mut Vec<IconInstance>,
        icon_batches: &mut Vec<IconBatch>,
    ) {
        let count = text.chars().count();
        let Some(highlight_end) = highlight_end else {
            self.push_rythmo_text_icons_tinted_clipped(
                text,
                font_size,
                x,
                y,
                w,
                h,
                base_tint,
                1.0,
                all_icons,
                icon_batches,
            );
            return;
        };
        if count == 0 || highlight_end <= segment_start {
            self.push_rythmo_text_icons_tinted_clipped(
                text,
                font_size,
                x,
                y,
                w,
                h,
                base_tint,
                1.0,
                all_icons,
                icon_batches,
            );
            return;
        }
        let end_ratio = ((highlight_end - segment_start) as f32 / count as f32).min(1.0);
        if end_ratio < 1.0 {
            self.push_rythmo_text_icons_tinted_clipped(
                text,
                font_size,
                x,
                y,
                w,
                h,
                base_tint,
                1.0,
                all_icons,
                icon_batches,
            );
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
            false,
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
            false,
            all_icons,
            icon_batches,
        );
    }

