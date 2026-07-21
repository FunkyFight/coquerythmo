//! Backward-compatible semantic metadata for rythmo lines.
//!
//! Until the project archive receives a dedicated annotations collection, the
//! metadata is stored in a versioned header inside the already serialized note
//! field. User note text remains byte-for-byte preserved after the header.

use serde::{Deserialize, Serialize};

const HEADER_PREFIX: &str = "\u{001f}coquerythmo-line-v1:";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinePresentation {
    #[default]
    On,
    Off,
    Back,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineSemanticKind {
    #[default]
    Dialogue,
    AmbienceStart,
    AmbienceEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineMetadata {
    #[serde(default)]
    pub presentation: LinePresentation,
    #[serde(default)]
    pub kind: LineSemanticKind,
}

pub fn decode(note: &str) -> (LineMetadata, &str) {
    let Some(rest) = note.strip_prefix(HEADER_PREFIX) else {
        return (LineMetadata::default(), note);
    };
    let Some((json, user_note)) = rest.split_once('\n') else {
        return (LineMetadata::default(), note);
    };
    let metadata = serde_json::from_str(json).unwrap_or_default();
    (metadata, user_note)
}

pub fn user_note(note: &str) -> &str {
    decode(note).1
}

pub fn encode(metadata: LineMetadata, existing_note: &str) -> String {
    let user_note = user_note(existing_note);
    if metadata == LineMetadata::default() {
        return user_note.to_string();
    }
    let json = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string());
    format!("{HEADER_PREFIX}{json}\n{user_note}")
}

pub fn with_presentation(existing_note: &str, presentation: LinePresentation) -> String {
    let (mut metadata, _) = decode(existing_note);
    metadata.presentation = presentation;
    encode(metadata, existing_note)
}

pub fn with_kind(existing_note: &str, kind: LineSemanticKind) -> String {
    let (mut metadata, _) = decode(existing_note);
    metadata.kind = kind;
    encode(metadata, existing_note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trip_preserves_user_note() {
        let source = "À vérifier avec la DA";
        let encoded = with_presentation(source, LinePresentation::Off);
        let (metadata, note) = decode(&encoded);
        assert_eq!(metadata.presentation, LinePresentation::Off);
        assert_eq!(note, source);
    }

    #[test]
    fn returning_to_defaults_removes_header() {
        let encoded = with_presentation("note", LinePresentation::Back);
        let decoded = with_presentation(&encoded, LinePresentation::On);
        assert_eq!(decoded, "note");
    }

    #[test]
    fn malformed_header_falls_back_to_plain_note() {
        let malformed = "\u{001f}coquerythmo-line-v1:not-json\nhello";
        let (metadata, note) = decode(malformed);
        assert_eq!(metadata, LineMetadata::default());
        assert_eq!(note, "hello");
    }
}
