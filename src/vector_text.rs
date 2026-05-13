use std::sync::{Arc, OnceLock};

use glyphon::{Attrs, Buffer as GlyphonBuffer, Family, FontSystem, Metrics, Shaping};
use resvg::tiny_skia::{Pixmap, Transform};

static SYSTEM_FONT_DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();

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
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }

    let font_family = rythmo_font_family_name();
    let line_height = (font_size * 1.4).ceil().max(1.0);
    let natural_width =
        measure_text_width(font_system, text, font_size, line_height, &font_family).max(1.0);
    let char_x_ratios = measure_char_x_ratios(
        font_system,
        text,
        font_size,
        line_height,
        &font_family,
        natural_width,
    );

    let svg = build_svg(
        text,
        &font_family,
        font_size,
        natural_width,
        line_height,
        dest_w,
        dest_h,
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

fn measure_char_x_ratios(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_family: &str,
    natural_width: f32,
) -> Vec<f32> {
    let char_count = text.chars().count();
    let mut ratios = Vec::with_capacity(char_count + 1);
    ratios.push(0.0);

    for char_idx in 1..=char_count {
        let byte_end = text
            .char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len());
        let prefix = &text[..byte_end];
        let x = measure_text_width(font_system, prefix, font_size, line_height, font_family);
        ratios.push((x / natural_width).clamp(0.0, 1.0));
    }

    if let Some(last) = ratios.last_mut() {
        *last = 1.0;
    }
    ratios
}

fn measure_text_width(
    font_system: &mut FontSystem,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_family: &str,
) -> f32 {
    if text.is_empty() {
        return 0.0;
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
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            text_width = text_width.max(glyph.x + glyph.w);
        }
    }
    text_width
}

fn build_svg(
    text: &str,
    font_family: &str,
    font_size: f32,
    natural_width: f32,
    line_height: f32,
    dest_w: u32,
    dest_h: u32,
) -> String {
    let escaped_text = escape_xml(text);
    let escaped_family = escape_xml(font_family);
    let baseline = font_size;

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{dest_w}" height="{dest_h}" viewBox="0 0 {natural_width:.3} {line_height:.3}" preserveAspectRatio="none">
<text x="0" y="{baseline:.3}" font-family="{escaped_family}" font-size="{font_size:.3}" fill="white" textLength="{natural_width:.3}" lengthAdjust="spacingAndGlyphs" xml:space="preserve">{escaped_text}</text>
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
