use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 540.0;
const CARD_H: f32 = 300.0;
const DETAIL_LINE_H: f32 = 16.0;
const MAX_DETAIL_LINES: usize = 8;
const CHAR_WIDTH: f32 = 6.2;

pub struct ProxyErrorModal {
    detail_lines: Vec<String>,
}

pub enum ProxyErrorResult {
    Consumed,
    Close,
}

impl ProxyErrorModal {
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail_lines: wrap_detail(&detail.into(), CARD_W - 72.0, MAX_DETAIL_LINES),
        }
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> ProxyErrorResult {
        let card = Self::card_rect(screen_w, screen_h);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" || text == "\r" || text == "\n" => {
                ProxyErrorResult::Close
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return ProxyErrorResult::Close;
                }

                let close_rect = Rect {
                    x: card.x + (card.width - 130.0) / 2.0,
                    y: card.y + CARD_H - 48.0,
                    width: 130.0,
                    height: 34.0,
                };
                if close_rect.contains(*x, *y) {
                    return ProxyErrorResult::Close;
                }

                ProxyErrorResult::Consumed
            }
            _ => ProxyErrorResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card_rect(screen_w, screen_h);

        overlay_quads.push(QuadInstance {
            rect: [0.0, 0.0, screen_w, screen_h],
            color: [0.0, 0.0, 0.0, 0.78],
            color_bottom: [0.0, 0.0, 0.0, 0.78],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        overlay_quads.push(QuadInstance {
            rect: [card.x, card.y, card.width, card.height],
            color: [0.24, 0.19, 0.20, 1.0],
            color_bottom: [0.16, 0.13, 0.15, 1.0],
            border_color: [0.85, 0.25, 0.25, 0.85],
            border_width: 1.5,
            border_radius: 14.0,
            shadow_offset: [0.0, 4.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 10.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        push_label(
            labels,
            t("proxy_error.title"),
            Rect {
                x: card.x,
                y: card.y + 14.0,
                width: card.width,
                height: 24.0,
            },
            HAlign::Center,
            Some(15.0),
            Some([245, 120, 120]),
        );

        push_label(
            labels,
            t("proxy_error.message"),
            Rect {
                x: card.x + 24.0,
                y: card.y + 50.0,
                width: card.width - 48.0,
                height: 22.0,
            },
            HAlign::Center,
            Some(12.0),
            Some([220, 210, 215]),
        );

        let detail_rect = Rect {
            x: card.x + 24.0,
            y: card.y + 88.0,
            width: card.width - 48.0,
            height: DETAIL_LINE_H * MAX_DETAIL_LINES as f32 + 18.0,
        };
        overlay_quads.push(QuadInstance {
            rect: [
                detail_rect.x,
                detail_rect.y,
                detail_rect.width,
                detail_rect.height,
            ],
            color: [0.08, 0.07, 0.08, 1.0],
            color_bottom: [0.08, 0.07, 0.08, 1.0],
            border_color: [0.42, 0.22, 0.24, 0.8],
            border_width: 1.0,
            border_radius: 6.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        for (index, line) in self.detail_lines.iter().enumerate() {
            push_label(
                labels,
                line,
                Rect {
                    x: detail_rect.x + 10.0,
                    y: detail_rect.y + 9.0 + index as f32 * DETAIL_LINE_H,
                    width: detail_rect.width - 20.0,
                    height: DETAIL_LINE_H,
                },
                HAlign::Left,
                Some(10.0),
                Some([210, 190, 195]),
            );
        }

        let close_rect = Rect {
            x: card.x + (card.width - 130.0) / 2.0,
            y: card.y + CARD_H - 48.0,
            width: 130.0,
            height: 34.0,
        };
        overlay_quads.push(QuadInstance {
            rect: [
                close_rect.x,
                close_rect.y,
                close_rect.width,
                close_rect.height,
            ],
            color: [0.55, 0.25, 0.22, 1.0],
            color_bottom: [0.42, 0.18, 0.17, 1.0],
            border_color: [0.75, 0.35, 0.32, 0.85],
            border_width: 1.0,
            border_radius: 7.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.3],
            shadow_blur: 4.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        push_label(
            labels,
            t("proxy_error.close"),
            close_rect,
            HAlign::Center,
            Some(12.0),
            Some([235, 235, 240]),
        );
    }
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    h_align: HAlign,
    font_size: Option<f32>,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: font_size,
        color_override: color,
        font_family_override: None,
    });
}

fn wrap_detail(detail: &str, max_width: f32, max_lines: usize) -> Vec<String> {
    let max_chars = (max_width / CHAR_WIDTH).floor().max(16.0) as usize;
    let mut lines = Vec::new();

    for raw_line in detail.lines().flat_map(|line| line.split("\\n")) {
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let next_len = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };

            if next_len > max_chars && !current.is_empty() {
                lines.push(current);
                current = String::new();
            }

            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(t("proxy_error.unknown").to_string());
    }

    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            if last.len() + 4 <= max_chars {
                last.push_str(" ...");
            } else {
                last.truncate(max_chars.saturating_sub(4));
                last.push_str(" ...");
            }
        }
    }

    lines
}
