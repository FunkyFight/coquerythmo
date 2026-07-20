from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one occurrence, found {count}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


replace_once(
    "src/application/delta_codec.rs",
    """        Command::DeleteLines { .. } => return None,
        Command::SplitLine {
""",
    """        Command::DeleteLines { .. } => return None,
        Command::Detection { .. } => return None,
        Command::SplitLine {
""",
)

replace_once(
    "src/project_metadata.rs",
    """        Command::UpdateLineNote {
            line_id, old_note, ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.note != old_note {
                return Err(format!("line {line_id} does not match the previous note"));
            }
        }
        Command::AddDrawingStroke { stroke } => {
""",
    """        Command::UpdateLineNote {
            line_id, old_note, ..
        } => {
            let line = project
                .get_line(*line_id)
                .ok_or_else(|| missing_line(*line_id))?;
            if &line.note != old_note {
                return Err(format!("line {line_id} does not match the previous note"));
            }
        }
        Command::Detection { change } => match change {
            crate::detection::DetectionChange::Add { address, cue } => {
                if project.get_line(address.line_id).is_none() {
                    return Err(missing_line(address.line_id));
                }
                if cue.id != address.detection_id {
                    return Err("detection address does not match cue id".into());
                }
                if project.detections().detection(*address).is_some() {
                    return Err(format!(
                        "detection {} already exists on line {}",
                        address.detection_id.0, address.line_id
                    ));
                }
                cue.target.validate()?;
            }
            crate::detection::DetectionChange::Remove { address, cue } => {
                if project.detections().detection(*address) != Some(cue) {
                    return Err(format!(
                        "detection {} on line {} does not match the remove snapshot",
                        address.detection_id.0, address.line_id
                    ));
                }
            }
            crate::detection::DetectionChange::Move {
                address, old_tick, ..
            } => {
                let current = project
                    .detections()
                    .detection(*address)
                    .ok_or_else(|| {
                        format!(
                            "detection {} on line {} does not exist",
                            address.detection_id.0, address.line_id
                        )
                    })?;
                if current.media_tick != *old_tick {
                    return Err(format!(
                        "detection {} on line {} does not match the move origin",
                        address.detection_id.0, address.line_id
                    ));
                }
            }
            crate::detection::DetectionChange::RemoveLine { line_id, data } => {
                if project.detections().line(*line_id) != Some(data) {
                    return Err(format!(
                        "detection data for line {line_id} does not match the remove snapshot"
                    ));
                }
            }
        },
        Command::AddDrawingStroke { stroke } => {
""",
)

replace_once(
    "src/state.rs",
    """                Selection::Strokes(ids) => {
                    if !ids.is_empty() {
                        self.erase_drawing_strokes(ids.clone());
                    }
                }
            }
""",
    """                Selection::Strokes(ids) => {
                    if !ids.is_empty() {
                        self.erase_drawing_strokes(ids.clone());
                    }
                }
                Selection::Detection(_) => {
                    // Routed through the semantic detection action before this
                    // legacy selection deletion path is reached.
                }
            }
""",
)

replace_once(
    "src/state.rs",
    """            Some(Selection::Marker(_) | Selection::Strokes(_)) | None => Vec::new(),
""",
    """            Some(
                Selection::Marker(_) | Selection::Strokes(_) | Selection::Detection(_),
            )
            | None => Vec::new(),
""",
)

replace_once(
    "src/workspaces/rythmo/selection.rs",
    """        Selection::Line(_) | Selection::Marker(_) | Selection::Strokes(_) => Vec::new(),
""",
    """        Selection::Line(_)
        | Selection::Marker(_)
        | Selection::Detection(_)
        | Selection::Strokes(_) => Vec::new(),
""",
)

print("Detection exhaustive-match fixes applied")
