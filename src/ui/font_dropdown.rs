use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};

const ITEM_HEIGHT: f32 = 28.0;
const VISIBLE_ROWS: usize = 7;

pub struct FontDropdown {
    fonts: Vec<String>,
    selected: Option<String>,
    open: bool,
    scroll: usize,
    hovered: usize,
}

impl FontDropdown {
    pub fn new(mut fonts: Vec<String>, selected: Option<String>) -> Self {
        fonts.sort_unstable_by_key(|font| font.to_lowercase());
        fonts.dedup();
        Self {
            fonts,
            selected,
            open: false,
            scroll: 0,
            hovered: 0,
        }
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn selected_owned(&self) -> Option<String> {
        self.selected.clone()
    }

    pub fn expanded_height(&self) -> f32 {
        if self.open {
            self.list_height() + 4.0
        } else {
            0.0
        }
    }

    pub fn handle_event(&mut self, event: &UiEvent, trigger: Rect) -> bool {
        let list = self.list_rect(trigger);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" && self.open => {
                self.open = false;
                true
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                if self.open {
                    self.choose(self.hovered);
                } else {
                    self.open();
                }
                true
            }
            UiEvent::Activate => {
                if self.open {
                    self.choose(self.hovered);
                } else {
                    self.open();
                }
                true
            }
            UiEvent::CursorUp if self.open => {
                self.hovered = self.hovered.saturating_sub(1);
                self.reveal_hovered();
                true
            }
            UiEvent::CursorDown if self.open => {
                self.hovered = (self.hovered + 1).min(self.item_count() - 1);
                self.reveal_hovered();
                true
            }
            UiEvent::MouseMove { x, y } if self.open && list.contains(*x, *y) => {
                self.hovered = self
                    .scroll
                    .saturating_add(((*y - list.y) / ITEM_HEIGHT) as usize)
                    .min(self.item_count() - 1);
                true
            }
            UiEvent::Scroll { x, y, delta, .. } if self.open && list.contains(*x, *y) => {
                let step = delta.abs().ceil().max(1.0) as usize;
                self.scroll = if *delta > 0.0 {
                    self.scroll.saturating_sub(step)
                } else {
                    (self.scroll + step).min(self.max_scroll())
                };
                true
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if trigger.contains(*x, *y) {
                    if self.open {
                        self.open = false;
                    } else {
                        self.open();
                    }
                    return true;
                }
                if self.open && list.contains(*x, *y) {
                    let index = self.scroll + ((*y - list.y) / ITEM_HEIGHT) as usize;
                    if index < self.item_count() {
                        self.choose(index);
                    }
                    return true;
                }
                if self.open {
                    self.open = false;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        trigger: Rect,
        default_label: &'a str,
    ) {
        push_quad(
            quads,
            trigger,
            [0.08, 0.08, 0.10, 1.0],
            [0.35, 0.35, 0.42, 0.8],
        );
        labels.push(label(
            self.selected().unwrap_or(default_label),
            Rect {
                width: trigger.width - 34.0,
                ..trigger
            },
            self.selected(),
        ));
        labels.push(label(
            if self.open { "▲" } else { "▼" },
            Rect {
                x: trigger.x + trigger.width - 34.0,
                width: 34.0,
                ..trigger
            },
            None,
        ));
        if !self.open {
            return;
        }

        let list = self.list_rect(trigger);
        push_quad(
            quads,
            list,
            [0.08, 0.08, 0.10, 1.0],
            [0.35, 0.35, 0.42, 0.8],
        );
        for index in self.scroll..(self.scroll + VISIBLE_ROWS).min(self.item_count()) {
            let row = Rect {
                x: list.x + 2.0,
                y: list.y + (index - self.scroll) as f32 * ITEM_HEIGHT,
                width: list.width - 4.0,
                height: ITEM_HEIGHT,
            };
            if index == self.hovered || self.is_selected(index) {
                push_quad(
                    quads,
                    row,
                    if index == self.hovered {
                        [0.30, 0.28, 0.55, 0.9]
                    } else {
                        [0.22, 0.20, 0.40, 0.8]
                    },
                    [0.0; 4],
                );
            }
            let (text, family) = self.item(index, default_label);
            labels.push(label(text, row, family));
        }
    }

    fn open(&mut self) {
        self.open = true;
        self.hovered = self
            .selected
            .as_ref()
            .and_then(|selected| self.fonts.iter().position(|font| font == selected))
            .map_or(0, |index| index + 1);
        self.reveal_hovered();
    }

    fn choose(&mut self, index: usize) {
        self.selected = index
            .checked_sub(1)
            .and_then(|index| self.fonts.get(index))
            .cloned();
        self.open = false;
    }

    fn reveal_hovered(&mut self) {
        if self.hovered < self.scroll {
            self.scroll = self.hovered;
        } else if self.hovered >= self.scroll + VISIBLE_ROWS {
            self.scroll = self.hovered + 1 - VISIBLE_ROWS;
        }
    }

    fn item<'a>(&'a self, index: usize, default_label: &'a str) -> (&'a str, Option<&'a str>) {
        index.checked_sub(1).map_or((default_label, None), |index| {
            let font = self.fonts[index].as_str();
            (font, Some(font))
        })
    }

    fn is_selected(&self, index: usize) -> bool {
        index
            .checked_sub(1)
            .map_or(self.selected.is_none(), |index| {
                self.selected.as_ref() == self.fonts.get(index)
            })
    }

    fn item_count(&self) -> usize {
        self.fonts.len() + 1
    }

    fn max_scroll(&self) -> usize {
        self.item_count().saturating_sub(VISIBLE_ROWS)
    }

    fn list_height(&self) -> f32 {
        self.item_count().min(VISIBLE_ROWS) as f32 * ITEM_HEIGHT
    }

    fn list_rect(&self, trigger: Rect) -> Rect {
        Rect {
            y: trigger.y + trigger.height + 4.0,
            height: self.list_height(),
            ..trigger
        }
    }
}

fn label<'a>(text: &'a str, bounds: Rect, font_family: Option<&'a str>) -> LabelInfo<'a> {
    LabelInfo {
        text,
        bounds,
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 8.0,
        font_size_override: Some(12.0),
        color_override: None,
        font_family_override: font_family,
    }
}

fn push_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], border: [f32; 4]) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: 5.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropdown_selects_a_visible_font_directly() {
        let mut dropdown = FontDropdown::new(vec!["Verdana".into(), "Arial".into()], None);
        let trigger = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 34.0,
        };
        assert!(dropdown.handle_event(&UiEvent::MousePress { x: 5.0, y: 5.0 }, trigger));
        assert!(dropdown.handle_event(&UiEvent::MousePress { x: 5.0, y: 75.0 }, trigger));
        assert_eq!(dropdown.selected(), Some("Arial"));
    }

    #[test]
    fn every_font_name_is_rendered_with_its_own_family() {
        let mut dropdown = FontDropdown::new(vec!["Arial".into()], None);
        let trigger = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 34.0,
        };
        dropdown.handle_event(&UiEvent::Activate, trigger);
        let mut quads = Vec::new();
        let mut labels = Vec::new();
        dropdown.render(&mut quads, &mut labels, trigger, "Default");
        assert!(labels
            .iter()
            .any(|label| label.text == "Arial" && label.font_family_override == Some("Arial")));
    }
}
