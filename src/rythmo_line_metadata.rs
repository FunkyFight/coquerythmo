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

pub fn is_encoded(note: &str) -> bool {
    note.starts_with(HEADER_PREFIX)
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

fn encode_with_user_note(metadata: LineMetadata, user_note: &str) -> String {
    if metadata == LineMetadata::default() {
        return user_note.to_string();
    }
    let json = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string());
    format!("{HEADER_PREFIX}{json}\n{user_note}")
}

pub fn encode(metadata: LineMetadata, existing_note: &str) -> String {
    encode_with_user_note(metadata, user_note(existing_note))
}

/// Replace only the human-authored note while retaining presentation metadata.
pub fn replace_user_note(existing_note: &str, new_user_note: &str) -> String {
    let (metadata, _) = decode(existing_note);
    encode_with_user_note(metadata, new_user_note)
}

/// Normalize any note update at the application boundary. Semantic controls
/// submit an already encoded value; text editors submit plain human text.
pub fn merge_note_update(existing_note: &str, proposed_note: &str) -> String {
    if is_encoded(proposed_note) {
        proposed_note.to_string()
    } else {
        replace_user_note(existing_note, proposed_note)
    }
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
    fn text_update_preserves_existing_semantics() {
        let encoded = with_kind("ancienne note", LineSemanticKind::AmbienceStart);
        let updated = merge_note_update(&encoded, "nouvelle note");
        let (metadata, note) = decode(&updated);
        assert_eq!(metadata.kind, LineSemanticKind::AmbienceStart);
        assert_eq!(note, "nouvelle note");
    }

    #[test]
    fn semantic_update_is_not_wrapped_twice() {
        let existing = with_presentation("note", LinePresentation::Off);
        let proposed = with_kind(&existing, LineSemanticKind::AmbienceEnd);
        assert_eq!(merge_note_update(&existing, &proposed), proposed);
    }

    #[test]
    fn malformed_header_falls_back_to_plain_note() {
        let malformed = "\u{001f}coquerythmo-line-v1:not-json\nhello";
        let (metadata, note) = decode(malformed);
        assert_eq!(metadata, LineMetadata::default());
        assert_eq!(note, "hello");
    }
}
