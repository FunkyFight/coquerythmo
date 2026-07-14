//! Deterministic, backend-independent description of visible rythmo content.

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_layout::{build_track_layouts, used_track_indices, TrackLayout};
use crate::rythmo_line::{MarkerKind, RythmoLine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameWindow {
    pub first: i64,
    pub last: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneOptions {
    pub frame_window: FrameWindow,
    pub current_frame: f64,
    pub source_fps: f64,
    pub normal_body_height: f32,
    pub slot_header_height: f32,
    pub badge_gap: f32,
    pub scale: f32,
}

impl Default for SceneOptions {
    fn default() -> Self {
        Self {
            frame_window: FrameWindow { first: 0, last: 0 },
            current_frame: 0.0,
            source_fps: 24.0,
            normal_body_height: 40.0,
            slot_header_height: 28.0,
            badge_gap: 2.0,
            scale: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLine {
    pub line: RythmoLine,
    pub track_index: usize,
    pub karaoke_progress: Option<f32>,
    pub karaoke_active: bool,
    pub karaoke_count_in_progress: Option<f32>,
    pub karaoke_prestart_scroll: bool,
    pub karaoke_upcoming_stack: bool,
    pub karaoke_stack_row: usize,
    pub character_label_visible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneMarker {
    pub kind: MarkerKind,
    pub frame: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RythmoScene {
    pub frame_window: FrameWindow,
    pub current_frame: f64,
    pub tracks: Vec<TrackLayout>,
    pub lines: Vec<SceneLine>,
    pub markers: Vec<SceneMarker>,
    pub drawings: Vec<DrawingStroke>,
}

impl RythmoScene {
    pub fn build(
        project: &Project,
        render_index: &ProjectRenderIndex,
        options: SceneOptions,
    ) -> Self {
        let line_ids = render_index.visible_line_ids(
            project,
            options.frame_window.first,
            options.frame_window.last,
        );
        let source_fps = valid_fps(options.source_fps);
        let max_gap_frames = karaoke_adjacent_max_gap_frames(source_fps);
        let count_in_frames = karaoke_count_in_frames(source_fps);
        let lines = line_ids
            .into_iter()
            .filter_map(|id| project.get_line(id))
            .map(|line| {
                let karaoke_active = line.karaoke_active(options.current_frame);
                let karaoke_count_in_progress = karaoke_count_in_progress(
                    line,
                    options.current_frame,
                    count_in_frames,
                );
                let karaoke_prestart_scroll = karaoke_prestart_scroll_visible(
                    project,
                    line,
                    options.current_frame,
                    max_gap_frames,
                    count_in_frames,
                );
                let karaoke_upcoming_stack = karaoke_upcoming_stack_visible(
                    project,
                    line,
                    options.current_frame,
                    max_gap_frames,
                );
                let karaoke_stack_row = karaoke_stack_row(project, line, max_gap_frames);
                let character_label_visible = karaoke_character_label_visible(
                    project,
                    line,
                    max_gap_frames,
                );
                SceneLine {
                    line: line.clone(),
                    track_index: crate::rythmo_layout::track_index_for_y_slot(line.y_slot),
                    karaoke_progress: line.karaoke_progress(options.current_frame),
                    karaoke_active,
                    karaoke_count_in_progress,
                    karaoke_prestart_scroll,
                    karaoke_upcoming_stack,
                    karaoke_stack_row,
                    character_label_visible,
                }
            })
            .collect();

        let markers = render_index
            .visible_marker_indices(options.frame_window.first, options.frame_window.last)
            .into_iter()
            .filter_map(|index| project.markers.get(index))
            .map(|marker| SceneMarker {
                kind: marker.kind.clone(),
                frame: marker.frame,
            })
            .collect();

        let drawings = project
            .drawing
            .query_window(options.frame_window.first, options.frame_window.last)
            .into_iter()
            .cloned()
            .collect();

        let tracks = build_track_layouts(
            project,
            &used_track_indices(project),
            options.normal_body_height,
            options.slot_header_height,
            options.badge_gap,
            options.scale,
        );

        Self {
            frame_window: options.frame_window,
            current_frame: options.current_frame,
            tracks,
            lines,
            markers,
            drawings,
        }
    }

    pub fn active_karaoke_skip_ranges(
        &self,
        ruler_height: f32,
        slot_header_height: f32,
        badge_gap: f32,
        scale: f32,
    ) -> Vec<(f32, f32)> {
        self.lines
            .iter()
            .filter(|scene_line| scene_line.karaoke_active)
            .filter_map(|scene_line| {
                let track = crate::rythmo_layout::track_for_index(
                    &self.tracks,
                    scene_line.track_index,
                )?;
                let body_y = ruler_height + track.top + slot_header_height + badge_gap;
                let line_y = karaoke_stack_y(
                    body_y,
                    track.body_h,
                    scene_line.karaoke_stack_row,
                    scale,
                );
                Some((
                    line_y,
                    line_y + karaoke_stack_height(track.body_h, scale),
                ))
            })
            .collect()
    }
}

fn valid_fps(fps: f64) -> f64 {
    if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    }
}

pub fn karaoke_adjacent_max_gap_frames(fps: f64) -> i64 {
    (crate::constants::KARAOKE_ADJACENT_MAX_GAP_SECONDS * valid_fps(fps)).round() as i64
}

pub fn karaoke_count_in_frames(fps: f64) -> i64 {
    (crate::constants::KARAOKE_COUNT_IN_SECONDS * valid_fps(fps))
        .round()
        .max(1.0) as i64
}

fn karaoke_count_in_progress(
    line: &RythmoLine,
    current_frame: f64,
    count_in_frames: i64,
) -> Option<f32> {
    if !line.karaoke || current_frame >= line.start_frame as f64 || count_in_frames <= 0 {
        return None;
    }
    let count_in_start = line.start_frame as f64 - count_in_frames as f64;
    if current_frame < count_in_start {
        return None;
    }
    Some(((current_frame - count_in_start) / count_in_frames as f64).clamp(0.0, 1.0) as f32)
}

fn same_karaoke_track(a: &RythmoLine, b: &RythmoLine) -> bool {
    crate::rythmo_layout::track_index_for_y_slot(a.y_slot)
        == crate::rythmo_layout::track_index_for_y_slot(b.y_slot)
}

fn previous_line_on_same_track_before<'a>(
    project: &'a Project,
    line: &RythmoLine,
) -> Option<&'a RythmoLine> {
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && same_karaoke_track(candidate, line)
                && (candidate.start_frame < line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id < line.id))
        })
        .max_by_key(|candidate| (candidate.start_frame, candidate.id))
}

fn previous_karaoke_line_before<'a>(
    project: &'a Project,
    line: &RythmoLine,
    max_gap_frames: i64,
) -> Option<&'a RythmoLine> {
    let previous = previous_line_on_same_track_before(project, line)?;
    if previous.karaoke && (line.start_frame - previous.end_frame()).max(0) <= max_gap_frames {
        Some(previous)
    } else {
        None
    }
}

fn karaoke_prestart_scroll_visible(
    project: &Project,
    line: &RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
    count_in_frames: i64,
) -> bool {
    line.karaoke
        && karaoke_count_in_progress(line, current_frame, count_in_frames).is_some()
        && previous_karaoke_line_before(project, line, max_gap_frames).is_none()
}

fn karaoke_upcoming_stack_visible(
    project: &Project,
    line: &RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || current_frame >= line.start_frame as f64 {
        return false;
    }
    previous_karaoke_line_before(project, line, max_gap_frames)
        .is_some_and(|previous| current_frame >= previous.start_frame as f64)
}

fn karaoke_island_index(project: &Project, line: &RythmoLine, max_gap_frames: i64) -> usize {
    let mut index = 0;
    let mut current = line;
    while let Some(previous) = previous_karaoke_line_before(project, current, max_gap_frames) {
        index += 1;
        current = previous;
    }
    if previous_line_on_same_track_before(project, current)
        .is_some_and(|previous| !previous.karaoke)
    {
        index += 1;
    }
    index
}

fn karaoke_stack_row(project: &Project, line: &RythmoLine, max_gap_frames: i64) -> usize {
    karaoke_island_index(project, line, max_gap_frames) % 2
}

fn karaoke_character_label_visible(
    project: &Project,
    line: &RythmoLine,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || line.character_name.is_empty() {
        return false;
    }
    previous_karaoke_line_before(project, line, max_gap_frames)
        .map(|previous| previous.character_name != line.character_name)
        .unwrap_or(true)
}

pub fn karaoke_stack_height(height: f32, scale: f32) -> f32 {
    ((height - crate::rythmo_layout::karaoke_stack_gap(height, scale)).max(1.0) / 2.0).max(1.0)
}

pub fn karaoke_stack_y(y: f32, height: f32, row: usize, scale: f32) -> f32 {
    let row_height = karaoke_stack_height(height, scale);
    y + row.min(1) as f32
        * (row_height + crate::rythmo_layout::karaoke_stack_gap(height, scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line::{MarkerKind, RythmoMarker};

    #[test]
    fn scene_build_is_deterministic_and_revision_indexed() {
        let mut project = Project::new();
        let normal = project.add_line_full(
            0,
            48,
            0.0,
            "normal".into(),
            "Alice".into(),
            [1.0, 0.0, 0.0, 1.0],
        );
        let karaoke = project.add_line_full(
            32,
            48,
            0.5,
            "karaoke".into(),
            "Bob".into(),
            [0.0, 1.0, 0.0, 1.0],
        );
        project.get_line_mut(karaoke).unwrap().karaoke = true;
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 40,
        });

        let options = SceneOptions {
            frame_window: FrameWindow { first: 16, last: 72 },
            current_frame: 40.0,
            ..SceneOptions::default()
        };
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);
        let first = RythmoScene::build(&project, &index, options);
        let second = RythmoScene::build(&project, &index, options);

        assert_eq!(first, second);
        assert_eq!(first.lines.iter().map(|line| line.line.id).collect::<Vec<_>>(), vec![normal, karaoke]);
        assert_eq!(first.markers.len(), 1);
        assert_eq!(first.lines[1].karaoke_progress, Some(1.0 / 6.0));
    }
}
