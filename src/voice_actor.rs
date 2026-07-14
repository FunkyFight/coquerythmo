use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{ImageEncoder, ImageReader, Limits};
use serde::{Deserialize, Serialize};

pub const VOICE_ACTOR_ICON_SIZE: u32 = 256;
pub const MAX_ICON_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EMBEDDED_ICON_BYTES: usize = 2 * 1024 * 1024;
const MAX_EMBEDDED_ICON_BASE64_LEN: usize = MAX_EMBEDDED_ICON_BYTES.div_ceil(3) * 4;
const MAX_ICON_SOURCE_DIMENSION: u32 = 4096;
const MAX_ICON_DECODE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VoiceActor {
    pub name: String,
    #[serde(default)]
    pub icon_path: String,
    #[serde(default)]
    pub icon_png_base64: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LineVoiceActorsChange {
    pub line_id: u64,
    pub old_voice_actor_names: Vec<String>,
    pub new_voice_actor_names: Vec<String>,
}

pub fn load_icon_png_base64(path: &Path) -> Result<String, String> {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() > MAX_ICON_SOURCE_BYTES as u64 {
            return Err(format!(
                "Icône trop volumineuse: {} octets maximum",
                MAX_ICON_SOURCE_BYTES
            ));
        }
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Lecture de l'icône impossible: {e}"))?;
    if bytes.len() > MAX_ICON_SOURCE_BYTES {
        return Err(format!(
            "Icône trop volumineuse: {} octets maximum",
            MAX_ICON_SOURCE_BYTES
        ));
    }
    let rgba = if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        rasterize_svg_icon(&bytes)?
    } else {
        let image = decode_image_with_limits(&bytes)?
            .resize_exact(
                VOICE_ACTOR_ICON_SIZE,
                VOICE_ACTOR_ICON_SIZE,
                image::imageops::FilterType::Lanczos3,
            )
            .to_rgba8();
        image.into_raw()
    };

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &rgba,
            VOICE_ACTOR_ICON_SIZE,
            VOICE_ACTOR_ICON_SIZE,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Encodage PNG impossible: {e}"))?;

    if png.len() > MAX_EMBEDDED_ICON_BYTES {
        return Err(format!(
            "Icône encodée trop volumineuse: {} octets maximum",
            MAX_EMBEDDED_ICON_BYTES
        ));
    }

    Ok(STANDARD.encode(png))
}

pub fn decode_icon_rgba(icon_png_base64: &str) -> Result<Vec<u8>, String> {
    if icon_png_base64.len() > MAX_EMBEDDED_ICON_BASE64_LEN {
        return Err(format!(
            "Icône intégrée trop volumineuse: {} caractères maximum",
            MAX_EMBEDDED_ICON_BASE64_LEN
        ));
    }
    let bytes = STANDARD
        .decode(icon_png_base64)
        .map_err(|e| format!("Décodage base64 impossible: {e}"))?;
    if bytes.len() > MAX_EMBEDDED_ICON_BYTES {
        return Err(format!(
            "Icône intégrée trop volumineuse: {} octets maximum",
            MAX_EMBEDDED_ICON_BYTES
        ));
    }
    let image = decode_image_with_limits(&bytes)?
        .resize_exact(
            VOICE_ACTOR_ICON_SIZE,
            VOICE_ACTOR_ICON_SIZE,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    Ok(image.into_raw())
}

pub fn icon_hash(icon_png_base64: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    icon_png_base64.hash(&mut hasher);
    hasher.finish()
}

fn decode_image_with_limits(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Format d'icône non supporté: {e}"))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_ICON_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_ICON_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_ICON_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Décodage de l'icône impossible: {e}"))
}

fn rasterize_svg_icon(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .map_err(|e| format!("SVG invalide: {e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(VOICE_ACTOR_ICON_SIZE, VOICE_ACTOR_ICON_SIZE)
        .ok_or_else(|| "Allocation de l'icône impossible".to_string())?;

    let svg_size = tree.size();
    let sx = VOICE_ACTOR_ICON_SIZE as f32 / svg_size.width();
    let sy = VOICE_ACTOR_ICON_SIZE as f32 / svg_size.height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    Ok(pixmap.data().to_vec())
}
