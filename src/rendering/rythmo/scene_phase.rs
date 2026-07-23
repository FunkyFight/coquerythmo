//! Explicit karaoke lifecycle layered over the deterministic scene builder.

#[path = "scene.rs"]
mod implementation;

pub use implementation::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, SceneMarker, SceneOptions,
};

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_layout::TrackLayout;
use crate::rythmo_line::RythmoLine;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KaraokeRenderPhase {
    Hidden,
    CountIn { progress: f32 },
    UpcomingPreview,
    Active { progress: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLine {
    pub line: RythmoLine,
    pub track_index: usize,
    pub karaoke_phase: KaraokeRenderPhase,
    // Compatibility fields kept during the renderer migration. They are always
    // derived from `karaoke_phase` and must not be used to decide visibility.
    pub karaoke_progress: Option<f32>,
    pub karaoke_active: bool,
    pub karaoke_count_in_progress: Option<f32>,
    pub karaoke_prestart_scroll: bool,
    pub karaoke_upcoming_stack: bool,
    pub karaoke_stack_row: usize,
    pub character_label_visible: bool,
}

impl SceneLine {
    pub fn karaoke_should_be_visible(&self) -> bool {
        !matches!(self.karaoke_phase, KaraokeRenderPhase::Hidden)
    }

    pub fn karaoke_should_be_centered(&self) -> bool {
        matches!(
            self.karaoke_phase,
            KaraokeRenderPhase::CountIn { .. }
                | KaraokeRenderPhase::UpcomingPreview
                | KaraokeRenderPhase::Active { .. }
        )
    }

    pub fn karaoke_progress(&self) -> Option<f32> {
        match self.karaoke_phase {
            KaraokeRenderPhase::Active { progress } => Some(progress),
            _ => None,
        }
    }

    pub fn karaoke_count_in_progress(&self) -> Option<f32> {
        match self.karaoke_phase {
            KaraokeRenderPhase::CountIn { progress } => Some(progress),
            _ => None,
        }
    }
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
        let current_frame = options.current_frame;
        let built = implementation::RythmoScene::build(project, render_index, options);
        let lines = built
            .lines
            .into_iter()
            .map(|line| SceneLine::from_legacy(line, current_frame))
            .collect();

        Self {
            frame_window: built.frame_window,
            syllable_language: built.syllable_language,
            current_frame: built.current_frame,
            tracks: built.tracks,
            lines,
            markers: built.markers,
            drawings: built.drawings,
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
            .filter(|scene_line| {
                matches!(scene_line.karaoke_phase, KaraokeRenderPhase::Active { .. })
            })
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

impl SceneLine {
    fn from_legacy(line: implementation::SceneLine, current_frame: f64) -> Self {
        let end_frame = line.line.end_frame() as f64;
        let phase = if !line.line.karaoke || current_frame >= end_frame {
            KaraokeRenderPhase::Hidden
        } else if let Some(progress) = line.line.karaoke_progress(current_frame) {
            KaraokeRenderPhase::Active { progress }
        } else if let Some(progress) = line.karaoke_count_in_progress {
            KaraokeRenderPhase::CountIn { progress }
        } else if line.karaoke_prestart_scroll || line.karaoke_upcoming_stack {
            debug_assert!(current_frame < line.line.start_frame as f64);
            KaraokeRenderPhase::UpcomingPreview
        } else {
            KaraokeRenderPhase::Hidden
        };

        debug_assert!(
            !matches!(phase, KaraokeRenderPhase::UpcomingPreview)
                || current_frame < line.line.start_frame as f64
        );
        debug_assert!(
            !matches!(phase, KaraokeRenderPhase::Active { .. }) || current_frame < end_frame
        );

        let karaoke_progress = match phase {
            KaraokeRenderPhase::Active { progress } => Some(progress),
            _ => None,
        };
        let karaoke_count_in_progress = match phase {
            KaraokeRenderPhase::CountIn { progress } => Some(progress),
            _ => None,
        };
        let karaoke_active = matches!(phase, KaraokeRenderPhase::Active { .. });
        let karaoke_prestart_scroll = matches!(phase, KaraokeRenderPhase::CountIn { .. })
            && line.karaoke_prestart_scroll;
        let karaoke_upcoming_stack = matches!(phase, KaraokeRenderPhase::UpcomingPreview);

        Self {
            line: line.line,
            track_index: line.track_index,
            karaoke_phase: phase,
            karaoke_progress,
            karaoke_active,
            karaoke_count_in_progress,
            karaoke_prestart_scroll,
            karaoke_upcoming_stack,
            karaoke_stack_row: line.karaoke_stack_row,
            character_label_visible: line.character_label_visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_at(project: &Project, current_frame: f64) -> RythmoScene {
        let mut index = ProjectRenderIndex::new();
        index.refresh(project);
        RythmoScene::build(
            project,
            &index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: -1_000,
                    last: 1_000,
                },
                current_frame,
                source_fps: 24.0,
                ..SceneOptions::default()
            },
        )
    }

    #[test]
    fn completed_karaoke_line_is_hidden_at_its_exact_end() {
        let mut project = Project::new();
        let id = project.add_line_full(
            100,
            20,
            0.0,
            "ANCIENNE".into(),
            "Actor".into(),
            [1.0; 4],
        );
        project.get_line_mut(id).unwrap().karaoke = true;

        let before = scene_at(&project, 119.999);
        let at_end = scene_at(&project, 120.0);
        let after = scene_at(&project, 120.001);

        assert!(matches!(
            before.lines[0].karaoke_phase,
            KaraokeRenderPhase::Active { .. }
        ));
        for scene in [at_end, after] {
            let line = &scene.lines[0];
            assert_eq!(line.karaoke_phase, KaraokeRenderPhase::Hidden);
            assert!(!line.karaoke_should_be_visible());
            assert!(!line.karaoke_should_be_centered());
            assert_eq!(line.karaoke_progress(), None);
        }
    }

    #[test]
    fn following_preview_never_reuses_the_completed_line() {
        let mut project = Project::new();
        let old = project.add_line_full(
            100,
            20,
            0.0,
            "ANCIENNE".into(),
            "Actor".into(),
            [1.0; 4],
        );
        let next = project.add_line_full(
            140,
            20,
            0.0,
            "SUIVANTE".into(),
            "Actor".into(),
            [1.0; 4],
        );
        project.get_line_mut(old).unwrap().karaoke = true;
        project.get_line_mut(next).unwrap().karaoke = true;

        let scene = scene_at(&project, 120.0);
        let old_line = scene.lines.iter().find(|line| line.line.id == old).unwrap();
        let next_line = scene.lines.iter().find(|line| line.line.id == next).unwrap();
        assert_eq!(old_line.line.text, "ANCIENNE");
        assert_eq!(old_line.karaoke_phase, KaraokeRenderPhase::Hidden);
        assert_eq!(next_line.line.text, "SUIVANTE");
        assert_ne!(next_line.line.id, old_line.line.id);
        assert!(!matches!(
            next_line.karaoke_phase,
            KaraokeRenderPhase::Active { .. }
        ));
    }
}
