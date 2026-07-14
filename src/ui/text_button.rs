use super::interactive::{InteractiveResult, InteractiveState};
use super::primitives::{EventResponse, LabelInfo, QuadInstance, Rect, UiEvent, Widget};
use super::theme;

pub struct TextButton {
    bounds: Rect,
    label: String,
    tooltip_text: Option<String>,
    state: InteractiveState,
    accent: bool,
    on_click: Box<dyn FnMut() -> EventResponse>,
}

impl TextButton {
    pub fn new(
        bounds: Rect,
        label: impl Into<String>,
        on_click: impl FnMut() -> EventResponse + 'static,
    ) -> Self {
        Self {
            bounds,
            label: label.into(),
            tooltip_text: None,
            state: InteractiveState::default(),
            accent: false,
            on_click: Box::new(on_click),
        }
    }

    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip_text = Some(text.into());
        self
    }

    pub fn with_accent(mut self) -> Self {
        self.accent = true;
        self
    }
}

impl Widget for TextButton {
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn tooltip(&self) -> Option<&str> {
        self.tooltip_text.as_deref()
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match self.state.handle(event, &self.bounds) {
            InteractiveResult::Clicked => {
                let r = (self.on_click)();
                if r != EventResponse::Ignored {
                    r
                } else {
                    EventResponse::Consumed
                }
            }
            InteractiveResult::StateChanged => EventResponse::Consumed,
            InteractiveResult::None => EventResponse::Ignored,
        }
    }

    fn render_quads(&self) -> Vec<QuadInstance> {
        let (bg, border) = match self.state {
            InteractiveState::Normal => {
                if self.accent {
                    ([0.16, 0.15, 0.26, 1.0], [0.45, 0.40, 0.85, 0.9])
                } else {
                    ([0.18, 0.18, 0.21, 1.0], [0.35, 0.35, 0.42, 0.6])
                }
            }
            InteractiveState::Hovered => {
                if self.accent {
                    ([0.23, 0.20, 0.38, 1.0], [0.55, 0.50, 1.0, 1.0])
                } else {
                    ([0.22, 0.22, 0.26, 1.0], [0.45, 0.45, 0.55, 0.8])
                }
            }
            InteractiveState::Pressed => {
                if self.accent {
                    ([0.13, 0.12, 0.22, 1.0], [0.40, 0.36, 0.80, 0.9])
                } else {
                    ([0.14, 0.14, 0.17, 1.0], [0.20, 0.20, 0.25, 0.5])
                }
            }
        };
        vec![QuadInstance {
            rect: [
                self.bounds.x,
                self.bounds.y,
                self.bounds.width,
                self.bounds.height,
            ],
            color: bg,
            color_bottom: bg,
            border_color: border,
            border_width: 1.0,
            border_radius: theme::BORDER_RADIUS_SMALL,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    fn render_icons(&self) -> Vec<super::primitives::IconInstance> {
        vec![]
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        let text_color = if self.accent {
            [210, 205, 245]
        } else {
            [220, 220, 230]
        };
        vec![LabelInfo {
            text: &self.label,
            bounds: self.bounds,
            h_align: super::primitives::HAlign::Center,
            v_align: super::primitives::VAlign::Center,
            overflow: super::primitives::Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some(text_color),
            font_family_override: None,
        }]
    }
}
