//! Pure coordinate and editor layout calculations for the rythmo workspace.

use super::*;

pub(crate) fn ppf() -> f32 {
    constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

pub(crate) fn f64_floor_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.floor() as i64
    }
}

pub(crate) fn f64_ceil_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.ceil() as i64
    }
}

pub(crate) fn f64_round_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.round() as i64
    }
}

pub(crate) fn visual_frame_to_i64(current_frame: f64) -> i64 {
    f64_floor_to_i64(current_frame)
}

pub(crate) fn render_window(
    zone: &Rect,
    current_frame: f64,
    margin_frames: i64,
    fps: f64,
) -> (i64, i64) {
    let half_visible_frames = zone.width as f64 / ppf().max(0.001) as f64 / 2.0;
    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    let margin_frames = margin_frames.max(0);
    let first_frame = f64_floor_to_i64(current_frame - half_visible_frames + offset_frames)
        .saturating_sub(margin_frames);
    let last_frame = f64_ceil_to_i64(current_frame + half_visible_frames + offset_frames)
        .saturating_add(margin_frames);
    (first_frame, last_frame.max(first_frame))
}

pub(crate) fn interactive_render_margin_frames(
    fps: f64,
    _render_index: &ProjectRenderIndex,
) -> i64 {
    let fps = fps.max(1.0);
    // ProjectRenderIndex::visible_line_ids already accounts for lines that
    // started before the window but still overlap it. Adding the longest line
    // duration here expanded both sides a second time and could turn viewport
    // culling into an almost full-project scan.
    karaoke_adjacent_max_gap_frames(fps)
        .max(karaoke_count_in_frames(fps))
        .max((fps * 10.0).round() as i64)
}

pub(crate) fn frame_to_x(frame: i64, current_frame: f64, zone: &Rect, fps: f64) -> f32 {
    let center_x = zone.x + zone.width / 2.0;
    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    center_x - offset_frames as f32 * ppf() + (frame as f64 - current_frame) as f32 * ppf()
}

pub(crate) fn x_to_frame(x: f32, current_frame: f64, zone: &Rect, fps: f64) -> i64 {
    let center_x = zone.x + zone.width / 2.0;
    let offset_frames = crate::config::reading_bar_offset_seconds() * fps;
    let origin = center_x - offset_frames as f32 * ppf();
    f64_round_to_i64(current_frame + (x - origin) as f64 / ppf().max(0.001) as f64)
}

pub(crate) fn clamped_new_line_duration(
    project: &Project,
    frame: i64,
    y_slot: f32,
    fps: f64,
) -> i64 {
    let default_dur = (fps * constants::DEFAULT_LINE_DURATION_SEC) as i64;
    project
        .lines()
        .filter(|line| (line.y_slot - y_slot).abs() < 0.01 && line.start_frame > frame)
        .map(|line| line.start_frame)
        .min()
        .map(|start| (start - frame - constants::TICK_GAP_FRAMES).clamp(1, default_dur))
        .unwrap_or(default_dur)
}

#[cfg(test)]
pub(crate) fn y_to_slot(project: &Project, y: f32, zone: &Rect) -> f32 {
    y_to_slot_at_frame(project, y, 0.0, zone)
}

pub(crate) fn y_to_slot_at_frame(
    project: &Project,
    y: f32,
    current_frame: f64,
    zone: &Rect,
) -> f32 {
    let relative_y = y - zone.y - constants::RULER_HEIGHT;
    let layouts = editor_track_layouts_at_frame(project, current_frame, zone);
    let layout = layouts
        .iter()
        .find(|layout| relative_y < layout.top + layout.reserved_h)
        .or_else(|| layouts.last())
        .unwrap_or_else(|| {
            layouts
                .first()
                .expect("editor track layout should not be empty")
        });
    rythmo_layout::y_slot_for_track_index(layout.track_index)
}

pub(crate) fn karaoke_ui_font_size() -> f32 {
    crate::config::get().ui.font_size * 2.0 * constants::KARAOKE_TEXT_FONT_SCALE
}

pub(crate) fn hash_karaoke_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn measure_karaoke_ui_text_width(text: &str, font_size: f32) -> f32 {
    crate::vector_text::measure_rythmo_text_width_standalone(text, font_size)
        .map(|width| width.ceil() + 1.0)
        .unwrap_or_else(|| estimate_karaoke_ui_text_width(text, font_size))
        .max(2.0)
}

pub(crate) fn estimate_karaoke_ui_text_width(text: &str, font_size: f32) -> f32 {
    let char_count = text.chars().count().max(1) as f32;
    (char_count * font_size * 0.62 + font_size * 0.7).max(2.0)
}

pub(crate) fn karaoke_ui_text_width(text: &str) -> f32 {
    measure_karaoke_ui_text_width(text, karaoke_ui_font_size())
}

pub(crate) fn line_visual_x_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> (f32, f32) {
    line_visual_x_width_with_karaoke_width(
        line,
        current_frame,
        zone,
        karaoke_preview,
        None,
        reading_bar_offset_seconds,
        fps,
    )
}

pub(crate) fn line_visual_x_width_with_karaoke_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    active_karaoke_width: Option<f32>,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> (f32, f32) {
    let center_x = zone.x + zone.width / 2.0;
    let offset_frames = reading_bar_offset_seconds * fps;
    if karaoke_preview && line.karaoke_active(current_frame) {
        let width = active_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        return (center_x - width / 2.0, width);
    } else if karaoke_preview {
        return line.visual_x_width(
            current_frame,
            center_x,
            ppf(),
            zone.width,
            1.0,
            offset_frames,
        );
    }

    let x1 = frame_to_x(line.start_frame, current_frame, zone, fps);
    let width = (line.duration_frames as f32 * ppf()).max(2.0);
    (x1, width)
}

pub(crate) fn badge_rect_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    badge_rect_for_line_with_karaoke_preview(
        project,
        line,
        current_frame,
        zone,
        false,
        reading_bar_offset_seconds,
        fps,
    )
}

pub(crate) fn badge_rect_for_line_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(
        project,
        line,
        &line.character_name,
        current_frame,
        zone,
        karaoke_preview,
        reading_bar_offset_seconds,
        fps,
    )
}

pub(crate) fn badge_rect_for_name(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(
        project,
        line,
        name,
        current_frame,
        zone,
        false,
        reading_bar_offset_seconds,
        fps,
    )
}

pub(crate) fn badge_rect_for_name_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    let line_rect = line_rect_with_karaoke_preview(
        project,
        line,
        current_frame,
        zone,
        karaoke_preview,
        reading_bar_offset_seconds,
        fps,
    );
    let w = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
        ambiance_badge_width(name)
    } else {
        badge_width(name)
    };
    // Dialogue badges keep the traditional four-frame breathing room. An
    // ambiance name belongs directly to the liaison at the start of its line.
    let right = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
        line_rect.x
    } else {
        line_rect.x - 4.0 * ppf()
    };
    Rect {
        x: right - w,
        y: line_rect.y,
        width: w,
        height: line_rect.height,
    }
}

pub(crate) fn color_picker_origin_for_badge(badge: &Rect, zone: &Rect) -> (f32, f32) {
    let (picker_w, picker_h) = crate::ui::color_picker::ColorPickerState::panel_size();
    let gap = 10.0;
    let zone_right = zone.x + zone.width;
    let zone_bottom = zone.y + zone.height;

    let right_x = badge.x + badge.width + gap;
    let x = if right_x + picker_w <= zone_right {
        right_x
    } else {
        (badge.x - picker_w - gap).max(zone.x)
    };

    let y = (badge.y - 8.0).clamp(zone.y, (zone_bottom - picker_h).max(zone.y));
    (x, y)
}

#[cfg(test)]
pub(crate) fn collect_track_usage(project: &Project) -> (Vec<bool>, Vec<bool>) {
    let track_count = rythmo_layout::track_count();
    let mut used_tracks = vec![false; track_count];
    let mut karaoke_tracks = vec![false; track_count];
    for line in project.lines() {
        let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
        used_tracks[track_index] = true;
        if line.karaoke {
            karaoke_tracks[track_index] = true;
        }
    }

    if !used_tracks.iter().any(|used| *used) && !used_tracks.is_empty() {
        used_tracks[0] = true;
    }

    (used_tracks, karaoke_tracks)
}

pub(crate) fn build_track_layouts_from_karaoke_flags(
    track_indices: &[usize],
    karaoke_tracks: &[bool],
    reserved_karaoke_tracks: &[bool],
    emotion_tracks: &[bool],
    normal_body_h: f32,
    slot_header_h: f32,
    badge_gap: f32,
    scale: f32,
) -> Vec<rythmo_layout::TrackLayout> {
    let mut top = 0.0;
    track_indices
        .iter()
        .map(|&track_index| {
            let has_karaoke = karaoke_tracks.get(track_index).copied().unwrap_or(false);
            let body_h = if has_karaoke {
                rythmo_layout::karaoke_track_body_height(normal_body_h, scale)
            } else if emotion_tracks.get(track_index).copied().unwrap_or(false) {
                rythmo_layout::text_emotion_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let total_h = slot_header_h + badge_gap + body_h;
            let reserved_body_h = if reserved_karaoke_tracks
                .get(track_index)
                .copied()
                .unwrap_or(false)
            {
                rythmo_layout::karaoke_track_body_height(normal_body_h, scale)
            } else if emotion_tracks.get(track_index).copied().unwrap_or(false) {
                rythmo_layout::text_emotion_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let reserved_h = slot_header_h + badge_gap + reserved_body_h;
            let layout = rythmo_layout::TrackLayout {
                track_index,
                top,
                total_h,
                reserved_h,
                body_h,
                has_karaoke,
            };
            top += reserved_h;
            layout
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn editor_normal_body_height_for_karaoke_tracks(
    karaoke_track_count: usize,
    zone: &Rect,
) -> f32 {
    editor_normal_body_height(karaoke_track_count, 0, zone)
}

fn editor_normal_body_height(
    karaoke_track_count: usize,
    emotion_track_count: usize,
    zone: &Rect,
) -> f32 {
    editor_normal_body_height_for_track_count(
        karaoke_track_count,
        emotion_track_count,
        rythmo_layout::track_count(),
        zone,
    )
}

fn editor_normal_body_height_for_track_count(
    karaoke_track_count: usize,
    emotion_track_count: usize,
    track_count: usize,
    zone: &Rect,
) -> f32 {
    let track_count = track_count.max(1);
    let usable_h = (zone.height - constants::RULER_HEIGHT).max(1.0);
    let header_total = track_count as f32 * (slot_header_height() + BADGE_GAP);
    let weighted_rows = (track_count + karaoke_track_count + emotion_track_count) as f32;
    let mut body_h = ((usable_h - header_total) / weighted_rows).max(8.0);
    for _ in 0..4 {
        let stack_gaps = karaoke_track_count as f32
            * rythmo_layout::karaoke_stack_gap(body_h * 2.0, 1.0)
            + emotion_track_count as f32 * rythmo_layout::karaoke_stack_gap(body_h, 1.0);
        body_h = ((usable_h - header_total - stack_gaps) / weighted_rows).max(8.0);
    }
    body_h
}

pub(crate) struct EditorLayoutCtx {
    pub(crate) normal_body_h: f32,
    track_layouts: Vec<rythmo_layout::TrackLayout>,
    track_by_index: Vec<Option<rythmo_layout::TrackLayout>>,
}

impl EditorLayoutCtx {
    #[cfg(test)]
    pub(crate) fn new(project: &Project, zone: &Rect) -> Self {
        let (_, karaoke_tracks) = collect_track_usage(project);
        Self::from_karaoke_tracks(&karaoke_tracks, zone)
    }

    pub(crate) fn new_at_frame(project: &Project, current_frame: f64, zone: &Rect) -> Self {
        Self::new_at_frame_with_fps(project, current_frame, 24.0, zone)
    }

    pub(crate) fn new_at_frame_with_fps(
        project: &Project,
        current_frame: f64,
        fps: f64,
        zone: &Rect,
    ) -> Self {
        Self::new_at_frame_with_fps_for_tracks(
            project,
            current_frame,
            fps,
            zone,
            &rythmo_layout::all_track_indices(),
        )
    }

    pub(crate) fn new_at_frame_with_fps_for_tracks(
        project: &Project,
        current_frame: f64,
        fps: f64,
        zone: &Rect,
        track_indices: &[usize],
    ) -> Self {
        let track_indices = if track_indices.is_empty() {
            &[0]
        } else {
            track_indices
        };
        let karaoke_mode_tracks = rythmo_layout::karaoke_mode_tracks(
            project,
            current_frame,
            karaoke_count_in_frames(fps),
        );
        let reserved_karaoke_tracks = rythmo_layout::karaoke_tracks(project);
        let emotion_tracks = rythmo_layout::text_emotion_tracks(project);
        Self::new_for_indexed_tracks(
            zone,
            track_indices,
            &karaoke_mode_tracks,
            &reserved_karaoke_tracks,
            &emotion_tracks,
        )
    }

    pub(crate) fn new_for_indexed_tracks(
        zone: &Rect,
        track_indices: &[usize],
        karaoke_mode_tracks: &[bool],
        reserved_karaoke_tracks: &[bool],
        emotion_tracks: &[bool],
    ) -> Self {
        let track_indices = if track_indices.is_empty() {
            &[0]
        } else {
            track_indices
        };
        let karaoke_track_count = reserved_karaoke_tracks
            .iter()
            .enumerate()
            .filter(|(index, has_karaoke)| track_indices.contains(index) && **has_karaoke)
            .count();
        let emotion_track_count = emotion_tracks
            .iter()
            .enumerate()
            .filter(|(index, has_emotion)| {
                track_indices.contains(index)
                    && **has_emotion
                    && !reserved_karaoke_tracks
                        .get(*index)
                        .copied()
                        .unwrap_or(false)
            })
            .count();
        let normal_body_h = editor_normal_body_height_for_track_count(
            karaoke_track_count,
            emotion_track_count,
            track_indices.len(),
            zone,
        );
        let track_layouts = build_track_layouts_from_karaoke_flags(
            track_indices,
            &karaoke_mode_tracks,
            &reserved_karaoke_tracks,
            &emotion_tracks,
            normal_body_h,
            slot_header_height(),
            BADGE_GAP,
            1.0,
        );
        Self::from_track_layouts(normal_body_h, track_layouts)
    }

    #[cfg(test)]
    pub(crate) fn from_karaoke_tracks(karaoke_tracks: &[bool], zone: &Rect) -> Self {
        let karaoke_track_count = karaoke_tracks
            .iter()
            .filter(|has_karaoke| **has_karaoke)
            .count();
        let normal_body_h = editor_normal_body_height_for_karaoke_tracks(karaoke_track_count, zone);
        let track_layouts = build_track_layouts_from_karaoke_flags(
            &rythmo_layout::all_track_indices(),
            karaoke_tracks,
            karaoke_tracks,
            &vec![false; rythmo_layout::track_count()],
            normal_body_h,
            slot_header_height(),
            BADGE_GAP,
            1.0,
        );
        Self::from_track_layouts(normal_body_h, track_layouts)
    }

    pub(crate) fn from_track_layouts(
        normal_body_h: f32,
        track_layouts: Vec<rythmo_layout::TrackLayout>,
    ) -> Self {
        let mut track_by_index = vec![None; rythmo_layout::track_count()];
        for layout in &track_layouts {
            if let Some(slot) = track_by_index.get_mut(layout.track_index) {
                *slot = Some(*layout);
            }
        }

        Self {
            normal_body_h,
            track_layouts,
            track_by_index,
        }
    }

    pub(crate) fn track_for_index(
        &self,
        track_index: usize,
    ) -> Option<&rythmo_layout::TrackLayout> {
        self.track_by_index
            .get(track_index)
            .and_then(|layout| layout.as_ref())
    }

    pub(crate) fn track_layouts(&self) -> &[rythmo_layout::TrackLayout] {
        &self.track_layouts
    }

    pub(crate) fn track_for_y_slot(&self, y_slot: f32) -> &rythmo_layout::TrackLayout {
        let track_index = rythmo_layout::track_index_for_y_slot(y_slot);
        self.track_for_index(track_index).unwrap_or_else(|| {
            self.track_layouts
                .first()
                .expect("editor track layout should not be empty")
        })
    }

    pub(crate) fn track_body_rect(&self, y_slot: f32, zone: &Rect) -> Rect {
        let layout = self.track_for_y_slot(y_slot);
        Rect {
            x: zone.x,
            y: zone.y + constants::RULER_HEIGHT + layout.top + slot_header_height() + BADGE_GAP,
            width: zone.width,
            height: layout.body_h,
        }
    }

    pub(crate) fn line_rect_with_karaoke_width(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
        zone: &Rect,
        karaoke_preview: bool,
        active_karaoke_width: Option<f32>,
        reading_bar_offset_seconds: f64,
        fps: f64,
    ) -> Rect {
        let (x1, width) = line_visual_x_width_with_karaoke_width(
            line,
            current_frame,
            zone,
            karaoke_preview,
            active_karaoke_width,
            reading_bar_offset_seconds,
            fps,
        );
        let body_rect = self.track_body_rect(line.y_slot, zone);
        Rect {
            x: x1,
            y: body_rect.y,
            width,
            height: self.normal_body_h,
        }
    }

    pub(crate) fn badge_rect_for_name(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        name: &str,
        x: f32,
        zone: &Rect,
        reading_bar_offset_seconds: f64,
        fps: f64,
    ) -> Rect {
        let body_rect = self.track_body_rect(line.y_slot, zone);
        let badge_h = self.normal_body_h;
        let w = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
            ambiance_badge_width(name)
        } else {
            badge_width(name)
        };
        let right = if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart) {
            x
        } else {
            x - 4.0 * ppf()
        };
        Rect {
            x: right - w,
            y: body_rect.y,
            width: w,
            height: badge_h,
        }
    }
}

#[cfg(test)]
pub(crate) fn editor_track_layouts(
    project: &Project,
    zone: &Rect,
) -> Vec<rythmo_layout::TrackLayout> {
    EditorLayoutCtx::new(project, zone).track_layouts
}

pub(crate) fn editor_track_layouts_at_frame(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
) -> Vec<rythmo_layout::TrackLayout> {
    EditorLayoutCtx::new_at_frame(project, current_frame, zone).track_layouts
}

pub(crate) fn editor_track_body_rect_at_frame(
    project: &Project,
    y_slot: f32,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    EditorLayoutCtx::new_at_frame(project, current_frame, zone).track_body_rect(y_slot, zone)
}

pub(crate) fn line_rect(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    line_rect_with_karaoke_preview(
        project,
        line,
        current_frame,
        zone,
        false,
        reading_bar_offset_seconds,
        fps,
    )
}

pub(crate) fn line_rect_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    reading_bar_offset_seconds: f64,
    fps: f64,
) -> Rect {
    EditorLayoutCtx::new_at_frame_with_fps(project, current_frame, fps, zone)
        .line_rect_with_karaoke_width(
            line,
            current_frame,
            zone,
            karaoke_preview,
            None,
            reading_bar_offset_seconds,
            fps,
        )
}

pub(crate) fn badge_width(name: &str) -> f32 {
    let rendered_font_size = crate::config::get().ui.font_size * 2.0;
    let measured = crate::vector_text::measure_rythmo_text_width_emphasized_standalone(
        name,
        rendered_font_size,
    )
    .unwrap_or_else(|| text_input::text_width(name, rendered_font_size));
    let italic_left_overhang = rendered_font_size * 0.25;
    let horizontal_padding = BADGE_PADDING_H * 2.0;
    (italic_left_overhang + measured + horizontal_padding).max(BADGE_MIN_W)
}

fn ambiance_badge_width(name: &str) -> f32 {
    let display = crate::rythmo_line::ambiance_label(name);
    let rendered_font_size = crate::config::get().ui.font_size * 2.0;
    let measured = crate::vector_text::measure_rythmo_text_width_emphasized_standalone(
        &display,
        rendered_font_size,
    )
    .unwrap_or_else(|| text_input::text_width(&display, rendered_font_size));
    let italic_left_overhang = rendered_font_size * 0.25;
    let horizontal_padding = BADGE_PADDING_H * 2.0;
    (italic_left_overhang + measured + horizontal_padding).max(150.0)
}

pub(crate) fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

pub(crate) fn note_text_metrics() -> TextInputMetrics {
    TextInputMetrics::left(9.0, 0.0)
}
