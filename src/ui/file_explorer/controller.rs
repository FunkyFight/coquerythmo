//! Event controller for the file explorer modal.
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::{FileExplorerModal, FileExplorerResult};
use crate::ui::primitives::UiEvent;

impl FileExplorerModal {
    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> FileExplorerResult {
        self.poll_background();
        let layout = Self::layout(screen_w, screen_h);

        if self.overwrite_path.is_some() {
            return self.handle_overwrite_event(event, &layout);
        }

        match event {
            UiEvent::MouseMove { y, .. } if self.dragging_scrollbar => {
                self.drag_scrollbar(*y, &layout);
                return FileExplorerResult::Consumed;
            }
            UiEvent::MouseRelease { .. } if self.dragging_scrollbar => {
                self.dragging_scrollbar = false;
                return FileExplorerResult::Consumed;
            }
            _ => {}
        }

        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !layout.card.contains(*x, *y) {
                    return FileExplorerResult::Consumed;
                }
            }
            _ => {}
        }

        match event {
            UiEvent::KeyInput { text } => self.handle_key(text),
            UiEvent::CursorLeft => {
                self.move_cursor_active(-1, false);
                FileExplorerResult::Consumed
            }
            UiEvent::CursorRight => {
                self.move_cursor_active(1, false);
                FileExplorerResult::Consumed
            }
            UiEvent::ShiftCursorLeft => {
                self.move_cursor_active(-1, true);
                FileExplorerResult::Consumed
            }
            UiEvent::ShiftCursorRight => {
                self.move_cursor_active(1, true);
                FileExplorerResult::Consumed
            }
            UiEvent::CursorUp => {
                if !self.move_filename_suggestion(-1) {
                    self.move_selection(-1, &layout);
                }
                FileExplorerResult::Consumed
            }
            UiEvent::CursorDown => {
                if !self.move_filename_suggestion(1) {
                    self.move_selection(1, &layout);
                }
                FileExplorerResult::Consumed
            }
            UiEvent::Delete => {
                self.edit_active("\x7f");
                FileExplorerResult::Consumed
            }
            UiEvent::SelectAll => {
                self.select_all_active();
                FileExplorerResult::Consumed
            }
            UiEvent::Copy => self
                .copy_selection()
                .map(FileExplorerResult::Clipboard)
                .unwrap_or(FileExplorerResult::Consumed),
            UiEvent::Cut => self
                .cut_selection()
                .map(FileExplorerResult::Clipboard)
                .unwrap_or(FileExplorerResult::Consumed),
            UiEvent::UndoTextEdit => {
                self.undo_active();
                FileExplorerResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } => {
                if layout.rows.contains(*x, *y) {
                    self.scroll_rows(*delta, &layout);
                }
                FileExplorerResult::Consumed
            }
            UiEvent::MousePress { x, y } => self.handle_mouse_press(*x, *y, &layout),
            UiEvent::DoubleClick { x, y } => self.handle_double_click(*x, *y, &layout),
            UiEvent::MouseMove { .. } | UiEvent::MouseRelease { .. } => {
                FileExplorerResult::Consumed
            }
            _ => FileExplorerResult::Consumed,
        }
    }
}
