//! Keyboard handling of a karaoke line: a cursor walks its syllable handles
//! and, under them, its points and liaisons.

use super::*;

/// What the keyboard cursor stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyllableTarget {
    /// The handle that starts this syllable (never the first syllable, which
    /// starts with the line).
    Handle(usize),
    /// A point or a liaison under the line, in reading order.
    Mark(usize),
}

/// The keyboard cursor on a karaoke line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyllableCursor {
    pub line_id: u64,
    pub target: SyllableTarget,
}

/// What is toggled under a karaoke line: the point of a word that can be
/// cut into more syllables, or the liaison between two words.
#[derive(Clone, Debug, PartialEq)]
pub enum SyllableMark {
    Pin(crate::syllable::SyllableSplitPin),
    Liaison(crate::syllable::LiaisonSite),
}

impl SyllableMark {
    /// Character of the text the mark is attached to: marks are walked in
    /// that order, which does not change when one of them is toggled.
    fn character(&self) -> usize {
        match self {
            Self::Pin(pin) => pin.split_at,
            Self::Liaison(site) => site.from,
        }
    }
}

/// Points and liaisons of `line`, in reading order.
pub(crate) fn syllable_marks(
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
) -> Vec<SyllableMark> {
    if line.kind.is_ambiance() || line.text.is_empty() {
        return Vec::new();
    }
    let mut marks: Vec<SyllableMark> =
        crate::syllable::syllable_split_pins(&line.text, line.syllable_rules(lang))
            .into_iter()
            .map(SyllableMark::Pin)
            .chain(
                crate::syllable::liaison_sites(&line.text, lang)
                    .into_iter()
                    .map(SyllableMark::Liaison),
            )
            .collect();
    marks.sort_by_key(SyllableMark::character);
    marks
}

/// Where each of `marks` is drawn, as a share of the width of the line.
pub(crate) fn syllable_mark_shares(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    state: &RythmoState,
    marks: &[SyllableMark],
) -> Vec<f32> {
    if marks.is_empty() {
        return Vec::new();
    }
    let positions = editor_char_positions(project, line, state.syllable_drag.as_ref(), lang, state);
    let position = |character: usize| positions[character.min(positions.len() - 1)];
    marks
        .iter()
        .map(|mark| match mark {
            SyllableMark::Pin(pin) => (position(pin.split_at) + position(pin.end)) / 2.0,
            SyllableMark::Liaison(site) => (position(site.from) + position(site.to + 1)) / 2.0,
        })
        .collect()
}

/// Characters (`start..end`) of every syllable of `line`.
pub(crate) fn syllable_char_ranges(
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    state: &RythmoState,
) -> Vec<(usize, usize)> {
    let count = line.text.chars().count();
    if count == 0 {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    for end in state
        .get_syllable_breaks(line, lang)
        .into_iter()
        .chain(std::iter::once(count))
    {
        let end = end.min(count);
        ranges.push((start.min(end), end));
        start = end;
    }
    ranges
}

/// Text of the characters `range` of `line`, as it is read out.
pub(crate) fn syllable_text(
    line: &crate::rythmo_line::RythmoLine,
    range: (usize, usize),
) -> String {
    line.text
        .chars()
        .skip(range.0)
        .take(range.1.saturating_sub(range.0))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Start and end of every syllable of `line` as shares of the line: one
/// more value than it has syllables. They follow the handle being moved.
pub(crate) fn syllable_boundary_shares(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    state: &RythmoState,
) -> Vec<f32> {
    let count = state.get_syllable_breaks(line, lang).len() + 1;
    let drag = state.syllable_drag.as_ref();
    sync_syllable_boundary_ratios(project, line, drag, lang, state)
        .or_else(|| {
            let ratios = syllable_ratios_for_line(line, drag, lang, state)?;
            let mut boundaries = Vec::with_capacity(ratios.len() + 1);
            let mut x = 0.0;
            boundaries.push(x);
            for ratio in &ratios {
                x += ratio;
                boundaries.push(x.min(1.0));
            }
            Some(boundaries)
        })
        .filter(|boundaries| boundaries.len() == count + 1)
        .unwrap_or_else(|| {
            (0..=count)
                .map(|index| index as f32 / count as f32)
                .collect()
        })
}

/// The syllable that covers `share` of the line.
pub(crate) fn syllable_at_share(boundaries: &[f32], share: f32) -> usize {
    let count = boundaries.len().saturating_sub(1);
    (0..count)
        .rev()
        .find(|index| share >= boundaries[*index])
        .unwrap_or(0)
}

/// Whether the handle that starts `syllable` is placed.
pub(crate) fn syllable_handle_is_placed(
    state: &RythmoState,
    line_id: u64,
    syllable: usize,
    count: usize,
) -> bool {
    syllable > 0
        && state
            .placed_syllable_handles(line_id, count)
            .contains(&(syllable - 1))
}

/// Frees the handle that starts `syllable`: the next proportional moves
/// share it out again.
pub(crate) fn free_syllable_handle(state: &mut RythmoState, line_id: u64, syllable: usize) {
    if syllable == 0 {
        return;
    }
    if let Some(placed) = state.syllable_placed_handles.get_mut(&line_id) {
        placed.separators.retain(|other| *other != syllable - 1);
    }
}

/// Moves the start of `syllable` to `target`, a share of the line, the way
/// a drag of its handle does: the other handles follow in proportion and
/// the handle is placed. Returns what to apply and the share it reached, or
/// nothing when the handle cannot move.
pub(crate) fn move_syllable_start(
    project: &Project,
    state: &mut RythmoState,
    line: &crate::rythmo_line::RythmoLine,
    syllable: usize,
    target: f32,
) -> Option<(Vec<UiAction>, f32)> {
    let lang = project.syllable_language_code();
    if syllable == 0 || line.duration_frames <= 0 || state.slot_reservation_for(line).is_some() {
        return None;
    }
    let ratios = syllable_ratios_for_line(line, None, lang, state)?;
    if syllable >= ratios.len() {
        return None;
    }
    let separator_index = syllable - 1;
    let synced = sync_syllable_boundary_ratios(project, line, None, lang, state).is_some();
    let drag_line_id = if synced {
        encode_sync_syllable_drag_line_id(line.id)
    } else {
        line.id
    };
    // The drag works in pixels: the line is given a width that does not
    // depend on the zoom, so that a key always moves the handle the same way.
    let width = (line.duration_frames as f32 * 4.0).max(1.0);
    state.syllable_drag = Some(SyllableDrag {
        line_id: drag_line_id,
        separator_index,
        ratios: ratios.clone(),
        original_ratios: ratios,
        drag_start_x: 0.0,
        line_rect: Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: 1.0,
        },
        preserve_prefix: false,
    });
    if synced {
        crate::workspaces::rythmo::detection_ui::begin_sync_syllable_drag(
            project,
            line,
            drag_line_id,
            separator_index,
            state,
        );
    } else {
        crate::workspaces::rythmo::detection_ui::clear_sync_syllable_drag();
    }

    // With synchronization points the handle does not follow the pointer one
    // for one: the move is corrected until the handle stands on the target.
    let position =
        |state: &RythmoState| syllable_boundary_shares(project, line, lang, state)[syllable];
    let mut current = position(state);
    let mut x = 0.0;
    let mut scale = 1.0_f32;
    for _ in 0..5 {
        let request = (target - current) / scale;
        if request.abs() <= 0.0001 {
            break;
        }
        x += request * width;
        let _ = syllable_mouse_move(state, x);
        let reached = position(state);
        let moved = reached - current;
        if moved.abs() <= f32::EPSILON {
            break;
        }
        scale = (moved / request).clamp(0.05, 20.0);
        current = reached;
    }

    let changed = state
        .syllable_drag
        .as_ref()
        .is_some_and(|drag| drag.ratios != drag.original_ratios);
    if !changed {
        state.syllable_drag = None;
        crate::workspaces::rythmo::detection_ui::clear_sync_syllable_drag();
        return None;
    }
    let actions = match syllable_mouse_release(state)? {
        EventResponse::Action(action) => vec![action],
        EventResponse::Actions(actions) => actions,
        _ => Vec::new(),
    };
    Some((actions, current))
}

/// The handle the keyboard works on (the one that starts `syllable`): its
/// bar in a light halo, over the whole height of the line.
pub(crate) fn render_syllable_cursor(
    rect: &Rect,
    boundaries: &[f32],
    syllable: usize,
    placed: bool,
    quads: &mut Vec<QuadInstance>,
) {
    // The two ends of the line are not handles.
    if syllable == 0 || syllable + 1 >= boundaries.len() {
        return;
    }
    let x = rect.x + boundaries[syllable] * rect.width;
    let halo = 5.0;
    quads.push(QuadInstance {
        rect: [x - halo, rect.y - 2.0, halo * 2.0, rect.height + 4.0],
        color: [1.0; 4],
        color_bottom: [1.0; 4],
        border_color: [0.05, 0.05, 0.08, 1.0],
        border_width: 1.5,
        border_radius: halo,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        shadow_blur: 4.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    let color = if placed {
        [0.85, 0.55, 0.0, 1.0]
    } else {
        [0.95, 0.08, 0.03, 1.0]
    };
    quads.push(QuadInstance {
        rect: [x - 1.5, rect.y + 1.0, 3.0, syllable_handle_height(rect)],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 1.5,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_syllable_under_a_share_is_the_last_one_that_started() {
        let boundaries = [0.0, 0.25, 0.6, 1.0];
        assert_eq!(syllable_at_share(&boundaries, 0.0), 0);
        assert_eq!(syllable_at_share(&boundaries, 0.24), 0);
        assert_eq!(syllable_at_share(&boundaries, 0.25), 1);
        assert_eq!(syllable_at_share(&boundaries, 0.99), 2);
        assert_eq!(syllable_at_share(&boundaries, -1.0), 0);
    }

    fn karaoke_project(text: &str) -> (Project, u64) {
        let mut project = Project::new();
        let id = project.add_line_full(0, 100, 0.5, text.into(), "A".into(), [1.0; 4]);
        // Saved handles: the default ones are measured from the settings.
        let count = {
            let line = project.get_line(id).unwrap();
            let rules = line.syllable_rules(project.syllable_language_code());
            crate::syllable::syllable_breaks(&line.text, rules).len() + 1
        };
        let line = project.get_line_mut(id).unwrap();
        line.karaoke = true;
        line.syllable_ratios = vec![1.0 / count as f32; count];
        (project, id)
    }

    #[test]
    fn the_keyboard_moves_a_handle_like_a_drag_and_places_it() {
        let (project, id) = karaoke_project("bonjour tout le monde");
        let mut state = RythmoState::new();
        let line = project.get_line(id).unwrap();
        let lang = project.syllable_language_code();
        let before = syllable_boundary_shares(&project, line, lang, &state);
        let count = before.len() - 1;
        assert!(count >= 4);

        let target = before[2] + 0.05;
        let (actions, reached) =
            move_syllable_start(&project, &mut state, line, 2, target).unwrap();
        assert!((reached - target).abs() < 0.002);
        assert!(state.syllable_drag.is_none());
        assert_eq!(state.placed_syllable_handles(id, count), &[1]);
        assert!(syllable_handle_is_placed(&state, id, 2, count));
        let [UiAction::SetSyllableRatios { line_id, ratios }] = actions.as_slice() else {
            panic!("unexpected {actions:?}");
        };
        assert_eq!(*line_id, id);
        assert!((ratios[..2].iter().sum::<f32>() - target).abs() < 0.002);
        assert!((ratios.iter().sum::<f32>() - 1.0).abs() < 0.001);

        // The first syllable starts with its line, and a handle that is
        // already there does not move.
        assert!(move_syllable_start(&project, &mut state, line, 0, 0.1).is_none());
        assert!(move_syllable_start(&project, &mut state, line, 2, before[2]).is_none());
        assert!(state.syllable_drag.is_none());

        free_syllable_handle(&mut state, id, 2);
        assert!(!syllable_handle_is_placed(&state, id, 2, count));
    }

    #[test]
    fn points_and_liaisons_are_walked_in_reading_order() {
        let (project, id) = karaoke_project("les amis monstres");
        let state = RythmoState::new();
        let line = project.get_line(id).unwrap();
        let lang = project.syllable_language_code();
        let ranges = syllable_char_ranges(line, lang, &state);
        assert_eq!(syllable_text(line, ranges[0]), "les");

        // The liaison of "les amis", then the point of "monstres".
        let marks = syllable_marks(line, lang);
        let [SyllableMark::Liaison(site), SyllableMark::Pin(pin)] = marks.as_slice() else {
            panic!("unexpected {marks:?}");
        };
        assert_eq!(site.pair, "les amis");
        assert_eq!(pin.word, "monstres");
        let shares = syllable_mark_shares(&project, line, lang, &state, &marks);
        assert_eq!(shares.len(), 2);
        assert!(0.0 < shares[0] && shares[0] < shares[1] && shares[1] < 1.0);
    }

    #[test]
    fn the_cursor_stands_on_its_handle_over_the_whole_line_height() {
        let rect = Rect {
            x: 100.0,
            y: 50.0,
            width: 200.0,
            height: 40.0,
        };
        let boundaries = [0.0, 0.25, 0.6, 1.0];
        let mut quads = Vec::new();
        render_syllable_cursor(&rect, &boundaries, 1, false, &mut quads);
        // A halo and the bar of the handle, both centred on it.
        assert_eq!(quads.len(), 2);
        for quad in &quads {
            let [x, y, width, height] = quad.rect;
            assert!((x + width / 2.0 - 150.0).abs() < 0.01);
            assert!(y <= 51.0 && y + height >= 89.0);
        }
        // The two ends of the line are not handles: nothing to show.
        render_syllable_cursor(&rect, &boundaries, 0, false, &mut quads);
        render_syllable_cursor(&rect, &boundaries, 3, false, &mut quads);
        render_syllable_cursor(&rect, &[0.0, 1.0], 4, false, &mut quads);
        assert_eq!(quads.len(), 2);
    }
}
