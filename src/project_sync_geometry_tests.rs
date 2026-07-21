use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionCueId, DetectionKind, MediaTick,
    TextAnchor,
};
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::ui::primitives::Rect;
use crate::workspaces::rythmo::view::{render_lines, RythmoState};

fn add_sync(project: &mut Project, line_id: u64, id: u64, index: u32, frame: i64) {
    let cue = DetectionCue {
        id: DetectionCueId(id),
        kind: DetectionKind::TextSyncPoint,
        media_tick: MediaTick::from_frame(frame),
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

fn render_example(project: &Project) -> (Vec<String>, Vec<(f32, f32)>) {
    let zone = Rect {
        x: 0.0,
        y: 0.0,
        width: 1200.0,
        height: 800.0,
    };
    let state = RythmoState::new();
    let mut index = ProjectRenderIndex::new();
    index.refresh(project);
    let mut quads = Vec::new();
    let mut syllable_quads = Vec::new();
    let mut labels = Vec::new();
    let mut stretched = Vec::new();
    let mut note_icons = Vec::new();
    let mut actor_icons = Vec::new();

    let _ = render_lines(
        &zone,
        project,
        &index,
        150.0,
        false,
        24.0,
        &state,
        &mut quads,
        &mut syllable_quads,
        &mut labels,
        &mut stretched,
        &mut note_icons,
        &mut actor_icons,
        [0.0; 4],
        [[0.0; 4]; 7],
    );

    let expected = ["Bonjour", " ", "à tous"];
    let mut segments = stretched
        .iter()
        .filter(|item| expected.contains(&item.text.as_str()))
        .collect::<Vec<_>>();
    segments.sort_by(|left, right| {
        left.dest_rect
            .x
            .partial_cmp(&right.dest_rect.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assert_eq!(segments.len(), 3);

    let line_start = segments
        .iter()
        .map(|item| item.dest_rect.x)
        .fold(f32::INFINITY, f32::min);
    let line_end = segments
        .iter()
        .map(|item| item.dest_rect.x + item.dest_rect.width)
        .fold(f32::NEG_INFINITY, f32::max);
    let line_width = (line_end - line_start).max(1.0);
    let text = segments.iter().map(|item| item.text.clone()).collect();
    let geometry = segments
        .iter()
        .map(|item| {
            (
                (item.dest_rect.x - line_start) / line_width,
                item.dest_rect.width / line_width,
            )
        })
        .collect();
    (text, geometry)
}

#[test]
fn bonjour_space_and_a_tous_are_three_independent_fitted_boxes() {
    let mut project = Project::new();
    let line_id = project.add_line_full(
        100,
        100,
        0.0,
        "Bonjour à tous".into(),
        String::new(),
        [1.0; 4],
    );
    add_sync(&mut project, line_id, 1, 7, 140); // after `r`
    add_sync(&mut project, line_id, 2, 8, 170); // before `à`

    let (text, geometry) = render_example(&project);
    assert_eq!(text, vec!["Bonjour", " ", "à tous"]);
    for ((actual_start, actual_width), (start, width)) in geometry
        .iter()
        .copied()
        .zip([(0.0, 0.4), (0.4, 0.3), (0.7, 0.3)])
    {
        assert!((actual_start - start).abs() < 0.001);
        assert!((actual_width - width).abs() < 0.001);
    }
}

#[test]
fn moving_first_cut_only_changes_its_two_neighbouring_boxes() {
    let mut project = Project::new();
    let line_id = project.add_line_full(
        100,
        100,
        0.0,
        "Bonjour à tous".into(),
        String::new(),
        [1.0; 4],
    );
    add_sync(&mut project, line_id, 1, 7, 140);
    add_sync(&mut project, line_id, 2, 8, 170);
    let before = render_example(&project).1;

    let address = DetectionAddress {
        line_id,
        detection_id: DetectionCueId(1),
    };
    assert!(project.apply_detection_change(
        &DetectionChange::Move {
            address,
            old_tick: MediaTick::from_frame(140),
            new_tick: MediaTick::from_frame(150),
        },
        true,
    ));
    let after = render_example(&project).1;

    assert!((before[0].1 - 0.4).abs() < 0.001);
    assert!((after[0].1 - 0.5).abs() < 0.001);
    assert!((before[1].0 - 0.4).abs() < 0.001);
    assert!((after[1].0 - 0.5).abs() < 0.001);
    assert_eq!(before[2], after[2], "the `à tous` box is not remapped");
}
