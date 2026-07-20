use crate::project::Project;
use std::collections::HashMap;

pub struct ProjectRenderIndex {
    version: u64,
    /// Sorted by `(start_frame, line_id)`.
    lines_by_start: Vec<(i64, i64, u64)>,
    /// Max end frame segment tree over `lines_by_start`.
    line_max_end_tree: Vec<i64>,
    line_tree_leaf_count: usize,
    line_order_by_id: HashMap<u64, usize>,
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
        self.markers_by_frame.clear();
        self.max_duration_frames = 0;

        for (line_index, line) in project.lines().enumerate() {
            self.lines_by_start
                .push((line.start_frame, line.end_frame(), line.id));
            self.line_order_by_id.insert(line.id, line_index);
            self.max_duration_frames = self.max_duration_frames.max(line.duration_frames.max(0));
        }
        self.lines_by_start
            .sort_unstable_by_key(|&(start_frame, _, line_id)| (start_frame, line_id));
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

        for (index, &(_, end_frame, _)) in self.lines_by_start.iter().enumerate() {
            self.line_max_end_tree[self.line_tree_leaf_count + index] = end_frame;
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
            .partition_point(|&(start_frame, _, _)| start_frame <= last_frame);
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
            if let Some(&(_, end_frame, line_id)) = self.lines_by_start.get(range_start) {
                if end_frame >= first_frame {
                    ids.push(line_id);
                }
            }
            return;
        }

        let middle = range_start + (range_end - range_start) / 2;
        self.collect_visible_line_ids(
            node * 2,
            range_start,
            middle,
            query_end,
            first_frame,
            ids,
        );
        self.collect_visible_line_ids(
            node * 2 + 1,
            middle,
            range_end,
            query_end,
            first_frame,
            ids,
        );
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
}
