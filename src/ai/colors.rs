//! Colours as people name them to the assistant.

/// Linear RGBA for a colour name (French, English or Spanish) or a hex code.
pub fn parse_color(text: &str) -> Option<[f32; 4]> {
    let raw = text.trim();
    if let Some(hex) = raw.strip_prefix('#').or_else(|| raw.strip_prefix("0x")) {
        return parse_hex(hex);
    }
    if raw.len() == 6 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return parse_hex(raw);
    }
    let normalized = super::retrieval::normalize(raw);
    let word = normalized.trim();
    let rgb: [u8; 3] = match word {
        "rouge" | "red" | "rojo" | "roja" => [220, 60, 60],
        "bleu" | "bleue" | "blue" | "azul" => [70, 130, 230],
        "bleu clair" | "light blue" | "azul claro" | "ciel" | "sky" => [120, 190, 250],
        "bleu fonce" | "dark blue" | "azul oscuro" | "marine" | "navy" => [40, 70, 160],
        "vert" | "verte" | "green" | "verde" => [70, 180, 90],
        "vert clair" | "light green" | "verde claro" => [140, 220, 140],
        "vert fonce" | "dark green" | "verde oscuro" => [30, 110, 60],
        "jaune" | "yellow" | "amarillo" | "amarilla" => [240, 210, 60],
        "orange" | "naranja" => [245, 140, 40],
        "violet" | "violette" | "purple" | "morado" | "morada" | "mauve" | "pourpre" => {
            [150, 90, 220]
        }
        "rose" | "pink" | "rosa" | "fuchsia" | "fushia" => [240, 110, 180],
        "noir" | "noire" | "black" | "negro" | "negra" => [25, 25, 30],
        "blanc" | "blanche" | "white" | "blanco" | "blanca" => [245, 245, 245],
        "gris" | "grise" | "gray" | "grey" => [150, 150, 160],
        "cyan" | "turquoise" | "teal" | "aqua" => [60, 200, 200],
        "marron" | "brun" | "brown" | "marron fonce" | "cafe" => [140, 90, 50],
        "beige" | "creme" | "cream" => [230, 210, 170],
        "or" | "dore" | "gold" | "dorado" => [220, 180, 50],
        "argent" | "silver" | "plata" => [190, 195, 205],
        "corail" | "coral" => [250, 120, 100],
        "saumon" | "salmon" => [250, 150, 130],
        "lavande" | "lavender" => [190, 170, 240],
        "olive" => [130, 140, 60],
        "bordeaux" | "burgundy" | "maroon" | "grenat" => [130, 30, 50],
        "indigo" => [80, 60, 180],
        "lime" | "citron vert" => [170, 230, 60],
        "menthe" | "mint" | "menta" => [120, 230, 190],
        _ => return None,
    };
    Some(from_rgb8(rgb))
}

fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<Vec<u8>>>()?;
    let rgb = match digits.as_slice() {
        [r, g, b] => [r * 17, g * 17, b * 17],
        [r1, r2, g1, g2, b1, b2] => [r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2],
        _ => return None,
    };
    Some(from_rgb8(rgb))
}

fn from_rgb8(rgb: [u8; 3]) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        1.0,
    ]
}

/// A stable colour for a character the project does not know yet.
pub fn color_for_new_character(name: &str) -> [f32; 4] {
    const PALETTE: [[u8; 3]; 8] = [
        [230, 90, 90],
        [90, 160, 240],
        [110, 200, 110],
        [240, 190, 70],
        [190, 120, 230],
        [240, 140, 60],
        [80, 200, 200],
        [230, 120, 180],
    ];
    let hash = name
        .bytes()
        .fold(0usize, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as usize));
    from_rgb8(PALETTE[hash % PALETTE.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_hex_codes() {
        assert_eq!(parse_color("rouge"), Some(from_rgb8([220, 60, 60])));
        assert_eq!(parse_color("Bleu"), parse_color("blue"));
        assert_eq!(parse_color("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("00ff00"), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_color("chartreuse"), None);
        assert_eq!(parse_color(""), None);
    }

    #[test]
    fn new_characters_get_a_stable_colour() {
        assert_eq!(color_for_new_character("Jean"), color_for_new_character("Jean"));
    }
}
