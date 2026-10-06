//! Karaoke dot textures shared by the editor preview and both video export
//! renderers.
//!
//! The classic circle keeps its procedural drawing in every renderer. The
//! other built-in shapes are white alpha masks rasterized from small SVGs
//! and tinted with the line's character colour, exactly like the circle. A
//! custom image is drawn with its own colours, never tinted.
//!
//! Every textured dot is drawn as the same three layers in all renderers
//! (see [`layers`]): a dark shadow, a light rim (built-in shapes only) and
//! the dot itself, all centred on the dot rectangle computed by the
//! unchanged bounce/position code.
//!
//! A custom image pair shows its ground image while the dot is near the
//! bottom of its bounce and its jump image while it is in the air (see
//! [`face_for_dot`]); every renderer picks the face the same way.

use std::hash::{Hash, Hasher};

use resvg::tiny_skia;

use crate::band_style::{decode_custom_dot_png, KaraokeDot};

/// Side of the largest mip level uploaded to the GPU.
pub const TEXTURE_BASE_SIZE: u32 = 64;

const SVG_VIEWBOX: f32 = 64.0;

fn builtin_svg(dot: &KaraokeDot) -> Option<&'static str> {
    Some(match dot {
        KaraokeDot::Circle => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="31" fill="#fff"/></svg>"##
        }
        KaraokeDot::Ring => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="25" fill="none" stroke="#fff" stroke-width="12"/></svg>"##
        }
        KaraokeDot::Star => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon fill="#fff" stroke="#fff" stroke-width="2" stroke-linejoin="round" points="32,3 39.3,23.9 61.5,24.4 43.9,37.9 50.2,59.1 32,46.5 13.8,59.1 20.1,37.9 2.5,24.4 24.7,23.9"/></svg>"##
        }
        KaraokeDot::Heart => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path fill="#fff" d="M32 60 C 12 45 2 33 2 20 C 2 10 9 3 18 3 C 25 3 30 7 32 13 C 34 7 39 3 46 3 C 55 3 62 10 62 20 C 62 33 52 45 32 60 Z"/></svg>"##
        }
        KaraokeDot::Diamond => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon fill="#fff" points="32,1 60,32 32,63 4,32"/></svg>"##
        }
        KaraokeDot::Note => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><ellipse cx="22" cy="49" rx="14" ry="10.5" transform="rotate(-22 22 49)" fill="#fff"/><rect x="29" y="4" width="7" height="46" fill="#fff"/><path fill="#fff" d="M29 4 C 36 14 56 18 56 36 C 56 42 54 46 52 49 C 53 36 44 28 36 26 L 36 4 Z"/></svg>"##
        }
        KaraokeDot::Custom { .. } => return None,
    })
}

/// Which image of a dot is drawn: only custom image pairs differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DotFace {
    Ground,
    Jump,
}

impl DotFace {
    pub const ALL: [DotFace; 2] = [DotFace::Ground, DotFace::Jump];
}

/// Normalized bounce height (0 on the ground, 1 at the top) above which a
/// dot shows its jump image.
pub const JUMP_BOUNCE_THRESHOLD: f32 = 0.15;

pub fn face_for_bounce(bounce: f32) -> DotFace {
    if bounce > JUMP_BOUNCE_THRESHOLD {
        DotFace::Jump
    } else {
        DotFace::Ground
    }
}

/// Face of a dot whose top edge is `dot_top`, on a line whose top edge is
/// `line_top`. All renderers place the dot at
/// `line_top + 3 * scale - bounce * size * KARAOKE_DOT_BOUNCE_AMPLITUDE`,
/// so the bounce is read back from the position.
pub fn face_for_dot(line_top: f32, dot_top: f32, size: f32, scale: f32) -> DotFace {
    let travel = size * crate::constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
    if !(travel > 0.0) {
        return DotFace::Ground;
    }
    face_for_bounce((line_top + 3.0 * scale.max(0.5) - dot_top) / travel)
}

/// Cache key of the dot's texture for `face`, `None` for the procedural
/// circle. Dots without a jump image share one key for both faces.
pub fn texture_key(dot: &KaraokeDot, face: DotFace) -> Option<u64> {
    if dot.is_circle() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "karaoke-dot".hash(&mut hasher);
    dot.hash(&mut hasher);
    if face == DotFace::Jump && dot.is_image_pair() {
        "jump".hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// Built-in shapes are tinted with the line colour; custom images are not.
pub fn is_tinted(dot: &KaraokeDot) -> bool {
    !dot.is_custom()
}

/// Rasterizes the dot's `face` as a `size` x `size` straight-alpha RGBA
/// image.
pub fn rasterize(dot: &KaraokeDot, face: DotFace, size: u32) -> Option<Vec<u8>> {
    let size = size.max(1);
    if let Some(png_base64) = dot.image(face == DotFace::Jump) {
        let image = decode_custom_dot_png(png_base64)?;
        return Some(resize_rgba(&image, size));
    }
    let svg = builtin_svg(dot)?;
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / SVG_VIEWBOX;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // White shapes: the premultiplied colour is the alpha, so the straight
    // colour is always white.
    Some(
        pixmap
            .data()
            .chunks_exact(4)
            .flat_map(|pixel| [255, 255, 255, pixel[3]])
            .collect(),
    )
}

fn resize_rgba(image: &image::RgbaImage, size: u32) -> Vec<u8> {
    if image.width() == size && image.height() == size {
        return image.as_raw().clone();
    }
    image::imageops::resize(image, size, size, image::imageops::FilterType::Triangle).into_raw()
}

/// Full mip chain, from `base` pixels down to 1, for GPU textures sampled
/// far below their size.
pub fn mip_chain(dot: &KaraokeDot, face: DotFace, base: u32) -> Option<Vec<(u32, Vec<u8>)>> {
    let base = base.max(1);
    let custom = match dot.image(face == DotFace::Jump) {
        Some(png_base64) => Some(decode_custom_dot_png(png_base64)?),
        None => None,
    };
    let mut levels = Vec::new();
    let mut size = base;
    loop {
        let level = match &custom {
            Some(image) => resize_rgba(image, size),
            None => rasterize(dot, face, size)?,
        };
        levels.push((size, level));
        if size == 1 {
            break;
        }
        size = (size / 2).max(1);
    }
    Some(levels)
}

/// One textured draw of the dot: a square grown by `expand` pixels on each
/// side of the dot rectangle, multiplied by `tint`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DotLayer {
    pub expand: f32,
    pub tint: [f32; 4],
}

pub const SHADOW_TINT: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
pub const RIM_TINT: [f32; 4] = [1.0, 1.0, 1.0, 0.85];

/// Layers to draw, back to front, for a textured dot of colour `color` at
/// export `scale` (1.0 in the editor).
pub fn layers(dot: &KaraokeDot, color: [f32; 4], scale: f32) -> Vec<DotLayer> {
    let unit = scale.max(0.5);
    let mut layers = vec![DotLayer {
        expand: 1.5 * unit,
        tint: SHADOW_TINT,
    }];
    if is_tinted(dot) {
        layers.push(DotLayer {
            expand: unit,
            tint: RIM_TINT,
        });
        layers.push(DotLayer {
            expand: 0.0,
            tint: [
                color[0].clamp(0.0, 1.0),
                color[1].clamp(0.0, 1.0),
                color[2].clamp(0.0, 1.0),
                color[3].clamp(0.0, 1.0),
            ],
        });
    } else {
        layers.push(DotLayer {
            expand: 0.0,
            tint: [1.0; 4],
        });
    }
    layers
}

/// CPU sprites of textured dots, rasterized at the pixel size they are
/// drawn at (the export size never changes during an export).
#[derive(Default)]
pub struct CpuDotSprites {
    sprites: std::collections::HashMap<(u64, u32), Option<Vec<u8>>>,
}

impl CpuDotSprites {
    pub fn new() -> Self {
        Self::default()
    }

    fn sprite(&mut self, dot: &KaraokeDot, face: DotFace, key: u64, size: u32) -> Option<&[u8]> {
        if self.sprites.len() > 64 {
            self.sprites.clear();
        }
        self.sprites
            .entry((key, size))
            .or_insert_with(|| rasterize(dot, face, size))
            .as_deref()
    }

    /// Draws the textured dot whose rectangle is `x`, `y`, `size` x `size`.
    /// Does nothing for the procedural circle.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        dot: &KaraokeDot,
        face: DotFace,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        let Some(key) = texture_key(dot, face) else {
            return;
        };
        if !(x.is_finite() && y.is_finite() && size.is_finite() && size > 0.0) {
            return;
        }
        for layer in layers(dot, color, scale) {
            let draw_size = size + layer.expand * 2.0;
            let pixels = draw_size.ceil().clamp(1.0, 1024.0) as u32;
            let Some(sprite) = self.sprite(dot, face, key, pixels) else {
                return;
            };
            let Some(tinted) = tinted_pixmap(sprite, pixels, layer.tint) else {
                continue;
            };
            let factor = draw_size / pixels as f32;
            pixmap.draw_pixmap(
                0,
                0,
                tinted.as_ref(),
                &tiny_skia::PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..tiny_skia::PixmapPaint::default()
                },
                tiny_skia::Transform::from_row(
                    factor,
                    0.0,
                    0.0,
                    factor,
                    x - layer.expand,
                    y - layer.expand,
                ),
                None,
            );
        }
    }
}

/// Multiplies a straight-alpha sprite by `tint` into a premultiplied pixmap.
fn tinted_pixmap(sprite: &[u8], size: u32, tint: [f32; 4]) -> Option<tiny_skia::Pixmap> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    for (dst, src) in pixmap
        .data_mut()
        .chunks_exact_mut(4)
        .zip(sprite.chunks_exact(4))
    {
        let alpha = src[3] as f32 / 255.0 * tint[3];
        for channel in 0..3 {
            let straight = src[channel] as f32 / 255.0 * tint[channel];
            dst[channel] = (straight * alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        dst[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count()
    }

    #[test]
    fn builtin_shapes_rasterize_to_white_masks() {
        for dot in KaraokeDot::BUILTINS {
            let rgba = rasterize(&dot, DotFace::Ground, 32).expect("built-in shapes rasterize");
            assert_eq!(rgba.len(), 32 * 32 * 4);
            let covered = coverage(&rgba);
            assert!(covered > 32 * 4, "{dot:?} is almost empty");
            assert!(covered < 32 * 32, "{dot:?} fills the whole square");
            assert!(rgba
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [255, 255, 255]));
        }
    }

    #[test]
    fn shapes_differ_from_each_other() {
        let masks: Vec<_> = KaraokeDot::BUILTINS
            .iter()
            .map(|dot| rasterize(dot, DotFace::Ground, 24).unwrap())
            .collect();
        for (i, a) in masks.iter().enumerate() {
            for b in &masks[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn circle_keeps_the_procedural_path() {
        assert_eq!(texture_key(&KaraokeDot::Circle, DotFace::Ground), None);
        assert_eq!(texture_key(&KaraokeDot::Circle, DotFace::Jump), None);
        assert!(texture_key(&KaraokeDot::Star, DotFace::Ground).is_some());
        assert_ne!(
            texture_key(&KaraokeDot::Star, DotFace::Ground),
            texture_key(&KaraokeDot::Heart, DotFace::Ground)
        );
        // Shapes look the same in the air.
        assert_eq!(
            texture_key(&KaraokeDot::Star, DotFace::Ground),
            texture_key(&KaraokeDot::Star, DotFace::Jump)
        );
        // Drawing a circle through the sprite path is a no-op: the renderers
        // keep their original circle code.
        let mut pixmap = tiny_skia::Pixmap::new(16, 16).unwrap();
        CpuDotSprites::new().draw(
            &mut pixmap,
            &KaraokeDot::Circle,
            DotFace::Ground,
            4.0,
            4.0,
            7.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0,
        );
        assert!(pixmap.data().iter().all(|byte| *byte == 0));
    }

    fn solid_png(rgba: [u8; 4]) -> String {
        let image = image::RgbaImage::from_pixel(8, 8, image::Rgba(rgba));
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            image.as_raw(),
            8,
            8,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(png)
    }

    #[test]
    fn custom_images_keep_their_colours_and_mips_reach_one_pixel() {
        let dot = KaraokeDot::custom(solid_png([10, 200, 30, 255]));
        assert!(!is_tinted(&dot));
        let levels = mip_chain(&dot, DotFace::Ground, 16).unwrap();
        assert_eq!(
            levels.iter().map(|(size, _)| *size).collect::<Vec<_>>(),
            vec![16, 8, 4, 2, 1]
        );
        assert_eq!(&levels[0].1[..4], &[10, 200, 30, 255]);
        let tints: Vec<_> = layers(&dot, [1.0, 0.0, 0.0, 1.0], 1.0)
            .iter()
            .map(|layer| layer.tint)
            .collect();
        assert_eq!(tints, vec![SHADOW_TINT, [1.0; 4]]);
    }

    #[test]
    fn image_pairs_show_the_jump_image_in_the_air() {
        let pair = KaraokeDot::Custom {
            png_base64: solid_png([255, 0, 0, 255]),
            jump_png_base64: Some(solid_png([0, 0, 255, 255])),
        };
        // The bounce height picks the face.
        assert_eq!(face_for_bounce(0.0), DotFace::Ground);
        assert_eq!(face_for_bounce(JUMP_BOUNCE_THRESHOLD), DotFace::Ground);
        assert_eq!(face_for_bounce(0.5), DotFace::Jump);
        assert_eq!(face_for_bounce(1.0), DotFace::Jump);
        // The face is read back from the dot position used by renderers.
        let size = crate::constants::KARAOKE_DOT_SIZE;
        let travel = size * crate::constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
        assert_eq!(face_for_dot(100.0, 103.0, size, 1.0), DotFace::Ground);
        assert_eq!(face_for_dot(100.0, 103.0 - travel, size, 1.0), DotFace::Jump);
        assert_eq!(
            face_for_dot(100.0, 103.0 - travel * 0.1, size, 1.0),
            DotFace::Ground
        );
        let scale = 2.0;
        let size2 = size * scale;
        assert_eq!(
            face_for_dot(10.0, 10.0 + 6.0 - size2 * 0.9, size2, scale),
            DotFace::Jump
        );
        // Each face has its own texture and pixels.
        assert_ne!(
            texture_key(&pair, DotFace::Ground),
            texture_key(&pair, DotFace::Jump)
        );
        let ground = rasterize(&pair, DotFace::Ground, 4).unwrap();
        let jump = rasterize(&pair, DotFace::Jump, 4).unwrap();
        assert_eq!(&ground[..4], &[255, 0, 0, 255]);
        assert_eq!(&jump[..4], &[0, 0, 255, 255]);
        let jump_mips = mip_chain(&pair, DotFace::Jump, 8).unwrap();
        assert_eq!(&jump_mips[0].1[..4], &[0, 0, 255, 255]);
        // A single image is used for both faces.
        let single = KaraokeDot::custom(solid_png([255, 0, 0, 255]));
        assert_eq!(
            texture_key(&single, DotFace::Ground),
            texture_key(&single, DotFace::Jump)
        );
        assert_eq!(rasterize(&single, DotFace::Jump, 4).unwrap()[..4], [255, 0, 0, 255]);
        // The CPU sprites draw the requested face.
        let mut sprites = CpuDotSprites::new();
        let mut pixmap = tiny_skia::Pixmap::new(16, 16).unwrap();
        sprites.draw(&mut pixmap, &pair, DotFace::Jump, 4.0, 4.0, 8.0, [1.0; 4], 1.0);
        let centre = &pixmap.data()[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
        assert!(centre[2] > 200 && centre[0] < 30);
        let mut pixmap = tiny_skia::Pixmap::new(16, 16).unwrap();
        sprites.draw(&mut pixmap, &pair, DotFace::Ground, 4.0, 4.0, 8.0, [1.0; 4], 1.0);
        let centre = &pixmap.data()[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
        assert!(centre[0] > 200 && centre[2] < 30);
    }

    #[test]
    fn cpu_sprites_tint_builtin_shapes() {
        let mut pixmap = tiny_skia::Pixmap::new(32, 32).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);
        CpuDotSprites::new().draw(
            &mut pixmap,
            &KaraokeDot::Diamond,
            DotFace::Ground,
            8.0,
            8.0,
            16.0,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
        );
        let centre = &pixmap.data()[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4];
        assert!(centre[2] > 200 && centre[0] < 30 && centre[1] < 30);
    }
}
