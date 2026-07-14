use crate::project::Project;

pub struct ProjectRenderIndex {
    version: u64,
    lines_by_start: Vec<(i64, u64)>,
    markers_by_frame: Vec<(i64, usize)>,
    max_duration_frames: i64,
}

impl Default for ProjectRenderIndex {
    fn default() -> Self {
        Self {
            version: u64::MAX,
            lines_by_start: Vec::new(),
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
        self.markers_by_frame.clear();
        self.max_duration_frames = 0;

        for line in project.lines() {
            self.lines_by_start.push((line.start_frame, line.id));
            self.max_duration_frames = self.max_duration_frames.max(line.duration_frames.max(0));
        }
        self.lines_by_start
            .sort_unstable_by_key(|&(start_frame, line_id)| (start_frame, line_id));

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

    pub fn visible_line_ids(
        &self,
        project: &Project,
        first_frame: i64,
        last_frame: i64,
    ) -> Vec<u64> {
        let search_start = first_frame.saturating_sub(self.max_duration_frames);
        let start_index = self
            .lines_by_start
            .partition_point(|&(start_frame, _)| start_frame < search_start);
        let mut ids = Vec::new();

        for &(start_frame, line_id) in &self.lines_by_start[start_index..] {
            if start_frame > last_frame {
                break;
            }
            let Some(line) = project.get_line(line_id) else {
                continue;
            };
            if line.end_frame() >= first_frame {
                ids.push(line_id);
            }
        }

        ids
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
