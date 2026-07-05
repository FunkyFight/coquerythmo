use super::widget::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::i18n::t;

const CARD_W: f32 = 700.0;
const CARD_H: f32 = 492.0;
const MIN_CARD_W: f32 = 420.0;
const MIN_CARD_H: f32 = 330.0;
const CARD_MARGIN: f32 = 32.0;
const BODY_PAD_X: f32 = 30.0;
const BODY_TOP: f32 = 118.0;
const BODY_BOTTOM: f32 = 76.0;
const BODY_INSET_X: f32 = 18.0;
const BODY_INSET_Y: f32 = 16.0;
const LINE_H: f32 = 18.0;
const BODY_FONT_SIZE: f32 = 11.0;
const HEADING_FONT_SIZE: f32 = 12.5;
const CHAR_WIDTH: f32 = 6.2;
const SCROLL_STEP_LINES: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoteLineKind {
    Heading,
    Bullet,
    BulletContinuation,
    Text,
    Blank,
}

struct NoteLine {
    text: String,
    kind: NoteLineKind,
}

pub struct WhatsNewModal {
    version_label: String,
    lines: Vec<NoteLine>,
    scroll_offset: usize,
}

pub enum WhatsNewResult {
    Consumed,
    Close,
}

impl WhatsNewModal {
    pub fn new(version: impl Into<String>, body: impl Into<String>) -> Self {
        let version = version.into();
        let max_chars = ((CARD_W - BODY_PAD_X * 2.0 - BODY_INSET_X * 2.0 - 46.0) / CHAR_WIDTH)
            .floor()
            .max(42.0) as usize;
        Self {
            version_label: format!("{} {version}", t("whats_new.version")),
            lines: format_release_notes(&body.into(), max_chars),
            scroll_offset: 0,
        }
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        let width = CARD_W.min((screen_w - CARD_MARGIN * 2.0).max(MIN_CARD_W));
        let height = CARD_H.min((screen_h - CARD_MARGIN * 2.0).max(MIN_CARD_H));
        Rect {
            x: (screen_w - width) / 2.0,
            y: (screen_h - height) / 2.0,
            width,
            height,
        }
    }

    fn body_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + BODY_PAD_X,
            y: card.y + BODY_TOP,
            width: card.width - BODY_PAD_X * 2.0,
            height: card.height - BODY_TOP - BODY_BOTTOM,
        }
    }

    fn body_text_rect(body: Rect) -> Rect {
        Rect {
            x: body.x + BODY_INSET_X,
            y: body.y + BODY_INSET_Y,
            width: body.width - BODY_INSET_X * 2.0 - 14.0,
            height: body.height - BODY_INSET_Y * 2.0,
        }
    }

    fn close_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + card.width - 152.0,
            y: card.y + card.height - 52.0,
            width: 120.0,
            height: 34.0,
        }
    }

    fn visible_line_count(card: Rect) -> usize {
        let body = Self::body_text_rect(Self::body_rect(card));
        (body.height / LINE_H).floor().max(1.0) as usize
    }

    fn max_scroll_offset(&self, card: Rect) -> usize {
        self.lines
            .len()
            .saturating_sub(Self::visible_line_count(card))
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> WhatsNewResult {
        let card = Self::card_rect(screen_w, screen_h);
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" || text == "\r" || text == "\n" => {
                WhatsNewResult::Close
            }
            UiEvent::Scroll { delta, .. } => {
                let max_offset = self.max_scroll_offset(card);
                if max_offset > 0 {
                    if *delta > 0.0 {
                        self.scroll_offset = self.scroll_offset.saturating_sub(SCROLL_STEP_LINES);
                    } else if *delta < 0.0 {
                        self.scroll_offset =
                            (self.scroll_offset + SCROLL_STEP_LINES).min(max_offset);
                    }
                }
                WhatsNewResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if Self::close_rect(card).contains(*x, *y) {
                    return WhatsNewResult::Close;
                }
                WhatsNewResult::Consumed
            }
            _ => WhatsNewResult::Consumed,
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
        let body = Self::body_rect(card);
        let body_text = Self::body_text_rect(body);
        let close = Self::close_rect(card);

        push_quad(
            overlay_quads,
            Rect {
                x: 0.0,
                y: 0.0,
                width: screen_w,
                height: screen_h,
            },
            [0.015, 0.018, 0.026, 0.82],
            [0.015, 0.018, 0.026, 0.82],
            [0.0; 4],
            0.0,
            0.0,
        );

        push_quad(
            overlay_quads,
            Rect {
                x: card.x + 8.0,
                y: card.y + 10.0,
                width: card.width - 16.0,
                height: card.height - 2.0,
            },
            [0.0, 0.0, 0.0, 0.24],
            [0.0, 0.0, 0.0, 0.24],
            [0.0; 4],
            0.0,
            18.0,
        );

        push_quad(
            overlay_quads,
            card,
            [0.125, 0.127, 0.150, 1.0],
            [0.070, 0.074, 0.095, 1.0],
            [0.42, 0.43, 0.52, 0.82],
            1.0,
            18.0,
        );

        push_label(
            labels,
            t("whats_new.title"),
            Rect {
                x: card.x + 32.0,
                y: card.y + 28.0,
                width: card.width - 64.0,
                height: 32.0,
            },
            HAlign::Left,
            Some(22.0),
            Some([246, 240, 226]),
        );
        push_label(
            labels,
            &self.version_label,
            Rect {
                x: card.x + card.width - 142.0,
                y: card.y + 28.0,
                width: 108.0,
                height: 18.0,
            },
            HAlign::Right,
            Some(10.0),
            Some([165, 168, 181]),
        );

        push_quad(
            overlay_quads,
            body,
            [0.052, 0.056, 0.074, 0.98],
            [0.038, 0.042, 0.060, 0.98],
            [0.24, 0.26, 0.34, 0.92],
            1.0,
            12.0,
        );
        push_quad(
            overlay_quads,
            Rect {
                x: body.x + 1.0,
                y: body.y + 1.0,
                width: 4.0,
                height: body.height - 2.0,
            },
            [0.92, 0.62, 0.26, 0.92],
            [0.48, 0.54, 0.88, 0.92],
            [0.0; 4],
            0.0,
            12.0,
        );

        let visible_count = Self::visible_line_count(card);
        let max_offset = self.max_scroll_offset(card);
        let start = self.scroll_offset.min(max_offset);
        let end = (start + visible_count).min(self.lines.len());

        for (visible_index, line) in self.lines[start..end].iter().enumerate() {
            let y = body_text.y + visible_index as f32 * LINE_H;
            match line.kind {
                NoteLineKind::Blank => {}
                NoteLineKind::Heading => {
                    push_quad(
                        overlay_quads,
                        Rect {
                            x: body_text.x,
                            y: y + 4.0,
                            width: 3.0,
                            height: 10.0,
                        },
                        [0.91, 0.60, 0.25, 0.95],
                        [0.91, 0.60, 0.25, 0.95],
                        [0.0; 4],
                        0.0,
                        1.5,
                    );
                    push_label(
                        labels,
                        &line.text,
                        Rect {
                            x: body_text.x + 12.0,
                            y: y - 1.0,
                            width: body_text.width - 12.0,
                            height: LINE_H,
                        },
                        HAlign::Left,
                        Some(HEADING_FONT_SIZE),
                        Some([245, 211, 151]),
                    );
                }
                NoteLineKind::Bullet => {
                    push_quad(
                        overlay_quads,
                        Rect {
                            x: body_text.x + 2.0,
                            y: y + 7.0,
                            width: 5.0,
                            height: 5.0,
                        },
                        [0.70, 0.78, 1.0, 0.95],
                        [0.70, 0.78, 1.0, 0.95],
                        [0.0; 4],
                        0.0,
                        2.5,
                    );
                    push_body_line(
                        labels,
                        &line.text,
                        body_text.x + 18.0,
                        y,
                        body_text.width - 18.0,
                    );
                }
                NoteLineKind::BulletContinuation => {
                    push_body_line(
                        labels,
                        &line.text,
                        body_text.x + 18.0,
                        y,
                        body_text.width - 18.0,
                    );
                }
                NoteLineKind::Text => {
                    push_body_line(labels, &line.text, body_text.x, y, body_text.width);
                }
            }
        }

        if max_offset > 0 {
            let track = Rect {
                x: body.x + body.width - 12.0,
                y: body.y + 14.0,
                width: 4.0,
                height: body.height - 28.0,
            };
            push_quad(
                overlay_quads,
                track,
                [0.18, 0.19, 0.24, 0.82],
                [0.18, 0.19, 0.24, 0.82],
                [0.0; 4],
                0.0,
                2.0,
            );
            let thumb_h = (track.height * visible_count as f32 / self.lines.len() as f32)
                .max(28.0)
                .min(track.height);
            let scroll_ratio = start as f32 / max_offset as f32;
            let thumb_y = track.y + (track.height - thumb_h) * scroll_ratio;
            push_quad(
                overlay_quads,
                Rect {
                    x: track.x,
                    y: thumb_y,
                    width: track.width,
                    height: thumb_h,
                },
                [0.78, 0.58, 0.30, 0.96],
                [0.48, 0.56, 0.88, 0.96],
                [0.0; 4],
                0.0,
                2.0,
            );
        }

        if max_offset > 0 {
            push_label(
                labels,
                t("whats_new.scroll_hint"),
                Rect {
                    x: card.x + 32.0,
                    y: close.y + 7.0,
                    width: close.x - card.x - 50.0,
                    height: 20.0,
                },
                HAlign::Left,
                Some(9.0),
                Some([132, 137, 154]),
            );
        }

        push_quad(
            overlay_quads,
            close,
            [0.78, 0.45, 0.24, 1.0],
            [0.55, 0.28, 0.18, 1.0],
            [0.98, 0.70, 0.38, 0.78],
            1.0,
            9.0,
        );
        push_label(
            labels,
            t("whats_new.close"),
            close,
            HAlign::Center,
            Some(11.0),
            Some([255, 244, 230]),
        );
    }
}

fn push_body_line<'a>(labels: &mut Vec<LabelInfo<'a>>, text: &'a str, x: f32, y: f32, width: f32) {
    push_label(
        labels,
        text,
        Rect {
            x,
            y,
            width,
            height: LINE_H,
        },
        HAlign::Left,
        Some(BODY_FONT_SIZE),
        Some([218, 222, 233]),
    );
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

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    color_bottom: [f32; 4],
    border_color: [f32; 4],
    border_width: f32,
    border_radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom,
        border_color,
        border_width,
        border_radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn format_release_notes(body: &str, max_chars: usize) -> Vec<NoteLine> {
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in normalized.lines() {
        let Some((kind, line)) = cleanup_markdown_line(raw_line) else {
            if !previous_blank && !lines.is_empty() {
                lines.push(NoteLine {
                    text: String::new(),
                    kind: NoteLineKind::Blank,
                });
                previous_blank = true;
            }
            continue;
        };

        lines.extend(wrap_line(&line, kind, max_chars));
        previous_blank = false;
    }

    while lines
        .last()
        .is_some_and(|line| line.kind == NoteLineKind::Blank)
    {
        lines.pop();
    }

    if lines.is_empty() {
        lines.push(NoteLine {
            text: t("whats_new.empty").to_string(),
            kind: NoteLineKind::Text,
        });
    }

    lines
}

fn cleanup_markdown_line(raw_line: &str) -> Option<(NoteLineKind, String)> {
    let mut line = raw_line.trim();
    if line.is_empty() || line.chars().all(|ch| ch == '-' || ch == '*' || ch == '_') {
        return None;
    }

    let mut kind = NoteLineKind::Text;
    if line.starts_with('#') {
        kind = NoteLineKind::Heading;
        line = line.trim_start_matches('#').trim_start();
    } else if let Some(stripped) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        kind = NoteLineKind::Bullet;
        line = stripped;
    } else if let Some(stripped) = strip_numbered_prefix(line) {
        kind = NoteLineKind::Bullet;
        line = stripped;
    }

    let text = strip_inline_markdown(line);
    if text.is_empty() {
        None
    } else {
        Some((kind, text))
    }
}

fn strip_numbered_prefix(line: &str) -> Option<&str> {
    let (prefix, rest) = line.split_once(". ")?;
    if !prefix.is_empty() && prefix.len() <= 3 && prefix.chars().all(|ch| ch.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
}

fn strip_inline_markdown(line: &str) -> String {
    line.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .trim()
        .to_string()
}

fn wrap_line(line: &str, kind: NoteLineKind, max_chars: usize) -> Vec<NoteLine> {
    let max_chars = max_chars.max(24);
    let mut wrapped = Vec::new();
    let mut current = String::new();

    for word in line.split_whitespace() {
        if word.len() > max_chars {
            if !current.is_empty() {
                push_wrapped_line(&mut wrapped, current, kind);
                current = String::new();
            }
            for chunk in split_long_word(word, max_chars) {
                push_wrapped_line(&mut wrapped, chunk, kind);
            }
            continue;
        }

        let next_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };

        if next_len > max_chars && !current.is_empty() {
            push_wrapped_line(&mut wrapped, current, kind);
            current = String::new();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        push_wrapped_line(&mut wrapped, current, kind);
    }

    if wrapped.is_empty() {
        wrapped.push(NoteLine {
            text: line.to_string(),
            kind,
        });
    }

    wrapped
}

fn push_wrapped_line(lines: &mut Vec<NoteLine>, text: String, kind: NoteLineKind) {
    let kind = if kind == NoteLineKind::Bullet && lines.last().is_some() {
        NoteLineKind::BulletContinuation
    } else {
        kind
    };
    lines.push(NoteLine { text, kind });
}

fn split_long_word(word: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        current.push(ch);
        if current.chars().count() >= max_chars {
            chunks.push(current);
            current = String::new();
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
