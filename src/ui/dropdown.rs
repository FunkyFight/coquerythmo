//! Dropdown and submenu widgets.
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_return)]

use super::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
    Widget,
};

const ITEM_HEIGHT: f32 = 36.0;
const RADIUS: f32 = 6.0;
const PANEL_GAP: f32 = 4.0;
const SUBMENU_REMOVE_WIDTH: f32 = 36.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum DropdownState {
    Normal,
    Hovered,
    Pressed,
}

struct Submenu {
    trigger_index: usize,
    items: Vec<String>,
    on_select: Box<dyn FnMut(usize, &str) -> EventResponse>,
    on_remove: Option<Box<dyn FnMut(usize, &str) -> EventResponse>>,
    open: bool,
    hovered: Option<usize>,
    hovered_remove: bool,
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
    disabled_items: Vec<bool>,
    submenus: Vec<Submenu>,
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
            disabled_items: Vec::new(),
            submenus: Vec::new(),
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

    pub fn with_disabled_items(mut self, disabled: Vec<bool>) -> Self {
        self.disabled_items = disabled;
        self
    }

    pub fn with_submenu(
        mut self,
        trigger_index: usize,
        items: Vec<String>,
        on_select: impl FnMut(usize, &str) -> EventResponse + 'static,
    ) -> Self {
        self.submenus.push(Submenu {
            trigger_index,
            items,
            on_select: Box::new(on_select),
            on_remove: None,
            open: false,
            hovered: None,
            hovered_remove: false,
        });
        self
    }

    pub fn with_removable_submenu(
        mut self,
        trigger_index: usize,
        items: Vec<String>,
        on_select: impl FnMut(usize, &str) -> EventResponse + 'static,
        on_remove: impl FnMut(usize, &str) -> EventResponse + 'static,
    ) -> Self {
        self.submenus.push(Submenu {
            trigger_index,
            items,
            on_select: Box::new(on_select),
            on_remove: Some(Box::new(on_remove)),
            open: false,
            hovered: None,
            hovered_remove: false,
        });
        self
    }

    fn submenu_panel_rect(&self, sub_idx: usize) -> Option<Rect> {
        let sub = self.submenus.get(sub_idx)?;
        if !sub.open || sub.items.is_empty() {
            return None;
        }
        let parent = self.panel_rect();
        let trigger_item = self.option_rect(sub.trigger_index);
        Some(Rect {
            x: parent.x + parent.width + 2.0,
            y: trigger_item.y,
            width: self.panel_width.unwrap_or(200.0),
            height: sub.items.len() as f32 * ITEM_HEIGHT,
        })
    }

    fn hit_submenu_option(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        for (i, sub) in self.submenus.iter().enumerate() {
            if let Some(rect) = self.submenu_panel_rect(i) {
                if rect.contains(x, y) {
                    let index = ((y - rect.y) / ITEM_HEIGHT) as usize;
                    if index < sub.items.len() {
                        return Some((i, index));
                    }
                }
            }
        }
        None
    }

    fn hit_submenu_remove(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let (sub_idx, item_idx) = self.hit_submenu_option(x, y)?;
        let sub = self.submenus.get(sub_idx)?;
        let panel = self.submenu_panel_rect(sub_idx)?;
        (sub.on_remove.is_some() && x >= panel.x + panel.width - SUBMENU_REMOVE_WIDTH)
            .then_some((sub_idx, item_idx))
    }

    fn close(&mut self) {
        self.open = false;
        self.hovered_option = None;
        for sub in &mut self.submenus {
            sub.open = false;
            sub.hovered = None;
            sub.hovered_remove = false;
        }
    }

    fn is_disabled(&self, index: usize) -> bool {
        self.disabled_items.get(index).copied().unwrap_or(false)
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

    fn first_enabled(&self) -> Option<usize> {
        (0..self.options.len()).find(|index| !self.is_disabled(*index))
    }

    fn move_keyboard(&mut self, direction: i32) {
        if let Some(sub) = self.submenus.iter_mut().find(|submenu| submenu.open) {
            if sub.items.is_empty() {
                return;
            }
            let len = sub.items.len() as i32;
            let current = sub.hovered.unwrap_or(if direction > 0 {
                sub.items.len() - 1
            } else {
                0
            }) as i32;
            sub.hovered = Some((current + direction).rem_euclid(len) as usize);
            sub.hovered_remove = false;
            return;
        }
        if self.options.is_empty() {
            return;
        }
        let len = self.options.len() as i32;
        let mut current = self.hovered_option.unwrap_or(if direction > 0 {
            self.options.len() - 1
        } else {
            0
        }) as i32;
        for _ in 0..self.options.len() {
            current = (current + direction).rem_euclid(len);
            if !self.is_disabled(current as usize) {
                self.hovered_option = Some(current as usize);
                break;
            }
        }
        for sub in &mut self.submenus {
            sub.open = false;
            sub.hovered = None;
        }
    }

    fn open_hovered_submenu(&mut self) -> bool {
        let Some(index) = self.hovered_option else {
            return false;
        };
        let Some(sub) = self
            .submenus
            .iter_mut()
            .find(|submenu| submenu.trigger_index == index)
        else {
            return false;
        };
        sub.open = true;
        sub.hovered = (!sub.items.is_empty()).then_some(0);
        true
    }

    fn activate_keyboard(&mut self) -> EventResponse {
        if !self.open {
            self.open = true;
            self.hovered_option = self.first_enabled();
            return EventResponse::Consumed;
        }
        if let Some(sub_index) = self.submenus.iter().position(|submenu| submenu.open) {
            let item_index = self.submenus[sub_index].hovered.unwrap_or(0);
            let Some(label) = self.submenus[sub_index].items.get(item_index).cloned() else {
                return EventResponse::Consumed;
            };
            let response = (self.submenus[sub_index].on_select)(item_index, &label);
            self.close();
            return if response == EventResponse::Ignored {
                EventResponse::Consumed
            } else {
                response
            };
        }
        let Some(index) = self.hovered_option.or(self.first_enabled()) else {
            return EventResponse::Consumed;
        };
        if self.open_hovered_submenu() {
            return EventResponse::Consumed;
        }
        if self.is_disabled(index) {
            return EventResponse::Consumed;
        }
        self.selected = index;
        let label = self.options[index].clone();
        let response = (self.on_select)(index, &label);
        self.close();
        if response == EventResponse::Ignored {
            EventResponse::Consumed
        } else {
            response
        }
    }

    fn remove_keyboard_submenu_item(&mut self) -> EventResponse {
        let Some(sub_index) = self.submenus.iter().position(|submenu| submenu.open) else {
            return EventResponse::Consumed;
        };
        let item_index = self.submenus[sub_index].hovered.unwrap_or(0);
        let Some(label) = self.submenus[sub_index].items.get(item_index).cloned() else {
            return EventResponse::Consumed;
        };
        let Some(on_remove) = self.submenus[sub_index].on_remove.as_mut() else {
            return EventResponse::Consumed;
        };
        let response = on_remove(item_index, &label);
        self.close();
        if response == EventResponse::Ignored {
            EventResponse::Consumed
        } else {
            response
        }
    }

    fn announce_keyboard_selection(&self) -> EventResponse {
        let label = if let Some(submenu) = self.submenus.iter().find(|submenu| submenu.open) {
            submenu
                .hovered
                .and_then(|index| submenu.items.get(index))
                .cloned()
        } else {
            self.hovered_option
                .and_then(|index| self.options.get(index))
                .cloned()
        };
        label.map_or(EventResponse::Consumed, |label| {
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection { label },
            ))
        })
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
                width: panel.width.max(self.bounds.width),
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
            UiEvent::Activate => return self.activate_keyboard(),
            UiEvent::CursorDown if self.open => {
                self.move_keyboard(1);
                return self.announce_keyboard_selection();
            }
            UiEvent::CursorUp if self.open => {
                self.move_keyboard(-1);
                return self.announce_keyboard_selection();
            }
            UiEvent::Home if self.open => {
                if let Some(sub) = self.submenus.iter_mut().find(|submenu| submenu.open) {
                    sub.hovered = (!sub.items.is_empty()).then_some(0);
                } else {
                    self.hovered_option = self.first_enabled();
                }
                return self.announce_keyboard_selection();
            }
            UiEvent::End if self.open => {
                if let Some(sub) = self.submenus.iter_mut().find(|submenu| submenu.open) {
                    sub.hovered = sub.items.len().checked_sub(1);
                } else {
                    self.hovered_option = (0..self.options.len())
                        .rev()
                        .find(|index| !self.is_disabled(*index));
                }
                return self.announce_keyboard_selection();
            }
            UiEvent::CursorRight if self.open => {
                self.open_hovered_submenu();
                return EventResponse::Consumed;
            }
            UiEvent::CursorLeft if self.open => {
                if let Some(sub) = self.submenus.iter_mut().find(|submenu| submenu.open) {
                    sub.open = false;
                    sub.hovered = None;
                } else {
                    self.close();
                }
                return EventResponse::Consumed;
            }
            UiEvent::Delete if self.open => return self.remove_keyboard_submenu_item(),
            UiEvent::KeyInput { text } if self.open && text == "\x1b" => {
                self.close();
                return EventResponse::Consumed;
            }
            UiEvent::MouseMove { x, y } => {
                if self.open {
                    let prev = self.hovered_option;
                    self.hovered_option = self.hit_option(*x, *y);

                    self.trigger_state = if self.bounds.contains(*x, *y) {
                        DropdownState::Hovered
                    } else {
                        DropdownState::Normal
                    };

                    let sub_hit = self.hit_submenu_option(*x, *y);
                    let remove_hit = self.hit_submenu_remove(*x, *y);

                    let mut state_changed = false;
                    for (i, sub) in self.submenus.iter_mut().enumerate() {
                        let was_open = sub.open;
                        let prev_hover = sub.hovered;
                        let prev_remove = sub.hovered_remove;

                        if self.hovered_option == Some(sub.trigger_index) {
                            sub.open = true;
                        } else if let Some((hit_sub_idx, _)) = sub_hit {
                            if hit_sub_idx != i {
                                sub.open = false;
                                sub.hovered = None;
                                sub.hovered_remove = false;
                            }
                        } else if self.hovered_option.is_some() {
                            sub.open = false;
                            sub.hovered = None;
                        }

                        if sub.open {
                            if let Some((hit_sub_idx, hit_item_idx)) = sub_hit {
                                if hit_sub_idx == i {
                                    sub.hovered = Some(hit_item_idx);
                                    sub.hovered_remove =
                                        remove_hit == Some((hit_sub_idx, hit_item_idx));
                                } else {
                                    sub.hovered = None;
                                    sub.hovered_remove = false;
                                }
                            } else {
                                sub.hovered = None;
                                sub.hovered_remove = false;
                            }
                        }

                        if was_open != sub.open
                            || prev_hover != sub.hovered
                            || prev_remove != sub.hovered_remove
                        {
                            state_changed = true;
                        }
                    }

                    if self.hovered_option != prev || state_changed {
                        return EventResponse::Consumed;
                    }
                    EventResponse::Ignored
                } else {
                    let inside = self.bounds.contains(*x, *y);
                    let new_state = if inside && self.trigger_state == DropdownState::Pressed {
                        DropdownState::Pressed
                    } else if inside {
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
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if self.open {
                    let in_submenu = self.submenus.iter().enumerate().any(|(i, _)| {
                        self.submenu_panel_rect(i)
                            .map(|r| r.contains(*x, *y))
                            .unwrap_or(false)
                    });

                    if self.bounds.contains(*x, *y)
                        || self.panel_rect().contains(*x, *y)
                        || in_submenu
                    {
                        EventResponse::Consumed
                    } else {
                        self.close();
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
                    if let Some((sub_idx, item_idx)) = self.hit_submenu_remove(*x, *y) {
                        let label = self.submenus[sub_idx].items[item_idx].clone();
                        let response =
                            if let Some(on_remove) = self.submenus[sub_idx].on_remove.as_mut() {
                                on_remove(item_idx, &label)
                            } else {
                                EventResponse::Consumed
                            };
                        self.close();
                        self.trigger_state = DropdownState::Normal;
                        return if response != EventResponse::Ignored {
                            response
                        } else {
                            EventResponse::Consumed
                        };
                    }
                    if let Some((sub_idx, item_idx)) = self.hit_submenu_option(*x, *y) {
                        let label = self.submenus[sub_idx].items[item_idx].clone();
                        let response = (self.submenus[sub_idx].on_select)(item_idx, &label);
                        self.close();
                        self.trigger_state = DropdownState::Normal;
                        return if response != EventResponse::Ignored {
                            response
                        } else {
                            EventResponse::Consumed
                        };
                    }

                    let is_submenu_trigger = self
                        .submenus
                        .iter()
                        .any(|s| Some(s.trigger_index) == self.hit_option(*x, *y));

                    if let Some(index) = self.hit_option(*x, *y) {
                        if self.is_disabled(index) || is_submenu_trigger {
                            return EventResponse::Consumed;
                        }
                        self.selected = index;
                        let label = self.options[index].clone();
                        let response = (self.on_select)(index, &label);
                        self.close();
                        self.trigger_state = if self.bounds.contains(*x, *y) {
                            DropdownState::Hovered
                        } else {
                            DropdownState::Normal
                        };
                        if response != EventResponse::Ignored {
                            response
                        } else {
                            EventResponse::Consumed
                        }
                    } else if self.bounds.contains(*x, *y) {
                        self.close();
                        self.trigger_state = DropdownState::Hovered;
                        EventResponse::Consumed
                    } else {
                        self.close();
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

        if self.show_trigger_bg {
            quads.push(QuadInstance {
                rect: [
                    self.bounds.x,
                    self.bounds.y,
                    self.bounds.width,
                    self.bounds.height,
                ],
                color: self.trigger_bg_top(),
                color_bottom: self.trigger_bg_bottom(),
                border_color: self.trigger_border(),
                border_width: 1.0,
                border_radius: RADIUS,
                shadow_offset: [0.0, 2.0],
                shadow_color: [0.0, 0.0, 0.0, 0.35],
                shadow_blur: 6.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        if self.open {
            let panel = self.panel_rect();

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
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            for (i, sub) in self.submenus.iter().enumerate() {
                if let Some(sub_rect) = self.submenu_panel_rect(i) {
                    quads.push(QuadInstance {
                        rect: [sub_rect.x, sub_rect.y, sub_rect.width, sub_rect.height],
                        color: [0.15, 0.15, 0.17, 1.0],
                        color_bottom: [0.12, 0.12, 0.14, 1.0],
                        border_color: [0.30, 0.30, 0.36, 0.6],
                        border_width: 1.0,
                        border_radius: RADIUS,
                        shadow_offset: [0.0, 4.0],
                        shadow_color: [0.0, 0.0, 0.0, 0.5],
                        shadow_blur: 12.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                    for j in 0..sub.items.len() {
                        if sub.hovered == Some(j) {
                            let sy = sub_rect.y + j as f32 * ITEM_HEIGHT;
                            quads.push(QuadInstance {
                                rect: [
                                    sub_rect.x + 3.0,
                                    sy + 1.0,
                                    sub_rect.width - 6.0,
                                    ITEM_HEIGHT - 2.0,
                                ],
                                color: [0.12, 0.34, 0.72, 0.72],
                                color_bottom: [0.08, 0.24, 0.58, 0.72],
                                border_color: [0.36, 0.68, 1.0, 0.9],
                                border_width: 1.0,
                                border_radius: 4.0,
                                shadow_offset: [0.0; 2],
                                shadow_color: [0.0; 4],
                                shadow_blur: 0.0,
                                rotation: 0.0,
                                _padding: [0.0; 2],
                            });
                            if sub.hovered_remove && sub.on_remove.is_some() {
                                quads.push(QuadInstance {
                                    rect: [
                                        sub_rect.x + sub_rect.width - SUBMENU_REMOVE_WIDTH + 3.0,
                                        sy + 3.0,
                                        SUBMENU_REMOVE_WIDTH - 6.0,
                                        ITEM_HEIGHT - 6.0,
                                    ],
                                    color: [0.65, 0.15, 0.18, 0.55],
                                    color_bottom: [0.52, 0.10, 0.13, 0.55],
                                    border_color: [0.95, 0.35, 0.38, 0.45],
                                    border_width: 1.0,
                                    border_radius: 4.0,
                                    shadow_offset: [0.0; 2],
                                    shadow_color: [0.0; 4],
                                    shadow_blur: 0.0,
                                    rotation: 0.0,
                                    _padding: [0.0; 2],
                                });
                            }
                        }
                    }
                }
            }

            for i in 0..self.options.len() {
                if self.is_disabled(i) {
                    continue;
                }
                let is_hovered = self.hovered_option == Some(i);
                let is_selected = self.selected == i;

                if is_hovered || is_selected {
                    let r = self.option_rect(i);
                    let inset = 3.0;
                    let bg = if is_hovered && is_selected {
                        [0.12, 0.34, 0.72, 0.82]
                    } else if is_hovered {
                        [0.12, 0.34, 0.72, 0.72]
                    } else {
                        [0.30, 0.27, 0.75, 0.3]
                    };
                    quads.push(QuadInstance {
                        rect: [
                            r.x + inset,
                            r.y + 1.0,
                            r.width - inset * 2.0,
                            r.height - 2.0,
                        ],
                        color: bg,
                        color_bottom: bg,
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        border_width: 0.0,
                        border_radius: 4.0,
                        shadow_offset: [0.0, 0.0],
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_blur: 0.0,
                        rotation: 0.0,
                        _padding: [0.0; 2],
                    });
                }
            }
        }

        quads
    }

    fn labels(&self) -> Vec<LabelInfo<'_>> {
        let mut result = Vec::new();

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
            padding: 12.0,
            font_size_override: None,
            color_override: None,
            font_family_override: None,
        });

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
                padding: 0.0,
                font_size_override: None,
                color_override: None,
                font_family_override: None,
            });
        }

        if self.open {
            for (i, option) in self.options.iter().enumerate() {
                let r = self.option_rect(i);
                let color_override = if self.is_disabled(i) {
                    Some([100, 100, 100])
                } else {
                    None
                };
                result.push(LabelInfo {
                    text: option,
                    bounds: r,
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 12.0,
                    font_size_override: None,
                    color_override,
                    font_family_override: None,
                });
            }

            for (i, sub) in self.submenus.iter().enumerate() {
                if let Some(sub_rect) = self.submenu_panel_rect(i) {
                    for (j, item) in sub.items.iter().enumerate() {
                        let sy = sub_rect.y + j as f32 * ITEM_HEIGHT;
                        let remove_width = if sub.on_remove.is_some() {
                            SUBMENU_REMOVE_WIDTH
                        } else {
                            0.0
                        };
                        result.push(LabelInfo {
                            text: item,
                            bounds: Rect {
                                x: sub_rect.x,
                                y: sy,
                                width: sub_rect.width - remove_width,
                                height: ITEM_HEIGHT,
                            },
                            h_align: HAlign::Left,
                            v_align: VAlign::Center,
                            overflow: Overflow::Ellipsis,
                            padding: 12.0,
                            font_size_override: None,
                            color_override: None,
                            font_family_override: None,
                        });
                        if sub.on_remove.is_some() {
                            result.push(LabelInfo {
                                text: "×",
                                bounds: Rect {
                                    x: sub_rect.x + sub_rect.width - SUBMENU_REMOVE_WIDTH,
                                    y: sy,
                                    width: SUBMENU_REMOVE_WIDTH,
                                    height: ITEM_HEIGHT,
                                },
                                h_align: HAlign::Center,
                                v_align: VAlign::Center,
                                overflow: Overflow::Clip,
                                padding: 0.0,
                                font_size_override: Some(17.0),
                                color_override: sub.hovered_remove.then_some([255, 205, 205]),
                                font_family_override: None,
                            });
                        }
                    }
                }
            }
        }

        result
    }

    fn accessible_label(&self) -> Option<&str> {
        self.trigger_label
            .as_deref()
            .or_else(|| self.options.get(self.selected).map(String::as_str))
    }

    fn accessible_role(&self) -> super::focus::AccessibleRole {
        super::focus::AccessibleRole::MenuButton
    }

    fn open_submenu(&mut self, trigger_index: usize) -> bool {
        let Some(sub_index) = self
            .submenus
            .iter()
            .position(|submenu| submenu.trigger_index == trigger_index)
        else {
            return false;
        };
        self.open = true;
        self.hovered_option = Some(trigger_index);
        for (index, submenu) in self.submenus.iter_mut().enumerate() {
            submenu.open = index == sub_index;
            submenu.hovered = if index == sub_index && !submenu.items.is_empty() {
                Some(0)
            } else {
                None
            };
        }
        true
    }
}
