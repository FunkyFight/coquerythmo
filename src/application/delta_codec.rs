//! Legacy delta JSON codec.
//!
//! The wire keys are intentionally kept here rather than reconstructed in
//! the state machine. This is an application/infrastructure boundary: the
//! reversible domain command remains the source of truth, while this module
//! preserves the existing forward-only delta format.

use crate::command::Command;
use crate::packet::{CharacterData, CommandPayload, MoveLinePayload};
use crate::project::Project;

/// Encode a canonical command into the legacy `delta` event payload.
pub fn encode_delta(command: &Command, project: &Project) -> Option<serde_json::Value> {
    Some(match command {
        Command::CreateLine { snapshot, .. } => {
            serde_json::json!({ "action": "create_line", "line": serde_json::to_value(snapshot).ok()? })
        }
        Command::InsertLine { snapshot, .. } => {
            serde_json::json!({ "action": "create_line", "line": serde_json::to_value(snapshot).ok()? })
        }
        Command::InsertLines { .. } => return None,
        Command::DeleteLine { snapshot, .. } => {
            serde_json::json!({ "action": "delete_line", "line_id": snapshot.id })
        }
        Command::DeleteLines { .. } => return None,
        Command::Detection { .. } => return None,
        Command::SplitLine {
            first_line,
            second_line,
            second_index,
            ..
        } => serde_json::json!({
            "action": "split_line",
            "first_line": serde_json::to_value(first_line).ok()?,
            "second_line": serde_json::to_value(second_line).ok()?,
            "second_index": second_index,
        }),
        Command::MoveLine {
            line_id,
            new_start,
            new_y_slot,
            ..
        } => serde_json::json!({
            "action": "move_line",
            "line_id": line_id,
            "start_frame": new_start,
            "y_slot": new_y_slot,
        }),
        Command::MoveLines { moves } => {
            let lines: Vec<_> = moves
                .iter()
                .map(|movement| {
                    serde_json::json!({
                        "line_id": movement.line_id,
                        "start_frame": movement.new_start,
                        "y_slot": movement.new_y_slot,
                    })
                })
                .collect();
            serde_json::json!({ "action": "move_lines", "lines": lines })
        }
        Command::ResizeLine {
            line_id,
            new_start,
            new_dur,
            ..
        } => serde_json::json!({
            "action": "resize_line",
            "line_id": line_id,
            "start_frame": new_start,
            "duration_frames": new_dur,
        }),
        Command::UpdateLineText {
            line_id, new_text, ..
        } => serde_json::json!({ "action": "update_text", "line_id": line_id, "text": new_text }),
        Command::UpdateLineNote {
            line_id, new_note, ..
        } => serde_json::json!({ "action": "update_note", "line_id": line_id, "note": new_note }),
        Command::SetLineKaraoke {
            line_id,
            new_karaoke,
            new_ratios,
            ..
        } => serde_json::json!({
            "action": "set_line_karaoke",
            "line_id": line_id,
            "karaoke": new_karaoke,
            "syllable_ratios": new_ratios,
        }),
        Command::SetSyllableRatios {
            line_id,
            new_ratios,
            ..
        } => serde_json::json!({
            "action": "set_syllable_ratios",
            "line_id": line_id,
            "ratios": new_ratios,
        }),
        Command::SetCharacter {
            line_id,
            new_name,
            new_color,
            new_voice_actor_names,
            ..
        } => serde_json::json!({
            "action": "set_character",
            "line_id": line_id,
            "name": new_name,
            "color": new_color,
            "voice_actor_names": new_voice_actor_names,
        }),
        Command::SetCharacterColor {
            line_id, new_color, ..
        } => serde_json::json!({
            "action": "set_character_color",
            "line_id": line_id,
            "color": new_color,
        }),
        Command::RenameCharacter {
            changes,
            new_known_characters,
            ..
        } => serde_json::json!({
            "action": "rename_character",
            "changes": changes,
            "known_characters": new_known_characters,
        }),
        Command::SetVoiceActors { changes } => {
            serde_json::json!({ "action": "set_voice_actors", "changes": changes })
        }
        Command::CreateVoiceActor { actor } => {
            serde_json::json!({ "action": "create_voice_actor", "actor": actor })
        }
        Command::AddMarker { marker, .. } => {
            serde_json::json!({
                "action": "add_marker",
                "kind": serde_json::to_value(&marker.kind).ok()?,
                "frame": marker.frame,
            })
        }
        Command::RemoveMarker { marker, .. } => serde_json::json!({
            "action": "remove_marker",
            "kind": serde_json::to_value(&marker.kind).ok()?,
            "frame": marker.frame,
        }),
        Command::MoveMarker {
            index,
            old_frame,
            new_frame,
        } => {
            let marker = project.marker(*index)?;
            serde_json::json!({
                "action": "move_marker",
                "kind": serde_json::to_value(&marker.kind).ok()?,
                "old_frame": old_frame,
                "new_frame": new_frame,
            })
        }
        Command::AddDrawingStroke { stroke } => serde_json::json!({
            "action": "add_drawing_stroke",
            "stroke": serde_json::to_value(stroke).ok()?,
        }),
        Command::EraseDrawingStrokes { strokes } => serde_json::json!({
            "action": "erase_drawing_strokes",
            "strokes": serde_json::to_value(strokes).ok()?,
        }),
        Command::TransformStrokes {
            stroke_ids,
            new_points,
            ..
        } => serde_json::json!({
            "action": "transform_strokes",
            "stroke_ids": stroke_ids,
            "new_points": new_points,
        }),
    })
}

/// Decode the legacy `delta` event into the validated forward-only payload.
/// Unknown actions and malformed fields are rejected before they reach the
/// project mutation boundary.
pub fn decode_delta(data: &serde_json::Value) -> Option<CommandPayload> {
    let action = data.get("action")?.as_str()?;
    match action {
        "create_line" => Some(CommandPayload::CreateLine {
            line: serde_json::from_value(data.get("line")?.clone()).ok()?,
        }),
        "delete_line" => Some(CommandPayload::DeleteLine {
            line_id: data.get("line_id")?.as_u64()?,
        }),
        "split_line" => Some(CommandPayload::SplitLine {
            first_line: serde_json::from_value(data.get("first_line")?.clone()).ok()?,
            second_line: serde_json::from_value(data.get("second_line")?.clone()).ok()?,
            second_index: data.get("second_index")?.as_u64()? as usize,
        }),
        "move_line" => Some(CommandPayload::MoveLine {
            line_id: data.get("line_id")?.as_u64()?,
            start_frame: data.get("start_frame")?.as_i64()?,
            y_slot: data.get("y_slot")?.as_f64()? as f32,
        }),
        "move_lines" => Some(CommandPayload::MoveLines {
            lines: data.get("lines").and_then(|value| {
                serde_json::from_value::<Vec<MoveLinePayload>>(value.clone()).ok()
            })?,
        }),
        "resize_line" => Some(CommandPayload::ResizeLine {
            line_id: data.get("line_id")?.as_u64()?,
            start_frame: data.get("start_frame")?.as_i64()?,
            duration_frames: data.get("duration_frames")?.as_i64()?,
        }),
        "update_text" => Some(CommandPayload::UpdateLineText {
            line_id: data.get("line_id")?.as_u64()?,
            text: data.get("text")?.as_str()?.to_string(),
        }),
        "update_note" => Some(CommandPayload::UpdateLineNote {
            line_id: data.get("line_id")?.as_u64()?,
            note: data.get("note")?.as_str()?.to_string(),
        }),
        "set_line_karaoke" => Some(CommandPayload::SetLineKaraoke {
            line_id: data.get("line_id")?.as_u64()?,
            karaoke: data.get("karaoke")?.as_bool()?,
            syllable_ratios: serde_json::from_value(data.get("syllable_ratios")?.clone()).ok()?,
        }),
        "set_syllable_ratios" => Some(CommandPayload::SetSyllableRatios {
            line_id: data.get("line_id")?.as_u64()?,
            ratios: serde_json::from_value(data.get("ratios")?.clone()).ok()?,
        }),
        "set_character" => Some(CommandPayload::SetCharacter {
            line_id: data.get("line_id")?.as_u64()?,
            name: data.get("name")?.as_str()?.to_string(),
            color: serde_json::from_value(data.get("color")?.clone()).ok()?,
            voice_actor_names: Some(
                serde_json::from_value(data.get("voice_actor_names")?.clone()).ok()?,
            ),
        }),
        "set_character_color" => Some(CommandPayload::SetCharacterColor {
            line_id: data.get("line_id")?.as_u64()?,
            color: serde_json::from_value(data.get("color")?.clone()).ok()?,
        }),
        "rename_character" => Some(CommandPayload::RenameCharacter {
            changes: serde_json::from_value(data.get("changes")?.clone()).ok()?,
            known_characters: serde_json::from_value::<Vec<CharacterData>>(
                data.get("known_characters")?.clone(),
            )
            .ok()?,
        }),
        "set_voice_actors" => Some(CommandPayload::SetVoiceActors {
            changes: serde_json::from_value(data.get("changes")?.clone()).ok()?,
        }),
        "create_voice_actor" => Some(CommandPayload::CreateVoiceActor {
            actor: serde_json::from_value(data.get("actor")?.clone()).ok()?,
        }),
        "add_marker" => Some(CommandPayload::AddMarker {
            kind: serde_json::from_value(data.get("kind")?.clone()).ok()?,
            frame: data.get("frame")?.as_i64()?,
        }),
        "remove_marker" => Some(CommandPayload::RemoveMarker {
            kind: serde_json::from_value(data.get("kind")?.clone()).ok()?,
            frame: data.get("frame")?.as_i64()?,
        }),
        "move_marker" => Some(CommandPayload::MoveMarker {
            kind: serde_json::from_value(data.get("kind")?.clone()).ok()?,
            old_frame: data.get("old_frame")?.as_i64()?,
            new_frame: data.get("new_frame")?.as_i64()?,
        }),
        "add_drawing_stroke" => Some(CommandPayload::AddDrawingStroke {
            stroke: serde_json::from_value(data.get("stroke")?.clone()).ok()?,
        }),
        "erase_drawing_strokes" => Some(CommandPayload::EraseDrawingStrokes {
            strokes: serde_json::from_value(data.get("strokes")?.clone()).ok()?,
        }),
        "transform_strokes" => Some(CommandPayload::TransformStrokes {
            stroke_ids: serde_json::from_value(data.get("stroke_ids")?.clone()).ok()?,
            new_points: serde_json::from_value(data.get("new_points")?.clone()).ok()?,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_line_delta_keeps_legacy_keys() {
        let mut project = Project::new();
        let line_id = project.add_line(0, 12, 0.0);
        let snapshot = project.get_line(line_id).cloned().expect("line snapshot");
        let value = encode_delta(&Command::CreateLine { snapshot, index: 0 }, &project).unwrap();
        assert_eq!(value["action"], "create_line");
        assert_eq!(value["line"]["id"], line_id);
    }

    #[test]
    fn delta_decode_rejects_unknown_actions_and_round_trips_known_shape() {
        assert!(decode_delta(&serde_json::json!({ "action": "unknown" })).is_none());
        let data = serde_json::json!({
            "action": "move_line",
            "line_id": 7,
            "start_frame": 12,
            "y_slot": 0.5
        });
        assert!(matches!(
            decode_delta(&data),
            Some(CommandPayload::MoveLine { line_id: 7, .. })
        ));
    }
}
