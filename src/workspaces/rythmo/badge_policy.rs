//! Shared visibility and collision rules for character labels in the rythmo workspace.

use crate::project::Project;
use crate::rythmo_layout;
use crate::rythmo_line::RythmoLine;
use crate::ui::primitives::Rect;

const BADGE_GAP: f32 = 2.0;

/// Show a karaoke character label only for the first karaoke line on its track
/// or when the singer changes from the chronologically previous karaoke line
/// on that same track.
///
/// Other tracks, ordinary dialogue lines and long pauses do not reset the
/// singer continuity of this track.
pub(crate) fn karaoke_character_label_visible(project: &Project, line: &RythmoLine) -> bool {
    if !line.karaoke || line.character_name.is_empty() {
        return false;
    }

    let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && candidate.karaoke
                && rythmo_layout::track_index_for_y_slot(candidate.y_slot) == track_index
                && (candidate.start_frame < line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id < line.id))
        })
        .max_by_key(|candidate| (candidate.start_frame, candidate.id))
        .map(|previous| previous.character_name != line.character_name)
        .unwrap_or(true)
}

/// Compute the ordinary editor badge layout against every line in the project.
///
/// The legacy renderer normally performs this calculation only against its
/// current viewport subset. Using the whole project keeps the hidden/fitted
/// decision stable while a colliding line enters or leaves that subset.
pub(crate) fn stable_character_badge_layout(
    project: &Project,
    line: &RythmoLine,
    current_frame: f64,
    zone: &Rect,
) -> (bool, Rect, f32) {
    let line_body = super::view_implementation::line_rect(project, line, current_frame, zone);
    let badge =
        super::view_implementation::badge_rect_for_line(project, line, current_frame, zone);
    let collision_targets: Vec<(u64, Rect, &str)> = project
        .lines()
        .map(|candidate| {
            (
                candidate.id,
                super::view_implementation::line_rect(
                    project,
                    candidate,
                    current_frame,
                    zone,
                ),
                candidate.character_name.as_str(),
            )
        })
        .collect();

    character_badge_collision_layout(
        line.id,
        &line.character_name,
        &badge,
        line_body.x,
        &collision_targets,
    )
}

fn character_badge_collision_layout(
    line_id: u64,
    character_name: &str,
    badge_rect: &Rect,
    line_x: f32,
    other_lines: &[(u64, Rect, &str)],
) -> (bool, Rect, f32) {
    let collides = |candidate: &Rect| {
        other_lines.iter().any(|(other_id, other_rect, _)| {
            *other_id != line_id && rects_overlap(candidate, other_rect)
        })
    };

    for (other_id, other_rect, other_character_name) in other_lines {
        if *other_id == line_id || !rects_overlap(badge_rect, other_rect) {
            continue;
        }
        if *other_character_name == character_name {
            return (true, *badge_rect, 1.0);
        }
    }

    if !collides(badge_rect) {
        return (false, *badge_rect, 1.0);
    }

    let mut fitted = *badge_rect;
    fitted.x = line_x - BADGE_GAP - fitted.width;
    if !collides(&fitted) {
        return (false, fitted, 1.0);
    }

    let top = fitted.y;
    let base_width = fitted.width;
    let base_height = fitted.height;
    for step in 1..=95 {
        let scale = 1.0 - step as f32 * 0.01;
        fitted.width = base_width * scale;
        fitted.height = base_height * scale;
        fitted.x = line_x - BADGE_GAP - fitted.width;
        fitted.y = top;
        if !collides(&fitted) {
            return (false, fitted, scale);
        }
    }

    (false, fitted, 0.05)
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width
        && a.x + a.width > b.x
        && a.y < b.y + b.height
        && a.y + a.height > b.y
}

#[cfg(test)]
mod tests {
    use super::{
        character_badge_collision_layout, karaoke_character_label_visible,
    };
    use crate::project::Project;
    use crate::ui::primitives::Rect;

    #[test]
    fn singer_continuity_is_independent_for_each_track() {
        let mut project = Project::new();
        let track_one_first = project.add_line(0, 24, 0.0);
        let track_two_first = project.add_line(48, 24, 0.5);
        let track_one_second = project.add_line(96, 24, 0.0);

        {
            let line = project.get_line_mut(track_one_first).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        {
            let line = project.get_line_mut(track_two_first).unwrap();
            line.karaoke = true;
            line.character_name = "Bob".to_string();
        }
        {
            let line = project.get_line_mut(track_one_second).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }

        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(track_one_first).unwrap()
        ));
        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(track_two_first).unwrap()
        ));
        assert!(!karaoke_character_label_visible(
            &project,
            project.get_line(track_one_second).unwrap()
        ));
    }

    #[test]
    fn same_name_still_appears_once_on_each_track() {
        let mut project = Project::new();
        let track_one = project.add_line(0, 24, 0.0);
        let track_two = project.add_line(48, 24, 0.5);
        for id in [track_one, track_two] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }

        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(track_one).unwrap()
        ));
        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(track_two).unwrap()
        ));
    }

    #[test]
    fn same_singer_is_not_repeated_after_a_gap_or_normal_line_on_the_same_track() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let normal_id = project.add_line(240, 24, 0.25);
        let repeated_id = project.add_line(480, 24, 0.25);
        let changed_id = project.add_line(720, 24, 0.25);

        for id in [first_id, repeated_id, changed_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(changed_id).unwrap().character_name = "Bob".to_string();

        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(first_id).unwrap()
        ));
        assert!(!karaoke_character_label_visible(
            &project,
            project.get_line(repeated_id).unwrap()
        ));
        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(changed_id).unwrap()
        ));
    }

    #[test]
    fn collision_visibility_does_not_depend_on_the_rendered_subset() {
        let badge = Rect {
            x: 80.0,
            y: 10.0,
            width: 60.0,
            height: 20.0,
        };
        let colliding_body = Rect {
            x: 100.0,
            y: 10.0,
            width: 120.0,
            height: 20.0,
        };
        let all_project_lines = [(2, colliding_body, "Alice")];

        let (hidden_with_project, _, _) = character_badge_collision_layout(
            1,
            "Alice",
            &badge,
            240.0,
            &all_project_lines,
        );
        let (hidden_with_viewport_subset, _, _) =
            character_badge_collision_layout(1, "Alice", &badge, 240.0, &[]);

        assert!(hidden_with_project);
        assert!(!hidden_with_viewport_subset);
    }
}
