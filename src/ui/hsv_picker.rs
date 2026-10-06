//! Hue / saturation / value colour picker drawn only with quads, so it can
//! live inside any modal without its own textures.
//!
//! The saturation/value square is a row of thin columns: each column fades
//! from its fully bright colour at the top to black at the bottom, which is
//! exactly the value axis since HSV value scales the colour linearly.

use super::primitives::{QuadInstance, Rect};

const SQUARE_COLUMNS: usize = 96;
const HUE_SEGMENTS: usize = 120;
const HUE_BAR_H: f32 = 14.0;
const GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragPart {
    Square,
    Hue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsvPicker {
    /// Degrees, 0..360.
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    dragging: Option<DragPart>,
}

impl HsvPicker {
    pub fn new(color: [f32; 4]) -> Self {
        let (hue, saturation, value) = rgb_to_hsv(color[0], color[1], color[2]);
        Self {
            hue,
            saturation,
            value,
            dragging: None,
        }
    }

    /// Follows a colour changed elsewhere (hex field), keeping the hue of
    /// greys so the square does not jump back to red.
    pub fn set_color(&mut self, color: [f32; 4]) {
        let (hue, saturation, value) = rgb_to_hsv(color[0], color[1], color[2]);
        if saturation > 0.0 && value > 0.0 {
            self.hue = hue;
        }
        self.saturation = saturation;
        self.value = value;
    }

    pub fn color(&self) -> [f32; 4] {
        let [r, g, b] = hsv_to_rgb(self.hue, self.saturation, self.value);
        [r, g, b, 1.0]
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Saturation/value square and hue bar inside `area`.
    pub fn layout(area: Rect) -> (Rect, Rect) {
        let square = Rect {
            height: (area.height - HUE_BAR_H - GAP).max(10.0),
            ..area
        };
        let hue = Rect {
            y: square.y + square.height + GAP,
            height: HUE_BAR_H,
            ..area
        };
        (square, hue)
    }

    /// Starts a drag when the press lands on the picker. Returns the new
    /// colour when it changed.
    pub fn press(&mut self, area: Rect, x: f32, y: f32) -> Option<[f32; 4]> {
        let (square, hue) = Self::layout(area);
        if square.contains(x, y) {
            self.dragging = Some(DragPart::Square);
        } else if hue.contains(x, y) {
            self.dragging = Some(DragPart::Hue);
        } else {
            return None;
        }
        self.drag(area, x, y)
    }

    /// Moves the active drag. Returns the new colour when it changed.
    pub fn drag(&mut self, area: Rect, x: f32, y: f32) -> Option<[f32; 4]> {
        let (square, hue) = Self::layout(area);
        let before = *self;
        match self.dragging? {
            DragPart::Square => {
                self.saturation = ((x - square.x) / square.width).clamp(0.0, 1.0);
                self.value = 1.0 - ((y - square.y) / square.height).clamp(0.0, 1.0);
            }
            DragPart::Hue => {
                self.hue = ((x - hue.x) / hue.width).clamp(0.0, 1.0) * 360.0;
            }
        }
        (before.hue != self.hue
            || before.saturation != self.saturation
            || before.value != self.value)
            .then(|| self.color())
    }

    pub fn release(&mut self) -> bool {
        self.dragging.take().is_some()
    }

    /// Keyboard nudge: `dx` moves saturation, `dy` moves value (up = brighter).
    pub fn nudge(&mut self, dx: f32, dy: f32) -> [f32; 4] {
        self.saturation = (self.saturation + dx).clamp(0.0, 1.0);
        self.value = (self.value + dy).clamp(0.0, 1.0);
        self.color()
    }

    pub fn nudge_hue(&mut self, degrees: f32) -> [f32; 4] {
        self.hue = (self.hue + degrees).rem_euclid(360.0);
        self.color()
    }

    pub fn render(&self, quads: &mut Vec<QuadInstance>, area: Rect) {
        let (square, hue_bar) = Self::layout(area);
        let column_w = square.width / SQUARE_COLUMNS as f32;
        for column in 0..SQUARE_COLUMNS {
            let saturation = (column as f32 + 0.5) / SQUARE_COLUMNS as f32;
            let [r, g, b] = hsv_to_rgb(self.hue, saturation, 1.0);
            push(
                quads,
                Rect {
                    x: square.x + column as f32 * column_w,
                    y: square.y,
                    // Slight overlap hides seams between columns.
                    width: column_w + 0.6,
                    height: square.height,
                },
                [r, g, b, 1.0],
                [0.0, 0.0, 0.0, 1.0],
                0.0,
                [0.0; 4],
            );
        }
        push(quads, square, [0.0; 4], [0.0; 4], 0.0, [0.45, 0.45, 0.52, 0.9]);

        let segment_w = hue_bar.width / HUE_SEGMENTS as f32;
        for segment in 0..HUE_SEGMENTS {
            let hue = (segment as f32 + 0.5) / HUE_SEGMENTS as f32 * 360.0;
            let [r, g, b] = hsv_to_rgb(hue, 1.0, 1.0);
            push(
                quads,
                Rect {
                    x: hue_bar.x + segment as f32 * segment_w,
                    width: segment_w + 0.6,
                    ..hue_bar
                },
                [r, g, b, 1.0],
                [r, g, b, 1.0],
                0.0,
                [0.0; 4],
            );
        }
        push(quads, hue_bar, [0.0; 4], [0.0; 4], 0.0, [0.45, 0.45, 0.52, 0.9]);

        // Markers: a ring on the square, a bar on the hue strip.
        let marker_x = square.x + self.saturation * square.width;
        let marker_y = square.y + (1.0 - self.value) * square.height;
        let ring = 10.0;
        let ring_border = if self.value > 0.55 && self.saturation < 0.45 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        push(
            quads,
            Rect {
                x: marker_x - ring / 2.0,
                y: marker_y - ring / 2.0,
                width: ring,
                height: ring,
            },
            self.color(),
            self.color(),
            ring / 2.0,
            ring_border,
        );
        if let Some(last) = quads.last_mut() {
            last.border_width = 2.0;
        }
        let hue_x = hue_bar.x + self.hue / 360.0 * hue_bar.width;
        push(
            quads,
            Rect {
                x: hue_x - 2.0,
                y: hue_bar.y - 2.0,
                width: 4.0,
                height: hue_bar.height + 4.0,
            },
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            1.0,
            [0.0, 0.0, 0.0, 0.8],
        );
    }
}

fn push(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    top: [f32; 4],
    bottom: [f32; 4],
    radius: f32,
    border: [f32; 4],
) {
    // Shown like the band: colours are sRGB values on an sRGB surface.
    let linear = super::color_picker::srgb_to_linear;
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: linear(top),
        color_bottom: linear(bottom),
        border_color: linear(border),
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    [r + m, g + m, b + m]
}

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max <= f32::EPSILON { 0.0 } else { delta / max };
    (hue, saturation, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 150.0,
        }
    }

    #[test]
    fn hsv_round_trips() {
        for color in [[1.0, 0.0, 0.0], [0.2, 0.6, 0.9], [0.5, 0.5, 0.5], [0.0, 0.0, 0.0]] {
            let (h, s, v) = rgb_to_hsv(color[0], color[1], color[2]);
            let back = hsv_to_rgb(h, s, v);
            for (a, b) in color.iter().zip(back) {
                assert!((a - b).abs() < 1e-4, "{color:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn clicking_the_square_picks_saturation_and_value() {
        let mut picker = HsvPicker::new([1.0, 0.0, 0.0, 1.0]);
        let (square, _) = HsvPicker::layout(area());
        let color = picker
            .press(area(), square.x + square.width, square.y + square.height)
            .unwrap();
        assert_eq!(color, [0.0, 0.0, 0.0, 1.0]);
        assert!(picker.is_dragging());
        let color = picker.drag(area(), square.x + square.width, square.y).unwrap();
        assert_eq!(color, [1.0, 0.0, 0.0, 1.0]);
        assert!(picker.release());
    }

    #[test]
    fn the_hue_bar_changes_the_hue() {
        let mut picker = HsvPicker::new([1.0, 0.0, 0.0, 1.0]);
        let (_, hue) = HsvPicker::layout(area());
        let color = picker
            .press(area(), hue.x + hue.width / 3.0, hue.y + 2.0)
            .unwrap();
        assert!(color[1] > 0.99 && color[0] < 0.01, "{color:?}");
    }

    #[test]
    fn presses_outside_do_nothing() {
        let mut picker = HsvPicker::new([0.5, 0.5, 0.5, 1.0]);
        assert!(picker.press(area(), 0.0, 0.0).is_none());
        assert!(!picker.is_dragging());
    }
}
