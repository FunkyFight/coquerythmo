//! Optional right-side tables for lines and characters.

use std::collections::HashSet;

use crate::i18n::t;
use crate::project::Project;

use super::color_picker::ColorPickerState;
use super::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use super::text_input::{TextInputAction, TextInputState};

const HEADER_H: f32 = 50.0;
const COLUMNS_H: f32 = 30.0;
const ROW_H: f32 = 42.0;
const PAD: f32 = 14.0;
const ROLE_PICKER_W: f32 = 210.0;
const ROLE_PICKER_HEADER_H: f32 = 30.0;
const ROLE_PICKER_ROW_H: f32 = 30.0;
const ROLE_PICKER_MAX_ROWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidePanelKind {
    Lines,
    Roles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditField {
    LineText(u64),
    RoleName,
}

pub struct SidePanel {
    kind: Option<SidePanelKind>,
    scroll: usize,
    selected: HashSet<u64>,
    selection_anchor: Option<usize>,
    editing: Option<EditField>,
    edit_original: String,
    edit_buffer: String,
    input: TextInputState,
    context_menu: Option<(f32, f32)>,
    context_menu_index: usize,
    role_picker: Option<(f32, f32)>,
    role_picker_scroll: usize,
    role_picker_index: usize,
    keyboard_row: usize,
    keyboard_column: usize,
    keyboard_close: bool,
    color_role: Option<String>,
    color_picker: ColorPickerState,
    dragging_scrollbar: bool,
    scrollbar_drag_offset: f32,
}

impl Default for SidePanel {
    fn default() -> Self {
        Self {
            kind: None,
            scroll: 0,
            selected: HashSet::new(),
            selection_anchor: None,
            editing: None,
            edit_original: String::new(),
            edit_buffer: String::new(),
            input: TextInputState::new(),
            context_menu: None,
            context_menu_index: 0,
            role_picker: None,
            role_picker_scroll: 0,
            role_picker_index: 0,
            keyboard_row: 0,
            keyboard_column: 0,
            keyboard_close: false,
            color_role: None,
            color_picker: ColorPickerState::new(),
            dragging_scrollbar: false,
            scrollbar_drag_offset: 0.0,
        }
    }
}

impl SidePanel {
    pub fn open(&mut self, kind: SidePanelKind) {
        self.kind = Some(kind);
        self.scroll = 0;
        self.selected.clear();
        self.cancel_edit();
        self.context_menu = None;
        self.context_menu_index = 0;
        self.role_picker = None;
        self.role_picker_scroll = 0;
        self.role_picker_index = 0;
        self.keyboard_row = 0;
        self.keyboard_column = 0;
        self.keyboard_close = false;
        self.color_picker.close();
        self.color_role = None;
        self.dragging_scrollbar = false;
    }

    pub fn open_with_selection(
        &mut self,
        kind: SidePanelKind,
        selected_line_ids: impl IntoIterator<Item = u64>,
    ) {
        self.open(kind);
        self.selected.extend(selected_line_ids);
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }
    pub fn is_open(&self) -> bool {
        self.kind.is_some()
    }

    pub fn is_editing_text(&self) -> bool {
        self.editing.is_some() && self.input.active
    }

    pub fn captures_keyboard_event(&self, event: &UiEvent) -> bool {
        self.kind.is_some()
            && matches!(
                event,
                UiEvent::KeyInput { .. }
                    | UiEvent::FocusNext
                    | UiEvent::FocusPrevious
                    | UiEvent::Activate
                    | UiEvent::CursorLeft
                    | UiEvent::CursorRight
                    | UiEvent::ShiftCursorLeft
                    | UiEvent::ShiftCursorRight
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::SelectWordLeft
                    | UiEvent::SelectWordRight
                    | UiEvent::Home
                    | UiEvent::End
                    | UiEvent::PageUp
                    | UiEvent::PageDown
                    | UiEvent::OpenContextMenu
                    | UiEvent::Delete
                    | UiEvent::SelectAll
                    | UiEvent::Copy
                    | UiEvent::Cut
                    | UiEvent::UndoTextEdit
            )
    }

    pub fn first_accessibility_label(&self, project: &Project) -> String {
        match self.kind {
            Some(SidePanelKind::Lines) => self
                .line_cell_label(project, 0, 0)
                .unwrap_or_else(|| t("panel.empty.lines").to_string()),
            Some(SidePanelKind::Roles) => roles(project)
                .first()
                .map(|(name, _)| format!("{} : {name}", t("panel.name")))
                .unwrap_or_else(|| t("panel.empty.roles").to_string()),
            None => String::new(),
        }
    }

    pub fn accessibility_title(&self) -> Option<&'static str> {
        match self.kind {
            Some(SidePanelKind::Lines) => Some(t("panel.lines.title")),
            Some(SidePanelKind::Roles) => Some(t("panel.roles.title")),
            None => None,
        }
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        self.input.next_cursor_blink_deadline()
    }

    pub fn ensure_color_picker_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.color_picker
            .ensure_textures(device, queue, layout, sampler);
    }

    pub fn render_color_picker<'a>(
        &'a self,
        bg: &mut Vec<QuadInstance>,
        textures: &mut Vec<(super::primitives::IconInstance, &'a wgpu::BindGroup)>,
        fg: &mut Vec<QuadInstance>,
    ) {
        self.color_picker.render(bg, textures, fg);
    }

    fn line_cell_label(&self, project: &Project, index: usize, column: usize) -> Option<String> {
        let total = project.line_count();
        let line = project.line_at(index)?;
        let character = if line.character_name.trim().is_empty() {
            t("accessibility.character")
        } else {
            line.character_name.trim()
        };
        let dialogue = if line.text.trim().is_empty() {
            t("accessibility.line")
        } else {
            line.text.trim()
        };
        let track = crate::rythmo_layout::track_index_for_y_slot(line.y_slot) + 1;
        let cell = if column == 0 {
            t("panel.role")
        } else {
            t("panel.text")
        };
        let mut label = format!(
            "{} {} {} {total}, {} : {character}, {} : {dialogue}, {} {track}",
            t("accessibility.line"),
            index + 1,
            t("accessibility.of"),
            t("accessibility.character"),
            t("accessibility.dialogue"),
            t("accessibility.track")
        );
        if line.karaoke {
            label.push_str(", ");
            label.push_str(t("accessibility.karaoke_line"));
        }
        label.push_str(&format!(", {} : {cell}", t("accessibility.column")));
        Some(label)
    }

    fn current_focus_event(&self, project: &Project) -> crate::accessibility::AccessibilityEvent {
        if self.keyboard_close {
            return crate::accessibility::AccessibilityEvent::Focus {
                label: t("panel.close").to_string(),
                role: "button".to_string(),
            };
        }
        match self.kind {
            Some(SidePanelKind::Lines) => crate::accessibility::AccessibilityEvent::Focus {
                label: self
                    .line_cell_label(project, self.keyboard_row, self.keyboard_column)
                    .unwrap_or_else(|| t("panel.empty.lines").to_string()),
                role: "table cell".to_string(),
            },
            Some(SidePanelKind::Roles) => {
                let label = roles(project)
                    .get(self.keyboard_row)
                    .map(|(name, _)| format!("{} : {name}", t("panel.name")))
                    .unwrap_or_else(|| t("panel.empty.roles").to_string());
                crate::accessibility::AccessibilityEvent::Focus {
                    label,
                    role: "table cell".to_string(),
                }
            }
            None => crate::accessibility::AccessibilityEvent::Focus {
                label: String::new(),
                role: "table".to_string(),
            },
        }
    }

    fn focus_response(&self, project: &Project) -> EventResponse {
        EventResponse::Action(UiAction::Accessibility(self.current_focus_event(project)))
    }

    fn ensure_keyboard_row_visible(&mut self, panel: Rect, item_count: usize) {
        if item_count == 0 {
            self.keyboard_row = 0;
            self.scroll = 0;
            return;
        }
        self.keyboard_row = self.keyboard_row.min(item_count - 1);
        let visible = visible_rows(panel);
        if self.keyboard_row < self.scroll {
            self.scroll = self.keyboard_row;
        } else if self.keyboard_row >= self.scroll + visible {
            self.scroll = self.keyboard_row + 1 - visible;
        }
    }

    fn move_table_focus(&mut self, forward: bool, panel: Rect, project: &Project) -> EventResponse {
        let (item_count, columns) = match self.kind {
            Some(SidePanelKind::Lines) => (project.line_count(), 2),
            Some(SidePanelKind::Roles) => (roles(project).len(), 1),
            None => return EventResponse::Ignored,
        };
        let cell_count = item_count * columns;
        let current = if self.keyboard_close || cell_count == 0 {
            cell_count
        } else {
            self.keyboard_row * columns + self.keyboard_column.min(columns - 1)
        };
        let total = cell_count + 1;
        let next = if forward {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        self.keyboard_close = next == cell_count;
        if !self.keyboard_close {
            self.keyboard_row = next / columns;
            self.keyboard_column = next % columns;
            self.ensure_keyboard_row_visible(panel, item_count);
        }
        self.focus_response(project)
    }

    fn selected_text_event(&self) -> Option<EventResponse> {
        self.input
            .selected_text(&self.edit_buffer)
            .filter(|selection| !selection.is_empty())
            .map(|label| {
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection { label },
                ))
            })
    }

    fn handle_editing_keyboard(
        &mut self,
        event: &UiEvent,
        project: &Project,
    ) -> Option<EventResponse> {
        if self.editing.is_none() {
            return None;
        }
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.cancel_edit();
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Closed {
                        label: t("panel.editing").to_string(),
                    },
                )))
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                Some(self.finish_edit(project))
            }
            UiEvent::KeyInput { text } => {
                if let Some(action) = self.input.handle_key(text, &self.edit_buffer) {
                    if let TextInputAction::Changed(value) = action {
                        self.edit_buffer = value;
                    }
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorLeft => {
                self.input.move_left();
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorRight => {
                self.input.move_right(&self.edit_buffer);
                Some(EventResponse::Consumed)
            }
            UiEvent::MoveWordLeft | UiEvent::MoveWordRight => {
                if matches!(event, UiEvent::MoveWordLeft) {
                    self.input.move_word_left(&self.edit_buffer);
                } else {
                    self.input.move_word_right(&self.edit_buffer);
                }
                Some(
                    self.input
                        .word_at_cursor(&self.edit_buffer)
                        .map(|word| {
                            EventResponse::Action(UiAction::Accessibility(
                                crate::accessibility::AccessibilityEvent::Selection { label: word },
                            ))
                        })
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::ShiftCursorLeft => {
                self.input.move_left_shift();
                Some(
                    self.selected_text_event()
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::ShiftCursorRight => {
                self.input.move_right_shift(&self.edit_buffer);
                Some(
                    self.selected_text_event()
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::SelectWordLeft => {
                self.input.move_word_left_shift(&self.edit_buffer);
                Some(
                    self.selected_text_event()
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::SelectWordRight => {
                self.input.move_word_right_shift(&self.edit_buffer);
                Some(
                    self.selected_text_event()
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::SelectAll => {
                self.input.select_range(0, self.edit_buffer.chars().count());
                let label = self
                    .input
                    .selected_text(&self.edit_buffer)
                    .filter(|selection| !selection.is_empty())
                    .map(|selection| format!("{} : {selection}", t("accessibility.select_all")));
                Some(
                    label
                        .map(|label| {
                            EventResponse::Action(UiAction::Accessibility(
                                crate::accessibility::AccessibilityEvent::Selection { label },
                            ))
                        })
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::Home => {
                self.input.move_home(false);
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: t("accessibility.caret_dialogue_start").to_string(),
                    },
                )))
            }
            UiEvent::End => {
                self.input.move_end(&self.edit_buffer, false);
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: t("accessibility.caret_dialogue_end").to_string(),
                    },
                )))
            }
            UiEvent::Copy => self
                .input
                .selected_text(&self.edit_buffer)
                .map(|text| EventResponse::Action(UiAction::SetClipboard(text)))
                .or(Some(EventResponse::Consumed)),
            UiEvent::Cut => {
                let clipboard = self.input.selected_text(&self.edit_buffer);
                if clipboard.is_some() {
                    if let Some(TextInputAction::Changed(value)) =
                        self.input.handle_key("\x08", &self.edit_buffer)
                    {
                        self.edit_buffer = value;
                    }
                }
                Some(
                    clipboard
                        .map(|text| EventResponse::Action(UiAction::SetClipboard(text)))
                        .unwrap_or(EventResponse::Consumed),
                )
            }
            UiEvent::UndoTextEdit => {
                if let Some(value) = self.input.undo(&self.edit_buffer) {
                    self.edit_buffer = value;
                }
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_role_picker_keyboard(
        &mut self,
        event: &UiEvent,
        project: &Project,
    ) -> Option<EventResponse> {
        self.role_picker?;
        let all_roles = roles(project);
        if !all_roles.is_empty() {
            self.role_picker_index = self.role_picker_index.min(all_roles.len() - 1);
        }
        if keyboard_activation(event) {
            let Some((name, color)) = all_roles.get(self.role_picker_index) else {
                return Some(EventResponse::Consumed);
            };
            self.role_picker = None;
            self.role_picker_scroll = 0;
            self.role_picker_index = 0;
            return Some(EventResponse::Actions(vec![
                UiAction::SetLinesRole {
                    line_ids: self.selected.iter().copied().collect(),
                    name: (*name).to_string(),
                    color: *color,
                },
                UiAction::Accessibility(crate::accessibility::AccessibilityEvent::Activation {
                    label: (*name).to_string(),
                }),
            ]));
        }
        let selection = |index: usize| {
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection {
                    label: all_roles
                        .get(index)
                        .map(|(name, _)| (*name).to_string())
                        .unwrap_or_else(|| t("panel.empty.roles").to_string()),
                },
            ))
        };
        match event {
            UiEvent::CursorUp | UiEvent::FocusPrevious => {
                self.role_picker_index = self.role_picker_index.saturating_sub(1);
                self.role_picker_scroll = self.role_picker_scroll.min(self.role_picker_index);
                Some(selection(self.role_picker_index))
            }
            UiEvent::CursorDown | UiEvent::FocusNext => {
                if !all_roles.is_empty() {
                    self.role_picker_index = (self.role_picker_index + 1).min(all_roles.len() - 1);
                    if self.role_picker_index >= self.role_picker_scroll + ROLE_PICKER_MAX_ROWS {
                        self.role_picker_scroll = self.role_picker_index + 1 - ROLE_PICKER_MAX_ROWS;
                    }
                }
                Some(selection(self.role_picker_index))
            }
            UiEvent::Home => {
                self.role_picker_index = 0;
                self.role_picker_scroll = 0;
                Some(selection(0))
            }
            UiEvent::End => {
                self.role_picker_index = all_roles.len().saturating_sub(1);
                self.role_picker_scroll = all_roles.len().saturating_sub(ROLE_PICKER_MAX_ROWS);
                Some(selection(self.role_picker_index))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.role_picker = None;
                self.role_picker_scroll = 0;
                self.role_picker_index = 0;
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Collapsed {
                        label: t("panel.choose_role").to_string(),
                    },
                )))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_context_menu_keyboard(
        &mut self,
        event: &UiEvent,
        _panel: Rect,
        project: &Project,
    ) -> Option<EventResponse> {
        let (mx, my) = self.context_menu?;
        let item_count = if self.selected.len() == 1 { 2 } else { 1 };
        self.context_menu_index = self.context_menu_index.min(item_count - 1);
        let label = |index: usize| {
            if item_count == 2 && index == 0 {
                t("panel.go_to").to_string()
            } else {
                t("panel.change_role").to_string()
            }
        };
        if keyboard_activation(event) {
            if item_count == 2 && self.context_menu_index == 0 {
                self.context_menu = None;
                let action = self
                    .selected
                    .iter()
                    .next()
                    .and_then(|id| project.get_line(*id))
                    .map(|line| UiAction::SeekAbsolute(line.start_frame));
                return Some(
                    action
                        .map(EventResponse::Action)
                        .unwrap_or(EventResponse::Consumed),
                );
            }
            self.context_menu = None;
            self.role_picker = Some((mx, my));
            self.role_picker_index = 0;
            self.role_picker_scroll = 0;
            let first = roles(project)
                .first()
                .map(|(name, _)| *name)
                .unwrap_or_else(|| t("panel.empty.roles"));
            return Some(EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Activation {
                    label: format!("{} : {first}", t("panel.choose_role")),
                },
            )));
        }
        match event {
            UiEvent::CursorUp | UiEvent::FocusPrevious => {
                self.context_menu_index = self.context_menu_index.saturating_sub(1);
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: label(self.context_menu_index),
                    },
                )))
            }
            UiEvent::CursorDown | UiEvent::FocusNext => {
                self.context_menu_index = (self.context_menu_index + 1).min(item_count - 1);
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: label(self.context_menu_index),
                    },
                )))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.context_menu = None;
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Collapsed {
                        label: t("panel.actions").to_string(),
                    },
                )))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_keyboard_event(
        &mut self,
        event: &UiEvent,
        panel: Rect,
        project: &Project,
    ) -> EventResponse {
        if let Some(response) = self.handle_role_picker_keyboard(event, project) {
            return response;
        }
        if let Some(response) = self.handle_context_menu_keyboard(event, panel, project) {
            return response;
        }
        if let Some(response) = self.handle_editing_keyboard(event, project) {
            return response;
        }

        let Some(kind) = self.kind else {
            return EventResponse::Ignored;
        };
        let item_count = match kind {
            SidePanelKind::Lines => project.line_count(),
            SidePanelKind::Roles => roles(project).len(),
        };
        match event {
            UiEvent::FocusNext => self.move_table_focus(true, panel, project),
            UiEvent::FocusPrevious => self.move_table_focus(false, panel, project),
            UiEvent::CursorUp | UiEvent::CursorDown | UiEvent::PageUp | UiEvent::PageDown => {
                if self.keyboard_close {
                    self.keyboard_close = false;
                    self.keyboard_row = if matches!(event, UiEvent::CursorUp | UiEvent::PageUp) {
                        item_count.saturating_sub(1)
                    } else {
                        0
                    };
                } else if item_count > 0 {
                    let step = if matches!(event, UiEvent::PageUp | UiEvent::PageDown) {
                        visible_rows(panel)
                    } else {
                        1
                    };
                    if matches!(event, UiEvent::CursorUp | UiEvent::PageUp) {
                        self.keyboard_row = self.keyboard_row.saturating_sub(step);
                    } else {
                        self.keyboard_row = (self.keyboard_row + step).min(item_count - 1);
                    }
                }
                self.ensure_keyboard_row_visible(panel, item_count);
                self.focus_response(project)
            }
            UiEvent::CursorLeft | UiEvent::CursorRight => {
                if kind == SidePanelKind::Lines && !self.keyboard_close {
                    self.keyboard_column = if matches!(event, UiEvent::CursorLeft) {
                        0
                    } else {
                        1
                    };
                }
                self.focus_response(project)
            }
            UiEvent::Home | UiEvent::End => {
                self.keyboard_close = false;
                self.keyboard_row = if matches!(event, UiEvent::Home) {
                    0
                } else {
                    item_count.saturating_sub(1)
                };
                self.ensure_keyboard_row_visible(panel, item_count);
                self.focus_response(project)
            }
            UiEvent::Activate => self.activate_keyboard_focus(panel, project),
            UiEvent::KeyInput { text } if text == " " => {
                if kind == SidePanelKind::Lines && !self.keyboard_close {
                    let Some(line) = project.line_at(self.keyboard_row) else {
                        return EventResponse::Consumed;
                    };
                    if !self.selected.remove(&line.id) {
                        self.selected.insert(line.id);
                    }
                    self.selection_anchor = Some(self.keyboard_row);
                    return EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Selection {
                            label: self
                                .line_cell_label(project, self.keyboard_row, self.keyboard_column)
                                .unwrap_or_else(|| t("panel.empty.lines").to_string()),
                        },
                    ));
                }
                self.activate_keyboard_focus(panel, project)
            }
            UiEvent::OpenContextMenu if kind == SidePanelKind::Lines && !self.keyboard_close => {
                let Some(line) = project.line_at(self.keyboard_row) else {
                    return EventResponse::Consumed;
                };
                if !self.selected.contains(&line.id) {
                    self.selected.clear();
                    self.selected.insert(line.id);
                }
                self.context_menu = Some((panel.x + 12.0, panel.y + HEADER_H + COLUMNS_H + 8.0));
                self.context_menu_index = 0;
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Activation {
                        label: format!("{} : {}", t("panel.actions"), t("panel.go_to")),
                    },
                ))
            }
            UiEvent::SelectAll if kind == SidePanelKind::Lines => {
                self.selected = project.lines().map(|line| line.id).collect();
                self.selection_anchor = (!self.selected.is_empty()).then_some(0);
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: format!(
                            "{} : {}",
                            t("accessibility.select_all"),
                            self.selected.len()
                        ),
                    },
                ))
            }
            UiEvent::Delete if kind == SidePanelKind::Lines => {
                let mut line_ids: Vec<u64> = self.selected.iter().copied().collect();
                if line_ids.is_empty() && !self.keyboard_close {
                    if let Some(line) = project.line_at(self.keyboard_row) {
                        line_ids.push(line.id);
                    }
                }
                if line_ids.is_empty() {
                    EventResponse::Consumed
                } else {
                    self.selected.clear();
                    EventResponse::Action(UiAction::DeleteSidePanelLines { line_ids })
                }
            }
            UiEvent::Copy | UiEvent::Cut if kind == SidePanelKind::Lines => {
                let mut line_ids: Vec<u64> = self.selected.iter().copied().collect();
                if line_ids.is_empty() && !self.keyboard_close {
                    if let Some(line) = project.line_at(self.keyboard_row) {
                        line_ids.push(line.id);
                    }
                }
                if line_ids.is_empty() {
                    EventResponse::Consumed
                } else {
                    if matches!(event, UiEvent::Cut) {
                        self.selected.clear();
                    }
                    EventResponse::Action(UiAction::CopySidePanelLines {
                        line_ids,
                        cut: matches!(event, UiEvent::Cut),
                    })
                }
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                EventResponse::Action(UiAction::CloseSidePanel)
            }
            _ => EventResponse::Consumed,
        }
    }

    fn activate_keyboard_focus(&mut self, panel: Rect, project: &Project) -> EventResponse {
        if self.keyboard_close {
            if self.kind.is_none() {
                return EventResponse::Ignored;
            }
            return EventResponse::Action(UiAction::CloseSidePanel);
        }
        match self.kind {
            Some(SidePanelKind::Lines) => {
                let Some(line) = project.line_at(self.keyboard_row) else {
                    return EventResponse::Consumed;
                };
                if !self.selected.contains(&line.id) {
                    self.selected.clear();
                    self.selected.insert(line.id);
                    self.selection_anchor = Some(self.keyboard_row);
                }
                if self.keyboard_column == 0 {
                    self.cancel_edit();
                    let picker_h = ROLE_PICKER_HEADER_H
                        + roles(project).len().min(ROLE_PICKER_MAX_ROWS) as f32 * ROLE_PICKER_ROW_H;
                    self.role_picker = Some((
                        panel.x + 8.0,
                        (panel.y + HEADER_H + COLUMNS_H + ROW_H)
                            .min(panel.y + panel.height - picker_h - 4.0),
                    ));
                    self.role_picker_scroll = 0;
                    self.role_picker_index = 0;
                    let first = roles(project)
                        .first()
                        .map(|(name, _)| *name)
                        .unwrap_or_else(|| t("panel.empty.roles"));
                    EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Activation {
                            label: format!("{} : {first}", t("panel.choose_role")),
                        },
                    ))
                } else {
                    let value = line.text.clone();
                    self.start_edit(EditField::LineText(line.id), &value);
                    EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Focus {
                            label: format!("{} : {value}", t("panel.text")),
                            role: "text field".to_string(),
                        },
                    ))
                }
            }
            Some(SidePanelKind::Roles) => {
                let all_roles = roles(project);
                let Some((name, _)) = all_roles.get(self.keyboard_row) else {
                    return EventResponse::Consumed;
                };
                let value = (*name).to_string();
                self.start_edit(EditField::RoleName, &value);
                EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Focus {
                        label: format!("{} : {value}", t("panel.name")),
                        role: "text field".to_string(),
                    },
                ))
            }
            None => EventResponse::Ignored,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        panel: Rect,
        project: &Project,
    ) -> Option<EventResponse> {
        let kind = self.kind?;

        if self.captures_keyboard_event(event) {
            return Some(self.handle_keyboard_event(event, panel, project));
        }

        if self.color_picker.active {
            let before = self.color_picker.current_color();
            if self.color_picker.handle_event(event) {
                let after = self.color_picker.current_color();
                if before != after {
                    if let Some(role) = &self.color_role {
                        return Some(EventResponse::Action(UiAction::SetRoleColor {
                            role: role.clone(),
                            color: after,
                        }));
                    }
                }
                if !self.color_picker.active {
                    self.color_role = None;
                }
                return Some(EventResponse::Consumed);
            }
        }

        if let UiEvent::KeyInput { text } = event {
            if self.editing.is_some() {
                if let Some(action) = self.input.handle_key(text, &self.edit_buffer) {
                    match action {
                        TextInputAction::Changed(value) => self.edit_buffer = value,
                        TextInputAction::Finished => return Some(self.finish_edit(project)),
                    }
                    return Some(EventResponse::Consumed);
                }
            }
        }
        if self.editing.is_some() {
            match event {
                UiEvent::CursorLeft => self.input.move_left(),
                UiEvent::CursorRight => self.input.move_right(&self.edit_buffer),
                UiEvent::ShiftCursorLeft => self.input.move_left_shift(),
                UiEvent::ShiftCursorRight => self.input.move_right_shift(&self.edit_buffer),
                UiEvent::SelectAll => self.input.select_range(0, self.edit_buffer.chars().count()),
                _ => {}
            }
        }

        // Floating controls are hit-tested before the panel beneath them. This is
        // deliberately above both scrollbar and table handling: an overlay owns
        // every pointer/wheel event that lands inside its bounds.
        if let Some((px, py)) = self.role_picker {
            let all_roles = roles(project);
            let (picker, visible) = role_picker_geometry(px, py, all_roles.len());
            let max_scroll = all_roles.len().saturating_sub(visible);
            self.role_picker_scroll = self.role_picker_scroll.min(max_scroll);

            if let UiEvent::Scroll { x, y, delta, .. } = event {
                if picker.contains(*x, *y) {
                    if *delta > 0.0 {
                        self.role_picker_scroll = self.role_picker_scroll.saturating_sub(1);
                    } else if *delta < 0.0 {
                        self.role_picker_scroll = (self.role_picker_scroll + 1).min(max_scroll);
                    }
                    return Some(EventResponse::Consumed);
                }
            }

            if let Some((x, y)) = pointer_down_position(event) {
                if picker.contains(x, y) {
                    if y >= py + ROLE_PICKER_HEADER_H {
                        let visible_index =
                            ((y - py - ROLE_PICKER_HEADER_H) / ROLE_PICKER_ROW_H) as usize;
                        let index = self.role_picker_scroll + visible_index;
                        if let Some((name, color)) = all_roles.get(index) {
                            self.role_picker = None;
                            self.role_picker_scroll = 0;
                            return Some(EventResponse::Action(UiAction::SetLinesRole {
                                line_ids: self.selected.iter().copied().collect(),
                                name: (*name).to_string(),
                                color: *color,
                            }));
                        }
                    }
                    return Some(EventResponse::Consumed);
                }

                self.role_picker = None;
                self.role_picker_scroll = 0;
                return Some(EventResponse::Consumed);
            }
        }

        if let Some((mx, my)) = self.context_menu {
            let single_line = self.selected.len() == 1;
            let menu = Rect {
                x: mx,
                y: my,
                width: 190.0,
                height: if single_line { 64.0 } else { 32.0 },
            };
            if let UiEvent::Scroll { x, y, .. } = event {
                if menu.contains(*x, *y) {
                    return Some(EventResponse::Consumed);
                }
            }
            if let Some((x, y)) = pointer_down_position(event) {
                if menu.contains(x, y) {
                    let item = ((y - my) / 32.0) as usize;
                    self.context_menu = None;
                    if single_line && item == 0 {
                        if let Some(line) = self
                            .selected
                            .iter()
                            .next()
                            .and_then(|id| project.get_line(*id))
                        {
                            return Some(EventResponse::Action(UiAction::SeekAbsolute(
                                line.start_frame,
                            )));
                        }
                        return Some(EventResponse::Consumed);
                    }
                    self.role_picker = Some((mx, my));
                    self.role_picker_scroll = 0;
                    return Some(EventResponse::Consumed);
                }
                self.context_menu = None;
                return Some(EventResponse::Consumed);
            }
        }

        let item_count = match kind {
            SidePanelKind::Lines => project.line_count(),
            SidePanelKind::Roles => roles(project).len(),
        };
        if let Some((track, thumb, max_scroll)) = scrollbar_geometry(panel, item_count, self.scroll)
        {
            match event {
                UiEvent::MousePress { x, y } if thumb.contains(*x, *y) => {
                    self.dragging_scrollbar = true;
                    self.scrollbar_drag_offset = *y - thumb.y;
                    return Some(EventResponse::Consumed);
                }
                UiEvent::MousePress { x, y } if track.contains(*x, *y) => {
                    let travel = (track.height - thumb.height).max(1.0);
                    let ratio = ((*y - track.y - thumb.height / 2.0) / travel).clamp(0.0, 1.0);
                    self.scroll = (ratio * max_scroll as f32).round() as usize;
                    self.dragging_scrollbar = true;
                    self.scrollbar_drag_offset = thumb.height / 2.0;
                    return Some(EventResponse::Consumed);
                }
                UiEvent::MouseMove { y, .. } if self.dragging_scrollbar => {
                    let travel = (track.height - thumb.height).max(1.0);
                    let ratio =
                        ((*y - self.scrollbar_drag_offset - track.y) / travel).clamp(0.0, 1.0);
                    self.scroll = (ratio * max_scroll as f32).round() as usize;
                    return Some(EventResponse::Consumed);
                }
                UiEvent::MouseRelease { .. } if self.dragging_scrollbar => {
                    self.dragging_scrollbar = false;
                    return Some(EventResponse::Consumed);
                }
                _ => {}
            }
        } else {
            self.dragging_scrollbar = false;
            self.scroll = 0;
        }

        if let UiEvent::Scroll { x, y, delta, .. } = event {
            if panel.contains(*x, *y) {
                let visible = visible_rows(panel);
                let max = item_count.saturating_sub(visible);
                if *delta > 0.0 {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.scroll = (self.scroll + 1).min(max);
                }
                return Some(EventResponse::Consumed);
            }
        }

        let (x, y, modifier) = match event {
            UiEvent::MousePress { x, y } => (*x, *y, 0),
            UiEvent::CtrlClick { x, y } => (*x, *y, 1),
            UiEvent::ShiftMousePress { x, y } => (*x, *y, 2),
            UiEvent::DoubleClick { x, y } => (*x, *y, 3),
            UiEvent::ContextMenu { x, y } => (*x, *y, 4),
            _ => {
                return if panel.contains(event_xy(event).0, event_xy(event).1) {
                    Some(EventResponse::Consumed)
                } else {
                    None
                }
            }
        };

        if close_rect(panel).contains(x, y) {
            return Some(EventResponse::Action(UiAction::CloseSidePanel));
        }

        if !panel.contains(x, y) {
            return None;
        }
        let Some(row) = row_at(panel, y) else {
            return Some(EventResponse::Consumed);
        };
        let index = self.scroll + row;
        match kind {
            SidePanelKind::Lines => {
                let Some(line) = project.line_at(index) else {
                    return Some(EventResponse::Consumed);
                };
                let id = line.id;
                let role_w = (panel.width * 0.38).max(105.0);
                self.keyboard_close = false;
                self.keyboard_row = index;
                self.keyboard_column = if x < panel.x + role_w { 0 } else { 1 };
                if modifier == 4 {
                    if !self.selected.contains(&id) {
                        self.selected.clear();
                        self.selected.insert(id);
                    }
                    let menu_h = if self.selected.len() == 1 { 64.0 } else { 32.0 };
                    self.context_menu = Some((
                        x.min(panel.x + panel.width - 195.0),
                        y.min(panel.y + panel.height - menu_h - 4.0),
                    ));
                    self.context_menu_index = 0;
                    return Some(EventResponse::Action(UiAction::Accessibility(
                        crate::accessibility::AccessibilityEvent::Activation {
                            label: format!("{} : {}", t("panel.actions"), t("panel.go_to")),
                        },
                    )));
                } else if modifier == 1 {
                    if !self.selected.remove(&id) {
                        self.selected.insert(id);
                    }
                    self.selection_anchor = Some(index);
                } else if modifier == 2 {
                    let anchor = self.selection_anchor.unwrap_or(index);
                    self.selected.clear();
                    for line in project
                        .lines()
                        .skip(anchor.min(index))
                        .take(anchor.abs_diff(index) + 1)
                    {
                        self.selected.insert(line.id);
                    }
                } else if modifier == 3 {
                    if x < panel.x + role_w {
                        self.cancel_edit();
                        self.selected.clear();
                        self.selected.insert(id);
                        let picker_h = 30.0 + roles(project).len().min(8) as f32 * 30.0;
                        self.role_picker = Some((
                            panel.x + 8.0,
                            (row_rect(panel, row).y + ROW_H)
                                .min(panel.y + panel.height - picker_h - 4.0),
                        ));
                        self.role_picker_scroll = 0;
                        self.role_picker_index = 0;
                        let first = roles(project)
                            .first()
                            .map(|(name, _)| *name)
                            .unwrap_or_else(|| t("panel.empty.roles"));
                        return Some(EventResponse::Action(UiAction::Accessibility(
                            crate::accessibility::AccessibilityEvent::Activation {
                                label: format!("{} : {first}", t("panel.choose_role")),
                            },
                        )));
                    } else {
                        let value = line.text.clone();
                        self.start_edit(EditField::LineText(id), &value);
                        return Some(EventResponse::Action(UiAction::Accessibility(
                            crate::accessibility::AccessibilityEvent::Focus {
                                label: format!("{} : {value}", t("panel.text")),
                                role: "text field".to_string(),
                            },
                        )));
                    }
                } else {
                    self.selected.clear();
                    self.selected.insert(id);
                    self.selection_anchor = Some(index);
                }
                return Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Selection {
                        label: self
                            .line_cell_label(project, index, self.keyboard_column)
                            .unwrap_or_else(|| t("panel.empty.lines").to_string()),
                    },
                )));
            }
            SidePanelKind::Roles => {
                let Some((name, color)) = roles(project).get(index).copied() else {
                    return Some(EventResponse::Consumed);
                };
                let swatch = Rect {
                    x: panel.x + PAD,
                    y: row_rect(panel, row).y + 8.0,
                    width: 22.0,
                    height: 22.0,
                };
                if swatch.contains(x, y) {
                    let (_pw, ph) = ColorPickerState::panel_size();
                    let ox = panel.x + panel.width + 8.0;
                    let oy = y.min(panel.y + panel.height - ph - 4.0).max(4.0);
                    self.color_picker.open(ox, oy, color);
                    self.color_role = Some(name.to_string());
                } else if modifier == 3 {
                    self.start_edit(EditField::RoleName, name);
                }
            }
        }
        Some(EventResponse::Consumed)
    }

    fn start_edit(&mut self, field: EditField, value: &str) {
        self.editing = Some(field);
        self.edit_original = value.to_string();
        self.edit_buffer = value.to_string();
        self.input.activate(value);
    }
    fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_buffer.clear();
        self.edit_original.clear();
        self.input.deactivate();
    }
    fn finish_edit(&mut self, _project: &Project) -> EventResponse {
        let Some(field) = self.editing.take() else {
            return EventResponse::Consumed;
        };
        let value = self.edit_buffer.trim().to_string();
        self.input.deactivate();
        if value == self.edit_original {
            return EventResponse::Consumed;
        }
        match field {
            EditField::LineText(id) => EventResponse::Action(UiAction::UpdateLineText {
                id,
                text: self.edit_buffer.clone(),
            }),
            EditField::RoleName => EventResponse::Action(UiAction::RenameCharacter {
                old_name: self.edit_original.clone(),
                new_name: value,
            }),
        }
    }

    pub fn render<'a>(
        &'a self,
        panel: Rect,
        project: &'a Project,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let Some(kind) = self.kind else {
            return;
        };
        let item_count = match kind {
            SidePanelKind::Lines => project.line_count(),
            SidePanelKind::Roles => roles(project).len(),
        };
        solid(
            quads,
            panel,
            [0.065, 0.067, 0.082, 1.0],
            [0.24, 0.25, 0.31, 1.0],
            0.0,
        );
        solid(
            quads,
            Rect {
                x: panel.x,
                y: panel.y,
                width: panel.width,
                height: HEADER_H,
            },
            [0.09, 0.093, 0.115, 1.0],
            [0.0; 4],
            0.0,
        );
        labels.push(label(
            match kind {
                SidePanelKind::Lines => t("panel.lines.title"),
                SidePanelKind::Roles => t("panel.roles.title"),
            },
            Rect {
                x: panel.x + PAD + 4.0,
                y: panel.y,
                width: panel.width - 52.0,
                height: HEADER_H,
            },
            HAlign::Left,
            15.0,
            [238, 239, 245],
        ));
        solid(
            quads,
            Rect {
                x: panel.x,
                y: panel.y + HEADER_H - 1.0,
                width: panel.width,
                height: 1.0,
            },
            [0.22, 0.23, 0.28, 0.8],
            [0.0; 4],
            0.0,
        );
        solid(
            quads,
            Rect {
                x: panel.x + 10.0,
                y: panel.y + 15.0,
                width: 3.0,
                height: 20.0,
            },
            [0.42, 0.48, 0.92, 1.0],
            [0.0; 4],
            1.5,
        );
        let close = close_rect(panel);
        solid(
            quads,
            close,
            [0.13, 0.135, 0.16, 1.0],
            if self.keyboard_close {
                [0.38, 0.58, 0.96, 1.0]
            } else {
                [0.0; 4]
            },
            6.0,
        );
        labels.push(label("×", close, HAlign::Center, 19.0, [205, 208, 220]));

        let cols = columns_rect(panel);
        solid(quads, cols, [0.075, 0.078, 0.095, 1.0], [0.0; 4], 0.0);
        match kind {
            SidePanelKind::Lines => {
                let role_w = (panel.width * 0.38).max(105.0);
                labels.push(label(
                    t("panel.role"),
                    Rect {
                        x: panel.x + PAD,
                        y: cols.y,
                        width: role_w - PAD,
                        height: cols.height,
                    },
                    HAlign::Left,
                    11.0,
                    [150, 154, 170],
                ));
                labels.push(label(
                    t("panel.text"),
                    Rect {
                        x: panel.x + role_w + 6.0,
                        y: cols.y,
                        width: panel.width - role_w - 16.0,
                        height: cols.height,
                    },
                    HAlign::Left,
                    11.0,
                    [150, 154, 170],
                ));
                solid(
                    quads,
                    Rect {
                        x: panel.x + role_w,
                        y: cols.y + 7.0,
                        width: 1.0,
                        height: cols.height - 14.0,
                    },
                    [0.24, 0.25, 0.30, 0.8],
                    [0.0; 4],
                    0.0,
                );
                let visible = visible_rows(panel);
                for row in 0..visible {
                    let Some(line) = project.line_at(self.scroll + row) else {
                        break;
                    };
                    let rr = row_rect(panel, row);
                    if self.selected.contains(&line.id) {
                        solid(quads, rr, [0.12, 0.14, 0.23, 1.0], [0.0; 4], 0.0);
                        solid(
                            quads,
                            Rect {
                                x: rr.x,
                                y: rr.y,
                                width: 3.0,
                                height: rr.height,
                            },
                            [0.42, 0.48, 0.92, 1.0],
                            [0.0; 4],
                            0.0,
                        );
                    }
                    if !self.keyboard_close && self.scroll + row == self.keyboard_row {
                        let focus_rect = if self.keyboard_column == 0 {
                            Rect {
                                x: rr.x + 2.0,
                                y: rr.y + 2.0,
                                width: role_w - 4.0,
                                height: rr.height - 4.0,
                            }
                        } else {
                            Rect {
                                x: rr.x + role_w + 2.0,
                                y: rr.y + 2.0,
                                width: rr.width - role_w - 4.0,
                                height: rr.height - 4.0,
                            }
                        };
                        solid(quads, focus_rect, [0.0; 4], [0.38, 0.58, 0.96, 1.0], 4.0);
                    }
                    solid(
                        quads,
                        Rect {
                            x: rr.x + 10.0,
                            y: rr.y + rr.height - 1.0,
                            width: rr.width - 20.0,
                            height: 1.0,
                        },
                        [0.18, 0.185, 0.22, 0.55],
                        [0.0; 4],
                        0.0,
                    );
                    let role_text = &line.character_name;
                    let line_text = if self.editing == Some(EditField::LineText(line.id)) {
                        &self.edit_buffer
                    } else {
                        &line.text
                    };
                    if self.editing == Some(EditField::LineText(line.id)) {
                        solid(
                            quads,
                            Rect {
                                x: panel.x + role_w + 2.0,
                                y: rr.y + 3.0,
                                width: panel.width - role_w - 6.0,
                                height: rr.height - 6.0,
                            },
                            [0.10, 0.13, 0.20, 1.0],
                            [0.38, 0.58, 0.96, 1.0],
                            4.0,
                        );
                    }
                    labels.push(label(
                        role_text,
                        Rect {
                            x: panel.x + PAD,
                            y: rr.y,
                            width: role_w - PAD,
                            height: rr.height,
                        },
                        HAlign::Left,
                        13.0,
                        [220, 222, 232],
                    ));
                    labels.push(label(
                        "▾",
                        Rect {
                            x: panel.x + role_w - 24.0,
                            y: rr.y,
                            width: 18.0,
                            height: rr.height,
                        },
                        HAlign::Center,
                        11.0,
                        [126, 132, 154],
                    ));
                    labels.push(label(
                        line_text,
                        Rect {
                            x: panel.x + role_w + 6.0,
                            y: rr.y,
                            width: panel.width - role_w - 16.0,
                            height: rr.height,
                        },
                        HAlign::Left,
                        13.0,
                        [205, 207, 218],
                    ));
                }
                if project.line_count() == 0 {
                    labels.push(label(
                        t("panel.empty.lines"),
                        body_rect(panel),
                        HAlign::Center,
                        14.0,
                        [125, 128, 145],
                    ));
                }
            }
            SidePanelKind::Roles => {
                labels.push(label(
                    t("panel.name"),
                    Rect {
                        x: panel.x + 46.0,
                        y: cols.y,
                        width: panel.width - 56.0,
                        height: cols.height,
                    },
                    HAlign::Left,
                    11.0,
                    [150, 154, 170],
                ));
                let all_roles = roles(project);
                for (row, (name, color)) in all_roles
                    .iter()
                    .skip(self.scroll)
                    .take(visible_rows(panel))
                    .enumerate()
                {
                    let rr = row_rect(panel, row);
                    if !self.keyboard_close && self.scroll + row == self.keyboard_row {
                        solid(
                            quads,
                            Rect {
                                x: rr.x + 2.0,
                                y: rr.y + 2.0,
                                width: rr.width - 4.0,
                                height: rr.height - 4.0,
                            },
                            [0.0; 4],
                            [0.38, 0.58, 0.96, 1.0],
                            4.0,
                        );
                    }
                    solid(
                        quads,
                        Rect {
                            x: rr.x + 10.0,
                            y: rr.y + rr.height - 1.0,
                            width: rr.width - 20.0,
                            height: 1.0,
                        },
                        [0.18, 0.185, 0.22, 0.55],
                        [0.0; 4],
                        0.0,
                    );
                    solid(
                        quads,
                        Rect {
                            x: panel.x + PAD,
                            y: rr.y + 10.0,
                            width: 22.0,
                            height: 22.0,
                        },
                        *color,
                        [0.55, 0.56, 0.62, 1.0],
                        4.0,
                    );
                    let text: &str = if self.editing == Some(EditField::RoleName)
                        && self.edit_original == *name
                    {
                        self.edit_buffer.as_str()
                    } else {
                        name
                    };
                    if self.editing == Some(EditField::RoleName) && self.edit_original == *name {
                        solid(
                            quads,
                            Rect {
                                x: panel.x + 40.0,
                                y: rr.y + 3.0,
                                width: panel.width - 44.0,
                                height: rr.height - 6.0,
                            },
                            [0.10, 0.13, 0.20, 1.0],
                            [0.38, 0.58, 0.96, 1.0],
                            4.0,
                        );
                    }
                    labels.push(label(
                        text,
                        Rect {
                            x: panel.x + 46.0,
                            y: rr.y,
                            width: panel.width - 56.0,
                            height: rr.height,
                        },
                        HAlign::Left,
                        14.0,
                        [220, 222, 232],
                    ));
                }
                if all_roles.is_empty() {
                    labels.push(label(
                        t("panel.empty.roles"),
                        body_rect(panel),
                        HAlign::Center,
                        14.0,
                        [125, 128, 145],
                    ));
                }
            }
        }

        if let Some((track, thumb, _)) = scrollbar_geometry(panel, item_count, self.scroll) {
            solid(quads, track, [0.10, 0.103, 0.125, 1.0], [0.0; 4], 3.0);
            solid(
                quads,
                thumb,
                if self.dragging_scrollbar {
                    [0.48, 0.53, 0.82, 1.0]
                } else {
                    [0.31, 0.33, 0.42, 1.0]
                },
                [0.0; 4],
                3.0,
            );
        }
    }

    /// Contextual overlays use the modal text layer so regular table labels
    /// can never be rendered over them.
    pub fn render_menus<'a>(
        &'a self,
        project: &'a Project,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        if let Some((x, y)) = self.context_menu {
            let single_line = self.selected.len() == 1;
            let r = Rect {
                x,
                y,
                width: 190.0,
                height: if single_line { 64.0 } else { 32.0 },
            };
            solid(
                quads,
                r,
                [0.13, 0.135, 0.16, 1.0],
                [0.34, 0.35, 0.42, 1.0],
                5.0,
            );
            if single_line {
                let go_to = Rect { height: 32.0, ..r };
                labels.push(label(
                    t("panel.go_to"),
                    go_to,
                    HAlign::Left,
                    13.0,
                    [232, 234, 242],
                ));
                solid(
                    quads,
                    Rect {
                        x: r.x + 8.0,
                        y: r.y + 32.0,
                        width: r.width - 16.0,
                        height: 1.0,
                    },
                    [0.28, 0.29, 0.35, 0.8],
                    [0.0; 4],
                    0.0,
                );
                labels.push(label(
                    t("panel.change_role"),
                    Rect {
                        y: r.y + 32.0,
                        height: 32.0,
                        ..r
                    },
                    HAlign::Left,
                    13.0,
                    [232, 234, 242],
                ));
            } else {
                labels.push(label(
                    t("panel.change_role"),
                    r,
                    HAlign::Left,
                    13.0,
                    [232, 234, 242],
                ));
            }
        }
        if let Some((x, y)) = self.role_picker {
            let all_roles = roles(project);
            let (r, visible) = role_picker_geometry(x, y, all_roles.len());
            let max_scroll = all_roles.len().saturating_sub(visible);
            let scroll = self.role_picker_scroll.min(max_scroll);
            solid(
                quads,
                r,
                [0.12, 0.125, 0.15, 1.0],
                [0.34, 0.35, 0.42, 1.0],
                5.0,
            );
            labels.push(label(
                t("panel.choose_role"),
                Rect {
                    x,
                    y,
                    width: r.width,
                    height: ROLE_PICKER_HEADER_H,
                },
                HAlign::Left,
                12.0,
                [155, 158, 175],
            ));
            for (i, (name, color)) in all_roles.iter().skip(scroll).take(visible).enumerate() {
                let rr = Rect {
                    x,
                    y: y + ROLE_PICKER_HEADER_H + i as f32 * ROLE_PICKER_ROW_H,
                    width: r.width,
                    height: ROLE_PICKER_ROW_H,
                };
                if scroll + i == self.role_picker_index {
                    solid(
                        quads,
                        rr,
                        [0.16, 0.19, 0.30, 1.0],
                        [0.38, 0.58, 0.96, 1.0],
                        3.0,
                    );
                }
                solid(
                    quads,
                    Rect {
                        x: x + 8.0,
                        y: rr.y + 8.0,
                        width: 14.0,
                        height: 14.0,
                    },
                    *color,
                    [0.0; 4],
                    3.0,
                );
                labels.push(label(
                    name,
                    Rect {
                        x: x + 28.0,
                        width: r.width - 40.0,
                        ..rr
                    },
                    HAlign::Left,
                    13.0,
                    [225, 227, 236],
                ));
            }
            if max_scroll > 0 {
                let track = Rect {
                    x: r.x + r.width - 7.0,
                    y: r.y + ROLE_PICKER_HEADER_H + 4.0,
                    width: 3.0,
                    height: r.height - ROLE_PICKER_HEADER_H - 8.0,
                };
                let thumb_h = (track.height * visible as f32 / all_roles.len() as f32)
                    .max(18.0)
                    .min(track.height);
                let travel = track.height - thumb_h;
                let thumb = Rect {
                    x: track.x,
                    y: track.y + travel * scroll as f32 / max_scroll as f32,
                    width: track.width,
                    height: thumb_h,
                };
                solid(quads, track, [0.20, 0.205, 0.24, 1.0], [0.0; 4], 2.0);
                solid(quads, thumb, [0.48, 0.53, 0.82, 1.0], [0.0; 4], 2.0);
            }
        }
    }
}

fn role_picker_geometry(x: f32, y: f32, item_count: usize) -> (Rect, usize) {
    let visible = item_count.min(ROLE_PICKER_MAX_ROWS);
    (
        Rect {
            x,
            y,
            width: ROLE_PICKER_W,
            height: ROLE_PICKER_HEADER_H + visible as f32 * ROLE_PICKER_ROW_H,
        },
        visible,
    )
}

fn pointer_down_position(event: &UiEvent) -> Option<(f32, f32)> {
    match event {
        UiEvent::MousePress { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::ContextMenu { x, y } => Some((*x, *y)),
        _ => None,
    }
}

fn keyboard_activation(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Activate)
        || matches!(event, UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " ")
}

fn roles(project: &Project) -> Vec<(&str, [f32; 4])> {
    let mut out = Vec::new();
    for c in project.known_characters() {
        if !c.name.trim().is_empty() && !out.iter().any(|(n, _)| *n == c.name) {
            out.push((c.name.as_str(), c.color));
        }
    }
    for l in project.lines() {
        if l.kind.is_dialogue()
            && !l.character_name.trim().is_empty()
            && !out.iter().any(|(n, _)| *n == l.character_name)
        {
            out.push((l.character_name.as_str(), l.character_color));
        }
    }
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out
}
fn close_rect(p: Rect) -> Rect {
    Rect {
        x: p.x + p.width - 36.0,
        y: p.y + 8.0,
        width: 26.0,
        height: 26.0,
    }
}
fn columns_rect(p: Rect) -> Rect {
    Rect {
        x: p.x,
        y: p.y + HEADER_H,
        width: p.width,
        height: COLUMNS_H,
    }
}
fn body_rect(p: Rect) -> Rect {
    Rect {
        x: p.x,
        y: p.y + HEADER_H + COLUMNS_H,
        width: p.width,
        height: (p.height - HEADER_H - COLUMNS_H).max(0.0),
    }
}
fn visible_rows(p: Rect) -> usize {
    (body_rect(p).height / ROW_H).floor().max(1.0) as usize
}
fn row_rect(p: Rect, row: usize) -> Rect {
    let b = body_rect(p);
    Rect {
        x: b.x,
        y: b.y + row as f32 * ROW_H,
        width: b.width,
        height: ROW_H,
    }
}
fn row_at(p: Rect, y: f32) -> Option<usize> {
    let b = body_rect(p);
    if y < b.y || y > b.y + b.height {
        None
    } else {
        Some(((y - b.y) / ROW_H) as usize)
    }
}
fn scrollbar_geometry(
    panel: Rect,
    item_count: usize,
    scroll: usize,
) -> Option<(Rect, Rect, usize)> {
    let visible = visible_rows(panel);
    let max_scroll = item_count.saturating_sub(visible);
    if max_scroll == 0 {
        return None;
    }
    let body = body_rect(panel);
    let track = Rect {
        x: panel.x + panel.width - 10.0,
        y: body.y + 6.0,
        width: 4.0,
        height: (body.height - 12.0).max(28.0),
    };
    let thumb_h = (track.height * visible as f32 / item_count as f32).clamp(28.0, track.height);
    let travel = (track.height - thumb_h).max(0.0);
    let ratio = scroll.min(max_scroll) as f32 / max_scroll as f32;
    let thumb = Rect {
        x: track.x,
        y: track.y + ratio * travel,
        width: track.width,
        height: thumb_h,
    };
    Some((track, thumb, max_scroll))
}
fn event_xy(e: &UiEvent) -> (f32, f32) {
    match e {
        UiEvent::MouseMove { x, y }
        | UiEvent::MouseRelease { x, y }
        | UiEvent::MiddlePress { x, y }
        | UiEvent::MiddleRelease { x, y } => (*x, *y),
        _ => (-1.0, -1.0),
    }
}
fn solid(q: &mut Vec<QuadInstance>, r: Rect, c: [f32; 4], border: [f32; 4], radius: f32) {
    q.push(QuadInstance {
        rect: [r.x, r.y, r.width, r.height],
        color: c,
        color_bottom: c,
        border_color: border,
        border_width: if border == [0.0; 4] { 0.0 } else { 1.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}
fn label<'a>(
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    size: f32,
    color: [u8; 3],
) -> LabelInfo<'a> {
    LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(size),
        color_override: Some(color),
        font_family_override: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_panel_replaces_the_previous_one() {
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        assert_eq!(panel.kind, Some(SidePanelKind::Lines));
        panel.selected.insert(42);

        panel.open(SidePanelKind::Roles);

        assert_eq!(panel.kind, Some(SidePanelKind::Roles));
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn close_resets_panel_state() {
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        panel.selected.insert(7);

        panel.close();

        assert!(!panel.is_open());
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn wheel_direction_matches_the_list_direction() {
        let mut project = Project::new();
        for index in 0..10 {
            project.add_line(index * 100, 50, 0.0);
        }
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        panel.scroll = 2;
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 180.0,
        };

        panel.handle_event(
            &UiEvent::Scroll {
                x: 100.0,
                y: 120.0,
                delta: 1.0,
                fast: false,
                ctrl: false,
            },
            bounds,
            &project,
        );
        assert_eq!(panel.scroll, 1, "wheel up must move toward the first row");

        panel.handle_event(
            &UiEvent::Scroll {
                x: 100.0,
                y: 120.0,
                delta: -1.0,
                fast: false,
                ctrl: false,
            },
            bounds,
            &project,
        );
        assert_eq!(panel.scroll, 2, "wheel down must move toward later rows");
    }

    #[test]
    fn double_click_starts_a_real_text_edit() {
        let mut project = Project::new();
        project.add_line_full(
            0,
            50,
            0.0,
            "Bonjour".into(),
            "Alice".into(),
            [0.8, 0.2, 0.3, 1.0],
        );
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 300.0,
        };
        let first_row_y = bounds.y + HEADER_H + COLUMNS_H + ROW_H / 2.0;

        panel.handle_event(
            &UiEvent::DoubleClick {
                x: 250.0,
                y: first_row_y,
            },
            bounds,
            &project,
        );

        assert!(panel.is_editing_text());
        assert!(matches!(panel.editing, Some(EditField::LineText(_))));
    }

    #[test]
    fn double_clicking_a_role_cell_opens_the_existing_roles() {
        let mut project = Project::new();
        let alice_id = project.add_line_full(
            0,
            50,
            0.0,
            "Bonjour".into(),
            "Alice".into(),
            [0.8, 0.2, 0.3, 1.0],
        );
        project.add_line_full(
            100,
            50,
            0.0,
            "Salut".into(),
            "Bob".into(),
            [0.2, 0.4, 0.9, 1.0],
        );
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 300.0,
        };
        let first_row_y = bounds.y + HEADER_H + COLUMNS_H + ROW_H / 2.0;

        panel.handle_event(
            &UiEvent::DoubleClick {
                x: 50.0,
                y: first_row_y,
            },
            bounds,
            &project,
        );

        assert!(!panel.is_editing_text());
        let (picker_x, picker_y) = panel.role_picker.expect("role picker should open");
        let response = panel.handle_event(
            &UiEvent::MousePress {
                x: picker_x + 50.0,
                y: picker_y + 30.0 + 45.0,
            },
            bounds,
            &project,
        );
        match response {
            Some(EventResponse::Action(UiAction::SetLinesRole {
                line_ids,
                name,
                color,
            })) => {
                assert_eq!(line_ids, vec![alice_id]);
                assert_eq!(name, "Bob");
                assert_eq!(color, [0.2, 0.4, 0.9, 1.0]);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn role_picker_owns_wheel_input_and_scrolls_independently() {
        let mut project = Project::new();
        let mut selected_id = 0;
        for index in 0..12 {
            let id = project.add_line_full(
                index * 100,
                50,
                0.0,
                format!("Ligne {index}"),
                format!("Role {index:02}"),
                [0.2, 0.4, 0.8, 1.0],
            );
            if index == 0 {
                selected_id = id;
            }
        }
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        panel.scroll = 2;
        panel.selected.insert(selected_id);
        panel.role_picker = Some((20.0, 90.0));
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 380.0,
        };

        let response = panel.handle_event(
            &UiEvent::Scroll {
                x: 100.0,
                y: 150.0,
                delta: -1.0,
                fast: false,
                ctrl: false,
            },
            bounds,
            &project,
        );

        assert_eq!(response, Some(EventResponse::Consumed));
        assert_eq!(panel.role_picker_scroll, 1);
        assert_eq!(
            panel.scroll, 2,
            "the table below must not receive the wheel"
        );

        let response = panel.handle_event(
            &UiEvent::MousePress {
                x: 100.0,
                y: 90.0 + ROLE_PICKER_HEADER_H + ROLE_PICKER_ROW_H / 2.0,
            },
            bounds,
            &project,
        );
        match response {
            Some(EventResponse::Action(UiAction::SetLinesRole { name, .. })) => {
                assert_eq!(name, "Role 01");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn single_line_context_menu_can_seek_to_the_line() {
        let mut project = Project::new();
        project.add_line_full(
            123,
            50,
            0.0,
            "Bonjour".into(),
            "Alice".into(),
            [0.8, 0.2, 0.3, 1.0],
        );
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 300.0,
        };
        let row_y = bounds.y + HEADER_H + COLUMNS_H + ROW_H / 2.0;

        panel.handle_event(
            &UiEvent::ContextMenu { x: 80.0, y: row_y },
            bounds,
            &project,
        );
        let (menu_x, menu_y) = panel.context_menu.expect("context menu should open");
        let response = panel.handle_event(
            &UiEvent::MousePress {
                x: menu_x + 10.0,
                y: menu_y + 10.0,
            },
            bounds,
            &project,
        );

        assert_eq!(
            response,
            Some(EventResponse::Action(UiAction::SeekAbsolute(123)))
        );
    }

    #[test]
    fn scrollbar_thumb_tracks_the_scroll_position() {
        let panel = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 300.0,
        };
        let (track, first, max_scroll) =
            scrollbar_geometry(panel, 100, 0).expect("long list needs a scrollbar");
        let (_, last, _) = scrollbar_geometry(panel, 100, max_scroll).unwrap();

        assert!(first.height < track.height);
        assert_eq!(first.y, track.y);
        assert!((last.y + last.height - (track.y + track.height)).abs() < 0.01);
    }

    #[test]
    fn opening_a_menu_keeps_table_labels_in_the_overlay_layer() {
        let mut project = Project::new();
        let line_id = project.add_line_full(
            0,
            50,
            0.0,
            "Bonjour".into(),
            "Alice".into(),
            [0.8, 0.2, 0.3, 1.0],
        );
        let mut panel = SidePanel::default();
        panel.open(SidePanelKind::Lines);
        panel.selected.insert(line_id);
        panel.context_menu = Some((20.0, 140.0));
        let bounds = Rect {
            x: 0.0,
            y: 32.0,
            width: 320.0,
            height: 300.0,
        };
        let mut panel_quads = Vec::new();
        let mut panel_labels = Vec::new();
        let mut menu_quads = Vec::new();
        let mut menu_labels = Vec::new();

        panel.render(bounds, &project, &mut panel_quads, &mut panel_labels);
        panel.render_menus(&project, &mut menu_quads, &mut menu_labels);

        assert!(panel_labels.iter().any(|label| label.text == "Alice"));
        assert!(menu_labels
            .iter()
            .any(|label| label.text == t("panel.go_to")));
    }
}
