use crate::constants;
use crate::project::Project;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackLayout {
    pub track_index: usize,
    pub top: f32,
    pub total_h: f32,
    /// Stable vertical space assigned to the track. `total_h` can shrink to a
    /// single row without moving any following track.
    pub reserved_h: f32,
    pub body_h: f32,
    pub has_karaoke: bool,
}

pub fn track_count() -> usize {
    constants::NUM_SLOTS as usize
}

pub fn track_index_for_y_slot(y_slot: f32) -> usize {
    (y_slot * constants::NUM_SLOTS)
        .round()
        .clamp(0.0, constants::NUM_SLOTS - 1.0) as usize
}

pub fn y_slot_for_track_index(track_index: usize) -> f32 {
    (track_index.min(track_count().saturating_sub(1)) as f32 / constants::NUM_SLOTS)
        .clamp(0.0, 0.75)
}

pub fn export_timeline_x(
    frame: i64,
    current_frame: f64,
    center_x: f32,
    pixels_per_frame: f32,
    reading_bar_offset_frames: f64,
) -> f32 {
    center_x + (frame as f64 - current_frame - reading_bar_offset_frames) as f32 * pixels_per_frame
}

/// Returns whether either a line body or the envelope of its leading
/// decorations touches the horizontal viewport.
///
/// Character badges and voice-actor icons travel immediately before their line
/// body. Culling only against the body makes decorations that have already
/// entered from the right stay invisible until the body itself reaches the
/// edge, at which point the whole visual pops in at once.
pub fn line_or_badge_intersects_viewport(
    line_x: f32,
    line_width: f32,
    leading_visual: Option<(f32, f32)>,
    viewport_left: f32,
    viewport_right: f32,
) -> bool {
    horizontal_rect_intersects_viewport(line_x, line_width, viewport_left, viewport_right)
        || leading_visual.is_some_and(|(visual_x, visual_width)| {
            horizontal_rect_intersects_viewport(
                visual_x,
                visual_width,
                viewport_left,
                viewport_right,
            )
        })
}

/// Horizontal envelope covering a badge and every voice-actor icon rendered
/// immediately before it.
pub fn leading_visual_bounds(
    badge_x: f32,
    badge_width: f32,
    actor_count: usize,
    icon_size: f32,
    icon_gap: f32,
) -> (f32, f32) {
    let badge_left = badge_x.min(badge_x + badge_width);
    let badge_right = badge_x.max(badge_x + badge_width);
    let actor_span = actor_count as f32 * (icon_size.max(0.0) + icon_gap.max(0.0));
    let left = badge_left - actor_span;
    (left, (badge_right - left).max(0.0))
}

fn horizontal_rect_intersects_viewport(
    x: f32,
    width: f32,
    viewport_left: f32,
    viewport_right: f32,
) -> bool {
    if !x.is_finite()
        || !width.is_finite()
        || !viewport_left.is_finite()
        || !viewport_right.is_finite()
    {
        return false;
    }

    let rect_left = x.min(x + width);
    let rect_right = x.max(x + width);
    let view_left = viewport_left.min(viewport_right);
    let view_right = viewport_left.max(viewport_right);
    rect_right >= view_left && rect_left <= view_right
}

pub fn scaled_character_badge_width(character_name: &str, scale: f32) -> f32 {
    let scale = scale.max(0.0);
    let font_size = constants::CHARACTER_LABEL_FONT_SIZE * scale;

    let measured = crate::vector_text::measure_rythmo_text_width_emphasized_standalone(
        character_name,
        font_size,
    )
    .unwrap_or_else(|| {
        character_name.chars().count().max(1) as f32 * constants::BADGE_CHAR_W * scale
    });

    let italic_left_overhang = font_size * 0.25;
    let horizontal_padding = 16.0 * scale;

    (italic_left_overhang + measured + horizontal_padding).max(16.0 * scale)
}

pub fn leading_character_badge_x(
    line_x: f32,
    badge_width: f32,
    scale: f32,
    badge_lead_gap: Option<f32>,
) -> f32 {
    let gap = badge_lead_gap.unwrap_or_else(|| constants::BADGE_GAP * scale.max(0.0));
    line_x - badge_width - gap
}

pub fn all_track_indices() -> Vec<usize> {
    (0..track_count()).collect()
}

pub fn used_track_indices(project: &Project) -> Vec<usize> {
    let mut tracks: Vec<usize> = project
        .lines()
        .map(|line| track_index_for_y_slot(line.y_slot))
        .collect();
    tracks.sort_unstable();
    tracks.dedup();
    if tracks.is_empty() {
        tracks.push(0);
    }
    tracks
}

pub fn track_has_karaoke(project: &Project, track_index: usize) -> bool {
    project
        .lines()
        .any(|line| line.karaoke && track_index_for_y_slot(line.y_slot) == track_index)
}

/// Returns whether a karaoke line is being played on the track at this exact
/// timeline position.
pub fn track_has_active_karaoke(project: &Project, track_index: usize, current_frame: f64) -> bool {
    project.lines().any(|line| {
        line.karaoke
            && line.karaoke_active(current_frame)
            && track_index_for_y_slot(line.y_slot) == track_index
    })
}

pub fn active_karaoke_tracks(project: &Project, current_frame: f64) -> Vec<bool> {
    let mut tracks = vec![false; track_count()];
    for line in project.lines() {
        if line.karaoke && line.karaoke_active(current_frame) {
            tracks[track_index_for_y_slot(line.y_slot)] = true;
        }
    }
    tracks
}

/// Returns the tracks that currently need the two-row karaoke layout.
///
/// An active normal line always uses the one-row layout. In an empty gap, the
/// previous layout is kept when both the previous and next lines are karaoke.
/// A new karaoke section enters two-row mode only when its count-in starts.
pub fn karaoke_mode_tracks(
    project: &Project,
    current_frame: f64,
    count_in_frames: i64,
) -> Vec<bool> {
    let track_count = track_count();
    let mut active: Vec<Option<&crate::rythmo_line::RythmoLine>> = vec![None; track_count];
    let mut previous: Vec<Option<&crate::rythmo_line::RythmoLine>> = vec![None; track_count];
    let mut next: Vec<Option<&crate::rythmo_line::RythmoLine>> = vec![None; track_count];

    // This used to scan the complete project up to three times for every
    // track. Keep the exact ordering rules while collecting every candidate in
    // one pass; long bands therefore pay O(lines + tracks), not O(lines * tracks).
    for line in project.lines() {
        let track_index = track_index_for_y_slot(line.y_slot);
        let is_active =
            current_frame >= line.start_frame as f64 && current_frame <= line.end_frame() as f64;

        if is_active {
            let should_replace = active[track_index]
                .map(|current| (line.start_frame, line.id) > (current.start_frame, current.id))
                .unwrap_or(true);
            if should_replace {
                active[track_index] = Some(line);
            }
            continue;
        }

        if (line.end_frame() as f64) < current_frame {
            let should_replace = previous[track_index]
                .map(|current| {
                    (line.end_frame(), line.start_frame, line.id)
                        > (current.end_frame(), current.start_frame, current.id)
                })
                .unwrap_or(true);
            if should_replace {
                previous[track_index] = Some(line);
            }
            continue;
        }

        if line.start_frame as f64 > current_frame {
            let should_replace = next[track_index]
                .map(|current| (line.start_frame, line.id) < (current.start_frame, current.id))
                .unwrap_or(true);
            if should_replace {
                next[track_index] = Some(line);
            }
        }
    }

    let mut tracks = vec![false; track_count];
    for track_index in 0..track_count {
        if let Some(active) = active[track_index] {
            tracks[track_index] = active.karaoke;
            continue;
        }

        let Some(next) = next[track_index].filter(|line| line.karaoke) else {
            continue;
        };
        let continues_karaoke = previous[track_index].is_some_and(|line| line.karaoke);
        let count_in_started =
            current_frame >= next.start_frame.saturating_sub(count_in_frames.max(0)) as f64;
        tracks[track_index] = continues_karaoke || count_in_started;
    }

    tracks
}

/// Track flags used to choose a stable one-line height. Unlike
/// `karaoke_mode_tracks`, these flags do not change during playback.
pub fn karaoke_tracks(project: &Project) -> Vec<bool> {
    let mut tracks = vec![false; track_count()];
    for line in project.lines() {
        if line.karaoke {
            tracks[track_index_for_y_slot(line.y_slot)] = true;
        }
    }
    tracks
}

pub fn text_emotion_tracks(project: &Project) -> Vec<bool> {
    let mut tracks = vec![false; track_count()];
    if !project.settings().show_text_emotion_lanes {
        return tracks;
    }
    for line in project.lines() {
        if !line.text_emotions.is_empty() {
            tracks[track_index_for_y_slot(line.y_slot)] = true;
        }
    }
    tracks
}

pub fn karaoke_stack_gap(height: f32, scale: f32) -> f32 {
    (2.0 * scale.max(0.5)).min((height * 0.2).max(0.0))
}

pub fn karaoke_track_body_height(row_height: f32, scale: f32) -> f32 {
    row_height * 2.0 + karaoke_stack_gap(row_height * 2.0, scale)
}

pub fn text_emotion_copy_rect(line_y: f32, row_height: f32, scale: f32) -> (f32, f32) {
    let gap = karaoke_stack_gap(row_height, scale);
    (line_y + row_height + gap, row_height)
}

pub fn text_emotion_track_body_height(row_height: f32, scale: f32) -> f32 {
    let (copy_y, copy_height) = text_emotion_copy_rect(0.0, row_height, scale);
    copy_y + copy_height
}

pub fn build_track_layouts(
    project: &Project,
    track_indices: &[usize],
    normal_body_h: f32,
    slot_header_h: f32,
    badge_gap: f32,
    scale: f32,
) -> Vec<TrackLayout> {
    let mut top = 0.0;
    let emotion_tracks = text_emotion_tracks(project);
    track_indices
        .iter()
        .map(|&track_index| {
            let has_karaoke = track_has_karaoke(project, track_index);
            let has_text_emotion = emotion_tracks.get(track_index).copied().unwrap_or(false);
            let body_h = if has_karaoke {
                karaoke_track_body_height(normal_body_h, scale)
            } else if has_text_emotion {
                text_emotion_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let total_h = slot_header_h + badge_gap + body_h;
            let layout = TrackLayout {
                track_index,
                top,
                total_h,
                reserved_h: total_h,
                body_h,
                has_karaoke,
            };
            top += total_h;
            layout
        })
        .collect()
}

pub fn build_track_layouts_at_frame(
    project: &Project,
    track_indices: &[usize],
    current_frame: f64,
    count_in_frames: i64,
    normal_body_h: f32,
    slot_header_h: f32,
    badge_gap: f32,
    scale: f32,
) -> Vec<TrackLayout> {
    let mut top = 0.0;
    let karaoke_mode_tracks = karaoke_mode_tracks(project, current_frame, count_in_frames);
    let reserved_karaoke_tracks = karaoke_tracks(project);
    let emotion_tracks = text_emotion_tracks(project);
    track_indices
        .iter()
        .map(|&track_index| {
            let has_karaoke = karaoke_mode_tracks
                .get(track_index)
                .copied()
                .unwrap_or(false);
            let body_h = if has_karaoke {
                karaoke_track_body_height(normal_body_h, scale)
            } else if emotion_tracks.get(track_index).copied().unwrap_or(false) {
                text_emotion_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let total_h = slot_header_h + badge_gap + body_h;
            let reserved_body_h = if reserved_karaoke_tracks
                .get(track_index)
                .copied()
                .unwrap_or(false)
            {
                karaoke_track_body_height(normal_body_h, scale)
            } else if emotion_tracks.get(track_index).copied().unwrap_or(false) {
                text_emotion_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let reserved_h = slot_header_h + badge_gap + reserved_body_h;
            let layout = TrackLayout {
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

pub fn total_tracks_height(layouts: &[TrackLayout]) -> f32 {
    layouts
        .last()
        .map(|layout| layout.top + layout.reserved_h)
        .unwrap_or(0.0)
}

pub fn track_for_index(layouts: &[TrackLayout], track_index: usize) -> Option<&TrackLayout> {
    layouts
        .iter()
        .find(|layout| layout.track_index == track_index)
}

pub fn track_for_y_slot(layouts: &[TrackLayout], y_slot: f32) -> Option<&TrackLayout> {
    track_for_index(layouts, track_index_for_y_slot(y_slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_timeline_offset_moves_markers_with_the_lines() {
        let center_x = 500.0;
        let pixels_per_frame = 4.0;

        assert_eq!(
            export_timeline_x(124, 100.0, center_x, pixels_per_frame, 24.0),
            center_x
        );
        assert_eq!(
            export_timeline_x(100, 100.0, center_x, pixels_per_frame, 24.0),
            center_x - 96.0
        );
    }

    #[test]
    fn only_tracks_with_karaoke_get_double_body_height() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(normal_id).unwrap().karaoke = false;

        let layouts = build_track_layouts(
            &project,
            &used_track_indices(&project),
            40.0,
            28.0,
            2.0,
            1.0,
        );
        let normal = track_for_index(&layouts, 0).unwrap();
        let karaoke = track_for_index(&layouts, 2).unwrap();

        assert_eq!(normal.body_h, 40.0);
        assert_eq!(karaoke.body_h, karaoke_track_body_height(40.0, 1.0));
        assert_eq!(
            total_tracks_height(&layouts),
            normal.total_h + karaoke.total_h
        );
    }

    #[test]
    fn text_emotion_lanes_are_enabled_by_default_and_can_be_hidden() {
        let mut project = Project::new();
        let line_id = project.add_line_full(0, 24, 0.0, "Colère".into(), "Alice".into(), [1.0; 4]);
        project.get_line_mut(line_id).unwrap().set_text_emotion(
            0,
            6,
            Some(crate::rythmo_line::TextEmotion::AngerContained),
        );

        assert!(text_emotion_tracks(&project)[0]);
        let mut settings = project.settings().clone();
        settings.show_text_emotion_lanes = false;
        project.set_settings(settings);
        assert!(!text_emotion_tracks(&project)[0]);
    }

    #[test]
    fn track_height_enters_karaoke_mode_for_first_count_in() {
        let mut project = Project::new();
        let karaoke_id = project.add_line(240, 24, 0.5);
        project.add_line(0, 24, 0.75);
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;

        let tracks = used_track_indices(&project);
        let before =
            build_track_layouts_at_frame(&project, &tracks, 167.9, 72, 40.0, 28.0, 2.0, 1.0);
        let count_in =
            build_track_layouts_at_frame(&project, &tracks, 168.0, 72, 40.0, 28.0, 2.0, 1.0);
        let active =
            build_track_layouts_at_frame(&project, &tracks, 240.0, 72, 40.0, 28.0, 2.0, 1.0);
        let after =
            build_track_layouts_at_frame(&project, &tracks, 264.1, 72, 40.0, 28.0, 2.0, 1.0);

        assert_eq!(track_for_index(&before, 2).unwrap().body_h, 40.0);
        assert_eq!(
            track_for_index(&count_in, 2).unwrap().body_h,
            karaoke_track_body_height(40.0, 1.0)
        );
        assert_eq!(
            track_for_index(&active, 2).unwrap().body_h,
            karaoke_track_body_height(40.0, 1.0)
        );
        assert_eq!(track_for_index(&after, 2).unwrap().body_h, 40.0);
        let following_top = track_for_index(&before, 3).unwrap().top;
        assert_eq!(track_for_index(&count_in, 3).unwrap().top, following_top);
        assert_eq!(track_for_index(&active, 3).unwrap().top, following_top);
        assert_eq!(track_for_index(&after, 3).unwrap().top, following_top);
    }

    #[test]
    fn gap_layout_follows_the_next_line_type() {
        let mut karaoke_next = Project::new();
        let first_id = karaoke_next.add_line(0, 24, 0.5);
        let next_id = karaoke_next.add_line(72, 24, 0.5);
        karaoke_next.get_line_mut(first_id).unwrap().karaoke = true;
        karaoke_next.get_line_mut(next_id).unwrap().karaoke = true;

        assert!(karaoke_mode_tracks(&karaoke_next, 48.0, 72)[2]);
        karaoke_next.get_line_mut(next_id).unwrap().karaoke = false;
        assert!(!karaoke_mode_tracks(&karaoke_next, 48.0, 72)[2]);
    }

    #[test]
    fn leading_badge_keeps_an_incoming_line_visible_before_its_body() {
        let viewport_left = 0.0;
        let viewport_right = 100.0;
        let line_x = 110.0;
        let line_width = 40.0;
        let badge = Some((84.0, 24.0));

        assert!(!line_or_badge_intersects_viewport(
            line_x,
            line_width,
            None,
            viewport_left,
            viewport_right,
        ));
        assert!(line_or_badge_intersects_viewport(
            line_x,
            line_width,
            badge,
            viewport_left,
            viewport_right,
        ));
    }

    #[test]
    fn line_and_badge_are_culled_once_both_are_outside() {
        assert!(!line_or_badge_intersects_viewport(
            130.0,
            40.0,
            Some((105.0, 20.0)),
            0.0,
            100.0,
        ));
        assert!(line_or_badge_intersects_viewport(
            -10.0,
            20.0,
            Some((-35.0, 20.0)),
            0.0,
            100.0,
        ));
    }

    #[test]
    fn voice_actor_icon_envelope_enters_before_badge() {
        let badge_x = 130.0;
        let leading = leading_visual_bounds(badge_x, 24.0, 2, 20.0, 3.0);
        assert_eq!(leading, (84.0, 70.0));
        assert!(line_or_badge_intersects_viewport(
            160.0,
            40.0,
            Some(leading),
            0.0,
            100.0,
        ));
    }

    #[test]
    fn enlarged_export_badge_text_fits_without_vertical_stretching() {
        let badge_height = constants::SLOT_HEIGHT;
        let natural_text_line_height = (constants::CHARACTER_LABEL_FONT_SIZE * 1.4).ceil();

        assert!(badge_height >= natural_text_line_height);
    }

    #[test]
    fn character_badge_width_contains_emphasized_text() {
        crate::config::init();
        let cases = ["AL", "MADEMOISELLE", "Twilight Sparkle", "ÉMILIE"];
        for name in cases.iter() {
            let scale = 1.0;
            let badge_w = scaled_character_badge_width(name, scale);
            let font_size = constants::CHARACTER_LABEL_FONT_SIZE * scale;
            let emphasized_w = crate::vector_text::measure_rythmo_text_width_emphasized_standalone(
                name, font_size,
            )
            .unwrap_or_else(|| {
                name.chars().count().max(1) as f32 * constants::BADGE_CHAR_W * scale
            });
            let overhang = font_size * 0.25;
            let padding = 16.0 * scale;
            assert!(
                badge_w >= emphasized_w + overhang + padding - 0.5,
                "badge_width ({badge_w}) < emphasized_width ({emphasized_w}) + overhang ({overhang}) + padding ({padding}) for {name}"
            );
        }
    }
}
