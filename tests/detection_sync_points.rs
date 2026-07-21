use coquerythmo::detection::{DetectionDocument, DetectionKind, MediaTick, TextAnchor};

#[test]
fn moving_sync_point_changes_only_its_timing_data() {
    let line_id = 42;
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
    assert_eq!(
        detections.sync_point(address).unwrap().line_tick,
        MediaTick::from_frame(60)
    );
    assert_eq!(base_ratios, original_ratios);
}

#[test]
fn text_sync_point_remains_internal_to_the_detection_palette() {
    assert_eq!(DetectionKind::ALL.len(), 7);
    assert!(!DetectionKind::ALL.contains(&DetectionKind::TextSyncPoint));
}
