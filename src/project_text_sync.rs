//! Domain mutation for keeping synchronization cuts attached to edited text.
//!
//! Media ticks define immutable box edges. Text edits only rebase the character
//! index stored on each edge, so every other fitted box keeps the same geometry.

use crate::detection::{DetectionAddress, DetectionChange, DetectionCue, TextAnchor};
use crate::project::Project;

fn lcs_matches(old: &[char], new: &[char]) -> Vec<(usize, usize)> {
    let width = new.len() + 1;
    let mut lengths = vec![0usize; (old.len() + 1) * width];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            let slot = old_index * width + new_index;
            lengths[slot] = if old[old_index] == new[new_index] {
                1 + lengths[(old_index + 1) * width + new_index + 1]
            } else {
                lengths[(old_index + 1) * width + new_index]
                    .max(lengths[old_index * width + new_index + 1])
            };
        }
    }

    let mut matches = Vec::new();
    let mut old_index = 0usize;
    let mut new_index = 0usize;
    while old_index < old.len() && new_index < new.len() {
        if old[old_index] == new[new_index] {
            matches.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if lengths[(old_index + 1) * width + new_index]
            >= lengths[old_index * width + new_index + 1]
        {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

fn rebase_boundary(
    old_len: usize,
    new_len: usize,
    boundary: usize,
    matches: &[(usize, usize)],
) -> usize {
    let boundary = boundary.min(old_len);
    if boundary == 0 {
        return 0;
    }
    if boundary == old_len {
        return new_len;
    }

    let left = matches
        .iter()
        .copied()
        .rev()
        .find(|(old_index, _)| *old_index < boundary);
    let right = matches
        .iter()
        .copied()
        .find(|(old_index, _)| *old_index >= boundary);

    if let Some((old_index, new_index)) = left {
        if old_index + 1 == boundary {
            // Insertions exactly on a cut belong to the box on its right: the
            // cut remains immediately after the same left-side character.
            return (new_index + 1).min(new_len);
        }
    }
    if let Some((old_index, new_index)) = right {
        if old_index == boundary {
            return new_index.min(new_len);
        }
    }

    match (left, right) {
        (Some((old_left, new_left)), Some((old_right, new_right))) => {
            let old_gap = old_right.saturating_sub(old_left + 1);
            let new_gap = new_right.saturating_sub(new_left + 1);
            if old_gap == 0 {
                return (new_left + 1).min(new_len);
            }
            let local = boundary.saturating_sub(old_left + 1).min(old_gap);
            (new_left + 1 + local * new_gap / old_gap).min(new_len)
        }
        (Some((old_left, new_left)), None) => {
            (new_left + 1 + boundary.saturating_sub(old_left + 1)).min(new_len)
        }
        (None, Some((old_right, new_right))) => {
            new_right.saturating_sub(old_right.saturating_sub(boundary))
        }
        (None, None) => boundary.min(new_len),
    }
}

fn rebased_indices(old_text: &str, new_text: &str, cues: &[DetectionCue]) -> Vec<usize> {
    let old = old_text.chars().collect::<Vec<_>>();
    let new = new_text.chars().collect::<Vec<_>>();
    let matches = lcs_matches(&old, &new);
    let mut result = cues
        .iter()
        .map(|cue| {
            rebase_boundary(
                old.len(),
                new.len(),
                cue.target.grapheme_index().unwrap_or(0) as usize,
                &matches,
            )
        })
        .collect::<Vec<_>>();

    // Several cuts may legitimately collapse onto one text boundary when a box
    // becomes empty. Keep their temporal order instead of deleting a box.
    let mut previous = 0usize;
    for index in &mut result {
        *index = (*index).clamp(previous, new.len());
        previous = *index;
    }
    result
}

impl Project {
    pub(crate) fn update_line_text_preserving_sync_boxes(
        &mut self,
        line_id: u64,
        old_text: &str,
        new_text: &str,
    ) -> bool {
        if old_text == new_text || self.get_line(line_id).is_none() {
            return false;
        }

        let mut cues = self
            .detections()
            .line(line_id)
            .map(|data| data.text_sync_cues().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        cues.sort_by_key(|cue| {
            (
                cue.target.grapheme_index().unwrap_or(u32::MAX),
                cue.media_tick,
                cue.id,
            )
        });
        let indices = rebased_indices(old_text, new_text, &cues);

        if let Some(line) = self.get_line_mut(line_id) {
            line.text = new_text.to_string();
        }

        for (cue, new_index) in cues.into_iter().zip(indices) {
            if cue.target.grapheme_index() == Some(new_index as u32) {
                continue;
            }
            let address = DetectionAddress {
                line_id,
                detection_id: cue.id,
            };
            let mut updated = cue.clone();
            updated.target = TextAnchor::Grapheme {
                index: new_index as u32,
            };
            let _ = self.apply_detection_change(
                &DetectionChange::Remove {
                    address,
                    cue: cue.clone(),
                },
                true,
            );
            let _ = self.apply_detection_change(
                &DetectionChange::Add {
                    address,
                    cue: updated,
                },
                true,
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{DetectionCueId, DetectionKind, MediaTick};

    fn cue(id: u64, index: u32, tick: i64) -> DetectionCue {
        DetectionCue {
            id: DetectionCueId(id),
            kind: DetectionKind::TextSyncPoint,
            media_tick: MediaTick(tick),
            target: TextAnchor::Grapheme { index },
        }
    }

    #[test]
    fn editing_the_middle_box_keeps_other_box_limits_fixed() {
        let cues = vec![cue(1, 3, 300), cue(2, 6, 700)];
        assert_eq!(
            rebased_indices("abcdefghi", "abcdXXXXefghi", &cues),
            vec![3, 10]
        );
    }

    #[test]
    fn multiple_edits_still_preserve_unchanged_box_edges() {
        let cues = vec![cue(1, 3, 300), cue(2, 6, 700)];
        assert_eq!(
            rebased_indices("abcdefghi", "aZZbcdeQQfghi", &cues),
            vec![5, 10]
        );
    }

    #[test]
    fn an_empty_box_keeps_both_temporal_edges() {
        let cues = vec![cue(1, 3, 300), cue(2, 6, 700)];
        assert_eq!(rebased_indices("abcdefghi", "abcghi", &cues), vec![3, 3]);
    }

    #[test]
    fn project_mutation_changes_indices_but_never_ticks() {
        let mut project = Project::new();
        let line_id = project.add_line_full(
            0,
            100,
            0.0,
            "abcdefghi".into(),
            String::new(),
            [1.0; 4],
        );
        for detection in [cue(1, 3, 300), cue(2, 6, 700)] {
            let address = DetectionAddress {
                line_id,
                detection_id: detection.id,
            };
            assert!(project.apply_detection_change(
                &DetectionChange::Add {
                    address,
                    cue: detection,
                },
                true,
            ));
        }

        assert!(project.update_line_text_preserving_sync_boxes(
            line_id,
            "abcdefghi",
            "abcdXXXXefghi",
        ));
        let mut values = project
            .detections()
            .line(line_id)
            .unwrap()
            .text_sync_cues()
            .map(|cue| (cue.target.grapheme_index().unwrap(), cue.media_tick))
            .collect::<Vec<_>>();
        values.sort_by_key(|(index, _)| *index);
        assert_eq!(values, vec![(3, MediaTick(300)), (10, MediaTick(700))]);
    }
}
