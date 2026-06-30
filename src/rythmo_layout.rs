use crate::constants;
use crate::project::Project;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackLayout {
    pub track_index: usize,
    pub top: f32,
    pub total_h: f32,
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

pub fn karaoke_stack_gap(height: f32, scale: f32) -> f32 {
    (2.0 * scale.max(0.5)).min((height * 0.2).max(0.0))
}

pub fn karaoke_track_body_height(row_height: f32, scale: f32) -> f32 {
    row_height * 2.0 + karaoke_stack_gap(row_height * 2.0, scale)
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
    track_indices
        .iter()
        .map(|&track_index| {
            let has_karaoke = track_has_karaoke(project, track_index);
            let body_h = if has_karaoke {
                karaoke_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let total_h = slot_header_h + badge_gap + body_h;
            let layout = TrackLayout {
                track_index,
                top,
                total_h,
                body_h,
                has_karaoke,
            };
            top += total_h;
            layout
        })
        .collect()
}

pub fn total_tracks_height(layouts: &[TrackLayout]) -> f32 {
    layouts
        .last()
        .map(|layout| layout.top + layout.total_h)
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
}
