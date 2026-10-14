//! Step-by-step edit of the text of a line (`Ctrl + T`).
//!
//! The line is cut into sections: the text between two synchronization
//! limits when it has some, else its words. A small text field floats above
//! the current section; `Enter` replaces the section with the typed text and
//! moves to the next one, `Escape` stops. The synchronization points and the
//! karaoke syllable timings keep their place: a word replaced by a word of
//! the same length in syllables keeps every slot exactly.

use super::*;
use crate::ui::text_input::TextInputAction;

const FIELD_H: f32 = 26.0;
const FIELD_PADDING_X: f32 = 6.0;
const FIELD_GAP: f32 = 4.0;
const FIELD_MIN_W: f32 = 48.0;
const FIELD_FONT_SIZE: f32 = 13.0;
const COUNTER_FONT_SIZE: f32 = 10.0;
const COUNTER_W: f32 = 34.0;
const HIGHLIGHT_COLOR: [f32; 4] = [0.31, 0.40, 0.72, 0.30];
const HIGHLIGHT_BORDER: [f32; 4] = [0.55, 0.65, 1.0, 0.9];
const SELECTION_COLOR: [f32; 4] = [0.31, 0.40, 0.72, 0.65];
const FIELD_CURSOR_COLOR: [f32; 4] = [0.92, 0.92, 0.95, 1.0];

fn field_metrics() -> TextInputMetrics {
    TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

/// Character ranges of the sections of a line: the text between its
/// synchronization limits, or its words when it has none. Empty when the
/// line has no text.
pub(crate) fn line_sections(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Vec<(usize, usize)> {
    if let Some(segments) = SyncSegments::for_line(project, line) {
        return segments.ranges();
    }
    word_ranges(&line.text)
}

/// Character ranges of the words of `text`: its runs of non-blank characters.
fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    let mut index = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if let Some(from) = start.take() {
                ranges.push((from, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
        index += 1;
    }
    if let Some(from) = start {
        ranges.push((from, index));
    }
    ranges
}

/// Text of the characters `range` of `text`.
pub(crate) fn section_text(text: &str, range: (usize, usize)) -> String {
    text.chars()
        .skip(range.0)
        .take(range.1.saturating_sub(range.0))
        .collect()
}

pub(crate) struct SectionEditLayout {
    /// The floating text field.
    pub(crate) field: Rect,
    /// The section on the line.
    pub(crate) highlight: Rect,
    /// Where the step counter goes, when it fits.
    pub(crate) counter: Option<Rect>,
}

/// Share of the line's width where the section sits.
fn section_span_ratios(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    state: &RythmoState,
    index: usize,
    range: (usize, usize),
) -> (f32, f32) {
    if let Some(segments) = SyncSegments::for_line(project, line) {
        if index < segments.ranges().len() {
            return segments.time_span(index);
        }
    }
    let count = line.text.chars().count().max(1);
    let start = range.0.min(count);
    let end = range.1.clamp(start, count);
    let lang = project.syllable_language_code();
    if let Some(ratios) = segmented_cursor_ratios_for_line(line, None, lang, false, state) {
        if let (Some(from), Some(to)) = (ratios.get(start), ratios.get(end)) {
            return (*from, (*to).max(*from));
        }
    }
    (start as f32 / count as f32, end as f32 / count as f32)
}

pub(crate) fn section_edit_layout(
    project: &Project,
    state: &RythmoState,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
) -> Option<SectionEditLayout> {
    let edit = state.section_edit.as_ref()?;
    let line = project.get_line(edit.line_id)?;
    let range = *edit.sections.get(edit.index)?;
    let rect = line_rect(
        project,
        line,
        current_frame,
        zone,
        crate::config::reading_bar_offset_seconds(),
        fps,
    );
    if rect.width <= 0.0 || rect.x + rect.width < zone.x || rect.x > zone.x + zone.width {
        return None;
    }
    let (start_ratio, end_ratio) = section_span_ratios(project, line, state, edit.index, range);
    let start_x = rect.x + rect.width * start_ratio;
    let end_x = rect.x + rect.width * end_ratio;
    let highlight = Rect {
        x: start_x,
        y: rect.y,
        width: (end_x - start_x).max(2.0),
        height: rect.height,
    };
    let (text_width, _) =
        crate::vector_text::measure_ui_text_layout_standalone(&edit.text, FIELD_FONT_SIZE);
    let width = (end_x - start_x)
        .max(text_width + FIELD_PADDING_X * 2.0 + 8.0)
        .max(FIELD_MIN_W)
        .min((zone.width - FIELD_GAP * 2.0).max(FIELD_MIN_W));
    let max_x = (zone.x + zone.width - width - FIELD_GAP).max(zone.x);
    let x = start_x.clamp(zone.x + FIELD_GAP, max_x);
    let above = rect.y - FIELD_H - FIELD_GAP;
    let y = if above >= zone.y {
        above
    } else {
        rect.y + rect.height + FIELD_GAP
    };
    let field = Rect {
        x,
        y,
        width,
        height: FIELD_H,
    };
    let counter_x = field.x + field.width + FIELD_GAP;
    let counter = (counter_x + COUNTER_W <= zone.x + zone.width).then_some(Rect {
        x: counter_x,
        y,
        width: COUNTER_W,
        height: FIELD_H,
    });
    Some(SectionEditLayout {
        field,
        highlight,
        counter,
    })
}

/// Input of the step-by-step edit. `None` when no such edit is running, or
/// for events it lets through (a click away from the field ends the edit and
/// then acts as usual).
pub(crate) fn handle_section_edit_event(
    ctx: &RythmoCtx,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    state.section_edit.as_ref()?;
    match event {
        UiEvent::KeyInput { text } => Some(handle_key(ctx, state, text)),
        UiEvent::CursorLeft => Some(move_caret(state, -1, false)),
        UiEvent::CursorRight => Some(move_caret(state, 1, false)),
        UiEvent::ShiftCursorLeft => Some(move_caret(state, -1, true)),
        UiEvent::ShiftCursorRight => Some(move_caret(state, 1, true)),
        UiEvent::MoveWordLeft => Some(move_word(state, -1, false)),
        UiEvent::MoveWordRight => Some(move_word(state, 1, false)),
        UiEvent::SelectWordLeft => Some(move_word(state, -1, true)),
        UiEvent::SelectWordRight => Some(move_word(state, 1, true)),
        UiEvent::Home => Some(move_boundary(state, false)),
        UiEvent::End => Some(move_boundary(state, true)),
        UiEvent::CursorUp | UiEvent::CursorDown => Some(EventResponse::Consumed),
        UiEvent::SelectAll => {
            let edit = state.section_edit.as_mut()?;
            edit.input.select_all(&edit.text);
            Some(selection_response(&edit.input, &edit.text))
        }
        UiEvent::Copy => {
            let edit = state.section_edit.as_ref()?;
            Some(
                edit.input
                    .selected_text(&edit.text)
                    .map(|text| EventResponse::Action(UiAction::SetClipboard(text)))
                    .unwrap_or(EventResponse::Consumed),
            )
        }
        UiEvent::Cut => {
            let edit = state.section_edit.as_mut()?;
            let Some(selected) = edit.input.selected_text(&edit.text) else {
                return Some(EventResponse::Consumed);
            };
            let current = edit.text.clone();
            if let Some(TextInputAction::Changed(text)) = edit.input.handle_key("\x08", &current) {
                edit.text = text;
            }
            Some(EventResponse::Action(UiAction::SetClipboard(selected)))
        }
        UiEvent::UndoTextEdit => {
            let edit = state.section_edit.as_mut()?;
            let current = edit.text.clone();
            if let Some(text) = edit.input.undo(&current) {
                edit.text = text;
            }
            Some(EventResponse::Consumed)
        }
        UiEvent::Delete => Some(handle_key(ctx, state, "\x7f")),
        UiEvent::MousePress { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y } => {
            let layout =
                section_edit_layout(ctx.project, state, ctx.current_frame, ctx.zone, ctx.fps);
            match layout.filter(|layout| layout.field.contains(*x, *y)) {
                Some(layout) => {
                    let edit = state.section_edit.as_mut()?;
                    if matches!(event, UiEvent::DoubleClick { .. }) {
                        edit.input.select_all(&edit.text);
                    } else {
                        let position = text_input::cursor_pos_from_x(
                            &edit.text,
                            layout.field,
                            *x,
                            field_metrics(),
                        );
                        if matches!(event, UiEvent::ShiftMousePress { .. }) {
                            edit.input.update_selection(position);
                        } else {
                            edit.input.set_cursor_pos(position);
                        }
                    }
                    Some(EventResponse::Consumed)
                }
                None => {
                    // A click elsewhere ends the edit; the click then acts as usual.
                    state.stop_section_editing();
                    None
                }
            }
        }
        UiEvent::MouseRelease { x, y } => {
            let layout =
                section_edit_layout(ctx.project, state, ctx.current_frame, ctx.zone, ctx.fps);
            layout
                .filter(|layout| layout.field.contains(*x, *y))
                .map(|_| EventResponse::Consumed)
        }
        _ => None,
    }
}

fn handle_key(ctx: &RythmoCtx, state: &mut RythmoState, text: &str) -> EventResponse {
    match text {
        "\x1b" => {
            state.stop_section_editing();
            EventResponse::Action(UiAction::StopEditing)
        }
        "\r" | "\n" => commit_section(ctx, state),
        _ => {
            let Some(edit) = state.section_edit.as_mut() else {
                return EventResponse::Consumed;
            };
            let current = edit.text.clone();
            if let Some(TextInputAction::Changed(new_text)) = edit.input.handle_key(text, &current)
            {
                edit.text = new_text;
            }
            EventResponse::Consumed
        }
    }
}

/// `Enter`: replaces the section with the typed text, or just moves on when
/// nothing changed.
fn commit_section(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    let Some(edit) = state.section_edit.as_ref() else {
        return EventResponse::Consumed;
    };
    let (Some(line), Some(&range)) = (
        ctx.project.get_line(edit.line_id),
        edit.sections.get(edit.index),
    ) else {
        state.stop_section_editing();
        return EventResponse::Action(UiAction::StopEditing);
    };
    if edit.text == section_text(&line.text, range) {
        return advance(state, &line.text);
    }
    let limits_before = SyncSegments::for_line(ctx.project, line)
        .map_or(0, |segments| segments.limits_before_segment(edit.index));
    EventResponse::Action(UiAction::ReplaceLineSection {
        id: line.id,
        range,
        text: edit.text.clone(),
        limits_before,
    })
}

fn advance(state: &mut RythmoState, text: &str) -> EventResponse {
    match state.advance_section_edit(text) {
        Some(label) => EventResponse::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Selection { label },
        )),
        None => EventResponse::Action(UiAction::StopEditing),
    }
}

fn move_caret(state: &mut RythmoState, direction: i32, shift: bool) -> EventResponse {
    let Some(edit) = state.section_edit.as_mut() else {
        return EventResponse::Consumed;
    };
    match (direction < 0, shift) {
        (true, true) => edit.input.move_left_shift(),
        (true, false) => edit.input.move_left(),
        (false, true) => edit.input.move_right_shift(&edit.text),
        (false, false) => edit.input.move_right(&edit.text),
    }
    if shift {
        selection_response(&edit.input, &edit.text)
    } else {
        EventResponse::Consumed
    }
}

fn move_word(state: &mut RythmoState, direction: i32, shift: bool) -> EventResponse {
    let Some(edit) = state.section_edit.as_mut() else {
        return EventResponse::Consumed;
    };
    match (direction < 0, shift) {
        (true, true) => edit.input.move_word_left_shift(&edit.text),
        (true, false) => edit.input.move_word_left(&edit.text),
        (false, true) => edit.input.move_word_right_shift(&edit.text),
        (false, false) => edit.input.move_word_right(&edit.text),
    }
    if shift {
        selection_response(&edit.input, &edit.text)
    } else {
        EventResponse::Consumed
    }
}

fn move_boundary(state: &mut RythmoState, end: bool) -> EventResponse {
    let Some(edit) = state.section_edit.as_mut() else {
        return EventResponse::Consumed;
    };
    if end {
        edit.input.move_end(&edit.text, false);
    } else {
        edit.input.move_home(false);
    }
    EventResponse::Consumed
}

pub(crate) fn render_section_edit<'a>(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    state: &'a RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let Some(layout) = section_edit_layout(project, state, current_frame, zone, fps) else {
        return;
    };
    let Some(edit) = state.section_edit.as_ref() else {
        return;
    };
    // The section being replaced, on the line itself.
    quads.push(QuadInstance {
        rect: [
            layout.highlight.x,
            layout.highlight.y,
            layout.highlight.width,
            layout.highlight.height,
        ],
        color: HIGHLIGHT_COLOR,
        color_bottom: HIGHLIGHT_COLOR,
        border_color: HIGHLIGHT_BORDER,
        border_width: 1.5,
        border_radius: 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    context_menu::render_panel(quads, layout.field);
    labels.push(LabelInfo {
        text: &edit.text,
        bounds: Rect {
            x: layout.field.x + FIELD_PADDING_X,
            y: layout.field.y,
            width: (layout.field.width - FIELD_PADDING_X * 2.0).max(0.0),
            height: layout.field.height,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(FIELD_FONT_SIZE),
        color_override: Some([235, 235, 242]),
        font_family_override: None,
    });
    text_input::render_selection_and_cursor(
        quads,
        layout.field,
        &edit.text,
        &edit.input,
        true,
        field_metrics(),
        5.0,
        6.0,
        SELECTION_COLOR,
        FIELD_CURSOR_COLOR,
    );
    if let Some(counter) = layout.counter {
        labels.push(LabelInfo {
            text: &edit.step_label,
            bounds: counter,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(COUNTER_FONT_SIZE),
            color_override: Some([170, 170, 182]),
            font_family_override: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::{DetectionDocument, MediaTick};

    fn line_with_limits(text: &str, limits: &[(u32, i64)]) -> (Project, u64) {
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
        if !limits.is_empty() {
            project.restore_line_detections(line_id, detections.line(line_id).unwrap().clone());
        }
        (project, line_id)
    }

    fn press(project: &Project, state: &mut RythmoState, event: UiEvent) -> EventResponse {
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(project);
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        handle_rythmo_event(
            &event,
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

    fn key(text: &str) -> UiEvent {
        UiEvent::KeyInput { text: text.into() }
    }

    #[test]
    fn words_are_the_sections_of_a_line_without_limits() {
        assert_eq!(
            word_ranges("un  deux trois "),
            vec![(0, 2), (4, 8), (9, 14)]
        );
        assert_eq!(word_ranges("   "), Vec::<(usize, usize)>::new());
        assert_eq!(word_ranges(""), Vec::<(usize, usize)>::new());
        assert_eq!(section_text("un deux", (3, 7)), "deux");
        let (project, line_id) = line_with_limits("Bonjour, le monde", &[]);
        let line = project.get_line(line_id).unwrap();
        assert_eq!(
            line_sections(&project, line),
            vec![(0, 8), (9, 11), (12, 17)]
        );
    }

    #[test]
    fn limits_cut_a_line_into_its_sections() {
        let (project, line_id) = line_with_limits("un deux trois", &[(3, 30), (8, 60)]);
        let line = project.get_line(line_id).unwrap();
        assert_eq!(line_sections(&project, line), vec![(0, 3), (3, 8), (8, 13)]);
        let segments = SyncSegments::for_line(&project, line).unwrap();
        assert_eq!(segments.time_span(0), (0.0, 0.3));
        assert_eq!(segments.time_span(1), (0.3, 0.6));
        assert_eq!(segments.time_span(2), (0.6, 1.0));
        assert_eq!(segments.limits_before_segment(0), 0);
        assert_eq!(segments.limits_before_segment(1), 1);
    }

    #[test]
    fn enter_replaces_the_section_and_moves_to_the_next_one() {
        let (mut project, line_id) = line_with_limits("un deux trois", &[(3, 30), (8, 60)]);
        let line = project.get_line(line_id).unwrap();
        let mut state = RythmoState::new();
        let first = state.start_section_edit(line_id, &line.text, line_sections(&project, line));
        assert_eq!(first.as_deref(), Some("un "));
        assert!(state.is_editing());

        // The section's text starts selected: typing replaces it.
        assert_eq!(
            press(&project, &mut state, key("X")),
            EventResponse::Consumed
        );
        assert_eq!(state.section_edit.as_ref().unwrap().text, "X");
        let EventResponse::Action(UiAction::ReplaceLineSection {
            id,
            range,
            text,
            limits_before,
        }) = press(&project, &mut state, key("\r"))
        else {
            panic!("Enter must replace the section");
        };
        assert_eq!(
            (id, range, text.as_str(), limits_before),
            (line_id, (0, 3), "X", 0)
        );

        // The state moves on once the project applied the replacement.
        project.get_line_mut(line_id).unwrap().text = "Xdeux trois".into();
        let next = state.section_edit_replaced(1, "Xdeux trois", vec![(0, 1), (1, 6), (6, 11)]);
        assert_eq!(next.as_deref(), Some("deux "));
        assert_eq!(state.section_edit.as_ref().unwrap().index, 1);
        assert_eq!(state.section_edit.as_ref().unwrap().step_label, "2/3");

        // Enter on an unchanged section only moves on.
        assert!(matches!(
            press(&project, &mut state, key("\r")),
            EventResponse::Action(UiAction::Accessibility(_))
        ));
        assert_eq!(state.section_edit.as_ref().unwrap().index, 2);
        // Escape ends the edit.
        assert_eq!(
            press(&project, &mut state, key("\x1b")),
            EventResponse::Action(UiAction::StopEditing)
        );
        assert!(state.section_edit.is_none());
    }

    #[test]
    fn the_edit_ends_after_the_last_section_and_on_a_click_away() {
        let (project, line_id) = line_with_limits("un deux", &[]);
        let line = project.get_line(line_id).unwrap();
        let mut state = RythmoState::new();
        state.start_section_edit(line_id, &line.text, line_sections(&project, line));
        assert_eq!(
            state.advance_section_edit(&line.text).as_deref(),
            Some("deux")
        );
        assert_eq!(state.advance_section_edit(&line.text), None);
        assert!(state.section_edit.is_none());

        state.start_section_edit(line_id, &line.text, line_sections(&project, line));
        // A click far from the field ends the edit and acts as usual.
        press(
            &project,
            &mut state,
            UiEvent::MousePress { x: 790.0, y: 235.0 },
        );
        assert!(state.section_edit.is_none());
    }

    #[test]
    fn typed_words_shift_the_following_sections() {
        let (project, line_id) = line_with_limits("un deux trois", &[]);
        let line = project.get_line(line_id).unwrap();
        let mut state = RythmoState::new();
        state.start_section_edit(line_id, &line.text, line_sections(&project, line));
        // "un" becomes "a b": one more word than before, so the ranges are
        // shifted instead of recut, and the next step is still "deux".
        let next = state.section_edit_replaced(3, "a b deux trois", word_ranges("a b deux trois"));
        assert_eq!(next.as_deref(), Some("deux"));
        assert_eq!(
            state.section_edit.as_ref().unwrap().sections,
            vec![(0, 3), (4, 8), (9, 14)]
        );
    }
}
