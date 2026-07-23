//! Aspect-preserving facade for vector text rasterization.

#[path = "vector_text.rs"]
mod implementation;

pub use implementation::{
    clear_project_font, measure_project_text_width_standalone, measure_rythmo_text_char_ratios_standalone,
    measure_rythmo_text_width, measure_rythmo_text_width_standalone, prepare_font_system,
    register_project_font, register_project_font_file, render_project_text_natural_standalone,
    render_rythmo_text, render_rythmo_text_natural, render_rythmo_text_tile,
    render_rythmo_text_tile_natural, render_rythmo_text_with_ratios, rythmo_font_family_name,
    selected_font_asset, VectorTextPixmap,
};

/// Rasterize emphasized naturally-shaped text without ever scaling its glyphs
/// vertically to fill the destination rectangle. Extra destination height stays
/// transparent; a shorter destination crops instead of deforming the glyphs.
pub fn render_rythmo_text_natural_emphasized(
    font_system: &mut glyphon::FontSystem,
    text: &str,
    font_size: f32,
    dest_w: u32,
    dest_h: u32,
) -> Option<VectorTextPixmap> {
    if text.is_empty() || dest_w == 0 || dest_h == 0 {
        return None;
    }

    let natural_height = (font_size * 1.4).ceil().max(1.0) as u32;
    let rendered = implementation::render_rythmo_text_natural_emphasized(
        font_system,
        text,
        font_size,
        dest_w,
        natural_height,
    )?;

    if dest_h == rendered.height {
        return Some(rendered);
    }

    let mut pixels = vec![0u8; dest_w as usize * dest_h as usize * 4];
    let copy_height = dest_h.min(rendered.height) as usize;
    let row_bytes = dest_w as usize * 4;
    for row in 0..copy_height {
        let start = row * row_bytes;
        pixels[start..start + row_bytes]
            .copy_from_slice(&rendered.pixels[start..start + row_bytes]);
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
    fn emphasized_destination_keeps_natural_height_and_transparent_padding() {
        let mut font_system = glyphon::FontSystem::new();
        let font_size = 20.0;
        let natural_height = (font_size * 1.4_f32).ceil() as u32;
        let padded_height = natural_height + 12;
        let rendered = render_rythmo_text_natural_emphasized(
            &mut font_system,
            "Jjjj ffff",
            font_size,
            240,
            padded_height,
        )
        .expect("emphasized text should rasterize");

        assert_eq!(rendered.height, padded_height);
        let row_bytes = rendered.width as usize * 4;
        let padding = &rendered.pixels[natural_height as usize * row_bytes..];
        assert!(padding.iter().all(|channel| *channel == 0));
    }
}
