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

use self::animation::{enter_delay, SpringValue, Tween, ENTER_DURATION, SPRING_LAYOUT};
use self::data::{paths_equal, AudioData, VideoData};
use self::rows::{flatten, AudioRowId, ExpandedSet, GroupKind, Row, RowId};

use super::primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign,
};
use super::text_input::{TextInputAction, TextInputState};

const ROW_H: f32 = 36.0;
/// Horizontal distance between a group's entries and their children. Groups
/// themselves stay aligned with the project row; entries use one level and
/// proxies use a second level.
const INDENT: f32 = 24.0;
const ICON_SIZE: f32 = 16.0;
const STATUS_ICON_SIZE: f32 = 15.0;
const STATUS_ICON_GAP: f32 = 5.0;
const PAD: f32 = 10.0;
const SCROLLBAR_W: f32 = 4.0;
const MENU_ROW_H: f32 = 30.0;
const MENU_W: f32 = 190.0;
const SUBMENU_W: f32 = 200.0;
const MENU_MARGIN: f32 = 8.0;
/// Minimum gap between an element name and its right-aligned badges.
const BADGE_TEXT_GAP: f32 = 6.0;
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

#[derive(Clone, Debug)]
struct RowRect {
    row: Row,
    index: usize,
    rect: Rect,
}

#[derive(Clone, Copy, Debug)]
enum DragFeedback {
    Target(Rect),
    Insertion { x: f32, y: f32, width: f32 },
}

#[derive(Clone, Copy, Debug)]
struct StatusIcon<'a> {
    name: &'static str,
    tooltip: &'a str,
    rect: Rect,
    color: [u8; 3],
}

pub struct FileTree {
    open: bool,
    expanded: ExpandedSet,
    selected: Option<RowId>,
    focused: Option<RowId>,
    scroll: usize,
    hover: Option<RowId>,
    hover_tooltip: Option<String>,
    /// Shared-layout hover pill: y position springs between rows.
    hover_pill: Option<SpringValue>,
    hover_pill_fade: f32,
    /// Per-row entry animations (offset-y + opacity), keyed by row id.
    enter: HashMap<RowId, Tween>,
    /// A tree can open before the caller has a snapshot to seed its entries.
    entry_pending: bool,
    /// Row positions preserve continuity across expand/collapse and reorders.
    layout_positions: HashMap<RowId, SpringValue>,
    rename: Option<(RenameTarget, String)>,
    rename_original: String,
    rename_hydrated: bool,
    rename_input: TextInputState,
    context_menu: Option<ContextMenu>,
    drag: Option<DragState>,
    scroll_drag: Option<f32>,
}

/// Contextual menu with an optional submenu (one level, "▸" style).
pub struct ContextMenu {
    pub anchor: (f32, f32),
    panel: Rect,
    pub index: usize,
    pub submenu: Option<Submenu>,
    pub submenu_index: usize,
    pub target: RowId,
    /// Labels snapshot taken when the menu opened (avoids rebuilding
    /// per-frame owned strings during render).
    pub labels: Vec<String>,
    pub enabled: Vec<bool>,
    pub submenu_labels: Vec<String>,
    pub submenu_enabled: Vec<bool>,
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
        Self {
            open: false,
            expanded: ExpandedSet::all_expanded(),
            selected: None,
            focused: None,
            scroll: 0,
            hover: None,
            hover_tooltip: None,
            hover_pill: None,
            hover_pill_fade: 0.0,
            enter: HashMap::new(),
            entry_pending: false,
            layout_positions: HashMap::new(),
            rename: None,
            rename_original: String::new(),
            rename_hydrated: false,
            rename_input: TextInputState::new(),
            context_menu: None,
            drag: None,
            scroll_drag: None,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.focused = Some(RowId::Root);
        self.enter.clear();
        self.entry_pending = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.cancel_rename();
        self.context_menu = None;
        self.drag = None;
        self.scroll_drag = None;
        self.hover = None;
        self.hover_tooltip = None;
        self.hover_pill = None;
        self.enter.clear();
        self.layout_positions.clear();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn hovered_tooltip(&self) -> Option<&str> {
        self.hover_tooltip.as_deref()
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

        if self.entry_pending {
            self.seed_entry_animations(data);
            self.entry_pending = false;
            running = !self.enter.is_empty();
        }

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

        let rows = flatten(data, &self.expanded);
        for (index, row) in rows.iter().enumerate() {
            let target = index as f32 * ROW_H;
            let position = self
                .layout_positions
                .entry(row.id)
                .or_insert_with(|| SpringValue::at(target));
            position.retarget(target);
            position.step(SPRING_LAYOUT, dt);
            if !position.settled() {
                running = true;
            }
        }
        self.layout_positions
            .retain(|id, _| rows.iter().any(|row| row.id == *id));

        running
    }

    fn seed_entry_animations(&mut self, data: &FileTreeData) {
        self.enter.clear();
        for (index, row) in flatten(data, &self.expanded).into_iter().enumerate() {
            self.enter.insert(
                row.id,
                Tween::start_delayed(ENTER_DURATION, enter_delay(index)),
            );
        }
    }

    fn toggle_group(&mut self, kind: GroupKind, data: &FileTreeData) {
        self.expanded.toggle(kind);
        self.seed_entry_animations(data);
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

    /// Return the media group under an OS file drop. Entries count as their
    /// parent group so drops remain useful when the group already has files.
    pub fn drop_target_at(
        &self,
        panel: Rect,
        x: f32,
        y: f32,
        data: &FileTreeData,
    ) -> Option<GroupKind> {
        if !self.open {
            return None;
        }
        match self.row_at(panel, x, y, data)? {
            RowId::Group(kind) => Some(kind),
            RowId::Video(_) => Some(GroupKind::Videos),
            RowId::Audio(_) => Some(GroupKind::Audios),
            RowId::Root | RowId::Band(_) => None,
        }
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
        self.clamp_scroll(panel, data);
        self.hydrate_rename(data);

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
                if self.scroll_drag.is_some() {
                    self.hover_tooltip = None;
                    self.update_scroll_drag(panel, data, *y);
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.drag.as_mut() {
                    self.hover_tooltip = None;
                    drag.current = (*x, *y);
                    self.auto_scroll_during_drag(panel, data);
                    return Some(EventResponse::Consumed);
                }
                // The tree is an overlay panel. Once the pointer leaves its
                // bounds, clear transient hover state immediately instead of
                // leaving a spring/fade running at the edge while the rest of
                // the workspace receives the same mouse event.
                if !panel.contains(*x, *y) {
                    self.hover = None;
                    self.hover_tooltip = None;
                    self.hover_pill = None;
                    self.hover_pill_fade = 0.0;
                    return None;
                }

                self.hover_tooltip = self.status_tooltip_at(panel, *x, *y, data);
                let hovered = self.row_at(panel, *x, *y, data);
                if hovered != self.hover {
                    self.hover = hovered;
                    if let Some(y) = hovered.and_then(|id| {
                        self.row_rects(panel, data)
                            .into_iter()
                            .find(|row| row.row.id == id)
                            .map(|row| row.rect.y)
                    }) {
                        let pill = self.hover_pill.get_or_insert(SpringValue::at(y));
                        pill.retarget(y);
                    } else {
                        self.hover_pill = None;
                        self.hover_pill_fade = 0.0;
                    }
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } => {
                if !panel.contains(*x, *y) {
                    return None;
                }
                if self.rename.is_some() {
                    return Some(self.finish_rename(data));
                }
                if self.begin_scroll_drag(panel, data, *x, *y) {
                    return Some(EventResponse::Consumed);
                }
                let Some(row) = self.row_at(panel, *x, *y, data) else {
                    return Some(EventResponse::Consumed);
                };
                self.selected = Some(row);
                self.focused = Some(row);
                match row {
                    RowId::Group(kind) => {
                        self.toggle_group(kind, data);
                        Some(self.expand_event(kind, data))
                    }
                    RowId::Root => Some(EventResponse::Consumed),
                    RowId::Audio(AudioRowId::OriginalVideo) => Some(EventResponse::Consumed),
                    _ => {
                        // Arm a potential drag (element rows only).
                        self.drag = Some(DragState {
                            id: row,
                            label: self.row_label(row, data).to_string(),
                            origin: (*x, *y),
                            current: (*x, *y),
                        });
                        if matches!(row, RowId::Band(_)) {
                            Some(self.activate_row(row, data))
                        } else {
                            Some(EventResponse::Consumed)
                        }
                    }
                }
            }
            UiEvent::MouseRelease { x, y } => {
                if self.scroll_drag.take().is_some() {
                    return Some(EventResponse::Consumed);
                }
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
                if self.rename.is_some() {
                    return Some(self.finish_rename(data));
                }
                self.selected = Some(row);
                self.focused = Some(row);
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
            // `UiEvent` is intentionally backend-neutral. Platforms that can
            // surface F2 may pass this named key through `KeyInput`.
            UiEvent::KeyInput { text } if text == "F2" => self
                .focused
                .map(|id| self.start_rename_for_row(id, data))
                .unwrap_or(EventResponse::Consumed),
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
                        self.toggle_group(kind, data);
                        return self.expand_event(kind, data);
                    }
                } else if self.focused.is_some() {
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
                        self.toggle_group(kind, data);
                        return self.expand_event(kind, data);
                    }
                } else {
                    let parent = index.and_then(|index| {
                        (rows[index].depth == 3)
                            .then(|| {
                                rows[..index]
                                    .iter()
                                    .rev()
                                    .find(|row| row.depth == 2 && matches!(row.id, RowId::Video(_)))
                                    .map(|row| row.id)
                            })
                            .flatten()
                    });
                    if let Some(parent) = parent {
                        self.focused = Some(parent);
                    } else if let Some(kind) = self.focused.and_then(parent_group) {
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
                self.activate_row(id, data)
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
                    .unwrap_or((panel.x + 12.0, panel.y + MENU_MARGIN));
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
                self.toggle_group(kind, data);
                self.expand_event(kind, data)
            }
            RowId::Root => EventResponse::Consumed,
            RowId::Video(media_id) => {
                EventResponse::Action(UiAction::MediaVideoUse { id: media_id })
            }
            RowId::Audio(AudioRowId::OriginalVideo) => EventResponse::Consumed,
            RowId::Audio(AudioRowId::Media(media_id)) => {
                // Double-click on an audio: nothing special (single-clic selects).
                let _ = media_id;
                EventResponse::Consumed
            }
            RowId::Band(band_id) => {
                // Spec exception: clicking a band loads it immediately.
                if band_id
                    == data
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

    fn start_rename_for_row(&mut self, row: RowId, data: &FileTreeData) -> EventResponse {
        let target = match row {
            RowId::Video(id) => RenameTarget::Video(id),
            RowId::Audio(AudioRowId::Media(id)) => RenameTarget::Audio(id),
            RowId::Band(id) => RenameTarget::Band(id),
            RowId::Root | RowId::Group(_) | RowId::Audio(AudioRowId::OriginalVideo) => {
                return EventResponse::Consumed;
            }
        };
        let value = self.row_label(row, data).to_string();
        self.start_rename(target, &value);
        EventResponse::Consumed
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
        self.rename = Some((target, value.to_string()));
        self.rename_original = value.to_string();
        self.rename_hydrated = !value.is_empty();
        self.rename_input.activate(value);
        self.drag = None;
    }

    fn start_rename(&mut self, target: RenameTarget, value: &str) {
        self.rename = Some((target, value.to_string()));
        self.rename_original = value.to_string();
        self.rename_hydrated = true;
        self.rename_input.activate(value);
        self.drag = None;
    }

    fn cancel_rename(&mut self) {
        self.rename = None;
        self.rename_original.clear();
        self.rename_hydrated = false;
        self.rename_input.deactivate();
    }

    fn hydrate_rename(&mut self, data: &FileTreeData) {
        if self.rename_hydrated {
            return;
        }
        let Some((target, _)) = self.rename.clone() else {
            return;
        };
        let value = match target {
            RenameTarget::Video(id) => data.video(id).map(|video| video.name.as_str()),
            RenameTarget::Audio(id) => data
                .audio(AudioRowId::Media(id))
                .map(|audio| audio.name.as_str()),
            RenameTarget::Band(id) => data
                .bands
                .iter()
                .find(|band| band.id == id)
                .map(|band| band.name.as_str()),
        }
        .unwrap_or_default()
        .to_string();
        if let Some((_, buffer)) = self.rename.as_mut() {
            *buffer = value.clone();
        }
        self.rename_original = value.clone();
        self.rename_hydrated = true;
        self.rename_input.activate(&value);
    }

    fn finish_rename(&mut self, data: &FileTreeData) -> EventResponse {
        self.hydrate_rename(data);
        let Some((current_target, buffer)) = self.rename.take() else {
            return EventResponse::Consumed;
        };
        let value = buffer.trim().to_string();
        self.rename_input.deactivate();
        self.rename_hydrated = false;
        if value.is_empty() || value == self.rename_original {
            return EventResponse::Consumed;
        }
        if self.rename_value_is_duplicate(data, &current_target, &value) {
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
                EventResponse::Action(UiAction::RenameLanguage { id, name: value })
            }
        }
    }

    fn rename_value_is_duplicate(
        &self,
        data: &FileTreeData,
        target: &RenameTarget,
        value: &str,
    ) -> bool {
        match target {
            RenameTarget::Video(id) => data
                .videos
                .iter()
                .any(|video| video.id != *id && video.name.eq_ignore_ascii_case(value)),
            RenameTarget::Audio(id) => data
                .audios
                .iter()
                .any(|audio| audio.media_id != Some(*id) && audio.name.eq_ignore_ascii_case(value)),
            RenameTarget::Band(id) => data
                .bands
                .iter()
                .any(|band| band.id != *id && band.name.eq_ignore_ascii_case(value)),
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
        self.rename.as_ref()?;
        if !is_keyboard_event(event) {
            return None;
        }
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
                Some(self.finish_rename(data))
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
        if items.is_empty() {
            self.context_menu = None;
            return;
        }
        let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
        let enabled: Vec<bool> = items.iter().map(|item| item.enabled).collect();
        let count = labels.len();
        self.context_menu = Some(ContextMenu {
            anchor: clamped_menu_origin(panel, x, y, MENU_W, count as f32 * MENU_ROW_H),
            panel,
            index: 0,
            submenu: None,
            submenu_index: 0,
            target: row,
            labels,
            enabled,
            submenu_labels: Vec::new(),
            submenu_enabled: Vec::new(),
        });
    }

    fn open_submenu(
        &mut self,
        kind: SubmenuKind,
        index: usize,
        data: &FileTreeData,
        target: RowId,
    ) {
        let items = submenu_items(kind, data, target);
        let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
        let enabled: Vec<bool> = items.iter().map(|item| item.enabled).collect();
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let anchor = (
            menu.anchor.0 + MENU_W + MENU_MARGIN,
            menu.anchor.1 + index as f32 * MENU_ROW_H,
        );
        menu.submenu = Some(Submenu { kind, anchor });
        menu.submenu_index = 0;
        menu.submenu_labels = labels;
        menu.submenu_enabled = enabled;
    }

    fn handle_menu_keyboard(
        &mut self,
        event: &UiEvent,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        let menu = self.context_menu.as_ref()?;
        if !is_keyboard_event(event) {
            return None;
        }
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

        let label = |index: usize| {
            items
                .get(index)
                .map(|i| i.label.clone())
                .unwrap_or_default()
        };
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
                    let first = submenu_items(kind, data, target)
                        .first()
                        .map(|item| item.label.clone())
                        .unwrap_or_default();
                    self.open_submenu(kind, menu_index, data, target);
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
                self.open_submenu(kind, index, data, target);
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
        _panel: Rect,
        data: &FileTreeData,
    ) -> Option<EventResponse> {
        let (target, anchor, submenu) = {
            let menu = self.context_menu.as_ref()?;
            (
                menu.target,
                menu.anchor,
                menu.submenu
                    .as_ref()
                    .map(|submenu| (submenu.kind, submenu.anchor)),
            )
        };
        let items = context_menu_items(target, data);
        let menu_rect = Rect {
            x: anchor.0,
            y: anchor.1,
            width: MENU_W,
            height: items.len() as f32 * MENU_ROW_H,
        };

        let submenu_rect = submenu.map(|(kind, anchor)| {
            let item_count = submenu_items(kind, data, target).len();
            let submenu_rect = Rect {
                x: anchor.0,
                y: anchor.1,
                width: SUBMENU_W,
                height: item_count as f32 * MENU_ROW_H,
            };
            (kind, submenu_rect)
        });

        match event {
            UiEvent::MouseMove { x, y } => {
                if let Some((_, submenu_rect)) = submenu_rect {
                    if submenu_rect.contains(*x, *y) {
                        let index = ((*y - submenu_rect.y) / MENU_ROW_H) as usize;
                        if let Some(menu) = self.context_menu.as_mut() {
                            menu.submenu_index =
                                index.min(menu.submenu_labels.len().saturating_sub(1));
                        }
                        return Some(EventResponse::Consumed);
                    }
                }
                if menu_rect.contains(*x, *y) {
                    let index = ((*y - menu_rect.y) / MENU_ROW_H) as usize;
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.index = index.min(menu.labels.len().saturating_sub(1));
                    }
                    return Some(EventResponse::Consumed);
                }
                None
            }
            UiEvent::MousePress { x, y } => {
                if let Some((kind, submenu_rect)) = submenu_rect {
                    if submenu_rect.contains(*x, *y) {
                        let index = ((*y - submenu_rect.y) / MENU_ROW_H) as usize;
                        return Some(self.activate_submenu(kind, index, data, target));
                    }
                }
                if menu_rect.contains(*x, *y) {
                    let index = ((*y - menu_rect.y) / MENU_ROW_H) as usize;
                    return Some(self.activate_menu_item(index, data, target));
                }
                self.context_menu = None;
                Some(EventResponse::Consumed)
            }
            _ => None,
        }
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
        let hit = self.row_rect_at(panel, x, y, data);
        match id {
            RowId::Audio(AudioRowId::Media(audio_id)) => {
                if let Some(RowRect {
                    row:
                        Row {
                            id: RowId::Band(band_id),
                            ..
                        },
                    ..
                }) = hit.as_ref()
                {
                    let Some(audio) = data.audio(AudioRowId::Media(audio_id)) else {
                        return EventResponse::Consumed;
                    };
                    return EventResponse::Action(UiAction::SetLanguageInstrumentalAudioPath {
                        id: *band_id,
                        path: audio.path.clone(),
                    });
                }
                self.audio_insertion_index(panel, y, data).map(|to_index| {
                    UiAction::MediaReorderAudio {
                        id: audio_id,
                        to_index,
                    }
                })
            }
            RowId::Video(proxy_id) => {
                if let Some(hit) = hit.as_ref() {
                    if let RowId::Video(source_id) = hit.row.id {
                        if self.drop_is_on_row_center(hit, y)
                            && can_associate_proxy(data, proxy_id, source_id)
                        {
                            return EventResponse::Action(UiAction::MediaVideoAssociateProxy {
                                proxy_id,
                                source_id,
                            });
                        }
                    }
                }
                self.video_insertion_index(panel, y, data).map(|to_index| {
                    UiAction::MediaReorderVideo {
                        id: proxy_id,
                        to_index,
                    }
                })
            }
            RowId::Band(band_id) => {
                if let Some(RowRect {
                    row:
                        Row {
                            id: RowId::Audio(AudioRowId::Media(audio_id)),
                            ..
                        },
                    ..
                }) = hit.as_ref()
                {
                    let Some(audio) = data.audio(AudioRowId::Media(*audio_id)) else {
                        return EventResponse::Consumed;
                    };
                    return EventResponse::Action(UiAction::SetLanguageInstrumentalAudioPath {
                        id: band_id,
                        path: audio.path.clone(),
                    });
                }
                self.band_insertion_index(panel, y, data).map(|to_index| {
                    UiAction::LanguageReorder {
                        id: band_id,
                        to_index,
                    }
                })
            }
            RowId::Root | RowId::Group(_) | RowId::Audio(AudioRowId::OriginalVideo) => None,
        }
        .map(EventResponse::Action)
        .unwrap_or(EventResponse::Consumed)
    }

    fn video_insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let rows = self.group_row_rects(panel, data, GroupKind::Videos);
        let position = insertion_position(&rows, y)?;
        rows.get(position)
            .and_then(|row| match row.row.id {
                RowId::Video(id) => data.videos.iter().position(|video| video.id == id),
                _ => None,
            })
            .or(Some(data.videos.len()))
    }

    fn audio_insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let rows = self.group_row_rects(panel, data, GroupKind::Audios);
        let position = insertion_position(&rows, y)?;
        rows.get(position)
            .and_then(|row| match row.row.id {
                RowId::Audio(AudioRowId::Media(id)) => data
                    .audios
                    .iter()
                    .filter_map(|audio| audio.media_id)
                    .position(|id_at_index| id_at_index == id),
                _ => None,
            })
            .or_else(|| {
                Some(
                    data.audios
                        .iter()
                        .filter(|audio| audio.media_id.is_some())
                        .count(),
                )
            })
    }

    fn band_insertion_index(&self, panel: Rect, y: f32, data: &FileTreeData) -> Option<usize> {
        let rows = self.group_row_rects(panel, data, GroupKind::Bands);
        let position = insertion_position(&rows, y)?;
        rows.get(position)
            .and_then(|row| match row.row.id {
                RowId::Band(id) => data.bands.iter().position(|band| band.id == id),
                _ => None,
            })
            .or(Some(data.bands.len()))
    }

    fn group_row_rects(&self, panel: Rect, data: &FileTreeData, group: GroupKind) -> Vec<RowRect> {
        self.row_rects(panel, data)
            .into_iter()
            .filter(|row| match group {
                GroupKind::Videos => matches!(row.row.id, RowId::Video(_)),
                GroupKind::Bands => matches!(row.row.id, RowId::Band(_)),
                GroupKind::Audios => matches!(row.row.id, RowId::Audio(AudioRowId::Media(_))),
            })
            .collect()
    }

    fn drop_is_on_row_center(&self, row: &RowRect, y: f32) -> bool {
        (y - (row.rect.y + row.rect.height * 0.5)).abs() <= row.rect.height * 0.25
    }

    fn auto_scroll_during_drag(&mut self, panel: Rect, data: &FileTreeData) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        let edge = 30.0;
        if drag.current.1 < body_rect(panel).y + edge {
            self.scroll = self.scroll.saturating_sub(1);
        } else if drag.current.1 > body_rect(panel).y + body_rect(panel).height - edge {
            self.scroll = (self.scroll + 1).min(self.max_scroll(panel, data));
        }
    }

    // -- Helpers --

    fn max_scroll(&self, panel: Rect, data: &FileTreeData) -> usize {
        flatten(data, &self.expanded)
            .len()
            .saturating_sub(visible_rows(panel))
    }

    fn clamped_scroll(&self, panel: Rect, data: &FileTreeData) -> usize {
        self.scroll.min(self.max_scroll(panel, data))
    }

    fn clamp_scroll(&mut self, panel: Rect, data: &FileTreeData) {
        self.scroll = self.clamped_scroll(panel, data);
    }

    fn row_rects(&self, panel: Rect, data: &FileTreeData) -> Vec<RowRect> {
        let body = body_rect(panel);
        let rows = flatten(data, &self.expanded);
        let scroll = self.clamped_scroll(panel, data);
        let visible = visible_rows(panel);
        let width = (body.width - SCROLLBAR_W - 12.0).max(0.0);
        rows.iter()
            .enumerate()
            .skip(scroll)
            .take(visible)
            .map(|(index, row)| RowRect {
                row: row.clone(),
                index,
                rect: Rect {
                    x: body.x + 4.0,
                    y: body.y + (index - scroll) as f32 * ROW_H,
                    width,
                    height: ROW_H.min(
                        (body.y + body.height - (body.y + (index - scroll) as f32 * ROW_H))
                            .max(0.0),
                    ),
                },
            })
            .collect()
    }

    fn row_rect_at(&self, panel: Rect, x: f32, y: f32, data: &FileTreeData) -> Option<RowRect> {
        self.row_rects(panel, data)
            .into_iter()
            .find(|row| row.rect.contains(x, y))
    }

    fn begin_scroll_drag(&mut self, panel: Rect, data: &FileTreeData, x: f32, y: f32) -> bool {
        let rows = flatten(data, &self.expanded);
        let Some((track, thumb, _)) =
            scrollbar_geometry(panel, rows.len(), self.clamped_scroll(panel, data))
        else {
            return false;
        };
        if thumb.contains(x, y) {
            self.scroll_drag = Some(y - thumb.y);
            return true;
        }
        if track.contains(x, y) {
            self.scroll_drag = Some(thumb.height * 0.5);
            self.update_scroll_drag(panel, data, y);
            return true;
        }
        false
    }

    fn update_scroll_drag(&mut self, panel: Rect, data: &FileTreeData, y: f32) {
        let Some(offset) = self.scroll_drag else {
            return;
        };
        let rows = flatten(data, &self.expanded);
        let Some((track, thumb, max_scroll)) =
            scrollbar_geometry(panel, rows.len(), self.clamped_scroll(panel, data))
        else {
            self.scroll_drag = None;
            return;
        };
        let travel = track.height - thumb.height;
        if travel <= f32::EPSILON || max_scroll == 0 {
            self.scroll = 0;
            return;
        }
        let top = (y - offset).clamp(track.y, track.y + travel);
        self.scroll = (((top - track.y) / travel) * max_scroll as f32).round() as usize;
    }

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
        self.clamp_scroll(panel, data);
    }

    fn row_at(&self, panel: Rect, x: f32, y: f32, data: &FileTreeData) -> Option<RowId> {
        self.row_rect_at(panel, x, y, data).map(|row| row.row.id)
    }

    fn row_screen_position(
        &self,
        panel: Rect,
        id: RowId,
        data: &FileTreeData,
    ) -> Option<(f32, f32)> {
        self.row_rects(panel, data)
            .into_iter()
            .find(|row| row.row.id == id)
            .map(|row| (row.rect.x + PAD, row.rect.y))
    }

    fn drag_feedback(&self, panel: Rect, data: &FileTreeData) -> Option<DragFeedback> {
        let drag = self.drag.as_ref()?;
        if !drag.is_past_threshold() {
            return None;
        }
        let hit = self.row_rect_at(panel, drag.current.0, drag.current.1, data);
        match drag.id {
            RowId::Audio(AudioRowId::Media(_)) => {
                if let Some(hit) = hit.filter(|hit| matches!(hit.row.id, RowId::Band(_))) {
                    return Some(DragFeedback::Target(hit.rect));
                }
                self.audio_insertion_feedback(panel, drag.current.1, data)
            }
            RowId::Video(proxy_id) => {
                if let Some(hit) = hit.as_ref() {
                    if let RowId::Video(source_id) = hit.row.id {
                        if self.drop_is_on_row_center(hit, drag.current.1)
                            && can_associate_proxy(data, proxy_id, source_id)
                        {
                            return Some(DragFeedback::Target(hit.rect));
                        }
                    }
                }
                self.video_insertion_feedback(panel, drag.current.1, data)
            }
            RowId::Band(_) => {
                if let Some(hit) =
                    hit.filter(|hit| matches!(hit.row.id, RowId::Audio(AudioRowId::Media(_))))
                {
                    return Some(DragFeedback::Target(hit.rect));
                }
                self.band_insertion_feedback(panel, drag.current.1, data)
            }
            RowId::Root | RowId::Group(_) | RowId::Audio(AudioRowId::OriginalVideo) => None,
        }
    }

    fn video_insertion_feedback(
        &self,
        panel: Rect,
        y: f32,
        data: &FileTreeData,
    ) -> Option<DragFeedback> {
        insertion_feedback(&self.group_row_rects(panel, data, GroupKind::Videos), y)
    }

    fn audio_insertion_feedback(
        &self,
        panel: Rect,
        y: f32,
        data: &FileTreeData,
    ) -> Option<DragFeedback> {
        insertion_feedback(&self.group_row_rects(panel, data, GroupKind::Audios), y)
    }

    fn band_insertion_feedback(
        &self,
        panel: Rect,
        y: f32,
        data: &FileTreeData,
    ) -> Option<DragFeedback> {
        insertion_feedback(&self.group_row_rects(panel, data, GroupKind::Bands), y)
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
        let label = self.accessibility_label(id, data);
        EventResponse::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Focus {
                label,
                role: "tree item".to_string(),
            },
        ))
    }

    fn expand_event(&self, kind: GroupKind, data: &FileTreeData) -> EventResponse {
        EventResponse::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::ValueChanged {
                label: self.accessibility_label(RowId::Group(kind), data),
                value: if self.expanded.get(kind) {
                    t("accessibility.expanded").to_string()
                } else {
                    t("accessibility.collapsed").to_string()
                },
            },
        ))
    }

    fn accessibility_label(&self, id: RowId, data: &FileTreeData) -> String {
        let rows = flatten(data, &self.expanded);
        let position = self::rows::set_metrics(&rows, id)
            .map(|(position, total)| {
                interpolate(
                    t("file_tree.a11y.position"),
                    &[
                        ("{position}", position.to_string()),
                        ("{total}", total.to_string()),
                    ],
                )
            })
            .unwrap_or_default();
        let mut parts = match id {
            RowId::Root => vec![interpolate(
                t("file_tree.a11y.project"),
                &[("{name}", data.root_name.clone())],
            )],
            RowId::Group(kind) => {
                let count = match kind {
                    GroupKind::Videos => data.videos.len(),
                    GroupKind::Bands => data.bands.len(),
                    GroupKind::Audios => data.audios.len(),
                };
                let count_label = if count == 1 {
                    t("file_tree.a11y.one_item").to_string()
                } else {
                    interpolate(
                        t("file_tree.a11y.many_items"),
                        &[("{count}", count.to_string())],
                    )
                };
                vec![
                    interpolate(
                        t("file_tree.a11y.group"),
                        &[("{name}", group_label(kind).to_string())],
                    ),
                    count_label,
                ]
            }
            RowId::Video(video_id) => {
                let Some(video) = data.video(video_id) else {
                    return String::new();
                };
                let first = if let Some(source_id) = video.proxy_of {
                    let source = data
                        .video(source_id)
                        .map(|source| source.name.as_str())
                        .unwrap_or("");
                    interpolate(
                        t("file_tree.a11y.video_proxy"),
                        &[
                            ("{source}", source.to_string()),
                            ("{name}", video.name.clone()),
                        ],
                    )
                } else {
                    interpolate(
                        t("file_tree.a11y.video_source"),
                        &[("{name}", video.name.clone())],
                    )
                };
                let mut video_parts = vec![first];
                if video.proxy_of.is_some() {
                    video_parts.push(t("file_tree.a11y.is_proxy").to_string());
                }
                if video.is_default {
                    video_parts.push(t("file_tree.badges.default").to_string());
                }
                if video.is_proxy_source {
                    video_parts.push(t("file_tree.badges.has_proxy").to_string());
                }
                if video.active {
                    video_parts.push(t("file_tree.a11y.active").to_string());
                }
                if video.missing {
                    video_parts.push(t("file_tree.a11y.missing").to_string());
                }
                video_parts
            }
            RowId::Band(band_id) => {
                let Some(band) = data.bands.iter().find(|band| band.id == band_id) else {
                    return String::new();
                };
                let mut band_parts = vec![interpolate(
                    t("file_tree.a11y.band"),
                    &[("{name}", band.name.clone())],
                )];
                if band.active {
                    band_parts.push(t("accessibility.selected").to_string());
                }
                band_parts
            }
            RowId::Audio(AudioRowId::OriginalVideo) => {
                vec![t("file_tree.a11y.original_audio").to_string()]
            }
            RowId::Audio(audio_id) => {
                let Some(audio) = data.audio(audio_id) else {
                    return String::new();
                };
                let mut audio_parts = vec![interpolate(
                    t("file_tree.a11y.audio"),
                    &[("{name}", audio.name.clone())],
                )];
                if !audio.instrumental_of.is_empty() {
                    audio_parts.push(interpolate(
                        t("file_tree.a11y.instrumental"),
                        &[("{bands}", audio.instrumental_of.join(", "))],
                    ));
                }
                audio_parts
            }
        };
        if self.selected == Some(id) && !matches!(id, RowId::Band(_)) {
            parts.push(t("accessibility.selected").to_string());
        }
        if !position.is_empty() {
            parts.push(position);
        }
        parts.join(", ")
    }

    fn status_icons_for_row<'a>(
        &self,
        id: RowId,
        y: f32,
        body: Rect,
        data: &'a FileTreeData,
    ) -> Vec<StatusIcon<'a>> {
        let mut right = body.x + body.width - SCROLLBAR_W - PAD;
        let mut icons = Vec::new();
        let mut push = |name, tooltip, color| {
            let rect = Rect {
                x: right - STATUS_ICON_SIZE,
                y: y + (ROW_H - STATUS_ICON_SIZE) * 0.5,
                width: STATUS_ICON_SIZE,
                height: STATUS_ICON_SIZE,
            };
            right -= STATUS_ICON_SIZE + STATUS_ICON_GAP;
            icons.push(StatusIcon {
                name,
                tooltip,
                rect,
                color,
            });
        };
        match id {
            RowId::Video(video_id) => {
                if let Some(video) = data.video(video_id) {
                    if video.proxy_of.is_some() {
                        push(
                            "file-tree/proxy",
                            t("file_tree.badges.proxy"),
                            [126, 176, 255],
                        );
                    }
                    if video.is_default {
                        push(
                            "file-tree/default",
                            t("file_tree.badges.default"),
                            [255, 207, 92],
                        );
                    }
                    if video.is_proxy_source {
                        push(
                            "file-tree/has-proxy",
                            t("file_tree.badges.has_proxy"),
                            [92, 210, 235],
                        );
                    }
                }
            }
            RowId::Audio(audio_id) => {
                if let Some(audio) = data.audio(audio_id) {
                    if !audio.instrumental_of.is_empty() {
                        push(
                            "file-tree/rythmo-band",
                            audio.instrumental_badge.as_str(),
                            [202, 143, 255],
                        );
                    }
                }
            }
            _ => {}
        }
        icons
    }

    fn status_tooltip_at(
        &self,
        panel: Rect,
        x: f32,
        y: f32,
        data: &FileTreeData,
    ) -> Option<String> {
        let row = self.row_rect_at(panel, x, y, data)?;
        self.status_icons_for_row(row.row.id, row.rect.y, body_rect(panel), data)
            .into_iter()
            .find(|icon| icon.rect.contains(x, y))
            .map(|icon| icon.tooltip.to_string())
    }

    // -- Rendering --

    pub fn render<'a>(
        &'a self,
        panel: Rect,
        data: &'a FileTreeData,
        icon_uvs: &HashMap<String, [f32; 4]>,
        quads: &mut Vec<QuadInstance>,
        icons: &mut Vec<IconInstance>,
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

        let rows = flatten(data, &self.expanded);
        let body = body_rect(panel);
        let scroll = self.clamped_scroll(panel, data);

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

        for row in self.row_rects(panel, data) {
            let progress = self
                .enter
                .get(&row.row.id)
                .map(Tween::progress)
                .unwrap_or(1.0);
            let layout_y = self
                .layout_positions
                .get(&row.row.id)
                .map(|position| position.value)
                .unwrap_or(row.index as f32 * ROW_H);
            let y = body.y + layout_y - scroll as f32 * ROW_H - (1.0 - progress) * 6.0;
            self.render_row(
                &row.row, y, panel, data, icon_uvs, progress, quads, icons, labels,
            );
        }

        // Scrollbar.
        if let Some((track, thumb, _)) = scrollbar_geometry(panel, rows.len(), scroll) {
            solid(quads, track, [0.10, 0.103, 0.125, 1.0], [0.0; 4], 2.0);
            solid(quads, thumb, [0.31, 0.33, 0.42, 1.0], [0.0; 4], 2.0);
        }

        if let Some(feedback) = self.drag_feedback(panel, data) {
            match feedback {
                DragFeedback::Target(rect) => solid(
                    quads,
                    Rect {
                        x: rect.x + 2.0,
                        y: rect.y + 2.0,
                        width: (rect.width - 4.0).max(0.0),
                        height: (rect.height - 4.0).max(0.0),
                    },
                    [0.16, 0.27, 0.50, 0.50],
                    [0.42, 0.60, 1.0, 0.95],
                    4.0,
                ),
                DragFeedback::Insertion { x, y, width } => solid(
                    quads,
                    Rect {
                        x,
                        y: y - 1.0,
                        width,
                        height: 2.0,
                    },
                    [0.42, 0.60, 1.0, 1.0],
                    [0.0; 4],
                    1.0,
                ),
            }
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
        icon_uvs: &HashMap<String, [f32; 4]>,
        opacity: f32,
        quads: &mut Vec<QuadInstance>,
        icons: &mut Vec<IconInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
    ) {
        let body = body_rect(panel);
        let indent = row_indent(row);
        let selected = self.selected == Some(row.id);
        let focused = self.focused == Some(row.id);
        let tint = |color| faded_text_color(color, opacity);

        if let Some(kind) = row_category(row.id) {
            let row_rect = Rect {
                x: body.x + 4.0,
                y: y + 2.0,
                width: body.width - 8.0 - SCROLLBAR_W,
                height: ROW_H - 4.0,
            };
            solid(
                quads,
                row_rect,
                category_background(kind, matches!(row.id, RowId::Group(_)), opacity),
                [0.0; 4],
                5.0,
            );
        }

        if selected {
            let mut color = [0.12, 0.14, 0.23, 1.0];
            color[3] = 0.9 * opacity;
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
                [0.38, 0.58, 0.96, opacity],
                4.0,
            );
        }

        let icon_x = body.x + PAD + indent;
        match row.id {
            RowId::Root => {
                push_icon(
                    icons,
                    icon_uvs,
                    "file-tree/folder",
                    Rect {
                        x: icon_x,
                        y: y + (ROW_H - ICON_SIZE) * 0.5,
                        width: ICON_SIZE,
                        height: ICON_SIZE,
                    },
                    [255, 205, 96],
                    opacity,
                );
                labels.push(label(
                    &data.root_name,
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 8.0 - SCROLLBAR_W - 2.0 * PAD,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    14.0,
                    tint([235, 238, 246]),
                ));
            }
            RowId::Group(kind) => {
                let accent = category_color(kind);
                push_icon(
                    icons,
                    icon_uvs,
                    "file-tree/folder",
                    Rect {
                        x: icon_x,
                        y: y + (ROW_H - ICON_SIZE) * 0.5,
                        width: ICON_SIZE,
                        height: ICON_SIZE,
                    },
                    color_to_u8(accent),
                    opacity,
                );
                labels.push(label(
                    group_label(kind),
                    Rect {
                        x: icon_x + ICON_SIZE + 8.0,
                        y,
                        width: body.width - indent - ICON_SIZE - 14.0 - 2.0 * PAD,
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.5,
                    tint([200, 204, 218]),
                ));
            }
            RowId::Video(id) => {
                let Some(video) = data.videos.iter().find(|v| v.id == id) else {
                    return;
                };
                push_icon(
                    icons,
                    icon_uvs,
                    if video.proxy_of.is_some() {
                        "file-tree/video-proxy"
                    } else {
                        "file-tree/video-source"
                    },
                    Rect {
                        x: icon_x,
                        y: y + (ROW_H - ICON_SIZE) * 0.5,
                        width: ICON_SIZE,
                        height: ICON_SIZE,
                    },
                    if video.missing {
                        [220, 170, 80]
                    } else {
                        [105, 169, 255]
                    },
                    opacity,
                );
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
                let text_x = icon_x + ICON_SIZE + 8.0;
                let status_icons = self.status_icons_for_row(row.id, y, body, data);
                let badge_left = status_icons
                    .iter()
                    .map(|icon| icon.rect.x)
                    .reduce(f32::min)
                    .unwrap_or(body.x + body.width - SCROLLBAR_W - PAD);
                labels.push(label(
                    name,
                    Rect {
                        x: text_x,
                        y,
                        width: (badge_left - BADGE_TEXT_GAP - text_x).max(0.0),
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.0,
                    tint(text_color),
                ));
                self.render_status_icons(status_icons, icon_uvs, opacity, icons);
            }
            RowId::Audio(audio_id) => {
                let Some(audio) = data.audios.iter().find(|a| a.id == audio_id) else {
                    return;
                };
                let is_original = audio.media_id.is_none();
                push_icon(
                    icons,
                    icon_uvs,
                    if is_original {
                        "file-tree/audio-original"
                    } else {
                        "file-tree/audio-file"
                    },
                    Rect {
                        x: icon_x,
                        y: y + (ROW_H - ICON_SIZE) * 0.5,
                        width: ICON_SIZE,
                        height: ICON_SIZE,
                    },
                    if is_original {
                        [142, 225, 205]
                    } else {
                        [82, 211, 184]
                    },
                    opacity,
                );
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
                let text_x = icon_x + ICON_SIZE + 8.0;
                let status_icons = self.status_icons_for_row(row.id, y, body, data);
                let badge_left = status_icons
                    .iter()
                    .map(|icon| icon.rect.x)
                    .reduce(f32::min)
                    .unwrap_or(body.x + body.width - SCROLLBAR_W - PAD);
                labels.push(label(
                    name,
                    Rect {
                        x: text_x,
                        y,
                        width: (badge_left - BADGE_TEXT_GAP - text_x).max(0.0),
                        height: ROW_H,
                    },
                    HAlign::Left,
                    13.0,
                    tint(text_color),
                ));
                self.render_status_icons(status_icons, icon_uvs, opacity, icons);
            }
            RowId::Band(band_id) => {
                let Some(band) = data.bands.iter().find(|b| b.id == band_id) else {
                    return;
                };
                push_icon(
                    icons,
                    icon_uvs,
                    "file-tree/rythmo-band",
                    Rect {
                        x: icon_x,
                        y: y + (ROW_H - ICON_SIZE) * 0.5,
                        width: ICON_SIZE,
                        height: ICON_SIZE,
                    },
                    [193, 129, 255],
                    opacity,
                );
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
                    tint(if band.active {
                        [130, 180, 255]
                    } else {
                        [222, 225, 235]
                    }),
                ));
            }
        }
    }

    fn render_status_icons(
        &self,
        status_icons: Vec<StatusIcon<'_>>,
        icon_uvs: &HashMap<String, [f32; 4]>,
        opacity: f32,
        icons: &mut Vec<IconInstance>,
    ) {
        for status in status_icons {
            push_icon(
                icons,
                icon_uvs,
                status.name,
                status.rect,
                status.color,
                opacity,
            );
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
            width: MENU_W,
            height: menu.labels.len() as f32 * MENU_ROW_H,
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
                y: menu_rect.y + index as f32 * MENU_ROW_H,
                width: menu_rect.width,
                height: MENU_ROW_H,
            };
            let enabled = menu.enabled.get(index).copied().unwrap_or(false);
            if index == menu.index && enabled {
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
                if enabled {
                    [232, 234, 242]
                } else {
                    [128, 131, 145]
                },
            ));
        }

        if let Some(submenu) = &menu.submenu {
            let submenu_rect = Rect {
                x: submenu.anchor.0,
                y: submenu.anchor.1,
                width: SUBMENU_W,
                height: menu.submenu_labels.len() as f32 * MENU_ROW_H,
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
                    y: submenu_rect.y + index as f32 * MENU_ROW_H,
                    width: submenu_rect.width,
                    height: MENU_ROW_H,
                };
                let enabled = menu.submenu_enabled.get(index).copied().unwrap_or(false);
                if index == menu.submenu_index && enabled {
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
                    if enabled {
                        [232, 234, 242]
                    } else {
                        [128, 131, 145]
                    },
                ));
            }
        }
    }
}

pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    pub submenu: Option<SubmenuKind>,
    pub action: Box<dyn Fn() -> EventResponse>,
}

fn context_menu_items(row: RowId, data: &FileTreeData) -> Vec<MenuItem> {
    match row {
        RowId::Group(GroupKind::Videos) => vec![MenuItem {
            label: t("file_tree.menu.add_video").to_string(),
            enabled: true,
            submenu: None,
            action: Box::new(|| EventResponse::Action(UiAction::AddVideo)),
        }],
        RowId::Group(GroupKind::Audios) => vec![MenuItem {
            label: t("file_tree.menu.add_audio").to_string(),
            enabled: true,
            submenu: None,
            action: Box::new(|| EventResponse::Action(UiAction::AddMediaAudio)),
        }],
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
                action: Box::new(move || EventResponse::Action(UiAction::MediaVideoUse { id })),
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
                items.push(MenuItem {
                    label: t("file_tree.menu.restore_link").to_string(),
                    enabled: video.missing,
                    submenu: None,
                    action: Box::new(move || {
                        EventResponse::Action(UiAction::MediaVideoRelink { id })
                    }),
                });
                let has_proxy = data.videos.iter().any(|v| v.proxy_of == Some(id));
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
                if data.can_be_proxy_endpoint(id) {
                    items.push(MenuItem {
                        label: format!("{} ▸", t("file_tree.menu.associate_proxy")),
                        enabled: true,
                        submenu: Some(SubmenuKind::AssociateProxyTo),
                        action: Box::new(|| EventResponse::Consumed),
                    });
                }
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
                action: Box::new(move || EventResponse::Action(UiAction::MediaVideoRemove { id })),
            });
            items
        }
        RowId::Audio(AudioRowId::Media(id)) => vec![
            MenuItem {
                label: t("file_tree.menu.remove").to_string(),
                enabled: true,
                submenu: None,
                action: Box::new(move || EventResponse::Action(UiAction::MediaAudioRemove { id })),
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
                action: Box::new(move || EventResponse::Action(UiAction::DeleteLanguage { id })),
            });
            items
        }
        _ => Vec::new(),
    }
}

fn submenu_items(kind: SubmenuKind, data: &FileTreeData, target: RowId) -> Vec<MenuItem> {
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
                    EventResponse::Action(UiAction::ClearLanguageInstrumentalAudio { id: band_id })
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

fn data_audio_matches(_data: &FileTreeData, path: &str, audio: &AudioData) -> bool {
    paths_equal(path, &audio.path)
}

fn group_label(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Videos => t("file_tree.groups.videos"),
        GroupKind::Bands => t("file_tree.groups.bands"),
        GroupKind::Audios => t("file_tree.groups.audios"),
    }
}

fn row_category(id: RowId) -> Option<GroupKind> {
    match id {
        RowId::Group(kind) => Some(kind),
        other => parent_group(other),
    }
}

fn row_indent(row: &Row) -> f32 {
    match row.id {
        RowId::Root | RowId::Group(_) => 0.0,
        _ => row.depth.saturating_sub(1) as f32 * INDENT,
    }
}

fn category_color(kind: GroupKind) -> [f32; 3] {
    match kind {
        GroupKind::Videos => [0.41, 0.66, 1.0],
        GroupKind::Bands => [0.76, 0.51, 1.0],
        GroupKind::Audios => [0.32, 0.83, 0.72],
    }
}

fn category_background(kind: GroupKind, header: bool, opacity: f32) -> [f32; 4] {
    let (color, alpha) = match (kind, header) {
        (GroupKind::Videos, true) => ([0.075, 0.13, 0.225], 0.94),
        (GroupKind::Videos, false) => ([0.055, 0.085, 0.135], 0.76),
        (GroupKind::Bands, true) => ([0.15, 0.085, 0.20], 0.94),
        (GroupKind::Bands, false) => ([0.10, 0.065, 0.13], 0.76),
        (GroupKind::Audios, true) => ([0.045, 0.15, 0.14], 0.94),
        (GroupKind::Audios, false) => ([0.045, 0.105, 0.105], 0.76),
    };
    [color[0], color[1], color[2], alpha * opacity]
}

fn color_to_u8(color: [f32; 3]) -> [u8; 3] {
    std::array::from_fn(|index| (color[index] * 255.0).round() as u8)
}

fn push_icon(
    icons: &mut Vec<IconInstance>,
    icon_uvs: &HashMap<String, [f32; 4]>,
    name: &str,
    rect: Rect,
    color: [u8; 3],
    opacity: f32,
) {
    let Some(uv_rect) = icon_uvs.get(name).copied() else {
        return;
    };
    icons.push(IconInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        uv_rect,
        tint: [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            opacity.clamp(0.0, 1.0),
        ],
        transform: [0.0, 0.0, 0.5, 0.5],
    });
}

fn interpolate(template: &str, replacements: &[(&str, String)]) -> String {
    replacements
        .iter()
        .fold(template.to_string(), |text, (key, value)| {
            text.replace(key, value)
        })
}

fn parent_group(id: RowId) -> Option<GroupKind> {
    match id {
        RowId::Video(_) => Some(GroupKind::Videos),
        RowId::Audio(_) => Some(GroupKind::Audios),
        RowId::Band(_) => Some(GroupKind::Bands),
        _ => None,
    }
}

fn can_associate_proxy(data: &FileTreeData, proxy_id: MediaId, source_id: MediaId) -> bool {
    proxy_id != source_id
        && data.can_be_proxy_endpoint(proxy_id)
        && data
            .video(source_id)
            .is_some_and(|source| source.proxy_of.is_none() && !source.is_proxy_source)
}

fn insertion_position(rows: &[RowRect], y: f32) -> Option<usize> {
    let first = rows.first()?;
    let last = rows.last()?;
    if y < first.rect.y || y > last.rect.y + last.rect.height {
        return None;
    }
    Some(
        rows.iter()
            .position(|row| y < row.rect.y + row.rect.height * 0.5)
            .unwrap_or(rows.len()),
    )
}

fn insertion_feedback(rows: &[RowRect], y: f32) -> Option<DragFeedback> {
    let position = insertion_position(rows, y)?;
    let reference = rows.get(position).or_else(|| rows.last())?;
    let line_y = if position < rows.len() {
        reference.rect.y
    } else {
        reference.rect.y + reference.rect.height
    };
    Some(DragFeedback::Insertion {
        x: reference.rect.x,
        y: line_y,
        width: reference.rect.width,
    })
}

fn keyboard_activation(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Activate)
        || matches!(event, UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " ")
}

fn is_keyboard_event(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::KeyInput { .. }
            | UiEvent::CursorLeft
            | UiEvent::CursorRight
            | UiEvent::MoveWordLeft
            | UiEvent::MoveWordRight
            | UiEvent::ShiftCursorLeft
            | UiEvent::ShiftCursorRight
            | UiEvent::CursorUp
            | UiEvent::CursorDown
            | UiEvent::SelectWordLeft
            | UiEvent::SelectWordRight
            | UiEvent::FocusNext
            | UiEvent::FocusPrevious
            | UiEvent::Activate
            | UiEvent::Home
            | UiEvent::End
            | UiEvent::PageUp
            | UiEvent::PageDown
            | UiEvent::AltCursorLeft
            | UiEvent::AltCursorRight
            | UiEvent::OpenContextMenu
            | UiEvent::Delete
            | UiEvent::SelectAll
            | UiEvent::Copy
            | UiEvent::Cut
            | UiEvent::UndoTextEdit
    )
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
    p
}

fn visible_rows(p: Rect) -> usize {
    (body_rect(p).height / ROW_H).floor().max(1.0) as usize
}

fn scrollbar_geometry(panel: Rect, row_count: usize, scroll: usize) -> Option<(Rect, Rect, usize)> {
    let body = body_rect(panel);
    let visible = visible_rows(panel);
    if row_count <= visible {
        return None;
    }
    let max_scroll = row_count - visible;
    let track_height = (body.height - 12.0).max(0.0);
    if track_height <= 0.0 {
        return None;
    }
    let track = Rect {
        x: panel.x + panel.width - 10.0,
        y: body.y + 6.0,
        width: SCROLLBAR_W,
        height: track_height,
    };
    let thumb_height = (track.height * visible as f32 / row_count as f32)
        .clamp(28.0_f32.min(track.height), track.height);
    let travel = track.height - thumb_height;
    let thumb = Rect {
        x: track.x,
        y: track.y + travel * scroll.min(max_scroll) as f32 / max_scroll as f32,
        width: track.width,
        height: thumb_height,
    };
    Some((track, thumb, max_scroll))
}

fn clamped_menu_origin(panel: Rect, x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    let min_x = panel.x + MENU_MARGIN;
    let min_y = panel.y + MENU_MARGIN;
    let max_x = (panel.x + panel.width - width - MENU_MARGIN).max(min_x);
    let max_y = (panel.y + panel.height - height - MENU_MARGIN).max(min_y);
    (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
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

fn faded_text_color(color: [u8; 3], opacity: f32) -> [u8; 3] {
    let background = [17.0, 17.0, 21.0];
    std::array::from_fn(|index| {
        (background[index] + (color[index] as f32 - background[index]) * opacity.clamp(0.0, 1.0))
            .round() as u8
    })
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
    use crate::project::Project;

    fn panel() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 280.0,
            height: 420.0,
        }
    }

    fn sample() -> (FileTreeData, MediaId, MediaId, u64, u64, MediaId) {
        let mut project = Project::new();
        let source = project
            .add_media_video("Source", "C:/videos/source.mp4", None, false)
            .unwrap();
        let second_video = project
            .add_media_video("Second", "C:/videos/second.mp4", None, false)
            .unwrap();
        let active_band = project.active_language_id();
        let second_band = project.create_language_named("English");
        let audio = project
            .add_media_audio("Instrumental", "C:/audio/inst.wav")
            .unwrap();
        (
            FileTreeData::from_project(
                &project,
                "Demo project",
                Some("C:/videos/source.mp4"),
                None,
            ),
            source,
            second_video,
            active_band,
            second_band,
            audio,
        )
    }

    fn center(tree: &FileTree, data: &FileTreeData, id: RowId) -> (f32, f32) {
        let rect = tree
            .row_rects(panel(), data)
            .into_iter()
            .find(|row| row.row.id == id)
            .expect("row should be visible")
            .rect;
        (rect.x + 8.0, rect.y + rect.height * 0.5)
    }

    #[test]
    fn leaving_the_panel_clears_transient_hover_without_affecting_panel_state() {
        let (data, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();
        let (x, y) = center(&tree, &data, RowId::Root);

        assert_eq!(
            tree.handle_event(&UiEvent::MouseMove { x, y }, panel(), &data),
            Some(EventResponse::Consumed)
        );
        assert_eq!(tree.hover, Some(RowId::Root));

        assert_eq!(
            tree.handle_event(
                &UiEvent::MouseMove {
                    x: panel().width + 20.0,
                    y
                },
                panel(),
                &data,
            ),
            None
        );
        assert!(tree.is_open());
        assert_eq!(tree.hover, None);
        assert!(tree.hover_pill.is_none());
        assert_eq!(tree.hover_pill_fade, 0.0);
    }

    #[test]
    fn root_is_a_single_interactive_tree_row_without_a_header_duplicate() {
        let (data, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();

        let root = tree
            .row_rects(panel(), &data)
            .into_iter()
            .find(|row| row.row.id == RowId::Root)
            .unwrap();
        assert_eq!(root.rect.y, panel().y);
        assert_eq!(root.rect.height, ROW_H);

        let mut quads = Vec::new();
        let mut icons = Vec::new();
        let mut labels = Vec::new();
        tree.render(
            panel(),
            &data,
            &HashMap::new(),
            &mut quads,
            &mut icons,
            &mut labels,
        );
        assert_eq!(
            labels
                .iter()
                .filter(|label| label.text == "Demo project")
                .count(),
            1
        );
    }

    #[test]
    fn simple_band_click_selects_the_language_and_keeps_drag_available() {
        let (data, ..) = sample();
        let band = data
            .bands
            .iter()
            .find(|band| !band.active)
            .expect("sample needs an inactive band")
            .id;
        let mut tree = FileTree::new();
        tree.open();
        let (x, y) = center(&tree, &data, RowId::Band(band));

        let response = tree.handle_event(&UiEvent::MousePress { x, y }, panel(), &data);

        assert!(matches!(
            response,
            Some(EventResponse::Action(UiAction::SelectLanguage { id })) if id == band
        ));
        assert!(
            matches!(tree.drag.as_ref().map(|drag| drag.id), Some(RowId::Band(id)) if id == band)
        );
    }

    #[test]
    fn virtual_original_audio_can_be_selected_but_never_dragged_or_menued() {
        let (data, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();
        let (x, y) = center(&tree, &data, RowId::Audio(AudioRowId::OriginalVideo));

        assert_eq!(
            tree.handle_event(&UiEvent::MousePress { x, y }, panel(), &data),
            Some(EventResponse::Consumed)
        );
        assert!(tree.drag.is_none());
        assert_eq!(tree.selected, Some(RowId::Audio(AudioRowId::OriginalVideo)));

        assert_eq!(
            tree.handle_event(&UiEvent::ContextMenu { x, y }, panel(), &data),
            Some(EventResponse::Consumed)
        );
        assert!(tree.context_menu.is_none());
    }

    #[test]
    fn context_menu_pointer_click_reaches_its_action() {
        let (data, source, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();
        let (x, y) = center(&tree, &data, RowId::Video(source));
        tree.handle_event(&UiEvent::ContextMenu { x, y }, panel(), &data);
        let anchor = tree.context_menu.as_ref().unwrap().anchor;

        assert!(matches!(
            tree.handle_event(
                &UiEvent::MousePress {
                    x: anchor.0 + 8.0,
                    y: anchor.1 + MENU_ROW_H * 0.5,
                },
                panel(),
                &data,
            ),
            Some(EventResponse::Action(UiAction::MediaVideoUse { id })) if id == source
        ));
    }

    #[test]
    fn video_reordering_uses_the_videos_group_index() {
        let (data, source, second, ..) = sample();
        let mut tree = FileTree::new();
        let source_rect = tree
            .row_rects(panel(), &data)
            .into_iter()
            .find(|row| row.row.id == RowId::Video(source))
            .unwrap()
            .rect;

        let response = tree.drop_drag(
            RowId::Video(second),
            source_rect.x + 8.0,
            source_rect.y + 2.0,
            panel(),
            &data,
        );

        assert!(matches!(
            response,
            EventResponse::Action(UiAction::MediaReorderVideo { id, to_index })
                if id == second && to_index == 0
        ));
    }

    #[test]
    fn reverse_audio_to_band_drag_assigns_the_audio_path() {
        let (data, _, _, _, band, audio) = sample();
        let mut tree = FileTree::new();
        let (x, y) = center(&tree, &data, RowId::Audio(AudioRowId::Media(audio)));

        let response = tree.drop_drag(RowId::Band(band), x, y, panel(), &data);

        assert!(matches!(
            response,
            EventResponse::Action(UiAction::SetLanguageInstrumentalAudioPath { id, path })
                if id == band && path == "C:/audio/inst.wav"
        ));
    }

    #[test]
    fn f2_and_deferred_rename_both_use_the_existing_label() {
        let (data, source, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();
        let (x, y) = center(&tree, &data, RowId::Video(source));
        tree.handle_event(&UiEvent::MousePress { x, y }, panel(), &data);

        assert_eq!(
            tree.handle_event(&UiEvent::KeyInput { text: "F2".into() }, panel(), &data,),
            Some(EventResponse::Consumed)
        );
        assert_eq!(tree.rename_buffer(), "Source");
        tree.cancel_rename();

        tree.begin_rename(RenameTarget::Video(source), "");
        tree.handle_event(&UiEvent::KeyInput { text: "!".into() }, panel(), &data);
        assert_eq!(tree.rename_buffer(), "Source!");
        assert!(matches!(
            tree.handle_event(
                &UiEvent::KeyInput {
                    text: "\r".into(),
                },
                panel(),
                &data,
            ),
            Some(EventResponse::Action(UiAction::MediaVideoRename { id, name }))
                if id == source && name == "Source!"
        ));
    }

    #[test]
    fn instrumental_none_uses_the_clear_action_and_assigned_audio_is_checked() {
        let (mut data, _, _, active_band, _, audio) = sample();
        data.bands
            .iter_mut()
            .find(|band| band.id == active_band)
            .unwrap()
            .instrumental_audio_path = Some("C:/audio/inst.wav".into());

        let items = submenu_items(SubmenuKind::Instrumental, &data, RowId::Band(active_band));
        assert_eq!(items[1].label, "✓ Instrumental");
        assert!(matches!(
            (items[0].action)(),
            EventResponse::Action(UiAction::ClearLanguageInstrumentalAudio { id }) if id == active_band
        ));
        assert_eq!(
            data.audio(AudioRowId::Media(audio)).unwrap().name,
            "Instrumental"
        );
    }

    #[test]
    fn opening_the_tree_seeds_row_entry_animations() {
        let (data, source, ..) = sample();
        let mut tree = FileTree::new();
        tree.open();

        assert!(tree.enter.is_empty());
        assert!(tree.animate(&data, 0.0));
        assert!(tree.enter.contains_key(&RowId::Video(source)));
        assert_eq!(tree.enter[&RowId::Video(source)].progress(), 0.0);
    }

    #[test]
    fn accessible_proxy_label_contains_its_full_breadcrumb_and_states() {
        let mut project = Project::new();
        let source = project
            .add_media_video("Source", "C:/videos/source.mp4", None, false)
            .unwrap();
        let proxy = project
            .add_media_video("A1 proxy", "C:/videos/a1-proxy.mp4", Some(source), true)
            .unwrap();
        project.set_default_video(Some(proxy)).unwrap();
        let data = FileTreeData::from_project(
            &project,
            "Demo",
            Some("C:/videos/source.mp4"),
            Some("C:/videos/a1-proxy.mp4"),
        );
        let mut tree = FileTree::new();
        tree.selected = Some(RowId::Video(proxy));

        let spoken = tree.accessibility_label(RowId::Video(proxy), &data);

        assert!(spoken.contains("Source"));
        assert!(spoken.contains("A1 proxy"));
        assert!(spoken.contains(t("file_tree.badges.default")));
        assert!(spoken.contains(t("file_tree.a11y.is_proxy")));
        assert!(spoken.contains(t("accessibility.selected")));
        assert!(spoken.contains(&interpolate(
            t("file_tree.a11y.position"),
            &[("{position}", "1".into()), ("{total}", "1".into())],
        )));
    }

    #[test]
    fn accessible_audio_label_names_its_instrumental_rythmo_band() {
        let mut project = Project::new();
        let french = project.active_language_id();
        project.rename_language(french, "Français");
        let audio = project
            .add_media_audio("Version instrumentale", "C:/audio/fr.wav")
            .unwrap();
        project.set_language_instrumental_audio_path(french, Some("C:/audio/fr.wav".into()));
        let data = FileTreeData::from_project(&project, "Demo", None, None);
        let tree = FileTree::new();

        let spoken = tree.accessibility_label(RowId::Audio(AudioRowId::Media(audio)), &data);

        assert!(spoken.contains("Version instrumentale"));
        assert!(spoken.contains("Français"));
    }

    #[test]
    fn compact_status_icons_expose_their_tooltip_on_hover() {
        let mut project = Project::new();
        let source = project
            .add_media_video("Source", "C:/videos/source.mp4", None, false)
            .unwrap();
        project.set_default_video(Some(source)).unwrap();
        let data = FileTreeData::from_project(&project, "Demo", None, None);
        let mut tree = FileTree::new();
        tree.open();
        let row = tree
            .row_rects(panel(), &data)
            .into_iter()
            .find(|row| row.row.id == RowId::Video(source))
            .unwrap();
        let status = tree
            .status_icons_for_row(row.row.id, row.rect.y, body_rect(panel()), &data)
            .into_iter()
            .find(|icon| icon.name == "file-tree/default")
            .unwrap();

        tree.handle_event(
            &UiEvent::MouseMove {
                x: status.rect.x + status.rect.width * 0.5,
                y: status.rect.y + status.rect.height * 0.5,
            },
            panel(),
            &data,
        );

        assert_eq!(tree.hovered_tooltip(), Some(t("file_tree.badges.default")));
    }

    #[test]
    fn category_backgrounds_are_visually_distinct() {
        assert_ne!(
            category_background(GroupKind::Videos, true, 1.0),
            category_background(GroupKind::Bands, true, 1.0)
        );
        assert_ne!(
            category_background(GroupKind::Bands, true, 1.0),
            category_background(GroupKind::Audios, true, 1.0)
        );
    }

    #[test]
    fn rows_use_group_alignment_then_two_entry_levels() {
        let group = Row {
            id: RowId::Group(GroupKind::Videos),
            depth: 1,
        };
        let source = Row {
            id: RowId::Video(1),
            depth: 2,
        };
        let proxy = Row {
            id: RowId::Video(2),
            depth: 3,
        };

        assert_eq!(row_indent(&group), 0.0);
        assert_eq!(row_indent(&source), INDENT);
        assert_eq!(row_indent(&proxy), INDENT * 2.0);
    }
}
