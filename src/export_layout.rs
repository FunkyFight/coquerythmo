//! Placement of the video and of the rythmo band in an exported MP4.
//!
//! The default layout is the historical one: the video fitted in the area
//! above the band, the band across the full width at the bottom. Each layer
//! can then be moved and stretched independently.

use serde::{Deserialize, Serialize};

pub const MIN_OFFSET: f32 = -1.0;
pub const MAX_OFFSET: f32 = 1.0;
pub const MIN_SCALE: f32 = 0.1;
pub const MAX_SCALE: f32 = 4.0;

/// Move and stretch of one layer, relative to its default placement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTransform {
    /// Horizontal move, as a fraction of the output width.
    pub offset_x: f32,
    /// Vertical move, as a fraction of the output height.
    pub offset_y: f32,
    /// Horizontal stretch of the layer around its centre.
    pub scale_x: f32,
    /// Vertical stretch of the layer around its centre.
    pub scale_y: f32,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }
}

impl LayerTransform {
    pub fn normalized(self) -> Self {
        let clamp = |value: f32, min: f32, max: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        Self {
            offset_x: clamp(self.offset_x, MIN_OFFSET, MAX_OFFSET, 0.0),
            offset_y: clamp(self.offset_y, MIN_OFFSET, MAX_OFFSET, 0.0),
            scale_x: clamp(self.scale_x, MIN_SCALE, MAX_SCALE, 1.0),
            scale_y: clamp(self.scale_y, MIN_SCALE, MAX_SCALE, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportLayout {
    pub video: LayerTransform,
    pub band: LayerTransform,
}

impl ExportLayout {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn normalized(self) -> Self {
        Self {
            video: self.video.normalized(),
            band: self.band.normalized(),
        }
    }
}

/// Pixel rectangle on the output canvas. It may extend past the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposedLayout {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub video: PixelRect,
    pub band: PixelRect,
}

fn even_size(value: f32) -> u32 {
    let rounded = value.round().max(2.0) as u32;
    rounded + (rounded & 1)
}

/// Places both layers on a `canvas_width × canvas_height` output whose band,
/// at its default size, is `band_height` pixels tall.
pub fn compose(
    canvas_width: u32,
    canvas_height: u32,
    band_height: u32,
    source_width: u32,
    source_height: u32,
    layout: &ExportLayout,
) -> ComposedLayout {
    let layout = layout.normalized();
    let width = canvas_width as f32;
    let height = canvas_height as f32;
    let band_h = band_height.min(canvas_height) as f32;
    let video_area_h = (height - band_h).max(2.0);

    let fit = (width / source_width.max(1) as f32).min(video_area_h / source_height.max(1) as f32);
    let video_w = even_size(source_width.max(1) as f32 * fit * layout.video.scale_x);
    let video_h = even_size(source_height.max(1) as f32 * fit * layout.video.scale_y);
    let video = PixelRect {
        x: ((width - video_w as f32) / 2.0 + layout.video.offset_x * width).round() as i32,
        y: ((video_area_h - video_h as f32) / 2.0 + layout.video.offset_y * height).round() as i32,
        width: video_w,
        height: video_h,
    };

    let band_w = even_size(width * layout.band.scale_x);
    let band_h_scaled = even_size(band_h * layout.band.scale_y);
    let band = PixelRect {
        x: ((width - band_w as f32) / 2.0 + layout.band.offset_x * width).round() as i32,
        y: (video_area_h + (band_h - band_h_scaled as f32) / 2.0 + layout.band.offset_y * height)
            .round() as i32,
        width: band_w,
        height: band_h_scaled,
    };

    ComposedLayout {
        canvas_width,
        canvas_height,
        video,
        band,
    }
}

/// Size, in output pixels, of the video and of the band at 100 % (before the
/// layout stretches them), as `compose` places them.
pub fn base_sizes(
    canvas_width: u32,
    canvas_height: u32,
    band_height: u32,
    source_width: u32,
    source_height: u32,
) -> ([f32; 2], [f32; 2]) {
    let width = canvas_width as f32;
    let band_h = band_height.min(canvas_height) as f32;
    let video_area_h = (canvas_height as f32 - band_h).max(2.0);
    let source_w = source_width.max(1) as f32;
    let source_h = source_height.max(1) as f32;
    let fit = (width / source_w).min(video_area_h / source_h);
    ([source_w * fit, source_h * fit], [width, band_h])
}

/// Largest supersampling of the band at export.
pub const MAX_BAND_SUPERSAMPLE: f32 = 4.0;

/// How much wider than the output the band is rendered so that ffmpeg only
/// ever shrinks it: an enlarged band keeps sharp text instead of stretching
/// pixels.
pub fn band_supersample_factor(layout: &ExportLayout) -> f32 {
    let band = layout.normalized().band;
    band.scale_x
        .max(band.scale_y)
        .clamp(1.0, MAX_BAND_SUPERSAMPLE)
}

/// Width at which the band is rendered for an output `output_width` wide.
/// The band renderer scales everything with its width (text, slots, pixels
/// per frame), so a wider render shows the same time window, only sharper.
/// Always even (NV12) and at most `max_width`.
pub fn band_render_width(output_width: u32, layout: &ExportLayout, max_width: u32) -> u32 {
    let factor = band_supersample_factor(layout);
    let width = (output_width as f32 * factor).ceil() as u32;
    let width = width + (width & 1);
    width.min(max_width & !1).max(output_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_sizes_match_the_default_composition() {
        let (video, band) = base_sizes(1920, 1080, 180, 1920, 1080);
        let composed = compose(1920, 1080, 180, 1920, 1080, &ExportLayout::default());
        assert!((video[0] - composed.video.width as f32).abs() <= 2.0);
        assert!((video[1] - composed.video.height as f32).abs() <= 2.0);
        assert_eq!(band, [1920.0, 180.0]);
    }

    #[test]
    fn enlarged_bands_are_rendered_wider_then_shrunk() {
        let mut layout = ExportLayout::default();
        assert_eq!(band_supersample_factor(&layout), 1.0);
        assert_eq!(band_render_width(1920, &layout, 8192), 1920);
        // Shrinking never renders narrower than the output.
        layout.band.scale_x = 0.5;
        assert_eq!(band_render_width(1920, &layout, 8192), 1920);
        layout.band.scale_x = 1.0;
        layout.band.scale_y = 2.5;
        assert_eq!(band_supersample_factor(&layout), 2.5);
        assert_eq!(band_render_width(1920, &layout, 8192), 4800);
        layout.band.scale_x = 4.0;
        assert_eq!(band_supersample_factor(&layout), 4.0);
        assert_eq!(band_render_width(1920, &layout, 8192), 7680);
        // Capped, and even.
        assert_eq!(band_render_width(3840, &layout, 8192), 8192);
        layout.band.scale_x = 1.333;
        layout.band.scale_y = 1.0;
        let width = band_render_width(1279, &layout, 8192);
        assert_eq!(width % 2, 0);
        assert!(width as f32 >= 1279.0 * 1.333);
        // The rendered band always covers the composed band rect.
        let composed = compose(1920, 1080, 180, 1920, 1080, &layout);
        assert!(band_render_width(1920, &layout, 8192) >= composed.band.width);
    }

    #[test]
    fn default_layout_stacks_video_above_full_width_band() {
        let composed = compose(1920, 1080, 180, 1920, 1080, &ExportLayout::default());
        assert_eq!(
            composed.band,
            PixelRect {
                x: 0,
                y: 900,
                width: 1920,
                height: 180
            }
        );
        // 16:9 video fitted into 1920×900: 1600×900, centred.
        assert_eq!(
            composed.video,
            PixelRect {
                x: 160,
                y: 0,
                width: 1600,
                height: 900
            }
        );
    }

    #[test]
    fn transforms_move_and_stretch_around_the_centre() {
        let layout = ExportLayout {
            video: LayerTransform::default(),
            band: LayerTransform {
                offset_x: 0.0,
                offset_y: -0.5,
                scale_x: 0.5,
                scale_y: 2.0,
            },
        };
        let composed = compose(1920, 1080, 180, 1920, 1080, &layout);
        assert_eq!(composed.band.width, 960);
        assert_eq!(composed.band.height, 360);
        assert_eq!(composed.band.x, 480);
        assert_eq!(composed.band.y, 900 - 90 - 540);
    }

    #[test]
    fn sizes_stay_even_and_values_are_clamped() {
        let layout = ExportLayout {
            video: LayerTransform {
                offset_x: f32::NAN,
                offset_y: 9.0,
                scale_x: 0.333,
                scale_y: 100.0,
            },
            band: LayerTransform::default(),
        };
        let composed = compose(1280, 720, 121, 1000, 1000, &layout);
        assert_eq!(composed.video.width % 2, 0);
        assert_eq!(composed.video.height % 2, 0);
        assert!(layout.normalized().video.scale_y <= MAX_SCALE);
        assert_eq!(layout.normalized().video.offset_x, 0.0);
    }

    #[test]
    fn old_configurations_load_with_the_default_layout() {
        let layout: ExportLayout = serde_json::from_str("{}").unwrap();
        assert!(layout.is_default());
        let layout: ExportLayout = serde_json::from_str("{\"band\":{\"scale_y\":1.5}}").unwrap();
        assert_eq!(layout.band.scale_y, 1.5);
        assert_eq!(layout.band.scale_x, 1.0);
    }
}
