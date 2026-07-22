//! Text-emotion adapter around the established vector text rasterizer.
//!
//! The editor, GPU exporter and CPU fallback already converge on
//! `crate::vector_text`. Render-only metadata is therefore attached to scene
//! text and decoded here, while the persisted/dialogue text remains untouched.

#[path = "vector_text.rs"]
mod base;

pub use base::*;

use crate::text_emotion::{self, TextEmotion};
use glyphon::FontSystem;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default)]
struct Motion {
    dx: f32,
    dy: f32,
    rotation: f32,
    scale_x: f32,
    skew_x: f32,
}

pub fn render_rythmo_text(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_with_emotions(font_system, text, font_size, dest_w, dest_h, RenderMode::Stretched)
}

pub fn render_rythmo_text_natural(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_with_emotions(font_system, text, font_size, dest_w, dest_h, RenderMode::Natural)
}

pub fn render_rythmo_text_natural_emphasized(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_with_emotions(font_system, text, font_size, dest_w, dest_h, RenderMode::Emphasized)
}

pub fn render_rythmo_text_with_ratios(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_with_emotions(font_system, text, font_size, dest_w, dest_h, RenderMode::StretchedRatios)
}

pub fn render_rythmo_text_tile(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
) -> Option<VectorTextPixmap> {
    render_tile(
        font_system,
        text,
        font_size,
        full_w,
        dest_h,
        tile_x,
        tile_w,
        RenderMode::Stretched,
    )
}

pub fn render_rythmo_text_tile_natural(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
) -> Option<VectorTextPixmap> {
    render_tile(
        font_system,
        text,
        font_size,
        full_w,
        dest_h,
        tile_x,
        tile_w,
        RenderMode::Natural,
    )
}

pub fn measure_rythmo_text_width(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
) -> Option<f32> {
    base::measure_rythmo_text_width(font_system, text_emotion::plain_render_text(text), font_size)
}

pub fn measure_rythmo_text_width_standalone(text: &str, font_size: f32) -> Option<f32> {
    base::measure_rythmo_text_width_standalone(text_emotion::plain_render_text(text), font_size)
}

pub fn measure_rythmo_text_char_ratios_standalone(
    text: &str,
    font_size: f32,
) -> Option<Vec<f32>> {
    base::measure_rythmo_text_char_ratios_standalone(
        text_emotion::plain_render_text(text),
        font_size,
    )
}

#[derive(Clone, Copy)]
enum RenderMode {
    Stretched,
    StretchedRatios,
    Natural,
    Emphasized,
}

fn base_render(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    mode: RenderMode,
) -> Option<VectorTextPixmap> {
    match mode {
        RenderMode::Stretched => {
            base::render_rythmo_text(font_system, text, font_size, dest_w, dest_h)
        }
        RenderMode::StretchedRatios => base::render_rythmo_text_with_ratios(
            font_system,
            text,
            font_size,
            dest_w,
            dest_h,
        ),
        RenderMode::Natural => {
            base::render_rythmo_text_natural(font_system, text, font_size, dest_w, dest_h)
        }
        RenderMode::Emphasized => base::render_rythmo_text_natural_emphasized(
            font_system,
            text,
            font_size,
            dest_w,
            dest_h,
        ),
    }
}

fn render_with_emotions(
    font_system: &mut FontSystem,
    encoded_text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    mode: RenderMode,
) -> Option<VectorTextPixmap> {
    let Some((line_id, phase, text)) = text_emotion::decode_render_text(encoded_text) else {
        return base_render(font_system, encoded_text, font_size, dest_w, dest_h, mode);
    };
    let spans = text_emotion::spans_for_line(line_id, text);
    if spans.is_empty() {
        return base_render(font_system, text, font_size, dest_w, dest_h, mode);
    }

    let mut canvas = base_render(font_system, text, font_size, dest_w, dest_h, mode)?;
    if canvas.width == 0 || canvas.height == 0 {
        return Some(canvas);
    }

    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if graphemes.is_empty() {
        return Some(canvas);
    }
    let ratios = usable_char_ratios(&canvas, text, font_size);
    let grapheme_char_boundaries = grapheme_char_boundaries(text);
    let time = phase as f32 / 60.0;

    for (grapheme_index, grapheme) in graphemes.iter().enumerate() {
        let Some(emotion) = spans
            .iter()
            .find(|span| span.contains(grapheme_index))
            .map(|span| span.emotion)
        else {
            continue;
        };
        let start_char = grapheme_char_boundaries[grapheme_index];
        let end_char = grapheme_char_boundaries[grapheme_index + 1];
        let x0 = ratio_at(&ratios, start_char) * dest_w as f32;
        let x1 = ratio_at(&ratios, end_char) * dest_w as f32;
        let advance = (x1 - x0).max(1.0);
        clear_grapheme_cell(&mut canvas, x0, x1, font_size);

        let padding = (font_size * 0.55).ceil().max(4.0) as u32;
        let glyph_w = advance.ceil() as u32 + padding * 2;
        let glyph = base::render_rythmo_text_natural(
            font_system,
            grapheme,
            font_size,
            glyph_w.max(1),
            dest_h,
        );
        if let Some(glyph) = glyph {
            let motion = motion_for(emotion, grapheme_index, graphemes.len(), time, font_size);
            composite_transformed(
                &mut canvas,
                &glyph,
                x0 - padding as f32,
                0.0,
                advance + padding as f32 * 2.0,
                emotion,
                motion,
                time,
                grapheme_index,
            );
        }
    }

    for span in &spans {
        draw_readability_copy(
            font_system,
            &mut canvas,
            text,
            &graphemes,
            &grapheme_char_boundaries,
            &ratios,
            span.start_grapheme,
            span.end_grapheme,
            font_size,
        );
    }

    Some(canvas)
}

fn render_tile(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
    mode: RenderMode,
) -> Option<VectorTextPixmap> {
    if text_emotion::decode_render_text(text).is_none() {
        return match mode {
            RenderMode::Natural => base::render_rythmo_text_tile_natural(
                font_system, text, font_size, full_w, dest_h, tile_x, tile_w,
            ),
            _ => base::render_rythmo_text_tile(
                font_system, text, font_size, full_w, dest_h, tile_x, tile_w,
            ),
        };
    }
    if tile_x >= full_w || tile_w == 0 {
        return None;
    }
    let full = render_with_emotions(font_system, text, font_size, full_w, dest_h, mode)?;
    let width = tile_w.min(full_w - tile_x);
    let mut pixels = vec![0; width as usize * dest_h as usize * 4];
    for y in 0..dest_h as usize {
        let source = (y * full_w as usize + tile_x as usize) * 4;
        let target = y * width as usize * 4;
        let len = width as usize * 4;
        pixels[target..target + len].copy_from_slice(&full.pixels[source..source + len]);
    }
    Some(VectorTextPixmap {
        pixels,
        width,
        height: dest_h,
        char_x_ratios: Vec::new(),
    })
}

fn usable_char_ratios(canvas: &VectorTextPixmap, text: &str, font_size: f32) -> Vec<f32> {
    if canvas.char_x_ratios.len() == text.chars().count() + 1 {
        return canvas.char_x_ratios.clone();
    }
    base::measure_rythmo_text_char_ratios_standalone(text, font_size)
        .filter(|ratios| ratios.len() == text.chars().count() + 1)
        .unwrap_or_else(|| {
            let count = text.chars().count().max(1);
            (0..=count).map(|index| index as f32 / count as f32).collect()
        })
}

fn grapheme_char_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::new();
    for (byte, _) in text.grapheme_indices(true) {
        boundaries.push(text[..byte].chars().count());
    }
    boundaries.push(text.chars().count());
    boundaries
}

fn ratio_at(ratios: &[f32], index: usize) -> f32 {
    ratios.get(index).copied().unwrap_or_else(|| {
        if ratios.len() <= 1 {
            0.0
        } else {
            index.min(ratios.len() - 1) as f32 / (ratios.len() - 1) as f32
        }
    })
}

fn clear_grapheme_cell(canvas: &mut VectorTextPixmap, x0: f32, x1: f32, font_size: f32) {
    let margin = (font_size * 0.65).ceil() as i32;
    let left = x0.floor() as i32 - margin;
    let right = x1.ceil() as i32 + margin;
    let upper_h = (canvas.height as f32 * 0.78).ceil() as i32;
    for y in 0..upper_h.min(canvas.height as i32) {
        for x in left.max(0)..right.min(canvas.width as i32) {
            let index = ((y as u32 * canvas.width + x as u32) * 4) as usize;
            canvas.pixels[index..index + 4].fill(0);
        }
    }
}

fn motion_for(
    emotion: TextEmotion,
    index: usize,
    count: usize,
    time: f32,
    font_size: f32,
) -> Motion {
    let phase = time * std::f32::consts::TAU + index as f32 * 0.46;
    match emotion {
        TextEmotion::Pendulum => Motion {
            rotation: phase.sin() * 0.22,
            scale_x: 1.0,
            ..Motion::default()
        },
        TextEmotion::Swing => Motion {
            rotation: phase.sin() * 0.18,
            scale_x: 0.78 + 0.22 * phase.cos().abs(),
            dx: phase.sin() * font_size * 0.07,
            ..Motion::default()
        },
        TextEmotion::Yay => Motion {
            scale_x: 1.0,
            ..Motion::default()
        },
        TextEmotion::Bounce => {
            let chain = ((time * 2.1 - index as f32 * 0.12).rem_euclid(1.35) / 0.35)
                .clamp(0.0, 1.0);
            Motion {
                dy: -(chain * std::f32::consts::PI).sin() * font_size * 0.34,
                scale_x: 1.0,
                ..Motion::default()
            }
        }
        TextEmotion::Slide => Motion {
            scale_x: 1.0,
            skew_x: phase.sin() * 0.30,
            ..Motion::default()
        },
        TextEmotion::Oscillation => Motion {
            rotation: phase.sin() * 0.17,
            scale_x: 1.0,
            ..Motion::default()
        },
        TextEmotion::Wave => Motion {
            dy: phase.sin() * font_size * 0.24,
            scale_x: 1.0,
            ..Motion::default()
        },
        TextEmotion::Shake => Motion {
            dx: noise(index, time, 17.0) * font_size * 0.10,
            dy: noise(index + count, time, 23.0) * font_size * 0.09,
            rotation: noise(index + 91, time, 19.0) * 0.045,
            scale_x: 1.0,
            ..Motion::default()
        },
        TextEmotion::Wiggle => Motion {
            dx: phase.sin() * font_size * 0.10,
            dy: (phase * 0.77).cos() * font_size * 0.055,
            rotation: phase.sin() * 0.035,
            scale_x: 1.0,
            ..Motion::default()
        },
    }
}

fn noise(seed: usize, time: f32, speed: f32) -> f32 {
    let value = (seed as f32 * 12.9898 + (time * speed).floor() * 78.233).sin() * 43_758.547;
    value.fract() * 2.0 - 1.0
}

#[allow(clippy::too_many_arguments)]
fn composite_transformed(
    target: &mut VectorTextPixmap,
    source: &VectorTextPixmap,
    origin_x: f32,
    origin_y: f32,
    cell_width: f32,
    emotion: TextEmotion,
    motion: Motion,
    time: f32,
    grapheme_index: usize,
) {
    let source_w = source.width as f32;
    let source_h = source.height as f32;
    let pivot_y = if matches!(emotion, TextEmotion::Oscillation) {
        source_h * 0.50
    } else {
        source_h * 0.12
    };
    let pivot_x = source_w * 0.50;
    let cosine = motion.rotation.cos();
    let sine = motion.rotation.sin();

    for sy in 0..source.height {
        for sx in 0..source.width {
            let source_index = ((sy * source.width + sx) * 4) as usize;
            let alpha = source.pixels[source_index + 3];
            if alpha == 0 {
                continue;
            }
            let mut x = sx as f32 - pivot_x;
            let y = sy as f32 - pivot_y;
            x += y * motion.skew_x;
            x *= motion.scale_x.max(0.05);
            let rotated_x = x * cosine - y * sine;
            let rotated_y = x * sine + y * cosine;
            let dx = origin_x + pivot_x + rotated_x + motion.dx;
            let dy = origin_y + pivot_y + rotated_y + motion.dy;
            let mut rgba = [
                source.pixels[source_index],
                source.pixels[source_index + 1],
                source.pixels[source_index + 2],
                alpha,
            ];
            if emotion == TextEmotion::Yay {
                let hue = (time * 0.28
                    + grapheme_index as f32 * 0.10
                    + sx as f32 / cell_width.max(1.0) * 0.18)
                    .fract();
                let rainbow = hsv_rgb(hue, 0.86, 1.0);
                rgba[0] = ((rgba[0] as u16 * rainbow[0] as u16) / 255) as u8;
                rgba[1] = ((rgba[1] as u16 * rainbow[1] as u16) / 255) as u8;
                rgba[2] = ((rgba[2] as u16 * rainbow[2] as u16) / 255) as u8;
            }
            blend_pixel(target, dx.round() as i32, dy.round() as i32, rgba);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_readability_copy(
    font_system: &mut FontSystem,
    target: &mut VectorTextPixmap,
    text: &str,
    graphemes: &[&str],
    grapheme_char_boundaries: &[usize],
    ratios: &[f32],
    start: usize,
    end: usize,
    font_size: f32,
) {
    if start >= end || end > graphemes.len() {
        return;
    }
    let substring = graphemes[start..end].concat();
    let start_char = grapheme_char_boundaries[start];
    let end_char = grapheme_char_boundaries[end];
    let x0 = ratio_at(ratios, start_char) * target.width as f32;
    let x1 = ratio_at(ratios, end_char) * target.width as f32;
    let width = (x1 - x0).ceil().max(1.0) as u32;
    let small_size = (font_size * 0.48).max(6.0);
    let fallback_h = (target.height as f32 * 0.31).ceil().max(1.0) as u32;
    let Some(copy) = base::render_rythmo_text_natural(
        font_system,
        &substring,
        small_size,
        width,
        fallback_h,
    ) else {
        return;
    };
    let y = target.height.saturating_sub(fallback_h) as i32;
    for sy in 0..copy.height {
        for sx in 0..copy.width {
            let index = ((sy * copy.width + sx) * 4) as usize;
            let alpha = ((copy.pixels[index + 3] as f32) * 0.78).round() as u8;
            if alpha == 0 {
                continue;
            }
            blend_pixel(
                target,
                x0.round() as i32 + sx as i32,
                y + sy as i32,
                [copy.pixels[index], copy.pixels[index + 1], copy.pixels[index + 2], alpha],
            );
        }
    }
    let _ = text;
}

fn blend_pixel(target: &mut VectorTextPixmap, x: i32, y: i32, source: [u8; 4]) {
    if x < 0 || y < 0 || x >= target.width as i32 || y >= target.height as i32 {
        return;
    }
    let index = ((y as u32 * target.width + x as u32) * 4) as usize;
    let alpha = source[3] as u32;
    let inverse = 255 - alpha;
    target.pixels[index] = ((source[0] as u32 * alpha + target.pixels[index] as u32 * inverse) / 255) as u8;
    target.pixels[index + 1] = ((source[1] as u32 * alpha + target.pixels[index + 1] as u32 * inverse) / 255) as u8;
    target.pixels[index + 2] = ((source[2] as u32 * alpha + target.pixels[index + 2] as u32 * inverse) / 255) as u8;
    target.pixels[index + 3] = (alpha + target.pixels[index + 3] as u32 * inverse / 255).min(255) as u8;
}

fn hsv_rgb(hue: f32, saturation: f32, value: f32) -> [u8; 3] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_produces_finite_motion() {
        for effect in TextEmotion::ALL {
            let motion = motion_for(effect, 2, 8, 1.25, 28.0);
            assert!(motion.dx.is_finite());
            assert!(motion.dy.is_finite());
            assert!(motion.rotation.is_finite());
            assert!(motion.scale_x.is_finite());
            assert!(motion.skew_x.is_finite());
        }
    }
}
