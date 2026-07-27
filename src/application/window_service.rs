//! Main and secondary window ownership.

use std::sync::Arc;

use winit::window::Window;

use crate::graphics::WindowSurface;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SecondaryWindowKind {
    Video,
    Daw,
}

pub struct WindowManager {
    pub main_window: Arc<Window>,
    pub secondary_display: Option<WindowSurface>,
    pub secondary_kind: Option<SecondaryWindowKind>,
}

impl WindowManager {
    pub fn new(main_window: Arc<Window>) -> Self {
        Self {
            main_window,
            secondary_display: None,
            secondary_kind: None,
        }
    }
}
