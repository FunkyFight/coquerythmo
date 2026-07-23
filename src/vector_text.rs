//! Vector text facade preserving natural emphasized glyph metrics.

#[path = "vector_text_legacy.rs"]
mod legacy;

pub use legacy::{
    clear_project_font, measure_project_text_width_standalone,
    measure_rythmo_text_char_ratios_standalone, measure_rythmo_text_width,
    measure_rythmo_text_width_standalone, prepare_font_system, register_project_font,
    register_project_font_file, render_project_text_natural_standalone, render_rythmo_text,
    render_rythmo_text_natural, render_rythmo_text_tile, render_rythmo_text_tile_natural,
    render_rythmo_text_with_ratios, rythmo_font_family_name, selected_font_asset,
    VectorTextPixmap,
};

/// Rasterize emphasized labels at their natural line height. The legacy
/// renderer remains responsible for shaping, bold/italic style and the active
/// project font; this facade only prevents its SVG viewBox from being stretched
/// to an unrelated collision rectangle height.
pub fn render_rythmo_text_natural_emphasized(
    font_system: &mut glyphon::FontSystem,
    text: &str,
    _font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }
    let font_size = dest_h as f32 / 1.4;
    let natural_height = (font_size * 1.4).ceil().max(1.0) as u32;
    let rendered = legacy::render_rythmo_text_natural_emphasized(
        font_system,
        text,
        font_size,
        dest_w,
        natural_height,
    )?;

    if rendered.height == dest_h {
        return Some(rendered);
    }

    let mut pixels = vec![0; dest_w as usize * dest_h as usize * 4];
    let rows = rendered.height.min(dest_h) as usize;
    let row_bytes = dest_w as usize * 4;
    for row in 0..rows {
        let source_start = row * row_bytes;
        let destination_start = row * row_bytes;
        pixels[destination_start..destination_start + row_bytes].copy_from_slice(
            &rendered.pixels[source_start..source_start + row_bytes],
        );
    }
    Some(VectorTextPixmap {
        pixels,
        width: dest_w,
        height: dest_h,
        char_x_ratios: rendered.char_x_ratios,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasized_destination_uses_natural_font_height() {
        for height in [20_u32, 40, 80] {
            let font_size = height as f32 / 1.4;
            assert!(((font_size * 1.4).ceil() - height as f32).abs() <= 1.0);
        }
    }
}
