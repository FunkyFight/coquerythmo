use coquerythmo::detection::{DetectionDocument, DetectionKind, MediaTick, TextAnchor};

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.0001
}

#[test]
fn moving_sync_point_changes_timing_without_mutating_base_ratios() {
    let line_id = 42;
    let text = "abcd";
    let breaks = [1, 2, 3];
    let base_ratios = vec![0.1, 0.2, 0.3, 0.4];
    let original_ratios = base_ratios.clone();

    let mut detections = DetectionDocument::default();
    let address = detections
        .add_detection(
            line_id,
            DetectionKind::TextSyncPoint,
            MediaTick::from_frame(30),
            TextAnchor::Grapheme { index: 2 },
        )
        .expect("the synchronization point should be created");

    assert!(detections.move_detection(address, MediaTick::from_frame(60)));

    let warped = detections.warped_ratios(
        line_id,
        text,
        &breaks,
        &base_ratios,
        0,
        100,
    );

    assert_eq!(base_ratios, original_ratios);
    assert_eq!(warped.len(), original_ratios.len());
    assert!(approximately_equal(warped.iter().sum(), 1.0));

    // The point at character boundary 2 moves the second half of the phrase,
    // while the relative dispersion on each side stays unchanged.
    assert!(approximately_equal(warped[0] / warped[1], 0.5));
    assert!(approximately_equal(warped[2] / warped[3], 0.75));
    assert!(approximately_equal(warped[0] + warped[1], 0.6));
}

#[test]
fn text_sync_point_remains_internal_to_the_detection_palette() {
    assert_eq!(DetectionKind::ALL.len(), 7);
    assert!(!DetectionKind::ALL.contains(&DetectionKind::TextSyncPoint));
}
