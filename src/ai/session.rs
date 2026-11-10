//! The assistant request in flight and the actions it produced.

use std::sync::mpsc::Receiver;
use std::time::Instant;

use super::needle::NeedleResponse;
use crate::application::command::UiAction;

pub struct AssistantJob {
    pub query: String,
    /// Names of the tools the engine was given, best lexical match first.
    pub tool_names: Vec<&'static str>,
    pub receiver: Receiver<Result<NeedleResponse, String>>,
    pub started: Instant,
}

#[derive(Default)]
pub struct AssistantSession {
    pub pending: Option<AssistantJob>,
    /// Actions planned from the last answer, run by the event loop through
    /// the command dispatcher like any other action.
    pub queued_actions: Vec<UiAction>,
    /// The whole request as typed, echoed in the bubble.
    pub request: String,
    /// Sentences of the request still to send, once the previous one has
    /// been answered and its actions run.
    pub pending_clauses: Vec<String>,
    /// Messages of the sentences answered so far.
    pub results: Vec<String>,
}

impl AssistantSession {
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
}

/// What the engine is told about the session before the query. Built by the
/// app from its state and by the training data exporter from samples, so the
/// model is trained on the exact text it will read.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct SessionFacts {
    pub date: String,
    pub language: String,
    pub playhead: String,
    pub fps: f64,
    pub duration: Option<String>,
    /// Text and character of the first selected line, and how many more.
    pub selected: Option<(String, String, usize)>,
    pub characters: Vec<String>,
    pub bands: Vec<String>,
    pub shown_band: String,
}

impl SessionFacts {
    /// The system text: one fact per line, kept short since it shares the
    /// context window with the tool schemas.
    pub fn to_system_prompt(&self) -> String {
        let mut facts = vec![
            format!("date: {}", self.date),
            "app: Coquerythmo, a dubbing rythmo band (bande rythmo) editor; lines sit on tracks 1 to 4".to_string(),
            format!("user language: {}", self.language),
            format!("playhead: {} ({} fps)", self.playhead, self.fps),
        ];
        if let Some(duration) = &self.duration {
            facts.push(format!("video duration: {duration}"));
        }
        match &self.selected {
            Some((text, character, more)) => {
                let text: String = text.chars().take(80).collect();
                facts.push(format!(
                    "selected line: \"{}\" by {}{}",
                    text.trim(),
                    if character.trim().is_empty() { "nobody" } else { character.trim() },
                    if *more > 0 { format!(" (+{more} more selected)") } else { String::new() }
                ));
            }
            None => facts.push("selected line: none".to_string()),
        }
        let characters: Vec<&str> = self
            .characters
            .iter()
            .map(String::as_str)
            .filter(|name| !name.trim().is_empty())
            .take(20)
            .collect();
        if !characters.is_empty() {
            facts.push(format!("characters: {}", characters.join(", ")));
        }
        if self.bands.len() > 1 {
            facts.push(format!("bands: {} (shown: {})", self.bands.join(", "), self.shown_band));
        }
        facts.join("\n")
    }
}

/// Today's date as `YYYY-MM-DD` (UTC), the fact the engine expects first.
pub fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard
/// Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
        assert_eq!(civil_from_days(20_741), (2026, 10, 15));
        assert_eq!(today().len(), 10);
    }
}
