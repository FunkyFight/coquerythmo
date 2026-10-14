use crate::rendering::rythmo::scene::SceneLine;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KaraokeRowPriority {
    pub active: bool,
    pub start_frame: i64,
    pub line_id: u64,
}

impl From<&SceneLine> for KaraokeRowPriority {
    fn from(line: &SceneLine) -> Self {
        Self {
            active: line.karaoke_active,
            start_frame: line.line.start_frame,
            line_id: line.line.id,
        }
    }
}

pub fn karaoke_row_candidate_wins(
    candidate: KaraokeRowPriority,
    current: KaraokeRowPriority,
) -> bool {
    match (candidate.active, current.active) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => {
            (candidate.start_frame, candidate.line_id) > (current.start_frame, current.line_id)
        }
        (false, false) => {
            (candidate.start_frame, candidate.line_id) < (current.start_frame, current.line_id)
        }
    }
}

pub fn select_karaoke_winners<I, K>(items: I) -> std::collections::HashSet<u64>
where
    K: Eq + std::hash::Hash,
    I: IntoIterator<Item = (K, KaraokeRowPriority, u64)>,
{
    let mut best: HashMap<K, (KaraokeRowPriority, u64)> = HashMap::new();
    for (key, priority, line_id) in items {
        best.entry(key)
            .and_modify(|current| {
                if karaoke_row_candidate_wins(priority, current.0) {
                    *current = (priority, line_id);
                }
            })
            .or_insert((priority, line_id));
    }
    best.into_values().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::{select_karaoke_winners, KaraokeRowPriority};

    fn upcoming(start_frame: i64, line_id: u64) -> KaraokeRowPriority {
        KaraokeRowPriority {
            active: false,
            start_frame,
            line_id,
        }
    }

    /// A pasted line is appended after the lines that were created before
    /// it, so it is often listed after a farther line of the same row.
    #[test]
    fn nearest_line_wins_the_row_whatever_the_listing_order() {
        let far_first = select_karaoke_winners(vec![
            ((0, 1), upcoming(1613, 16), 16),
            ((0, 1), upcoming(1583, 32), 32),
        ]);
        assert!(far_first.contains(&32));
        assert!(!far_first.contains(&16));

        let near_first = select_karaoke_winners(vec![
            ((0, 1), upcoming(1583, 32), 32),
            ((0, 1), upcoming(1613, 16), 16),
        ]);
        assert!(near_first.contains(&32));
        assert!(!near_first.contains(&16));
    }
}
