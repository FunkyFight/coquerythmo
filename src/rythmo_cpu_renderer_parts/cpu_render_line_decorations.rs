{
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
            for (&oid, other) in &prepared_lines {
                if oid == line.id {
                    continue;
                }
                let other_rect = other.line_rect;
                let overlap = badge_x < other_rect.x + other_rect.width
                    && badge_x + badge_w > other_rect.x
                    && badge_y < other_rect.y + other_rect.height
                    && badge_y + badge_h > other_rect.y;
                if overlap {
                    let same_character = project
                        .get_line(oid)
                        .is_some_and(|other_line| other_line.character_name == line.character_name);
                    if same_character {
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
                let underline_x = badge_x + badge_font_size * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &line.character_name,
                    badge_font_size,
                )
                .unwrap_or(badge_w)
                .min((badge_x + badge_w - underline_x).max(0.0));
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    &line.character_name,
                    badge_text.x,
                    badge_text.y,
                    badge_text.width,
                    badge_text.height,
                    badge_font_size,
                    [
                        color_channel(cr),
                        color_channel(cg),
                        color_channel(cb),
                        badge_overlap_alpha,
                    ],
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
                    scene_line.karaoke_count_in_progress(),
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
