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
        tint: [u8; 4],
    ) {
        self.blit_rythmo_text_tinted_clipped_with_mode_alpha(
            pixmap,
            text,
            x,
            y,
            dest_w,
            dest_h,
            font_size,
            [tint[0], tint[1], tint[2]],
            1.0,
            false,
            true,
            tint[3] as f32 / 255.0,
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
        self.blit_rythmo_text_tinted_clipped_with_mode_alpha(
            pixmap, text, x, y, dest_w, dest_h, font_size, tint, clip_ratio, stretch,
            emphasized, 1.0,
        );
    }

    fn blit_rythmo_text_tinted_clipped_with_mode_alpha(
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
        opacity: f32,
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

                let sa = (rendered.pixels[src_idx + 3] as f32 * opacity.clamp(0.0, 1.0)).round() as u32;
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

    /// Render the bande rythmo for a fractional source-frame position.
    ///
    /// Integer frame bounds are used only for visibility queries; every visual
    /// position keeps the fractional component so a 24 fps project can scroll
    /// smoothly in a 60 fps export.
