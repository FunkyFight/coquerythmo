//! Turns a [`TimeMap`] into the reversible commands that move a band, and
//! the recording operations that move its takes.

use crate::command::{Command, LineMove};
use crate::detection::{DetectionAddress, DetectionChange, DetectionCueId, MediaTick};
use crate::project::Project;
use crate::recording::{AudioClip, ClipPlacement, RecordingOperation, RecordingProject};
use crate::rythmo_line::MarkerKind;

use super::{ConformEntry, ConformKind, SpanStatus, TimeMap};

/// Commands and report rows for one band.
#[derive(Default)]
pub struct BandPlan {
    /// A single batch, so one `Ctrl + Z` restores the band. `None` when
    /// nothing moves.
    pub command: Option<Command>,
    pub entries: Vec<ConformEntry>,
}

/// Scales an old-cut frame count to new-cut frames when the frame rate
/// changed with the cut.
fn scaled(frame: i64, ratio: f64) -> i64 {
    if ratio == 1.0 {
        frame
    } else {
        (frame as f64 * ratio).round() as i64
    }
}

fn scaled_position(position: f64, ratio: f64) -> f64 {
    if ratio == 1.0 {
        position
    } else {
        position * ratio
    }
}

fn scaled_tick(tick: MediaTick, ratio: f64) -> MediaTick {
    if ratio == 1.0 {
        tick
    } else {
        tick.scaled(ratio)
    }
}

fn marker_label(kind: &MarkerKind) -> &'static str {
    crate::i18n::t(match kind {
        MarkerKind::Boucle => "conform.marker.loop",
        MarkerKind::Out => "conform.marker.out",
        MarkerKind::SceneChange => "conform.marker.scene_change",
        MarkerKind::LiaisonLeft | MarkerKind::LiaisonRight => "conform.marker.liaison",
    })
}

fn line_label(line: &crate::rythmo_line::RythmoLine) -> String {
    let text = line.text.trim();
    let text: String = if text.chars().count() > 60 {
        let mut short: String = text.chars().take(57).collect();
        short.push('…');
        short
    } else {
        text.to_string()
    };
    let mut label = if line.character_name.is_empty() {
        text
    } else {
        format!("{}: {text}", line.character_name)
    };
    // The comment travels with its dialogue; the report says so.
    let note = line.note.trim();
    if !note.is_empty() {
        let note: String = if note.chars().count() > 40 {
            let mut short: String = note.chars().take(37).collect();
            short.push('…');
            short
        } else {
            note.to_string()
        };
        label.push_str(&format!(" [{}: {note}]", crate::i18n::t("conform.note")));
    }
    label
}

/// Plans the conformation of the active band of `project`.
///
/// `ratio` converts old-cut frames to new-cut frames (1 when the frame rate
/// did not change).
pub fn plan_band(project: &Project, map: &TimeMap, ratio: f64) -> BandPlan {
    let mut entries = Vec::new();
    let mut commands = Vec::new();

    // Dialogues. Whole moves carry their synchronisation points along;
    // re-cut dialogues are resized and their points re-placed one by one.
    // Deletions come first so their snapshots match the band as it is, and
    // every other step sees the same band as a journal replay will.
    let mut moves = Vec::new();
    let mut deleted = Vec::new();
    let mut sync_commands = Vec::new();
    for line in project.lines() {
        let mapping = map.map_span(line.start_frame, line.end_frame());
        let label = line_label(line);
        match (mapping.status, mapping.new_start, mapping.new_end) {
            (SpanStatus::Deleted, _, _) | (_, None, _) | (_, _, None) => {
                if let Some(index) = project.line_index(line.id) {
                    deleted.push((line.clone(), index));
                }
                entries.push(ConformEntry {
                    kind: ConformKind::Line,
                    label,
                    old_start: line.start_frame,
                    new_start: None,
                    status: SpanStatus::Deleted,
                });
            }
            (SpanStatus::Unchanged, Some(new_start), _) if ratio == 1.0 => {
                entries.push(ConformEntry {
                    kind: ConformKind::Line,
                    label,
                    old_start: line.start_frame,
                    new_start: Some(new_start),
                    status: SpanStatus::Unchanged,
                });
            }
            (SpanStatus::Unchanged | SpanStatus::Moved, Some(new_start), Some(new_end)) => {
                let new_start = scaled(new_start, ratio);
                let new_end = scaled(new_end, ratio).max(new_start + 1);
                if new_end - new_start == line.duration_frames {
                    moves.push(LineMove {
                        line_id: line.id,
                        old_start: line.start_frame,
                        old_y_slot: line.y_slot,
                        new_start,
                        new_y_slot: line.y_slot,
                    });
                } else {
                    // The frame rate changed: resize and re-place the points.
                    commands.push(Command::ResizeLine {
                        line_id: line.id,
                        old_start: line.start_frame,
                        old_dur: line.duration_frames,
                        new_start,
                        new_dur: new_end - new_start,
                    });
                    sync_commands.extend(replace_sync_points(
                        project, map, ratio, line.id, new_start, new_end,
                    ));
                }
                entries.push(ConformEntry {
                    kind: ConformKind::Line,
                    label,
                    old_start: line.start_frame,
                    new_start: Some(new_start),
                    status: SpanStatus::Moved,
                });
            }
            (status, Some(new_start), Some(new_end)) => {
                let new_start = scaled(new_start, ratio);
                let new_end = scaled(new_end, ratio).max(new_start + 1);
                commands.push(Command::ResizeLine {
                    line_id: line.id,
                    old_start: line.start_frame,
                    old_dur: line.duration_frames,
                    new_start,
                    new_dur: new_end - new_start,
                });
                sync_commands.extend(replace_sync_points(
                    project, map, ratio, line.id, new_start, new_end,
                ));
                entries.push(ConformEntry {
                    kind: ConformKind::Line,
                    label,
                    old_start: line.start_frame,
                    new_start: Some(new_start),
                    status,
                });
            }
        }
    }
    if !moves.is_empty() {
        commands.insert(0, Command::MoveLines { moves });
    }
    if !deleted.is_empty() {
        commands.insert(0, Command::DeleteLines { lines: deleted });
    }
    commands.extend(sync_commands);

    // Source signs on the detection tracks (not tied to a dialogue).
    let mut cue_changes = Vec::new();
    for line_id in project.detections().line_ids() {
        if !crate::detection::is_track_storage_line_id(line_id) {
            continue;
        }
        let Some(data) = project.detections().line(line_id) else {
            continue;
        };
        for cue in data.detections() {
            let address = DetectionAddress {
                line_id,
                detection_id: cue.id,
            };
            let old_frame = cue.media_tick.as_frame_position();
            let label = format!("{:?}", cue.kind).to_lowercase();
            match map.map_position(old_frame) {
                Some(new_frame) => {
                    let new_tick =
                        scaled_tick(MediaTick::from_frame_position(new_frame), ratio);
                    let status = if new_tick == cue.media_tick {
                        SpanStatus::Unchanged
                    } else {
                        cue_changes.push(DetectionChange::Move {
                            address,
                            old_tick: cue.media_tick,
                            new_tick,
                        });
                        SpanStatus::Moved
                    };
                    entries.push(ConformEntry {
                        kind: ConformKind::Detection,
                        label,
                        old_start: old_frame.floor() as i64,
                        new_start: Some(new_tick.as_frame_position().floor() as i64),
                        status,
                    });
                }
                None => {
                    cue_changes.push(DetectionChange::Remove {
                        address,
                        cue: cue.clone(),
                    });
                    entries.push(ConformEntry {
                        kind: ConformKind::Detection,
                        label,
                        old_start: old_frame.floor() as i64,
                        new_start: None,
                        status: SpanStatus::Deleted,
                    });
                }
            }
        }
    }
    if !cue_changes.is_empty() {
        commands.push(Command::Detection {
            change: DetectionChange::Batch {
                changes: cue_changes,
            },
        });
    }

    // Markers: moves keep the indices valid; removals go last, highest
    // index first, so each index still means the same marker.
    let mut removals = Vec::new();
    for (index, marker) in project.markers().iter().enumerate() {
        let label = marker_label(&marker.kind).to_string();
        match map.map_frame(marker.frame) {
            Some(new_frame) => {
                let new_frame = scaled(new_frame, ratio);
                let status = if new_frame == marker.frame {
                    SpanStatus::Unchanged
                } else {
                    commands.push(Command::MoveMarker {
                        index,
                        old_frame: marker.frame,
                        new_frame,
                    });
                    SpanStatus::Moved
                };
                entries.push(ConformEntry {
                    kind: ConformKind::Marker,
                    label,
                    old_start: marker.frame,
                    new_start: Some(new_frame),
                    status,
                });
            }
            None => {
                removals.push((index, marker.clone()));
                entries.push(ConformEntry {
                    kind: ConformKind::Marker,
                    label,
                    old_start: marker.frame,
                    new_start: None,
                    status: SpanStatus::Deleted,
                });
            }
        }
    }
    for (index, marker) in removals.into_iter().rev() {
        commands.push(Command::RemoveMarker { marker, index });
    }

    // Drawings: a stroke follows its points; points over deleted pictures
    // are dropped, and a stroke left with fewer than two points is erased.
    let mut stroke_ids = Vec::new();
    let mut old_points = Vec::new();
    let mut new_points = Vec::new();
    let mut erased = Vec::new();
    for stroke in &project.drawing().strokes {
        let first = stroke
            .points
            .first()
            .map(|(frame, _)| frame.floor() as i64)
            .unwrap_or(0);
        let kept: Vec<(f64, f32)> = stroke
            .points
            .iter()
            .filter_map(|(frame, y)| {
                map.map_position(*frame)
                    .map(|new_frame| (scaled_position(new_frame, ratio), *y))
            })
            .collect();
        let label = crate::i18n::t("conform.kind.stroke").to_string();
        if kept.len() < 2 {
            erased.push(stroke.clone());
            entries.push(ConformEntry {
                kind: ConformKind::Stroke,
                label,
                old_start: first,
                new_start: None,
                status: SpanStatus::Deleted,
            });
            continue;
        }
        let status = if kept == stroke.points {
            SpanStatus::Unchanged
        } else if kept.len() == stroke.points.len() {
            SpanStatus::Moved
        } else {
            SpanStatus::Truncated
        };
        entries.push(ConformEntry {
            kind: ConformKind::Stroke,
            label,
            old_start: first,
            new_start: Some(kept[0].0.floor() as i64),
            status,
        });
        if status != SpanStatus::Unchanged {
            stroke_ids.push(stroke.id);
            old_points.push(stroke.points.clone());
            new_points.push(kept);
        }
    }
    if !stroke_ids.is_empty() {
        commands.push(Command::TransformStrokes {
            stroke_ids,
            old_points,
            new_points,
        });
    }
    if !erased.is_empty() {
        commands.push(Command::EraseDrawingStrokes { strokes: erased });
    }

    BandPlan {
        command: (!commands.is_empty()).then_some(Command::Batch { commands }),
        entries,
    }
}

/// Re-places the synchronisation points of a re-cut dialogue inside its new
/// span; points over deleted pictures, or pushed outside the span, go away.
/// One command per dialogue, so linked dialogues each get synchronised.
fn replace_sync_points(
    project: &Project,
    map: &TimeMap,
    ratio: f64,
    line_id: u64,
    new_start: i64,
    new_end: i64,
) -> Option<Command> {
    let data = project.detections().line(line_id)?;
    let start = MediaTick::from_frame(new_start);
    let end = MediaTick::from_frame(new_end);
    let mut changes = Vec::new();
    for point in data.sync_points() {
        let address = DetectionAddress {
            line_id,
            detection_id: DetectionCueId(point.id.0),
        };
        let mapped = map
            .map_position(point.line_tick.as_frame_position())
            .map(|frame| scaled_tick(MediaTick::from_frame_position(frame), ratio))
            .filter(|tick| *tick > start && *tick < end);
        match mapped {
            Some(new_tick) if new_tick != point.line_tick => changes.push(DetectionChange::Move {
                address,
                old_tick: point.line_tick,
                new_tick,
            }),
            Some(_) => {}
            None => changes.push(DetectionChange::Remove {
                address,
                cue: point.as_detection_cue(),
            }),
        }
    }
    (!changes.is_empty()).then_some(Command::Detection {
        change: DetectionChange::Batch { changes },
    })
}

/// Operations and report rows for the recording timeline.
#[derive(Default)]
pub struct RecordingPlan {
    pub operation: Option<RecordingOperation>,
    pub entries: Vec<ConformEntry>,
}

/// Plans the move of every take. `video_fps` is the old cut's frame rate,
/// `new_video_fps` the new cut's; the timeline keeps its own rate.
pub fn plan_recording(
    recording: &RecordingProject,
    map: &TimeMap,
    video_fps: f64,
    new_video_fps: f64,
) -> RecordingPlan {
    let timeline_fps = recording.timeline_fps();
    if !timeline_fps.is_finite() || timeline_fps <= 0.0 || video_fps <= 0.0 {
        return RecordingPlan::default();
    }
    let to_video = video_fps / timeline_fps;
    let from_new_video = timeline_fps / new_video_fps.max(f64::EPSILON);
    let mut entries = Vec::new();
    let mut placements = Vec::new();
    let mut deleted = Vec::new();
    let mut clips: Vec<&AudioClip> = recording.clips().collect();
    clips.sort_by_key(|clip| (clip.start_frame, clip.id));
    for clip in clips {
        let old_start = (clip.start_frame as f64 * to_video).round() as i64;
        let old_end = (clip.end_frame() as f64 * to_video).round() as i64;
        let mapping = map.map_span(old_start, old_end);
        let label = format!("{} {}", crate::i18n::t("conform.kind.take"), clip.id);
        match mapping.new_start {
            Some(new_start) => {
                // Old-cut frames → new-cut frames → timeline frames.
                let timeline_start =
                    (new_start as f64 * (new_video_fps / video_fps) * from_new_video).round() as i64;
                let status = if timeline_start == clip.start_frame
                    && mapping.status == SpanStatus::Unchanged
                {
                    SpanStatus::Unchanged
                } else {
                    if timeline_start != clip.start_frame {
                        placements.push(ClipPlacement {
                            clip_id: clip.id,
                            track_id: clip.track_id,
                            start_frame: timeline_start,
                        });
                    }
                    mapping.status.max(SpanStatus::Moved)
                };
                entries.push(ConformEntry {
                    kind: ConformKind::Take,
                    label,
                    old_start,
                    new_start: Some((new_start as f64 * (new_video_fps / video_fps)).round() as i64),
                    status,
                });
            }
            None => {
                deleted.push(clip.id);
                entries.push(ConformEntry {
                    kind: ConformKind::Take,
                    label,
                    old_start,
                    new_start: None,
                    status: SpanStatus::Deleted,
                });
            }
        }
    }
    let mut operations = Vec::new();
    if !placements.is_empty() {
        operations.push(RecordingOperation::MoveClips { placements });
    }
    if !deleted.is_empty() {
        operations.push(RecordingOperation::DeleteClips { clip_ids: deleted });
    }
    RecordingPlan {
        operation: match operations.len() {
            0 => None,
            1 => operations.pop(),
            _ => Some(RecordingOperation::Batch { operations }),
        },
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conform::Segment;
    use crate::rythmo_line::{RythmoLine, RythmoMarker};

    fn line(id: u64, start: i64, duration: i64, text: &str) -> RythmoLine {
        RythmoLine {
            id,
            start_frame: start,
            duration_frames: duration,
            y_slot: 0.0,
            text: text.into(),
            character_name: "Alice".into(),
            character_color: [1.0; 4],
            kind: Default::default(),
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: String::new(),
            presence: Default::default(),
            text_emotions: Vec::new(),
            text_styles: Vec::new(),
        }
    }

    /// 0..100 unchanged, 100..150 deleted, 150..300 shifted to 100..250.
    fn map() -> TimeMap {
        TimeMap::new(vec![
            Segment {
                old_start: 0,
                old_end: 100,
                new_start: 0,
            },
            Segment {
                old_start: 150,
                old_end: 300,
                new_start: 100,
            },
        ])
    }

    fn project() -> Project {
        let mut project = Project::new();
        project.insert_line_at(0, line(1, 10, 20, "Bonjour"));
        project.insert_line_at(1, line(2, 110, 20, "Coupée"));
        project.insert_line_at(2, line(3, 200, 30, "Décalée"));
        project.insert_line_at(3, line(4, 90, 70, "À cheval"));
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 50,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 120,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Out,
            frame: 250,
        });
        project
    }

    #[test]
    fn the_band_follows_the_pictures_and_undo_restores_it() {
        let mut project = project();
        let before = project.snapshot();
        let plan = plan_band(&project, &map(), 1.0);
        let command = plan.command.expect("something moves");
        command.apply(&mut project);

        assert_eq!(project.get_line(1).unwrap().start_frame, 10);
        assert!(project.get_line(2).is_none(), "the cut dialogue is removed");
        assert_eq!(project.get_line(3).unwrap().start_frame, 150);
        assert_eq!(project.get_line(3).unwrap().duration_frames, 30);
        let straddling = project.get_line(4).unwrap();
        assert_eq!(straddling.start_frame, 90);
        assert_eq!(straddling.end_frame(), 110, "the deleted middle is gone");
        let markers: Vec<i64> = project.markers().iter().map(|marker| marker.frame).collect();
        assert_eq!(markers, vec![50, 200]);

        let statuses: Vec<(u64, SpanStatus)> = plan
            .entries
            .iter()
            .filter(|entry| entry.kind == ConformKind::Line)
            .zip([1u64, 2, 3, 4])
            .map(|(entry, id)| (id, entry.status))
            .collect();
        assert_eq!(
            statuses,
            vec![
                (1, SpanStatus::Unchanged),
                (2, SpanStatus::Deleted),
                (3, SpanStatus::Moved),
                (4, SpanStatus::Truncated),
            ]
        );
        assert!(plan.entries.iter().any(|entry| entry.needs_rerecord()));

        command.unapply(&mut project);
        assert_eq!(project.lines_vec(), before.lines_vec());
        assert_eq!(
            project.markers().iter().map(|m| m.frame).collect::<Vec<_>>(),
            before.markers().iter().map(|m| m.frame).collect::<Vec<_>>()
        );
    }

    /// A sync point of `line` at `tick`, before grapheme `boundary`.
    fn add_sync_point(project: &mut Project, line: u64, id: u64, boundary: u32, tick: i64) {
        let cue = crate::detection::DetectionCue {
            id: DetectionCueId(id),
            kind: crate::detection::DetectionKind::TextSyncPoint,
            media_tick: MediaTick::from_frame(tick),
            duration: MediaTick::ZERO,
            target: crate::detection::TextAnchor::Grapheme { index: boundary },
        };
        Command::Detection {
            change: DetectionChange::Add {
                address: DetectionAddress {
                    line_id: line,
                    detection_id: DetectionCueId(id),
                },
                cue,
            },
        }
        .apply(project);
    }

    #[test]
    fn the_batch_replays_through_the_transaction_journal() {
        let mut project = project();
        // Points on the unchanged line, the moved line and the truncated one.
        add_sync_point(&mut project, 1, 1, 2, 15);
        add_sync_point(&mut project, 3, 1, 3, 210);
        add_sync_point(&mut project, 4, 1, 2, 95);
        add_sync_point(&mut project, 4, 2, 4, 130);
        let fps = 25.0;
        let mut journal =
            crate::project_metadata::TransactionJournal::from_project(&project, fps).unwrap();

        let plan = plan_band(&project, &map(), 1.0);
        let command = plan.command.expect("something moves");
        journal
            .append(project.active_language_id(), command.clone())
            .unwrap();
        command.apply(&mut project);

        let replayed = journal.replay(fps).expect("every step of the batch holds on replay");
        assert_eq!(replayed.lines_vec(), project.lines_vec());
        assert_eq!(replayed.detections(), project.detections());

        // The moved line kept its point, the truncated one lost the point
        // that sat on the deleted pictures.
        let moved = project.detections().line(3).unwrap();
        assert_eq!(moved.sync_points()[0].line_tick, MediaTick::from_frame(160));
        let truncated = project.detections().line(4).unwrap();
        assert_eq!(truncated.sync_points().len(), 1);
        assert_eq!(truncated.sync_points()[0].line_tick, MediaTick::from_frame(95));
    }

    #[test]
    fn the_report_carries_the_comment_of_a_moved_line() {
        let mut project = project();
        project.get_line_mut(3).unwrap().note = "reprendre le souffle".into();
        let plan = plan_band(&project, &map(), 1.0);
        let row = plan
            .entries
            .iter()
            .find(|entry| entry.kind == ConformKind::Line && entry.label.contains("Décalée"))
            .unwrap();
        assert_eq!(row.status, SpanStatus::Moved);
        assert!(row.label.contains("reprendre le souffle"), "{}", row.label);
        let text = row.line(25.0, 25.0);
        assert!(text.contains("00:00:08:00 → 00:00:06:00"), "{text}");
    }

    #[test]
    fn nothing_to_do_gives_no_command() {
        let project = project();
        let map = TimeMap::new(vec![Segment {
            old_start: 0,
            old_end: 1000,
            new_start: 0,
        }]);
        let plan = plan_band(&project, &map, 1.0);
        assert!(plan.command.is_none());
        assert!(plan
            .entries
            .iter()
            .all(|entry| entry.status == SpanStatus::Unchanged));
    }

    #[test]
    fn a_frame_rate_change_rescales_the_positions() {
        let mut project = project();
        let map = TimeMap::new(vec![Segment {
            old_start: 0,
            old_end: 1000,
            new_start: 0,
        }]);
        let plan = plan_band(&project, &map, 2.0);
        plan.command.unwrap().apply(&mut project);
        assert_eq!(project.get_line(1).unwrap().start_frame, 20);
        assert_eq!(project.get_line(1).unwrap().duration_frames, 40);
        assert_eq!(project.markers()[0].frame, 100);
    }

    #[test]
    fn takes_move_with_their_pictures() {
        let mut recording = RecordingProject::new(25.0).unwrap();
        let track = recording.allocate_track_id();
        recording
            .apply(&RecordingOperation::AddTrack {
                track: crate::recording::AudioTrack::new(track, "Voix"),
            })
            .unwrap();
        let asset = crate::recording::AudioAsset {
            id: recording.allocate_asset_id(),
            file_name: "take.flac".into(),
            sample_rate: 48_000,
            channels: 1,
            sample_count: 48_000,
            checksum: "0".repeat(40),
            waveform: Default::default(),
        };
        recording
            .apply(&RecordingOperation::AddAsset {
                asset: asset.clone(),
            })
            .unwrap();
        let kept = recording.allocate_clip_id();
        let cut = recording.allocate_clip_id();
        for (id, start) in [(kept, 200), (cut, 110)] {
            recording
                .apply(&RecordingOperation::AddClip {
                    clip: AudioClip {
                        id,
                        asset_id: asset.id,
                        track_id: track,
                        start_frame: start,
                        source_start_frame: 0,
                        duration_frames: 25,
                    },
                })
                .unwrap();
        }
        let plan = plan_recording(&recording, &map(), 25.0, 25.0);
        recording.apply(plan.operation.as_ref().unwrap()).unwrap();
        assert_eq!(recording.clip(kept).unwrap().start_frame, 150);
        assert!(recording.clip(cut).is_none());
        assert_eq!(
            plan.entries
                .iter()
                .filter(|entry| entry.needs_rerecord())
                .count(),
            1
        );
    }
}
