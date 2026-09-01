//! File tree side panel replacing the legacy media explorer modal.
//!
//! Renders the project media library (videos with proxy children, rythmo
//! bands, audios) as a beui-style animated tree. Events produce
//! `UiAction`s consumed by the dispatcher; the tree itself only owns
//! presentation state (selection, expansion, scroll, rename buffer, drag).

pub mod animation;
pub mod data;
pub mod rows;

pub use data::FileTreeData;

use std::collections::HashMap;

use crate::i18n::t;
use crate::project::{MediaId, SyllableLanguage};

use self::animation::{Spring, SpringValue, SPRING_LAYOUT, SPRING_SWAP, Tween, ENTER_DURATION};
use self::data::{AudioData, VideoData};
use self::rows::{flatten, AudioRowId, ExpandedSet, GroupKind, Row, RowId};

use super::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use super::text_input::{TextInputAction, TextInputState};

const HEADER_H: f32 = 50.0;
const ROW_H: f32 = 36.0;
const INDENT: f32 = 18.0;
const CHEVRON_W: f32 = 14.0;
const ICON_SIZE: f32 = 16.0;
const PAD: f32 = 10.0;
const SCROLLBAR_W: f32 = 4.0;
const QUICK_ACTION_SIZE: f32 = 20.0;
/// Movement (px) before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub enum RenameTarget {
    Video(MediaId),
    Audio(MediaId),
    Band(u64),
}

#[derive(Clone, Debug)]
struct DragState {
    id: RowId,
    label: String,
    origin: (f32, f32),
    current: (f32, f32),
}

impl DragState {
    fn is_past_threshold(&self) -> bool {
        (self.current.0 - self.origin.0).abs() > DRAG_THRESHOLD
            || (self.current.1 - self.origin.1).abs() > DRAG_THRESHOLD
    }
}

pub struct FileTree {
    open: bool,
    expanded: ExpandedSet,
    selected: Option<RowId>,
    focused: Option<RowId>,
    scroll: usize,
    hover: Option<RowId>,
    /// Shared-layout hover pill: y position springs between rows.
    hover_pill: Option<SpringValue>,
    hover_pill_fade: f32,
    /// Per-row entry animations (offset-y + opacity), keyed by row id.
    enter: HashMap<RowId, Tween>,
    /// Chevron rotation progress per group (0 = closed, 90 = open).
    chevron: HashMap<GroupKind, SpringValue>,
    rename: Option<(RenameTarget, String)>,
    rename_original: String,
    rename_input: TextInputState,
    context_menu: Option<ContextMenu>,
    drag: Option<DragState>,
    /// Row rects from the last render, used for hit-testing and the
    /// insertion line during drags.
    row_rects: Vec<Row>,
    scroll_drag: Option<f32>,
}

/// Contextual menu with an optional submenu (one level, "▸" style).
pub struct ContextMenu {
    pub anchor: (f32, f32),
    pub index: usize,
    pub submenu: Option<Submenu>,
    pub submenu_index: usize,
    pub target: RowId,
    /// Labels snapshot taken when the menu opened (avoids rebuilding
    /// per-frame owned strings during render).
    pub labels: Vec<String>,
    pub submenu_labels: Vec<String>,
}

pub struct Submenu {
    pub kind: SubmenuKind,
    pub anchor: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SubmenuKind {
    /// "Associer en tant que proxy de ▸" — eligible top-level videos.
    AssociateProxyTo,
    /// "Définir la langue de découpe ▸".
    SyllableLanguage,
    /// "Définir l'instrumental ▸".
    Instrumental,
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTree {
    pub fn new() -> Self {
        let mut chevron = HashMap::new();
        for kind in [GroupKind::Videos, GroupKind::Bands, GroupKind::Audios] {
            chevron.insert(kind, SpringValue::at(90.0));
        }
        Self {
            open: false,
            expanded: ExpandedSet::all_expanded(),
            selected: None,
            focused: None,
            scroll: 0,
            hover: None,
            hover_pill: None,
            hover_pill_fade: 0.0,
            enter: HashMap::new(),
            chevron,
            rename: None,
            rename_original: String::new(),
            rename_input: TextInputState::new(),
            context_menu: None,
            drag: None,
            row_rects: Vec::new(),
            scroll_drag: None,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.enter.clear();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.cancel_rename();
        self.context_menu = None;
        self.drag = None;
        self.hover = None;
        self.hover_pill = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_editing_text(&self) -> bool {
        self.rename.is_some() && self.rename_input.active
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        self.rename_input.next_cursor_blink_deadline()
    }

    /// Advance every animation by `dt`; returns true while any is running.
    pub fn animate(&mut self, data: &FileTreeData, dt: f32) -> bool {
        if !self.open {
            return false;
        }
        let mut running = false;

        if let Some(pill) = self.hover_pill.as_mut() {
            pill.step(SPRING_LAYOUT, dt);
            let target_fade = if self.hover.is_some() { 1.0 } else { 0.0 };
            let fade_delta = (target_fade - self.hover_pill_fade).signum() * dt / 0.15;
            self.hover_pill_fade = (self.hover_pill_fade + fade_delta).clamp(0.0, 1.0);
            if !pill.settled() || (target_fade - self.hover_pill_fade).abs() > 1e-3 {
                running = true;
            }
        }

        for (_, tween) in self.enter.iter_mut() {
            tween.advance(dt);
            if !tween.finished() {
                running = true;
            }
        }
        self.enter.retain(|_, tween| !tween.finished());

        // Retarget chevrons towards their open/closed angle.
        for (kind, spring) in self.chevron.iter_mut() {
            let open = self.expanded.get(*kind);
            spring.retarget(if open { 90.0 } else { 0.0 });
            spring.step(SPRING_SWAP, dt);
            if !spring.settled() {
                running = true;
            }
        }

        running
    }

    pub fn captures_keyboard_event(&self, event: &UiEvent) -> bool {
        self.open
            && matches!(
                event,
                UiEvent::KeyInput { .. }
                    | UiEvent::FocusNext
                    | UiEvent::FocusPrevious
                    | UiEvent::Activate
                    | UiEvent::CursorLeft
                    | UiEvent::CursorRight
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::Home
                    | UiEvent::End
                    | UiEvent::PageUp
                    | UiEvent::PageDown
                    | UiEvent::OpenContextMenu
                    | UiEvent::Delete
            )
    }

    // -- Event handling --

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        panel: Rect,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        if !self.open {
            return None;
        }

        if let Some(response) = self.handle_rename_keyboard(event, data) {
            return Some(response);
        }
        if let Some(response) = self.handle_menu_keyboard(event, data) {
            return Some(response);
        }
        if self.captures_keyboard_event(event) {
            return Some(self.handle_tree_keyboard(event, panel, data));
        }

        // Context menu and drag ghost float above the tree.
        if let Some(response) = self.handle_menu_pointer(event, panel, data) {
            return Some(response);
        }

        match event {
            UiEvent::MouseMove { x, y } => {
                if let Some(drag) = self.drag.as_mut() {
                    drag.current = (*x, *y);
                    self.auto_scroll_during_drag(panel);
                    return Some(EventResponse::Consumed);
                }
                let hovered = self.row_at(panel, *x, *y, data);
                if hovered != self.hover {
                    self.hover = hovered;
                    let row_y = hovered.and_then(|row| {
                        let index = row_index_of(&self.row_rects, row)?;
                        self.row_y(panel, Some(index - self.scroll.min(index)), data)
                    });
                    if let Some(y) = row_y {
                        let pill = self.hover_pill.get_or_insert(SpringValue::at(y));
                        pill.retarget(y);
                    }
                }
                if panel.contains(*x, *y) {
                    Some(EventResponse::Consumed)
                } else {
                    None
                }
            }
            UiEvent::MousePress { x, y } => {
                if !panel.contains(*x, *y) {
                    return None;
                }
                let Some(row) = self.row_at(panel, *x, *y, data) else {
                    return Some(EventResponse::Consumed);
                };
                if let Some((target, _)) = &self.rename {
                    // Click outside commits the rename.
                    return Some(self.finish_rename(data, target.clone()));
                }
                self.selected = Some(row);
                self.focused = Some(row);
                match row {
                    RowId::Group(kind) => {
                        self.expanded.toggle(kind);
                        self.enter.clear();
                        Some(self.expand_event(kind))
                    }
                    RowId::Root => Some(EventResponse::Consumed),
                    _ => {
                        // Arm a potential drag (element rows only).
                        self.drag = Some(DragState {
                            id: row,
                            label: self.row_label(row, data).to_string(),
                            origin: (*x, *y),
                            current: (*x, *y),
                        });
                        Some(EventResponse::Consumed)
                    }
                }
            }
            UiEvent::MouseRelease { x, y } => {
                if let Some(drag) = self.drag.take() {
                    if drag.is_past_threshold() {
                        return Some(self.drop_drag(drag.id, *x, *y, panel, data));
                    }
                }
                if panel.contains(*x, *y) {
                    Some(EventResponse::Consumed)
                } else {
                    None
                }
            }
            UiEvent::DoubleClick { x, y } => {
                let Some(row) = self.row_at(panel, *x, *y, data) else {
                    return Some(EventResponse::Consumed);
                };
                Some(self.activate_row(row, data))
            }
            UiEvent::ContextMenu { x, y } => {
                let Some(row) = self.row_at(panel, *x, *y, data) else {
                    if panel.contains(*x, *y) {
                        return Some(EventResponse::Consumed);
                    }
                    return None;
                };
                if let Some((target, _)) = &self.rename {
                    return Some(self.finish_rename(data, target.clone()));
                }
                self.selected = Some(row);
                self.open_context_menu(row, *x, *y, panel, data);
                Some(EventResponse::Consumed)
            }
            UiEvent::Scroll { x, y, delta, .. } if panel.contains(*x, *y) => {
                let rows_all = flatten(data, &self.expanded);
                let visible = visible_rows(panel);
                let max = rows_all.len().saturating_sub(visible);
                if *delta > 0.0 {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.scroll = (self.scroll + 1).min(max);
                }
                Some(EventResponse::Consumed)
            }
            _ => {
                if panel.contains(event_xy(event).0, event_xy(event).1) {
                    Some(EventResponse::Consumed)
                } else {
                    None
                }
            }
        }
    }

    fn handle_tree_keyboard(
        &mut self,
        event: &UiEvent,
        panel: Rect,
        data: &FileTreeData,
    ) -> EventResponse {
        let rows = flatten(data, &self.expanded);
        let index = self
            .focused
            .and_then(|id| rows.iter().position(|row| row.id == id));
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                EventResponse::Action(UiAction::CloseFileTree)
            }
            UiEvent::CursorDown | UiEvent::FocusNext => {
                let next = index.map(|i| (i + 1).min(rows.len() - 1)).unwrap_or(0);
                self.focused = rows.get(next).map(|row| row.id);
                self.ensure_focused_visible(panel, data);
                self.focus_event(data)
            }
            UiEvent::CursorUp | UiEvent::FocusPrevious => {
                let previous = index
                    .map(|i| i.saturating_sub(1))
                    .unwrap_or(rows.len().saturating_sub(1));
                self.focused = rows.get(previous).map(|row| row.id);
                self.ensure_focused_visible(panel, data);
                self.focus_event(data)
            }
            UiEvent::PageDown | UiEvent::PageUp => {
                let step = visible_rows(panel);
                let next = match index {
                    Some(i) if matches!(event, UiEvent::PageUp) => i.saturating_sub(step),
                    Some(i) => (i + step).min(rows.len() - 1),
                    None => 0,
                };
                self.focused = rows.get(next).map(|row| row.id);
                self.ensure_focused_visible(panel, data);
                self.focus_event(data)
            }
            UiEvent::Home => {
                self.focused = rows.first().map(|row| row.id);
                self.scroll = 0;
                self.focus_event(data)
            }
            UiEvent::End => {
                self.focused = rows.last().map(|row| row.id);
                self.ensure_focused_visible(panel, data);
                self.focus_event(data)
            }
            UiEvent::CursorRight => {
                if let Some(RowId::Group(kind)) = self.focused {
                    if !self.expanded.get(kind) {
                        self.expanded.toggle(kind);
                        self.enter.clear();
                        return self.expand_event(kind);
                    }
                } else if let Some(id) = self.focused {
                    // descend: focus next visible row
                    let next = index.map(|i| (i + 1).min(rows.len() - 1)).unwrap_or(0);
                    self.focused = rows.get(next).map(|row| row.id);
                    self.ensure_focused_visible(panel, data);
                }
                self.focus_event(data)
            }
            UiEvent::CursorLeft => {
                if let Some(RowId::Group(kind)) = self.focused {
                    if self.expanded.get(kind) {
                        self.expanded.toggle(kind);
                        self.enter.clear();
                        return self.expand_event(kind);
                    }
                } else {
                    // focus the parent group of the current entry
                    let group = self.focused.and_then(|id| parent_group(id));
                    if let Some(kind) = group {
                        if self.expanded.get(kind) && index.is_some_and(|i| i > 0) {
                            // collapse only when the entry is a top-level child
                        }
                        self.focused = Some(RowId::Group(kind));
                    }
                }
                self.focus_event(data)
            }
            UiEvent::Activate => {
                let Some(id) = self.focused else {
                    return EventResponse::Consumed;
                };
                self.activate_row(id, data)
            }
            UiEvent::KeyInput { text } if text == " " => {
                let Some(id) = self.focused else {
                    return EventResponse::Consumed;
                };
                if let RowId::Group(kind) = id {
                    self.expanded.toggle(kind);
                    self.enter.clear();
                    return self.expand_event(kind);
                }
                self.selected = Some(id);
                self.focus_event(data)
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                let Some(id) = self.focused else {
                    return EventResponse::Consumed;
                };
                self.activate_row(id, data)
            }
            UiEvent::OpenContextMenu => {
                let Some(id) = self.focused else {
                    return EventResponse::Consumed;
                };
                let (x, y) = self
                    .row_screen_position(panel, id, data)
                    .unwrap_or((panel.x + 12.0, panel.y + HEADER_H + 8.0));
                self.open_context_menu(id, x, y, panel, data);
                EventResponse::Consumed
            }
            UiEvent::Delete => {
                let Some(id) = self.focused else {
                    return EventResponse::Consumed;
                };
                self.remove_action(id, data)
                    .map(EventResponse::Action)
                    .unwrap_or(EventResponse::Consumed)
            }
            _ => EventResponse::Consumed,
        }
    }

    fn activate_row(&mut self, id: RowId, data: &FileTreeData) -> EventResponse {
        match id {
            RowId::Group(kind) => {
                self.expanded.toggle(kind);
                self.enter.clear();
                self.expand_event(kind)
            }
            RowId::Root => EventResponse::Consumed,
            RowId::Video(media_id) => EventResponse::Action(UiAction::MediaVideoUse { id: media_id }),
            RowId::Audio(AudioRowId::OriginalVideo) => EventResponse::Consumed,
            RowId::Audio(AudioRowId::Media(media_id)) => {
                // Double-click on an audio: nothing special (single-clic selects).
                let _ = media_id;
                EventResponse::Consumed
            }
            RowId::Band(band_id) => {
                // Spec exception: clicking a band loads it immediately.
                if band_id == data
                    .bands
                    .iter()
                    .find(|band| band.active)
                    .map(|band| band.id)
                    .unwrap_or(u64::MAX)
                {
                    return EventResponse::Consumed;
                }
                EventResponse::Action(UiAction::SelectLanguage { id: band_id })
            }
        }
    }

    fn remove_action(&self, id: RowId, _data: &FileTreeData) -> Option<UiAction> {
        match id {
            RowId::Video(media_id) => Some(UiAction::MediaVideoRemove { id: media_id }),
            RowId::Audio(AudioRowId::Media(media_id)) => {
                Some(UiAction::MediaAudioRemove { id: media_id })
            }
            RowId::Band(band_id) => Some(UiAction::DeleteLanguage { id: band_id }),
            _ => None,
        }
    }

    pub fn begin_rename(&mut self, target: RenameTarget, value: &str) {
        self.start_rename(target, value);
    }

    fn start_rename(&mut self, target: RenameTarget, value: &str) {
        self.rename = Some((target, value.to_string()));
        self.rename_original = value.to_string();
        self.rename_input.activate(value);
    }

    fn cancel_rename(&mut self) {
        self.rename = None;
        self.rename_original.clear();
        self.rename_input.deactivate();
    }

    fn finish_rename(&mut self, data: &FileTreeData, target: RenameTarget) -> EventResponse {
        let Some((current_target, _)) = self.rename.take() else {
            return EventResponse::Consumed;
        };
        let _ = target;
        let value = self.rename_buffer().trim().to_string();
        self.rename_input.deactivate();
        if value.is_empty() || value == self.rename_original {
            return EventResponse::Consumed;
        }
        match current_target {
            RenameTarget::Video(id) => {
                EventResponse::Action(UiAction::MediaVideoRename { id, name: value })
            }
            RenameTarget::Audio(id) => {
                EventResponse::Action(UiAction::MediaAudioRename { id, name: value })
            }
            RenameTarget::Band(id) => {
                let _ = data;
                EventResponse::Action(UiAction::RenameLanguage { id, name: value })
            }
        }
    }

    fn rename_buffer(&self) -> String {
        self.rename
            .as_ref()
            .map(|(_, buffer)| buffer.clone())
            .unwrap_or_default()
    }

    fn handle_rename_keyboard(
        &mut self,
        event: &UiEvent,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        let (target, _) = self.rename.clone()?;
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.cancel_rename();
                Some(EventResponse::Action(UiAction::Accessibility(
                    crate::accessibility::AccessibilityEvent::Closed {
                        label: t("file_tree.rename").to_string(),
                    },
                )))
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                Some(self.finish_rename(data, target))
            }
            UiEvent::KeyInput { text } => {
                let buffer = self.rename_buffer();
                if let Some(action) = self.rename_input.handle_key(text, &buffer) {
                    if let TextInputAction::Changed(value) = action {
                        if let Some((_, buffer)) = self.rename.as_mut() {
                            *buffer = value;
                        }
                    }
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorLeft => {
                self.rename_input.move_left();
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorRight => {
                self.rename_input.move_right(&self.rename_buffer());
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    // -- Context menu --

    fn open_context_menu(&mut self, row: RowId, x: f32, y: f32, panel: Rect, data: &FileTreeData) {
        let items = context_menu_items(row, data);
        let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
        let count = labels.len();
        self.context_menu = Some(ContextMenu {
            anchor: (
                x.min(panel.x + panel.width - 200.0),
                y.min(panel.y + panel.height - (count as f32 + 0.5) * 30.0 - 8.0),
            ),
            index: 0,
            submenu: None,
            submenu_index: 0,
            target: row,
            labels,
            submenu_labels: Vec::new(),
        });
    }

    fn handle_menu_keyboard(
        &mut self,
        event: &UiEvent,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        let menu = self.context_menu.as_ref()?;
        let target = menu.target;
        let items = context_menu_items(target, data);
        let count = items.len();
        let (menu_index, submenu_kind, submenu_index) = {
            let menu = self.context_menu.as_ref().unwrap();
            (
                menu.index,
                menu.submenu.as_ref().map(|s| s.kind),
                menu.submenu_index,
            )
        };

        if let Some(kind) = submenu_kind {
            let submenu_count = submenu_items(kind, data, target).len();
            let label = |index: usize| {
                submenu_items(kind, data, target)
                    .get(index)
                    .map(|item| item.label.clone())
                    .unwrap_or_default()
            };
            if keyboard_activation(event) {
                let action = self.activate_submenu(kind, submenu_index, data, target);
                return Some(action);
            }
            match event {
                UiEvent::CursorUp | UiEvent::FocusPrevious => {
                    let next = submenu_index.saturating_sub(1);
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.submenu_index = next;
                    }
                    return Some(selection_event(label(next)));
                }
                UiEvent::CursorDown | UiEvent::FocusNext => {
                    let next = (submenu_index + 1).min(submenu_count.saturating_sub(1));
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.submenu_index = next;
                    }
                    return Some(selection_event(label(next)));
                }
                UiEvent::KeyInput { text } if text == "\x1b" => {
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.submenu = None;
                        menu.submenu_index = 0;
                    }
                    return Some(collapse_event());
                }
                _ => return Some(EventResponse::Consumed),
            }
        }

        let label = |index: usize| items.get(index).map(|i| i.label.clone()).unwrap_or_default();
        if keyboard_activation(event) {
            let action = self.activate_menu_item(menu_index, data, target);
            return Some(action);
        }
        match event {
            UiEvent::CursorUp | UiEvent::FocusPrevious => {
                let next = menu_index.saturating_sub(1);
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.index = next;
                }
                Some(selection_event(label(next)))
            }
            UiEvent::CursorDown | UiEvent::FocusNext => {
                let next = (menu_index + 1).min(count.saturating_sub(1));
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.index = next;
                }
                Some(selection_event(label(next)))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.context_menu = None;
                Some(collapse_event())
            }
            UiEvent::CursorRight => {
                if let Some(kind) = items.get(menu_index).and_then(|item| item.submenu) {
                    let anchor = self.context_menu.as_ref().unwrap().anchor;
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.submenu = Some(Submenu {
                            kind,
                            anchor: (anchor.0 + 180.0, anchor.1 + menu_index as f32 * 30.0),
                        });
                        menu.submenu_index = 0;
                    }
                    let first = submenu_items(kind, data, target)
                        .first()
                        .map(|item| item.label.clone())
                        .unwrap_or_default();
                    Some(selection_event(first))
                } else {
                    Some(EventResponse::Consumed)
                }
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    fn activate_menu_item(
        &mut self,
        index: usize,
        data: &FileTreeData,
        target: RowId,
    ) -> EventResponse {
        let items = context_menu_items(target, data);
        let Some(item) = items.get(index) else {
            self.context_menu = None;
            return EventResponse::Consumed;
        };
        if item.submenu.is_some() {
            if let Some(kind) = item.submenu {
                let anchor = self.context_menu.as_ref().map(|m| m.anchor).unwrap_or((0.0, 0.0));
                let submenu_labels: Vec<String> = submenu_items(kind, data, target)
                    .iter()
                    .map(|item| item.label.clone())
                    .collect();
                let existing_labels = self
                    .context_menu
                    .as_ref()
                    .map(|menu| menu.labels.clone())
                    .unwrap_or_default();
                self.context_menu = Some(ContextMenu {
                    anchor,
                    index,
                    submenu: Some(Submenu {
                        kind,
                        anchor: (anchor.0 + 180.0, anchor.1 + index as f32 * 30.0),
                    }),
                    submenu_index: 0,
                    target,
                    labels: existing_labels,
                    submenu_labels,
                });
            }
            return EventResponse::Consumed;
        }
        self.context_menu = None;
        if item.enabled {
            (item.action)()
        } else {
            EventResponse::Consumed
        }
    }

    fn activate_submenu(
        &mut self,
        kind: SubmenuKind,
        index: usize,
        data: &FileTreeData,
        target: RowId,
    ) -> EventResponse {
        let items = submenu_items(kind, data, target);
        self.context_menu = None;
        let Some(item) = items.get(index) else {
            return EventResponse::Consumed;
        };
        if item.enabled {
            (item.action)()
        } else {
            EventResponse::Consumed
        }
    }

    fn handle_menu_pointer(
        &mut self,
        event: &UiEvent,
        panel: Rect,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        let menu = self.context_menu.as_ref()?;
        let target = menu.target;
        let (x, y) = match event {
            UiEvent::MousePress { x, y } => (*x, *y),
            _ => return None,
        };
        let items = context_menu_items(target, data);
        let menu_rect = Rect {
            x: menu.anchor.0,
            y: menu.anchor.1,
            width: 190.0,
            height: items.len() as f32 * 30.0,
        };

        if let Some(submenu) = &menu.submenu {
            let submenu_items = submenu_items(submenu.kind, data, target);
            let submenu_rect = Rect {
                x: submenu.anchor.0,
                y: submenu.anchor.1,
                width: 200.0,
                height: submenu_items.len() as f32 * 30.0,
            };
            if submenu_rect.contains(x, y) {
                let index = ((y - submenu_rect.y) / 30.0) as usize;
                let kind = submenu.kind;
                self.context_menu = None;
                return Some(self.activate_submenu(kind, index, data, target));
            }
        }

        if menu_rect.contains(x, y) {
            let index = ((y - menu_rect.y) / 30.0) as usize;
            return Some(self.activate_menu_item(index, data, target));
        }
        // Click outside closes.
        let _ = panel;
        self.context_menu = None;
        Some(EventResponse::Consumed)
    }

    // -- Drag & drop --

    fn drop_drag(
        &mut self,
        id: RowId,
        x: f32,
        y: f32,
        panel: Rect,
        data: &FileTreeData,
    ) -> EventResponse {
        // Drop on a band row while dragging an audio = set instrumental.
        if let RowId::Audio(AudioRowId::Media(audio_id)) = id {
            if let Some(RowId::Band(band_id)) = self.row_at(panel, x, y, data) {
                let Some(audio) = data
                    .audios
                    .iter()
                    .find(|audio| audio.id == AudioRowId::Media(audio_id))
                else {
                    return EventResponse::Consumed;
                };
                return EventResponse::Action(UiAction::SetLanguageInstrumentalAudioPath {
                    id: band_id,
                    path: audio.path.clone(),
                });
            }
        }
        // Drop a video on a video = associate as proxy.
        if let RowId::Video(proxy_id) = id {
            if let Some(RowId::Video(source_id)) = self.row_at(panel, x, y, data) {
                if source_id != proxy_id {
                    return EventResponse::Action(UiAction::MediaVideoAssociateProxy {
                        proxy_id,
                        source_id,
                    });
                }
            }
            // Otherwise: reorder within the videos group.
            if let Some(index) = self.insertion_index(panel, y, data) {
                return EventResponse::Action(UiAction::MediaReorderVideo {
                    id: proxy_id,
                    to_index: index,
                });
            }
        }
        if let RowId::Audio(AudioRowId::Media(audio_id)) = id {
            if let Some(index) = self.audio_insertion_index(panel, y, data) {
                return EventResponse::Action(UiAction::MediaReorderAudio {
                    id: audio_id,
                    to_index: index,
                });
            }
        }
        if let RowId::Band(band_id) = id {
            if let Some(index) = self.band_insertion_index(panel, y, data) {
                return EventResponse::Action(UiAction::LanguageReorder {
                    id: band_id,
                    to_index: index,
                });
            }
        }
        EventResponse::Consumed
    }

    fn insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let rows = flatten(data, &self.expanded);
        let top = self.top_level_row(&rows, RowId::Group(GroupKind::Videos))?;
        let body = body_rect(panel);
        let row_index = ((y - body.y) / ROW_H).floor() as i64 + self.scroll as i64;
        let videos: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row.id, RowId::Video(_)) && row.depth == 2)
            .map(|(index, _)| index)
            .collect();
        let _ = top;
        let count = videos.len();
        if count == 0 {
            return Some(0);
        }
        Some(row_index.clamp(0, count as i64) as usize)
    }

    fn audio_insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let audios = data.audios.len();
        if audios == 0 {
            return Some(0);
        }
        let body = body_rect(panel);
        let row_index = ((y - body.y) / ROW_H).floor() as i64 + self.scroll as i64;
        // +1 to account for the virtual original-video row.
        Some((row_index - 1).clamp(0, audios as i64) as usize)
    }

    fn band_insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let bands = data.bands.len();
        if bands == 0 {
            return Some(0);
        }
        let body = body_rect(panel);
        let row_index = ((y - body.y) / ROW_H).floor() as i64 + self.scroll as i64;
        Some(row_index.clamp(0, bands as i64) as usize)
    }

    fn top_level_row(&self, rows: &[Row], id: RowId) -> Option<usize> {
        rows.iter().position(|row| row.id == id)
    }

    fn auto_scroll_during_drag(&mut self, panel: Rect) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        let edge = 30.0;
        if drag.current.1 < body_rect(panel).y + edge {
            self.scroll = self.scroll.saturating_sub(1);
        } else if drag.current.1 > panel.y + panel.height - edge {
            self.scroll = self.scroll + 1;
        }
    }

    // -- Helpers --

    fn ensure_focused_visible(&mut self, panel: Rect, data: &FileTreeData) {
        let rows = flatten(data, &self.expanded);
        let Some(index) = self
            .focused
            .and_then(|id| rows.iter().position(|row| row.id == id))
        else {
            return;
        };
        let visible = visible_rows(panel);
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + visible {
            self.scroll = index + 1 - visible;
        }
    }

    fn row_at(&self, panel: Rect, x: f32, y: f32, data: &FileTreeData) -> Option<RowId> {
        let body = body_rect(panel);
        if !body.contains(x, y) {
            return None;
        }
        let rows = flatten(data, &self.expanded);
        let index = ((y - body.y) / ROW_H) as usize + self.scroll;
        rows.get(index).map(|row| row.id)
    }

    fn row_y(&self, panel: Rect, index: Option<usize>, _data: &FileTreeData) -> Option<f32> {
        let index = index?;
        Some(body_rect(panel).y + index as f32 * ROW_H)
    }

    fn row_screen_position(&self, panel: Rect, id: RowId, data: &FileTreeData) -> Option<(f32, f32)> {
        let rows = flatten(data, &self.expanded);
        let index = rows.iter().position(|row| row.id == id)?;
        Some((
            panel.x + 12.0,
            body_rect(panel).y + (index.saturating_sub(self.scroll)) as f32 * ROW_H,
        ))
    }

    fn row_label<'a>(&self, row: RowId, data: &'a FileTreeData) -> &'a str {
        match row {
            RowId::Root => &data.root_name,
            RowId::Group(kind) => group_label(kind),
            RowId::Video(id) => data
                .videos
                .iter()
                .find(|video| video.id == id)
                .map(|video| video.name.as_str())
                .unwrap_or(""),
            RowId::Audio(AudioRowId::OriginalVideo) => t("file_tree.original_audio"),
            RowId::Audio(AudioRowId::Media(id)) => data
                .audios
                .iter()
                .find(|audio| audio.id == AudioRowId::Media(id))
                .map(|audio| audio.name.as_str())
                .unwrap_or(""),
            RowId::Band(id) => data
                .bands
                .iter()
                .find(|band| band.id == id)
                .map(|band| band.name.as_str())
                .unwrap_or(""),
        }
    }

    fn focus_event(&self, data: &FileTreeData) -> EventResponse {
        let Some(id) = self.focused else {
            return EventResponse::Consumed;
        };
        let label = self.row_label(id, data).to_string();
        EventResponse::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Focus {
                label,
                role: "tree item".to_string(),
            },
        ))
    }

    fn expand_event(&self, kind: GroupKind) -> EventResponse {
        EventResponse::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::ValueChanged {
                label: group_label(kind).to_string(),
                value: if self.expanded.get(kind) {
                    t("accessibility.expanded").to_string()
                } else {
                    t("accessibility.collapsed").to_string()
                },
            },
        ))
    }

    // -- Rendering --

    pub fn render<'a>(
        &'a self,
        panel: Rect,
        data: &'a FileTreeData,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        if !self.open {
            return;
        }
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
            &data.root_name,
            Rect {
                x: panel.x + PAD,
                y: panel.y,
                width: panel.width - 2.0 * PAD,
                height: HEADER_H,
            },
            HAlign::Left,
            15.0,
            [238, 239, 245],
        ));

        let rows = flatten(data, &self.expanded);
        let visible = visible_rows(panel);
        let body = body_rect(panel);

        // Sliding hover pill (rendered beneath rows).
        if let Some(pill) = self.hover_pill {
            if self.hover_pill_fade > 0.01 {
                let mut color = [0.16, 0.17, 0.21, 1.0];
                color[3] *= self.hover_pill_fade;
                let pill_rect = Rect {
                    x: body.x + 6.0,
                    y: pill.value,
                    width: body.width - 12.0 - SCROLLBAR_W,
                    height: ROW_H,
                };
                solid(quads, pill_rect, color, [0.0; 4], 10.0);
            }
        }

        for (i, row) in rows.iter().skip(self.scroll).take(visible).enumerate() {
            let y = body.y + i as f32 * ROW_H;
            self.render_row(row, y, panel, data, quads, labels);
        }

        // Scrollbar.
        if rows.len() > visible {
            let track = Rect {
                x: panel.x + panel.width - 10.0,
                y: body.y + 6.0,
                width: SCROLLBAR_W,
                height: (body.height - 12.0).max(28.0),
            };
            let thumb_h = (track.height * visible as f32 / rows.len() as f32)
                .clamp(28.0, track.height);
            let travel = track.height - thumb_h;
            let max_scroll = rows.len() - visible;
            let thumb = Rect {
                x: track.x,
                y: track.y + travel * self.scroll as f32 / max_scroll as f32,
                width: track.width,
                height: thumb_h,
            };
            solid(quads, track, [0.10, 0.103, 0.125, 1.0], [0.0; 4], 2.0);
            solid(quads, thumb, [0.31, 0.33, 0.42, 1.0], [0.0; 4], 2.0);
        }

        // Drag ghost (follows the cursor).
        if let Some(drag) = &self.drag {
            if drag.is_past_threshold() {
                let ghost = Rect {
                    x: drag.current.0 + 8.0,
                    y: drag.current.1 - ROW_H / 2.0,
                    width: 160.0,
                    height: ROW_H,
                };
                solid(
                    quads,
                    ghost,
                    [0.13, 0.15, 0.22, 0.95],
                    [0.42, 0.48, 0.92, 1.0],
                    6.0,
                );
                labels.push(label(
                    &drag.label,
                    ghost,
                    HAlign::Left,
                    13.0,
                    [235, 238, 246],
                ));
            }
        }
    }

    fn render_row<'a>(
        &'a self,
        row: &Row,
        y: f32,
        panel: Rect,
        data: &'a FileTreeData,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let body = body_rect(panel);
        let indent = row.depth as f32 * INDENT;
        let selected = self.selected == Some(row.id);
        let focused = self.focused == Some(row.id);

        if selected {
            let mut color = [0.12, 0.14, 0.23, 1.0];
            color[3] = 0.9;
            solid(
                quads,
                Rect {
                    x: body.x + 4.0,
                    y,
                    width: body.width - 8.0 - SCROLLBAR_W,
                    height: ROW_H,
                },
                color,
                [0.0; 4],
                0.0,
            );
        }
        if focused {
            solid(
                quads,
                Rect {
                    x: body.x + 2.0,
                    y: y + 2.0,
                    width: body.width - 4.0 - SCROLLBAR_W,
                    height: ROW_H - 4.0,
                },
                [0.0; 4],
                [0.38, 0.58, 0.96, 1.0],
                4.0,
            );
        }

        let mut icon_x = body.x + PAD + indent;
        match row.id {
            RowId::Root => {
                labels.push(label(
                    &data.root_name,
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 30.0,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    14.0,
                    [235, 238, 246],
                ));
            }
            RowId::Group(kind) => {
                // Chevron (animated rotation handled via progress).
                let chevron_progress = self
                    .chevron
                    .get(&kind)
                    .map(|spring| spring.value / 90.0)
                    .unwrap_or(if self.expanded.get(kind) { 1.0 } else { 0.0 })
                    .clamp(0.0, 1.0);
                labels.push(label(
                    if chevron_progress > 0.5 { "▾" } else { "▸" },
                    Rect {
                        x: icon_x,
                        y,
                        width: CHEVRON_W,
                        height: ROW_H,
                    },
                    HAlign::Center,
                    12.0,
                    [126, 132, 154],
                ));
                icon_x += CHEVRON_W + 6.0;
                labels.push(label(
                    group_label(kind),
                    Rect {
                        x: icon_x + ICON_SIZE + 6.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 40.0,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.5,
                    [200, 204, 218],
                ));
                // Quick-action "+" on hover.
                if self.hover == Some(row.id) {
                    labels.push(label(
                        "+",
                        Rect {
                            x: body.x + body.width - QUICK_ACTION_SIZE - 14.0,
                            y,
                            width: QUICK_ACTION_SIZE,
                            height: ROW_H,
                        },
                        HAlign::Center,
                        16.0,
                        [170, 175, 195],
                    ));
                }
            }
            RowId::Video(id) => {
                let Some(video) = data.videos.iter().find(|v| v.id == id) else {
                    return;
                };
                let is_child = row.depth == 3;
                if is_child {
                    // Branch line.
                    let line_x = body.x + PAD + (row.depth - 1) as f32 * INDENT - 4.0;
                    solid(
                        quads,
                        Rect {
                            x: line_x,
                            y,
                            width: 1.5,
                            height: ROW_H,
                        },
                        [0.30, 0.32, 0.40, 0.8],
                        [0.0; 4],
                        0.0,
                    );
                }
                labels.push(label(
                    "🎬",
                    Rect {
                        x: icon_x,
                        y,
                        width: ICON_SIZE,
                        height: ROW_H,
                    },
                    HAlign::Center,
                    12.0,
                    if video.missing {
                        [220, 170, 80]
                    } else {
                        [150, 155, 175]
                    },
                ));
                let text_color: [u8; 3] = if video.active {
                    [130, 180, 255]
                } else if video.missing {
                    [220, 170, 80]
                } else {
                    [222, 225, 235]
                };
                let name: &str = match &self.rename {
                    Some((RenameTarget::Video(rid), buffer)) if *rid == id => buffer.as_str(),
                    _ => video.name.as_str(),
                };
                labels.push(label(
                    name,
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 90.0,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.0,
                    text_color,
                ));
                // Badges (right-aligned).
                self.render_badges(video, y, body, labels);
            }
            RowId::Audio(audio_id) => {
                let Some(audio) = data.audios.iter().find(|a| a.id == audio_id) else {
                    return;
                };
                let is_original = audio.media_id.is_none();
                labels.push(label(
                    "🎵",
                    Rect {
                        x: icon_x,
                        y,
                        width: ICON_SIZE,
                        height: ROW_H,
                    },
                    HAlign::Center,
                    12.0,
                    [150, 155, 175],
                ));
                let name: &str = if is_original {
                    t("file_tree.original_audio")
                } else {
                    audio.name.as_str()
                };
                let text_color: [u8; 3] = if is_original {
                    [150, 154, 170]
                } else {
                    [222, 225, 235]
                };
                labels.push(label(
                    name,
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 90.0,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.0,
                    text_color,
                ));
                if !audio.instrumental_of.is_empty() {
                    labels.push(label(
                        audio.instrumental_badge.as_str(),
                        Rect {
                            x: body.x + body.width - 150.0 - SCROLLBAR_W,
                            y,
                            width: 150.0,
                            height: ROW_H,
                        },
                        HAlign::Right,
                        10.0,
                        [140, 165, 230],
                    ));
                }
            }
            RowId::Band(band_id) => {
                let Some(band) = data.bands.iter().find(|b| b.id == band_id) else {
                    return;
                };
                labels.push(label(
                    "🎞",
                    Rect {
                        x: icon_x,
                        y,
                        width: ICON_SIZE,
                        height: ROW_H,
                    },
                    HAlign::Center,
                    12.0,
                    [150, 155, 175],
                ));
                let name: &str = match &self.rename {
                    Some((RenameTarget::Band(rid), buffer)) if *rid == band_id => buffer.as_str(),
                    _ => band.name.as_str(),
                };
                labels.push(label(
                    name,
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 90.0,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.0,
                    if band.active {
                        [130, 180, 255]
                    } else {
                        [222, 225, 235]
                    },
                ));
            }
        }
    }

    fn render_badges<'a>(
        &self,
        video: &'a VideoData,
        y: f32,
        body: Rect,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let mut right = body.x + body.width - SCROLLBAR_W - 14.0;
        let badge = |labels: &mut Vec<LabelInfo<'a>>, text: &'a str, color: [u8; 3], x: f32| {
            labels.push(LabelInfo {
                text,
                bounds: Rect {
                    x: x - 60.0,
                    y,
                    width: 60.0,
                    height: ROW_H,
                },
                h_align: HAlign::Right,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 4.0,
                font_size_override: Some(10.0),
                color_override: Some(color),
                font_family_override: None,
            });
        };
        if video.proxy_of.is_some() {
            badge(labels, t("file_tree.badges.proxy"), [140, 165, 230], right);
            right -= 64.0;
        }
        if video.is_default {
            badge(labels, t("file_tree.badges.default"), [170, 230, 170], right);
            right -= 64.0;
        }
        if video.is_proxy_source {
            badge(labels, t("file_tree.badges.has_proxy"), [140, 165, 230], right);
            right -= 64.0;
        }
    }

    /// Contextual menu overlay (rendered on the modal layer).
    pub fn render_menus<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let menu_rect = Rect {
            x: menu.anchor.0,
            y: menu.anchor.1,
            width: 190.0,
            height: menu.labels.len() as f32 * 30.0,
        };
        solid(
            quads,
            menu_rect,
            [0.13, 0.135, 0.16, 1.0],
            [0.34, 0.35, 0.42, 1.0],
            5.0,
        );
        for (index, item_label) in menu.labels.iter().enumerate() {
            let item_rect = Rect {
                x: menu_rect.x,
                y: menu_rect.y + index as f32 * 30.0,
                width: menu_rect.width,
                height: 30.0,
            };
            if index == menu.index {
                solid(quads, item_rect, [0.16, 0.19, 0.30, 1.0], [0.0; 4], 3.0);
            }
            labels.push(label(
                item_label,
                Rect {
                    x: item_rect.x + 10.0,
                    width: item_rect.width - 20.0,
                    ..item_rect
                },
                HAlign::Left,
                13.0,
                [232, 234, 242],
            ));
        }

        if let Some(submenu) = &menu.submenu {
            let submenu_rect = Rect {
                x: submenu.anchor.0,
                y: submenu.anchor.1,
                width: 200.0,
                height: menu.submenu_labels.len() as f32 * 30.0,
            };
            solid(
                quads,
                submenu_rect,
                [0.14, 0.145, 0.17, 1.0],
                [0.34, 0.35, 0.42, 1.0],
                5.0,
            );
            for (index, item_label) in menu.submenu_labels.iter().enumerate() {
                let item_rect = Rect {
                    x: submenu_rect.x,
                    y: submenu_rect.y + index as f32 * 30.0,
                    width: submenu_rect.width,
                    height: 30.0,
                };
                if index == menu.submenu_index {
                    solid(quads, item_rect, [0.16, 0.19, 0.30, 1.0], [0.0; 4], 3.0);
                }
                labels.push(label(
                    item_label,
                    Rect {
                        x: item_rect.x + 10.0,
                        width: item_rect.width - 20.0,
                        ..item_rect
                    },
                    HAlign::Left,
                    13.0,
                    [232, 234, 242],
                ));
            }
        }
    }
}

fn library_audio_path(audio: &AudioData) -> Option<String> {
    (!audio.path.is_empty()).then(|| audio.path.clone())
}

pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    pub submenu: Option<SubmenuKind>,
    pub action: Box<dyn Fn() -> EventResponse>,
}

fn context_menu_items(row: RowId, data: &FileTreeData) -> Vec<MenuItem> {
    match row {
        RowId::Video(id) => {
            let Some(video) = data.videos.iter().find(|v| v.id == id) else {
                return Vec::new();
            };
            let is_proxy = video.proxy_of.is_some();
            let mut items = Vec::new();
            items.push(MenuItem {
                label: t("file_tree.menu.use").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::MediaVideoUse { id })
                }),
            });
            items.push(MenuItem {
                label: t("file_tree.menu.make_default").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::MediaVideoSetDefault { id })
                }),
            });
            if is_proxy {
                items.push(MenuItem {
                    label: t("file_tree.menu.dissociate_proxy").to_string(),
                    enabled: true,
                    submenu: None,
                    action: Box::new(move || {
                        EventResponse::Action(UiAction::MediaVideoDissociateProxy { id })
                    }),
                });
            } else {
                let has_proxy = data
                    .videos
                    .iter()
                    .any(|v| v.proxy_of == Some(id));
                items.push(MenuItem {
                    label: t(if has_proxy {
                        "file_tree.menu.recreate_proxy"
                    } else {
                        "file_tree.menu.create_proxy"
                    })
                    .to_string(),
                    enabled: true,
                    submenu: None,
                    action: Box::new(move || {
                        EventResponse::Action(UiAction::MediaVideoCreateProxy { id })
                    }),
                });
                items.push(MenuItem {
                    label: format!("{} ▸", t("file_tree.menu.associate_proxy")),
                    enabled: true,
                    submenu: Some(SubmenuKind::AssociateProxyTo),
                    action: Box::new(|| EventResponse::Consumed),
                });
            }
            items.push(MenuItem {
                label: t("file_tree.menu.rename").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    // Renames are started by the tree, not the dispatcher;
                    // emit a marker the shell intercepts.
                    EventResponse::Action(UiAction::MediaVideoBeginRename { id })
                }),
            });
            items.push(MenuItem {
                label: t("file_tree.menu.remove").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::MediaVideoRemove { id })
                }),
            });
            items
        }
        RowId::Audio(AudioRowId::Media(id)) => vec![
            MenuItem {
                label: t("file_tree.menu.remove").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::MediaAudioRemove { id })
                }),
            },
            MenuItem {
                label: t("file_tree.menu.rename").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::MediaAudioBeginRename { id })
                }),
            },
        ],
        RowId::Audio(AudioRowId::OriginalVideo) => Vec::new(),
        RowId::Band(id) => {
            let current_syllable = data
                .bands
                .iter()
                .find(|band| band.id == id)
                .map(|band| band.syllable_language);
            let current_instrumental = data
                .bands
                .iter()
                .find(|band| band.id == id)
                .and_then(|band| band.instrumental_audio_path.clone());
            let mut items = vec![MenuItem {
                label: t("file_tree.menu.rename").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::LanguageBeginRename { id })
                }),
            }];
            items.push(MenuItem {
                label: format!("{} ▸", t("file_tree.menu.set_syllable_language")),
                enabled: true,
                submenu: Some(SubmenuKind::SyllableLanguage),
                action: Box::new(|| EventResponse::Consumed),
            });
            let _ = current_syllable;
            items.push(MenuItem {
                label: format!("{} ▸", t("file_tree.menu.set_instrumental")),
                enabled: true,
                submenu: Some(SubmenuKind::Instrumental),
                action: Box::new(|| EventResponse::Consumed),
            });
            let _ = current_instrumental;
            items.push(MenuItem {
                label: t("file_tree.menu.delete").to_string(),
                enabled: data.bands.len() > 1,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::DeleteLanguage { id })
                }),
            });
            items
        }
        _ => Vec::new(),
    }
}

fn submenu_items(
    kind: SubmenuKind,
    data: &FileTreeData,
    target: RowId,
) -> Vec<MenuItem> {
    match kind {
        SubmenuKind::AssociateProxyTo => {
            let RowId::Video(proxy_id) = target else {
                return Vec::new();
            };
            let sources: Vec<&VideoData> = data
                .videos
                .iter()
                .filter(|video| {
                    video.id != proxy_id
                        && video.proxy_of.is_none()
                        && !data.videos.iter().any(|v| v.proxy_of == Some(video.id))
                })
                .collect();
            if sources.is_empty() {
                return vec![MenuItem {
                    label: t("file_tree.menu.no_eligible_video").to_string(),
                    enabled: false,
                    submenu: None,
                    action: Box::new(|| EventResponse::Consumed),
                }];
            }
            sources
                .into_iter()
                .map(|source| {
                    let (proxy_id, source_id) = (proxy_id, source.id);
                    MenuItem {
                        label: source.name.clone(),
                        enabled: true,
                        submenu: None,
                        action: Box::new(move || {
                            EventResponse::Action(UiAction::MediaVideoAssociateProxy {
                                proxy_id,
                                source_id,
                            })
                        }),
                    }
                })
                .collect()
        }
        SubmenuKind::SyllableLanguage => {
            let RowId::Band(band_id) = target else {
                return Vec::new();
            };
            let current = data
                .bands
                .iter()
                .find(|band| band.id == band_id)
                .map(|band| band.syllable_language);
            [
                (SyllableLanguage::French, "Français"),
                (SyllableLanguage::English, "English"),
                (SyllableLanguage::Spanish, "Español"),
            ]
            .into_iter()
            .map(|(language, label)| {
                let checked = current == Some(language);
                MenuItem {
                    label: if checked {
                        format!("✓ {label}")
                    } else {
                        label.to_string()
                    },
                    enabled: true,
                    submenu: None,
                    action: Box::new(move || {
                        EventResponse::Action(UiAction::SetLanguageSyllableLanguage {
                            id: band_id,
                            language,
                        })
                    }),
                }
            })
            .collect()
        }
        SubmenuKind::Instrumental => {
            let RowId::Band(band_id) = target else {
                return Vec::new();
            };
            let current = data
                .bands
                .iter()
                .find(|band| band.id == band_id)
                .and_then(|band| band.instrumental_audio_path.clone());
            let mut items = vec![MenuItem {
                label: if current.is_none() {
                    format!("✓ {}", t("file_tree.menu.none"))
                } else {
                    t("file_tree.menu.none").to_string()
                },
                enabled: true,
                submenu: None,
                action: Box::new(move || {
                    EventResponse::Action(UiAction::SetLanguageInstrumentalAudioPath {
                        id: band_id,
                        path: String::new(),
                    })
                }),
            }];
            for audio in &data.audios {
                let Some(media_id) = audio.media_id else {
                    continue;
                };
                // Only stored audios can be instrumentals.
                if audio.media_id.is_none() {
                    continue;
                }
                let checked = current
                    .as_deref()
                    .map(|path| data_audio_matches(data, path, audio))
                    .unwrap_or(false);
                items.push(MenuItem {
                    label: if checked {
                        format!("✓ {}", audio.name)
                    } else {
                        audio.name.clone()
                    },
                    enabled: true,
                    submenu: None,
                    action: Box::new(move || {
                        EventResponse::Action(UiAction::SetLanguageInstrumentalAudioByMediaId {
                            band_id,
                            media_id,
                        })
                    }),
                });
            }
            items
        }
    }
}

fn data_audio_matches(_data: &FileTreeData, _path: &str, _audio: &AudioData) -> bool {
    false
}

fn group_label(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Videos => t("file_tree.groups.videos"),
        GroupKind::Bands => t("file_tree.groups.bands"),
        GroupKind::Audios => t("file_tree.groups.audios"),
    }
}

fn parent_group(id: RowId) -> Option<GroupKind> {
    match id {
        RowId::Video(_) => Some(GroupKind::Videos),
        RowId::Audio(_) => Some(GroupKind::Audios),
        RowId::Band(_) => Some(GroupKind::Bands),
        _ => None,
    }
}

fn row_index_of(rows: &[Row], id: RowId) -> Option<usize> {
    rows.iter().position(|row| row.id == id)
}

fn keyboard_activation(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Activate)
        || matches!(event, UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " ")
}

fn selection_event(label: String) -> EventResponse {
    EventResponse::Action(UiAction::Accessibility(
        crate::accessibility::AccessibilityEvent::Selection { label },
    ))
}

fn collapse_event() -> EventResponse {
    EventResponse::Action(UiAction::Accessibility(
        crate::accessibility::AccessibilityEvent::Collapsed {
            label: t("file_tree.title").to_string(),
        },
    ))
}

fn body_rect(p: Rect) -> Rect {
    Rect {
        x: p.x,
        y: p.y + HEADER_H,
        width: p.width,
        height: (p.height - HEADER_H).max(0.0),
    }
}

fn visible_rows(p: Rect) -> usize {
    (body_rect(p).height / ROW_H).floor().max(1.0) as usize
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
