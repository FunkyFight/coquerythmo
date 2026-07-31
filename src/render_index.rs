use crate::project::Project;
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct IndexedLine {
    start_frame: i64,
    end_frame: i64,
    line_id: u64,
    track_index: usize,
    karaoke: bool,
}

pub struct ProjectRenderIndex {
    version: u64,
    /// Sorted by `(start_frame, line_id)`.
    lines_by_start: Vec<IndexedLine>,
    /// Max end frame segment tree over `lines_by_start`.
    line_max_end_tree: Vec<i64>,
    line_tree_leaf_count: usize,
    line_order_by_id: HashMap<u64, usize>,
    lines_by_track_start: Vec<Vec<IndexedLine>>,
    lines_by_track_end: Vec<Vec<IndexedLine>>,
    used_track_indices: Vec<usize>,
    karaoke_tracks: Vec<bool>,
    text_emotion_tracks: Vec<bool>,
    has_text_emotions: bool,
    markers_by_frame: Vec<(i64, usize)>,
    max_duration_frames: i64,
}

impl Default for ProjectRenderIndex {
    fn default() -> Self {
        Self {
            version: u64::MAX,
            lines_by_start: Vec::new(),
            line_max_end_tree: Vec::new(),
            line_tree_leaf_count: 1,
            line_order_by_id: HashMap::new(),
            lines_by_track_start: Vec::new(),
            lines_by_track_end: Vec::new(),
            used_track_indices: Vec::new(),
            karaoke_tracks: Vec::new(),
            text_emotion_tracks: Vec::new(),
            has_text_emotions: false,
            markers_by_frame: Vec::new(),
            max_duration_frames: 0,
        }
    }
}

impl ProjectRenderIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn refresh(&mut self, project: &Project) {
        let version = project.revision();
        if self.version == version {
            return;
        }

        self.lines_by_start.clear();
        self.line_max_end_tree.clear();
        self.line_order_by_id.clear();
        let track_count = crate::rythmo_layout::track_count();
        self.lines_by_track_start = vec![Vec::new(); track_count];
        self.lines_by_track_end = vec![Vec::new(); track_count];
        self.used_track_indices.clear();
        self.karaoke_tracks = vec![false; track_count];
        self.text_emotion_tracks = vec![false; track_count];
        self.has_text_emotions = false;
        self.markers_by_frame.clear();
        self.max_duration_frames = 0;

        let mut used_tracks = vec![false; track_count];
        for (line_index, line) in project.lines().enumerate() {
            let track_index = crate::rythmo_layout::track_index_for_y_slot(line.y_slot);
            let indexed = IndexedLine {
                start_frame: line.start_frame,
                end_frame: line.end_frame(),
                line_id: line.id,
                track_index,
                karaoke: line.karaoke,
            };
            self.lines_by_start.push(indexed);
            self.lines_by_track_start[track_index].push(indexed);
            self.lines_by_track_end[track_index].push(indexed);
            used_tracks[track_index] = true;
            self.karaoke_tracks[track_index] |= line.karaoke;
            let has_text_emotions = !line.text_emotions.is_empty();
            self.has_text_emotions |= has_text_emotions;
            if project.settings().show_text_emotion_lanes {
                self.text_emotion_tracks[track_index] |= has_text_emotions;
            }
            self.line_order_by_id.insert(line.id, line_index);
            self.max_duration_frames = self.max_duration_frames.max(line.duration_frames.max(0));
        }
        self.lines_by_start
            .sort_unstable_by_key(|line| (line.start_frame, line.line_id));
        for lines in &mut self.lines_by_track_start {
            lines.sort_unstable_by_key(|line| (line.start_frame, line.line_id));
        }
        for lines in &mut self.lines_by_track_end {
            lines.sort_unstable_by_key(|line| (line.end_frame, line.start_frame, line.line_id));
        }
        self.used_track_indices.extend(
            used_tracks
                .into_iter()
                .enumerate()
                .filter_map(|(index, used)| used.then_some(index)),
        );
        self.rebuild_line_interval_tree();

        self.markers_by_frame = project
            .markers()
            .iter()
            .enumerate()
            .map(|(index, marker)| (marker.frame, index))
            .collect();
        self.markers_by_frame
            .sort_unstable_by_key(|&(frame, index)| (frame, index));

        self.version = version;
    }

    fn rebuild_line_interval_tree(&mut self) {
        self.line_tree_leaf_count = self.lines_by_start.len().next_power_of_two().max(1);
        self.line_max_end_tree = vec![i64::MIN; self.line_tree_leaf_count * 2];

        for (index, line) in self.lines_by_start.iter().enumerate() {
            self.line_max_end_tree[self.line_tree_leaf_count + index] = line.end_frame;
        }
        for node in (1..self.line_tree_leaf_count).rev() {
            self.line_max_end_tree[node] =
                self.line_max_end_tree[node * 2].max(self.line_max_end_tree[node * 2 + 1]);
        }
    }

    pub fn visible_line_ids(
        &self,
        _project: &Project,
        first_frame: i64,
        last_frame: i64,
    ) -> Vec<u64> {
        let query_end = self
            .lines_by_start
            .partition_point(|line| line.start_frame <= last_frame);
        if query_end == 0 || self.lines_by_start.is_empty() {
            return Vec::new();
        }

        let mut ids = Vec::new();
        self.collect_visible_line_ids(
            1,
            0,
            self.line_tree_leaf_count,
            query_end,
            first_frame,
            &mut ids,
        );
        ids
    }

    fn collect_visible_line_ids(
        &self,
        node: usize,
        range_start: usize,
        range_end: usize,
        query_end: usize,
        first_frame: i64,
        ids: &mut Vec<u64>,
    ) {
        if range_start >= query_end || self.line_max_end_tree[node] < first_frame {
            return;
        }

        if range_end - range_start == 1 {
            if let Some(line) = self.lines_by_start.get(range_start) {
                if line.end_frame >= first_frame {
                    ids.push(line.line_id);
                }
            }
            return;
        }

        let middle = range_start + (range_end - range_start) / 2;
        self.collect_visible_line_ids(node * 2, range_start, middle, query_end, first_frame, ids);
        self.collect_visible_line_ids(node * 2 + 1, middle, range_end, query_end, first_frame, ids);
    }

    pub fn visible_marker_indices(&self, first_frame: i64, last_frame: i64) -> Vec<usize> {
        let start = self
            .markers_by_frame
            .partition_point(|&(frame, _)| frame < first_frame);
        let end = self
            .markers_by_frame
            .partition_point(|&(frame, _)| frame <= last_frame);
        self.markers_by_frame[start..end]
            .iter()
            .map(|&(_, index)| index)
            .collect()
    }

    pub fn max_duration_frames(&self) -> i64 {
        self.max_duration_frames
    }

    pub fn used_track_indices(&self) -> &[usize] {
        &self.used_track_indices
    }

    pub fn karaoke_tracks(&self) -> &[bool] {
        &self.karaoke_tracks
    }

    pub fn text_emotion_tracks(&self) -> &[bool] {
        &self.text_emotion_tracks
    }

    pub fn has_text_emotions(&self) -> bool {
        self.has_text_emotions
    }

    /// Returns the dynamic karaoke layout flags without scanning the project.
    ///
    /// The segment tree finds active overlapping lines while the per-track
    /// sorted arrays find the immediate previous and next lines.
    pub fn karaoke_mode_tracks(&self, current_frame: f64, count_in_frames: i64) -> Vec<bool> {
        let track_count = self.lines_by_track_start.len();
        let mut tracks = vec![false; track_count];
        if !current_frame.is_finite() {
            return tracks;
        }

        let mut active = vec![None; track_count];
        let query_end = self
            .lines_by_start
            .partition_point(|line| line.start_frame as f64 <= current_frame);
        let minimum_end = current_frame.ceil().clamp(i64::MIN as f64, i64::MAX as f64) as i64;
        self.collect_active_lines(
            1,
            0,
            self.line_tree_leaf_count,
            query_end,
            minimum_end,
            &mut active,
        );

        for track_index in 0..track_count {
            if let Some(line) = active[track_index] {
                tracks[track_index] = line.karaoke;
                continue;
            }

            let previous_lines = &self.lines_by_track_end[track_index];
            let previous = previous_lines
                .partition_point(|line| (line.end_frame as f64) < current_frame)
                .checked_sub(1)
                .and_then(|index| previous_lines.get(index));
            let next_lines = &self.lines_by_track_start[track_index];
            let next = next_lines
                .get(next_lines.partition_point(|line| line.start_frame as f64 <= current_frame));
            let Some(next) = next.filter(|line| line.karaoke) else {
                continue;
            };
            let continues_karaoke = previous.is_some_and(|line| line.karaoke);
            let count_in_started =
                current_frame >= next.start_frame.saturating_sub(count_in_frames.max(0)) as f64;
            tracks[track_index] = continues_karaoke || count_in_started;
        }

        tracks
    }

    fn collect_active_lines(
        &self,
        node: usize,
        range_start: usize,
        range_end: usize,
        query_end: usize,
        minimum_end: i64,
        active: &mut [Option<IndexedLine>],
    ) {
        if range_start >= query_end || self.line_max_end_tree[node] < minimum_end {
            return;
        }

        if range_end - range_start == 1 {
            if let Some(&line) = self.lines_by_start.get(range_start) {
                if line.end_frame >= minimum_end {
                    let slot = &mut active[line.track_index];
                    if slot.is_none_or(|current| {
                        (line.start_frame, line.line_id) > (current.start_frame, current.line_id)
                    }) {
                        *slot = Some(line);
                    }
                }
            }
            return;
        }

        let middle = range_start + (range_end - range_start) / 2;
        self.collect_active_lines(
            node * 2,
            range_start,
            middle,
            query_end,
            minimum_end,
            active,
        );
        self.collect_active_lines(
            node * 2 + 1,
            middle,
            range_end,
            query_end,
            minimum_end,
            active,
        );
    }

    pub fn line_order_index(&self, line_id: u64) -> usize {
        self.line_order_by_id
            .get(&line_id)
            .copied()
            .unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::{MarkerKind, RythmoMarker};

    #[test]
    fn render_index_includes_line_started_before_window() {
        let mut project = Project::new();
        let id = project.add_line(0, 100, 0.0);
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        assert_eq!(index.visible_line_ids(&project, 50, 60), vec![id]);
    }

    #[test]
    fn render_index_excludes_lines_outside_window() {
        let mut project = Project::new();
        project.add_line(0, 10, 0.0);
        let visible = project.add_line(20, 10, 0.0);
        project.add_line(40, 10, 0.0);
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        assert_eq!(index.visible_line_ids(&project, 20, 30), vec![visible]);
    }

    #[test]
    fn render_index_prunes_old_short_lines_around_a_long_interval() {
        let mut project = Project::new();
        for frame in 0..1_000 {
            project.add_line(frame * 10, 2, 0.0);
        }
        let long = project.add_line(0, 20_000, 0.25);
        let visible = project.add_line(15_000, 10, 0.5);
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        let mut ids = index.visible_line_ids(&project, 15_000, 15_020);
        ids.sort_unstable();
        let mut expected = vec![long, visible];
        expected.sort_unstable();
        assert_eq!(ids, expected);
    }

    #[test]
    fn render_index_tracks_max_duration() {
        let mut project = Project::new();
        project.add_line(0, 10, 0.0);
        project.add_line(20, 42, 0.0);
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        assert_eq!(index.max_duration_frames(), 42);
    }

    #[test]
    fn render_index_visible_markers_respects_bounds_and_order() {
        let mut project = Project::new();
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 10,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Out,
            frame: 20,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::SceneChange,
            frame: 20,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::LiaisonLeft,
            frame: 30,
        });
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        assert_eq!(index.visible_marker_indices(20, 20), vec![1, 2]);
    }

    #[test]
    fn indexed_karaoke_layout_matches_project_scan_with_many_distant_lines() {
        let mut project = Project::new();
        for frame in 0..1_000 {
            project.add_line(frame * 20, 4, 0.0);
        }
        let karaoke = project.add_line(20_100, 48, 0.5);
        project.get_line_mut(karaoke).unwrap().karaoke = true;

        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);

        for frame in [20_027.5, 20_050.0, 20_149.0] {
            assert_eq!(
                index.karaoke_mode_tracks(frame, 72),
                crate::rythmo_layout::karaoke_mode_tracks(&project, frame, 72)
            );
        }
    }
}
