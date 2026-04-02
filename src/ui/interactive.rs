use super::widget::{Rect, UiEvent};

/// Shared state machine for any clickable/hoverable element.
/// Replaces duplicated Normal/Hovered/Pressed logic across widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractiveState {
    Normal,
    Hovered,
    Pressed,
}

impl Default for InteractiveState {
    fn default() -> Self {
        Self::Normal
    }
}

pub enum InteractiveResult {
    /// State changed visually, needs redraw.
    StateChanged,
    /// User completed a click (press + release inside bounds).
    Clicked,
    /// Nothing happened.
    None,
}

impl InteractiveState {
    /// Process a UI event against the given bounds.
    /// Returns what happened so the caller can decide the EventResponse.
    pub fn handle(&mut self, event: &UiEvent, bounds: &Rect) -> InteractiveResult {
        match event {
            UiEvent::MouseMove { x, y } => {
                let inside = bounds.contains(*x, *y);
                let new = if inside && *self == Self::Pressed {
                    Self::Pressed
                } else if inside {
                    Self::Hovered
                } else {
                    Self::Normal
                };
                if new != *self {
                    *self = new;
                    InteractiveResult::StateChanged
                } else {
                    InteractiveResult::None
                }
            }
            UiEvent::MousePress { x, y } => {
                if bounds.contains(*x, *y) {
                    *self = Self::Pressed;
                    InteractiveResult::StateChanged
                } else {
                    InteractiveResult::None
                }
            }
            UiEvent::MouseRelease { x, y } => {
                if *self == Self::Pressed && bounds.contains(*x, *y) {
                    *self = Self::Hovered;
                    InteractiveResult::Clicked
                } else {
                    let new = if bounds.contains(*x, *y) {
                        Self::Hovered
                    } else {
                        Self::Normal
                    };
                    if new != *self {
                        *self = new;
                        InteractiveResult::StateChanged
                    } else {
                        InteractiveResult::None
                    }
                }
            }
            _ => InteractiveResult::None,
        }
    }

    pub fn is_hovered(&self) -> bool {
        matches!(self, Self::Hovered | Self::Pressed)
    }
}
