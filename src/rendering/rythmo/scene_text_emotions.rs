//! Scene adapter that attaches render-only text-emotion metadata.

#[path = "scene.rs"]
mod base;

pub use base::{
    karaoke_adjacent_max_gap_frames, karaoke_count_in_frames, karaoke_stack_height,
    karaoke_stack_y, FrameWindow, SceneLine, SceneMarker, SceneOptions,
};

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_layout::TrackLayout;

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
        let scene = base::RythmoScene::build(project, render_index, options);
        let mut lines = scene.lines;
        for scene_line in &mut lines {
            let line = &mut scene_line.line;
            if line.kind.is_dialogue() && !line.karaoke && crate::text_emotion::has_line(line.id) {
                line.text = crate::text_emotion::encode_render_text(line.id, &line.text);
            }
        }
        Self {
            frame_window: scene.frame_window,
            syllable_language: scene.syllable_language,
            current_frame: scene.current_frame,
            tracks: scene.tracks,
            lines,
            markers: scene.markers,
            drawings: scene.drawings,
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
                Some((line_y, line_y + karaoke_stack_height(track.body_h, scale)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_emotion::{apply_range, clear, TextEmotion};

    #[test]
    fn dialogue_text_is_encoded_only_in_the_render_scene() {
        clear();
        let mut project = Project::new();
        let id = project.add_line_full(
            0,
            48,
            0.25,
            "Bonjour".into(),
            "Alice".into(),
            [1.0; 4],
        );
        apply_range(id, "Bonjour", 0, 7, Some(TextEmotion::Wave));
        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);
        let scene = RythmoScene::build(
            &project,
            &index,
            SceneOptions {
                frame_window: FrameWindow { first: 0, last: 48 },
                ..SceneOptions::default()
            },
        );
        assert!(crate::text_emotion::decode_render_text(&scene.lines[0].line.text).is_some());
        assert_eq!(project.get_line(id).unwrap().text, "Bonjour");
    }
}
