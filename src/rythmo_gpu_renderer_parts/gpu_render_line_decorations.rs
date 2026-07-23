{
            if !line.presence.is_on() && !line.text.is_empty() {
                let underline_y = line_y + body_h - (3.0 * s).max(1.0);
                let thickness = (1.5 * s).max(1.0);
                if line.presence == crate::rythmo_line::LinePresence::Off {
                    quads.push(quad(x1, underline_y, lw, thickness, 1.0, 1.0, 1.0, 1.0));
                } else {
                    let (dash, gap) = ((8.0 * s).max(2.0), (5.0 * s).max(2.0));
                    let mut x = x1;
                    while x < x1 + lw {
                        quads.push(quad(
                            x,
                            underline_y,
                            dash.min(x1 + lw - x),
                            thickness,
                            1.0,
                            1.0,
                            1.0,
                            1.0,
                        ));
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
                    let dx = tip_x - base_x;
                    quads.push(rotated_line(
                        base_x + dx * 0.5,
                        cy + dy * 0.5,
                        (dx * dx + dy * dy).sqrt(),
                        5.0 * s,
                        (-dy).atan2(dx),
                        1.0,
                        1.0,
                        1.0,
                        1.0,
                    ));
                }
                quads.push(quad(
                    gx + 5.0 * s,
                    cy - 2.5 * s,
                    (gutter - 10.0 * s).max(1.0),
                    5.0 * s,
                    1.0,
                    1.0,
                    1.0,
                    1.0,
                ));
                let bar_x = if at_start {
                    gx + 3.0 * s
                } else {
                    gx + gutter - 3.0 * s
                };
                quads.push(quad(
                    bar_x - 2.5 * s,
                    cy - 13.0 * s,
                    5.0 * s,
                    26.0 * s,
                    1.0,
                    1.0,
                    1.0,
                    1.0,
                ));
            }

            // Draw the character label after the scrolling text, using the
            // exact prepared text and collision rectangles.
            if let Some((text_rect, collision_rect, label_font_size, ba)) = badge_info {
                let underline_x = collision_rect.x + label_font_size * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &line.character_name,
                    label_font_size,
                )
                .unwrap_or(collision_rect.width)
                .min((collision_rect.x + collision_rect.width - underline_x).max(0.0));
                self.push_rythmo_text_icons_emphasized(
                    &line.character_name,
                    label_font_size,
                    text_rect.x,
                    text_rect.y,
                    text_rect.width,
                    text_rect.height,
                    [cr, cg, cb, ba],
                    &mut all_icons,
                    &mut icon_batches,
                );
                for y_offset in [2.0, 5.5] {
                    quads.push(quad(
                        underline_x,
                        collision_rect.y + collision_rect.height - y_offset * s,
                        underline_w,
                        1.5 * s,
                        cr,
                        cg,
                        cb,
                        ba,
                    ));
                }

                self.push_voice_actor_icons(
                    scene,
                    line,
                    collision_rect.x,
                    collision_rect.y,
                    collision_rect.width,
                    actor_icon_size,
                    s,
                    w,
                    &mut quads,
                    &mut all_icons,
                    &mut icon_batches,
                );
            }

            if line.text == "\u{2191}" || line.text == "\u{2193}" {
                let up = line.text == "\u{2191}";
                let margin = 4.0;
                if lw > margin * 2.0 + 1.0 && body_h > margin * 2.0 + 1.0 {
                    let dx = lw - margin * 2.0;
                    let dy = body_h - margin * 2.0;
                    let length = (dx * dx + dy * dy).sqrt();
                    let cx = x1 + lw / 2.0;
                    let cy = line_y + body_h / 2.0;
                    let angle = if up { (-dy).atan2(dx) } else { dy.atan2(dx) };
                    quads.push(rotated_line(
                        cx,
                        cy,
                        length,
                        2.0 * s,
                        angle,
                        220.0 / 255.0,
                        220.0 / 255.0,
                        230.0 / 255.0,
                        230.0 / 255.0,
                    ));
                }
            }

            if karaoke_count_in {
                push_karaoke_count_in_dot(
                    &mut quads,
                    line,
                    x1,
                    line_y,
                    scene_line.karaoke_count_in_progress(),
                    s,
                );
            } else {
                push_karaoke_dot(
                    &mut quads,
                    line,
                    scene.project.syllable_language_code(),
                    current_frame,
                    x1,
                    line_y,
                    lw,
                    s,
                );
            }

            // Note text (discrete, gray, at the bottom of the line)
            if !line.note.is_empty() {
                let note_font = badge_font * 0.9;
                let hash = self.get_or_upload_text(&line.note, note_font);
                if let Some(cached) = self.text_cache.get(&hash) {
                    let tw = cached.width as f32;
                    let _th = cached.height as f32;
                    let note_h = (note_font * 1.3).ceil();
                    let note_y = line_y + body_h - note_h - 1.0;
                    let max_note_w = lw - 8.0 * s;
                    let draw_w = tw.min(max_note_w);
                    let uv_end = (draw_w / tw).min(1.0);
                    let start = all_icons.len() as u32;
                    all_icons.push(IconInstance {
                        rect: [x1 + 4.0 * s, note_y, draw_w, note_h],
                        uv_rect: [0.0, 0.0, uv_end, 1.0],
                        tint: [160.0 / 255.0, 160.0 / 255.0, 170.0 / 255.0, 1.0],
                    });
                    icon_batches.push(IconBatch {
                        hash,
                        start,
                        count: 1,
                    });
                }
            }
}
