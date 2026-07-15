//! Shared rendering primitives for native context menus.

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, VAlign};

pub const MARGIN: f32 = 8.0;

pub fn clamped_origin(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    screen_w: f32,
    screen_h: f32,
) -> (f32, f32) {
    (
        x.clamp(MARGIN, (screen_w - width - MARGIN).max(MARGIN)),
        y.clamp(MARGIN, (screen_h - height - MARGIN).max(MARGIN)),
    )
}

pub fn render_panel(quads: &mut Vec<QuadInstance>, rect: Rect) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.16, 0.16, 0.19, 0.98],
        color_bottom: [0.11, 0.11, 0.14, 0.98],
        border_color: [0.42, 0.42, 0.50, 0.85],
        border_width: 1.0,
        border_radius: 0.0,
        shadow_offset: [0.0, 4.0],
        shadow_color: [0.0, 0.0, 0.0, 0.45],
        shadow_blur: 10.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

pub fn render_item<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    hovered: bool,
    arrow: bool,
    font_size: f32,
) {
    if hovered {
        quads.push(QuadInstance {
            rect: [
                rect.x + 3.0,
                rect.y + 2.0,
                rect.width - 6.0,
                rect.height - 4.0,
            ],
            color: [0.31, 0.40, 0.72, 0.85],
            color_bottom: [0.24, 0.32, 0.62, 0.85],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0, 2.0],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
    labels.push(LabelInfo {
        text,
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y,
            width: rect.width - if arrow { 28.0 } else { 20.0 },
            height: rect.height,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(font_size),
        color_override: Some([230, 230, 238]),
        font_family_override: None,
    });
    if arrow {
        labels.push(LabelInfo {
            text: ">",
            bounds: Rect {
                x: rect.x + rect.width - 24.0,
                y: rect.y,
                width: 16.0,
                height: rect.height,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(font_size),
            color_override: Some([190, 190, 205]),
            font_family_override: None,
        });
    }
}
