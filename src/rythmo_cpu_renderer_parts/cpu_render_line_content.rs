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
                self.blit_rythmo_text_natural_emphasized_tinted(
                    &mut pixmap,
                    &ambiance_label,
                    badge_text.x,
                    badge_text.y,
                    badge_text.width,
                    badge_text.height,
                    badge_font_size,
                    [51, 140, 255, 255],
                );
                blit_rect(
                    &mut pixmap,
                    underline_x,
                    badge_y + badge_h - 2.0 * s,
                    underline_w,
                    1.5 * s,
                    [51, 140, 255, 255],
                );
                blit_rect(
                    &mut pixmap,
                    underline_x,
                    badge_y + badge_h - 5.5 * s,
                    underline_w,
                    1.5 * s,
                    [51, 140, 255, 255],
                );
            }

            // Rythmo text, rendered vectorially at final size.
            if !line.text.is_empty() && line.text != "↑" && line.text != "↓" {
                let read_highlight_end = if project.settings().highlight_read_word && !line.karaoke
                {
                    let progress = (current_frame - line.start_frame as f64)
                        / line.duration_frames.max(1) as f64;
                    crate::syllable::read_highlight_end_from_timing(
                        &line.text,
                        &line.syllable_ratios,
                        scene.syllable_language.code(),
                        progress as f32,
                    )
                } else {
                    None
                };
                let scrolling_text_tint = if line.kind.is_ambiance() {
                    [242, 31, 41]
                } else if project.settings().scrolling_text_uses_character_color {
                    [
                        color_channel(line.character_color[0]),
                        color_channel(line.character_color[1]),
                        color_channel(line.character_color[2]),
                    ]
                } else {
                    [255; 3]
                };
                if line.kind.is_ambiance() {
                    let reserve = (54.0 * s).min(lw);
                    let (text_x, text_w) =
                        if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
                            (x1 + reserve, (lw - reserve).max(1.0))
                        } else {
                            (x1, (lw - reserve).max(1.0))
                        };
                    self.blit_rythmo_text_tinted_clipped(
                        &mut pixmap,
                        &line.text,
                        text_x,
                        line_y,
                        text_w,
                        body_h,
                        font_size,
                        scrolling_text_tint,
                        1.0,
                    );
                } else if line.karaoke {
                    let karaoke_font_size =
                        font_size * constants::KARAOKE_TEXT_FONT_SCALE * karaoke_text_scale;
                    self.blit_rythmo_text_natural_tinted_clipped(
                        &mut pixmap,
                        &line.text,
                        x1,
                        line_y,
                        lw,
                        body_h,
                        karaoke_font_size,
                        [255, 255, 255],
                        1.0,
                    );
                    if let Some(progress) = scene_line.karaoke_progress() {
                        let visual_progress = crate::syllable::visual_progress_from_timing(
                            &line.text,
                            &line.syllable_ratios,
                            scene.syllable_language.code(),
                            progress,
                        );
                        self.blit_rythmo_text_natural_tinted_clipped(
                            &mut pixmap,
                            &line.text,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            karaoke_font_size,
                            [
                                color_channel(line.character_color[0]),
                                color_channel(line.character_color[1]),
                                color_channel(line.character_color[2]),
                            ],
                            visual_progress,
                        );
                    }
                } else {
                    let lang = scene.syllable_language.code();
                    let breaks = crate::syllable::syllable_breaks(&line.text, lang);
                    let base_ratios =
                        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
                    let ratios = project.detections().warped_ratios(
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
                                self.blit_read_word_text(
                                    &mut pixmap,
                                    &segment,
                                    seg_x,
                                    line_y,
                                    seg_w,
                                    body_h,
                                    font_size,
                                    prev_break,
                                    read_highlight_end,
                                    scrolling_text_tint,
                                );
                            }
                            seg_x += seg_w;
                            prev_break = end_break;
                        }
                    } else {
                        self.blit_read_word_text(
                            &mut pixmap,
                            &line.text,
                            x1,
                            line_y,
                            lw,
                            body_h,
                            font_size,
                            0,
                            read_highlight_end,
                            scrolling_text_tint,
                        );
                    }
                }
            }
}
