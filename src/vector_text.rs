//! Vector text rasterization shared by preview and export.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::field_reassign_with_default)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping, Style, Weight,
};
use resvg::tiny_skia::{Pixmap, Transform};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectFont {
    family: String,
    path: PathBuf,
}

struct FontDbCache {
    project_font: Option<ProjectFont>,
    db: Arc<resvg::usvg::fontdb::Database>,
}

static SESSION_FONT_FAMILY: OnceLock<RwLock<Option<String>>> = OnceLock::new();

static PROJECT_FONT: OnceLock<RwLock<Option<ProjectFont>>> = OnceLock::new();
static SYSTEM_FONT_DB: OnceLock<RwLock<Option<FontDbCache>>> = OnceLock::new();

thread_local! {
    static MEASURE_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(FontSystem::new());
}

pub struct VectorTextPixmap {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub char_x_ratios: Vec<f32>,
}

/// Styled run of a text segment: `(start, end, style)`, a half-open range of
/// Unicode scalar indices relative to the segment text. Plain text has no run.
pub type TextStyleRun = (usize, usize, crate::rythmo_line::TextStyle);

/// Follow the room's font selection without saving over local preferences.
pub fn set_session_font_family(family: Option<String>) -> bool {
    let mut current = SESSION_FONT_FAMILY
        .get_or_init(|| RwLock::new(None))
        .write()
        .unwrap();
    if *current == family {
        return false;
    }
    *current = family;
    true
}

pub fn rythmo_font_family_name() -> String {
    if let Some(family) = SESSION_FONT_FAMILY
        .get_or_init(|| RwLock::new(None))
        .read()
        .unwrap()
        .as_ref()
    {
        return family.clone();
    }
    if let Some(font) = project_font() {
        return font.family;
    }
    crate::config::get()
        .ui
        .rythmo_font
        .clone()
        .unwrap_or_else(|| "sans-serif".to_string())
}

fn project_font() -> Option<ProjectFont> {
    PROJECT_FONT
        .get_or_init(|| RwLock::new(None))
        .read()
        .ok()
        .and_then(|font| font.clone())
}

/// Register a font extracted from a `.coquerythmo` bundle. The registration is
/// process-local: it overrides the global preference while this project is open
/// without modifying the user's application settings.
pub fn register_project_font(family: impl Into<String>, path: impl Into<PathBuf>) {
    let font = ProjectFont {
        family: family.into(),
        path: path.into(),
    };
    if let Ok(mut current) = PROJECT_FONT.get_or_init(|| RwLock::new(None)).write() {
        *current = Some(font);
    }
}

/// Register an extracted font by reading its family metadata. Returns the
/// family selected for rendering, or `None` when the file is not a valid font.
pub fn register_project_font_file(path: impl AsRef<Path>) -> Option<String> {
    let path = path.as_ref();
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_font_file(path).ok()?;
    let face = db.faces().next()?;
    let family = face.families.first()?.0.clone();
    register_project_font(family.clone(), path.to_path_buf());
    Some(family)
}

pub fn clear_project_font() {
    if let Ok(mut current) = PROJECT_FONT.get_or_init(|| RwLock::new(None)).write() {
        *current = None;
    }
}

/// Return the exact font file that should be embedded when saving a project.
/// For a font originating from a bundle this is the extracted asset; otherwise
/// the selected system face is resolved through fontdb.
pub fn selected_font_asset() -> Option<(String, PathBuf)> {
    if let Some(font) = project_font() {
        if font.path.is_file() {
            return Some((font.family, font.path));
        }
    }

    use resvg::usvg::fontdb::{Database, Family as DbFamily, Query, Source};
    let configured = crate::config::get().ui.rythmo_font.clone();
    let mut db = Database::new();
    db.load_system_fonts();
    let families = match configured.as_deref() {
        Some(name) => vec![DbFamily::Name(name)],
        None => vec![DbFamily::SansSerif],
    };
    let id = db.query(&Query {
        families: &families,
        ..Query::default()
    })?;
    let face = db.face(id)?;
    let family = face
        .families
        .first()
        .map(|(name, _)| name.clone())
        .or(configured)?;
    let path = match &face.source {
        Source::File(path) => path.clone(),
        Source::SharedFile(path, _) => path.clone(),
        Source::Binary(_) => return None,
        #[allow(unreachable_patterns)]
        _ => return None,
    };
    path.is_file().then_some((family, path))
}

/// Make a font system aware of the font bundled with the active project.
pub fn prepare_font_system(font_system: &mut FontSystem) {
    let Some(font) = project_font() else {
        return;
    };
    if !font.path.is_file() {
        return;
    }
    let family = [glyphon::cosmic_text::fontdb::Family::Name(&font.family)];
    let already_loaded = font_system
        .db()
        .query(&glyphon::cosmic_text::fontdb::Query {
            families: &family,
            ..glyphon::cosmic_text::fontdb::Query::default()
        })
        .is_some();
    if !already_loaded {
        if let Err(error) = font_system.db_mut().load_font_file(&font.path) {
            log::warn!(
                "Could not load bundled font {}: {error}",
                font.path.display()
            );
        }
    }
}

pub fn render_rythmo_text(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        false,
        true,
        false,
    )
}

pub fn render_rythmo_text_natural(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        false,
        false,
        false,
    )
}

pub fn render_rythmo_text_natural_emphasized(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        false,
        false,
        true,
    )
}

pub fn render_rythmo_text_tile(
    _font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_tile_impl(text, font_size, full_w, dest_h, tile_x, tile_w, true, &[])
}

pub fn render_rythmo_text_tile_natural(
    _font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_tile_impl(text, font_size, full_w, dest_h, tile_x, tile_w, false, &[])
}

/// Export tile of a text segment carrying per-character styles. Empty `runs`
/// renders exactly like the plain tile functions.
pub fn render_rythmo_text_tile_styled(
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
    stretch: bool,
    runs: &[TextStyleRun],
) -> Option<VectorTextPixmap> {
    render_rythmo_text_tile_impl(
        text, font_size, full_w, dest_h, tile_x, tile_w, stretch, runs,
    )
}

fn render_rythmo_text_tile_impl(
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
    stretch: bool,
    runs: &[TextStyleRun],
) -> Option<VectorTextPixmap> {
    if text.is_empty() || full_w == 0 || dest_h == 0 || tile_w == 0 || tile_x >= full_w {
        return None;
    }

    let tile_w = tile_w.min(full_w - tile_x).max(1);
    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let stretched_runs = (stretch && has_styled_runs(runs))
        .then(|| measure_rythmo_text_char_ratios_standalone(text, font_size))
        .flatten()
        .map(|ratios| {
            build_svg_stretched_runs(
                text,
                &font_family,
                font_size,
                line_height,
                full_w,
                dest_h,
                tile_x,
                tile_w,
                &ratios,
                runs,
            )
        });
    let svg = stretched_runs.unwrap_or_else(|| {
        build_svg_tile(
            text,
            &font_family,
            font_size,
            line_height,
            full_w,
            dest_h,
            tile_x,
            tile_w,
            stretch,
            runs,
        )
    });
    let mut options = resvg::usvg::Options::default();
    options.font_family = font_family;
    options.font_size = font_size;
    options.fontdb = system_fontdb();

    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).ok()?;
    let mut pixmap = Pixmap::new(tile_w, dest_h)?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());

    Some(VectorTextPixmap {
        pixels: pixmap.data().to_vec(),
        width: tile_w,
        height: dest_h,
        char_x_ratios: Vec::new(),
    })
}

pub fn render_rythmo_text_with_ratios(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        true,
        true,
        false,
    )
}

pub fn measure_rythmo_text_width(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
) -> Option<f32> {
    if text.is_empty() {
        return None;
    }

    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    Some(measure_text(font_system, text, font_size, line_height, &font_family).0)
}

pub fn measure_rythmo_text_width_standalone(text: &str, font_size: f32) -> Option<f32> {
    MEASURE_FONT_SYSTEM.with(|font_system| {
        measure_rythmo_text_width(&mut font_system.borrow_mut(), text, font_size)
    })
}

pub fn measure_ui_text_layout_standalone(text: &str, font_size: f32) -> (f32, Vec<f32>) {
    let line_height = (font_size * 1.3).ceil().max(1.0);
    MEASURE_FONT_SYSTEM.with(|font_system| {
        let (width, ratios) = measure_text(
            &mut font_system.borrow_mut(),
            text,
            font_size,
            line_height,
            "sans-serif",
        );
        let positions = ratios.into_iter().map(|ratio| ratio * width).collect();
        (width, positions)
    })
}

pub fn measure_rythmo_text_width_emphasized_standalone(text: &str, font_size: f32) -> Option<f32> {
    MEASURE_FONT_SYSTEM.with(|font_system| {
        measure_rythmo_text_width_emphasized(&mut font_system.borrow_mut(), text, font_size)
    })
}

fn measure_rythmo_text_width_emphasized(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
) -> Option<f32> {
    if text.is_empty() {
        return None;
    }

    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    Some(measure_text_emphasized(
        font_system,
        text,
        font_size,
        line_height,
        &font_family,
    ))
}

fn measure_text_emphasized(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_family: &str,
) -> f32 {
    prepare_font_system(font_system);
    let mut buffer = GlyphonBuffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(font_system, None, Some(line_height));
    let family = if font_family == "sans-serif" {
        Family::SansSerif
    } else {
        Family::Name(font_family)
    };
    buffer.set_text(
        font_system,
        text,
        &Attrs::new()
            .family(family)
            .style(Style::Italic)
            .weight(Weight::BOLD),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);

    let mut width = 0.0_f32;
    for run in buffer.layout_runs() {
        let mut left = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        for glyph in run.glyphs.iter() {
            left = left.min(glyph.x);
            right = right.max(glyph.x + glyph.w);
        }
        if left.is_finite() && right.is_finite() {
            width = width.max((right - left).max(0.0));
        }
    }
    width.max(1.0)
}

/// Measure every character boundary using the same shaping configuration as
/// the bande rythmo text renderer. Ratios are relative to the full text width.
pub fn measure_rythmo_text_char_ratios_standalone(text: &str, font_size: f32) -> Option<Vec<f32>> {
    if text.is_empty() {
        return None;
    }

    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    MEASURE_FONT_SYSTEM.with(|font_system| {
        Some(
            measure_text(
                &mut font_system.borrow_mut(),
                text,
                font_size,
                line_height,
                &font_family,
            )
            .1,
        )
    })
}

/// Measure text for non-UI artifacts without requiring `config::init()`.
/// A font embedded with the active project is preferred; otherwise the system
/// sans-serif fallback chain is used.
pub fn measure_project_text_width_standalone(text: &str, font_size: f32) -> Option<f32> {
    let font_family = project_font()
        .map(|font| font.family)
        .unwrap_or_else(|| "sans-serif".to_string());
    measure_text_width_with_family_standalone(text, font_size, Some(&font_family))
}

pub fn measure_text_width_with_family_standalone(
    text: &str,
    font_size: f32,
    font_family: Option<&str>,
) -> Option<f32> {
    if text.is_empty() {
        return None;
    }
    let font_family = font_family.unwrap_or("sans-serif");
    let line_height = (font_size * 1.4).ceil().max(1.0);
    MEASURE_FONT_SYSTEM.with(|font_system| {
        Some(measure_text_visual_bounds(
            &mut font_system.borrow_mut(),
            text,
            font_size,
            line_height,
            font_family,
        ))
    })
}

/// Rasterize naturally-shaped text for non-UI artifacts without requiring
/// application configuration. The returned RGBA pixmap uses the exact same
/// embedded-project/system font database as the bande rythmo renderer.
pub fn render_project_text_natural_standalone(
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    let font_family = project_font()
        .map(|font| font.family)
        .unwrap_or_else(|| "sans-serif".to_string());
    render_text_natural_with_family_standalone(text, font_size, dest_w, dest_h, Some(&font_family))
}

pub fn render_text_natural_with_family_standalone(
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    font_family: Option<&str>,
) -> Option<VectorTextPixmap> {
    render_text_natural_with_family_and_spacing_standalone(
        text,
        font_size,
        dest_w,
        dest_h,
        font_family,
        0.0,
    )
}

pub fn render_text_natural_with_family_and_spacing_standalone(
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    font_family: Option<&str>,
    letter_spacing: f32,
) -> Option<VectorTextPixmap> {
    render_text_natural_with_family_spacing_and_style_standalone(
        text,
        font_size,
        dest_w,
        dest_h,
        font_family,
        letter_spacing,
        false,
        false,
        false,
    )
}

pub fn render_text_natural_with_family_spacing_and_style_standalone(
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    font_family: Option<&str>,
    letter_spacing: f32,
    bold: bool,
    strikethrough: bool,
    underline: bool,
) -> Option<VectorTextPixmap> {
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }
    let font_family = font_family.unwrap_or("sans-serif");
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let svg = build_svg_styled(
        text,
        font_family,
        font_size,
        line_height,
        dest_w,
        dest_h,
        false,
        false,
        letter_spacing,
        bold,
        strikethrough,
        underline,
        &[],
    );
    let mut options = resvg::usvg::Options::default();
    options.font_family = font_family.to_string();
    options.font_size = font_size;
    options.fontdb = system_fontdb();

    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).ok()?;
    let mut pixmap = Pixmap::new(dest_w, dest_h)?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    Some(VectorTextPixmap {
        pixels: pixmap.data().to_vec(),
        width: dest_w,
        height: dest_h,
        char_x_ratios: Vec::new(),
    })
}

/// Rasterizes one Comic Dubs bubble line. The pixmap is `ceil(1.4 × size)`
/// tall (no vertical distortion) and starts `left_pad` pixels before the
/// first glyph so italic overhangs are not clipped.
#[allow(clippy::too_many_arguments)]
pub fn render_bubble_line_standalone(
    text: &str,
    font_size: f32,
    font_family: Option<&str>,
    letter_spacing: f32,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    underline: bool,
    left_pad: f32,
) -> Option<VectorTextPixmap> {
    if text.trim().is_empty() || !font_size.is_finite() || font_size <= 0.0 {
        return None;
    }
    let font_family = font_family.unwrap_or("sans-serif");
    let measured = measure_text_width_with_family_standalone(text, font_size, Some(font_family))
        .unwrap_or(font_size * text.chars().count() as f32 * 0.6);
    let spacing = letter_spacing.max(0.0) * text.chars().count() as f32;
    let width_factor = if bold { 1.08 } else { 1.0 };
    let dest_w = (measured * width_factor + spacing + left_pad * 2.0 + font_size * 0.35)
        .ceil()
        .clamp(1.0, 16_384.0) as u32;
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let dest_h = line_height as u32;
    let escaped_text = escape_xml(text);
    let escaped_family = escape_xml(font_family);
    let decoration = match (underline, strikethrough) {
        (true, true) => r#" text-decoration="underline line-through""#,
        (true, false) => r#" text-decoration="underline""#,
        (false, true) => r#" text-decoration="line-through""#,
        (false, false) => "",
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{dest_w}" height="{dest_h}" viewBox="0 0 {dest_w} {line_height:.3}">
<text x="{left_pad:.3}" y="{font_size:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white"{style}{weight}{decoration} letter-spacing="{letter_spacing:.3}" xml:space="preserve">{escaped_text}</text>
</svg>"#,
        style = if italic {
            r#" font-style="italic""#
        } else {
            ""
        },
        weight = if bold { r#" font-weight="700""# } else { "" },
    );
    let mut options = resvg::usvg::Options::default();
    options.font_family = font_family.to_string();
    options.font_size = font_size;
    options.fontdb = system_fontdb();
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).ok()?;
    let mut pixmap = Pixmap::new(dest_w, dest_h)?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    Some(VectorTextPixmap {
        pixels: pixmap.data().to_vec(),
        width: dest_w,
        height: dest_h,
        char_x_ratios: Vec::new(),
    })
}

/// Band text carrying per-character styles. Empty `runs` renders exactly like
/// the plain `render_rythmo_text*` functions with the same flags.
pub fn render_rythmo_text_styled(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    include_ratios: bool,
    stretch: bool,
    emphasized: bool,
    runs: &[TextStyleRun],
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl_with_runs(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        include_ratios,
        stretch,
        emphasized,
        runs,
    )
}

fn render_rythmo_text_impl(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    include_ratios: bool,
    stretch: bool,
    emphasized: bool,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl_with_runs(
        font_system,
        text,
        font_size,
        dest_w,
        dest_h,
        include_ratios,
        stretch,
        emphasized,
        &[],
    )
}

fn render_rythmo_text_impl_with_runs(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    include_ratios: bool,
    stretch: bool,
    emphasized: bool,
    runs: &[TextStyleRun],
) -> Option<VectorTextPixmap> {
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }

    prepare_font_system(font_system);
    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let char_x_ratios = if include_ratios {
        measure_text(font_system, text, font_size, line_height, &font_family).1
    } else {
        Vec::new()
    };

    let stretched_runs = (stretch && !emphasized && has_styled_runs(runs)).then(|| {
        let ratios = if char_x_ratios.is_empty() {
            measure_text(font_system, text, font_size, line_height, &font_family).1
        } else {
            char_x_ratios.clone()
        };
        build_svg_stretched_runs(
            text,
            &font_family,
            font_size,
            line_height,
            dest_w,
            dest_h,
            0,
            dest_w,
            &ratios,
            runs,
        )
    });
    let svg = stretched_runs.unwrap_or_else(|| {
        build_svg_styled(
            text,
            &font_family,
            font_size,
            line_height,
            dest_w,
            dest_h,
            stretch,
            emphasized,
            0.0,
            false,
            false,
            false,
            runs,
        )
    });
    let mut options = resvg::usvg::Options::default();
    options.font_family = font_family;
    options.font_size = font_size;
    options.fontdb = system_fontdb();

    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).ok()?;
    let mut pixmap = Pixmap::new(dest_w, dest_h)?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());

    Some(VectorTextPixmap {
        pixels: pixmap.data().to_vec(),
        width: dest_w,
        height: dest_h,
        char_x_ratios,
    })
}

fn system_fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    let project_font = project_font();
    let cache = SYSTEM_FONT_DB.get_or_init(|| RwLock::new(None));
    if let Ok(current) = cache.read() {
        if let Some(current) = current.as_ref() {
            if current.project_font == project_font {
                return current.db.clone();
            }
        }
    }

    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    if let Some(font) = &project_font {
        if let Err(error) = db.load_font_file(&font.path) {
            log::warn!(
                "Could not load bundled SVG font {}: {error}",
                font.path.display()
            );
        }
    }
    let db = Arc::new(db);
    if let Ok(mut current) = cache.write() {
        *current = Some(FontDbCache {
            project_font,
            db: db.clone(),
        });
    }
    db
}

fn measure_text(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_family: &str,
) -> (f32, Vec<f32>) {
    if text.is_empty() {
        return (0.0, vec![0.0]);
    }

    prepare_font_system(font_system);
    let mut buffer = GlyphonBuffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(font_system, None, Some(line_height));
    let family = if font_family == "sans-serif" {
        Family::SansSerif
    } else {
        Family::Name(font_family)
    };
    buffer.set_text(
        font_system,
        text,
        &Attrs::new().family(family),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);

    let mut text_width = 0.0_f32;
    let mut clusters = std::collections::BTreeMap::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let end = glyph.x + glyph.w;
            text_width = text_width.max(end);
            clusters
                .entry((glyph.start, glyph.end))
                .and_modify(|(left, right, _): &mut (f32, f32, bool)| {
                    *left = left.min(glyph.x);
                    *right = right.max(end);
                })
                .or_insert((glyph.x, end, glyph.level.is_rtl()));
        }
    }

    let natural_width = text_width.max(1.0);
    let char_offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let char_count = char_offsets.len() - 1;
    let mut boundary_x = vec![(0.0_f32, 0_u32); char_count + 1];
    for ((byte_start, byte_end), (left, right, rtl)) in clusters {
        let Ok(char_start) = char_offsets.binary_search(&byte_start) else {
            continue;
        };
        let Ok(char_end) = char_offsets.binary_search(&byte_end) else {
            continue;
        };
        let count = char_end.saturating_sub(char_start).max(1);
        for offset in 0..=count {
            let progress = offset as f32 / count as f32;
            let x = if rtl {
                right - (right - left) * progress
            } else {
                left + (right - left) * progress
            };
            boundary_x[char_start + offset].0 += x;
            boundary_x[char_start + offset].1 += 1;
        }
    }

    let mut ratios = Vec::with_capacity(char_count + 1);
    for (index, (sum, count)) in boundary_x.into_iter().enumerate() {
        let x = if count == 0 {
            index as f32 / char_count.max(1) as f32 * natural_width
        } else {
            sum / count as f32
        };
        ratios.push((x / natural_width).clamp(0.0, 1.0));
    }
    ratios[0] = 0.0;
    if let Some(last) = ratios.last_mut() {
        *last = 1.0;
    }

    (text_width, ratios)
}

/// Measure the occupied glyph bounds rather than their absolute buffer
/// positions. RTL runs are right-aligned by cosmic-text in the wide shaping
/// buffer, so using only `max(x + width)` would report a width near 10,000 px.
fn measure_text_visual_bounds(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_family: &str,
) -> f32 {
    prepare_font_system(font_system);
    let mut buffer = GlyphonBuffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(font_system, None, Some(line_height));
    let family = if font_family == "sans-serif" {
        Family::SansSerif
    } else {
        Family::Name(font_family)
    };
    buffer.set_text(
        font_system,
        text,
        &Attrs::new().family(family),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);

    let mut widest = 0.0_f32;
    for run in buffer.layout_runs() {
        let mut left = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        for glyph in run.glyphs.iter() {
            left = left.min(glyph.x);
            right = right.max(glyph.x + glyph.w);
        }
        if left.is_finite() && right.is_finite() {
            widest = widest.max((right - left).max(0.0));
        }
    }
    widest.max(1.0)
}

fn build_svg_styled(
    text: &str,
    font_family: &str,
    font_size: f32,
    line_height: f32,
    dest_w: u32,
    dest_h: u32,
    stretch: bool,
    emphasized: bool,
    letter_spacing: f32,
    bold: bool,
    strikethrough: bool,
    underline: bool,
    runs: &[TextStyleRun],
) -> String {
    let escaped_text = styled_text_content(text, runs);
    let escaped_family = escape_xml(font_family);
    let baseline = font_size;
    let stretch_attrs = if stretch {
        format!(r#" textLength="{dest_w}" lengthAdjust="spacingAndGlyphs""#)
    } else {
        String::new()
    };
    let emphasis = format!(
        "{}{}",
        if emphasized {
            r#" font-style="italic""#
        } else {
            ""
        },
        if emphasized || bold {
            r#" font-weight="700""#
        } else {
            ""
        },
    );
    let decoration = match (underline, strikethrough) {
        (true, true) => r#" text-decoration="underline line-through""#,
        (true, false) => r#" text-decoration="underline""#,
        (false, true) => r#" text-decoration="line-through""#,
        (false, false) => "",
    };
    let spacing = format!(r#" letter-spacing="{letter_spacing:.3}""#);
    // Bold italic glyphs commonly overhang to the left of their advance.
    // Starting at x=0 clips the first grapheme regardless of destination
    // width, so reserve explicit ink space inside emphasized label textures.
    let text_x = if emphasized { font_size * 0.25 } else { 0.0 };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{dest_w}" height="{dest_h}" viewBox="0 0 {dest_w} {line_height:.3}" preserveAspectRatio="none">
<text x="{text_x:.3}" y="{baseline:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white"{emphasis}{decoration}{spacing}{stretch_attrs} xml:space="preserve">{escaped_text}</text>
</svg>"#
    )
}

fn build_svg_tile(
    text: &str,
    font_family: &str,
    font_size: f32,
    line_height: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
    stretch: bool,
    runs: &[TextStyleRun],
) -> String {
    let escaped_text = styled_text_content(text, runs);
    let escaped_family = escape_xml(font_family);
    let baseline = font_size;
    let stretch_attrs = if stretch {
        format!(r#" textLength="{full_w}" lengthAdjust="spacingAndGlyphs""#)
    } else {
        String::new()
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{tile_w}" height="{dest_h}" viewBox="{tile_x} 0 {tile_w} {line_height:.3}" preserveAspectRatio="none">
<text x="0" y="{baseline:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white"{stretch_attrs} xml:space="preserve">{escaped_text}</text>
</svg>"#
    )
}

fn has_styled_runs(runs: &[TextStyleRun]) -> bool {
    runs.iter()
        .any(|(start, end, style)| start < end && !style.is_plain())
}

/// Stretched text with styled runs. resvg drops `textLength` as soon as a
/// `<text>` holds a `<tspan>`, which left styled segments at their natural
/// width. Each run is therefore its own `<text>`, placed where the regular
/// shaping puts it (`ratios`, one per character boundary) and stretched to
/// its share of `full_w`, so the segment still fills its synchronization
/// interval and the caret positions stay valid.
#[allow(clippy::too_many_arguments)]
fn build_svg_stretched_runs(
    text: &str,
    font_family: &str,
    font_size: f32,
    line_height: f32,
    full_w: u32,
    dest_h: u32,
    view_x: u32,
    view_w: u32,
    ratios: &[f32],
    runs: &[TextStyleRun],
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let style_at = |index: usize| {
        runs.iter()
            .rev()
            .find(|(start, end, _)| *start <= index && index < *end)
            .map(|(_, _, style)| *style)
            .unwrap_or_default()
    };
    let ratio_at = |index: usize| {
        ratios
            .get(index)
            .copied()
            .unwrap_or(index as f32 / len.max(1) as f32)
    };
    let escaped_family = escape_xml(font_family);
    let mut elements = String::new();
    let mut start = 0;
    while start < len {
        let style = style_at(start);
        let mut end = start + 1;
        while end < len && style_at(end) == style {
            end += 1;
        }
        let x = ratio_at(start) * full_w as f32;
        let width = (ratio_at(end) - ratio_at(start)) * full_w as f32;
        if width > 0.01 {
            let mut attrs = String::new();
            if style.bold {
                attrs.push_str(r#" font-weight="700""#);
            }
            if style.italic {
                attrs.push_str(r#" font-style="italic""#);
            }
            match (style.underline, style.strikethrough) {
                (true, true) => attrs.push_str(r#" text-decoration="underline line-through""#),
                (true, false) => attrs.push_str(r#" text-decoration="underline""#),
                (false, true) => attrs.push_str(r#" text-decoration="line-through""#),
                (false, false) => {}
            }
            let piece = escape_xml(&chars[start..end].iter().collect::<String>());
            elements.push_str(&format!(
                r#"<text x="{x:.3}" y="{font_size:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white"{attrs} textLength="{width:.3}" lengthAdjust="spacingAndGlyphs" xml:space="preserve">{piece}</text>"#
            ));
            elements.push('\n');
        }
        start = end;
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{view_w}" height="{dest_h}" viewBox="{view_x} 0 {view_w} {line_height:.3}" preserveAspectRatio="none">
{elements}</svg>"#
    )
}

/// Escaped `<text>` content where each styled run becomes a `<tspan>`. Only
/// used when the text is not stretched: resvg ignores `textLength` on a
/// `<text>` that holds `<tspan>` elements.
fn styled_text_content(text: &str, runs: &[TextStyleRun]) -> String {
    if runs
        .iter()
        .all(|(start, end, style)| start >= end || style.is_plain())
    {
        return escape_xml(text);
    }
    let mut runs: Vec<TextStyleRun> = runs
        .iter()
        .copied()
        .filter(|(start, end, style)| start < end && !style.is_plain())
        .collect();
    runs.sort_by_key(|(start, end, _)| (*start, *end));

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let slice = |start: usize, end: usize| -> String {
        escape_xml(&chars[start..end].iter().collect::<String>())
    };
    let mut content = String::with_capacity(text.len() + runs.len() * 64);
    let mut cursor = 0;
    for (start, end, style) in runs {
        let start = start.max(cursor).min(len);
        let end = end.min(len);
        if start >= end {
            continue;
        }
        content.push_str(&slice(cursor, start));
        content.push_str("<tspan");
        if style.bold {
            content.push_str(r#" font-weight="700""#);
        }
        if style.italic {
            content.push_str(r#" font-style="italic""#);
        }
        match (style.underline, style.strikethrough) {
            (true, true) => content.push_str(r#" text-decoration="underline line-through""#),
            (true, false) => content.push_str(r#" text-decoration="underline""#),
            (false, true) => content.push_str(r#" text-decoration="line-through""#),
            (false, false) => {}
        }
        content.push('>');
        content.push_str(&slice(start, end));
        content.push_str("</tspan>");
        cursor = end;
    }
    content.push_str(&slice(cursor, len));
    content
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasized_width_is_not_smaller_than_empty() {
        crate::config::init();
        let cases = ["AL"];
        for name in cases.iter() {
            let w = measure_rythmo_text_width_emphasized_standalone(name, 16.0);
            assert!(w.is_some());
            assert!(w.unwrap() > 0.0);
        }
    }

    #[test]
    fn character_ratios_follow_clusters_without_wrapping_long_lines() {
        crate::config::init();
        let combined = measure_rythmo_text_char_ratios_standalone("e\u{301}", 16.0).unwrap();
        assert!(combined[0] < combined[1] && combined[1] < combined[2]);

        let long = measure_rythmo_text_char_ratios_standalone(&"W".repeat(2_000), 16.0).unwrap();
        assert!(long.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn comic_text_style_is_encoded_in_svg() {
        let svg = build_svg_styled(
            "Texte",
            "sans-serif",
            24.0,
            34.0,
            200,
            40,
            false,
            false,
            1.5,
            true,
            true,
            true,
            &[],
        );
        assert!(svg.contains(r#"font-weight="700""#));
        assert!(svg.contains(r#"text-decoration="underline line-through""#));
        assert!(svg.contains(r#"letter-spacing="1.500""#));
    }

    fn style(
        bold: bool,
        italic: bool,
        underline: bool,
        strikethrough: bool,
    ) -> crate::rythmo_line::TextStyle {
        crate::rythmo_line::TextStyle {
            bold,
            italic,
            underline,
            strikethrough,
        }
    }

    fn band_svg(text: &str, runs: &[TextStyleRun]) -> String {
        build_svg_styled(
            text,
            "sans-serif",
            24.0,
            34.0,
            200,
            40,
            true,
            false,
            0.0,
            false,
            false,
            false,
            runs,
        )
    }

    #[test]
    fn styled_runs_become_tspans_inside_stretched_text() {
        let svg = band_svg(
            "Bonjour <toi> & moi",
            &[
                (0, 3, style(true, false, false, false)),
                (8, 13, style(false, true, true, false)),
                (16, 19, style(false, false, false, true)),
            ],
        );
        assert!(svg.contains(r#"<tspan font-weight="700">Bon</tspan>jour "#));
        assert!(svg.contains(
            r#"<tspan font-style="italic" text-decoration="underline">&lt;toi&gt;</tspan> &amp; "#
        ));
        assert!(svg.contains(r#"<tspan text-decoration="line-through">moi</tspan></text>"#));
        assert!(svg.contains(r#"textLength="200" lengthAdjust="spacingAndGlyphs""#));
        let tile = build_svg_tile(
            "abcd",
            "sans-serif",
            24.0,
            34.0,
            200,
            40,
            0,
            100,
            true,
            &[(1, 2, style(true, true, true, true))],
        );
        assert!(tile.contains(
            r#"a<tspan font-weight="700" font-style="italic" text-decoration="underline line-through">b</tspan>cd"#
        ));
    }

    #[test]
    fn plain_runs_keep_the_unstyled_svg() {
        let plain = band_svg("\u{c9}t\u{e9} <ok>", &[]);
        assert_eq!(
            plain,
            band_svg(
                "\u{c9}t\u{e9} <ok>",
                &[(0, 3, style(false, false, false, false))]
            )
        );
        assert!(!plain.contains("tspan"));
        assert!(plain.contains(">\u{c9}t\u{e9} &lt;ok&gt;</text>"));
        // Runs are clamped to the text and indexed by char, not by byte.
        let svg = band_svg("\u{c9}t\u{e9}", &[(1, 9, style(true, false, false, false))]);
        assert!(svg.contains(">\u{c9}<tspan font-weight=\"700\">t\u{e9}</tspan></text>"));
    }

    #[test]
    fn styled_text_rasterizes_differently() {
        crate::config::init();
        let mut font_system = FontSystem::new();
        let plain = render_rythmo_text_styled(
            &mut font_system,
            "Texte",
            24.0,
            120,
            34,
            false,
            true,
            false,
            &[],
        )
        .unwrap();
        let styled = render_rythmo_text_styled(
            &mut font_system,
            "Texte",
            24.0,
            120,
            34,
            false,
            true,
            false,
            &[(0, 5, style(true, true, true, true))],
        )
        .unwrap();
        assert_eq!(plain.width, styled.width);
        assert_ne!(plain.pixels, styled.pixels);
    }

    /// Columns holding ink, as `(first, last)`.
    fn ink_columns(pixmap: &VectorTextPixmap) -> (u32, u32) {
        let mut first = pixmap.width;
        let mut last = 0;
        for y in 0..pixmap.height {
            for x in 0..pixmap.width {
                let alpha = pixmap.pixels[((y * pixmap.width + x) * 4 + 3) as usize];
                if alpha > 40 {
                    first = first.min(x);
                    last = last.max(x);
                }
            }
        }
        (first, last)
    }

    #[test]
    fn styled_runs_keep_the_stretch_to_the_segment_width() {
        crate::config::init();
        let mut font_system = FontSystem::new();
        for runs in [
            vec![(0, 3, style(true, false, false, false))],
            vec![(2, 5, style(false, true, false, false))],
            vec![(1, 4, style(true, true, true, true))],
        ] {
            let plain = render_rythmo_text_styled(
                &mut font_system, "Texte", 24.0, 600, 34, false, true, false, &[],
            )
            .unwrap();
            let styled = render_rythmo_text_styled(
                &mut font_system, "Texte", 24.0, 600, 34, false, true, false, &runs,
            )
            .unwrap();
            let (_, plain_last) = ink_columns(&plain);
            let (first, last) = ink_columns(&styled);
            assert!(first < 60, "styled text starts at {first}");
            assert!(last > 540, "styled text stops at {last}, plain at {plain_last}");
        }
    }
}
