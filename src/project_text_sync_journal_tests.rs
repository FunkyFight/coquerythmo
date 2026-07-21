use crate::command::{Command, CommandHistory};
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionCueId, DetectionKind, MediaTick,
    TextAnchor,
};
use crate::project::Project;
use crate::project_metadata::TransactionJournal;

fn add_sync(project: &mut Project, line_id: u64, id: u64, index: u32, tick: i64) {
    let cue = DetectionCue {
        id: DetectionCueId(id),
        kind: DetectionKind::TextSyncPoint,
        media_tick: MediaTick(tick),
        target: TextAnchor::Grapheme { index },
    };
    let address = DetectionAddress {
        line_id,
        detection_id: cue.id,
    };
    assert!(project.apply_detection_change(
        &DetectionChange::Add { address, cue },
        true,
    ));
}

fn sync_values(project: &Project, line_id: u64) -> Vec<(u32, MediaTick)> {
    let mut values = project
        .detections()
        .line(line_id)
        .unwrap()
        .text_sync_cues()
        .map(|cue| (cue.target.grapheme_index().unwrap(), cue.media_tick))
        .collect::<Vec<_>>();
    values.sort_by_key(|(index, tick)| (*index, *tick));
    values
}

fn project_with_boxes() -> (Project, u64) {
    let mut project = Project::new();
    let line_id = project.add_line_full(
        0,
        100,
        0.0,
        "abcdefghi".into(),
        String::new(),
        [1.0; 4],
    );
    add_sync(&mut project, line_id, 1, 3, 300);
    add_sync(&mut project, line_id, 2, 6, 700);
    (project, line_id)
}

#[test]
fn transaction_replay_preserves_independent_text_boxes() {
    let (mut project, line_id) = project_with_boxes();
    let mut journal = TransactionJournal::from_project(&project, 24.0).unwrap();
    let language_id = project.active_language_id();
    let command = Command::UpdateLineText {
        line_id,
        old_text: "abcdefghi".into(),
        new_text: "aZZbcdeQQfghi".into(),
    };

    command.apply(&mut project);
    journal.append(language_id, command).unwrap();
    let replayed = journal.replay(24.0).unwrap();

    assert_eq!(replayed.get_line(line_id).unwrap().text, "aZZbcdeQQfghi");
    assert_eq!(
        sync_values(&replayed, line_id),
        vec![(5, MediaTick(300)), (10, MediaTick(700))]
    );
}

#[test]
fn undo_redo_rebases_indices_without_moving_ticks() {
    let (mut project, line_id) = project_with_boxes();
    let mut history = CommandHistory::new();
    let command = Command::UpdateLineText {
        line_id,
        old_text: "abcdefghi".into(),
        new_text: "abcdXXXXefghi".into(),
    };
    command.apply(&mut project);
    history.push(command);

    assert_eq!(
        sync_values(&project, line_id),
        vec![(3, MediaTick(300)), (10, MediaTick(700))]
    );
    history.undo(&mut project);
    assert_eq!(
        sync_values(&project, line_id),
        vec![(3, MediaTick(300)), (6, MediaTick(700))]
    );
    history.redo(&mut project);
    assert_eq!(
        sync_values(&project, line_id),
        vec![(3, MediaTick(300)), (10, MediaTick(700))]
    );
}
