//! Main/secondary window and studio-mode state.

use std::sync::Arc;

use winit::window::{Fullscreen, Window};

use crate::graphics::WindowSurface;

pub struct WindowManager {
    pub main_window: Arc<Window>,
    pub secondary_display: Option<WindowSurface>,
    pub studio_mode: bool,
    pub fullscreen_before_studio: Option<Fullscreen>,
    pub show_studio_warning: bool,
}

impl WindowManager {
    pub fn new(main_window: Arc<Window>) -> Self {
        Self {
            main_window,
            secondary_display: None,
            studio_mode: false,
            fullscreen_before_studio: None,
            show_studio_warning: false,
        }
    }
}
