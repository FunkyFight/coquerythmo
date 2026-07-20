//! Main and secondary window ownership.

use std::sync::Arc;

use winit::window::Window;

use crate::graphics::WindowSurface;

pub struct WindowManager {
    pub main_window: Arc<Window>,
    pub secondary_display: Option<WindowSurface>,
}

impl WindowManager {
    pub fn new(main_window: Arc<Window>) -> Self {
        Self {
            main_window,
            secondary_display: None,
        }
    }
}
