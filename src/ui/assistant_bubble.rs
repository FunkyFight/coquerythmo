//! The floating prompt of Needle 3 (`Ctrl + Entrée`): one text field, the
//! state of the request and its answer.

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input;
use crate::i18n::t;

const FIELD_FONT: f32 = 15.0;
const FIELD_PADDING: f32 = 12.0;
const STATUS_FONT: f32 = 13.0;
const STATUS_LINE_HEIGHT: f32 = 19.0;
const MARGIN: f32 = 18.0;

pub enum BubbleStatus {
    Idle,
    Thinking {
        query: String,
    },
    Done {
        query: String,
        message: String,
        ok: bool,
    },
}

pub struct AssistantBubble {
    query: String,
    input: text_input::TextInputState,
    status: BubbleStatus,
}

pub enum AssistantBubbleResult {
    Consumed,
    Close,
    /// Enter on a non-empty query.
    Submit(String),
    /// Copy or cut: the text to put on the clipboard.
    Clipboard(String),
}

struct Layout {
    card: Rect,
    field: Rect,
    status_top: f32,
    status_lines: usize,
}

impl Default for AssistantBubble {
    fn default() -> Self {
        Self::new()
    }
}

impl AssistantBubble {
    pub fn new() -> Self {
        let mut input = text_input::TextInputState::new();
        input.activate("");
        Self {
            query: String::new(),
            input,
            status: BubbleStatus::Idle,
        }
    }

    pub fn is_thinking(&self) -> bool {
        matches!(self.status, BubbleStatus::Thinking { .. })
    }

    pub fn set_thinking(&mut self, query: String) {
        self.status = BubbleStatus::Thinking { query };
    }

    pub fn set_result(&mut self, message: String, ok: bool) {
        let query = match &self.status {
            BubbleStatus::Thinking { query, .. } | BubbleStatus::Done { query, .. } => query.clone(),
            BubbleStatus::Idle => String::new(),
        };
        self.status = BubbleStatus::Done { query, message, ok };
        self.input.activate(&self.query);
    }

    /// Text of the result line, for the screen reader.
    pub fn status_text(&self) -> Option<&str> {
        match &self.status {
            BubbleStatus::Done { message, .. } => Some(message.as_str()),
            _ => None,
        }
    }

    pub fn next_cursor_blink_deadline(&self) -> std::time::Instant {
        self.input
            .next_cursor_blink_deadline()
            .unwrap_or_else(std::time::Instant::now)
    }

    /// The echoed query (one line) and the message under the field.
    fn status_parts(&self) -> Option<(Option<&str>, &str)> {
        match &self.status {
            BubbleStatus::Idle => None,
            BubbleStatus::Thinking { query } => Some((Some(query.as_str()), t("assistant.thinking"))),
            BubbleStatus::Done { query, message, .. } => Some((Some(query.as_str()), message.as_str())),
        }
    }

    fn layout(&self, sw: f32, sh: f32) -> Layout {
        let width = (sw - 2.0 * MARGIN).clamp(240.0, 640.0);
        let x = ((sw - width) / 2.0).max(0.0);
        let y = (sh * 0.14).max(48.0);
        let status_lines = self
            .status_parts()
            .map(|(query, message)| {
                usize::from(query.is_some())
                    + super::pricing_page::wrap_text(message, width - 2.0 * MARGIN, STATUS_FONT).len()
            })
            .unwrap_or(0);
        let field = Rect {
            x: x + MARGIN,
            y: y + 50.0,
            width: width - 2.0 * MARGIN,
            height: 38.0,
        };
        let hint_bottom = field.y + field.height + 8.0 + 16.0;
        let status_top = hint_bottom + 10.0;
        let height = if status_lines == 0 {
            hint_bottom - y + MARGIN
        } else {
            status_top - y + status_lines as f32 * STATUS_LINE_HEIGHT + MARGIN
        };
        Layout {
            card: Rect {
                x,
                y,
                width,
                height,
            },
            field,
            status_top,
            status_lines,
        }
    }

    fn metrics() -> text_input::TextInputMetrics {
        text_input::TextInputMetrics::left(FIELD_FONT, FIELD_PADDING)
    }

    pub fn handle_event(&mut self, event: &UiEvent, sw: f32, sh: f32) -> AssistantBubbleResult {
        let layout = self.layout(sw, sh);
        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return AssistantBubbleResult::Close;
                }
                if text == "\r" || text == "\n" {
                    if self.is_thinking() {
                        return AssistantBubbleResult::Consumed;
                    }
                    let query = self.query.trim().to_string();
                    if query.is_empty() {
                        return AssistantBubbleResult::Consumed;
                    }
                    self.query.clear();
                    self.input.activate("");
                    return AssistantBubbleResult::Submit(query);
                }
                if text == "\t" || text == "\u{b}" {
                    return AssistantBubbleResult::Consumed;
                }
                if let Some(text_input::TextInputAction::Changed(new_text)) =
                    self.input.handle_key(text, &self.query)
                {
                    self.query = new_text;
                }
                AssistantBubbleResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                AssistantBubbleResult::Consumed
            }
            UiEvent::CursorRight => {
                self.input.move_right(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::MoveWordLeft => {
                self.input.move_word_left(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::MoveWordRight => {
                self.input.move_word_right(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::ShiftCursorLeft => {
                self.input.move_left_shift();
                AssistantBubbleResult::Consumed
            }
            UiEvent::ShiftCursorRight => {
                self.input.move_right_shift(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::SelectWordLeft => {
                self.input.move_word_left_shift(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::SelectWordRight => {
                self.input.move_word_right_shift(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::Home => {
                self.input.move_home(false);
                AssistantBubbleResult::Consumed
            }
            UiEvent::End => {
                self.input.move_end(&self.query, false);
                AssistantBubbleResult::Consumed
            }
            UiEvent::SelectAll => {
                self.input.select_all(&self.query);
                AssistantBubbleResult::Consumed
            }
            UiEvent::Copy => match self.input.selected_text(&self.query) {
                Some(text) if !text.is_empty() => AssistantBubbleResult::Clipboard(text),
                _ => AssistantBubbleResult::Consumed,
            },
            UiEvent::Cut => match self.input.selected_text(&self.query) {
                Some(text) if !text.is_empty() => {
                    if let Some(text_input::TextInputAction::Changed(new_text)) =
                        self.input.handle_key("\x7f", &self.query)
                    {
                        self.query = new_text;
                    }
                    AssistantBubbleResult::Clipboard(text)
                }
                _ => AssistantBubbleResult::Consumed,
            },
            UiEvent::UndoTextEdit => {
                if let Some(text) = self.input.undo(&self.query) {
                    self.query = text;
                }
                AssistantBubbleResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if layout.field.contains(*x, *y) {
                    self.input.activate(&self.query);
                    let pos = text_input::cursor_pos_from_x(&self.query, layout.field, *x, Self::metrics());
                    if matches!(event, UiEvent::DoubleClick { .. }) {
                        self.input.select_word_at(&self.query, pos);
                    } else {
                        self.input.set_cursor_pos(pos);
                    }
                    return AssistantBubbleResult::Consumed;
                }
                if layout.card.contains(*x, *y) {
                    AssistantBubbleResult::Consumed
                } else {
                    AssistantBubbleResult::Close
                }
            }
            _ => AssistantBubbleResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        sw: f32,
        sh: f32,
    ) {
        let layout = self.layout(sw, sh);
        let card = layout.card;
        quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.17, 0.17, 0.22, 0.98],
            color_bottom: [0.12, 0.12, 0.16, 0.98],
            border_color: [0.45, 0.50, 0.85, 0.9],
            border_width: 1.5,
            border_radius: 16.0,
            shadow_offset: [0.0, 8.0],
            shadow_color: [0.0, 0.0, 0.0, 0.55],
            shadow_blur: 22.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        labels.push(LabelInfo {
            text: t("assistant.title"),
            bounds: Rect {
                x: card.x + MARGIN,
                y: card.y + 14.0,
                width: card.width - 2.0 * MARGIN,
                height: 26.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(17.0),
            color_override: Some([236, 236, 246]),
            font_family_override: None,
        });

        let field = layout.field;
        quads.push(QuadInstance {
            rect: [field.x, field.y, field.width, field.height],
            color: [0.07, 0.07, 0.10, 1.0],
            color_bottom: [0.07, 0.07, 0.10, 1.0],
            border_color: if self.is_thinking() {
                [0.35, 0.35, 0.45, 0.8]
            } else {
                [0.45, 0.50, 0.90, 0.9]
            },
            border_width: 1.0,
            border_radius: 8.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        if self.query.is_empty() {
            labels.push(LabelInfo {
                text: t("assistant.placeholder"),
                bounds: field,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: FIELD_PADDING,
                font_size_override: Some(FIELD_FONT),
                color_override: Some([120, 122, 140]),
                font_family_override: None,
            });
        } else {
            labels.push(LabelInfo {
                text: self.query.as_str(),
                bounds: field,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: FIELD_PADDING,
                font_size_override: Some(FIELD_FONT),
                color_override: Some([240, 240, 246]),
                font_family_override: None,
            });
        }
        text_input::render_selection_and_cursor(
            quads,
            field,
            &self.query,
            &self.input,
            !self.is_thinking(),
            Self::metrics(),
            6.0,
            6.0,
            [0.25, 0.45, 0.95, 0.42],
            [0.90, 0.90, 0.98, 1.0],
        );

        labels.push(LabelInfo {
            text: t("assistant.hint"),
            bounds: Rect {
                x: field.x,
                y: field.y + field.height + 8.0,
                width: field.width,
                height: 16.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([140, 142, 160]),
            font_family_override: None,
        });

        if layout.status_lines > 0 {
            let (color, first_color) = match &self.status {
                BubbleStatus::Done { ok: false, .. } => ([235, 150, 150], [170, 172, 190]),
                BubbleStatus::Done { ok: true, .. } => ([214, 236, 214], [170, 172, 190]),
                _ => ([190, 192, 215], [190, 192, 215]),
            };
            // The message lives in `self.status`; wrapping borrows it for the
            // lifetime of the labels.
            let (query_line, message): (Option<&'a str>, &'a str) =
                self.status_parts().unwrap_or((None, ""));
            let mut y = layout.status_top;
            let width = field.width;
            if let Some(query) = query_line {
                labels.push(LabelInfo {
                    text: query,
                    bounds: Rect {
                        x: field.x,
                        y,
                        width,
                        height: STATUS_LINE_HEIGHT,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 0.0,
                    font_size_override: Some(STATUS_FONT),
                    color_override: Some(first_color),
                    font_family_override: None,
                });
                y += STATUS_LINE_HEIGHT;
            }
            for line in super::pricing_page::wrap_text(message, width, STATUS_FONT) {
                labels.push(LabelInfo {
                    text: line,
                    bounds: Rect {
                        x: field.x,
                        y,
                        width,
                        height: STATUS_LINE_HEIGHT,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(STATUS_FONT),
                    color_override: Some(color),
                    font_family_override: None,
                });
                y += STATUS_LINE_HEIGHT;
            }
        }
    }
}
