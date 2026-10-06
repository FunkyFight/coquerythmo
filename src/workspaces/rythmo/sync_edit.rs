//! Keeps text edits inside the segment between two synchronization limits.
//!
//! A limit ties a place in the text to a place in time, so the text between two
//! limits is stretched over the time between them. Typing or erasing must only
//! change the segment the caret is in: that segment stretches, while its
//! neighbours and the limits stay where they are.

use super::*;
use crate::detection::TextEditSpan;

/// The segments of one line, cut by its synchronization limits.
pub(crate) struct SyncSegments {
    /// Place of each limit in time order: (grapheme position, character
    /// position).
    limits: Vec<(usize, usize)>,
    /// Time of each limit as a share of the line, in time order.
    ratios: Vec<f32>,
    char_count: usize,
}

impl SyncSegments {
    /// `None` when the line has no limit to respect.
    pub(crate) fn for_line(
        project: &Project,
        line: &crate::rythmo_line::RythmoLine,
    ) -> Option<Self> {
        if !line.kind.is_dialogue() || line.karaoke || line.duration_frames <= 0 {
            return None;
        }
        let data = project.detections().line(line.id)?;
        if data.sync_points().is_empty() {
            return None;
        }
        let mut char_at_boundary = vec![0usize];
        for grapheme in UnicodeSegmentation::graphemes(line.text.as_str(), true) {
            let previous = char_at_boundary.last().copied().unwrap_or(0);
            char_at_boundary.push(previous + grapheme.chars().count());
        }
        let char_count = char_at_boundary.last().copied().unwrap_or(0);
        let mut previous = 0usize;
        let limits = project
            .detections()
            .sync_limit_positions(line.id, &line.text)
            .into_iter()
            .map(|grapheme| {
                let boundary = grapheme.min(char_at_boundary.len() - 1);
                previous = previous.max(char_at_boundary[boundary]);
                (grapheme, previous)
            })
            .collect();
        let ratios = data
            .sync_points()
            .iter()
            .map(|point| {
                ((point.line_tick.as_frame_position() - line.start_frame as f64)
                    / line.duration_frames as f64) as f32
            })
            .collect();
        Some(Self {
            limits,
            ratios,
            char_count,
        })
    }

    fn segment_count(&self) -> usize {
        self.limits.len() + 1
    }

    /// Character position of the edge `index` of the segments: the start of
    /// the line, each limit in turn, then the end of the line.
    fn edge(&self, index: usize) -> usize {
        if index == 0 {
            0
        } else if index > self.limits.len() {
            self.char_count
        } else {
            self.limits[index - 1].1
        }
    }

    /// First and last character position of a segment.
    pub(crate) fn range(&self, segment: usize) -> (usize, usize) {
        (self.edge(segment), self.edge(segment + 1))
    }

    fn contains(&self, segment: usize, caret: usize) -> bool {
        let (start, end) = self.range(segment);
        start <= caret && caret <= end
    }

    fn containing(&self, caret: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.segment_count()).filter(move |segment| self.contains(*segment, caret))
    }

    /// The segment the caret writes in. A caret standing on a limit belongs to
    /// the segment it was last steered into; otherwise the segment before the
    /// limit wins, so that typing goes on where it started.
    pub(crate) fn owner(&self, caret: usize, remembered: Option<usize>) -> usize {
        remembered
            .filter(|segment| *segment < self.segment_count() && self.contains(*segment, caret))
            .or_else(|| self.containing(caret).next())
            .unwrap_or(self.limits.len())
    }

    /// The segment a caret that has just moved onto `caret` belongs to: the one
    /// it came from, so it stays on the side it was approached from.
    pub(crate) fn arrival_owner(&self, caret: usize, from_left: bool) -> usize {
        let mut segments = self.containing(caret);
        let arrived_in = if from_left {
            segments.next()
        } else {
            segments.last()
        };
        arrived_in.unwrap_or(self.limits.len())
    }

    /// The segment under a click, when the caret landed inside it.
    pub(crate) fn owner_at_ratio(&self, caret: usize, ratio: f32) -> Option<usize> {
        let segment = self.ratios.iter().filter(|limit| **limit <= ratio).count();
        self.contains(segment, caret).then_some(segment)
    }

    /// Erasing must not reach into a neighbouring segment: Backspace stops at
    /// the start of the segment and Delete at its end.
    pub(crate) fn blocks_erase(
        &self,
        key: &str,
        caret: usize,
        has_selection: bool,
        owner: usize,
    ) -> bool {
        if has_selection {
            return false;
        }
        let (start, end) = self.range(owner);
        match key {
            "\x08" => caret == start && start > 0,
            "\x7f" => caret == end && end < self.char_count,
            _ => false,
        }
    }

    /// Exact place of an edit of the line text, given the characters it
    /// replaced, plus which limits stay on its left. `None` when the edit
    /// cannot be described in graphemes.
    pub(crate) fn edit_span(
        &self,
        old_text: &str,
        new_text: &str,
        planned: (usize, usize),
        owner: usize,
    ) -> Option<TextEditSpan> {
        let mut span = TextEditSpan::from_char_edit(old_text, new_text, planned.0, planned.1)?;
        let start = span.start as usize;
        let staying = if span.inserted == 0 {
            // Erased text: undoing puts it back in the segment it left, so the
            // limits that were already before it must stay before it.
            self.limits
                .iter()
                .filter(|(grapheme, _)| *grapheme == start)
                .count()
        } else if span.removed == 0 {
            // Typed text: the limits of the segments before the caret's stay
            // before the text, the others follow it.
            self.limits
                .iter()
                .take(owner)
                .filter(|(grapheme, _)| *grapheme == start)
                .count()
        } else {
            0
        };
        span.limits_before = staying as u32;
        Some(span)
    }
}

/// Characters an editing key is about to replace: `(start, removed)`.
pub(crate) fn planned_edit(
    key: &str,
    caret: usize,
    selection: Option<(usize, usize)>,
    char_count: usize,
) -> Option<(usize, usize)> {
    if let Some((start, end)) = selection {
        return Some((start, end.saturating_sub(start)));
    }
    match key {
        "\x08" => caret.checked_sub(1).map(|start| (start, 1)),
        "\x7f" => (caret < char_count).then_some((caret, 1)),
        "\x1b" | "\r" | "\n" | "" => None,
        _ => Some((caret, 0)),
    }
}

pub(crate) fn remembered_caret_segment(state: &RythmoState, line_id: u64) -> Option<usize> {
    state
        .sync_caret_segment
        .filter(|(id, _)| *id == line_id)
        .map(|(_, segment)| segment)
}

/// Records the segment of a caret the keyboard has just moved.
pub(crate) fn remember_caret_segment(project: &Project, state: &mut RythmoState, from_left: bool) {
    let Some(line_id) = state.editing_line else {
        return;
    };
    let caret = state.line_input.cursor_pos;
    state.sync_caret_segment = project
        .get_line(line_id)
        .and_then(|line| SyncSegments::for_line(project, line))
        .map(|segments| (line_id, segments.arrival_owner(caret, from_left)));
}

/// Records the segment of a caret placed by a click at `ratio` of the line.
pub(crate) fn remember_clicked_segment(
    project: &Project,
    state: &mut RythmoState,
    line_id: u64,
    ratio: f32,
) {
    let caret = state.line_input.cursor_pos;
    state.sync_caret_segment = project
        .get_line(line_id)
        .and_then(|line| SyncSegments::for_line(project, line))
        .and_then(|segments| segments.owner_at_ratio(caret, ratio))
        .map(|segment| (line_id, segment));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{DetectionDocument, MediaTick};

    fn segments_for(text: &str, limits: &[(u32, i64)]) -> (Project, u64) {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 100, 0.0);
        project.get_line_mut(line_id).unwrap().text = text.into();
        let count = UnicodeSegmentation::graphemes(text, true).count();
        let mut detections = DetectionDocument::default();
        for (grapheme, frame) in limits {
            detections
                .add_sync_point(
                    line_id,
                    count,
                    MediaTick::from_frame(0),
                    MediaTick::from_frame(100),
                    *grapheme,
                    MediaTick::from_frame(*frame),
                )
                .unwrap();
        }
        project.restore_line_detections(line_id, detections.line(line_id).unwrap().clone());
        (project, line_id)
    }

    fn segments(project: &Project, line_id: u64) -> SyncSegments {
        SyncSegments::for_line(project, project.get_line(line_id).unwrap()).unwrap()
    }

    #[test]
    fn a_line_without_limits_has_no_segments() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 100, 0.0);
        project.get_line_mut(line_id).unwrap().text = "Bonjour".into();
        assert!(SyncSegments::for_line(&project, project.get_line(line_id).unwrap()).is_none());
    }

    #[test]
    fn limits_cut_the_line_into_segments() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let segments = segments(&project, line_id);
        assert_eq!(segments.range(0), (0, 3));
        assert_eq!(segments.range(1), (3, 8));
        assert_eq!(segments.range(2), (8, 13));
    }

    #[test]
    fn a_caret_on_a_limit_stays_with_the_segment_it_was_steered_into() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let segments = segments(&project, line_id);
        // Default: the segment before the limit.
        assert_eq!(segments.owner(3, None), 0);
        assert_eq!(segments.owner(3, Some(1)), 1);
        // A remembered segment that no longer holds the caret is ignored.
        assert_eq!(segments.owner(5, Some(0)), 1);
        assert_eq!(segments.arrival_owner(3, true), 0);
        assert_eq!(segments.arrival_owner(3, false), 1);
    }

    #[test]
    fn backspace_and_delete_stop_at_the_edges_of_the_segment() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let segments = segments(&project, line_id);
        // At the start of the middle segment, Backspace would eat the "n".
        assert!(segments.blocks_erase("\x08", 3, false, 1));
        // At the end of the first segment, Backspace erases its own text...
        assert!(!segments.blocks_erase("\x08", 3, false, 0));
        // ...while Delete would eat the space that opens the next one.
        assert!(segments.blocks_erase("\x7f", 3, false, 0));
        assert!(!segments.blocks_erase("\x7f", 3, false, 1));
        // Inside a segment, and at the edges of the line, nothing is blocked.
        assert!(!segments.blocks_erase("\x08", 5, false, 1));
        assert!(!segments.blocks_erase("\x08", 0, false, 0));
        assert!(!segments.blocks_erase("\x7f", 13, false, 2));
        // Erasing a selection is an explicit choice.
        assert!(!segments.blocks_erase("\x08", 3, true, 1));
        // Typing is never blocked.
        assert!(!segments.blocks_erase("a", 3, false, 1));
    }

    #[test]
    fn a_click_picks_the_segment_under_the_pointer_when_the_caret_landed_in_it() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let segments = segments(&project, line_id);
        // The caret lands on the limit at character 3 whichever side is hit.
        assert_eq!(segments.owner_at_ratio(3, 0.25), Some(0));
        assert_eq!(segments.owner_at_ratio(3, 0.35), Some(1));
        // A click in a segment that does not hold the caret picks nothing.
        assert_eq!(segments.owner_at_ratio(3, 0.9), None);
    }

    #[test]
    fn edit_spans_tell_which_limits_stay_before_the_typed_text() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let segments = segments(&project, line_id);
        let old = "un deux trois";
        let new = "un Xdeux trois";
        // Caret at character 3 in the middle segment: the limit before it
        // stays before the typed "X".
        let span = segments.edit_span(old, new, (3, 0), 1).unwrap();
        assert_eq!((span.start, span.removed, span.inserted), (3, 0, 1));
        assert_eq!(span.limits_before, 1);
        // The same place in the first segment: the limit follows the text.
        let span = segments.edit_span(old, new, (3, 0), 0).unwrap();
        assert_eq!(span.limits_before, 0);
    }

    fn press(project: &Project, state: &mut RythmoState, key: &str) -> EventResponse {
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        handle_rythmo_event(
            &UiEvent::KeyInput { text: key.into() },
            &zone,
            project,
            &render_index,
            50.0,
            false,
            24.0,
            state,
            ToolMode::Select,
            [1.0; 4],
            0.012,
            false,
            RythmoInteractionMode::Editable,
        )
    }

    #[test]
    fn keys_never_cross_a_limit_and_report_the_place_of_the_edit() {
        let (project, line_id) = segments_for("un deux trois", &[(3, 30), (8, 60)]);
        let text = project.get_line(line_id).unwrap().text.clone();
        let mut state = RythmoState::new();
        state.start_editing_line(line_id, &text);

        // Caret on the first limit, steered into the segment after it: it must
        // not eat the letter before.
        state.line_input.set_cursor_pos(3);
        state.sync_caret_segment = Some((line_id, 1));
        assert!(matches!(
            press(&project, &mut state, "\x08"),
            EventResponse::Action(UiAction::Accessibility(_))
        ));
        assert_eq!(state.line_input.cursor_pos, 3);

        // Steered into the segment before the limit, Delete must not eat the
        // letter after, while Backspace erases the segment's own last letter.
        state.sync_caret_segment = Some((line_id, 0));
        assert!(matches!(
            press(&project, &mut state, "\x7f"),
            EventResponse::Action(UiAction::Accessibility(_))
        ));
        let EventResponse::Action(UiAction::UpdateLineText { text, edit, .. }) =
            press(&project, &mut state, "\x08")
        else {
            panic!("Backspace inside the segment must edit the text");
        };
        assert_eq!(text, "un deux trois".replacen(' ', "", 1));
        let edit = edit.expect("the place of the edit is reported");
        assert_eq!((edit.start, edit.removed, edit.inserted), (2, 1, 0));

        // Typing where the limit sits goes to the segment that owns the caret.
        state.line_input.set_cursor_pos(3);
        state.sync_caret_segment = Some((line_id, 1));
        let EventResponse::Action(UiAction::UpdateLineText { text, edit, .. }) =
            press(&project, &mut state, "X")
        else {
            panic!("typing must edit the text");
        };
        assert_eq!(text, "un Xdeux trois");
        let edit = edit.expect("the place of the edit is reported");
        assert_eq!((edit.start, edit.removed, edit.inserted), (3, 0, 1));
        assert_eq!(edit.limits_before, 1);
        assert_eq!(state.sync_caret_segment, Some((line_id, 1)));
    }

    #[test]
    fn lines_without_limits_are_edited_as_before() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 100, 0.0);
        project.get_line_mut(line_id).unwrap().text = "un deux".into();
        let mut state = RythmoState::new();
        state.start_editing_line(line_id, "un deux");
        state.line_input.set_cursor_pos(3);

        let EventResponse::Action(UiAction::UpdateLineText { text, edit, .. }) =
            press(&project, &mut state, "\x08")
        else {
            panic!("Backspace must edit the text");
        };
        assert_eq!(text, "undeux");
        assert_eq!(edit, None);
    }

    #[test]
    fn planned_edits_follow_the_key_and_the_selection() {
        assert_eq!(planned_edit("a", 4, None, 10), Some((4, 0)));
        assert_eq!(planned_edit("\x08", 4, None, 10), Some((3, 1)));
        assert_eq!(planned_edit("\x08", 0, None, 10), None);
        assert_eq!(planned_edit("\x7f", 4, None, 10), Some((4, 1)));
        assert_eq!(planned_edit("\x7f", 10, None, 10), None);
        assert_eq!(planned_edit("a", 4, Some((2, 6)), 10), Some((2, 4)));
        assert_eq!(planned_edit("\r", 4, None, 10), None);
    }
}
