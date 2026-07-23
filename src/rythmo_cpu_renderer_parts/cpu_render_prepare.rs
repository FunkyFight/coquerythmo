{
        // -- Lines (no handles, no border -- clean export) --
        // Prepare each geometry once, then reuse the exact same rectangles for
        // culling, collision policy and rasterization.
        let mut prepared_lines: HashMap<u64, PreparedLineGeometry> = HashMap::new();
        for scene_line in &scene.lines {
            let line = &scene_line.line;
            if line.karaoke && !scene_line.karaoke_should_be_visible() {
                continue;
            }

            let Some(track) = rythmo_layout::track_for_y_slot(track_layouts, line.y_slot) else {
                continue;
            };
            let body_y = ruler_h + track.top + slot_header_h + badge_gap;
            let (line_y, body_h) = if line.karaoke {
                (
                    karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, s),
                    karaoke_stack_height(track.body_h, s),
                )
            } else {
                (body_y, normal_slot_h)
            };

            let (line_x, line_width) = if scene_line.karaoke_should_be_centered() {
                let width = self.karaoke_text_width(&line.text, font_size, karaoke_text_scale);
                (geometry.centered_karaoke_x(width), width)
            } else {
                (
                    geometry.frame_x(line.start_frame as f64, current_frame),
                    (line.duration_frames as f32 * ppf).max(2.0),
                )
            };
            let line_rect = Rect {
                x: line_x,
                y: line_y,
                width: line_width,
                height: body_h,
            };

            let show_badge =
                line.kind.is_dialogue() && (!line.karaoke || scene_line.character_label_visible);
            let ambiance_label = matches!(
                line.kind,
                crate::rythmo_line::RythmoLineKind::AmbianceStart
            )
            .then(|| crate::rythmo_line::ambiance_label(&line.character_name));
            let label_text = ambiance_label.as_deref().or_else(|| {
                (show_badge && !line.character_name.is_empty()).then_some(line.character_name.as_str())
            });

            let (badge_collision_rect, badge_text_rect, badge_font_size, badge_gap_px) =
                if let Some(label_text) = label_text {
                    let metrics = character_label_metrics(label_text, body_h, s, ppf);
                    let badge_x = if ambiance_label.is_some() {
                        ambiance_character_label_x(line_x, metrics.width)
                    } else if scene_line.karaoke_should_be_centered() {
                        centered_karaoke_character_label_x(line_rect, metrics.width, s)
                    } else {
                        normal_character_label_x(line_x, metrics.width, ppf)
                    };
                    let rects = character_label_rects(badge_x, line_y, body_h, metrics);
                    (
                        Some(rects.collision_rect),
                        Some(rects.text_draw_rect),
                        metrics.font_size,
                        if ambiance_label.is_some() {
                            0.0
                        } else if scene_line.karaoke_should_be_centered() {
                            metrics.centered_karaoke_gap
                        } else {
                            metrics.normal_gap
                        },
                    )
                } else {
                    (None, None, 0.0, 0.0)
                };

            let leading_visual = badge_collision_rect.map(|badge| {
                rythmo_layout::leading_visual_bounds(
                    badge.x,
                    badge.width,
                    if !line.karaoke {
                        line.voice_actor_names.len()
                    } else {
                        0
                    },
                    actor_icon_size,
                    3.0 * s,
                )
            });
            if !rythmo_layout::line_or_badge_intersects_viewport(
                line_rect.x,
                line_rect.width,
                leading_visual,
                geometry.viewport_left,
                geometry.viewport_right(),
            ) {
                continue;
            }

            prepared_lines.insert(
                line.id,
                PreparedLineGeometry {
                    line_rect,
                    badge_collision_rect,
                    badge_text_rect,
                    badge_font_size,
                    badge_scale: s,
                    badge_gap: badge_gap_px,
                },
            );
        }
        prepared_lines
}
