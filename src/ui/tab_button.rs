use super::focus::AccessibleRole;
use super::interactive::{InteractiveResult, InteractiveState};
use super::primitives::{EventResponse, LabelInfo, QuadInstance, Rect, UiEvent, Widget};

/// A workspace tab with explicit selected semantics.
pub struct TabButton {
    bounds: Rect,
    label: String,
    selected: bool,
    state: InteractiveState,
    on_activate: Box<dyn FnMut() -> EventResponse>,
}

impl TabButton {
    pub fn new(
        bounds: Rect,
        label: impl Into<String>,
        selected: bool,
        on_activate: impl FnMut() -> EventResponse + 'static,
    ) -> Self {
        Self {
            bounds,
            label: label.into(),
            selected,
            state: InteractiveState::default(),
            on_activate: Box::new(on_activate),
        }
    }

    fn activate(&mut self) -> EventResponse {
        let response = (self.on_activate)();
        if response == EventResponse::Ignored {
            EventResponse::Consumed
        } else {
            response
        }
    }
}

impl Widget for TabButton {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        if matches!(event, UiEvent::Activate) {
            return self.activate();
        }

        // Modified clicks are normalized by the event loop for the rythmo
        // workspace. Tabs remain shell controls and must keep ordinary click
        // behavior regardless of Ctrl/Shift state.
        let normalized;
        let event = match event {
            UiEvent::CtrlClick { x, y } | UiEvent::ShiftMousePress { x, y } => {
                normalized = UiEvent::MousePress { x: *x, y: *y };
                &normalized
            }
            event => event,
        };
        match self.state.handle(event, &self.bounds) {
            InteractiveResult::Clicked => self.activate(),
            InteractiveResult::StateChanged => EventResponse::Consumed,
            InteractiveResult::None => EventResponse::Ignored,
        }
    }

    fn render_quads(&self) -> Vec<QuadInstance> {
        let hovered = self.state.is_hovered();
        let background = if self.selected {
            [0.17, 0.17, 0.21, 1.0]
        } else if hovered {
            [0.15, 0.15, 0.18, 1.0]
        } else {
            [0.0; 4]
        };
        let mut quads = vec![QuadInstance {
            rect: [
                self.bounds.x,
                self.bounds.y,
                self.bounds.width,
                self.bounds.height,
            ],
            color: background,
            color_bottom: background,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 4.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }];
        if self.selected {
            quads.push(QuadInstance {
                rect: [
                    self.bounds.x + 8.0,
                    self.bounds.y + self.bounds.height - 2.0,
                    (self.bounds.width - 16.0).max(0.0),
                    2.0,
                ],
                color: [0.42, 0.55, 1.0, 1.0],
                color_bottom: [0.42, 0.55, 1.0, 1.0],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 1.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        quads
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        vec![LabelInfo {
            text: &self.label,
            bounds: self.bounds,
            h_align: super::primitives::HAlign::Center,
            v_align: super::primitives::VAlign::Center,
            overflow: super::primitives::Overflow::Ellipsis,
            padding: 8.0,
            font_size_override: Some(13.0),
            color_override: Some(if self.selected {
                [235, 236, 245]
            } else {
                [178, 180, 193]
            }),
            font_family_override: None,
        }]
    }

    fn accessible_label(&self) -> Option<&str> {
        Some(&self.label)
    }

    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Tab
    }

    fn accessible_selected(&self) -> Option<bool> {
        Some(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::workspace_service::WorkspaceId;
    use crate::ui::primitives::UiAction;

    fn tab() -> TabButton {
        TabButton::new(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 32.0,
            },
            "Recording",
            true,
            || EventResponse::Action(UiAction::ActivateWorkspace(WorkspaceId::Recording)),
        )
    }

    #[test]
    fn tab_exposes_role_and_selected_state() {
        let tab = tab();
        assert_eq!(tab.accessible_role(), AccessibleRole::Tab);
        assert_eq!(tab.accessible_selected(), Some(true));
        assert_eq!(tab.accessible_label(), Some("Recording"));
    }

    #[test]
    fn modified_pointer_click_activates_like_an_ordinary_click() {
        let mut tab = tab();
        assert_eq!(
            tab.handle_event(&UiEvent::CtrlClick { x: 10.0, y: 10.0 }),
            EventResponse::Consumed
        );
        assert_eq!(
            tab.handle_event(&UiEvent::MouseRelease { x: 10.0, y: 10.0 }),
            EventResponse::Action(UiAction::ActivateWorkspace(WorkspaceId::Recording))
        );
    }
}
