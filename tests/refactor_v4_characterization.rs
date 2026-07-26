use std::fs;
use std::path::Path;

use coquerythmo::application::edit_service::{EditExecutor, EditOrigin};
use coquerythmo::application::project_service::ProjectSession;
use coquerythmo::command::Command;
use coquerythmo::export::ProjectData;
use coquerythmo::packet::{CommandPayload, Packet};
use coquerythmo::project::Project;
use coquerythmo::render_index::ProjectRenderIndex;
use coquerythmo::rendering::rythmo::scene::{FrameWindow, RythmoScene, SceneOptions};
use coquerythmo::rythmo_line::RythmoLine;
use serde_json::Value;

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .expect("fixture must exist")
}

fn golden(name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/goldens")
                .join(name),
        )
        .expect("golden must exist"),
    )
    .expect("golden must be valid JSON")
}

fn normalize_float_precision(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(normalize_float_precision),
        Value::Object(values) => values.values_mut().for_each(normalize_float_precision),
        Value::Number(number) if !number.is_i64() && !number.is_u64() => {
            let rounded = (number.as_f64().unwrap() * 1_000_000.0).round() / 1_000_000.0;
            *number = serde_json::Number::from_f64(rounded).unwrap();
        }
        _ => {}
    }
}

#[test]
fn all_phase_zero_project_fixtures_round_trip_without_schema_drift() {
    for name in [
        "project-empty.json",
        "project-small.json",
        "project-karaoke.json",
        "project-drawings.json",
        "project-characters-markers.json",
        "project-large.json",
    ] {
        let source: Value =
            serde_json::from_str(&fixture(name)).expect("fixture must be valid JSON");
        let data: ProjectData = serde_json::from_value(source.clone()).expect("project schema");
        let mut round_trip = serde_json::to_value(data).expect("project must serialize");
        let mut source = source;
        normalize_float_precision(&mut round_trip);
        normalize_float_precision(&mut source);
        assert_eq!(round_trip, source, "schema changed for {name}");
    }
}

#[test]
fn packet_and_command_payload_match_legacy_goldens() {
    let packet = Packet::RoomCreated {
        code: "ABC123".into(),
    };
    assert_eq!(
        serde_json::to_value(packet).unwrap(),
        golden("packet-room-created.json")
    );

    let payload = CommandPayload::CreateLine {
        line: RythmoLine {
            id: 42,
            start_frame: 10,
            duration_frames: 20,
            y_slot: 0.5,
            text: "test".into(),
            character_name: "Alice".into(),
            character_color: [1.0, 0.0, 0.0, 1.0],
            kind: coquerythmo::rythmo_line::RythmoLineKind::Dialogue,
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
            presence: coquerythmo::rythmo_line::LinePresence::On,
            text_emotions: Vec::new(),
        },
    };
    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        golden("command-payload-create-line.json")
    );
}

#[test]
fn local_edit_undo_redo_preserves_revision_and_values() {
    let mut session = ProjectSession::new();
    let line_id = session.project.add_line_full(
        0,
        48,
        0.5,
        "before".into(),
        "Alice".into(),
        [0.35, 0.55, 0.90, 1.0],
    );
    let before = session.project.revision();
    EditExecutor::execute(
        &mut session,
        Command::UpdateLineText {
            line_id,
            old_text: "before".into(),
            new_text: "after".into(),
            old_emotions: Vec::new(),
            new_emotions: Vec::new(),
        },
        EditOrigin::Local,
    );
    let after = session.project.revision();
    assert!(after > before);
    assert!(session.dirty);
    assert!(session.history.last().is_some());

    assert!(EditExecutor::undo(&mut session));
    assert_eq!(session.project.get_line(line_id).unwrap().text, "before");
    assert!(EditExecutor::redo(&mut session));
    assert_eq!(session.project.get_line(line_id).unwrap().text, "after");
    assert!(session.project.revision() > after);
}

#[test]
fn rythmo_scene_fixtures_share_visible_lines_markers_and_drawings() {
    let mut karaoke_project = Project::new();
    let karaoke_data: ProjectData =
        serde_json::from_str(&fixture("project-karaoke.json")).expect("karaoke fixture schema");
    karaoke_data.apply_to_project(&mut karaoke_project, 24.0);
    let mut karaoke_index = ProjectRenderIndex::new();
    karaoke_index.refresh(&karaoke_project);
    let karaoke_scene = RythmoScene::build(
        &karaoke_project,
        &karaoke_index,
        SceneOptions {
            frame_window: FrameWindow {
                first: 0,
                last: 144,
            },
            current_frame: 48.0,
            source_fps: 24.0,
            ..SceneOptions::default()
        },
    );
    assert_eq!(karaoke_scene.lines.len(), 1);
    assert_eq!(karaoke_scene.markers.len(), 2);
    assert!(karaoke_scene.lines[0].karaoke_active);
    assert_eq!(karaoke_scene.lines[0].karaoke_progress, Some(0.25));

    let mut drawing_project = Project::new();
    let drawing_data: ProjectData =
        serde_json::from_str(&fixture("project-drawings.json")).expect("drawing fixture schema");
    drawing_data.apply_to_project(&mut drawing_project, 25.0);
    let mut drawing_index = ProjectRenderIndex::new();
    drawing_index.refresh(&drawing_project);
    let drawing_scene = RythmoScene::build(
        &drawing_project,
        &drawing_index,
        SceneOptions {
            frame_window: FrameWindow { first: 0, last: 32 },
            current_frame: 14.0,
            source_fps: 25.0,
            ..SceneOptions::default()
        },
    );
    assert_eq!(drawing_scene.drawings.len(), 1);
}
