{
            if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                let ambiance_label = crate::rythmo_line::ambiance_label(&line.character_name);
                let underline_x = badge_x + badge_font_size * 0.25;
                let underline_w = crate::vector_text::measure_rythmo_text_width_standalone(
                    &ambiance_label,
                    badge_font_size,
                )
                .unwrap_or(badge_w)
                .min((badge_x + badge_w - underline_x).max(0.0));
                self.push_rythmo_text_icons_emphasized(
                    &ambiance_label,
                    badge_font_size,
                    badge_text.x,
                    badge_text.y,
                    badge_text.width,
                    badge_text.height,
                    [0.2, 0.55, 1.0, 1.0],
                    &mut all_icons,
                    &mut icon_batches,
                );
                for y_offset in [2.0, 5.5] {
                    quads.push(quad(
                        underline_x,
                        badge_y + badge_h - y_offset * s,
                        underline_w,
                        1.5 * s,
                        0.2,
                        0.55,
                        1.0,
                        1.0,
                    ));
                }
            }

            // Overlap detection uses the same prepared collision rectangle that
            // is later rendered and used for culling.
            let mut badge_hidden = false;
            let mut badge_overlap_alpha = 1.0_f32;
            if show_badge {
                for (&other_id, other) in &prepared_lines {
                    if other_id == line.id {
                        continue;
                    }
                    let other_rect = other.badge_collision_rect.unwrap_or(other.line_rect);
                    let overlap = badge_x < other_rect.x + other_rect.width
                        && badge_x + badge_w > other_rect.x
                        && badge_y < other_rect.y + other_rect.height
                        && badge_y + badge_h > other_rect.y;
                    if overlap {
                        let same_character = scene
                            .project
                            .get_line(other_id)
                            .is_some_and(|other_line| other_line.character_name == line.character_name);
                        if same_character {
                            badge_hidden = true;
                            break;
                        }
                        badge_overlap_alpha = constants::CHARACTER_BADGE_COLLISION_OPACITY;
                    }
                }
            }

            let badge_info = (show_badge && !badge_hidden && !line.character_name.is_empty())
                .then_some((badge_text, badge_collision, badge_font_size, badge_overlap_alpha));

            if !line.text.is_empty() && line.text != "\u{2191}" && line.text != "\u{2193}" {
                let read_highlight_end =
                    if scene.project.settings().highlight_read_word && !line.karaoke {
                        let progress = (current_frame - line.start_frame as f64)
                            / line.duration_frames.max(1) as f64;
                        crate::syllable::read_highlight_end_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            scene.project.syllable_language_code(),
                            progress as f32,
                        )
                    } else {
                        None
                    };
                let scrolling_text_tint = if line.kind.is_ambiance() {
                    [0.95, 0.12, 0.16, 1.0]
                } else if scene.project.settings().scrolling_text_uses_character_color {
                    [
                        line.character_color[0].clamp(0.0, 1.0),
                        line.character_color[1].clamp(0.0, 1.0),
                        line.character_color[2].clamp(0.0, 1.0),
                        1.0,
                    ]
                } else {
                    [1.0; 4]
                };
                if line.kind.is_ambiance() {
                    let reserve = (54.0 * s).min(lw);
                    let (text_x, text_w) =
                        if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                            (x1 + reserve, (lw - reserve).max(1.0))
                        } else {
                            (x1, (lw - reserve).max(1.0))
                        };
                    self.push_rythmo_text_icons_tinted_clipped(
                        &line.text,
                        font_size,
                        text_x,
                        line_y,
                        text_w,
                        body_h,
                        scrolling_text_tint,
                        1.0,
                        &mut all_icons,
                        &mut icon_batches,
                    );
                } else if line.karaoke {
                    let karaoke_font_size =
                        font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
                    self.push_rythmo_text_icons_natural_tinted_clipped(
                        &line.text,
                        karaoke_font_size,
                        x1,
                        line_y,
                        lw,
                        body_h,
                        [1.0, 1.0, 1.0, 1.0],
                        1.0,
                        &mut all_icons,
                        &mut icon_batches,
                    );
                    if let Some(progress) = scene_line.karaoke_progress() {
                        let visual_progress = crate::syllable::visual_progress_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            scene.project.syllable_language_code(),
                            progress,
                        );
                        self.push_rythmo_text_icons_natural_tinted_clipped(
                            &line.text,
                            karaoke_font_size,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            [
                                line.character_color[0].clamp(0.0, 1.0),
                                line.character_color[1].clamp(0.0, 1.0),
                                line.character_color[2].clamp(0.0, 1.0),
                                1.0,
                            ],
                            visual_progress,
                            &mut all_icons,
                            &mut icon_batches,
                        );
                    }
                } else {
                    let lang = scene.project.syllable_language_code();
                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                    let base_ratios =
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
                    let ratios = scene.project.detections().warped_ratios(
                        line.id,
                        &line.text,
                        &breaks,
                        &base_ratios,
                        line.start_frame,
                        line.duration_frames,
                    );

                    if !ratios.is_empty() {
                        let chars: Vec<char> = line.text.chars().collect();
                        let mut seg_x = x1;
                        let mut prev_break = 0usize;
                        for (i, &ratio) in ratios.iter().enumerate() {
                            let seg_w = ratio * lw;
                            let end_break = if i < breaks.len() {
                                breaks[i]
                            } else {
                                chars.len()
                            };
                            let segment: String = chars[prev_break..end_break].iter().collect();
                            if !segment.is_empty() && seg_w > 0.5 {
                                self.push_read_word_text_icons(
                                    &segment,
                                    font_size,
                                    seg_x,
                                    line_y,
                                    seg_w,
                                    body_h,
                                    prev_break,
                                    read_highlight_end,
                                    scrolling_text_tint,
                                    &mut all_icons,
                                    &mut icon_batches,
                                );
                            }
                            seg_x += seg_w;
                            prev_break = end_break;
                        }
                    } else {
                        self.push_read_word_text_icons(
                            &line.text,
                            font_size,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            0,
                            read_highlight_end,
                            scrolling_text_tint,
                            &mut all_icons,
                            &mut icon_batches,
                        );
                    }
                }
            }
}
