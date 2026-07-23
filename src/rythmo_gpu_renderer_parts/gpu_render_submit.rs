{
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
