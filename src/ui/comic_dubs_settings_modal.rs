use super::font_dropdown::FontDropdown;
use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const WIDTH: f32 = 500.0;
const HEIGHT: f32 = 500.0;
const CONTROL_COUNT: usize = 6;

pub struct ComicDubsSettingsModal {
    font: FontDropdown,
    bubble_duration_ms: u64,
    bubble_duration_text: String,
    page_duration_ms: u64,
    page_duration_text: String,
    default_font_size: f32,
    default_font_size_text: String,
    focus: usize,
}

pub enum ComicDubsSettingsModalResult {
    Consumed,
    Close,
    Save {
        font_family: Option<String>,
        bubble_duration_ms: u64,
        page_duration_ms: u64,
        default_font_size: f32,
    },
}

impl ComicDubsSettingsModal {
    pub fn new(
        fonts: Vec<String>,
        font_family: Option<String>,
        bubble_duration_ms: u64,
        page_duration_ms: u64,
        default_font_size: f32,
    ) -> Self {
        Self {
            font: FontDropdown::new(fonts, font_family),
            bubble_duration_ms,
            bubble_duration_text: format!("{bubble_duration_ms} ms"),
            page_duration_ms,
            page_duration_text: format!("{page_duration_ms} ms"),
            default_font_size,
            default_font_size_text: format!("{} px", default_font_size.round()),
            focus: 0,
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focus {
            0 => format!(
                "{}, {}",
                t("comic_dubs_settings.font"),
                self.font
                    .selected()
                    .unwrap_or_else(|| t("settings.default_font"))
            ),
            1 => format!(
                "{}, {} ms",
                t("comic_dubs_settings.bubble_duration"),
                self.bubble_duration_ms
            ),
            2 => format!(
                "{}, {} ms",
                t("comic_dubs_settings.page_duration"),
                self.page_duration_ms
            ),
            3 => format!(
                "{}, {} px",
                t("comic_dubs_settings.default_text_size"),
                self.default_font_size.round()
            ),
            4 => t("settings.save").to_string(),
            _ => t("project_settings.close").to_string(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ComicDubsSettingsModalResult {
        let card = card_rect(screen_w, screen_h);
        let font_rect = font_rect(card);
        if self.focus == 0
            && matches!(
                event,
                UiEvent::Activate
                    | UiEvent::CursorUp
                    | UiEvent::CursorDown
                    | UiEvent::KeyInput { .. }
            )
            && self.font.handle_event(event, font_rect)
        {
            return ComicDubsSettingsModalResult::Consumed;
        }
        if matches!(
            event,
            UiEvent::MousePress { .. }
                | UiEvent::DoubleClick { .. }
                | UiEvent::MouseMove { .. }
                | UiEvent::Scroll { .. }
        ) && self.font.handle_event(event, font_rect)
        {
            self.focus = 0;
            return ComicDubsSettingsModalResult::Consumed;
        }
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => ComicDubsSettingsModalResult::Close,
            UiEvent::KeyInput { text } if text == "\t" || text == "\u{b}" => {
                self.focus = if text == "\t" {
                    (self.focus + 1) % CONTROL_COUNT
                } else {
                    (self.focus + CONTROL_COUNT - 1) % CONTROL_COUNT
                };
                ComicDubsSettingsModalResult::Consumed
            }
            UiEvent::CursorUp => {
                self.focus = (self.focus + CONTROL_COUNT - 1) % CONTROL_COUNT;
                ComicDubsSettingsModalResult::Consumed
            }
            UiEvent::CursorDown => {
                self.focus = (self.focus + 1) % CONTROL_COUNT;
                ComicDubsSettingsModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.adjust(self.focus, -1);
                ComicDubsSettingsModalResult::Consumed
            }
            UiEvent::CursorRight => {
                self.adjust(self.focus, 1);
                ComicDubsSettingsModalResult::Consumed
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                match self.focus {
                    4 => self.save(),
                    5 => ComicDubsSettingsModalResult::Close,
                    _ => ComicDubsSettingsModalResult::Consumed,
                }
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ComicDubsSettingsModalResult::Close;
                }
                for control in 1..4 {
                    if self.minus_rect(card, control).contains(*x, *y) {
                        self.adjust(control, -1);
                        return ComicDubsSettingsModalResult::Consumed;
                    }
                    if self.plus_rect(card, control).contains(*x, *y) {
                        self.adjust(control, 1);
                        return ComicDubsSettingsModalResult::Consumed;
                    }
                }
                if save_rect(card).contains(*x, *y) {
                    return self.save();
                }
                if close_rect(card).contains(*x, *y) {
                    return ComicDubsSettingsModalResult::Close;
                }
                ComicDubsSettingsModalResult::Consumed
            }
            _ => ComicDubsSettingsModalResult::Consumed,
        }
    }

    fn adjust(&mut self, control: usize, delta: isize) {
        match control {
            1 => {
                self.bubble_duration_ms = adjust_duration(self.bubble_duration_ms, delta);
                self.bubble_duration_text = format!("{} ms", self.bubble_duration_ms);
            }
            2 => {
                self.page_duration_ms = adjust_duration(self.page_duration_ms, delta);
                self.page_duration_text = format!("{} ms", self.page_duration_ms);
            }
            3 => {
                self.default_font_size =
                    (self.default_font_size + delta as f32 * 2.0).clamp(6.0, 72.0);
                self.default_font_size_text = format!("{} px", self.default_font_size.round());
            }
            _ => {}
        }
    }

    fn save(&self) -> ComicDubsSettingsModalResult {
        ComicDubsSettingsModalResult::Save {
            font_family: self.font.selected_owned(),
            bubble_duration_ms: self.bubble_duration_ms,
            page_duration_ms: self.page_duration_ms,
            default_font_size: self.default_font_size,
        }
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = card_rect(screen_w, screen_h);
        push_quad(
            quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.0, 0.0, 0.0, 0.75],
            [0.0; 4],
            0.0,
        );
        push_quad(
            quads,
            card,
            [0.22, 0.22, 0.26, 1.0],
            [0.45, 0.45, 0.52, 0.8],
            14.0,
        );
        push_label(
            labels,
            t("comic_dubs_settings.title"),
            Rect {
                y: card.y + 12.0,
                height: 28.0,
                ..card
            },
            HAlign::Center,
            16.0,
            None,
        );

        let values = [
            &self.bubble_duration_text,
            &self.page_duration_text,
            &self.default_font_size_text,
        ];
        let names = [
            t("comic_dubs_settings.font"),
            t("comic_dubs_settings.bubble_duration"),
            t("comic_dubs_settings.page_duration"),
            t("comic_dubs_settings.default_text_size"),
        ];
        let font = font_rect(card);
        push_label(
            labels,
            names[0],
            Rect {
                x: card.x + 24.0,
                width: 190.0,
                ..font
            },
            HAlign::Left,
            12.0,
            None,
        );
        for control in 1..4 {
            let row = self.row_rect(card, control);
            push_label(
                labels,
                names[control],
                Rect {
                    width: 190.0,
                    ..row
                },
                HAlign::Left,
                12.0,
                None,
            );
            let minus = self.minus_rect(card, control);
            let plus = self.plus_rect(card, control);
            push_button(quads, labels, minus, "−");
            push_button(quads, labels, plus, "+");
            push_label(
                labels,
                values[control - 1],
                Rect {
                    x: minus.x + minus.width + 4.0,
                    width: plus.x - minus.x - minus.width - 8.0,
                    ..row
                },
                HAlign::Center,
                12.0,
                None,
            );
        }
        self.font
            .render(quads, labels, font, t("settings.default_font"));
        push_button(quads, labels, save_rect(card), t("settings.save"));
        push_button(quads, labels, close_rect(card), t("project_settings.close"));
        let focus = match self.focus {
            0 => font,
            1..=3 => self.row_rect(card, self.focus),
            4 => save_rect(card),
            _ => close_rect(card),
        };
        push_outline(quads, focus);
    }

    fn row_rect(&self, card: Rect, index: usize) -> Rect {
        Rect {
            x: card.x + 24.0,
            y: card.y + 58.0 + index as f32 * 48.0 + self.font.expanded_height(),
            width: card.width - 48.0,
            height: 34.0,
        }
    }

    fn minus_rect(&self, card: Rect, index: usize) -> Rect {
        Rect {
            x: card.x + 220.0,
            width: 34.0,
            ..self.row_rect(card, index)
        }
    }

    fn plus_rect(&self, card: Rect, index: usize) -> Rect {
        Rect {
            x: card.x + card.width - 58.0,
            width: 34.0,
            ..self.row_rect(card, index)
        }
    }
}

fn adjust_duration(value: u64, delta: isize) -> u64 {
    value.saturating_add_signed(delta as i64 * 250).min(60_000)
}

fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
    Rect {
        x: (screen_w - WIDTH) / 2.0,
        y: (screen_h - HEIGHT) / 2.0,
        width: WIDTH,
        height: HEIGHT,
    }
}

fn font_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 220.0,
        y: card.y + 58.0,
        width: card.width - 278.0,
        height: 34.0,
    }
}

fn save_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + 100.0,
        y: card.y + HEIGHT - 52.0,
        width: 140.0,
        height: 34.0,
    }
}

fn close_rect(card: Rect) -> Rect {
    Rect {
        x: card.x + card.width - 220.0,
        y: card.y + HEIGHT - 52.0,
        width: 120.0,
        height: 34.0,
    }
}

fn push_button<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
) {
    push_quad(
        quads,
        rect,
        [0.15, 0.15, 0.18, 1.0],
        [0.35, 0.35, 0.42, 0.8],
        6.0,
    );
    push_label(labels, text, rect, HAlign::Center, 13.0, None);
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    font_family: Option<&'a str>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(font_size),
        color_override: None,
        font_family_override: font_family,
    });
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_outline(quads: &mut Vec<QuadInstance>, rect: Rect) {
    push_quad(quads, rect, [0.0; 4], [0.38, 0.65, 1.0, 1.0], 8.0);
    quads.last_mut().unwrap().border_width = 2.5;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_move_in_250_ms_steps_and_stay_bounded() {
        assert_eq!(adjust_duration(250, -1), 0);
        assert_eq!(adjust_duration(250, 1), 500);
        assert_eq!(adjust_duration(60_000, 1), 60_000);
    }
}
