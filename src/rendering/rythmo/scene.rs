//! Deterministic, backend-independent description of visible rythmo content.

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_layout::{
    build_track_layouts, build_track_layouts_at_frame, used_track_indices, TrackLayout,
};
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
    pub dynamic_track_layout: bool,
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
            dynamic_track_layout: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KaraokeRenderPhase {
    Hidden,
    CountIn { progress: f32 },
    UpcomingPreview,
    Active { progress: f32 },
}

impl KaraokeRenderPhase {
    #[inline]
    pub fn is_visible(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    #[inline]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active { .. })
    }

    #[inline]
    pub fn count_in_progress(self) -> Option<f32> {
        match self {
            Self::CountIn { progress } => Some(progress),
            _ => None,
        }
    }

    #[inline]
    pub fn active_progress(self) -> Option<f32> {
        match self {
            Self::Active { progress } => Some(progress),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLine {
    pub line: RythmoLine,
    pub track_index: usize,
    pub karaoke_phase: KaraokeRenderPhase,
    // Compatibility views for renderers during the phase migration. These are
    // derived exactly once from karaoke_phase and are never independent state.
    pub karaoke_progress: Option<f32>,
    pub karaoke_active: bool,
    pub karaoke_count_in_progress: Option<f32>,
    pub karaoke_prestart_scroll: bool,
    pub karaoke_upcoming_stack: bool,
    pub karaoke_stack_row: usize,
    pub character_label_visible: bool,
}

impl SceneLine {
    #[inline]
    pub fn karaoke_should_be_centered(&self) -> bool {
        self.line.karaoke && self.karaoke_phase.is_visible()
    }

    #[inline]
    pub fn karaoke_should_be_visible(&self) -> bool {
        self.line.karaoke && self.karaoke_phase.is_visible()
    }

    #[inline]
    pub fn karaoke_progress(&self) -> Option<f32> {
        self.karaoke_phase.active_progress()
    }

    #[inline]
    pub fn karaoke_count_in_progress(&self) -> Option<f32> {
        self.karaoke_phase.count_in_progress()
    }

    #[inline]
    pub fn karaoke_is_active(&self) -> bool {
        self.karaoke_phase.is_active()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneMarker {
    pub kind: MarkerKind,
    pub frame: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RythmoScene {
    pub frame_window: FrameWindow,
    pub syllable_language: crate::project::SyllableLanguage,
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
                let karaoke_phase = karaoke_render_phase(
                    project,
                    line,
                    options.current_frame,
                    max_gap_frames,
                    count_in_frames,
                );
                SceneLine {
                    line: line.clone(),
                    track_index: crate::rythmo_layout::track_index_for_y_slot(line.y_slot),
                    karaoke_phase,
                    karaoke_progress: karaoke_phase.active_progress(),
                    karaoke_active: karaoke_phase.is_active(),
                    karaoke_count_in_progress: karaoke_phase.count_in_progress(),
                    karaoke_prestart_scroll: matches!(karaoke_phase, KaraokeRenderPhase::CountIn { .. }),
                    karaoke_upcoming_stack: matches!(
                        karaoke_phase,
                        KaraokeRenderPhase::UpcomingPreview
                    ),
                    karaoke_stack_row: karaoke_stack_row(project, line, max_gap_frames),
                    character_label_visible: karaoke_character_label_visible(
                        project,
                        line,
                        max_gap_frames,
                    ),
                }
            })
            .collect();

        let markers = render_index
            .visible_marker_indices(options.frame_window.first, options.frame_window.last)
            .into_iter()
            .filter_map(|index| project.marker(index))
            .map(|marker| SceneMarker {
                kind: marker.kind.clone(),
                frame: marker.frame,
            })
            .collect();

        let drawings = project
            .drawing()
            .query_window(options.frame_window.first, options.frame_window.last)
            .into_iter()
            .cloned()
            .collect();

        let track_indices = used_track_indices(project);
        let tracks = if options.dynamic_track_layout {
            build_track_layouts_at_frame(
                project,
                &track_indices,
                options.current_frame,
                count_in_frames,
                options.normal_body_height,
                options.slot_header_height,
                options.badge_gap,
                options.scale,
            )
        } else {
            build_track_layouts(
                project,
                &track_indices,
                options.normal_body_height,
                options.slot_header_height,
                options.badge_gap,
                options.scale,
            )
        };

        Self {
            frame_window: options.frame_window,
            syllable_language: project.syllable_language(),
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
            .filter(|scene_line| scene_line.karaoke_phase.is_active())
            .filter_map(|scene_line| {
                let track =
                    crate::rythmo_layout::track_for_index(&self.tracks, scene_line.track_index)?;
                let body_y = ruler_height + track.top + slot_header_height + badge_gap;
                let line_y =
                    karaoke_stack_y(body_y, track.body_h, scene_line.karaoke_stack_row, scale);
                Some((line_y, line_y + karaoke_stack_height(track.body_h, scale)))
            })
            .collect()
    }
}

fn valid_fps(fps: f64) -> f64 {
    if fps.is_finite() && fps > 0.0 { fps } else { 24.0 }
}

pub fn karaoke_adjacent_max_gap_frames(fps: f64) -> i64 {
    (crate::constants::KARAOKE_ADJACENT_MAX_GAP_SECONDS * valid_fps(fps)).round() as i64
}

pub fn karaoke_count_in_frames(fps: f64) -> i64 {
    (crate::constants::KARAOKE_COUNT_IN_SECONDS * valid_fps(fps))
        .round()
        .max(1.0) as i64
}

fn karaoke_render_phase(
    project: &Project,
    line: &RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
    count_in_frames: i64,
) -> KaraokeRenderPhase {
    if !line.karaoke || !current_frame.is_finite() {
        return KaraokeRenderPhase::Hidden;
    }
    let start = line.start_frame as f64;
    let end = line.end_frame() as f64;
    if current_frame >= end {
        return KaraokeRenderPhase::Hidden;
    }
    if current_frame >= start {
        let progress = ((current_frame - start) / line.duration_frames.max(1) as f64)
            .clamp(0.0, 1.0) as f32;
        return KaraokeRenderPhase::Active { progress };
    }
    if let Some(progress) = karaoke_count_in_progress(line, current_frame, count_in_frames) {
        return KaraokeRenderPhase::CountIn { progress };
    }
    if karaoke_upcoming_stack_visible(project, line, current_frame, max_gap_frames) {
        return KaraokeRenderPhase::UpcomingPreview;
    }
    KaraokeRenderPhase::Hidden
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

fn karaoke_upcoming_stack_visible(
    project: &Project,
    line: &RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || current_frame >= line.start_frame as f64 {
        return false;
    }
    previous_karaoke_line_before(project, line, max_gap_frames).is_some_and(|previous| {
        current_frame >= previous.start_frame as f64 && current_frame < line.start_frame as f64
    })
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
    y + row.min(1) as f32 * (row_height + crate::rythmo_layout::karaoke_stack_gap(height, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_at(project: &Project, current_frame: f64) -> RythmoScene {
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        RythmoScene::build(
            project,
            &render_index,
            SceneOptions {
                frame_window: FrameWindow { first: -1_000, last: 1_000 },
                current_frame,
                source_fps: 24.0,
                ..SceneOptions::default()
            },
        )
    }

    #[test]
    fn karaoke_active_interval_is_semi_open() {
        let mut project = Project::new();
        let id = project.add_line(100, 20, 0.0);
        project.get_line_mut(id).unwrap().karaoke = true;
        assert!(matches!(
            scene_at(&project, 119.999).lines[0].karaoke_phase,
            KaraokeRenderPhase::Active { .. }
        ));
        let at_end = scene_at(&project, 120.0);
        assert_eq!(at_end.lines[0].karaoke_phase, KaraokeRenderPhase::Hidden);
        assert!(!at_end.lines[0].karaoke_should_be_visible());
        assert!(!at_end.lines[0].karaoke_should_be_centered());
        assert_eq!(
            scene_at(&project, 120.001).lines[0].karaoke_phase,
            KaraokeRenderPhase::Hidden
        );
    }

    #[test]
    fn completed_line_cannot_reappear_as_next_preview() {
        let mut project = Project::new();
        let old_id = project.add_line(100, 20, 0.0);
        let next_id = project.add_line(140, 20, 0.0);
        {
            let old = project.get_line_mut(old_id).unwrap();
            old.karaoke = true;
            old.text = "ANCIENNE".into();
        }
        {
            let next = project.get_line_mut(next_id).unwrap();
            next.karaoke = true;
            next.text = "SUIVANTE".into();
        }
        let scene = scene_at(&project, 120.0);
        let old = scene.lines.iter().find(|line| line.line.id == old_id).unwrap();
        let next = scene.lines.iter().find(|line| line.line.id == next_id).unwrap();
        assert_eq!(old.karaoke_phase, KaraokeRenderPhase::Hidden);
        assert_eq!(old.line.text, "ANCIENNE");
        assert!(matches!(
            next.karaoke_phase,
            KaraokeRenderPhase::UpcomingPreview | KaraokeRenderPhase::CountIn { .. }
        ));
        assert_eq!(next.line.text, "SUIVANTE");
        assert_ne!(old.line.id, next.line.id);
    }
}
