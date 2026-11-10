//! Picks the tools worth showing the engine for one query.
//!
//! Needle 3 answers well when it sees a handful of tools and poorly when it
//! sees the whole catalogue (the schemas eat its 8192-token context and the
//! model confuses neighbouring tools). Its own embeddings are weak on French
//! queries, so the choice is lexical: every tool declares trigger stems in
//! French, English and Spanish, and the tools whose stems appear in the
//! query are sent, best matches first.

use super::tools::{ToolSpec, CATALOG};

/// Most tools sent to the engine for one query. The engine chooses badly
/// among many tools, so the choice is mostly made here: the top scores only,
/// and nothing far below the best one.
pub const TOOLS_PER_QUERY: usize = 3;
/// A tool scoring below this fraction of the best score is left out.
const SCORE_MARGIN: f32 = 0.5;
/// The best tool is sent alone when it beats the second one by this factor
/// and reaches [`STRONG_SCORE`]: the words have chosen, the engine only
/// fills the arguments.
const DOMINANCE: f32 = 1.5;
/// A score meaning a distinctive trigger matched (a shared stem like
/// "ligne" scores well under 1).
pub const STRONG_SCORE: f32 = 2.0;

/// Lower-case ASCII form of a text: accents folded, punctuation and
/// apostrophes turned into spaces, single spaces, padded with one space on
/// each side so a stem can be matched at a word start.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    let mut last_space = true;
    for c in text.chars().flat_map(char::to_lowercase) {
        let folded: &str = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => "a",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'ç' => "c",
            'ñ' => "n",
            'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            'ß' => "ss",
            c if c.is_alphanumeric() => {
                for lower in c.to_lowercase() {
                    out.push(lower);
                }
                last_space = false;
                continue;
            }
            _ => "",
        };
        if folded.is_empty() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push_str(folded);
            last_space = false;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

/// How many tools of the catalogue declare each trigger: a stem shared by
/// many tools ("ligne", "export") says little about which one is wanted.
fn trigger_frequency(catalog: &[ToolSpec]) -> std::collections::HashMap<&'static str, usize> {
    let mut frequency = std::collections::HashMap::new();
    for tool in catalog {
        for trigger in tool.triggers {
            *frequency.entry(*trigger).or_insert(0) += 1;
        }
    }
    frequency
}

/// Score of one tool for a normalized query: the sum of its trigger stems
/// found at a word start, each weighed by its length and divided by the
/// number of tools sharing it. Zero when nothing matches.
pub fn score(
    tool: &ToolSpec,
    normalized_query: &str,
    frequency: &std::collections::HashMap<&'static str, usize>,
) -> f32 {
    let matched: Vec<&'static str> = tool
        .triggers
        .iter()
        .copied()
        .filter(|trigger| {
            let mut needle = String::with_capacity(trigger.len() + 1);
            needle.push(' ');
            needle.push_str(trigger);
            normalized_query.contains(&needle)
        })
        .collect();
    matched
        .iter()
        // "export", "exporte" and "exporter" are one match, not three.
        .filter(|trigger| {
            !matched
                .iter()
                .any(|other| other != *trigger && other.starts_with(trigger.trim_end()))
        })
        .map(|trigger| {
            let shared = frequency.get(trigger).copied().unwrap_or(1) as f32;
            trigger.trim().len() as f32 / (1.0 + shared)
        })
        .sum()
}

/// Every tool matching the query with its score, best first (catalogue order
/// breaks ties).
pub fn rank<'a>(query: &str, catalog: &'a [ToolSpec]) -> Vec<(f32, &'a ToolSpec)> {
    let normalized = normalize(query);
    let frequency = trigger_frequency(catalog);
    let mut scored: Vec<(f32, usize, &ToolSpec)> = catalog
        .iter()
        .enumerate()
        .map(|(index, tool)| (score(tool, &normalized, &frequency), index, tool))
        .filter(|(score, _, _)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(score, _, tool)| (score, tool)).collect()
}

/// The tools to send for a query, best first: the dominant tool alone, or
/// the top `limit` within the margin. A query matching nothing gets the
/// fallback tools (help and shortcut lookup) so the engine can still answer
/// something sensible.
pub fn select<'a>(query: &str, catalog: &'a [ToolSpec], limit: usize) -> Vec<&'a ToolSpec> {
    let ranked = rank(query, catalog);
    let best = ranked.first().map(|(score, _)| *score).unwrap_or(0.0);
    let second = ranked.get(1).map(|(score, _)| *score).unwrap_or(0.0);
    let mut picked: Vec<&ToolSpec> = if best >= STRONG_SCORE && best >= second * DOMINANCE {
        ranked.iter().take(1).map(|(_, tool)| *tool).collect()
    } else {
        ranked
            .iter()
            .take(limit)
            .filter(|(score, _)| *score >= best * SCORE_MARGIN)
            .map(|(_, tool)| *tool)
            .collect()
    };
    if picked.is_empty() {
        picked.extend(catalog.iter().filter(|tool| tool.fallback));
    }
    picked
}

/// The tool the words alone point at strongly enough to trust a call the
/// engine was unsure about, or to make the call when the engine made none.
pub fn strong_top_tool(query: &str) -> Option<&'static ToolSpec> {
    let ranked = rank(query, CATALOG);
    let (best, tool) = ranked.first()?;
    let second = ranked.get(1).map(|(score, _)| *score).unwrap_or(0.0);
    (*best >= STRONG_SCORE && *best >= second * DOMINANCE).then_some(*tool)
}

pub fn is_strong_match(query: &str, tool_name: &str) -> bool {
    strong_top_tool(query).is_some_and(|tool| tool.name == tool_name)
}

/// The sentences of a request, each answered on its own in order: "Crée une
/// réplique. Positionne-la à 1:30" is two tool calls the engine could not
/// chain by itself. Nothing inside quotes splits, a colon introduces dialogue
/// text, and a sentence that only clarifies the previous one ("Celle qui dit
/// « … »") stays attached to it.
pub fn split_clauses(query: &str) -> Vec<String> {
    const SEPARATORS: &[&str] = &[
        ". ", "! ", "; ", "\n", " puis ", " ensuite ", " et puis ", " et ensuite ", " après ça ", " then ",
        " and then ", " luego ", " y luego ", " después ",
    ];
    let quoted = super::query::quoted_ranges(query);
    let inside = |index: usize| quoted.iter().any(|(start, end)| index > *start && index < *end);
    let lowered = query.to_lowercase();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    if lowered.len() == query.len() {
        for separator in SEPARATORS {
            for (index, _) in lowered.match_indices(separator) {
                if !inside(index) {
                    cuts.push((index, index + separator.len()));
                }
            }
        }
    } else {
        for (index, _) in query.match_indices(". ") {
            if !inside(index) {
                cuts.push((index, index + 2));
            }
        }
    }
    cuts.sort();
    let mut clauses: Vec<String> = Vec::new();
    let mut from = 0;
    for (start, end) in cuts {
        if start < from {
            continue;
        }
        clauses.push(query[from..start].to_string());
        from = end;
    }
    clauses.push(query[from..].to_string());

    const CLARIFIERS: &[&str] = &[
        "celle qui", "celui qui", "celles qui", "ceux qui", "c est celle", "c est celui", "la ligne qui",
        "la replique qui", "the one that", "the one which", "the line that", "la que", "el que", "la linea que",
    ];
    let mut merged: Vec<String> = Vec::new();
    for clause in clauses {
        let clause = clause.trim().trim_end_matches('.').trim().to_string();
        if clause.is_empty() {
            continue;
        }
        let normalized = normalize(&clause);
        let clarifies = CLARIFIERS.iter().any(|marker| normalized.trim_start().starts_with(marker));
        match merged.last_mut() {
            Some(previous) if clarifies => {
                previous.push_str(", ");
                previous.push_str(&clause);
            }
            _ => merged.push(clause),
        }
    }
    merged
}

/// Shortcut for [`select`] on the application catalogue.
pub fn select_for_query(query: &str) -> Vec<&'static ToolSpec> {
    select(query, CATALOG, TOOLS_PER_QUERY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_accents_and_punctuation() {
        assert_eq!(normalize("Crée un personnage : Zoé !"), " cree un personnage zoe ");
        assert_eq!(normalize("l'export"), " l export ");
        assert_eq!(normalize("VIDÉO LÉGÈRE À"), " video legere a ");
        assert_eq!(normalize(""), " ");
    }

    fn names(tools: &[&ToolSpec]) -> Vec<&'static str> {
        tools.iter().map(|tool| tool.name).collect()
    }

    #[test]
    fn french_queries_reach_the_right_tools() {
        assert!(names(&select_for_query("ajoute une boucle")).contains(&"add_loop"));
        assert!(names(&select_for_query("va à 1 minute 30")).contains(&"seek_to_time"));
        assert!(names(&select_for_query("baisse le son à 30")).contains(&"set_volume"));
        assert!(names(&select_for_query("mets la ligne en gras")).contains(&"toggle_bold"));
        assert!(names(&select_for_query("renomme Jean en Pierre")).contains(&"rename_character"));
        assert!(names(&select_for_query("combien de répliques a Jean ?")).contains(&"count_lines"));
        assert!(names(&select_for_query("exporte en srt")).contains(&"export_subtitles"));
        assert!(names(&select_for_query("coupe le son")).contains(&"mute"));
        assert!(names(&select_for_query("j'ai oublié le raccourci pour exporter"))
            .contains(&"get_shortcut"));
    }

    #[test]
    fn english_and_spanish_queries_work_too() {
        assert!(names(&select_for_query("save the project")).contains(&"save_project"));
        assert!(names(&select_for_query("go to 2:15")).contains(&"seek_to_time"));
        assert!(names(&select_for_query("guarda el proyecto")).contains(&"save_project"));
        assert!(names(&select_for_query("siguiente fotograma")).contains(&"next_frame"));
    }

    #[test]
    fn a_dominant_tool_is_sent_alone() {
        assert_eq!(names(&select_for_query("ajoute une boucle")), vec!["add_loop"]);
        assert_eq!(names(&select_for_query("coupe le son")), vec!["mute"]);
        assert!(is_strong_match("ajoute une boucle", "add_loop"));
        assert!(!is_strong_match("ajoute une boucle", "go_to_next_loop"));
        // A stem shared by several tools proves nothing on its own.
        assert!(!is_strong_match("ajoute", "create_line"));
        assert!(!is_strong_match("ajoute", "add_loop"));
    }

    #[test]
    fn unknown_queries_fall_back_to_help() {
        let picked = select_for_query("xyzzy plugh");
        assert!(!picked.is_empty());
        assert!(picked.iter().all(|tool| tool.fallback));
    }

    #[test]
    fn verbs_and_objects_rank_the_wanted_tool_first() {
        let first = |query: &str| select_for_query(query)[0].name;
        assert_eq!(first("ajoute une boucle"), "add_loop");
        assert_eq!(first("ajoute un point out"), "add_out_point");
        assert_eq!(first("changement de plan"), "add_scene_change");
        assert_eq!(first("va à la boucle suivante"), "go_to_next_loop");
        assert_eq!(first("crée une réplique de Marie sur la piste 2 : Bonjour tout le monde"), "create_line");
        assert_eq!(first("Créé une phrase piste 2 qui dit \"Hello World\""), "create_line");
        assert_eq!(first("Positionne la au niveau de la première ligne de Chanson"), "move_lines_to_character_line");
        assert_eq!(first("positionne-la à 1:30"), "move_lines_to_time");
        assert_eq!(first("déplace la réplique sur la piste 3"), "move_lines_to_track");
        assert_eq!(first("supprime toutes les répliques de Marie"), "delete_lines");
        assert_eq!(first("mute"), "mute");
        assert_eq!(first("remets le son"), "unmute");
        assert_eq!(first("baisse le son"), "volume_down");
        assert_eq!(first("baisse le son à 30"), "set_volume");
        assert_eq!(first("lance la lecture"), "play");
        assert_eq!(first("image suivante"), "next_frame");
        assert_eq!(first("mets la ligne en gras"), "toggle_bold");
        assert_eq!(first("combien de répliques a Jean ?"), "count_lines");
        assert_eq!(first("quel est le raccourci pour exporter"), "get_shortcut");
        assert_eq!(first("ouvre le style de la bande"), "open_band_style");
        assert_eq!(first("hors champ"), "set_line_off_screen");
        assert_eq!(first("affiche les raccourcis"), "show_shortcuts");
    }

    #[test]
    fn clarifications_and_quotes_do_not_split() {
        assert_eq!(
            split_clauses("Met la TOUTE PREMIERE LIGNE en gras. Celle qui dit \"Postière Muffins.\""),
            vec!["Met la TOUTE PREMIERE LIGNE en gras, Celle qui dit \"Postière Muffins.\""]
        );
        assert_eq!(
            split_clauses("crée une réplique qui dit \"Stop. Puis pars.\" puis ajoute une boucle"),
            vec!["crée une réplique qui dit \"Stop. Puis pars.\"", "ajoute une boucle"]
        );
    }

    #[test]
    fn splits_requests_into_sentences() {
        assert_eq!(
            split_clauses("Crée une phrase piste 2 qui dit \"Hello World\". Positionne-la à 1:30."),
            vec!["Crée une phrase piste 2 qui dit \"Hello World\"", "Positionne-la à 1:30"]
        );
        assert_eq!(
            split_clauses("va à 1:30 puis ajoute une boucle"),
            vec!["va à 1:30", "ajoute une boucle"]
        );
        assert_eq!(
            split_clauses("crée une réplique de Marie : Bonjour. Ça va"),
            vec!["crée une réplique de Marie : Bonjour", "Ça va"]
        );
        assert_eq!(split_clauses("  "), Vec::<String>::new());
    }

    #[test]
    fn never_exceeds_the_limit() {
        assert!(select_for_query("ouvre exporte sauvegarde ligne personnage boucle volume").len() <= TOOLS_PER_QUERY);
    }
}

#[cfg(test)]
mod debug_scores {
    #[test]
    #[ignore]
    fn print_scores() {
        let query = std::env::var("NEEDLE_QUERY").unwrap_or_default();
        let normalized = super::normalize(&query);
        let frequency = super::trigger_frequency(super::CATALOG);
        let mut rows: Vec<(f32, &str, Vec<&str>)> = super::CATALOG
            .iter()
            .map(|tool| {
                let matched: Vec<&str> = tool.triggers.iter().copied().filter(|t| normalized.contains(&format!(" {t}"))).collect();
                (super::score(tool, &normalized, &frequency), tool.name, matched)
            })
            .filter(|(s, _, _)| *s > 0.0)
            .collect();
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for row in rows.iter().take(8) {
            println!("{:.2} {} {:?}", row.0, row.1, row.2);
        }
    }
}
