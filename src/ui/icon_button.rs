use super::interactive::{InteractiveResult, InteractiveState};
use super::theme;
use super::widget::{EventResponse, IconInstance, LabelInfo, QuadInstance, Rect, UiEvent, Widget};

pub struct IconButton {
    bounds: Rect,
    icon_uv: [f32; 4],
    tooltip_text: Option<String>,
    state: InteractiveState,
    on_click: Box<dyn FnMut() -> EventResponse>,
    active: bool,
}

impl IconButton {
    pub fn new(
        bounds: Rect,
        _icon_name: impl Into<String>,
        icon_uv: [f32; 4],
        on_click: impl FnMut() -> EventResponse + 'static,
    ) -> Self {
        Self {
            bounds,
            icon_uv,
            tooltip_text: None,
            state: InteractiveState::default(),
            on_click: Box::new(on_click),
            active: false,
        }
    }

    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip_text = Some(text.into());
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Widget for IconButton {
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
        let (bg, border_color, border_width) = if self.active {
            (theme::TRANSPARENT_HOVER, theme::INTERACTIVE_BORDER_HOVERED, 2.0)
        } else {
            let bg = match self.state {
                InteractiveState::Normal => theme::TRANSPARENT,
                InteractiveState::Hovered => theme::TRANSPARENT_HOVER,
                InteractiveState::Pressed => theme::TRANSPARENT_PRESS,
            };
            (bg, [0.0; 4], 0.0)
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
            border_color,
            border_width,
            border_radius: theme::BORDER_RADIUS_SMALL,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }]
    }

    fn render_icons(&self) -> Vec<IconInstance> {
        let tint = match self.state {
            InteractiveState::Normal => theme::ICON_TINT_NORMAL,
            InteractiveState::Hovered => theme::ICON_TINT_HOVERED,
            InteractiveState::Pressed => theme::ICON_TINT_PRESSED,
        };
        let padding = 4.0;
        let icon_size = self.bounds.width.min(self.bounds.height) - padding * 2.0;
        let x = self.bounds.x + (self.bounds.width - icon_size) / 2.0;
        let y = self.bounds.y + (self.bounds.height - icon_size) / 2.0;
        vec![IconInstance {
            rect: [x, y, icon_size, icon_size],
            uv_rect: self.icon_uv,
            tint,
        }]
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        vec![]
    }
}
