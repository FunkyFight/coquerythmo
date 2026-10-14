//! Linked dialogues: lines that share their content.
//!
//! Linked lines keep one text, one timing layout (duration, syllable
//! timings, synchronization points), one note and the same options. Each
//! line keeps its own position on the band, and the character is shared only
//! when the group asks for it. The links live in the band settings, next to
//! the detection document, so each language keeps its own groups.

use crate::rythmo_line::RythmoLine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Membership of one line in a group of linked dialogues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineLink {
    pub group: u64,
    /// The character (name, colour and voice actors) follows the group too.
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_character: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Line id → link, for every linked line of a band.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineLinks {
    links: BTreeMap<u64, LineLink>,
}

/// One reversible membership change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineLinkChange {
    pub line_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<LineLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<LineLink>,
}

impl LineLinks {
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    pub fn link(&self, line_id: u64) -> Option<LineLink> {
        self.links.get(&line_id).copied()
    }

    pub fn is_linked(&self, line_id: u64) -> bool {
        self.links.contains_key(&line_id)
    }

    pub fn includes_character(&self, line_id: u64) -> bool {
        self.link(line_id)
            .is_some_and(|link| link.include_character)
    }

    /// Every line of the group of `line_id`, `line_id` included, in id order.
    pub fn members(&self, line_id: u64) -> Vec<u64> {
        let Some(link) = self.link(line_id) else {
            return Vec::new();
        };
        self.links
            .iter()
            .filter(|(_, other)| other.group == link.group)
            .map(|(id, _)| *id)
            .collect()
    }

    /// The other lines of the group of `line_id`.
    pub fn partners(&self, line_id: u64) -> Vec<u64> {
        self.members(line_id)
            .into_iter()
            .filter(|id| *id != line_id)
            .collect()
    }

    pub fn set(&mut self, line_id: u64, link: Option<LineLink>) -> bool {
        match link {
            Some(link) => self.links.insert(line_id, link) != Some(link),
            None => self.links.remove(&line_id).is_some(),
        }
    }

    pub fn next_group_id(&self) -> u64 {
        self.links
            .values()
            .map(|link| link.group)
            .max()
            .map_or(1, |group| group.wrapping_add(1).max(1))
    }
}

/// Copy the shared content of `source` onto `target`. The position (start
/// frame, track) is never copied; the character only when asked. Returns
/// whether `target` changed.
pub fn apply_linked_content(
    target: &mut RythmoLine,
    source: &RythmoLine,
    include_character: bool,
) -> bool {
    let mut linked = RythmoLine {
        id: target.id,
        start_frame: target.start_frame,
        y_slot: target.y_slot,
        ..source.clone()
    };
    if !include_character {
        linked.character_name = target.character_name.clone();
        linked.character_color = target.character_color;
        linked.voice_actor_names = target.voice_actor_names.clone();
    }
    if linked == *target {
        return false;
    }
    *target = linked;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: u64, start_frame: i64, text: &str, character: &str) -> RythmoLine {
        RythmoLine {
            id,
            start_frame,
            duration_frames: 24,
            y_slot: 0.0,
            text: text.to_string(),
            character_name: character.to_string(),
            character_color: [1.0, 0.0, 0.0, 1.0],
            kind: Default::default(),
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
            presence: Default::default(),
            text_emotions: Vec::new(),
            text_styles: Vec::new(),
        }
    }

    #[test]
    fn groups_list_their_members_in_id_order() {
        let mut links = LineLinks::default();
        assert_eq!(links.next_group_id(), 1);
        for id in [30, 10, 20] {
            links.set(
                id,
                Some(LineLink {
                    group: 7,
                    include_character: false,
                }),
            );
        }
        links.set(
            40,
            Some(LineLink {
                group: 8,
                include_character: true,
            }),
        );
        assert_eq!(links.members(20), vec![10, 20, 30]);
        assert_eq!(links.partners(20), vec![10, 30]);
        assert!(links.members(99).is_empty());
        assert_eq!(links.next_group_id(), 9);
        assert!(!links.includes_character(10));
        assert!(links.includes_character(40));
        assert!(links.set(10, None));
        assert!(!links.set(10, None));
        assert_eq!(links.members(20), vec![20, 30]);
    }

    #[test]
    fn linked_content_keeps_the_position_and_optionally_the_character() {
        let mut source = line(1, 100, "Bonjour", "Alice");
        source.duration_frames = 48;
        source.note = "doux".into();
        source.karaoke = true;
        source.syllable_ratios = vec![0.5, 0.5];
        let mut target = line(2, 400, "Salut", "Bob");
        target.y_slot = 0.25;

        assert!(apply_linked_content(&mut target, &source, false));
        assert_eq!(target.id, 2);
        assert_eq!(target.start_frame, 400);
        assert_eq!(target.y_slot, 0.25);
        assert_eq!(target.text, "Bonjour");
        assert_eq!(target.duration_frames, 48);
        assert_eq!(target.note, "doux");
        assert!(target.karaoke);
        assert_eq!(target.syllable_ratios, vec![0.5, 0.5]);
        assert_eq!(target.character_name, "Bob");
        assert!(!apply_linked_content(&mut target, &source, false));

        assert!(apply_linked_content(&mut target, &source, true));
        assert_eq!(target.character_name, "Alice");
        assert_eq!(target.character_color, source.character_color);
        assert_eq!(target.start_frame, 400);
    }
}
