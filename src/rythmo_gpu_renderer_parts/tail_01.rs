

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::rythmo::scene::FrameWindow;

    #[test]
    fn gpu_export_karaoke_island_after_normal_line_continues_alternating_rows() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.25);
        let first_karaoke_id = project.add_line(24 * 2, 24, 0.25);
        let second_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(first_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(second_karaoke_id).unwrap().karaoke = true;

        let mut index = ProjectRenderIndex::new();
        index.refresh(&project);
        let scene = RythmoScene::build(
            &project,
            &index,
            SceneOptions {
                frame_window: FrameWindow {
                    first: 0,
                    last: 120,
                },
                current_frame: 48.0,
                source_fps: 24.0,
                ..SceneOptions::default()
            },
        );
        assert_eq!(
            scene
                .lines
                .iter()
                .find(|line| line.line.id == first_karaoke_id)
                .unwrap()
                .karaoke_stack_row,
            1
        );
        assert_eq!(
            scene
                .lines
                .iter()
                .find(|line| line.line.id == second_karaoke_id)
                .unwrap()
                .karaoke_stack_row,
            0
        );
    }
}
