//! Vector text rasterization shared by preview and export.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::field_reassign_with_default)]

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use glyphon::{Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping};
use resvg::tiny_skia::{Pixmap, Transform};

static SYSTEM_FONT_DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();

thread_local! {
    static MEASURE_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(FontSystem::new());
}

pub struct VectorTextPixmap {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub char_x_ratios: Vec<f32>,
}

pub fn rythmo_font_family_name() -> String {
    crate::config::get()
        .ui
        .rythmo_font
        .clone()
        .unwrap_or_else(|| "sans-serif".to_string())
}

pub fn render_rythmo_text(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(font_system, text, font_size, dest_w, dest_h, false, true)
}

pub fn render_rythmo_text_natural(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    render_rythmo_text_impl(font_system, text, font_size, dest_w, dest_h, false, false)
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
    render_rythmo_text_tile_impl(text, font_size, full_w, dest_h, tile_x, tile_w, true)
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
    render_rythmo_text_tile_impl(text, font_size, full_w, dest_h, tile_x, tile_w, false)
}

fn render_rythmo_text_tile_impl(
    text: &str,
    font_size: f32,
    full_w: u32,
    dest_h: u32,
    tile_x: u32,
    tile_w: u32,
    stretch: bool,
) -> Option<VectorTextPixmap> {
    if text.is_empty() || full_w == 0 || dest_h == 0 || tile_w == 0 || tile_x >= full_w {
        return None;
    }

    let tile_w = tile_w.min(full_w - tile_x).max(1);
    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let svg = build_svg_tile(
        text,
        &font_family,
        font_size,
        line_height,
        full_w,
        dest_h,
        tile_x,
        tile_w,
        stretch,
    );
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
    render_rythmo_text_impl(font_system, text, font_size, dest_w, dest_h, true, true)
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

fn render_rythmo_text_impl(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
    include_ratios: bool,
    stretch: bool,
) -> Option<VectorTextPixmap> {
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }

    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let char_x_ratios = if include_ratios {
        measure_text(font_system, text, font_size, line_height, &font_family).1
    } else {
        Vec::new()
    };

    let svg = build_svg(
        text,
        &font_family,
        font_size,
        line_height,
        dest_w,
        dest_h,
        stretch,
    );
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
    SYSTEM_FONT_DB
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
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

    let mut buffer = GlyphonBuffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(font_system, Some(10000.0), Some(line_height));
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
    let mut glyph_ends = Vec::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let end = glyph.x + glyph.w;
            glyph_ends.push(end);
            text_width = text_width.max(end);
        }
    }

    let natural_width = text_width.max(1.0);
    let char_count = text.chars().count();
    let mut ratios = Vec::with_capacity(char_count + 1);
    ratios.push(0.0);
    for char_idx in 1..=char_count {
        let end = if glyph_ends.is_empty() {
            natural_width
        } else if glyph_ends.len() == char_count {
            glyph_ends[char_idx - 1]
        } else {
            let glyph_idx = ((char_idx * glyph_ends.len()).saturating_sub(1)) / char_count;
            glyph_ends[glyph_idx.min(glyph_ends.len() - 1)]
        };
        ratios.push((end / natural_width).clamp(0.0, 1.0));
    }
    if let Some(last) = ratios.last_mut() {
        *last = 1.0;
    }

    (text_width, ratios)
}

fn build_svg(
    text: &str,
    font_family: &str,
    font_size: f32,
    line_height: f32,
    dest_w: u32,
    dest_h: u32,
    stretch: bool,
) -> String {
    let escaped_text = escape_xml(text);
    let escaped_family = escape_xml(font_family);
    let baseline = font_size;
    let stretch_attrs = if stretch {
        format!(r#" textLength="{dest_w}" lengthAdjust="spacingAndGlyphs""#)
    } else {
        String::new()
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{dest_w}" height="{dest_h}" viewBox="0 0 {dest_w} {line_height:.3}" preserveAspectRatio="none">
<text x="0" y="{baseline:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white"{stretch_attrs} xml:space="preserve">{escaped_text}</text>
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
) -> String {
    let escaped_text = escape_xml(text);
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
