//! Turns a tool call of the engine into the actions the application runs.
//!
//! A plan is built against the current state (selection, playhead,
//! characters, bands) and then dispatched by the event loop through the
//! `CommandDispatcher`, exactly like a shortcut: the guards, the undo
//! history, the collaboration sync and the accessibility announcements of
//! the manual action all apply. Information tools produce a message and no
//! action.

use super::needle::ToolCall;
use super::{colors, query, retrieval, timecode, tools};
use crate::application::command::{ToolMode, UiAction};
use crate::i18n::t;
use crate::rythmo_line::{LinePresence, MarkerKind, RythmoLineKind, TextEmotion, TextStyleKind};
use crate::state::State;

/// What one tool call does: actions to dispatch and a message for the bubble.
#[derive(Debug, Default)]
pub struct Plan {
    pub actions: Vec<UiAction>,
    pub message: Option<String>,
}

impl Plan {
    fn run(actions: Vec<UiAction>) -> Self {
        Self {
            actions,
            message: None,
        }
    }

    fn say(message: String) -> Self {
        Self {
            actions: Vec::new(),
            message: Some(message),
        }
    }

    fn run_and_say(actions: Vec<UiAction>, message: String) -> Self {
        Self {
            actions,
            message: Some(message),
        }
    }
}

/// Localized name of a tool, for messages.
pub fn label(tool_name: &str) -> String {
    let key = format!("assistant.tool.{tool_name}");
    let label = t(&key);
    if label == key {
        tool_name.replace('_', " ")
    } else {
        label.to_string()
    }
}

fn arg<'a>(call: &'a ToolCall, name: &str) -> Option<&'a serde_json::Value> {
    call.arguments.get(name).filter(|value| !value.is_null())
}

fn arg_text(call: &ToolCall, name: &str) -> Option<String> {
    let value = arg(call, name)?;
    let text = match value {
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    };
    (!text.is_empty()).then_some(text)
}

fn arg_number(call: &ToolCall, name: &str) -> Option<f64> {
    match arg(call, name)? {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => parse_number(text),
        _ => None,
    }
}

fn arg_int(call: &ToolCall, name: &str) -> Option<i64> {
    arg_number(call, name).map(|value| value.round() as i64)
}

/// A number said with a unit or a sign: `30`, `30 %`, `-2,5`, `x1.5`.
pub fn parse_number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .trim()
        .replace(',', ".")
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    cleaned.parse::<f64>().ok()
}

pub fn parse_bool(text: &str) -> Option<bool> {
    let word = retrieval::normalize(text);
    let word = word.trim();
    const YES: &[&str] = &["true", "oui", "yes", "on", "1", "si", "vrai", "active", "activer", "activee", "affiche", "afficher", "montre", "show", "enable", "enabled", "visible"];
    const NO: &[&str] = &["false", "non", "no", "off", "0", "faux", "desactive", "desactiver", "desactivee", "masque", "masquer", "cache", "cacher", "hide", "disable", "disabled", "invisible", "retire"];
    if YES.contains(&word) {
        Some(true)
    } else if NO.contains(&word) {
        Some(false)
    } else {
        None
    }
}

fn missing(name: &str) -> String {
    t("assistant.missing_argument").replace("{name}", name)
}

fn bad_time(value: &str) -> String {
    t("assistant.bad_time").replace("{value}", value)
}

/// Frames for a `time`-like argument, or an error message.
fn frames_arg(state: &State, call: &ToolCall, name: &str) -> Result<Option<i64>, String> {
    match arg_text(call, name) {
        None => Ok(None),
        Some(text) => timecode::parse_frames(&text, state.fps())
            .map(Some)
            .ok_or_else(|| bad_time(&text)),
    }
}

/// The lines a tool acts on: those the sentence names ("la première ligne",
/// "celle qui dit …") when it names some, else the selection.
fn selected_lines(state: &State, targets: &Option<Vec<u64>>) -> Result<Vec<u64>, String> {
    let ids = targets.clone().unwrap_or_else(|| state.selected_line_ids());
    if ids.is_empty() {
        Err(t("assistant.no_selection").to_string())
    } else {
        Ok(ids)
    }
}

fn selected_line(state: &State, targets: &Option<Vec<u64>>) -> Result<u64, String> {
    selected_lines(state, targets).map(|ids| ids[0])
}

/// Y slot of a 1-based track number, clamped to the band.
fn y_slot_for_track(track: i64) -> f32 {
    let last = crate::rythmo_layout::track_count().saturating_sub(1) as i64;
    crate::rythmo_layout::y_slot_for_track_index(track.saturating_sub(1).clamp(0, last) as usize)
}

/// Known character matching a name said by the user: exact, then prefix,
/// then contained, accents and case ignored.
pub fn resolve_character(state: &State, name: &str) -> Option<(String, [f32; 4])> {
    let project = &state.project_session.project;
    let mut candidates: Vec<(String, [f32; 4])> = project
        .known_characters()
        .iter()
        .map(|character| (character.name.clone(), character.color))
        .collect();
    for line in project.lines() {
        let name = line.character_name.trim();
        if !name.is_empty() && !candidates.iter().any(|(known, _)| known == name) {
            candidates.push((name.to_string(), line.character_color));
        }
    }
    let wanted = retrieval::normalize(name);
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return None;
    }
    let normalized: Vec<String> = candidates
        .iter()
        .map(|(known, _)| retrieval::normalize(known).trim().to_string())
        .collect();
    let index = normalized
        .iter()
        .position(|known| known == wanted)
        .or_else(|| normalized.iter().position(|known| known.starts_with(wanted)))
        .or_else(|| normalized.iter().position(|known| known.contains(wanted)))?;
    Some(candidates[index].clone())
}

fn character_or_new(state: &State, name: &str) -> (String, [f32; 4]) {
    resolve_character(state, name)
        .unwrap_or_else(|| (name.trim().to_string(), colors::color_for_new_character(name.trim())))
}

fn known_character(state: &State, name: &str) -> Result<(String, [f32; 4]), String> {
    resolve_character(state, name)
        .ok_or_else(|| t("assistant.unknown_character").replace("{name}", name))
}

/// Ways a language is named, so "passe en anglais" finds the band "EN".
fn language_aliases(word: &str) -> &'static [&'static str] {
    const GROUPS: &[&[&str]] = &[
        &["anglais", "anglaise", "english", "ingles", "inglesa", "en", "eng", "uk", "us", "va"],
        &["francais", "francaise", "french", "frances", "francesa", "fr", "fra", "vf"],
        &["espagnol", "espagnole", "spanish", "espanol", "espanola", "castellano", "es", "esp", "spa"],
        &["allemand", "allemande", "german", "aleman", "alemana", "deutsch", "de", "ger", "deu"],
        &["italien", "italienne", "italian", "italiano", "italiana", "it", "ita"],
        &["portugais", "portugaise", "portuguese", "portugues", "pt", "por", "br"],
        &["japonais", "japonaise", "japanese", "japones", "ja", "jp", "jpn"],
        &["chinois", "chinoise", "chinese", "chino", "zh", "cn", "mandarin"],
        &["arabe", "arabic", "ar", "ara"],
        &["russe", "russian", "ruso", "ru", "rus"],
        &["neerlandais", "dutch", "nl", "nld", "hollandais"],
    ];
    GROUPS
        .iter()
        .find(|group| group.contains(&word))
        .copied()
        .unwrap_or(&[])
}

/// The band named in a text: by its exact name, a prefix, a word of the
/// text, or a language alias of that word.
/// A project video (never a proxy) by its name, as the user wrote it.
fn video_by_name(state: &State, name: &str) -> Result<crate::project::MediaId, String> {
    let wanted = retrieval::normalize(name);
    let wanted = wanted.trim();
    let library = state.project_session.project.media_library();
    let videos: Vec<_> = library
        .videos
        .iter()
        .filter(|video| video.proxy_of.is_none())
        .collect();
    let normalized: Vec<String> = videos
        .iter()
        .map(|video| retrieval::normalize(&video.name).trim().to_string())
        .collect();
    normalized
        .iter()
        .position(|known| known == wanted)
        .or_else(|| normalized.iter().position(|known| known.starts_with(wanted)))
        .or_else(|| normalized.iter().position(|known| known.contains(wanted)))
        .or_else(|| {
            normalized
                .iter()
                .position(|known| wanted.split_whitespace().any(|word| word.len() >= 3 && known.contains(word)))
        })
        .map(|index| videos[index].id)
        .ok_or_else(|| t("assistant.info.video_unknown").replace("{name}", name))
}

/// The video a conformation starts from when none is named: the loaded one,
/// else the default, else the first of the library.
fn default_conform_video(state: &State) -> Result<crate::project::MediaId, String> {
    let library = state.project_session.project.media_library();
    let loaded = state.video_path().and_then(|path| {
        library
            .videos
            .iter()
            .find(|video| {
                video.proxy_of.is_none()
                    && crate::video_proxy::paths_match(std::path::Path::new(&video.path), &path)
            })
            .map(|video| video.id)
    });
    loaded
        .or(library
            .default_video
            .filter(|id| library.videos.iter().any(|v| v.id == *id && v.proxy_of.is_none())))
        .or_else(|| {
            library
                .videos
                .iter()
                .find(|video| video.proxy_of.is_none())
                .map(|video| video.id)
        })
        .ok_or_else(|| t("conform.no_video").to_string())
}

fn band_by_name(state: &State, name: &str) -> Result<crate::project::ProjectLanguage, String> {
    let wanted = retrieval::normalize(name);
    let wanted = wanted.trim();
    let bands = state.project_session.project.languages();
    let normalized: Vec<String> = bands
        .iter()
        .map(|band| retrieval::normalize(&band.name).trim().to_string())
        .collect();
    let by_word = || {
        wanted.split_whitespace().find_map(|word| {
            let aliases = language_aliases(word);
            normalized.iter().position(|known| {
                known == word
                    || aliases.contains(&known.as_str())
                    || (word.len() >= 3 && known.contains(word))
                    || aliases.iter().any(|alias| alias.len() >= 3 && known.contains(alias))
            })
        })
    };
    normalized
        .iter()
        .position(|known| known == wanted)
        .or_else(|| normalized.iter().position(|known| known.starts_with(wanted)))
        .or_else(|| normalized.iter().position(|known| known.contains(wanted)))
        .or_else(by_word)
        .map(|index| bands[index].clone())
        .ok_or_else(|| t("assistant.unknown_band").replace("{name}", name))
}

/// Lines matching the `character`, `text` and `track` filters of a call.
fn filter_lines(state: &State, call: &ToolCall) -> Result<Vec<u64>, String> {
    let character = arg_text(call, "character").map(|name| known_character(state, &name)).transpose()?;
    let text = arg_text(call, "text").map(|text| retrieval::normalize(&text).trim().to_string());
    let track = arg_int(call, "track");
    if character.is_none() && text.is_none() && track.is_none() {
        return Err(missing("character / text / track"));
    }
    let ids: Vec<u64> = state
        .project_session
        .project
        .lines()
        .filter(|line| {
            character
                .as_ref()
                .map_or(true, |(name, _)| line.character_name == *name)
                && text
                    .as_ref()
                    .map_or(true, |text| retrieval::normalize(&line.text).contains(text.as_str()))
                && track.map_or(true, |track| {
                    (line.y_slot - y_slot_for_track(track)).abs() < 0.01
                })
        })
        .map(|line| line.id)
        .collect();
    if ids.is_empty() {
        Err(t("assistant.no_lines_match").to_string())
    } else {
        Ok(ids)
    }
}

fn parse_emotion(text: &str) -> Option<TextEmotion> {
    let normalized = retrieval::normalize(text);
    let words = normalized.trim();
    let from_name = |name: &str| serde_json::from_value::<TextEmotion>(serde_json::Value::String(name.to_string())).ok();
    if let Some(emotion) = from_name(&words.replace(' ', "_")) {
        return Some(emotion);
    }
    let has = |stems: &[&str]| stems.iter().any(|stem| normalized.contains(&format!(" {stem}")));
    let intensity = if has(&["extreme", "enorme", "huge", "maximum", "max", "fou", "folle", "hysteri"]) {
        3
    } else if has(&["fort", "strong", "tres", "grand", "intense", "beaucoup", "fuerte", "big", "deep", "profond"]) {
        2
    } else if has(&["doux", "douce", "leger", "legere", "soft", "un peu", "light", "suave", "faible", "petit", "slight"]) {
        0
    } else {
        1
    };
    let family: &[&str] = if has(&["colere", "anger", "angry", "enerve", "rage", "ira", "furieux", "furious", "mad"]) {
        &["anger_soft", "anger_heavy", "anger_heavy", "anger_extreme"]
    } else if has(&["joie", "joy", "happy", "content", "heureux", "alegr", "ravi", "glad"]) {
        &["joy_soft", "joy_strong", "joy_burst", "joy_extreme"]
    } else if has(&["peur", "fear", "panique", "panic", "miedo", "effray", "scared", "terreur", "terror"]) {
        &["fear_soft", "fear_strong", "fear_panic", "fear_extreme"]
    } else if has(&["triste", "sad", "tristesse", "chagrin", "pleure", "cry", "deuil"]) {
        &["sadness_soft", "sadness_strong", "sadness_deep", "sadness_extreme"]
    } else if has(&["amour", "love", "tendre", "tenderness", "tendresse", "affection", "carino", "doux"]) {
        &["tenderness_soft", "love_tender", "tenderness_strong", "tenderness_extreme"]
    } else if has(&["degout", "disgust", "asco", "beurk", "ecoeur", "yuck"]) {
        &["disgust_soft", "disgust", "disgust_strong", "disgust_extreme"]
    } else if has(&["doute", "doubt", "duda", "hesit", "incertain", "unsure"]) {
        &["doubt_soft", "doubt", "doubt_strong", "doubt_extreme"]
    } else if has(&["question", "interrog", "pregunta", "demande"]) {
        if has(&["rapide", "fast", "vite", "quick"]) {
            &["question_fast", "question_fast", "question_fast", "question_fast"]
        } else {
            &["question_soft", "question", "question_strong", "question_extreme"]
        }
    } else if has(&["exclam", "crie", "cri", "shout", "scream", "hurle", "yell", "grito"]) {
        &["exclamation_soft", "exclamation", "exclamation_strong", "exclamation_huge"]
    } else if has(&["tremble", "shake", "trembl", "tiembla", "vibre"]) {
        &["shake", "shake", "shake", "shake"]
    } else if has(&["vague", "wave", "ola", "ondul"]) {
        &["wave", "wave", "wave", "wave"]
    } else if has(&["rebond", "bounce", "rebota", "saut"]) {
        &["bounce", "bounce", "bounce", "bounce"]
    } else if has(&["balanc", "swing", "columpi"]) {
        &["swing", "swing", "swing", "swing"]
    } else if has(&["pendul", "pendulum"]) {
        &["pendulum", "pendulum", "pendulum", "pendulum"]
    } else if has(&["gliss", "slide", "desliz"]) {
        &["slide", "slide", "slide", "slide"]
    } else if has(&["oscill"]) {
        &["oscillation", "oscillation", "oscillation", "oscillation"]
    } else if has(&["gigot", "wiggle", "remue", "frétill", "fretill"]) {
        &["wiggle", "wiggle", "wiggle", "wiggle"]
    } else if has(&["youpi", "yay", "hourra", "hooray", "viva"]) {
        &["yay", "yay", "yay", "yay"]
    } else {
        return None;
    };
    from_name(family[intensity]).or_else(|| from_name(family[1]))
}

fn breath_text(kind: Option<String>) -> String {
    match kind.map(|k| k.to_ascii_lowercase()).as_deref() {
        Some("hh") => "(HH)",
        Some("mh") => "(mH)",
        Some("mhh") => "(mHH)",
        Some("up") | Some("haut") => "↑",
        Some("down") | Some("bas") => "↓",
        _ => "(H)",
    }
    .to_string()
}

fn reaction_text(kind: Option<String>) -> String {
    const KINDS: &[&str] = &["x", "mts", "tsc", "ah", "oh", "ih", "mhm", "hm", "ptt", "pff", "unh", "hun", "psst"];
    let kind = kind.map(|k| k.trim().trim_matches(|c| c == '(' || c == ')').to_ascii_lowercase());
    match kind.as_deref() {
        Some(kind) if KINDS.contains(&kind) => {
            if kind == "x" {
                "(X)".to_string()
            } else {
                format!("({kind})")
            }
        }
        _ => "(X)".to_string(),
    }
}

/// One project setting changed by a tool: the toggles flip the current
/// value, the others take the `value` argument.
fn project_settings_action(state: &State, tool_name: &str, call: &ToolCall) -> Result<UiAction, String> {
    let settings = state.project_session.project.settings().clone();
    let rythmo_font = crate::config::get().ui.rythmo_font.clone();
    let value = || arg_text(call, "value").ok_or_else(|| missing("value"));
    let number = || {
        let value = value()?;
        parse_number(&value).ok_or_else(|| t("assistant.bad_value").replace("{value}", &value))
    };
    let mut scroll_speed = settings.scroll_speed;
    let mut reading_bar_offset_percent = settings.reading_bar_offset_percent;
    let mut highlight_read_word = settings.highlight_read_word;
    let mut scrolling_text_uses_character_color = settings.scrolling_text_uses_character_color;
    let mut show_text_emotion_lanes = settings.show_text_emotion_lanes;
    let mut show_timecode = settings.show_timecode;
    let mut start_timecode = settings.start_timecode.clone();
    let mut waveform_height = settings.waveform_height;
    match tool_name {
        "set_scroll_speed" => scroll_speed = number()? as f32,
        "set_reading_bar_offset" => reading_bar_offset_percent = number()? as f32,
        "toggle_highlight_read_word" => highlight_read_word = !highlight_read_word,
        "toggle_character_color_text" => {
            scrolling_text_uses_character_color = !scrolling_text_uses_character_color
        }
        "toggle_emotion_lanes" => show_text_emotion_lanes = !show_text_emotion_lanes,
        "toggle_timecode_display" => show_timecode = !show_timecode,
        "set_start_timecode" => {
            let value = value()?;
            let frames = timecode::parse_frames(&value, state.fps()).ok_or_else(|| bad_time(&value))?;
            start_timecode = timecode::format_timecode(frames, state.fps());
        }
        "set_waveform_height" => waveform_height = number()? as f32,
        other => return Err(t("assistant.unknown_tool").replace("{tool}", other)),
    }
    Ok(UiAction::SaveProjectSettings {
        rythmo_font,
        scroll_speed,
        reading_bar_offset_percent,
        instrumental_audio_path: settings.instrumental_audio_path.clone(),
        highlight_read_word,
        scrolling_text_uses_character_color,
        show_text_emotion_lanes,
        show_timecode,
        start_timecode,
        waveform_height,
    })
}

fn line_summary(state: &State, line: &crate::rythmo_line::RythmoLine) -> String {
    let fps = state.fps();
    let character = if line.character_name.trim().is_empty() {
        "—".to_string()
    } else {
        line.character_name.clone()
    };
    t("assistant.info.selected_line")
        .replace("{character}", &character)
        .replace("{text}", line.text.trim())
        .replace("{start}", &timecode::format_timecode(line.start_frame, fps))
        .replace(
            "{end}",
            &timecode::format_timecode(line.start_frame + line.duration_frames, fps),
        )
}

/// Drops the arguments the engine flagged as not coming from the query
/// (`ungrounded` paths `tool.argument`), optional ones only: the model
/// invents a time or a count the user never said, the rest of the call is
/// fine. Returns the call to run, or `None` when a required argument was
/// invented.
pub fn without_ungrounded(call: &ToolCall, ungrounded: &[String]) -> Option<ToolCall> {
    let spec = tools::find(&call.name)?;
    let mut call = call.clone();
    let prefix = format!("{}.", call.name);
    for path in ungrounded {
        let Some(argument) = path.strip_prefix(&prefix) else {
            continue;
        };
        let argument = argument.split(['.', '[']).next().unwrap_or(argument);
        let Some(param) = spec.param(argument) else {
            continue;
        };
        if param.required {
            // A text argument is "ungrounded" only when it was invented
            // too; the engine checks numbers and dates, so trust strings.
            if !matches!(param.kind, tools::ParamKind::Text) {
                return None;
            }
            continue;
        }
        if let Some(object) = call.arguments.as_object_mut() {
            object.remove(argument);
        }
    }
    Some(call)
}

/// Text the user quoted ("…", «…», '…'), or wrote after a colon.
pub fn quoted_text(query: &str) -> Option<String> {
    for (open, close) in [('"', '"'), ('«', '»'), ('“', '”'), ('\'', '\'')] {
        if let Some(start) = query.find(open) {
            let after = start + open.len_utf8();
            if let Some(len) = query[after..].find(close) {
                let text = query[after..after + len].trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }
    let colon = query.rfind(':')?;
    let text = query[colon + 1..].trim();
    // "va à 1:30" is a timecode, not a text.
    (!text.is_empty() && !text.chars().all(|c| c.is_ascii_digit() || c == ':')).then(|| text.to_string())
}

/// The known character named in the query, the longest name first.
pub fn character_in_query(state: &State, query: &str) -> Option<(String, [f32; 4])> {
    let normalized = retrieval::normalize(query);
    let project = &state.project_session.project;
    let mut candidates: Vec<(String, [f32; 4])> = project
        .known_characters()
        .iter()
        .map(|character| (character.name.clone(), character.color))
        .collect();
    for line in project.lines() {
        let name = line.character_name.trim();
        if !name.is_empty() && !candidates.iter().any(|(known, _)| known == name) {
            candidates.push((name.to_string(), line.character_color));
        }
    }
    candidates.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    candidates.into_iter().find(|(name, _)| {
        let needle = retrieval::normalize(name);
        needle.trim().len() >= 2 && normalized.contains(needle.as_str())
    })
}

/// Completes what the model missed or garbled from the query itself: the
/// quoted text, the character named, the two names of a rename. The model
/// is small; the words are right there.
pub fn fill_from_query(state: &State, spec: &tools::ToolSpec, call: &ToolCall, query: &str) -> ToolCall {
    let mut call = call.clone();
    if !call.arguments.is_object() {
        call.arguments = serde_json::json!({});
    }
    let text_params = ["text", "note", "feature"];
    for name in text_params {
        if spec.param(name).is_some() && arg_text(&call, name).is_none() {
            let value = if name == "feature" {
                Some(query.trim().to_string())
            } else {
                quoted_text(query)
            };
            if let Some(value) = value {
                call.arguments[name] = serde_json::Value::String(value);
            }
        }
    }
    if spec.param("character").is_some() {
        let current = arg_text(&call, "character");
        let text = arg_text(&call, "text");
        let garbled = current.is_none()
            || current == text
            || current
                .as_deref()
                .is_some_and(|name| resolve_character(state, name).is_none());
        if garbled {
            match character_in_query(state, query) {
                Some((name, _)) => call.arguments["character"] = serde_json::Value::String(name),
                None if current == text && text.is_some() => {
                    if let Some(object) = call.arguments.as_object_mut() {
                        object.remove("character");
                    }
                }
                None => {}
            }
        }
    }
    if spec.name == "rename_character" {
        // The sentence decides: the model often copies the old name into
        // the new one, or cuts "Cup Cakes" to "Cup".
        if let Some((old, new)) = query::rename_parts(query) {
            let old = character_in_query(state, &old)
                .map(|(name, _)| name)
                .unwrap_or(old);
            call.arguments["old_name"] = serde_json::Value::String(old);
            call.arguments["new_name"] = serde_json::Value::String(new);
        }
    }
    // A position is read as written ("1 min 30"): the model converts it,
    // sometimes wrongly ("130 seconds").
    let time_param = if spec.name == "set_start_timecode" { "value" } else { "time" };
    if spec.param(time_param).is_some() {
        if let Some(span) = query::time_span(query) {
            call.arguments[time_param] = serde_json::Value::String(span);
        }
    }
    if matches!(spec.name, "create_line" | "move_lines_to_track") {
        if let Some(track) = query::track_number(query) {
            call.arguments["track"] = serde_json::json!(track);
        }
    }
    call
}

/// Tools acting on the selected lines, which can instead act on lines the
/// sentence names ("mets la première ligne en gras").
const LINE_TARGET_TOOLS: &[&str] = &[
    "set_line_text", "edit_line_text", "edit_line_sections", "set_line_character", "edit_line_character",
    "delete_selected_lines", "copy_lines", "cut_lines", "move_lines_to_track", "shift_lines",
    "set_line_start", "set_line_end", "set_line_duration", "split_line", "toggle_karaoke",
    "toggle_bold", "toggle_italic", "toggle_underline", "toggle_strikethrough", "set_text_emotion",
    "clear_text_emotion", "open_emotion_menu", "unlink_lines", "set_line_note", "add_note",
    "set_line_on_screen", "set_line_off_screen", "set_line_back", "generate_detection_signs",
    "assign_voice_actor_to_line", "assign_voice_actor_to_character", "get_selected_line", "add_sync_point",
    "rename_character",
];

/// Lines named in the sentence, the most specific reference first, or
/// `None` when it names none (the tool then acts on the selection).
pub fn lines_named_in(state: &State, query: &str) -> Option<Vec<u64>> {
    let project = &state.project_session.project;
    let mut dialogue: Vec<&crate::rythmo_line::RythmoLine> = project
        .lines()
        .filter(|line| line.kind == RythmoLineKind::Dialogue)
        .collect();
    dialogue.sort_by_key(|line| (line.start_frame, (line.y_slot * 100.0) as i64));
    for reference in query::line_references(query) {
        let ids: Vec<u64> = match &reference {
            query::LineRef::Says(text) => {
                let wanted = retrieval::normalize(text);
                let wanted = wanted.trim();
                if wanted.is_empty() {
                    continue;
                }
                dialogue
                    .iter()
                    .filter(|line| retrieval::normalize(&line.text).contains(wanted))
                    .map(|line| line.id)
                    .collect()
            }
            query::LineRef::Ordinal { position, character } => {
                let character = character.as_deref().and_then(|name| {
                    // "de Marie en gras": the longest known name wins.
                    let words: Vec<&str> = name.split_whitespace().collect();
                    (1..=words.len())
                        .rev()
                        .find_map(|n| resolve_character(state, &words[..n].join(" ")))
                        .map(|(name, _)| name)
                });
                let lines: Vec<u64> = dialogue
                    .iter()
                    .filter(|line| character.as_ref().map_or(true, |name| line.character_name == *name))
                    .map(|line| line.id)
                    .collect();
                let index = if *position < 0 {
                    lines.len() as i64 + position
                } else {
                    position - 1
                };
                if index < 0 || index as usize >= lines.len() {
                    continue;
                }
                vec![lines[index as usize]]
            }
            query::LineRef::AtPlayhead => {
                let frame = state.current_frame();
                dialogue
                    .iter()
                    .filter(|line| line.start_frame <= frame && frame < line.start_frame + line.duration_frames)
                    .map(|line| line.id)
                    .take(1)
                    .collect()
            }
        };
        if !ids.is_empty() {
            return Some(ids);
        }
    }
    None
}

/// The plan for one call, or the message explaining why it cannot run.
pub fn plan(state: &State, call: &ToolCall, query: &str) -> Result<Plan, String> {
    let Some(spec) = tools::find(&call.name) else {
        return Err(t("assistant.unknown_tool").replace("{tool}", &call.name));
    };
    let filled = fill_from_query(state, spec, call, query);
    let call = &filled;
    let targets = if LINE_TARGET_TOOLS.contains(&spec.name) {
        lines_named_in(state, query)
    } else {
        None
    };
    let project = &state.project_session.project;
    let fps = state.fps();
    let current = state.current_frame();
    let seek = |frame: i64| vec![UiAction::SeekAbsolute(frame.max(0)), UiAction::FinishSeek];
    let plan = match spec.name {
        // ---- Playback ---------------------------------------------------
        "play" => {
            if state.is_video_playing() {
                Plan::say(label("play"))
            } else {
                Plan::run(vec![UiAction::TogglePlayPause])
            }
        }
        "pause" => {
            if state.is_video_playing() {
                Plan::run(vec![UiAction::TogglePlayPause])
            } else {
                Plan::say(label("pause"))
            }
        }
        "play_reverse" => Plan::run(vec![UiAction::ToggleReversePlayback]),
        "seek_to_time" => {
            let frame = frames_arg(state, call, "time")?.ok_or_else(|| missing("time"))?;
            Plan::run_and_say(
                seek(frame),
                t("assistant.info.playhead_moved")
                    .replace("{tc}", &timecode::format_timecode(frame, fps)),
            )
        }
        "seek_relative" => {
            let seconds = arg_number(call, "seconds").ok_or_else(|| missing("seconds"))?;
            let frame = current + (seconds * fps).round() as i64;
            Plan::run_and_say(
                seek(frame),
                t("assistant.info.playhead_moved")
                    .replace("{tc}", &timecode::format_timecode(frame.max(0), fps)),
            )
        }
        "next_frame" => Plan::run(vec![UiAction::NextFrame]),
        "previous_frame" => Plan::run(vec![UiAction::PrevFrame]),
        "step_frames" => {
            let count = arg_int(call, "count").ok_or_else(|| missing("count"))?;
            Plan::run(seek(current + count))
        }
        "go_to_start" => Plan::run(seek(0)),
        "go_to_end" => Plan::run(seek(state.total_frames().saturating_sub(1))),
        "go_to_next_loop" => Plan::run(vec![UiAction::SeekToNextBoucle { direction: 1 }]),
        "go_to_previous_loop" => Plan::run(vec![UiAction::SeekToNextBoucle { direction: -1 }]),
        "set_volume" => {
            let percent = arg_number(call, "percent").ok_or_else(|| missing("percent"))?;
            let percent = if (0.0..=1.0).contains(&percent) && percent != 1.0 && percent != 0.0 {
                percent * 100.0
            } else {
                percent
            };
            Plan::run(vec![UiAction::SetVolume((percent / 100.0).clamp(0.0, 1.0) as f32)])
        }
        "volume_up" => Plan::run(vec![UiAction::AdjustVolume(0.1)]),
        "volume_down" => Plan::run(vec![UiAction::AdjustVolume(-0.1)]),
        "mute" | "unmute" => {
            let is_muted = state.ui_shell.ui.volume() <= 0.001;
            if (spec.name == "mute") == is_muted {
                Plan::say(label(spec.name))
            } else {
                Plan::run(vec![UiAction::ToggleMute])
            }
        }
        "switch_audio" => Plan::run(vec![UiAction::ToggleActiveAudio]),
        "offset_audio" => {
            let milliseconds = arg_number(call, "milliseconds").ok_or_else(|| missing("milliseconds"))?;
            Plan::run(vec![UiAction::OffsetActiveAudioBy((milliseconds / 1000.0 * fps).round() as i64)])
        }
        "open_secondary_display" => Plan::run(vec![UiAction::OpenSecondaryDisplay]),
        "close_secondary_display" => Plan::run(vec![UiAction::CloseSecondaryDisplay]),
        // ---- Selection --------------------------------------------------
        "select_line_at_playhead" => Plan::run(vec![UiAction::SelectLineAtPlayhead]),
        "select_next_line" => Plan::run(vec![UiAction::NavigateLines { direction: 1 }]),
        "select_previous_line" => Plan::run(vec![UiAction::NavigateLines { direction: -1 }]),
        "select_all" => Plan::run(vec![UiAction::SelectAll]),
        "select_lines" => {
            let line_ids = filter_lines(state, call)?;
            let count = line_ids.len();
            Plan::run_and_say(
                vec![UiAction::SelectLines { line_ids }],
                t("assistant.info.lines_selected").replace("{count}", &count.to_string()),
            )
        }
        "clear_selection" => Plan::run(vec![UiAction::ClearLineSelection]),
        // ---- Lines ------------------------------------------------------
        "create_line" => {
            let y_slot = match arg_int(call, "track") {
                Some(track) => y_slot_for_track(track),
                None => crate::rythmo_layout::y_slot_for_track_index(
                    state.ui_shell.ui.rythmo_state.keyboard_track,
                ),
            };
            let text = arg_text(call, "text").unwrap_or_default();
            let character = arg_text(call, "character").map(|name| character_or_new(state, &name));
            Plan::run(vec![UiAction::AssistantCreateLine {
                frame: current,
                y_slot,
                duration_frames: None,
                text,
                character,
            }])
        }
        "set_line_text" => {
            let id = selected_line(state, &targets)?;
            let text = arg_text(call, "text").ok_or_else(|| missing("text"))?;
            Plan::run(vec![UiAction::UpdateLineText { id, text, edit: None }])
        }
        "edit_line_text" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::StartEditingSelectedLine])
        }
        "edit_line_sections" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::StartEditingSelectedLineSections])
        }
        "set_line_character" => {
            let line_ids = selected_lines(state, &targets)?;
            let name = arg_text(call, "character").ok_or_else(|| missing("character"))?;
            let (name, color) = character_or_new(state, &name);
            Plan::run(vec![UiAction::SetLinesRole { line_ids, name, color }])
        }
        "edit_line_character" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::StartEditingSelectedCharacter])
        }
        "delete_selected_lines" => {
            let line_ids = selected_lines(state, &targets)?;
            Plan::run(vec![UiAction::DeleteSidePanelLines { line_ids }])
        }
        "delete_lines" => {
            let line_ids = filter_lines(state, call)?;
            let count = line_ids.len();
            Plan::run_and_say(
                vec![UiAction::DeleteSidePanelLines { line_ids }],
                t("assistant.info.lines_deleted").replace("{count}", &count.to_string()),
            )
        }
        "copy_lines" => {
            selected_lines(state, &targets)?;
            Plan::run(vec![UiAction::CopySelectedLine])
        }
        "cut_lines" => {
            selected_lines(state, &targets)?;
            Plan::run(vec![UiAction::CutSelectedLine])
        }
        "paste_lines" => Plan::run(vec![UiAction::PasteLine]),
        "paste_lines_with_track_character" => Plan::run(vec![UiAction::PasteLineWithTrackCharacter]),
        "move_lines_to_track" => {
            let ids = selected_lines(state, &targets)?;
            let track = arg_int(call, "track").ok_or_else(|| missing("track"))?;
            let y_slot = y_slot_for_track(track);
            let moves: Vec<(u64, i64, f32)> = ids
                .iter()
                .filter_map(|id| project.get_line(*id))
                .map(|line| (line.id, line.start_frame, y_slot))
                .collect();
            Plan::run(vec![UiAction::MoveLines { moves }])
        }
        "move_lines_to_time" | "move_lines_to_character_line" => {
            let ids = selected_lines(state, &targets)?;
            let target = match frames_arg(state, call, "time")? {
                Some(frame) if spec.name == "move_lines_to_time" => frame,
                _ => {
                    let name = arg_text(call, "character").ok_or_else(|| missing("time / character"))?;
                    let (character, _) = known_character(state, &name)?;
                    let mut starts: Vec<i64> = project
                        .lines()
                        .filter(|line| line.character_name == character && !ids.contains(&line.id))
                        .map(|line| line.start_frame)
                        .collect();
                    starts.sort_unstable();
                    if starts.is_empty() {
                        return Err(t("assistant.no_lines_match").to_string());
                    }
                    let index = arg_int(call, "line_index").unwrap_or(1);
                    let position = if index < 0 {
                        starts.len() as i64 + index
                    } else {
                        index.max(1) - 1
                    };
                    starts[position.clamp(0, starts.len() as i64 - 1) as usize]
                }
            };
            let lines: Vec<&crate::rythmo_line::RythmoLine> =
                ids.iter().filter_map(|id| project.get_line(*id)).collect();
            let first = lines.iter().map(|line| line.start_frame).min().unwrap_or(target);
            let moves: Vec<(u64, i64, f32)> = lines
                .iter()
                .map(|line| (line.id, target + (line.start_frame - first), line.y_slot))
                .collect();
            Plan::run_and_say(
                vec![UiAction::MoveLines { moves }],
                t("assistant.info.playhead_moved").replace("{tc}", &timecode::format_timecode(target, fps)),
            )
        }
        "shift_lines" => {
            selected_lines(state, &targets)?;
            let delta_frames = match (arg_int(call, "frames"), arg_number(call, "seconds")) {
                (Some(frames), _) if frames != 0 => frames,
                (_, Some(seconds)) => (seconds * fps).round() as i64,
                (Some(frames), None) => frames,
                (None, None) => return Err(missing("seconds / frames")),
            };
            Plan::run(vec![UiAction::NudgeSelectedLines { delta_frames }])
        }
        "set_line_start" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::SetSelectedLineStartAtPlayhead])
        }
        "set_line_end" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::SetSelectedLineEndAtPlayhead])
        }
        "set_line_duration" => {
            let id = selected_line(state, &targets)?;
            let seconds = arg_number(call, "seconds").ok_or_else(|| missing("seconds"))?;
            let line = project.get_line(id).ok_or_else(|| t("assistant.no_selection").to_string())?;
            Plan::run(vec![UiAction::ResizeLine {
                id,
                start_frame: line.start_frame,
                duration_frames: ((seconds * fps).round() as i64).max(1),
            }])
        }
        "split_line" => Plan::run(vec![UiAction::SplitDialogue]),
        "toggle_karaoke" => Plan::run(vec![UiAction::ToggleKaraokeForSelection]),
        "toggle_bold" => Plan::run(vec![UiAction::ToggleTextStyle { kind: TextStyleKind::Bold }]),
        "toggle_italic" => Plan::run(vec![UiAction::ToggleTextStyle { kind: TextStyleKind::Italic }]),
        "toggle_underline" => Plan::run(vec![UiAction::ToggleTextStyle { kind: TextStyleKind::Underline }]),
        "toggle_strikethrough" => Plan::run(vec![UiAction::ToggleTextStyle {
            kind: TextStyleKind::Strikethrough,
        }]),
        "set_text_emotion" => {
            let line_id = selected_line(state, &targets)?;
            let text = arg_text(call, "emotion").ok_or_else(|| missing("emotion"))?;
            let emotion = parse_emotion(&text).ok_or_else(|| t("assistant.bad_value").replace("{value}", &text))?;
            Plan::run(vec![UiAction::SetTextEmotion {
                line_id,
                range: None,
                emotion: Some(emotion),
            }])
        }
        "clear_text_emotion" => {
            let line_id = selected_line(state, &targets)?;
            Plan::run(vec![UiAction::SetTextEmotion {
                line_id,
                range: None,
                emotion: None,
            }])
        }
        "open_emotion_menu" => Plan::run(vec![UiAction::OpenTextEmotionMenu]),
        "link_lines" | "link_lines_with_character" => {
            let line_ids = selected_lines(state, &targets)?;
            if line_ids.len() < 2 {
                return Err(t("accessibility.link_needs_two_lines").to_string());
            }
            Plan::run(vec![UiAction::LinkLines {
                source_line_id: line_ids[0],
                line_ids,
                include_character: spec.name == "link_lines_with_character",
            }])
        }
        "unlink_lines" => {
            let line_id = selected_line(state, &targets)?;
            Plan::run(vec![UiAction::UnlinkLines { line_id }])
        }
        "set_line_note" => {
            let line_id = selected_line(state, &targets)?;
            let note = arg_text(call, "note").ok_or_else(|| missing("note"))?;
            Plan::run(vec![UiAction::UpdateLineNote { line_id, note }])
        }
        "add_note" => {
            selected_line(state, &targets)?;
            Plan::run(vec![UiAction::AddNote])
        }
        "set_line_on_screen" | "set_line_off_screen" | "set_line_back" => {
            let line_id = selected_line(state, &targets)?;
            let presence = match spec.name {
                "set_line_on_screen" => LinePresence::On,
                "set_line_off_screen" => LinePresence::Off,
                _ => LinePresence::Back,
            };
            Plan::run(vec![UiAction::SetLinePresence { line_id, presence }])
        }
        "generate_detection_signs" => {
            let line_id = selected_line(state, &targets)?;
            Plan::run(vec![UiAction::GenerateDetectionSigns { line_id }])
        }
        "undo" => Plan::run(vec![UiAction::Undo]),
        "redo" => Plan::run(vec![UiAction::Redo]),
        // ---- Markers ----------------------------------------------------
        "add_loop" => Plan::run(vec![UiAction::AddMarker(MarkerKind::Boucle)]),
        "add_out_point" => Plan::run(vec![UiAction::AddMarker(MarkerKind::Out)]),
        "add_scene_change" => Plan::run(vec![UiAction::AddMarker(MarkerKind::SceneChange)]),
        "add_liaison_left" => Plan::run(vec![UiAction::AddMarker(MarkerKind::LiaisonLeft)]),
        "add_liaison_right" => Plan::run(vec![UiAction::AddMarker(MarkerKind::LiaisonRight)]),
        "add_breath" => Plan::run(vec![UiAction::AddQuickLine {
            text: breath_text(arg_text(call, "kind")),
        }]),
        "add_reaction" => Plan::run(vec![UiAction::AddQuickLine {
            text: reaction_text(arg_text(call, "kind")),
        }]),
        "add_sync_point" => Plan::run(vec![UiAction::AddSyncPointAtPlayhead]),
        "list_markers" => {
            let markers = project.markers();
            let count = |kind: MarkerKind| markers.iter().filter(|m| m.kind == kind).count();
            if markers.is_empty() {
                Plan::say(t("assistant.info.no_markers").to_string())
            } else {
                let loops: Vec<String> = markers
                    .iter()
                    .filter(|m| m.kind == MarkerKind::Boucle)
                    .take(12)
                    .map(|m| timecode::format_timecode(m.frame, fps))
                    .collect();
                let mut message = t("assistant.info.markers")
                    .replace("{loops}", &count(MarkerKind::Boucle).to_string())
                    .replace("{outs}", &count(MarkerKind::Out).to_string())
                    .replace("{scenes}", &count(MarkerKind::SceneChange).to_string());
                if !loops.is_empty() {
                    message.push('\n');
                    message.push_str(&t("assistant.info.loops_at").replace("{list}", &loops.join(", ")));
                }
                Plan::say(message)
            }
        }
        // ---- Characters -------------------------------------------------
        "rename_character" => {
            let new_name = arg_text(call, "new_name").ok_or_else(|| missing("new_name"))?;
            let old = arg_text(call, "old_name");
            let known = old.as_deref().and_then(|name| resolve_character(state, name));
            match known {
                Some((old_name, _)) if !query::refers_to_line(query) && targets.is_none() => {
                    Plan::run(vec![UiAction::RenameCharacter { old_name, new_name }])
                }
                // "le personnage de la ligne", or a name the project does not
                // know: like typing the name in the line's character field,
                // which creates the character if needed.
                _ => {
                    let line_ids = match selected_lines(state, &targets) {
                        Ok(ids) => ids,
                        Err(error) => {
                            return Err(match old {
                                Some(old) if !query::refers_to_line(query) => {
                                    t("assistant.unknown_character").replace("{name}", &old)
                                }
                                _ => error,
                            })
                        }
                    };
                    let (name, color) = character_or_new(state, &new_name);
                    Plan::run(vec![UiAction::SetLinesRole { line_ids, name, color }])
                }
            }
        }
        "set_character_color" => {
            let name = arg_text(call, "character").ok_or_else(|| missing("character"))?;
            let color_text = arg_text(call, "color").ok_or_else(|| missing("color"))?;
            let (role, _) = known_character(state, &name)?;
            let color = colors::parse_color(&color_text)
                .ok_or_else(|| t("assistant.bad_color").replace("{value}", &color_text))?;
            Plan::run(vec![UiAction::SetRoleColor { role, color }])
        }
        "list_characters" => {
            let mut names: Vec<String> = project.character_names_from_lines();
            names.retain(|name| !name.trim().is_empty());
            if names.is_empty() {
                Plan::say(t("assistant.info.no_characters").to_string())
            } else {
                Plan::say(t("assistant.info.characters").replace("{list}", &names.join(", ")))
            }
        }
        "open_roles_panel" => Plan::run(vec![UiAction::OpenRolesPanel]),
        "open_lines_panel" => Plan::run(vec![UiAction::OpenLinesPanel]),
        "open_rename_character" => Plan::run(vec![UiAction::OpenRenameCharacterModal]),
        "open_voice_actors" => Plan::run(vec![UiAction::OpenVoiceActorModal]),
        "create_voice_actor" => {
            let name = arg_text(call, "name").ok_or_else(|| missing("name"))?;
            Plan::run(vec![UiAction::CreateVoiceActor {
                name,
                icon_path: String::new(),
            }])
        }
        "assign_voice_actor_to_line" | "assign_voice_actor_to_character" => {
            let line_id = selected_line(state, &targets)?;
            let actor_name = arg_text(call, "actor").ok_or_else(|| missing("actor"))?;
            if spec.name == "assign_voice_actor_to_character" {
                Plan::run(vec![UiAction::AssignVoiceActorCharacter { line_id, actor_name }])
            } else {
                Plan::run(vec![UiAction::AssignVoiceActorLine { line_id, actor_name }])
            }
        }
        // ---- Drawing ----------------------------------------------------
        "use_select_tool" => Plan::run(vec![UiAction::SetToolMode(ToolMode::Select)]),
        "use_draw_tool" => Plan::run(vec![UiAction::SetToolMode(ToolMode::Draw)]),
        "toggle_eraser" => Plan::run(vec![UiAction::ToggleEraser]),
        "change_brush_size" => Plan::run(vec![UiAction::CycleBrushSize]),
        "open_brush_color" => Plan::run(vec![UiAction::OpenBrushColorPicker]),
        // ---- Export and windows -----------------------------------------
        "open_export" => Plan::run(vec![UiAction::OpenExportModal]),
        "export_subtitles" => Plan::run_and_say(
            vec![UiAction::OpenExportModal],
            t("assistant.export_hint").replace("{format}", "SRT / ASS / DETX / PDF"),
        ),
        "cancel_export" => Plan::run(vec![UiAction::CancelExport]),
        "open_proxy" => Plan::run(vec![UiAction::OpenProxyModal]),
        "open_project_settings" => Plan::run(vec![UiAction::OpenProjectSettings]),
        "set_scroll_speed"
        | "set_reading_bar_offset"
        | "toggle_highlight_read_word"
        | "toggle_character_color_text"
        | "toggle_emotion_lanes"
        | "toggle_timecode_display"
        | "set_start_timecode"
        | "set_waveform_height" => Plan::run(vec![project_settings_action(state, spec.name, call)?]),
        "open_band_style" => Plan::run(vec![UiAction::OpenBandStyleModal]),
        "open_settings" => Plan::run(vec![UiAction::OpenSettings]),
        "show_shortcuts" | "hide_shortcuts" => {
            let (lang, hide_formatting_hints) = {
                let config = crate::config::get();
                (config.lang.clone(), config.ui.hide_formatting_hints)
            };
            Plan::run(vec![UiAction::SaveSettings {
                lang,
                temporary_directory: crate::config::temporary_directory(),
                show_controls_hint: spec.name == "show_shortcuts",
                hide_formatting_hints,
            }])
        }
        "open_automation" => Plan::run(vec![UiAction::OpenAutomation]),
        "toggle_project_files" => Plan::run(vec![UiAction::ToggleFileTree]),
        "open_recent_projects" => Plan::run(vec![UiAction::OpenRecentProjects]),
        "close_panel" => Plan::run(vec![
            UiAction::CloseSidePanel,
            UiAction::CloseFileTree,
            UiAction::CloseAutomation,
            UiAction::CloseConformModal,
        ]),
        // ---- Conform ----------------------------------------------------
        "open_conform" => {
            let video_id = match arg_text(call, "video") {
                Some(name) => Some(video_by_name(state, &name)?),
                None => None,
            };
            let mut actions = vec![UiAction::OpenConformModal { video_id }];
            if let Some(name) = arg_text(call, "band") {
                let band = band_by_name(state, &name)?;
                actions.push(UiAction::ConformSelectBand { id: band.id });
            }
            Plan::run(actions)
        }
        "conform_video" => {
            let path = arg_text(call, "path").ok_or_else(|| missing("path"))?;
            let path = path.trim_matches(|c| c == '"' || c == '\'' || c == '«' || c == '»').trim().to_string();
            if !std::path::Path::new(&path).is_file() {
                return Err(t("assistant.info.conform_file_missing").replace("{file}", &path));
            }
            let video_id = match arg_text(call, "video") {
                Some(name) => video_by_name(state, &name)?,
                None => default_conform_video(state)?,
            };
            let band_id = match arg_text(call, "band") {
                Some(name) => band_by_name(state, &name)?.id,
                None => project.active_language_id(),
            };
            Plan::run_and_say(
                vec![UiAction::ConformStart {
                    video_id,
                    band_id,
                    path: path.clone(),
                    move_takes: true,
                }],
                t("assistant.info.conform_started").replace("{file}", &path),
            )
        }
        "get_conform_report" => match state.conform_report() {
            Some(report) => Plan::say(report.text()),
            None => Plan::say(t("assistant.info.conform_report_none").to_string()),
        },
        // ---- Project ----------------------------------------------------
        "save_project" => Plan::run(vec![UiAction::QuickSave]),
        "save_project_as" => Plan::run(vec![UiAction::ExportProject]),
        "new_project" => Plan::run(vec![UiAction::NewProject]),
        "open_project" => Plan::run(vec![UiAction::ImportProject]),
        "close_project" => Plan::run(vec![UiAction::CloseProject]),
        "import_subtitles" => Plan::run(vec![UiAction::ImportSubtitles]),
        "restore_backup" => Plan::run(vec![UiAction::RestoreBackup]),
        "add_video" => Plan::run(vec![UiAction::AddVideo]),
        "add_audio" => Plan::run(vec![UiAction::AddMediaAudio]),
        "set_instrumental_audio" => Plan::run(vec![UiAction::PickProjectInstrumentalAudio]),
        "add_band" => match arg_text(call, "name") {
            Some(name) => Plan::run(vec![UiAction::CreateLanguage { name }]),
            None => Plan::run(vec![UiAction::AddLanguage]),
        },
        "select_band" => {
            let name = arg_text(call, "name").ok_or_else(|| missing("name"))?;
            let band = band_by_name(state, &name)?;
            Plan::run_and_say(
                vec![UiAction::SelectLanguage { id: band.id }],
                t("assistant.info.band_selected").replace("{name}", &band.name),
            )
        }
        "rename_band" => {
            let name = arg_text(call, "name").ok_or_else(|| missing("name"))?;
            Plan::run(vec![UiAction::RenameLanguage {
                id: project.active_language_id(),
                name,
            }])
        }
        "delete_band" => {
            let name = arg_text(call, "name").ok_or_else(|| missing("name"))?;
            let band = band_by_name(state, &name)?;
            Plan::run(vec![UiAction::DeleteLanguage { id: band.id }])
        }
        "list_bands" => {
            let bands = project.languages();
            let active = project.active_language().name.clone();
            let names: Vec<String> = bands.iter().map(|band| band.name.clone()).collect();
            Plan::say(
                t("assistant.info.bands")
                    .replace("{list}", &names.join(", "))
                    .replace("{active}", &active),
            )
        }
        // ---- Information ------------------------------------------------
        "get_project_info" => {
            let name = state
                .project_session
                .project_path
                .as_ref()
                .and_then(|path| path.file_stem())
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| t("assistant.info.untitled").to_string());
            let video = state
                .video_path()
                .and_then(|path| path.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "—".to_string());
            let lines = project.lines().filter(|line| line.kind == RythmoLineKind::Dialogue).count();
            let characters = project
                .character_names_from_lines()
                .iter()
                .filter(|name| !name.trim().is_empty())
                .count();
            Plan::say(
                t("assistant.info.project")
                    .replace("{name}", &name)
                    .replace("{video}", &video)
                    .replace("{duration}", &timecode::format_timecode(state.total_frames(), fps))
                    .replace("{fps}", &format!("{fps:.3}").trim_end_matches('0').trim_end_matches('.').to_string())
                    .replace("{lines}", &lines.to_string())
                    .replace("{bands}", &project.languages().len().to_string())
                    .replace("{characters}", &characters.to_string()),
            )
        }
        "count_lines" => match arg_text(call, "character") {
            Some(name) => {
                let (character, _) = known_character(state, &name)?;
                let count = project
                    .lines()
                    .filter(|line| line.kind == RythmoLineKind::Dialogue && line.character_name == character)
                    .count();
                Plan::say(
                    t("assistant.info.count_character")
                        .replace("{count}", &count.to_string())
                        .replace("{character}", &character),
                )
            }
            None => {
                let count = project.lines().filter(|line| line.kind == RythmoLineKind::Dialogue).count();
                Plan::say(t("assistant.info.count").replace("{count}", &count.to_string()))
            }
        },
        "find_line" => {
            let text = arg_text(call, "text").ok_or_else(|| missing("text"))?;
            let wanted = retrieval::normalize(&text);
            let wanted = wanted.trim();
            let line = project
                .lines()
                .find(|line| retrieval::normalize(&line.text).contains(wanted))
                .ok_or_else(|| t("assistant.no_lines_match").to_string())?;
            let mut actions = vec![UiAction::SelectLines { line_ids: vec![line.id] }];
            actions.extend(seek(line.start_frame));
            Plan::run_and_say(actions, line_summary(state, line))
        }
        "get_playhead_time" => Plan::say(
            t("assistant.info.playhead")
                .replace("{tc}", &timecode::format_timecode(current, fps))
                .replace("{frame}", &current.to_string()),
        ),
        "get_selected_line" => {
            let id = selected_line(state, &targets)?;
            let line = project.get_line(id).ok_or_else(|| t("assistant.no_selection").to_string())?;
            Plan::say(line_summary(state, line))
        }
        "get_shortcut" => {
            let feature = arg_text(call, "feature").unwrap_or_default();
            let found: Vec<String> = retrieval::select(&feature, tools::CATALOG, 4)
                .into_iter()
                .filter(|tool| !tool.fallback)
                .filter_map(|tool| {
                    tool.shortcut.map(|shortcut| {
                        t("assistant.info.shortcut")
                            .replace("{tool}", &label(tool.name))
                            .replace("{shortcut}", shortcut)
                    })
                })
                .collect();
            if found.is_empty() {
                Plan::say(t("assistant.info.no_shortcut").replace("{feature}", &feature))
            } else {
                Plan::say(found.join("\n"))
            }
        }
        "help" => Plan::say(t("assistant.help").to_string()),
        // ---- Collaboration ----------------------------------------------
        "open_server_browser" => Plan::run(vec![UiAction::OpenServerBrowser]),
        "copy_invitation_link" => Plan::run(vec![UiAction::CopyQuickJoinLink]),
        "disconnect" => Plan::run(vec![UiAction::NetworkDisconnect]),
        // ---- Accessibility ----------------------------------------------
        "toggle_screen_reader" => Plan::run(vec![UiAction::ToggleScreenReader]),
        other => return Err(t("assistant.unknown_tool").replace("{tool}", other)),
    };
    let mut plan = plan;
    if let Some(line_ids) = targets {
        plan.actions.insert(0, UiAction::SelectLines { line_ids });
    }
    Ok(plan)
}

/// Names of the tools that `plan` knows, for the catalogue test.
#[cfg(test)]
fn planned_tools() -> Vec<&'static str> {
    // One probe per tool through `plan` would need a State; the match above
    // is checked textually instead: every catalogue name must appear quoted.
    let source = include_str!("dispatch.rs");
    tools::CATALOG
        .iter()
        .map(|tool| tool.name)
        .filter(|name| source.contains(&format!("\"{name}\"")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_tool_is_executed() {
        let planned = planned_tools();
        for tool in tools::CATALOG {
            assert!(planned.contains(&tool.name), "{} has no execution", tool.name);
        }
    }

    #[test]
    fn parses_numbers_and_booleans() {
        assert_eq!(parse_number("30 %"), Some(30.0));
        assert_eq!(parse_number("-2,5"), Some(-2.5));
        assert_eq!(parse_number("x1.5"), Some(1.5));
        assert_eq!(parse_number("plein"), None);
        assert_eq!(parse_bool("oui"), Some(true));
        assert_eq!(parse_bool("Désactivé"), Some(false));
        assert_eq!(parse_bool("peut-être"), None);
    }

    #[test]
    fn parses_emotions_by_family_and_intensity() {
        assert_eq!(parse_emotion("colère"), Some(TextEmotion::AngerHeavy));
        assert_eq!(parse_emotion("colère extrême"), Some(TextEmotion::AngerExtreme));
        assert_eq!(parse_emotion("un peu de joie"), Some(TextEmotion::JoySoft));
        assert_eq!(parse_emotion("grosse peur"), Some(TextEmotion::FearStrong));
        assert_eq!(parse_emotion("question rapide"), Some(TextEmotion::QuestionFast));
        assert_eq!(parse_emotion("shake"), Some(TextEmotion::Shake));
        assert_eq!(parse_emotion("joy_burst"), Some(TextEmotion::JoyBurst));
        assert_eq!(parse_emotion("bof"), None);
    }

    #[test]
    fn reads_the_words_the_model_missed() {
        assert_eq!(quoted_text("crée une réplique de Marie : Bonjour tout le monde"), Some("Bonjour tout le monde".into()));
        assert_eq!(quoted_text("Créé une phrase qui dit \"Hello World\""), Some("Hello World".into()));
        assert_eq!(quoted_text("dis « Salut »"), Some("Salut".into()));
        assert_eq!(quoted_text("va à 1:30"), None);
        assert_eq!(quoted_text("ajoute une boucle"), None);
    }

    #[test]
    fn invented_optional_arguments_are_dropped() {
        let call = ToolCall {
            name: "create_line".into(),
            arguments: serde_json::json!({"text": "Bonjour", "character": "Marie", "track": 7}),
        };
        let kept = without_ungrounded(&call, &["create_line.track".to_string()]).unwrap();
        assert!(kept.arguments.get("track").is_none());
        assert_eq!(kept.arguments["text"], "Bonjour");
        let seek = ToolCall {
            name: "set_volume".into(),
            arguments: serde_json::json!({"percent": 42}),
        };
        assert!(without_ungrounded(&seek, &["set_volume.percent".to_string()]).is_none());
        assert!(without_ungrounded(&seek, &["other.percent".to_string()]).is_some());
    }

    #[test]
    fn breath_and_reaction_texts() {
        assert_eq!(breath_text(None), "(H)");
        assert_eq!(breath_text(Some("mhh".into())), "(mHH)");
        assert_eq!(reaction_text(Some("pff".into())), "(pff)");
        assert_eq!(reaction_text(Some("(ah)".into())), "(ah)");
        assert_eq!(reaction_text(Some("zzz".into())), "(X)");
    }
}
