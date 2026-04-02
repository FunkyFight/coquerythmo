use super::widget::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign, Widget,
};

const ITEM_HEIGHT: f32 = 36.0;
const RADIUS: f32 = 6.0;
const PANEL_GAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DropdownState {
    Normal,
    Hovered,
    Pressed,
}

pub struct Dropdown {
    bounds: Rect,
    options: Vec<String>,
    selected: usize,
    open: bool,
    hovered_option: Option<usize>,
    trigger_state: DropdownState,
    on_select: Box<dyn FnMut(usize, &str) -> EventResponse>,
    show_arrow: bool,
    show_trigger_bg: bool,
    trigger_label: Option<String>,
    panel_width: Option<f32>,
}

impl Dropdown {
    pub fn new(
        bounds: Rect,
        options: Vec<String>,
        on_select: impl FnMut(usize, &str) -> EventResponse + 'static,
    ) -> Self {
        Self {
            bounds,
            options,
            selected: 0,
            open: false,
            hovered_option: None,
            trigger_state: DropdownState::Normal,
            on_select: Box::new(on_select),
            show_arrow: true,
            show_trigger_bg: true,
            trigger_label: None,
            panel_width: None,
        }
    }

    pub fn with_arrow(mut self, show: bool) -> Self {
        self.show_arrow = show;
        self
    }

    pub fn with_trigger_bg(mut self, show: bool) -> Self {
        self.show_trigger_bg = show;
        self
    }

    pub fn with_trigger_label(mut self, label: impl Into<String>) -> Self {
        self.trigger_label = Some(label.into());
        self
    }

    pub fn with_panel_width(mut self, width: f32) -> Self {
        self.panel_width = Some(width);
        self
    }

    fn panel_rect(&self) -> Rect {
        Rect {
            x: self.bounds.x,
            y: self.bounds.y + self.bounds.height + PANEL_GAP,
            width: self.panel_width.unwrap_or(self.bounds.width),
            height: self.options.len() as f32 * ITEM_HEIGHT,
        }
    }

    fn option_rect(&self, index: usize) -> Rect {
        let panel = self.panel_rect();
        Rect {
            x: panel.x,
            y: panel.y + index as f32 * ITEM_HEIGHT,
            width: panel.width,
            height: ITEM_HEIGHT,
        }
    }

    fn hit_option(&self, x: f32, y: f32) -> Option<usize> {
        let panel = self.panel_rect();
        if !panel.contains(x, y) {
            return None;
        }
        let index = ((y - panel.y) / ITEM_HEIGHT) as usize;
        if index < self.options.len() {
            Some(index)
        } else {
            None
        }
    }

    // -- Colors --

    fn trigger_bg_top(&self) -> [f32; 4] {
        match self.trigger_state {
            DropdownState::Normal => [0.20, 0.20, 0.23, 1.0],
            DropdownState::Hovered => [0.26, 0.26, 0.30, 1.0],
            DropdownState::Pressed => [0.12, 0.12, 0.14, 1.0],
        }
    }

    fn trigger_bg_bottom(&self) -> [f32; 4] {
        match self.trigger_state {
            DropdownState::Normal => [0.13, 0.13, 0.15, 1.0],
            DropdownState::Hovered => [0.18, 0.18, 0.21, 1.0],
            DropdownState::Pressed => [0.08, 0.08, 0.10, 1.0],
        }
    }

    fn trigger_border(&self) -> [f32; 4] {
        if self.open {
            [0.40, 0.37, 0.80, 0.8]
        } else {
            match self.trigger_state {
                DropdownState::Normal => [0.30, 0.30, 0.36, 0.6],
                DropdownState::Hovered => [0.40, 0.40, 0.48, 0.8],
                DropdownState::Pressed => [0.22, 0.22, 0.28, 0.5],
            }
        }
    }
}

impl Widget for Dropdown {
    fn bounds(&self) -> Rect {
        if self.open {
            let panel = self.panel_rect();
            Rect {
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: (panel.y + panel.height) - self.bounds.y,
            }
        } else {
            self.bounds
        }
    }

    fn captures_all(&self) -> bool {
        self.open
    }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                if self.open {
                    let prev = self.hovered_option;
                    self.hovered_option = self.hit_option(*x, *y);
                    // Also update trigger hover
                    self.trigger_state = if self.bounds.contains(*x, *y) {
                        DropdownState::Hovered
                    } else {
                        DropdownState::Normal
                    };
                    if self.hovered_option != prev {
                        return EventResponse::Consumed;
                    }
                    EventResponse::Ignored
                } else {
                    let inside = self.bounds.contains(*x, *y);
                    let new_state = if inside {
                        DropdownState::Hovered
                    } else {
                        DropdownState::Normal
                    };
                    if new_state != self.trigger_state {
                        self.trigger_state = new_state;
                        EventResponse::Consumed
                    } else {
                        EventResponse::Ignored
                    }
                }
            }
            UiEvent::MousePress { x, y } => {
                if self.open {
                    // Press inside trigger or panel is fine
                    if self.bounds.contains(*x, *y) || self.panel_rect().contains(*x, *y) {
                        EventResponse::Consumed
                    } else {
                        // Click outside → close
                        self.open = false;
                        self.hovered_option = None;
                        self.trigger_state = DropdownState::Normal;
                        EventResponse::Consumed
                    }
                } else if self.bounds.contains(*x, *y) {
                    self.trigger_state = DropdownState::Pressed;
                    EventResponse::Consumed
                } else {
                    EventResponse::Ignored
                }
            }
            UiEvent::MouseRelease { x, y } => {
                if self.open {
                    if let Some(index) = self.hit_option(*x, *y) {
                        self.selected = index;
                        self.open = false;
                        self.hovered_option = None;
                        let label = self.options[index].clone();
                        let response = (self.on_select)(index, &label);
                        self.trigger_state = if self.bounds.contains(*x, *y) {
                            DropdownState::Hovered
                        } else {
                            DropdownState::Normal
                        };
                        if response != EventResponse::Ignored { response } else { EventResponse::Consumed }
                    } else if self.bounds.contains(*x, *y) {
                        // Clicked trigger while open → close
                        self.open = false;
                        self.hovered_option = None;
                        self.trigger_state = DropdownState::Hovered;
                        EventResponse::Consumed
                    } else {
                        // Released outside
                        self.open = false;
                        self.hovered_option = None;
                        self.trigger_state = DropdownState::Normal;
                        EventResponse::Consumed
                    }
                } else if self.trigger_state == DropdownState::Pressed
                    && self.bounds.contains(*x, *y)
                {
                    self.open = true;
                    self.trigger_state = DropdownState::Hovered;
                    EventResponse::Consumed
                } else {
                    self.trigger_state = if self.bounds.contains(*x, *y) {
                        DropdownState::Hovered
                    } else {
                        DropdownState::Normal
                    };
                    EventResponse::Ignored
                }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn render_quads(&self) -> Vec<QuadInstance> {
        let mut quads = Vec::new();

        // Trigger button
        if self.show_trigger_bg {
            quads.push(QuadInstance {
                rect: [self.bounds.x, self.bounds.y, self.bounds.width, self.bounds.height],
                color: self.trigger_bg_top(),
                color_bottom: self.trigger_bg_bottom(),
                border_color: self.trigger_border(),
                border_width: 1.0,
                border_radius: RADIUS,
                shadow_offset: [0.0, 2.0],
                shadow_color: [0.0, 0.0, 0.0, 0.35],
                shadow_blur: 6.0,
                _padding: [0.0; 3],
            });
        }

        if self.open {
            let panel = self.panel_rect();

            // Panel background
            quads.push(QuadInstance {
                rect: [panel.x, panel.y, panel.width, panel.height],
                color: [0.15, 0.15, 0.17, 1.0],
                color_bottom: [0.12, 0.12, 0.14, 1.0],
                border_color: [0.30, 0.30, 0.36, 0.6],
                border_width: 1.0,
                border_radius: RADIUS,
                shadow_offset: [0.0, 4.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                shadow_blur: 12.0,
                _padding: [0.0; 3],
            });

            // Option highlights
            for i in 0..self.options.len() {
                let is_hovered = self.hovered_option == Some(i);
                let is_selected = self.selected == i;

                if is_hovered || is_selected {
                    let r = self.option_rect(i);
                    // Inset the highlight slightly
                    let inset = 3.0;
                    let bg = if is_hovered && is_selected {
                        [0.30, 0.27, 0.75, 0.5]
                    } else if is_hovered {
                        [1.0, 1.0, 1.0, 0.07]
                    } else {
                        // selected only
                        [0.30, 0.27, 0.75, 0.3]
                    };
                    quads.push(QuadInstance {
                        rect: [r.x + inset, r.y + 1.0, r.width - inset * 2.0, r.height - 2.0],
                        color: bg,
                        color_bottom: bg,
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        border_width: 0.0,
                        border_radius: 4.0,
                        shadow_offset: [0.0, 0.0],
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_blur: 0.0,
                        _padding: [0.0; 3],
                    });
                }
            }
        }

        quads
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        let mut result = Vec::new();

        // Trigger label: fixed label or selected option
        let selected_text = self
            .trigger_label
            .as_deref()
            .unwrap_or(&self.options[self.selected]);
        let arrow_space = if self.show_arrow { 32.0 } else { 0.0 };
        let trigger_label_rect = Rect {
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width - arrow_space,
            height: self.bounds.height,
        };
        result.push(LabelInfo {
            text: selected_text,
            bounds: trigger_label_rect,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 12.0, font_size_override: None, color_override: None,
        });

        // Arrow indicator
        if self.show_arrow {
            let arrow_rect = Rect {
                x: self.bounds.x + self.bounds.width - 32.0,
                y: self.bounds.y,
                width: 28.0,
                height: self.bounds.height,
            };
            result.push(LabelInfo {
                text: if self.open { "▲" } else { "▼" },
                bounds: arrow_rect,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0, font_size_override: None, color_override: None,
            });
        }

        // Option labels when open
        if self.open {
            for (i, option) in self.options.iter().enumerate() {
                let r = self.option_rect(i);
                result.push(LabelInfo {
                    text: option,
                    bounds: r,
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 12.0, font_size_override: None, color_override: None,
                });
            }
        }

        result
    }
}
