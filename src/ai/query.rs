//! Reads in the user's own words what the small model tends to miss or
//! rewrite: positions as written, track numbers, the two names of a rename,
//! and references to an existing line ("la toute première ligne", "celle qui
//! dit « … »"). Pure functions over the text; the app resolves the result
//! against the project.

use super::retrieval::normalize;

/// Every quoted span of a text ("…", «…», “…”), trimmed, in order.
pub fn quoted_strings(text: &str) -> Vec<String> {
    quoted_ranges(text)
        .into_iter()
        .filter_map(|(start, end)| {
            let open_len = text[start..].chars().next().map_or(1, char::len_utf8);
            let inner = text.get(start + open_len..end)?.trim();
            (!inner.is_empty()).then(|| inner.to_string())
        })
        .collect()
}

/// Byte ranges `(opening quote, closing quote)` of a text, so separators
/// inside quotes are read as text.
pub fn quoted_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<(usize, char)> = None;
    for (index, c) in text.char_indices() {
        match open {
            Some((start, close)) if c == close => {
                ranges.push((start, index));
                open = None;
            }
            Some(_) => {}
            None => {
                let close = match c {
                    '"' => '"',
                    '«' => '»',
                    '“' => '”',
                    _ => continue,
                };
                open = Some((index, close));
            }
        }
    }
    if let Some((start, _)) = open {
        ranges.push((start, text.len()));
    }
    ranges
}

fn is_time_unit(word: &str) -> bool {
    matches!(
        word,
        "h" | "hr" | "hrs" | "heure" | "heures" | "hour" | "hours" | "hora" | "horas"
            | "m" | "mn" | "min" | "mins" | "minute" | "minutes" | "minuto" | "minutos"
            | "s" | "sec" | "secs" | "seconde" | "secondes" | "second" | "seconds" | "segundo" | "segundos"
            | "image" | "images" | "frame" | "frames" | "fotograma" | "fotogramas" | "img" | "f"
    )
}

fn is_number(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|c| c.is_ascii_digit() || c == '.')
        && word.chars().any(|c| c.is_ascii_digit())
}

/// A token like `1:30`, `00:01:30:12`, `90s`, `1m30`, `2h`.
fn is_compact_time(word: &str) -> bool {
    if word.contains(':') {
        return word.split(':').all(is_number);
    }
    let digits: String = word.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if digits.is_empty() || digits.len() == word.len() {
        return false;
    }
    let rest = &word[digits.len()..];
    let unit: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    is_time_unit(&unit) && rest[unit.len()..].chars().all(|c| c.is_ascii_digit())
}

/// The position written in a sentence, exactly as written ("1 min 30",
/// "1:30", "00:01:30:12", "90s", "image 120"), or `None` when the sentence
/// has no number with a unit or a colon (a bare "2" is a track, a count...).
pub fn time_span(query: &str) -> Option<String> {
    let lowered = query.to_lowercase().replace(',', ".");
    let words: Vec<String> = lowered
        .split(|c: char| c.is_whitespace() || c == '\'' || c == '’')
        .map(|word| {
            word.trim_matches(|c: char| !(c.is_alphanumeric() || c == ':' || c == '.'))
                .trim_end_matches('.')
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let mut index = 0;
    while index < words.len() {
        let word = &words[index];
        let starts = is_number(word) || is_compact_time(word) || (is_time_unit(word) && word.len() > 1);
        if !starts {
            index += 1;
            continue;
        }
        let mut end = index;
        let mut timed = is_compact_time(word) || is_time_unit(word);
        let mut has_digit = word.chars().any(|c| c.is_ascii_digit());
        while end + 1 < words.len() {
            let next = &words[end + 1];
            if is_number(next) || is_compact_time(next) || is_time_unit(next) {
                timed |= is_compact_time(next) || is_time_unit(next);
                has_digit |= next.chars().any(|c| c.is_ascii_digit());
                end += 1;
            } else if matches!(next.as_str(), "et" | "and" | "y")
                && end + 2 < words.len()
                && is_number(&words[end + 2])
            {
                end += 1;
            } else {
                break;
            }
        }
        if timed && has_digit {
            let span: Vec<&str> = words[index..=end]
                .iter()
                .map(String::as_str)
                .filter(|word| !matches!(*word, "et" | "and" | "y"))
                .collect();
            return Some(span.join(" "));
        }
        index = end + 1;
    }
    None
}

/// 1-based position of an ordinal word (negative counts from the end).
pub fn ordinal_number(word: &str) -> Option<i64> {
    Some(match word {
        "premiere" | "premier" | "1re" | "1ere" | "1er" | "first" | "primera" | "primer" | "primero" | "1st" => 1,
        "deuxieme" | "seconde" | "second" | "2e" | "2eme" | "segunda" | "segundo" | "2nd" => 2,
        "troisieme" | "third" | "3e" | "3eme" | "tercera" | "tercero" | "3rd" => 3,
        "quatrieme" | "fourth" | "4e" | "4eme" | "cuarta" | "cuarto" | "4th" => 4,
        "cinquieme" | "fifth" | "5e" | "5eme" | "quinta" | "quinto" | "5th" => 5,
        "derniere" | "dernier" | "last" | "ultima" | "ultimo" => -1,
        "penultieme" | "penultimate" | "penultima" | "penultimo" => -2,
        _ => return None,
    })
}

/// The track number of a sentence: "piste 2", "ligne 2", "track 2",
/// "rangée 2", "pista 2", "deuxième piste".
pub fn track_number(query: &str) -> Option<i64> {
    let normalized = normalize(query);
    let words: Vec<&str> = normalized.split_whitespace().collect();
    const NOUNS: &[&str] = &["piste", "ligne", "track", "pista", "rangee", "row", "linea"];
    for (index, word) in words.iter().enumerate() {
        if NOUNS.contains(word) {
            if let Some(number) = words.get(index + 1).and_then(|next| next.parse::<i64>().ok()) {
                return Some(number);
            }
        }
        if words.get(index + 1).is_some_and(|next| NOUNS.contains(next)) {
            if let Some(number) = ordinal_number(word).filter(|number| *number > 0) {
                return Some(number);
            }
        }
    }
    None
}

const LINE_NOUNS: &[&str] = &[
    "ligne", "lignes", "replique", "repliques", "dialogue", "dialogues", "phrase", "line", "lines",
    "linea", "lineas", "replica", "frase",
];

/// A reference to existing lines in a sentence.
#[derive(Debug, Clone, PartialEq)]
pub enum LineRef {
    /// "celle qui dit « … »", "the line that says …": lines containing it.
    Says(String),
    /// "la première ligne" (1), "la dernière réplique de Marie" (-1, "marie").
    Ordinal { position: i64, character: Option<String> },
    /// "la ligne ici", "sous le curseur".
    AtPlayhead,
}

const SAYS_MARKERS: &[&str] = &[
    "qui dit", "qui disent", "qui contient", "qui contiennent", "contenant", "avec le texte",
    "that says", "which says", "saying", "containing", "with the text", "que dice", "que contiene",
];

/// References to existing lines, the most specific first.
pub fn line_references(query: &str) -> Vec<LineRef> {
    let mut refs = Vec::new();
    let normalized = normalize(query);
    for marker in SAYS_MARKERS {
        if let Some(position) = normalized.find(&format!(" {marker} ")) {
            // The quoted text if any, else the rest of the sentence.
            let text = quoted_strings(query).last().cloned().or_else(|| {
                let rest = normalized[position + marker.len() + 2..].trim().to_string();
                (!rest.is_empty()).then_some(rest)
            });
            if let Some(text) = text {
                refs.push(LineRef::Says(text));
            }
            break;
        }
    }
    let words: Vec<&str> = normalized.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        let position = if *word == "avant" && words.get(index + 1) == Some(&"derniere") {
            Some(-2)
        } else if index > 0 && words[index - 1] == "avant" && *word == "derniere" {
            continue;
        } else {
            ordinal_number(word)
        };
        let Some(position) = position else { continue };
        let Some(noun_at) = (index + 1..(index + 4).min(words.len())).find(|i| LINE_NOUNS.contains(&words[*i])) else {
            continue;
        };
        let character = match words.get(noun_at + 1) {
            Some(&("de" | "d" | "of" | "del" | "from")) => {
                let tail: Vec<&str> = words[noun_at + 2..].iter().copied().take(3).collect();
                (!tail.is_empty()).then(|| tail.join(" "))
            }
            _ => None,
        };
        refs.push(LineRef::Ordinal { position, character });
        break;
    }
    const HERE: &[&str] = &[
        "ligne ici", "replique ici", "sous le curseur", "sous la tete de lecture", "a la tete de lecture",
        "line here", "at the playhead", "under the playhead", "linea aqui",
    ];
    if HERE.iter().any(|marker| normalized.contains(&format!(" {marker}"))) {
        refs.push(LineRef::AtPlayhead);
    }
    refs
}

/// Whether the sentence points at the selected line ("de la ligne", "de cette
/// réplique", "of the selected line") rather than at a named character.
pub fn refers_to_line(query: &str) -> bool {
    const MARKERS: &[&str] = &[
        "de la ligne", "de cette ligne", "de la replique", "de cette replique", "du dialogue", "de la selection",
        "of the line", "of this line", "of the selected line", "de la linea", "de esta linea",
    ];
    let normalized = normalize(query);
    MARKERS.iter().any(|marker| normalized.contains(&format!(" {marker} ")))
}

/// The two halves of a rename: the old name as written (may be generic, like
/// "le personnage de la ligne") and the new name, from "renomme X en Y",
/// "X s'appelle Y", "rename X to Y", "renombra X a Y", or two quoted names.
pub fn rename_parts(query: &str) -> Option<(String, String)> {
    let quotes = quoted_strings(query);
    if quotes.len() >= 2 {
        return Some((quotes[0].clone(), quotes[1].clone()));
    }
    const SEPARATORS: &[&str] = &[" s'appelle ", " s’appelle ", " devient ", " becomes ", " en ", " to ", " por ", " a "];
    let lowered = query.to_lowercase();
    if lowered.len() != query.len() {
        return None;
    }
    let ranges = quoted_ranges(query);
    let clean = |text: &str| {
        text.trim()
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '«' | '»' | '“' | '”' | '.' | '!' | '?' | ','))
            .to_string()
    };
    for separator in SEPARATORS {
        let Some(index) = lowered
            .match_indices(separator)
            .map(|(index, _)| index)
            .filter(|index| !ranges.iter().any(|(start, end)| index > start && index < end))
            .last()
        else {
            continue;
        };
        let new = clean(&query[index + separator.len()..]);
        let before = &query[..index];
        let old = quotes
            .first()
            .filter(|quoted| before.contains(quoted.as_str()))
            .cloned()
            .unwrap_or_else(|| clean(before));
        if !new.is_empty() && !old.is_empty() {
            return Some((old, new));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes() {
        assert_eq!(
            quoted_strings(r#"Renomme le personnage "Muffins" en "Cup Cakes""#),
            vec!["Muffins", "Cup Cakes"]
        );
        assert_eq!(quoted_strings("dis « Salut » puis “Bye”"), vec!["Salut", "Bye"]);
        assert!(quoted_strings("rien").is_empty());
        assert_eq!(quoted_ranges(r#"a "b. c" d"#), vec![(2, 7)]);
    }

    #[test]
    fn positions_as_written() {
        assert_eq!(time_span("Va a 1 min 30").as_deref(), Some("1 min 30"));
        assert_eq!(time_span("va à 1 minute 30").as_deref(), Some("1 minute 30"));
        assert_eq!(time_span("positionne-la à 1:30.").as_deref(), Some("1:30"));
        assert_eq!(time_span("go to 00:01:30:12").as_deref(), Some("00:01:30:12"));
        assert_eq!(time_span("avance à 90s").as_deref(), Some("90s"));
        assert_eq!(time_span("va à 2 minutes et 15 secondes").as_deref(), Some("2 minutes 15 secondes"));
        assert_eq!(time_span("va à l'image 120").as_deref(), Some("image 120"));
        assert_eq!(time_span("crée une réplique piste 2"), None);
        assert_eq!(time_span("baisse le son à 30"), None);
        let frames = |q: &str| super::super::timecode::parse_frames(&time_span(q).unwrap(), 25.0);
        assert_eq!(frames("Va a 1 min 30"), Some(2250));
        assert_eq!(frames("va à 2 minutes et 15 secondes"), Some(3375));
        assert_eq!(frames("va à l'image 120"), Some(120));
    }

    #[test]
    fn track_numbers() {
        assert_eq!(track_number(r#"Créé une phrase ligne 2 qui dit "Hello World""#), Some(2));
        assert_eq!(track_number("sur la piste 3"), Some(3));
        assert_eq!(track_number("on track 4"), Some(4));
        assert_eq!(track_number("sur la deuxième piste"), Some(2));
        assert_eq!(track_number("ajoute une boucle"), None);
    }

    #[test]
    fn renames() {
        assert_eq!(
            rename_parts(r#"Renomme le personnage "Muffins" en "Cup Cakes""#),
            Some(("Muffins".into(), "Cup Cakes".into()))
        );
        assert_eq!(rename_parts("renomme Jean en Pierre").unwrap().1, "Pierre");
        assert_eq!(rename_parts("Change le nom du personnage de la ligne en Cupcake").unwrap().1, "Cupcake");
        assert_eq!(rename_parts("rename Jean to Mary Jane.").unwrap().1, "Mary Jane");
        assert!(refers_to_line("Change le nom du personnage de la ligne en Cupcake"));
        assert!(!refers_to_line("renomme Jean en Pierre"));
    }

    #[test]
    fn line_references_in_words() {
        let refs = line_references(r#"Met la TOUTE PREMIERE LIGNE en gras, Celle qui dit "Postière Muffins.""#);
        assert_eq!(refs[0], LineRef::Says("Postière Muffins.".into()));
        assert_eq!(refs[1], LineRef::Ordinal { position: 1, character: None });
        assert_eq!(
            line_references("supprime la dernière réplique de Marie")[0],
            LineRef::Ordinal { position: -1, character: Some("marie".into()) }
        );
        assert_eq!(
            line_references("supprime l'avant dernière réplique")[0],
            LineRef::Ordinal { position: -2, character: None }
        );
        assert_eq!(line_references("mets en italique la ligne qui dit bonjour")[0], LineRef::Says("bonjour".into()));
        assert_eq!(line_references("mets la ligne ici en gras")[0], LineRef::AtPlayhead);
        assert!(line_references("mets en gras").is_empty());
        assert!(line_references("ajoute une boucle").is_empty());
    }
}
