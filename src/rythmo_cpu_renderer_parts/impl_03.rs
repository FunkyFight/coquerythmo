    pub fn render_br(
        &mut self,
        project: &Project,
        current_frame: f64,
        width: u32,
        source_fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
    ) -> Vec<u8> {
        let current_frame = if current_frame.is_finite() {
            current_frame
        } else {
            0.0
        };
        let s = width as f32 / constants::REF_WIDTH * br_scale; // export BR scale factor
        let normal_slot_h = constants::SLOT_HEIGHT * s;
        let ruler_h = constants::RULER_HEIGHT * s;
        let ppf = constants::PIXELS_PER_FRAME * s * crate::config::scroll_speed();
        let tick_long = constants::TICK_LONG * s;
        let tick_short = constants::TICK_SHORT * s;
        let tick_w = BASE_TICK_WIDTH * s;
        let playhead_w = BASE_PLAYHEAD_WIDTH * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let badge_gap = constants::BADGE_GAP * s;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
        let slot_header_h = badge_h.max(actor_icon_size);
        let font_size = constants::RYTHMO_FONT_SIZE * s;
        let badge_font = constants::BADGE_FONT_SIZE * s;
        let geometry = HorizontalRythmoGeometry::new(0.0, width as f32, playhead_w, ppf);
        self.render_index.refresh(project);
        let render_margin_frames = ((source_fps.max(1.0) * 10.0).round() as i64)
            .max(karaoke_adjacent_max_gap_frames(source_fps))
            .max(karaoke_count_in_frames(source_fps))
            .saturating_add(self.render_index.max_duration_frames());
        let scene = RythmoScene::build(
            project,
            &self.render_index,
            SceneOptions {
                frame_window: geometry.visible_frame_window(current_frame, render_margin_frames),
                current_frame,
                source_fps,
                normal_body_height: normal_slot_h,
                slot_header_height: slot_header_h,
                badge_gap,
                scale: s,
                dynamic_track_layout: false,
            },
        );
        let track_layouts = &scene.tracks;
        let height = (ruler_h + rythmo_layout::total_tracks_height(track_layouts)).ceil() as u32;

        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(5, 5, 8, 255));

        let w = width as f32;
        let h = height as f32;

        // -- Ruler ticks --
        let first_tick_frame = geometry.visible_frame_window(current_frame, 0).first;
        let first_tick =
            first_tick_frame.div_euclid(constants::TICK_GAP_FRAMES) * constants::TICK_GAP_FRAMES;
        let mut tf = first_tick;
        loop {
            let x = geometry.frame_x(tf as f64, current_frame);
            if x > w {
                break;
            }
            if x >= 0.0 {
                let tick_idx = tf.div_euclid(constants::TICK_GAP_FRAMES);
                let th = if tick_idx % 2 == 0 {
                    tick_long
                } else {
                    tick_short
                };
                blit_rect(&mut pixmap, x, 0.0, tick_w, th, [100, 100, 115, 128]);
            }
            tf += constants::TICK_GAP_FRAMES;
        }

        // -- Playhead, split around active karaoke lines --
        let playhead_gaps = scene.active_karaoke_skip_ranges(ruler_h, slot_header_h, badge_gap, s);
        blit_playhead_segments(
            &mut pixmap,
            geometry.playhead_left_x,
            playhead_w,
            h,
            &playhead_gaps,
        );

        let prepared_lines: HashMap<u64, PreparedLineGeometry> =
            include!("cpu_render_prepare.rs");

        for scene_line in &scene.lines {
            let line = &scene_line.line;
            let Some(prepared) = prepared_lines.get(&line.id).copied() else {
                continue;
            };
            let karaoke_count_in = scene_line.karaoke_count_in_progress().is_some();
            let Rect {
                x: x1,
                y: line_y,
                width: lw,
                height: body_h,
            } = prepared.line_rect;
            let badge_collision = prepared.badge_collision_rect.unwrap_or(Rect {
                x: x1,
                y: line_y,
                width: 0.0,
                height: body_h,
            });
            let badge_text = prepared.badge_text_rect.unwrap_or(badge_collision);
            let badge_x = badge_collision.x;
            let badge_y = badge_collision.y;
            let badge_w = badge_collision.width;
            let badge_h = badge_collision.height;
            let badge_font_size = prepared.badge_font_size;
            let show_badge =
                line.kind.is_dialogue() && (!line.karaoke || scene_line.character_label_visible);
            let [cr, cg, cb, _] = line.character_color;
            include!("cpu_render_line_content.rs");
            include!("cpu_render_line_decorations.rs");
        }

        include!("cpu_render_markers.rs");
        include!("cpu_render_drawings.rs")
    }
