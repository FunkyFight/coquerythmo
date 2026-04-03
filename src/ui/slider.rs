use super::theme;
use super::widget::{EventResponse, LabelInfo, QuadInstance, Rect, UiEvent, Widget};

pub struct Slider {
    bounds: Rect,
    value: f32,
    dragging: bool,
    hovered: bool,
    tooltip_text: Option<String>,
    on_change: Box<dyn FnMut(f32) -> EventResponse>,
}

impl Slider {
    pub fn new(bounds: Rect, initial: f32, on_change: impl FnMut(f32) -> EventResponse + 'static) -> Self {
        Self { bounds, value: initial.clamp(0.0, 1.0), dragging: false, hovered: false, tooltip_text: None, on_change: Box::new(on_change) }
    }

    pub fn with_tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip_text = Some(text.into()); self
    }

    fn track_rect(&self) -> Rect {
        let y = self.bounds.y + (self.bounds.height - theme::SLIDER_TRACK_H) / 2.0;
        Rect { x: self.bounds.x + theme::SLIDER_THUMB_R, y, width: self.bounds.width - theme::SLIDER_THUMB_R * 2.0, height: theme::SLIDER_TRACK_H }
    }

    fn value_from_x(&self, x: f32) -> f32 {
        let t = self.track_rect();
        ((x - t.x) / t.width).clamp(0.0, 1.0)
    }
}

impl Widget for Slider {
    fn bounds(&self) -> Rect { self.bounds }
    fn tooltip(&self) -> Option<&str> { self.tooltip_text.as_deref() }

    fn handle_event(&mut self, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { x, y } => {
                if self.dragging {
                    self.value = self.value_from_x(*x);
                    let r = (self.on_change)(self.value);
                    return if r != EventResponse::Ignored { r } else { EventResponse::Consumed };
                }
                let was = self.hovered;
                self.hovered = self.bounds.contains(*x, *y);
                if self.hovered != was { EventResponse::Consumed } else { EventResponse::Ignored }
            }
            UiEvent::MousePress { x, y } => {
                if self.bounds.contains(*x, *y) {
                    self.dragging = true;
                    self.value = self.value_from_x(*x);
                    let r = (self.on_change)(self.value);
                    if r != EventResponse::Ignored { r } else { EventResponse::Consumed }
                } else { EventResponse::Ignored }
            }
            UiEvent::MouseRelease { .. } => {
                if self.dragging { self.dragging = false; EventResponse::Consumed } else { EventResponse::Ignored }
            }
            _ => EventResponse::Ignored,
        }
    }

    fn render_quads(&self) -> Vec<QuadInstance> {
        let track = self.track_rect();
        let cx = track.x + self.value * track.width;
        let fill_w = cx - track.x;
        let thumb_color = if self.dragging { theme::SLIDER_THUMB_PRESS } else if self.hovered { theme::SLIDER_THUMB_HOVER } else { theme::SLIDER_THUMB_NORMAL };

        vec![
            QuadInstance { // Track bg
                rect: [track.x, track.y, track.width, track.height],
                color: theme::SLIDER_TRACK_BG, color_bottom: theme::SLIDER_TRACK_BG,
                border_color: [0.0; 4], border_width: 0.0, border_radius: 2.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0, rotation: 0.0, _padding: [0.0; 2],
            },
            QuadInstance { // Track fill
                rect: [track.x, track.y, fill_w, track.height],
                color: theme::SLIDER_TRACK_FILL, color_bottom: theme::SLIDER_TRACK_FILL,
                border_color: [0.0; 4], border_width: 0.0, border_radius: 2.0,
                shadow_offset: [0.0; 2], shadow_color: [0.0; 4], shadow_blur: 0.0, rotation: 0.0, _padding: [0.0; 2],
            },
            QuadInstance { // Thumb
                rect: [cx - theme::SLIDER_THUMB_R, self.bounds.y + self.bounds.height / 2.0 - theme::SLIDER_THUMB_R, theme::SLIDER_THUMB_R * 2.0, theme::SLIDER_THUMB_R * 2.0],
                color: thumb_color, color_bottom: thumb_color,
                border_color: [0.0; 4], border_width: 0.0, border_radius: theme::SLIDER_THUMB_R,
                shadow_offset: [0.0, 1.0], shadow_color: [0.0, 0.0, 0.0, 0.3], shadow_blur: 3.0, rotation: 0.0, _padding: [0.0; 2],
            },
        ]
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> { vec![] }
}
