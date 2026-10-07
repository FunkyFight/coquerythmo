//! Visual style of the rythmo band, shared by the editor preview and both
//! video export renderers so the preview matches the exported video.

use std::io::Cursor;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{ImageEncoder, ImageReader, Limits};
use serde::{Deserialize, Serialize};

/// Extension of exported style preset files.
pub const PRESET_EXTENSION: &str = "coqstyle";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BandStyle {
    /// Band background, behind the tracks.
    pub background: [f32; 4],
    /// Reading bar ("barre de lecture").
    pub playhead: [f32; 4],
    /// Reading bar width in pixels at the 1080p reference scale.
    pub playhead_width: f32,
    /// Texture of the bouncing karaoke dot. Omitted from files while it is
    /// the classic circle so older versions read the style unchanged.
    #[serde(skip_serializing_if = "KaraokeDot::is_circle")]
    pub karaoke_dot: KaraokeDot,
    /// Named dot choices that characters can share.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub karaoke_dot_groups: Vec<KaraokeDotGroup>,
    /// Dot of each character, by character name. Characters without an
    /// entry use [`BandStyle::karaoke_dot`].
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub character_dots: std::collections::BTreeMap<String, CharacterDot>,
}

/// A named karaoke dot that several characters can use.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KaraokeDotGroup {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub dot: KaraokeDot,
}

/// Karaoke dot chosen for one character.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterDot {
    /// Uses the dot of the group with this id (the default dot when the
    /// group no longer exists).
    Group(u32),
    /// A dot of its own.
    Dot(KaraokeDot),
}

/// Most dot groups a style keeps.
pub const MAX_DOT_GROUPS: usize = 32;
/// Longest dot group name, in characters.
pub const MAX_DOT_GROUP_NAME_LEN: usize = 40;

/// Look of the dot that bounces over karaoke lines.
///
/// Built-in shapes are white masks tinted with the line's character colour,
/// exactly like the classic circle. A custom image keeps its own colours.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KaraokeDot {
    /// The classic coloured disc, drawn procedurally as before textures.
    #[default]
    Circle,
    Ring,
    Star,
    Heart,
    Diamond,
    /// Music note.
    Note,
    /// User image: a square PNG of at most [`MAX_CUSTOM_DOT_SIZE`] pixels,
    /// base64 encoded so it travels in the project and in style presets.
    /// With `jump_png_base64`, `png_base64` is shown while the dot is on the
    /// ground and the jump image while it is in the air.
    Custom {
        png_base64: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        jump_png_base64: Option<String>,
    },
}

/// Largest side of a custom karaoke dot image, in pixels.
pub const MAX_CUSTOM_DOT_SIZE: u32 = 128;
const MAX_CUSTOM_DOT_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CUSTOM_DOT_SOURCE_DIMENSION: u32 = 4096;
const MAX_CUSTOM_DOT_DECODE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
/// A 128x128 RGBA PNG never needs more than this once base64 encoded.
const MAX_CUSTOM_DOT_BASE64_LEN: usize = 128 * 1024;
/// Untrusted images larger than this are not even decoded.
const MAX_UNTRUSTED_DOT_BASE64_LEN: usize = 4 * 1024 * 1024;

impl KaraokeDot {
    /// Built-in choices, in the order the style window shows them.
    pub const BUILTINS: [KaraokeDot; 6] = [
        KaraokeDot::Circle,
        KaraokeDot::Ring,
        KaraokeDot::Star,
        KaraokeDot::Heart,
        KaraokeDot::Diamond,
        KaraokeDot::Note,
    ];

    pub fn is_circle(&self) -> bool {
        matches!(self, KaraokeDot::Circle)
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, KaraokeDot::Custom { .. })
    }

    /// A custom image used for both phases of the bounce.
    pub fn custom(png_base64: String) -> Self {
        KaraokeDot::Custom {
            png_base64,
            jump_png_base64: None,
        }
    }

    /// Custom dot with a ground image and a separate jump image.
    pub fn is_image_pair(&self) -> bool {
        matches!(
            self,
            KaraokeDot::Custom {
                jump_png_base64: Some(_),
                ..
            }
        )
    }

    /// Image shown on the ground (`jump == false`) or in the air. Single
    /// images serve both phases; built-in shapes have none.
    pub fn image(&self, jump: bool) -> Option<&str> {
        match self {
            KaraokeDot::Custom {
                png_base64,
                jump_png_base64,
            } => Some(match (jump, jump_png_base64) {
                (true, Some(jump_png)) => jump_png.as_str(),
                _ => png_base64.as_str(),
            }),
            _ => None,
        }
    }

    /// i18n key of the choice's name.
    pub fn label_key(&self) -> &'static str {
        match self {
            KaraokeDot::Circle => "band_style.karaoke_dot.circle",
            KaraokeDot::Ring => "band_style.karaoke_dot.ring",
            KaraokeDot::Star => "band_style.karaoke_dot.star",
            KaraokeDot::Heart => "band_style.karaoke_dot.heart",
            KaraokeDot::Diamond => "band_style.karaoke_dot.diamond",
            KaraokeDot::Note => "band_style.karaoke_dot.note",
            KaraokeDot::Custom { .. } => "band_style.karaoke_dot.custom",
        }
    }

    /// Keeps a custom image only when it decodes, resizing it to the size
    /// cap when needed. Anything unusable falls back to the classic circle.
    pub fn normalized(self) -> Self {
        match self {
            KaraokeDot::Custom {
                png_base64,
                jump_png_base64,
            } => match normalize_custom_dot_png(&png_base64) {
                Some(png_base64) => KaraokeDot::Custom {
                    png_base64,
                    // An unusable jump image falls back to the single image.
                    jump_png_base64: jump_png_base64
                        .and_then(|jump| normalize_custom_dot_png(&jump)),
                },
                None => KaraokeDot::Circle,
            },
            other => other,
        }
    }
}

/// Decodes a custom dot image into straight RGBA pixels.
pub fn decode_custom_dot_png(png_base64: &str) -> Option<image::RgbaImage> {
    if png_base64.len() > MAX_CUSTOM_DOT_BASE64_LEN {
        return None;
    }
    let bytes = STANDARD.decode(png_base64.trim()).ok()?;
    decode_image_with_limits(&bytes).map(|image| image.to_rgba8())
}

fn decode_image_with_limits(bytes: &[u8]) -> Option<image::DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_CUSTOM_DOT_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_CUSTOM_DOT_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_CUSTOM_DOT_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    reader.decode().ok()
}

/// Fits an image in a transparent square of at most [`MAX_CUSTOM_DOT_SIZE`]
/// pixels, centred with its proportions kept, and encodes it as PNG.
fn encode_custom_dot(image: &image::DynamicImage) -> Option<String> {
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return None;
    }
    let side = width.max(height).min(MAX_CUSTOM_DOT_SIZE);
    let fitted = if width.max(height) > side {
        image.resize(side, side, image::imageops::FilterType::Lanczos3)
    } else {
        image.clone()
    }
    .to_rgba8();
    let mut square = image::RgbaImage::new(side, side);
    image::imageops::overlay(
        &mut square,
        &fitted,
        i64::from((side - fitted.width()) / 2),
        i64::from((side - fitted.height()) / 2),
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(square.as_raw(), side, side, image::ExtendedColorType::Rgba8)
        .ok()?;
    let encoded = STANDARD.encode(png);
    (encoded.len() <= MAX_CUSTOM_DOT_BASE64_LEN).then_some(encoded)
}

/// Returns the image unchanged when it already is a square PNG within the
/// size cap, a re-encoded copy when it only needs resizing, or `None`.
fn normalize_custom_dot_png(png_base64: &str) -> Option<String> {
    if png_base64.len() > MAX_UNTRUSTED_DOT_BASE64_LEN {
        return None;
    }
    let png_base64 = png_base64.trim();
    let bytes = STANDARD.decode(png_base64).ok()?;
    let is_png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let image = decode_image_with_limits(&bytes)?;
    if is_png
        && png_base64.len() <= MAX_CUSTOM_DOT_BASE64_LEN
        && image.width() > 0
        && image.width() == image.height()
        && image.width() <= MAX_CUSTOM_DOT_SIZE
    {
        return Some(png_base64.to_string());
    }
    encode_custom_dot(&image)
}

/// Reads an image file chosen by the user for the karaoke dot and returns
/// it as an embeddable square PNG.
pub fn load_custom_dot_image(path: &Path) -> Result<String, String> {
    let too_big = || format!("image larger than {MAX_CUSTOM_DOT_SOURCE_BYTES} bytes");
    if std::fs::metadata(path)
        .map(|metadata| metadata.len() > MAX_CUSTOM_DOT_SOURCE_BYTES)
        .unwrap_or(false)
    {
        return Err(too_big());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_CUSTOM_DOT_SOURCE_BYTES {
        return Err(too_big());
    }
    let image = decode_image_with_limits(&bytes).ok_or("unreadable image")?;
    encode_custom_dot(&image).ok_or_else(|| "image could not be encoded".to_string())
}

pub const DEFAULT_BACKGROUND: [f32; 4] = [0.02, 0.02, 0.03, 1.0];
pub const DEFAULT_PLAYHEAD: [f32; 4] = [1.0, 0.02, 0.05, 1.0];
pub const DEFAULT_PLAYHEAD_WIDTH: f32 = 3.0;
pub const MIN_PLAYHEAD_WIDTH: f32 = 1.0;
pub const MAX_PLAYHEAD_WIDTH: f32 = 12.0;

impl Default for BandStyle {
    fn default() -> Self {
        Self {
            background: DEFAULT_BACKGROUND,
            playhead: DEFAULT_PLAYHEAD,
            playhead_width: DEFAULT_PLAYHEAD_WIDTH,
            karaoke_dot: KaraokeDot::Circle,
            karaoke_dot_groups: Vec::new(),
            character_dots: std::collections::BTreeMap::new(),
        }
    }
}

impl BandStyle {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Clamps values coming from a file or the network into usable ranges.
    pub fn normalized(mut self) -> Self {
        let clamp_color = |color: [f32; 4]| {
            color.map(|component| {
                if component.is_finite() {
                    component.clamp(0.0, 1.0)
                } else {
                    1.0
                }
            })
        };
        self.background = clamp_color(self.background);
        self.background[3] = 1.0;
        self.playhead = clamp_color(self.playhead);
        self.playhead_width = if self.playhead_width.is_finite() {
            self.playhead_width
                .clamp(MIN_PLAYHEAD_WIDTH, MAX_PLAYHEAD_WIDTH)
        } else {
            DEFAULT_PLAYHEAD_WIDTH
        };
        self.karaoke_dot = self.karaoke_dot.normalized();
        let mut seen_ids = std::collections::HashSet::new();
        self.karaoke_dot_groups = std::mem::take(&mut self.karaoke_dot_groups)
            .into_iter()
            .filter(|group| seen_ids.insert(group.id))
            .take(MAX_DOT_GROUPS)
            .map(|group| KaraokeDotGroup {
                id: group.id,
                name: group
                    .name
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(MAX_DOT_GROUP_NAME_LEN)
                    .collect(),
                dot: group.dot.normalized(),
            })
            .collect();
        self.character_dots = std::mem::take(&mut self.character_dots)
            .into_iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, choice)| {
                let choice = match choice {
                    CharacterDot::Dot(dot) => CharacterDot::Dot(dot.normalized()),
                    group => group,
                };
                (name, choice)
            })
            .collect();
        self
    }

    pub fn dot_group(&self, id: u32) -> Option<&KaraokeDotGroup> {
        self.karaoke_dot_groups.iter().find(|group| group.id == id)
    }

    /// Dot of a character: its own dot, else its group's dot, else the
    /// default dot (also when its group was deleted).
    pub fn dot_for_character(&self, character_name: &str) -> &KaraokeDot {
        match self.character_dots.get(character_name) {
            Some(CharacterDot::Dot(dot)) => dot,
            Some(CharacterDot::Group(id)) => self
                .dot_group(*id)
                .map(|group| &group.dot)
                .unwrap_or(&self.karaoke_dot),
            None => &self.karaoke_dot,
        }
    }

    /// Every dot the style can draw: default, groups and character dots.
    pub fn all_karaoke_dots(&self) -> impl Iterator<Item = &KaraokeDot> {
        std::iter::once(&self.karaoke_dot)
            .chain(self.karaoke_dot_groups.iter().map(|group| &group.dot))
            .chain(self.character_dots.values().filter_map(|choice| match choice {
                CharacterDot::Dot(dot) => Some(dot),
                CharacterDot::Group(_) => None,
            }))
    }

    /// Adds a group and returns its id.
    pub fn add_dot_group(&mut self, name: String, dot: KaraokeDot) -> Option<u32> {
        if self.karaoke_dot_groups.len() >= MAX_DOT_GROUPS {
            return None;
        }
        let id = self
            .karaoke_dot_groups
            .iter()
            .map(|group| group.id)
            .max()
            .map_or(1, |id| id.saturating_add(1));
        self.karaoke_dot_groups
            .push(KaraokeDotGroup { id, name, dot });
        Some(id)
    }

    /// Deletes a group; its characters go back to the default dot.
    pub fn remove_dot_group(&mut self, id: u32) {
        self.karaoke_dot_groups.retain(|group| group.id != id);
        self.character_dots
            .retain(|_, choice| *choice != CharacterDot::Group(id));
    }

    /// Sets (or with `None`, clears) the dot of a character.
    pub fn set_character_dot(&mut self, character_name: &str, choice: Option<CharacterDot>) {
        let name = character_name.trim();
        if name.is_empty() {
            return;
        }
        match choice {
            Some(choice) => {
                self.character_dots.insert(name.to_string(), choice);
            }
            None => {
                self.character_dots.remove(name);
            }
        }
    }

    /// Gives `new_name` the dot of `old_name` when it has none yet. The old
    /// entry stays for lines that still use the old name (other languages,
    /// undo).
    pub fn copy_character_dot(&mut self, old_name: &str, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty() || self.character_dots.contains_key(new_name) {
            return false;
        }
        let Some(choice) = self.character_dots.get(old_name).cloned() else {
            return false;
        };
        self.character_dots.insert(new_name.to_string(), choice);
        true
    }

    /// The part of the style that presets carry: everything except the
    /// project's per-character choices.
    pub fn preset_part(&self) -> BandStyle {
        BandStyle {
            character_dots: std::collections::BTreeMap::new(),
            ..self.clone()
        }
    }
}

pub fn to_rgba8(color: [f32; 4]) -> [u8; 4] {
    color.map(|component| (component.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#RRGGBB` notation used by the style editor.
pub fn to_hex(color: [f32; 4]) -> String {
    let [r, g, b, _] = to_rgba8(color);
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Parses `#RGB`, `#RRGGBB` or the same without `#`. The alpha is opaque.
pub fn parse_hex(text: &str) -> Option<[f32; 4]> {
    let digits = text.trim().trim_start_matches('#');
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_string(),
        _ => return None,
    };
    let channel = |index: usize| {
        u8::from_str_radix(&expanded[index..index + 2], 16)
            .ok()
            .map(|value| value as f32 / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?, 1.0])
}

/// File name stem for an exported preset: the preset name without the
/// characters file systems reject.
pub fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim().trim_end_matches('.').to_string();
    if stem.is_empty() {
        "style".to_string()
    } else {
        stem
    }
}

/// A named style the user can apply, share and import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandStylePreset {
    pub name: String,
    pub style: BandStyle,
}

#[derive(Serialize, Deserialize)]
struct PresetFile {
    format: String,
    version: u32,
    #[serde(flatten)]
    preset: BandStylePreset,
}

const PRESET_FORMAT: &str = "coquerythmo-band-style";
const PRESET_VERSION: u32 = 1;

pub fn preset_to_json(preset: &BandStylePreset) -> String {
    serde_json::to_string_pretty(&PresetFile {
        format: PRESET_FORMAT.to_string(),
        version: PRESET_VERSION,
        preset: BandStylePreset {
            name: preset.name.clone(),
            style: preset.style.preset_part(),
        },
    })
    .expect("style presets always serialize")
}

pub fn preset_from_json(text: &str) -> Result<BandStylePreset, String> {
    let file: PresetFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if file.format != PRESET_FORMAT {
        return Err("not a Coquerythmo band style".to_string());
    }
    if file.version > PRESET_VERSION {
        return Err(format!("style format {} is too recent", file.version));
    }
    let name = file.preset.name.trim();
    Ok(BandStylePreset {
        name: if name.is_empty() {
            "Style".to_string()
        } else {
            name.to_string()
        },
        style: file.preset.style.normalized().preset_part(),
    })
}

/// Presets shipped with the application. Their names are i18n keys.
pub fn builtin_presets() -> Vec<BandStylePreset> {
    vec![
        BandStylePreset {
            name: "band_style.preset.classic".to_string(),
            style: BandStyle::default(),
        },
        BandStylePreset {
            name: "band_style.preset.studio".to_string(),
            style: BandStyle {
                background: [0.07, 0.08, 0.11, 1.0],
                playhead: [1.0, 0.78, 0.10, 1.0],
                playhead_width: 3.0,
                karaoke_dot: KaraokeDot::Circle,
                ..BandStyle::default()
            },
        },
        BandStylePreset {
            name: "band_style.preset.night".to_string(),
            style: BandStyle {
                background: [0.03, 0.05, 0.14, 1.0],
                playhead: [0.35, 0.80, 1.0, 1.0],
                playhead_width: 3.0,
                karaoke_dot: KaraokeDot::Star,
                ..BandStyle::default()
            },
        },
        BandStylePreset {
            name: "band_style.preset.contrast".to_string(),
            style: BandStyle {
                background: [0.0, 0.0, 0.0, 1.0],
                playhead: [0.0, 1.0, 0.25, 1.0],
                playhead_width: 5.0,
                karaoke_dot: KaraokeDot::Diamond,
                ..BandStyle::default()
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let color = parse_hex("#FF8000").unwrap();
        assert_eq!(to_hex(color), "#FF8000");
        assert_eq!(parse_hex("f80"), parse_hex("#FF8800"));
        assert!(parse_hex("#12345").is_none());
        assert!(parse_hex("zzzzzz").is_none());
    }

    #[test]
    fn preset_files_round_trip_and_are_validated() {
        let preset = BandStylePreset {
            name: "Mon style".into(),
            style: BandStyle {
                playhead_width: 6.0,
                ..BandStyle::default()
            },
        };
        let json = preset_to_json(&preset);
        assert_eq!(preset_from_json(&json).unwrap(), preset);
        assert!(preset_from_json("{\"name\":\"x\"}").is_err());
        let wrong = json.replace(PRESET_FORMAT, "other");
        assert!(preset_from_json(&wrong).is_err());
    }

    #[test]
    fn normalization_clamps_untrusted_values() {
        let style = BandStyle {
            background: [2.0, -1.0, f32::NAN, 0.2],
            playhead: [0.5; 4],
            playhead_width: 100.0,
            karaoke_dot: KaraokeDot::custom("not an image".into()),
            ..BandStyle::default()
        }
        .normalized();
        assert_eq!(style.background, [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(style.playhead_width, MAX_PLAYHEAD_WIDTH);
        assert_eq!(style.karaoke_dot, KaraokeDot::Circle);
    }

    fn png_base64(width: u32, height: u32) -> String {
        let mut image = image::RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, 200, 255]);
        }
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(image.as_raw(), width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        STANDARD.encode(png)
    }

    #[test]
    fn karaoke_dot_defaults_to_circle_and_is_omitted() {
        let style: BandStyle = serde_json::from_str("{\"playhead_width\": 4.0}").unwrap();
        assert_eq!(style.karaoke_dot, KaraokeDot::Circle);
        let json = serde_json::to_string(&BandStyle::default()).unwrap();
        assert!(!json.contains("karaoke_dot"));
        assert!(BandStyle::default().is_default());
        let star = BandStyle {
            karaoke_dot: KaraokeDot::Star,
            ..BandStyle::default()
        };
        assert!(!star.is_default());
        let json = serde_json::to_string(&star).unwrap();
        assert!(json.contains("\"karaoke_dot\":\"star\""));
        assert_eq!(serde_json::from_str::<BandStyle>(&json).unwrap(), star);
    }

    #[test]
    fn custom_karaoke_dot_round_trips_through_presets() {
        let preset = BandStylePreset {
            name: "Image".into(),
            style: BandStyle {
                karaoke_dot: KaraokeDot::custom(png_base64(16, 16)),
                ..BandStyle::default()
            },
        };
        let loaded = preset_from_json(&preset_to_json(&preset)).unwrap();
        assert_eq!(loaded, preset);
    }

    #[test]
    fn custom_karaoke_dot_is_resized_and_squared() {
        let dot = KaraokeDot::custom(png_base64(300, 150)).normalized();
        let KaraokeDot::Custom { png_base64, .. } = &dot else {
            panic!("a valid image stays custom");
        };
        let image = decode_custom_dot_png(png_base64).unwrap();
        assert_eq!(image.dimensions(), (MAX_CUSTOM_DOT_SIZE, MAX_CUSTOM_DOT_SIZE));
        // The letterbox above the image stays transparent.
        assert_eq!(image.get_pixel(64, 0)[3], 0);
        assert_eq!(image.get_pixel(64, 64)[3], 255);
        // Normalizing twice keeps the same image.
        assert_eq!(dot.clone().normalized(), dot);
    }

    #[test]
    fn garbage_custom_karaoke_dots_fall_back_to_circle() {
        for garbage in ["", "!!!", "aGVsbG8=", "A".repeat(5 * 1024 * 1024).as_str()] {
            let dot = KaraokeDot::custom(garbage.to_string());
            assert_eq!(dot.normalized(), KaraokeDot::Circle);
        }
        let json = format!(
            "{{\"format\":\"{PRESET_FORMAT}\",\"version\":1,\"name\":\"x\",\
             \"style\":{{\"karaoke_dot\":{{\"custom\":{{\"png_base64\":\"nope\"}}}}}}}}"
        );
        let preset = preset_from_json(&json).unwrap();
        assert_eq!(preset.style.karaoke_dot, KaraokeDot::Circle);
        assert!(preset_from_json(&json.replace("custom", "hexagon")).is_err());
    }

    #[test]
    fn old_single_image_styles_still_load() {
        let png = png_base64(8, 8);
        let json = format!("{{\"karaoke_dot\":{{\"custom\":{{\"png_base64\":\"{png}\"}}}}}}");
        let style: BandStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(style.karaoke_dot, KaraokeDot::custom(png.clone()));
        assert!(style.karaoke_dot_groups.is_empty());
        assert!(style.character_dots.is_empty());
        // A single image is written back without the new fields.
        let written = serde_json::to_string(&style).unwrap();
        assert!(!written.contains("jump_png_base64"));
        assert!(!written.contains("karaoke_dot_groups"));
        assert!(!written.contains("character_dots"));
        assert_eq!(style.karaoke_dot.image(true), Some(png.as_str()));
        assert!(!style.karaoke_dot.is_image_pair());
    }

    #[test]
    fn image_pairs_round_trip_and_fall_back_to_single_images() {
        let ground = png_base64(8, 8);
        let jump = png_base64(4, 4);
        let pair = KaraokeDot::Custom {
            png_base64: ground.clone(),
            jump_png_base64: Some(jump.clone()),
        };
        let json = serde_json::to_string(&pair).unwrap();
        assert_eq!(serde_json::from_str::<KaraokeDot>(&json).unwrap(), pair);
        assert_eq!(pair.image(false), Some(ground.as_str()));
        assert_eq!(pair.image(true), Some(jump.as_str()));
        let broken = KaraokeDot::Custom {
            png_base64: ground.clone(),
            jump_png_base64: Some("garbage".into()),
        };
        assert_eq!(broken.normalized(), KaraokeDot::custom(ground));
        assert_eq!(KaraokeDot::Star.image(false), None);
    }

    #[test]
    fn character_dots_resolve_assignment_then_group_then_default() {
        let mut style = BandStyle {
            karaoke_dot: KaraokeDot::Ring,
            ..BandStyle::default()
        };
        let group = style.add_dot_group("Héros".into(), KaraokeDot::Star).unwrap();
        let other = style.add_dot_group("Méchants".into(), KaraokeDot::Heart).unwrap();
        assert_ne!(group, other);
        style.set_character_dot("ALICE", Some(CharacterDot::Group(group)));
        style.set_character_dot("BOB", Some(CharacterDot::Dot(KaraokeDot::Note)));
        style.set_character_dot("EVE", Some(CharacterDot::Group(other)));
        assert_eq!(style.dot_for_character("ALICE"), &KaraokeDot::Star);
        assert_eq!(style.dot_for_character("BOB"), &KaraokeDot::Note);
        assert_eq!(style.dot_for_character("CAROL"), &KaraokeDot::Ring);
        assert_eq!(style.dot_for_character(""), &KaraokeDot::Ring);
        // A missing group falls back to the default dot.
        style
            .character_dots
            .insert("DAN".into(), CharacterDot::Group(999));
        assert_eq!(style.dot_for_character("DAN"), &KaraokeDot::Ring);
        // Deleting a group sends its characters back to the default dot.
        style.remove_dot_group(other);
        assert_eq!(style.dot_for_character("EVE"), &KaraokeDot::Ring);
        assert!(!style.character_dots.contains_key("EVE"));
        // Clearing an assignment restores the default dot.
        style.set_character_dot("BOB", None);
        assert_eq!(style.dot_for_character("BOB"), &KaraokeDot::Ring);
        // Renaming copies the choice to the new name.
        assert!(style.copy_character_dot("ALICE", "ALICIA"));
        assert_eq!(style.dot_for_character("ALICIA"), &KaraokeDot::Star);
        assert!(!style.copy_character_dot("ALICE", "ALICIA"));
        let dots: Vec<_> = style.all_karaoke_dots().collect();
        assert!(dots.contains(&&KaraokeDot::Star));
        assert!(dots.contains(&&KaraokeDot::Ring));
    }

    #[test]
    fn presets_carry_groups_but_not_character_choices() {
        let mut style = BandStyle::default();
        let group = style
            .add_dot_group(
                "Paire".into(),
                KaraokeDot::Custom {
                    png_base64: png_base64(8, 8),
                    jump_png_base64: Some(png_base64(6, 6)),
                },
            )
            .unwrap();
        style.set_character_dot("ALICE", Some(CharacterDot::Group(group)));
        let preset = BandStylePreset {
            name: "Groupes".into(),
            style: style.clone(),
        };
        let loaded = preset_from_json(&preset_to_json(&preset)).unwrap();
        assert_eq!(loaded.style.karaoke_dot_groups, style.karaoke_dot_groups);
        assert!(loaded.style.character_dots.is_empty());
        assert_eq!(loaded.style, style.preset_part());
        // Projects keep the character choices.
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(serde_json::from_str::<BandStyle>(&json).unwrap(), style);
    }

    #[test]
    fn group_normalization_drops_duplicates_and_trims_names() {
        let style = BandStyle {
            karaoke_dot_groups: vec![
                KaraokeDotGroup {
                    id: 1,
                    name: "A\u{7}".repeat(60),
                    dot: KaraokeDot::custom("garbage".into()),
                },
                KaraokeDotGroup {
                    id: 1,
                    name: "B".into(),
                    dot: KaraokeDot::Star,
                },
            ],
            ..BandStyle::default()
        }
        .normalized();
        assert_eq!(style.karaoke_dot_groups.len(), 1);
        assert_eq!(style.karaoke_dot_groups[0].name.chars().count(), MAX_DOT_GROUP_NAME_LEN);
        assert_eq!(style.karaoke_dot_groups[0].dot, KaraokeDot::Circle);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let style: BandStyle = serde_json::from_str("{\"playhead_width\": 4.0}").unwrap();
        assert_eq!(style.background, DEFAULT_BACKGROUND);
        assert_eq!(style.playhead_width, 4.0);
    }
}
