#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input::{self, TextInputAction, TextInputMetrics, TextInputState};

use crate::i18n::t;

const CARD_W: f32 = 560.0;
const CARD_H: f32 = 430.0;
const ITEM_H: f32 = 28.0;
const LIST_H: f32 = 168.0;
const FIELD_FONT_SIZE: f32 = 12.0;
const FIELD_PADDING_X: f32 = 10.0;

pub struct RenameCharacterModal {
    characters: Vec<String>,
    selected_index: Option<usize>,
    hovered_index: Option<usize>,
    scroll_offset: f32,
    new_name: String,
    name_input: TextInputState,
    selecting_name: bool,
    error_key: Option<&'static str>,
    keyboard_focus: usize,
}

pub enum RenameCharacterModalResult {
    Consumed,
    Close,
    Clipboard(String),
    Rename { old_name: String, new_name: String },
}

impl RenameCharacterModal {
    pub fn new(characters: Vec<String>) -> Self {
        let mut modal = Self {
            characters,
            selected_index: None,
            hovered_index: None,
            scroll_offset: 0.0,
            new_name: String::new(),
            name_input: TextInputState::new(),
            selecting_name: false,
            error_key: None,
            keyboard_focus: 0,
        };
        modal.name_input.activate("");
        modal
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        self.name_input.next_cursor_blink_deadline()
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn list_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 24.0,
            y: card.y + 92.0,
            width: card.width - 48.0,
            height: LIST_H,
        }
    }

    fn name_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 24.0,
            y: card.y + 312.0,
            width: card.width - 48.0,
            height: 34.0,
        }
    }

    fn button_rects(card: Rect) -> (Rect, Rect) {
        let y = card.y + CARD_H - 52.0;
        let cancel = Rect {
            x: card.x + card.width - 232.0,
            y,
            width: 92.0,
            height: 36.0,
        };
        let rename = Rect {
            x: card.x + card.width - 128.0,
            y,
            width: 104.0,
            height: 36.0,
        };
        (cancel, rename)
    }

    fn selected_character(&self) -> Option<&str> {
        self.selected_index
            .and_then(|index| self.characters.get(index))
            .map(String::as_str)
    }

    fn can_confirm(&self) -> bool {
        self.selected_character().is_some_and(|old_name| {
            !self.new_name.trim().is_empty() && old_name != self.new_name.trim()
        })
    }

    fn rename_result(&mut self) -> RenameCharacterModalResult {
        let Some(old_name) = self.selected_character().map(str::to_string) else {
            self.error_key = Some("rename_character_modal.error_select");
            return RenameCharacterModalResult::Consumed;
        };

        let new_name = self.new_name.trim().to_string();
        if new_name.is_empty() {
            self.error_key = Some("rename_character_modal.error_name_required");
            return RenameCharacterModalResult::Consumed;
        }
        if old_name == new_name {
            self.error_key = Some("rename_character_modal.error_same_name");
            return RenameCharacterModalResult::Consumed;
        }

        RenameCharacterModalResult::Rename { old_name, new_name }
    }

    fn index_at_point(&self, list: Rect, x: f32, y: f32) -> Option<usize> {
        if !list.contains(x, y) {
            return None;
        }
        let index = ((y - list.y + self.scroll_offset) / ITEM_H) as usize;
        (index < self.characters.len()).then_some(index)
    }

    fn max_scroll(&self) -> f32 {
        (self.characters.len() as f32 * ITEM_H - LIST_H).max(0.0)
    }

    fn select_character(&mut self, index: usize) {
        if index >= self.characters.len() {
            return;
        }
        self.selected_index = Some(index);
        self.new_name = self.characters[index].clone();
        self.name_input.activate(&self.new_name);
        self.name_input.select_all(&self.new_name);
        self.error_key = None;
    }

    fn cursor_pos_from_x(value: &str, field: Rect, x: f32) -> usize {
        text_input::cursor_pos_from_x(value, field, x, input_metrics())
    }

    fn start_name_selection(&mut self, field: Rect, x: f32, double: bool) {
        self.name_input.activate(&self.new_name);
        if double {
            self.name_input.select_all(&self.new_name);
        } else {
            let pos = Self::cursor_pos_from_x(&self.new_name, field, x);
            self.name_input.start_selection(pos);
            self.selecting_name = true;
        }
    }

    fn update_name_selection(&mut self, field: Rect, x: f32) -> bool {
        if !self.selecting_name {
            return false;
        }
        let pos = Self::cursor_pos_from_x(&self.new_name, field, x);
        self.name_input.update_selection(pos);
        true
    }

    fn sanitize_change(value: String) -> String {
        let mut out: String = value.chars().filter(|c| !c.is_control()).collect();
        if out.len() > 80 {
            out.truncate(80);
        }
        out
    }

    fn handle_name_key(&mut self, text: &str) {
        if let Some(TextInputAction::Changed(name)) =
            self.name_input.handle_key(text, &self.new_name)
        {
            self.new_name = Self::sanitize_change(name);
            self.error_key = None;
        }
    }

    fn copy_selection(&self) -> Option<String> {
        self.name_input.selected_text(&self.new_name)
    }

    fn cut_selection(&mut self) -> Option<String> {
        let clipboard = self.copy_selection()?;
        self.handle_name_key("\x08");
        Some(clipboard)
    }

    fn undo_name(&mut self) {
        if let Some(name) = self.name_input.undo(&self.new_name) {
            self.new_name = Self::sanitize_change(name);
            self.error_key = None;
        }
    }

    fn move_cursor(&mut self, dir: i32, shift: bool) {
        if dir < 0 {
            if shift {
                self.name_input.move_left_shift();
            } else {
                self.name_input.move_left();
            }
        } else if shift {
            self.name_input.move_right_shift(&self.new_name);
        } else {
            self.name_input.move_right(&self.new_name);
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> RenameCharacterModalResult {
        let card = Self::card_rect(screen_w, screen_h);
        let list = Self::list_rect(card);
        let name = Self::name_rect(card);

        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return RenameCharacterModalResult::Close;
                }
            }
            _ => {}
        }

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return RenameCharacterModalResult::Close;
                }
                if text == "\t" {
                    self.keyboard_focus = (self.keyboard_focus + 1) % 4;
                    return RenameCharacterModalResult::Consumed;
                }
                if text == "\u{b}" {
                    self.keyboard_focus = (self.keyboard_focus + 3) % 4;
                    return RenameCharacterModalResult::Consumed;
                }
                if text == "\r" || text == "\n" {
                    return match self.keyboard_focus {
                        0 => {
                            if let Some(index) = self.selected_index {
                                self.select_character(index);
                                self.keyboard_focus = 1;
                            }
                            RenameCharacterModalResult::Consumed
                        }
                        2 => RenameCharacterModalResult::Close,
                        3 => self.rename_result(),
                        _ => RenameCharacterModalResult::Consumed,
                    };
                }
                if self.keyboard_focus == 1 {
                    self.handle_name_key(text);
                }
                RenameCharacterModalResult::Consumed
            }
            UiEvent::CursorUp if self.keyboard_focus == 0 => {
                let index = self.selected_index.unwrap_or(0).saturating_sub(1);
                self.select_character(index);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::CursorDown if self.keyboard_focus == 0 => {
                let index = (self.selected_index.map_or(0, |index| index + 1))
                    .min(self.characters.len().saturating_sub(1));
                self.select_character(index);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::MouseMove { x, y } => {
                if self.update_name_selection(name, *x) {
                    return RenameCharacterModalResult::Consumed;
                }
                self.hovered_index = self.index_at_point(list, *x, *y);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } => {
                if list.contains(*x, *y) {
                    let next_offset =
                        (self.scroll_offset - delta * ITEM_H).clamp(0.0, self.max_scroll());
                    self.scroll_offset =
                        ((next_offset / ITEM_H).round() * ITEM_H).clamp(0.0, self.max_scroll());
                }
                RenameCharacterModalResult::Consumed
            }
            UiEvent::MousePress { x, y } => {
                if let Some(index) = self.index_at_point(list, *x, *y) {
                    self.keyboard_focus = 0;
                    self.select_character(index);
                    return RenameCharacterModalResult::Consumed;
                }
                if name.contains(*x, *y) {
                    self.keyboard_focus = 1;
                    self.start_name_selection(name, *x, false);
                    return RenameCharacterModalResult::Consumed;
                }

                let (cancel, rename) = Self::button_rects(card);
                if cancel.contains(*x, *y) {
                    self.keyboard_focus = 2;
                    return RenameCharacterModalResult::Close;
                }
                if rename.contains(*x, *y) {
                    self.keyboard_focus = 3;
                    return self.rename_result();
                }
                RenameCharacterModalResult::Consumed
            }
            UiEvent::DoubleClick { x, y } => {
                if name.contains(*x, *y) {
                    self.start_name_selection(name, *x, true);
                }
                RenameCharacterModalResult::Consumed
            }
            UiEvent::MouseRelease { .. } => {
                self.selecting_name = false;
                RenameCharacterModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.move_cursor(-1, false);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::CursorRight => {
                self.move_cursor(1, false);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::ShiftCursorLeft => {
                self.move_cursor(-1, true);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::ShiftCursorRight => {
                self.move_cursor(1, true);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::SelectAll => {
                self.name_input.select_all(&self.new_name);
                RenameCharacterModalResult::Consumed
            }
            UiEvent::Copy => self
                .copy_selection()
                .map(RenameCharacterModalResult::Clipboard)
                .unwrap_or(RenameCharacterModalResult::Consumed),
            UiEvent::Cut => self
                .cut_selection()
                .map(RenameCharacterModalResult::Clipboard)
                .unwrap_or(RenameCharacterModalResult::Consumed),
            UiEvent::UndoTextEdit => {
                self.undo_name();
                RenameCharacterModalResult::Consumed
            }
            _ => RenameCharacterModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card_rect(screen_w, screen_h);
        let list = Self::list_rect(card);
        let name = Self::name_rect(card);

        overlay_quads.push(quad(
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.72],
            [0.0, 0.0, 0.0, 0.72],
            0.0,
            0.0,
        ));
        overlay_quads.push(card_quad(card));

        labels.push(LabelInfo {
            text: t("rename_character_modal.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 12.0,
                width: card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(17.0),
            color_override: None,
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: t("rename_character_modal.description"),
            bounds: Rect {
                x: card.x + 24.0,
                y: card.y + 42.0,
                width: card.width - 48.0,
                height: 22.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([170, 170, 185]),
            font_family_override: None,
        });

        labels.push(section_label(
            t("rename_character_modal.character"),
            Rect {
                x: list.x,
                y: list.y - 24.0,
                width: list.width,
                height: 20.0,
            },
        ));
        self.render_character_list(overlay_quads, labels, list);

        labels.push(section_label(
            t("rename_character_modal.new_name"),
            Rect {
                x: name.x,
                y: name.y - 24.0,
                width: name.width,
                height: 20.0,
            },
        ));
        overlay_quads.push(input_quad(name, self.name_input.active));
        render_text_selection_and_cursor(
            overlay_quads,
            name,
            &self.new_name,
            &self.name_input,
            self.name_input.active,
        );
        labels.push(LabelInfo {
            text: &self.new_name,
            bounds: name,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: FIELD_PADDING_X,
            font_size_override: Some(FIELD_FONT_SIZE),
            color_override: Some([226, 226, 235]),
            font_family_override: None,
        });

        if let Some(error_key) = self.error_key {
            labels.push(LabelInfo {
                text: t(error_key),
                bounds: Rect {
                    x: card.x + 24.0,
                    y: card.y + 352.0,
                    width: card.width - 48.0,
                    height: 18.0,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([245, 145, 135]),
                font_family_override: None,
            });
        }

        let (cancel, rename) = Self::button_rects(card);
        render_button(
            overlay_quads,
            labels,
            cancel,
            t("rename_character_modal.cancel"),
            false,
            true,
        );
        render_button(
            overlay_quads,
            labels,
            rename,
            t("rename_character_modal.rename"),
            true,
            self.can_confirm(),
        );
    }

    fn render_character_list<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        list: Rect,
    ) {
        overlay_quads.push(quad(
            list,
            [0.08, 0.08, 0.10, 1.0],
            [0.08, 0.08, 0.10, 1.0],
            1.0,
            6.0,
        ));

        let first_visible = (self.scroll_offset / ITEM_H) as usize;
        let visible_count = (list.height / ITEM_H) as usize + 2;
        for index in first_visible..self.characters.len().min(first_visible + visible_count) {
            let y = list.y + (index as f32 * ITEM_H) - self.scroll_offset;
            if y < list.y || y + ITEM_H > list.y + list.height {
                continue;
            }

            if self.selected_index == Some(index) {
                overlay_quads.push(quad(
                    Rect {
                        x: list.x + 3.0,
                        y,
                        width: list.width - 6.0,
                        height: ITEM_H,
                    },
                    [0.30, 0.28, 0.55, 0.86],
                    [0.30, 0.28, 0.55, 0.86],
                    0.0,
                    4.0,
                ));
            } else if self.hovered_index == Some(index) {
                overlay_quads.push(quad(
                    Rect {
                        x: list.x + 3.0,
                        y,
                        width: list.width - 6.0,
                        height: ITEM_H,
                    },
                    [1.0, 1.0, 1.0, 0.06],
                    [1.0, 1.0, 1.0, 0.06],
                    0.0,
                    4.0,
                ));
            }

            labels.push(LabelInfo {
                text: &self.characters[index],
                bounds: Rect {
                    x: list.x + 10.0,
                    y,
                    width: list.width - 20.0,
                    height: ITEM_H,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: None,
                font_family_override: None,
            });
        }
    }
}

fn section_label(text: &str, bounds: Rect) -> LabelInfo<'_> {
    LabelInfo {
        text,
        bounds,
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: Some([190, 190, 205]),
        font_family_override: None,
    }
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    radius: f32,
) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn card_quad(card: Rect) -> QuadInstance {
    QuadInstance {
        rect: [card.x, card.y, card.width, card.height],
        color: [0.22, 0.22, 0.26, 1.0],
        color_bottom: [0.16, 0.16, 0.19, 1.0],
        border_color: [0.45, 0.45, 0.52, 0.8],
        border_width: 1.5,
        border_radius: 14.0,
        shadow_offset: [0.0, 4.0],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        shadow_blur: 10.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn input_quad(rect: Rect, active: bool) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.10, 0.10, 0.13, 1.0],
        color_bottom: [0.08, 0.08, 0.11, 1.0],
        border_color: if active {
            [0.45, 0.62, 0.95, 0.9]
        } else {
            [0.35, 0.35, 0.42, 0.9]
        },
        border_width: if active { 1.5 } else { 1.0 },
        border_radius: 7.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn render_text_selection_and_cursor(
    overlay_quads: &mut Vec<QuadInstance>,
    rect: Rect,
    value: &str,
    input: &TextInputState,
    active: bool,
) {
    text_input::render_selection_and_cursor(
        overlay_quads,
        rect,
        value,
        input,
        active,
        input_metrics(),
        6.0,
        6.0,
        [0.25, 0.45, 0.95, 0.42],
        [0.90, 0.90, 0.96, 1.0],
    );
}

fn input_metrics() -> TextInputMetrics {
    TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

fn render_button<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    primary: bool,
    enabled: bool,
) {
    let (color, bottom) = if !enabled {
        ([0.18, 0.18, 0.22, 1.0], [0.15, 0.15, 0.18, 1.0])
    } else if primary {
        ([0.34, 0.47, 0.82, 1.0], [0.25, 0.36, 0.70, 1.0])
    } else {
        ([0.30, 0.30, 0.36, 1.0], [0.22, 0.22, 0.27, 1.0])
    };
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: bottom,
        border_color: [0.55, 0.55, 0.65, if enabled { 0.55 } else { 0.25 }],
        border_width: 1.0,
        border_radius: 8.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.25],
        shadow_blur: 4.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    labels.push(LabelInfo {
        text,
        bounds: rect,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: if enabled { None } else { Some([130, 130, 145]) },
        font_family_override: None,
    });
}
