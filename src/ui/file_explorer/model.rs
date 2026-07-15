//! Navigation model and asynchronous filesystem loading for the explorer.
#![allow(clippy::while_let_loop)]

use std::sync::mpsc::TryRecvError;

use crate::i18n::t;

use super::{ActiveField, FileExplorerModal, FileExplorerMode, FileExplorerRequest};

impl FileExplorerModal {
    pub fn new(request: FileExplorerRequest) -> Self {
        let current_dir = super::resolve_initial_dir(request.initial_dir.as_deref());
        let address = current_dir.to_string_lossy().into_owned();
        let filename = request.initial_filename.unwrap_or_default();
        let sidebar = super::build_sidebar(&request.extra_locations);
        let mut modal = Self {
            title: request.title,
            mode: request.mode,
            intent: request.intent,
            filters: request.filters,
            selected_filter: 0,
            default_extension: request
                .default_extension
                .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase()),
            current_dir,
            address,
            name_filter: String::new(),
            filename,
            entries: Vec::new(),
            selected: None,
            scroll_offset: 0.0,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            sidebar,
            scan_generation: 0,
            scan_receiver: None,
            loading: false,
            error: None,
            status_text: t("file_explorer.loading").to_string(),
            active_field: None,
            address_input: super::TextInputState::new(),
            name_filter_input: super::TextInputState::new(),
            filename_input: super::TextInputState::new(),
            filename_suggestion: None,
            show_filter_dropdown: false,
            overwrite_path: None,
            dragging_scrollbar: false,
            scrollbar_drag_anchor_y: 0.0,
            scrollbar_drag_anchor_offset: 0.0,
        };
        if modal.mode == FileExplorerMode::Save {
            modal.activate_field(ActiveField::Filename);
        }
        modal.start_scan();
        modal
    }

    pub fn needs_background_poll(&self) -> bool {
        self.scan_receiver.is_some()
    }

    pub fn is_editing_text(&self) -> bool {
        true
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        match self.active_field {
            Some(ActiveField::Address) => self.address_input.next_cursor_blink_deadline(),
            Some(ActiveField::NameFilter) => self.name_filter_input.next_cursor_blink_deadline(),
            Some(ActiveField::Filename) => self.filename_input.next_cursor_blink_deadline(),
            None => None,
        }
    }

    pub fn poll_background(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(receiver) = self.scan_receiver.as_ref() else {
                break;
            };
            let message = match receiver.try_recv() {
                Ok(message) => Some(Ok(message)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            };

            match message {
                Some(Ok(message)) => {
                    if message.generation == self.scan_generation && message.dir == self.current_dir
                    {
                        self.scan_receiver = None;
                        self.loading = false;
                        match message.result {
                            Ok(entries) => {
                                self.entries = entries;
                                self.selected = None;
                                self.scroll_offset = 0.0;
                                self.error = None;
                                self.update_status_text();
                            }
                            Err(error) => {
                                self.entries.clear();
                                self.selected = None;
                                self.scroll_offset = 0.0;
                                self.error = Some(error.clone());
                                self.status_text = error;
                            }
                        }
                        changed = true;
                    }
                }
                Some(Err(())) => {
                    self.scan_receiver = None;
                    self.loading = false;
                    self.entries.clear();
                    let error = t("file_explorer.error.scan_disconnected").to_string();
                    self.error = Some(error.clone());
                    self.status_text = error;
                    changed = true;
                }
                None => break,
            }
        }
        changed
    }
}
