

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
