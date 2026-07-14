#![allow(clippy::manual_clamp)]
#![allow(clippy::needless_range_loop)]

pub use crate::application::command::FilePickerIntent;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::i18n::t;
use crate::ui::text_input::{self, TextInputAction, TextInputMetrics, TextInputState};
use crate::ui::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};

mod controller;
mod model;
mod view;

const ROW_H: f32 = 28.0;
const HEADER_H: f32 = 26.0;
const TEXT_FIELD_PADDING_X: f32 = 8.0;
const TEXT_FIELD_FONT_SIZE: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileExplorerMode {
    Open,
    Save,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileFilterSpec {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilterSpec {
    pub fn new(name: impl Into<String>, extensions: &[&str]) -> Self {
        Self {
            name: name.into(),
            extensions: extensions
                .iter()
                .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
        }
    }

    fn matches_path(&self, path: &Path) -> bool {
        if self.extensions.is_empty() || self.extensions.iter().any(|ext| ext == "*") {
            return true;
        }

        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };
        let ext = ext.to_ascii_lowercase();
        self.extensions.iter().any(|candidate| candidate == &ext)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileExplorerRequest {
    pub title: String,
    pub mode: FileExplorerMode,
    pub intent: FilePickerIntent,
    pub filters: Vec<FileFilterSpec>,
    pub initial_dir: Option<PathBuf>,
    pub default_extension: Option<String>,
    pub initial_filename: Option<String>,
    pub extra_locations: Vec<(String, PathBuf)>,
}

pub enum FileExplorerResult {
    Consumed,
    Close,
    Selected {
        intent: FilePickerIntent,
        path: PathBuf,
    },
    Clipboard(String),
}

#[derive(Clone)]
struct DirectoryEntry {
    path: PathBuf,
    name: String,
    sort_name: String,
    is_dir: bool,
    modified_text: String,
    type_text: String,
    size_text: String,
}

#[derive(Clone)]
struct SidebarItem {
    label: String,
    path: PathBuf,
}

struct ScanMessage {
    generation: u64,
    dir: PathBuf,
    result: Result<Vec<DirectoryEntry>, String>,
}

#[derive(Clone, Copy, PartialEq)]
enum ActiveField {
    Address,
    NameFilter,
    Filename,
}

struct ExplorerLayout {
    card: Rect,
    back: Rect,
    forward: Rect,
    up: Rect,
    refresh: Rect,
    address: Rect,
    name_filter: Rect,
    sidebar: Rect,
    list: Rect,
    rows: Rect,
    scrollbar: Rect,
    filename: Rect,
    filter: Rect,
    new_folder: Rect,
    primary: Rect,
    cancel: Rect,
}

pub struct FileExplorerModal {
    title: String,
    mode: FileExplorerMode,
    intent: FilePickerIntent,
    filters: Vec<FileFilterSpec>,
    selected_filter: usize,
    default_extension: Option<String>,
    current_dir: PathBuf,
    address: String,
    name_filter: String,
    filename: String,
    entries: Vec<DirectoryEntry>,
    selected: Option<usize>,
    scroll_offset: f32,
    history_back: Vec<PathBuf>,
    history_forward: Vec<PathBuf>,
    sidebar: Vec<SidebarItem>,
    scan_generation: u64,
    scan_receiver: Option<Receiver<ScanMessage>>,
    loading: bool,
    error: Option<String>,
    status_text: String,
    active_field: Option<ActiveField>,
    address_input: TextInputState,
    name_filter_input: TextInputState,
    filename_input: TextInputState,
    show_filter_dropdown: bool,
    overwrite_path: Option<PathBuf>,
    dragging_scrollbar: bool,
    scrollbar_drag_anchor_y: f32,
    scrollbar_drag_anchor_offset: f32,
}

impl FileExplorerModal {
    fn layout(screen_w: f32, screen_h: f32) -> ExplorerLayout {
        let card_w = (screen_w - 48.0).max(620.0).min(1040.0);
        let card_h = (screen_h - 48.0).max(430.0).min(660.0);
        let card = Rect {
            x: (screen_w - card_w) / 2.0,
            y: (screen_h - card_h) / 2.0,
            width: card_w,
            height: card_h,
        };
        let toolbar_y = card.y + 48.0;
        let btn = 32.0;
        let gap = 6.0;
        let back = Rect {
            x: card.x + 18.0,
            y: toolbar_y,
            width: btn,
            height: btn,
        };
        let forward = Rect {
            x: back.x + btn + gap,
            y: toolbar_y,
            width: btn,
            height: btn,
        };
        let up = Rect {
            x: forward.x + btn + gap,
            y: toolbar_y,
            width: btn,
            height: btn,
        };
        let refresh = Rect {
            x: up.x + btn + gap,
            y: toolbar_y,
            width: btn,
            height: btn,
        };
        let search_w = (card.width * 0.25).clamp(190.0, 270.0);
        let name_filter = Rect {
            x: card.x + card.width - 18.0 - search_w,
            y: toolbar_y,
            width: search_w,
            height: btn,
        };
        let address_x = refresh.x + btn + 12.0;
        let address = Rect {
            x: address_x,
            y: toolbar_y,
            width: (name_filter.x - address_x - 8.0).max(140.0),
            height: btn,
        };
        let content_y = card.y + 92.0;
        let footer_y = card.y + card.height - 104.0;
        let sidebar = Rect {
            x: card.x + 18.0,
            y: content_y,
            width: 176.0,
            height: (footer_y - content_y - 12.0).max(120.0),
        };
        let list = Rect {
            x: sidebar.x + sidebar.width + 12.0,
            y: content_y,
            width: card.x + card.width - (sidebar.x + sidebar.width + 30.0),
            height: sidebar.height,
        };
        let rows = Rect {
            x: list.x,
            y: list.y + HEADER_H,
            width: list.width - 14.0,
            height: (list.height - HEADER_H).max(60.0),
        };
        let scrollbar = Rect {
            x: list.x + list.width - 11.0,
            y: rows.y + 3.0,
            width: 7.0,
            height: (rows.height - 6.0).max(20.0),
        };
        let filter_w = 214.0_f32.min((list.width * 0.34).max(160.0));
        let filename = Rect {
            x: list.x + 72.0,
            y: footer_y,
            width: (list.width - filter_w - 86.0).max(140.0),
            height: 32.0,
        };
        let filter = Rect {
            x: filename.x + filename.width + 8.0,
            y: footer_y,
            width: filter_w,
            height: 32.0,
        };
        let button_y = card.y + card.height - 50.0;
        let cancel = Rect {
            x: card.x + card.width - 230.0,
            y: button_y,
            width: 96.0,
            height: 34.0,
        };
        let primary = Rect {
            x: card.x + card.width - 124.0,
            y: button_y,
            width: 106.0,
            height: 34.0,
        };
        let new_folder = Rect {
            x: list.x,
            y: button_y,
            width: 136.0,
            height: 34.0,
        };

        ExplorerLayout {
            card,
            back,
            forward,
            up,
            refresh,
            address,
            name_filter,
            sidebar,
            list,
            rows,
            scrollbar,
            filename,
            filter,
            new_folder,
            primary,
            cancel,
        }
    }

    fn handle_key(&mut self, text: &str) -> FileExplorerResult {
        if text == "\x1b" {
            return FileExplorerResult::Close;
        }

        if text == "\r" || text == "\n" {
            if self.active_field == Some(ActiveField::Address) {
                self.submit_address();
                return FileExplorerResult::Consumed;
            }
            return self.complete_selection();
        }

        if text == "\x08" && self.active_field.is_none() {
            self.navigate_parent();
            return FileExplorerResult::Consumed;
        }

        if text == "\t" {
            self.toggle_focus();
            return FileExplorerResult::Consumed;
        }

        if self.active_field.is_some() {
            self.edit_active(text);
            return FileExplorerResult::Consumed;
        }

        if text.chars().any(|ch| !ch.is_control()) {
            self.activate_field(ActiveField::Filename);
            self.edit_active(text);
        }
        FileExplorerResult::Consumed
    }

    fn handle_mouse_press(
        &mut self,
        x: f32,
        y: f32,
        layout: &ExplorerLayout,
    ) -> FileExplorerResult {
        if self.show_filter_dropdown {
            if let Some(index) = self.filter_index_at(x, y, layout) {
                self.selected_filter = index;
                self.show_filter_dropdown = false;
                self.start_scan();
                return FileExplorerResult::Consumed;
            }
            if !layout.filter.contains(x, y) {
                self.show_filter_dropdown = false;
            }
        }

        if layout.back.contains(x, y) {
            self.navigate_back();
        } else if layout.forward.contains(x, y) {
            self.navigate_forward();
        } else if layout.up.contains(x, y) {
            self.navigate_parent();
        } else if layout.refresh.contains(x, y) {
            self.start_scan();
        } else if layout.address.contains(x, y) {
            self.activate_field_at(ActiveField::Address, layout.address, x, false);
        } else if layout.name_filter.contains(x, y) {
            self.activate_field_at(ActiveField::NameFilter, layout.name_filter, x, false);
        } else if let Some(index) = self.sidebar_index_at(x, y, layout) {
            let path = self.sidebar[index].path.clone();
            self.navigate_to(path, true);
        } else if self.scrollbar_track_contains(x, y, layout) {
            self.start_scrollbar_drag(y, layout);
        } else if let Some(index) = self.entry_index_at(x, y, layout) {
            self.select_entry(index);
            self.deactivate_fields();
        } else if layout.filename.contains(x, y) {
            self.activate_field_at(ActiveField::Filename, layout.filename, x, false);
        } else if layout.filter.contains(x, y) {
            self.show_filter_dropdown = !self.show_filter_dropdown;
            self.deactivate_fields();
        } else if layout.new_folder.contains(x, y) {
            self.create_new_folder();
        } else if layout.cancel.contains(x, y) {
            return FileExplorerResult::Close;
        } else if layout.primary.contains(x, y) {
            return self.complete_selection();
        } else {
            self.deactivate_fields();
        }
        FileExplorerResult::Consumed
    }

    fn handle_double_click(
        &mut self,
        x: f32,
        y: f32,
        layout: &ExplorerLayout,
    ) -> FileExplorerResult {
        if layout.address.contains(x, y) {
            self.activate_field_at(ActiveField::Address, layout.address, x, true);
            return FileExplorerResult::Consumed;
        }
        if layout.name_filter.contains(x, y) {
            self.activate_field_at(ActiveField::NameFilter, layout.name_filter, x, true);
            return FileExplorerResult::Consumed;
        }
        if layout.filename.contains(x, y) {
            self.activate_field_at(ActiveField::Filename, layout.filename, x, true);
            return FileExplorerResult::Consumed;
        }
        if let Some(index) = self.entry_index_at(x, y, layout) {
            self.select_entry(index);
            return self.complete_selection();
        }
        self.handle_mouse_press(x, y, layout)
    }

    fn handle_overwrite_event(
        &mut self,
        event: &UiEvent,
        layout: &ExplorerLayout,
    ) -> FileExplorerResult {
        let (prompt, cancel, overwrite) = overwrite_rects(layout.card);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.overwrite_path = None;
                FileExplorerResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => self.confirm_overwrite(),
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if overwrite.contains(*x, *y) {
                    self.confirm_overwrite()
                } else if cancel.contains(*x, *y) || !prompt.contains(*x, *y) {
                    self.overwrite_path = None;
                    FileExplorerResult::Consumed
                } else {
                    FileExplorerResult::Consumed
                }
            }
            _ => FileExplorerResult::Consumed,
        }
    }

    fn confirm_overwrite(&mut self) -> FileExplorerResult {
        let Some(path) = self.overwrite_path.take() else {
            return FileExplorerResult::Consumed;
        };
        FileExplorerResult::Selected {
            intent: self.intent.clone(),
            path,
        }
    }

    fn complete_selection(&mut self) -> FileExplorerResult {
        let Some(mut path) = self.candidate_path() else {
            self.error = Some(t("file_explorer.error.filename_required").to_string());
            self.status_text = t("file_explorer.error.filename_required").to_string();
            return FileExplorerResult::Consumed;
        };

        match self.mode {
            FileExplorerMode::Open => {
                if path.is_dir() {
                    self.navigate_to(path, true);
                    return FileExplorerResult::Consumed;
                }
                if !path.is_file() {
                    self.error = Some(t("file_explorer.error.file_missing").to_string());
                    self.status_text = t("file_explorer.error.file_missing").to_string();
                    return FileExplorerResult::Consumed;
                }
                FileExplorerResult::Selected {
                    intent: self.intent.clone(),
                    path,
                }
            }
            FileExplorerMode::Save => {
                if path.is_dir() {
                    self.navigate_to(path, true);
                    return FileExplorerResult::Consumed;
                }
                if let Some(default_extension) = &self.default_extension {
                    if path.extension().is_none() && !default_extension.is_empty() {
                        path.set_extension(default_extension);
                    }
                }
                let Some(parent) = path.parent() else {
                    self.error = Some(t("file_explorer.error.invalid_path").to_string());
                    self.status_text = t("file_explorer.error.invalid_path").to_string();
                    return FileExplorerResult::Consumed;
                };
                if !parent.is_dir() {
                    self.error = Some(t("file_explorer.error.folder_missing").to_string());
                    self.status_text = t("file_explorer.error.folder_missing").to_string();
                    return FileExplorerResult::Consumed;
                }
                if path.exists() {
                    self.overwrite_path = Some(path);
                    return FileExplorerResult::Consumed;
                }
                FileExplorerResult::Selected {
                    intent: self.intent.clone(),
                    path,
                }
            }
        }
    }

    fn candidate_path(&self) -> Option<PathBuf> {
        let typed = self
            .filename
            .trim()
            .trim_matches(|ch| ch == '"' || ch == '\'');
        if !typed.is_empty() {
            let path = PathBuf::from(typed);
            return Some(if path.is_absolute() {
                path
            } else {
                self.current_dir.join(path)
            });
        }

        self.selected
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.path.clone())
    }

    fn submit_address(&mut self) {
        let typed = self
            .address
            .trim()
            .trim_matches(|ch| ch == '"' || ch == '\'');
        if typed.is_empty() {
            return;
        }
        let path = PathBuf::from(typed);
        let path = if path.is_absolute() {
            path
        } else {
            self.current_dir.join(path)
        };
        if path.is_dir() {
            self.navigate_to(path, true);
        } else if path.is_file() {
            if let Some(parent) = path.parent() {
                self.filename = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.filename_input.set_text_external(&self.filename);
                self.navigate_to(parent.to_path_buf(), true);
            }
        } else {
            self.error = Some(t("file_explorer.error.folder_missing").to_string());
            self.status_text = t("file_explorer.error.folder_missing").to_string();
        }
    }

    fn start_scan(&mut self) {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        let generation = self.scan_generation;
        let dir = self.current_dir.clone();
        let filter = self.filters.get(self.selected_filter).cloned();
        let (tx, rx) = mpsc::channel();
        self.scan_receiver = Some(rx);
        self.loading = true;
        self.entries.clear();
        self.selected = None;
        self.scroll_offset = 0.0;
        self.error = None;
        self.status_text = t("file_explorer.loading").to_string();
        self.address = self.current_dir.to_string_lossy().into_owned();
        self.address_input.set_text_external(&self.address);

        std::thread::spawn(move || {
            let result = scan_directory(&dir, filter.as_ref());
            let _ = tx.send(ScanMessage {
                generation,
                dir,
                result,
            });
        });
    }

    fn navigate_to(&mut self, dir: PathBuf, push_history: bool) {
        if !dir.is_dir() {
            self.error = Some(t("file_explorer.error.folder_missing").to_string());
            self.status_text = t("file_explorer.error.folder_missing").to_string();
            return;
        }
        if push_history && dir != self.current_dir {
            self.history_back.push(self.current_dir.clone());
            self.history_forward.clear();
        }
        self.current_dir = dir;
        self.filename.clear();
        self.filename_input.set_text_external("");
        self.deactivate_fields();
        self.start_scan();
    }

    fn navigate_back(&mut self) {
        let Some(previous) = self.history_back.pop() else {
            return;
        };
        self.history_forward.push(self.current_dir.clone());
        self.current_dir = previous;
        self.start_scan();
    }

    fn navigate_forward(&mut self) {
        let Some(next) = self.history_forward.pop() else {
            return;
        };
        self.history_back.push(self.current_dir.clone());
        self.current_dir = next;
        self.start_scan();
    }

    fn navigate_parent(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.navigate_to(parent.to_path_buf(), true);
        }
    }

    fn create_new_folder(&mut self) {
        let base = t("file_explorer.new_folder.default_name");
        for i in 1..1000 {
            let name = if i == 1 {
                base.to_string()
            } else {
                format!("{base} ({i})")
            };
            let path = self.current_dir.join(&name);
            if path.exists() {
                continue;
            }
            match fs::create_dir(&path) {
                Ok(()) => self.navigate_to(path, true),
                Err(error) => {
                    self.error = Some(format!(
                        "{}: {error}",
                        t("file_explorer.error.new_folder_failed")
                    ));
                    self.status_text = self.error.clone().unwrap_or_default();
                }
            }
            return;
        }
        self.error = Some(t("file_explorer.error.new_folder_failed").to_string());
        self.status_text = t("file_explorer.error.new_folder_failed").to_string();
    }

    fn update_status_text(&mut self) {
        let visible_count = self.visible_entry_count();
        if visible_count == 0 {
            self.status_text = t("file_explorer.empty").to_string();
        } else {
            self.status_text = format!("{} {}", visible_count, t("file_explorer.status.items"));
        }
    }

    fn name_filter_query(&self) -> String {
        self.name_filter.trim().to_lowercase()
    }

    fn visible_entry_indices(&self) -> Vec<usize> {
        let query = self.name_filter_query();
        if query.is_empty() {
            return (0..self.entries.len()).collect();
        }
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.sort_name.contains(&query).then_some(index))
            .collect()
    }

    fn visible_entry_count(&self) -> usize {
        let query = self.name_filter_query();
        if query.is_empty() {
            self.entries.len()
        } else {
            self.entries
                .iter()
                .filter(|entry| entry.sort_name.contains(&query))
                .count()
        }
    }

    fn visible_position_of(&self, entry_index: usize) -> Option<usize> {
        self.visible_entry_indices()
            .into_iter()
            .position(|index| index == entry_index)
    }

    fn max_scroll(&self, layout: &ExplorerLayout) -> f32 {
        (self.visible_entry_count() as f32 * ROW_H - layout.rows.height).max(0.0)
    }

    fn select_entry(&mut self, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        self.selected = Some(index);
        if !self.entries[index].is_dir {
            self.filename = self.entries[index].name.clone();
            self.filename_input.set_text_external(&self.filename);
        } else {
            self.filename.clear();
            self.filename_input.set_text_external("");
        }
        self.ensure_selection_visible(index);
    }

    fn move_selection(&mut self, direction: i32, layout: &ExplorerLayout) {
        let visible_indices = self.visible_entry_indices();
        if visible_indices.is_empty() {
            return;
        }
        self.deactivate_fields();
        let current_pos = self
            .selected
            .and_then(|selected| visible_indices.iter().position(|index| *index == selected))
            .unwrap_or(if direction > 0 {
                0
            } else {
                visible_indices.len() - 1
            });
        let next_pos = if direction < 0 {
            current_pos.saturating_sub(1)
        } else {
            (current_pos + 1).min(visible_indices.len() - 1)
        };
        self.select_entry(visible_indices[next_pos]);
        self.clamp_scroll(layout);
    }

    fn ensure_selection_visible(&mut self, index: usize) {
        let Some(position) = self.visible_position_of(index) else {
            return;
        };
        let y = position as f32 * ROW_H;
        if y < self.scroll_offset {
            self.scroll_offset = y;
        }
    }

    fn scroll_rows(&mut self, delta: f32, layout: &ExplorerLayout) {
        let max_scroll = self.max_scroll(layout);
        self.scroll_offset = (self.scroll_offset - delta * 32.0).clamp(0.0, max_scroll);
    }

    fn clamp_scroll(&mut self, layout: &ExplorerLayout) {
        let max_scroll = self.max_scroll(layout);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);
        if let Some(index) = self.selected {
            let Some(position) = self.visible_position_of(index) else {
                return;
            };
            let y = position as f32 * ROW_H;
            if y + ROW_H > self.scroll_offset + layout.rows.height {
                self.scroll_offset = (y + ROW_H - layout.rows.height).clamp(0.0, max_scroll);
            }
        }
    }

    fn entry_index_at(&self, x: f32, y: f32, layout: &ExplorerLayout) -> Option<usize> {
        if !layout.rows.contains(x, y) {
            return None;
        }
        let row_index = ((y - layout.rows.y + self.scroll_offset) / ROW_H) as usize;
        self.visible_entry_indices().get(row_index).copied()
    }

    fn scrollbar_thumb_rect(&self, layout: &ExplorerLayout) -> Option<Rect> {
        let content_h = self.visible_entry_count() as f32 * ROW_H;
        if content_h <= layout.rows.height {
            return None;
        }
        let max_scroll = self.max_scroll(layout);
        let thumb_h = (layout.scrollbar.height * layout.rows.height / content_h)
            .clamp(24.0, layout.scrollbar.height);
        let travel = (layout.scrollbar.height - thumb_h).max(1.0);
        let thumb_y = layout.scrollbar.y + (self.scroll_offset / max_scroll.max(1.0)) * travel;
        Some(Rect {
            x: layout.scrollbar.x,
            y: thumb_y,
            width: layout.scrollbar.width,
            height: thumb_h,
        })
    }

    fn scrollbar_track_contains(&self, x: f32, y: f32, layout: &ExplorerLayout) -> bool {
        self.scrollbar_thumb_rect(layout).is_some() && layout.scrollbar.contains(x, y)
    }

    fn start_scrollbar_drag(&mut self, y: f32, layout: &ExplorerLayout) {
        let Some(thumb) = self.scrollbar_thumb_rect(layout) else {
            return;
        };
        if !thumb.contains(layout.scrollbar.x + layout.scrollbar.width / 2.0, y) {
            self.set_scroll_from_thumb_center(y, layout, thumb.height);
        }
        self.dragging_scrollbar = true;
        self.scrollbar_drag_anchor_y = y;
        self.scrollbar_drag_anchor_offset = self.scroll_offset;
    }

    fn drag_scrollbar(&mut self, y: f32, layout: &ExplorerLayout) {
        let Some(thumb) = self.scrollbar_thumb_rect(layout) else {
            self.dragging_scrollbar = false;
            return;
        };
        let max_scroll = self.max_scroll(layout);
        let travel = (layout.scrollbar.height - thumb.height).max(1.0);
        let delta_y = y - self.scrollbar_drag_anchor_y;
        self.scroll_offset = (self.scrollbar_drag_anchor_offset + delta_y / travel * max_scroll)
            .clamp(0.0, max_scroll);
    }

    fn set_scroll_from_thumb_center(
        &mut self,
        center_y: f32,
        layout: &ExplorerLayout,
        thumb_h: f32,
    ) {
        let max_scroll = self.max_scroll(layout);
        let travel = (layout.scrollbar.height - thumb_h).max(1.0);
        let rel = (center_y - layout.scrollbar.y - thumb_h / 2.0).clamp(0.0, travel);
        self.scroll_offset = (rel / travel * max_scroll).clamp(0.0, max_scroll);
    }

    fn sidebar_index_at(&self, x: f32, y: f32, layout: &ExplorerLayout) -> Option<usize> {
        if !layout.sidebar.contains(x, y) {
            return None;
        }
        let index = ((y - layout.sidebar.y) / 30.0) as usize;
        (index < self.sidebar.len()).then_some(index)
    }

    fn filter_index_at(&self, x: f32, y: f32, layout: &ExplorerLayout) -> Option<usize> {
        let rect = Rect {
            x: layout.filter.x,
            y: layout.filter.y - self.filters.len() as f32 * 28.0,
            width: layout.filter.width,
            height: self.filters.len() as f32 * 28.0,
        };
        if !rect.contains(x, y) {
            return None;
        }
        let index = ((y - rect.y) / 28.0) as usize;
        (index < self.filters.len()).then_some(index)
    }

    fn activate_field(&mut self, field: ActiveField) {
        self.active_field = Some(field);
        match field {
            ActiveField::Address => {
                self.name_filter_input.deactivate();
                self.filename_input.deactivate();
                self.address_input.activate(&self.address);
            }
            ActiveField::NameFilter => {
                self.address_input.deactivate();
                self.filename_input.deactivate();
                self.name_filter_input.activate(&self.name_filter);
            }
            ActiveField::Filename => {
                self.address_input.deactivate();
                self.name_filter_input.deactivate();
                self.filename_input.activate(&self.filename);
            }
        }
    }

    fn activate_field_at(&mut self, field: ActiveField, rect: Rect, x: f32, select_all: bool) {
        self.activate_field(field);
        let pos = match field {
            ActiveField::Address => {
                text_input::cursor_pos_from_x(&self.address, rect, x, text_field_metrics())
            }
            ActiveField::NameFilter => {
                text_input::cursor_pos_from_x(&self.name_filter, rect, x, text_field_metrics())
            }
            ActiveField::Filename => {
                text_input::cursor_pos_from_x(&self.filename, rect, x, text_field_metrics())
            }
        };
        match field {
            ActiveField::Address => {
                if select_all {
                    self.address_input.select_all(&self.address);
                } else {
                    self.address_input.set_cursor_pos(pos);
                }
            }
            ActiveField::NameFilter => {
                if select_all {
                    self.name_filter_input.select_all(&self.name_filter);
                } else {
                    self.name_filter_input.set_cursor_pos(pos);
                }
            }
            ActiveField::Filename => {
                if select_all {
                    self.filename_input.select_all(&self.filename);
                } else {
                    self.filename_input.set_cursor_pos(pos);
                }
            }
        }
    }

    fn deactivate_fields(&mut self) {
        self.active_field = None;
        self.address_input.deactivate();
        self.name_filter_input.deactivate();
        self.filename_input.deactivate();
    }

    fn toggle_focus(&mut self) {
        match self.active_field {
            Some(ActiveField::Address) => self.activate_field(ActiveField::NameFilter),
            Some(ActiveField::NameFilter) => self.activate_field(ActiveField::Filename),
            _ => self.activate_field(ActiveField::Address),
        }
    }

    fn edit_active(&mut self, text: &str) {
        match self.active_field {
            Some(ActiveField::Address) => {
                let current = self.address.clone();
                if let Some(TextInputAction::Changed(value)) =
                    self.address_input.handle_key(text, &current)
                {
                    self.address = sanitize_text(value, 2048);
                }
            }
            Some(ActiveField::NameFilter) => {
                let current = self.name_filter.clone();
                if let Some(TextInputAction::Changed(value)) =
                    self.name_filter_input.handle_key(text, &current)
                {
                    self.name_filter = sanitize_text(value, 256);
                    self.selected = None;
                    self.scroll_offset = 0.0;
                    self.update_status_text();
                }
            }
            Some(ActiveField::Filename) => {
                let current = self.filename.clone();
                if let Some(TextInputAction::Changed(value)) =
                    self.filename_input.handle_key(text, &current)
                {
                    self.filename = sanitize_text(value, 1024);
                }
            }
            None => {}
        }
    }

    fn move_cursor_active(&mut self, direction: i32, shift: bool) {
        match self.active_field {
            Some(ActiveField::Address) => {
                if direction < 0 {
                    if shift {
                        self.address_input.move_left_shift();
                    } else {
                        self.address_input.move_left();
                    }
                } else if shift {
                    self.address_input.move_right_shift(&self.address);
                } else {
                    self.address_input.move_right(&self.address);
                }
            }
            Some(ActiveField::NameFilter) => {
                if direction < 0 {
                    if shift {
                        self.name_filter_input.move_left_shift();
                    } else {
                        self.name_filter_input.move_left();
                    }
                } else if shift {
                    self.name_filter_input.move_right_shift(&self.name_filter);
                } else {
                    self.name_filter_input.move_right(&self.name_filter);
                }
            }
            Some(ActiveField::Filename) => {
                if direction < 0 {
                    if shift {
                        self.filename_input.move_left_shift();
                    } else {
                        self.filename_input.move_left();
                    }
                } else if shift {
                    self.filename_input.move_right_shift(&self.filename);
                } else {
                    self.filename_input.move_right(&self.filename);
                }
            }
            None => {}
        }
    }

    fn select_all_active(&mut self) {
        match self.active_field {
            Some(ActiveField::Address) => self.address_input.select_all(&self.address),
            Some(ActiveField::NameFilter) => self.name_filter_input.select_all(&self.name_filter),
            Some(ActiveField::Filename) => self.filename_input.select_all(&self.filename),
            None => {}
        }
    }

    fn copy_selection(&self) -> Option<String> {
        match self.active_field {
            Some(ActiveField::Address) => self.address_input.selected_text(&self.address),
            Some(ActiveField::NameFilter) => {
                self.name_filter_input.selected_text(&self.name_filter)
            }
            Some(ActiveField::Filename) => self.filename_input.selected_text(&self.filename),
            None => None,
        }
    }

    fn cut_selection(&mut self) -> Option<String> {
        let text = self.copy_selection()?;
        self.edit_active("\x08");
        Some(text)
    }

    fn undo_active(&mut self) {
        match self.active_field {
            Some(ActiveField::Address) => {
                if let Some(value) = self.address_input.undo(&self.address) {
                    self.address = value;
                }
            }
            Some(ActiveField::NameFilter) => {
                if let Some(value) = self.name_filter_input.undo(&self.name_filter) {
                    self.name_filter = value;
                    self.selected = None;
                    self.scroll_offset = 0.0;
                    self.update_status_text();
                }
            }
            Some(ActiveField::Filename) => {
                if let Some(value) = self.filename_input.undo(&self.filename) {
                    self.filename = value;
                }
            }
            None => {}
        }
    }

    fn render_toolbar<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        render_small_button(
            quads,
            labels,
            layout.back,
            "<",
            self.history_back.is_empty(),
        );
        render_small_button(
            quads,
            labels,
            layout.forward,
            ">",
            self.history_forward.is_empty(),
        );
        render_small_button(
            quads,
            labels,
            layout.up,
            "^",
            self.current_dir.parent().is_none(),
        );
        render_small_button(quads, labels, layout.refresh, "R", false);
        self.render_text_field(
            quads,
            labels,
            layout.address,
            &self.address,
            &self.address_input,
            self.active_field == Some(ActiveField::Address),
        );
        self.render_text_field(
            quads,
            labels,
            layout.name_filter,
            &self.name_filter,
            &self.name_filter_input,
            self.active_field == Some(ActiveField::NameFilter),
        );
        if self.name_filter.is_empty() {
            labels.push(LabelInfo {
                text: t("file_explorer.search_placeholder"),
                bounds: layout.name_filter,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 8.0,
                font_size_override: Some(12.0),
                color_override: Some([130, 132, 148]),
                font_family_override: None,
            });
        }
    }

    fn render_sidebar<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        quads.push(quad(
            layout.sidebar,
            [0.10, 0.10, 0.13, 1.0],
            [0.08, 0.08, 0.10, 1.0],
            [0.30, 0.30, 0.36, 0.55],
            1.0,
            6.0,
        ));
        for (i, item) in self.sidebar.iter().enumerate() {
            let row = Rect {
                x: layout.sidebar.x + 4.0,
                y: layout.sidebar.y + i as f32 * 30.0 + 4.0,
                width: layout.sidebar.width - 8.0,
                height: 26.0,
            };
            if row.y + row.height > layout.sidebar.y + layout.sidebar.height {
                break;
            }
            if paths_equal(&item.path, &self.current_dir) {
                quads.push(quad(
                    row,
                    [0.30, 0.28, 0.55, 0.65],
                    [0.30, 0.28, 0.55, 0.65],
                    [0.0; 4],
                    0.0,
                    4.0,
                ));
            }
            labels.push(LabelInfo {
                text: &item.label,
                bounds: row,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 9.0,
                font_size_override: Some(12.0),
                color_override: Some([218, 218, 230]),
                font_family_override: None,
            });
        }
    }

    fn render_list<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        quads.push(quad(
            layout.list,
            [0.08, 0.08, 0.10, 1.0],
            [0.07, 0.07, 0.09, 1.0],
            [0.30, 0.30, 0.36, 0.6],
            1.0,
            6.0,
        ));
        quads.push(quad(
            Rect {
                x: layout.list.x,
                y: layout.list.y,
                width: layout.list.width,
                height: HEADER_H,
            },
            [0.14, 0.14, 0.17, 1.0],
            [0.12, 0.12, 0.15, 1.0],
            [0.0; 4],
            0.0,
            6.0,
        ));
        let name_w = layout.rows.width * 0.48;
        let modified_w = layout.rows.width * 0.22;
        let type_w = layout.rows.width * 0.18;
        self.render_header_label(
            labels,
            layout.rows.x + 12.0,
            layout.list.y,
            name_w,
            t("file_explorer.column.name"),
        );
        self.render_header_label(
            labels,
            layout.rows.x + name_w,
            layout.list.y,
            modified_w,
            t("file_explorer.column.modified"),
        );
        self.render_header_label(
            labels,
            layout.rows.x + name_w + modified_w,
            layout.list.y,
            type_w,
            t("file_explorer.column.type"),
        );
        self.render_header_label(
            labels,
            layout.rows.x + layout.rows.width - 88.0,
            layout.list.y,
            76.0,
            t("file_explorer.column.size"),
        );

        if self.loading {
            labels.push(LabelInfo {
                text: t("file_explorer.loading"),
                bounds: layout.rows,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(13.0),
                color_override: Some([170, 172, 188]),
                font_family_override: None,
            });
            return;
        }

        let visible_indices = self.visible_entry_indices();
        if visible_indices.is_empty() {
            let text = self
                .error
                .as_deref()
                .unwrap_or_else(|| t("file_explorer.empty"));
            labels.push(LabelInfo {
                text,
                bounds: layout.rows,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 16.0,
                font_size_override: Some(13.0),
                color_override: Some([170, 172, 188]),
                font_family_override: None,
            });
            return;
        }

        let first = (self.scroll_offset / ROW_H) as usize;
        let visible = (layout.rows.height / ROW_H) as usize + 2;
        for position in first..visible_indices.len().min(first + visible) {
            let index = visible_indices[position];
            let row_y = layout.rows.y + position as f32 * ROW_H - self.scroll_offset;
            let clipped_y = row_y.max(layout.rows.y);
            let clipped_bottom = (row_y + ROW_H).min(layout.rows.y + layout.rows.height);
            let clipped_h = clipped_bottom - clipped_y;
            if clipped_h <= 0.0 {
                continue;
            }
            let row = Rect {
                x: layout.rows.x + 2.0,
                y: clipped_y,
                width: layout.rows.width - 4.0,
                height: clipped_h,
            };
            if self.selected == Some(index) {
                quads.push(quad(
                    row,
                    [0.30, 0.28, 0.55, 0.62],
                    [0.30, 0.28, 0.55, 0.62],
                    [0.50, 0.48, 0.80, 0.55],
                    1.0,
                    4.0,
                ));
            } else if index % 2 == 1 {
                quads.push(quad(
                    row,
                    [0.10, 0.10, 0.12, 0.55],
                    [0.10, 0.10, 0.12, 0.55],
                    [0.0; 4],
                    0.0,
                    0.0,
                ));
            }

            let entry = &self.entries[index];
            let icon = if entry.is_dir { "[D]" } else { "[F]" };
            labels.push(LabelInfo {
                text: icon,
                bounds: Rect {
                    x: layout.rows.x + 10.0,
                    y: clipped_y,
                    width: 30.0,
                    height: clipped_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some(if entry.is_dir {
                    [245, 198, 96]
                } else {
                    [160, 188, 240]
                }),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &entry.name,
                bounds: Rect {
                    x: layout.rows.x + 40.0,
                    y: clipped_y,
                    width: name_w - 44.0,
                    height: clipped_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: None,
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &entry.modified_text,
                bounds: Rect {
                    x: layout.rows.x + name_w,
                    y: clipped_y,
                    width: modified_w - 8.0,
                    height: clipped_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([170, 172, 188]),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &entry.type_text,
                bounds: Rect {
                    x: layout.rows.x + name_w + modified_w,
                    y: clipped_y,
                    width: type_w - 8.0,
                    height: clipped_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([170, 172, 188]),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: &entry.size_text,
                bounds: Rect {
                    x: layout.rows.x + layout.rows.width - 90.0,
                    y: clipped_y,
                    width: 78.0,
                    height: clipped_h,
                },
                h_align: HAlign::Right,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(11.0),
                color_override: Some([170, 172, 188]),
                font_family_override: None,
            });
        }
        self.render_scrollbar(quads, layout);
    }

    fn render_scrollbar(&self, quads: &mut Vec<QuadInstance>, layout: &ExplorerLayout) {
        let Some(thumb) = self.scrollbar_thumb_rect(layout) else {
            return;
        };
        quads.push(quad(
            layout.scrollbar,
            [0.05, 0.05, 0.07, 0.75],
            [0.05, 0.05, 0.07, 0.75],
            [0.0; 4],
            0.0,
            4.0,
        ));
        quads.push(quad(
            thumb,
            if self.dragging_scrollbar {
                [0.48, 0.46, 0.70, 1.0]
            } else {
                [0.34, 0.34, 0.44, 1.0]
            },
            if self.dragging_scrollbar {
                [0.38, 0.36, 0.58, 1.0]
            } else {
                [0.27, 0.27, 0.36, 1.0]
            },
            [0.60, 0.60, 0.75, 0.45],
            1.0,
            4.0,
        ));
    }

    fn render_header_label<'a>(
        &'a self,
        labels: &mut Vec<LabelInfo<'a>>,
        x: f32,
        y: f32,
        width: f32,
        text: &'a str,
    ) {
        labels.push(LabelInfo {
            text,
            bounds: Rect {
                x,
                y,
                width,
                height: HEADER_H,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([150, 152, 168]),
            font_family_override: None,
        });
    }

    fn render_footer<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        labels.push(LabelInfo {
            text: t("file_explorer.filename"),
            bounds: Rect {
                x: layout.filename.x - 70.0,
                y: layout.filename.y,
                width: 62.0,
                height: layout.filename.height,
            },
            h_align: HAlign::Right,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([190, 190, 205]),
            font_family_override: None,
        });
        self.render_text_field(
            quads,
            labels,
            layout.filename,
            &self.filename,
            &self.filename_input,
            self.active_field == Some(ActiveField::Filename),
        );

        render_filter_button(quads, labels, layout.filter, self.current_filter_name());
        render_button(
            quads,
            labels,
            layout.new_folder,
            t("file_explorer.new_folder"),
            false,
        );
        render_button(
            quads,
            labels,
            layout.cancel,
            t("file_explorer.cancel"),
            false,
        );
        render_button(quads, labels, layout.primary, self.primary_label(), true);
        labels.push(LabelInfo {
            text: &self.status_text,
            bounds: Rect {
                x: layout.new_folder.x + layout.new_folder.width + 10.0,
                y: layout.new_folder.y,
                width: layout.cancel.x - (layout.new_folder.x + layout.new_folder.width + 18.0),
                height: layout.new_folder.height,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some(if self.error.is_some() {
                [240, 126, 126]
            } else {
                [155, 157, 174]
            }),
            font_family_override: None,
        });
    }

    fn render_filter_dropdown<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        let rect = Rect {
            x: layout.filter.x,
            y: layout.filter.y - self.filters.len() as f32 * 28.0,
            width: layout.filter.width,
            height: self.filters.len() as f32 * 28.0,
        };
        quads.push(quad(
            rect,
            [0.13, 0.13, 0.16, 1.0],
            [0.10, 0.10, 0.13, 1.0],
            [0.38, 0.38, 0.46, 0.85],
            1.0,
            5.0,
        ));
        for (index, filter) in self.filters.iter().enumerate() {
            let row = Rect {
                x: rect.x,
                y: rect.y + index as f32 * 28.0,
                width: rect.width,
                height: 28.0,
            };
            if index == self.selected_filter {
                quads.push(quad(
                    Rect {
                        x: row.x + 2.0,
                        y: row.y + 2.0,
                        width: row.width - 4.0,
                        height: row.height - 4.0,
                    },
                    [0.30, 0.28, 0.55, 0.65],
                    [0.30, 0.28, 0.55, 0.65],
                    [0.0; 4],
                    0.0,
                    4.0,
                ));
            }
            labels.push(LabelInfo {
                text: &filter.name,
                bounds: row,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 9.0,
                font_size_override: Some(12.0),
                color_override: None,
                font_family_override: None,
            });
        }
    }

    fn render_overwrite_prompt<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &ExplorerLayout,
    ) {
        let (prompt, cancel, overwrite) = overwrite_rects(layout.card);
        quads.push(quad(
            layout.card,
            [0.0, 0.0, 0.0, 0.38],
            [0.0, 0.0, 0.0, 0.38],
            [0.0; 4],
            0.0,
            14.0,
        ));
        quads.push(quad(
            prompt,
            [0.23, 0.23, 0.27, 1.0],
            [0.16, 0.16, 0.20, 1.0],
            [0.56, 0.50, 0.36, 0.95],
            1.5,
            12.0,
        ));
        labels.push(LabelInfo {
            text: t("file_explorer.overwrite.title"),
            bounds: Rect {
                x: prompt.x + 20.0,
                y: prompt.y + 18.0,
                width: prompt.width - 40.0,
                height: 26.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(16.0),
            color_override: None,
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: t("file_explorer.overwrite.message"),
            bounds: Rect {
                x: prompt.x + 24.0,
                y: prompt.y + 54.0,
                width: prompt.width - 48.0,
                height: 24.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([200, 200, 216]),
            font_family_override: None,
        });
        if let Some(path) = &self.overwrite_path {
            labels.push(LabelInfo {
                text: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(""),
                bounds: Rect {
                    x: prompt.x + 24.0,
                    y: prompt.y + 78.0,
                    width: prompt.width - 48.0,
                    height: 22.0,
                },
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: Some([245, 212, 130]),
                font_family_override: None,
            });
        }
        render_button(quads, labels, cancel, t("file_explorer.cancel"), false);
        render_button(
            quads,
            labels,
            overwrite,
            t("file_explorer.overwrite.confirm"),
            true,
        );
    }

    fn render_text_field<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        rect: Rect,
        value: &'a str,
        input: &TextInputState,
        focused: bool,
    ) {
        quads.push(quad(
            rect,
            [0.07, 0.07, 0.09, 1.0],
            [0.07, 0.07, 0.09, 1.0],
            if focused {
                [0.45, 0.42, 0.85, 0.9]
            } else {
                [0.30, 0.30, 0.36, 0.65]
            },
            1.0,
            5.0,
        ));

        text_input::render_selection_and_cursor(
            quads,
            rect,
            value,
            input,
            focused,
            text_field_metrics(),
            5.0,
            6.0,
            [0.25, 0.40, 0.80, 0.55],
            [0.92, 0.92, 0.98, 1.0],
        );

        labels.push(LabelInfo {
            text: value,
            bounds: rect,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: TEXT_FIELD_PADDING_X,
            font_size_override: Some(TEXT_FIELD_FONT_SIZE),
            color_override: None,
            font_family_override: None,
        });
    }

    fn current_filter_name(&self) -> &str {
        self.filters
            .get(self.selected_filter)
            .map(|filter| filter.name.as_str())
            .unwrap_or_else(|| t("picker.filter.all_files"))
    }

    fn primary_label(&self) -> &str {
        match self.mode {
            FileExplorerMode::Open => t("file_explorer.open"),
            FileExplorerMode::Save => t("file_explorer.save"),
        }
    }
}

fn scan_directory(
    dir: &Path,
    filter: Option<&FileFilterSpec>,
) -> Result<Vec<DirectoryEntry>, String> {
    let read_dir = fs::read_dir(dir).map_err(|error| {
        format!(
            "{}: {error}",
            t("file_explorer.error.inaccessible_directory")
        )
    })?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let is_dir = metadata.is_dir();
        if !is_dir && filter.is_some_and(|filter| !filter.matches_path(&path)) {
            continue;
        }
        entries.push(directory_entry(
            path,
            metadata.len(),
            is_dir,
            metadata.modified().ok(),
        ));
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.sort_name.cmp(&b.sort_name))
    });
    Ok(entries)
}

fn directory_entry(
    path: PathBuf,
    size: u64,
    is_dir: bool,
    modified: Option<SystemTime>,
) -> DirectoryEntry {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let type_text = if is_dir {
        t("file_explorer.type.folder").to_string()
    } else {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                format!(
                    "{} {}",
                    ext.to_ascii_uppercase(),
                    t("file_explorer.type.file")
                )
            })
            .unwrap_or_else(|| t("file_explorer.type.file").to_string())
    };
    DirectoryEntry {
        path,
        sort_name: name.to_lowercase(),
        name,
        is_dir,
        modified_text: modified.map(format_modified).unwrap_or_else(|| "-".into()),
        type_text,
        size_text: if is_dir {
            String::new()
        } else {
            format_size(size)
        },
    }
}

fn resolve_initial_dir(initial_dir: Option<&Path>) -> PathBuf {
    let candidate = initial_dir
        .and_then(|path| {
            if path.is_dir() {
                Some(path.to_path_buf())
            } else {
                path.parent()
                    .filter(|parent| parent.is_dir())
                    .map(Path::to_path_buf)
            }
        })
        .or_else(|| dirs::download_dir().filter(|path| path.is_dir()))
        .or_else(|| dirs::home_dir().filter(|path| path.is_dir()))
        .or_else(|| std::env::current_dir().ok());

    candidate.unwrap_or_else(|| PathBuf::from("."))
}

fn build_sidebar(extra_locations: &[(String, PathBuf)]) -> Vec<SidebarItem> {
    let mut items = Vec::new();
    push_sidebar(
        &mut items,
        t("file_explorer.sidebar.desktop"),
        dirs::desktop_dir(),
    );
    push_sidebar(
        &mut items,
        t("file_explorer.sidebar.documents"),
        dirs::document_dir(),
    );
    push_sidebar(
        &mut items,
        t("file_explorer.sidebar.downloads"),
        dirs::download_dir(),
    );
    push_sidebar(
        &mut items,
        t("file_explorer.sidebar.home"),
        dirs::home_dir(),
    );
    for (label, path) in extra_locations {
        push_sidebar(&mut items, label, Some(path.clone()));
    }
    for drive in enumerate_drives() {
        let label = format!("{} {}", t("file_explorer.sidebar.drive"), drive.display());
        push_sidebar(&mut items, label, Some(drive));
    }
    items
}

fn push_sidebar(items: &mut Vec<SidebarItem>, label: impl Into<String>, path: Option<PathBuf>) {
    let Some(path) = path else {
        return;
    };
    if !path.is_dir() {
        return;
    }
    if items.iter().any(|item| paths_equal(&item.path, &path)) {
        return;
    }
    items.push(SidebarItem {
        label: label.into(),
        path,
    });
}

#[cfg(target_os = "windows")]
fn enumerate_drives() -> Vec<PathBuf> {
    #[link(name = "Kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|index| mask & (1_u32 << index) != 0)
        .map(|index| {
            let letter = (b'A' + index as u8) as char;
            PathBuf::from(format!("{letter}:\\"))
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn enumerate_drives() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn sanitize_text(value: String, max_len: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().filter(|ch| !ch.is_control()) {
        if out.len() + ch.len_utf8() > max_len {
            break;
        }
        out.push(ch);
    }
    out
}

fn text_field_metrics() -> TextInputMetrics {
    TextInputMetrics::left(TEXT_FIELD_FONT_SIZE, TEXT_FIELD_PADDING_X)
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} Go", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} Mo", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.0} Ko", bytes_f / KB)
    } else {
        format!("{bytes} o")
    }
}

fn format_modified(modified: SystemTime) -> String {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "-".into();
    };
    let Ok(then) = modified.duration_since(UNIX_EPOCH) else {
        return "-".into();
    };
    let elapsed = now.saturating_sub(then).as_secs();
    if elapsed < 60 {
        t("file_explorer.modified.now").to_string()
    } else if elapsed < 3600 {
        format!("{} min", elapsed / 60)
    } else if elapsed < 86_400 {
        format!("{} h", elapsed / 3600)
    } else {
        format!("{} j", elapsed / 86_400)
    }
}

fn overwrite_rects(card: Rect) -> (Rect, Rect, Rect) {
    let prompt = Rect {
        x: card.x + (card.width - 390.0) / 2.0,
        y: card.y + (card.height - 180.0) / 2.0,
        width: 390.0,
        height: 180.0,
    };
    let cancel = Rect {
        x: prompt.x + prompt.width - 226.0,
        y: prompt.y + prompt.height - 48.0,
        width: 96.0,
        height: 34.0,
    };
    let overwrite = Rect {
        x: prompt.x + prompt.width - 120.0,
        y: prompt.y + prompt.height - 48.0,
        width: 100.0,
        height: 34.0,
    };
    (prompt, cancel, overwrite)
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    color_bottom: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    radius: f32,
) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom,
        border_color,
        border_width,
        border_radius: radius,
        shadow_offset: [0.0, 3.0],
        shadow_color: [0.0, 0.0, 0.0, if radius > 8.0 { 0.34 } else { 0.0 }],
        shadow_blur: if radius > 8.0 { 10.0 } else { 0.0 },
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn render_small_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    disabled: bool,
) {
    quads.push(quad(
        rect,
        if disabled {
            [0.11, 0.11, 0.13, 0.8]
        } else {
            [0.16, 0.16, 0.20, 1.0]
        },
        if disabled {
            [0.10, 0.10, 0.12, 0.8]
        } else {
            [0.13, 0.13, 0.17, 1.0]
        },
        [0.32, 0.32, 0.39, 0.65],
        1.0,
        5.0,
    ));
    labels.push(LabelInfo {
        text,
        bounds: rect,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(13.0),
        color_override: Some(if disabled {
            [105, 106, 118]
        } else {
            [220, 220, 232]
        }),
        font_family_override: None,
    });
}

fn render_filter_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    label: &'a str,
) {
    quads.push(quad(
        rect,
        [0.12, 0.12, 0.15, 1.0],
        [0.10, 0.10, 0.13, 1.0],
        [0.32, 0.32, 0.39, 0.7],
        1.0,
        5.0,
    ));
    labels.push(LabelInfo {
        text: label,
        bounds: Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width - 24.0,
            height: rect.height,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 8.0,
        font_size_override: Some(12.0),
        color_override: None,
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: "v",
        bounds: Rect {
            x: rect.x + rect.width - 24.0,
            y: rect.y,
            width: 20.0,
            height: rect.height,
        },
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(10.0),
        color_override: Some([170, 172, 188]),
        font_family_override: None,
    });
}

fn render_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    label: &'a str,
    primary: bool,
) {
    quads.push(quad(
        rect,
        if primary {
            [0.36, 0.34, 0.72, 1.0]
        } else {
            [0.16, 0.16, 0.20, 1.0]
        },
        if primary {
            [0.26, 0.25, 0.56, 1.0]
        } else {
            [0.13, 0.13, 0.17, 1.0]
        },
        if primary {
            [0.58, 0.56, 0.92, 0.8]
        } else {
            [0.32, 0.32, 0.39, 0.65]
        },
        1.0,
        6.0,
    ));
    labels.push(LabelInfo {
        text: label,
        bounds: rect,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(12.0),
        color_override: Some([236, 236, 248]),
        font_family_override: None,
    });
}
