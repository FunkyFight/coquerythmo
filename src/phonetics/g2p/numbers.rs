//! Spoken forms for numbers, letters (acronyms) and symbols.

use crate::phonetics::phoneme::Language;

/// Letter names used when an acronym is spelled out.
pub fn letter_name(language: Language, letter: char) -> String {
    let names: &[(&'static str, &'static str, &'static str)] = &[
        // (fr, en, es) — es uses normalized lowercase without diacritics.
        ("a", "a", "a"),
        ("bé", "bee", "be"),
        ("cé", "cee", "ce"),
        ("dé", "dee", "de"),
        ("e", "e", "e"),
        ("effe", "eff", "efe"),
        ("gé", "gee", "ge"),
        ("ache", "aitch", "hache"),
        ("i", "i", "i"),
        ("ji", "jay", "jota"),
        ("ka", "kay", "ka"),
        ("elle", "ell", "ele"),
        ("emme", "em", "eme"),
        ("enne", "en", "ene"),
        ("o", "o", "o"),
        ("pé", "pee", "pe"),
        ("cu", "cue", "cu"),
        ("erre", "ar", "ere"),
        ("esse", "ess", "ese"),
        ("té", "tee", "te"),
        ("u", "u", "u"),
        ("vé", "vee", "uve"),
        ("double-vé", "double-u", "uve-doble"),
        ("ixe", "ex", "equis"),
        ("i-grec", "wy", "i-griega"),
        ("zède", "zee", "zeta"),
    ];
    let index = match letter {
        'a'..='z' => (letter as usize) - ('a' as usize),
        _ => return letter.to_string(),
    };
    let row = names[index];
    match language {
        Language::French => row.0.to_string(),
        Language::English => row.1.to_string(),
        Language::Spanish => row.2.to_string(),
    }
}

/// Reading of a standalone symbol.
pub fn symbol_reading(language: Language, symbol: &str) -> Option<String> {
    let reading = match symbol {
        "%" => Some(("pour cent", "percent", "por ciento")),
        "€" => Some(("euros", "euros", "euros")),
        "$" => Some(("dollars", "dollars", "dólares")),
        "£" => Some(("livres", "pounds", "libras")),
        "&" => Some(("et", "and", "y")),
        "@" => Some(("arobase", "at", "arroba")),
        "#" => Some(("dièse", "hash", "almohadilla")),
        "+" => Some(("plus", "plus", "más")),
        "=" => Some(("égale", "equals", "igual")),
        "°" => Some(("degrés", "degrees", "grados")),
        _ => None,
    }?;
    Some(
        match language {
            Language::French => reading.0,
            Language::English => reading.1,
            Language::Spanish => reading.2,
        }
        .to_string(),
    )
}

fn fr_units() -> [&'static str; 17] {
    [
        "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix",
        "onze", "douze", "treize", "quatorze", "quinze", "seize",
    ]
}

fn en_units() -> [&'static str; 20] {
    [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
        "nineteen",
    ]
}

fn es_units() -> [&'static str; 16] {
    [
        "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez",
        "once", "doce", "trece", "catorce", "quince",
    ]
}

/// Spell an integer or decimal number out into words (already lowercased).
pub fn spell_number(language: Language, raw: &str) -> Vec<String> {
    let (int_part, frac_part) = match raw.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (raw, None),
    };
    let int_value: i64 = int_part
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    let mut words = spell_integer(language, int_value);
    if let Some(frac) = frac_part {
        match language {
            Language::French => words.push("virgule".into()),
            Language::English => words.push("point".into()),
            Language::Spanish => words.push("coma".into()),
        }
        for digit in frac.chars().filter(|c| c.is_ascii_digit()) {
            words.push(spell_integer(language, digit.to_digit(10).unwrap_or(0) as i64).remove(0));
        }
    }
    words
}

fn spell_integer(language: Language, value: i64) -> Vec<String> {
    if value < 0 {
        let mut out = vec![match language {
            Language::French => "moins",
            Language::English => "minus",
            Language::Spanish => "menos",
        }
        .to_string()];
        out.extend(spell_integer(language, -value));
        return out;
    }
    match language {
        Language::French => fr_spell(value),
        Language::English => en_spell(value),
        Language::Spanish => es_spell(value),
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn en_spell(v: i64) -> Vec<&'static str> {
    let u = en_units();
    let tens = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    let mut out: Vec<&'static str> = Vec::new();
    if v >= 1000 {
        for chunk in en_spell(v / 1000) {
            out.push(chunk);
        }
        out.push("thousand");
    }
    let rem = v % 1000;
    if rem >= 100 {
        out.push(u[(rem / 100) as usize]);
        out.push("hundred");
    }
    let tail = rem % 100;
    if tail >= 20 {
        out.push(tens[(tail / 10) as usize]);
        if tail % 10 > 0 {
            out.push(u[(tail % 10) as usize]);
        }
    } else if tail > 0 {
        out.push(u[tail as usize]);
    }
    if out.is_empty() {
        out.push(u[0]);
    }
    out
}

fn es_spell(v: i64) -> Vec<&'static str> {
    let u = es_units();
    let tens = [
        "", "", "veinte", "treinta", "cuarenta", "cincuenta", "sesenta", "setenta", "ochenta",
        "noventa",
    ];
    let veintis = [
        "", "veintiuno", "veintidós", "veintitrés", "veinticuatro", "veinticinco", "veintiséis",
        "veintisiete", "veintiocho", "veintinueve",
    ];
    let mut out: Vec<&'static str> = Vec::new();
    if v >= 1000 {
        for chunk in es_spell(v / 1000) {
            out.push(chunk);
        }
        out.push("mil");
    }
    let rem = v % 1000;
    if rem >= 100 {
        out.push(match rem / 100 {
            1 if rem == 100 => "cien",
            1 => "ciento",
            2 => "doscientos",
            3 => "trescientos",
            4 => "cuatrocientos",
            5 => "quinientos",
            6 => "seiscientos",
            7 => "setecientos",
            8 => "ochocientos",
            _ => "novecientos",
        });
    }
    let tail = rem % 100;
    match tail {
        0..=15 => {
            if tail > 0 {
                out.push(u[tail as usize]);
            }
        }
        16..=19 => {
            out.push(match tail {
                16 => "dieciséis",
                17 => "diecisiete",
                18 => "dieciocho",
                _ => "diecinueve",
            });
        }
        20..=29 => {
            if tail == 20 {
                out.push("veinte");
            } else {
                out.push(veintis[(tail % 10) as usize]);
            }
        }
        _ => {
            out.push(tens[(tail / 10) as usize]);
            if tail % 10 > 0 {
                out.push("y");
                out.push(u[(tail % 10) as usize]);
            }
        }
    }
    if out.is_empty() {
        out.push(u[0]);
    }
    out
}

fn fr_spell(v: i64) -> Vec<&'static str> {
    let u = fr_units();
    let mut out: Vec<&'static str> = Vec::new();
    if v >= 1_000_000 {
        for chunk in fr_spell(v / 1_000_000) {
            out.push(chunk);
        }
        out.push("million");
    }
    let v = v % 1_000_000;
    if v >= 1000 {
        let thousands = v / 1000;
        if thousands > 1 {
            for chunk in fr_spell(thousands) {
                out.push(chunk);
            }
        }
        out.push("mille");
    }
    let rem = v % 1000;
    if rem >= 100 {
        let hundreds = rem / 100;
        if hundreds > 1 {
            out.push(u[hundreds as usize]);
        }
        out.push(if hundreds > 1 && rem % 100 == 0 {
            "cents"
        } else {
            "cent"
        });
    }
    let tail = rem % 100;
    let push_tail = |out: &mut Vec<&'static str>, tail: i64| match tail {
        1..=16 => out.push(u[tail as usize]),
        17..=19 => {
            out.push("dix");
            out.push(u[(tail - 10) as usize]);
        }
        20..=69 => {
            let tens = ["", "vingt", "trente", "quarante", "cinquante", "soixante"];
            let t = tail / 10;
            let r = tail % 10;
            out.push(tens[(t - 1) as usize]);
            if r == 1 && t <= 5 {
                out.push("et");
                out.push(u[1]);
            } else if r > 0 {
                out.push(u[r as usize]);
            }
        }
        70..=79 => {
            out.push("soixante");
            if tail == 71 {
                out.push("et");
                out.push(u[1]);
            } else {
                out.push(match tail {
                    70 => "dix",
                    72 => "douze",
                    73 => "treize",
                    74 => "quatorze",
                    75 => "quinze",
                    76 => "seize",
                    77 => "dix-sept",
                    78 => "dix-huit",
                    _ => "dix-neuf",
                });
            }
        }
        80..=99 => {
            let r = tail - 80;
            out.push(if r == 0 { "quatre-vingts" } else { "quatre-vingt" });
            match r {
                0 => {}
                1..=16 => out.push(u[r as usize]),
                _ => {
                    // 97..=99 → dix-sept / dix-huit / dix-neuf written with a dash
                    // so the tokenizer keeps them as one word.
                    out.push(match r {
                        17 => "dix-sept",
                        18 => "dix-huit",
                        _ => "dix-neuf",
                    });
                }
            }
        }
        _ => {}
    };
    push_tail(&mut out, tail);
    if out.is_empty() {
        out.push(u[0]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_numbers() {
        assert_eq!(spell_number(Language::French, "0"), vec!["zéro"]);
        assert_eq!(spell_number(Language::French, "5"), vec!["cinq"]);
        assert_eq!(spell_number(Language::French, "21"), vec!["vingt", "et", "un"]);
        assert_eq!(spell_number(Language::French, "75"), vec!["soixante", "quinze"]);
        assert_eq!(spell_number(Language::French, "80"), vec!["quatre-vingts"]);
        assert_eq!(spell_number(Language::French, "97"), vec!["quatre-vingt", "dix-sept"]);
        assert_eq!(
            spell_number(Language::French, "1200"),
            vec!["mille", "deux", "cents"]
        );
    }

    #[test]
    fn english_numbers() {
        assert_eq!(spell_number(Language::English, "42"), vec!["forty", "two"]);
        assert_eq!(
            spell_number(Language::English, "105"),
            vec!["one", "hundred", "five"]
        );
        assert_eq!(
            spell_number(Language::English, "2019"),
            vec!["two", "thousand", "nineteen"]
        );
    }

    #[test]
    fn spanish_numbers() {
        assert_eq!(spell_number(Language::Spanish, "31"), vec!["treinta", "y", "uno"]);
        assert_eq!(spell_number(Language::Spanish, "16"), vec!["dieciséis"]);
        assert_eq!(spell_number(Language::Spanish, "100"), vec!["cien"]);
        assert_eq!(
            spell_number(Language::Spanish, "250"),
            vec!["doscientos", "cincuenta"]
        );
    }

    #[test]
    fn decimals_and_letters_and_symbols() {
        assert_eq!(
            spell_number(Language::French, "3.14"),
            vec!["trois", "virgule", "un", "quatre"]
        );
        assert_eq!(letter_name(Language::French, 'b'), "bé");
        assert_eq!(letter_name(Language::English, 'b'), "bee");
        assert_eq!(letter_name(Language::Spanish, 'v'), "uve");
        assert_eq!(symbol_reading(Language::French, "%").unwrap(), "pour cent");
        assert!(symbol_reading(Language::French, "~").is_none());
    }
}
