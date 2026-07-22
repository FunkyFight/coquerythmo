//! Shared visibility rules for character labels in the rythmo workspace.

use crate::project::Project;
use crate::rythmo_line::RythmoLine;

/// Show a karaoke character label only for the first karaoke line or when the
/// singer changes from the chronologically previous karaoke line.
///
/// Ordinary dialogue lines, track changes and long pauses do not reset singer
/// continuity: they do not make the same name useful to repeat.
pub(crate) fn karaoke_character_label_visible(project: &Project, line: &RythmoLine) -> bool {
    if !line.karaoke || line.character_name.is_empty() {
        return false;
    }

    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && candidate.karaoke
                && (candidate.start_frame < line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id < line.id))
        })
        .max_by_key(|candidate| (candidate.start_frame, candidate.id))
        .map(|previous| previous.character_name != line.character_name)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::karaoke_character_label_visible;
    use crate::project::Project;

    #[test]
    fn same_singer_is_not_repeated_after_a_gap_or_normal_line() {
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
    fn changing_track_does_not_repeat_the_same_singer() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.0);
        let other_track_id = project.add_line(48, 24, 0.5);
        for id in [first_id, other_track_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }

        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(first_id).unwrap()
        ));
        assert!(!karaoke_character_label_visible(
            &project,
            project.get_line(other_track_id).unwrap()
        ));
    }

    #[test]
    fn changing_singer_on_another_track_shows_the_new_name() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.0);
        let other_track_id = project.add_line(48, 24, 0.5);
        {
            let first = project.get_line_mut(first_id).unwrap();
            first.karaoke = true;
            first.character_name = "Alice".to_string();
        }
        {
            let other = project.get_line_mut(other_track_id).unwrap();
            other.karaoke = true;
            other.character_name = "Bob".to_string();
        }

        assert!(karaoke_character_label_visible(
            &project,
            project.get_line(other_track_id).unwrap()
        ));
    }
}
