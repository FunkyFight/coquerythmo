#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::rythmo::scene::FrameWindow;
    use crate::rythmo_line::{MarkerKind, RythmoMarker};

    #[test]
    fn br_height_doubles_only_tracks_with_karaoke() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;

        let width = constants::REF_WIDTH as u32;
        let br_scale = 1.0;
        let s = width as f32 / constants::REF_WIDTH * br_scale;
        let normal_body_h = constants::SLOT_HEIGHT * s;
        let badge_h = constants::BADGE_HEIGHT * s;
        let actor_icon_size = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE * s;
        let slot_header_h = badge_h.max(actor_icon_size);
        let badge_gap = constants::BADGE_GAP * s;
        let normal_total_h = normal_body_h + slot_header_h + badge_gap;
        let karaoke_total_h =
            rythmo_layout::karaoke_track_body_height(normal_body_h, s) + slot_header_h + badge_gap;
        let expected =
            (constants::RULER_HEIGHT * s + normal_total_h + karaoke_total_h).ceil() as u32;

        assert_eq!(br_height(&project, width, br_scale), expected);
    }

    #[test]
    fn cpu_export_count_in_dot_moves_from_left_onto_text() {
        let x = 300.0;
        let y = 80.0;
        let (start_x, _, start_size) = karaoke_count_in_dot_rect(x, y, 0.0, 1.0);
        let (mid_x, _, _) = karaoke_count_in_dot_rect(x, y, 0.5, 1.0);
        let (end_x, _, _) = karaoke_count_in_dot_rect(x, y, 1.0, 1.0);

        assert!(start_x + start_size <= x);
        assert!(mid_x > start_x);
        assert!(mid_x < x);
        assert!((end_x - x).abs() < 0.01);
    }

    #[test]
    fn cpu_export_karaoke_island_after_normal_line_continues_alternating_rows() {
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

    #[test]
    fn cpu_render_handles_marker_and_breath_lines() {
        crate::config::init();
        let mut project = Project::new();
        project.add_line_full(0, 24, 0.0, "↑".into(), "Alice".into(), [0.8, 0.2, 0.2, 1.0]);
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 0,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Out,
            frame: 1,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::LiaisonLeft,
            frame: 2,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::LiaisonRight,
            frame: 3,
        });

        let width = 320;
        let br_scale = 0.5;
        let height = br_height(&project, width, br_scale);
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render_br(&project, 0.0, width, 24.0, br_scale, 1.0);

        assert_eq!(pixels.len(), width as usize * height as usize * 4);
    }
}
