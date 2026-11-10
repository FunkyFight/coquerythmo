//! Conforming a project to a new cut of its video.
//!
//! When the production delivers a V2 of the edit, both cuts are fingerprinted
//! ([`signature`]), the shots they share are aligned ([`align`]) and every
//! time-anchored element of a band (dialogues, markers, lip-sync detections,
//! drawings) and of the recording timeline (takes) is moved to where its
//! pictures now sit ([`plan`]). Elements whose pictures were re-cut or
//! removed are reported so the team knows what to check or record again.

pub mod align;
pub mod plan;
pub mod signature;

use std::path::Path;
use std::sync::atomic::AtomicBool;

pub use align::Segment;

/// Thumbnail difference (sum over 16×16 grey pixels) under which two frames
/// show the same picture, used to grow the hashed runs across dissolves and
/// dark frames: a mean of 8 grey levels.
const THUMB_SIMILARITY: u32 = 8 * 256;

/// The old cut of a project video, compared with its new cut.
pub struct ConformAnalysis {
    /// Shared shots, sorted by old position, in frames at `fps`.
    pub segments: Vec<Segment>,
    pub old_frames: i64,
    /// Length of the new cut, in frames at `fps`.
    pub new_frames: i64,
    /// Frame rate of the old cut: the unit of the project positions.
    pub fps: f64,
    /// Frame rate of the new cut, the unit of the conformed positions.
    pub new_fps: f64,
    /// Share of the old cut found again in the new one.
    pub coverage: f32,
    /// Shots recovered by their soundtrack only.
    pub audio_segments: usize,
}

impl ConformAnalysis {
    pub fn time_map(&self) -> TimeMap {
        TimeMap::new(self.segments.clone())
    }

    /// Multiplier from old-cut frames to new-cut frames.
    pub fn fps_ratio(&self) -> f64 {
        if crate::project_metadata::fps_matches(self.fps, self.new_fps) {
            1.0
        } else {
            self.new_fps / self.fps
        }
    }
}

/// Fingerprints both files and aligns them. `progress` goes from 0 to 1.
pub fn analyze(
    old_path: &Path,
    new_path: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(f32),
) -> Result<ConformAnalysis, String> {
    let old_info = crate::video_proxy::probe_video(old_path)?;
    let new_info = crate::video_proxy::probe_video(new_path)?;
    let fps = if old_info.fps.is_finite() && old_info.fps > 0.0 {
        old_info.fps
    } else {
        24.0
    };
    let new_fps = if new_info.fps.is_finite() && new_info.fps > 0.0 {
        new_info.fps
    } else {
        fps
    };
    let both_have_audio = old_info.audio_codec.is_some() && new_info.audio_codec.is_some();
    let old = signature::compute(
        old_path,
        fps,
        old_info.duration_secs,
        both_have_audio,
        cancel,
        &|share| progress(share * 0.45),
    )?;
    let new = signature::compute(
        new_path,
        fps,
        new_info.duration_secs,
        both_have_audio,
        cancel,
        &|share| progress(0.45 + share * 0.45),
    )?;
    let analysis = align_signatures(&old, &new, fps, new_fps);
    progress(1.0);
    Ok(analysis)
}

/// Pure part of [`analyze`], shared with the tests.
pub fn align_signatures(
    old: &signature::VideoSignature,
    new: &signature::VideoSignature,
    fps: f64,
    new_fps: f64,
) -> ConformAnalysis {
    let video = align::align(
        &old.hashes,
        &new.hashes,
        align::AlignParams::VIDEO,
        |old_frame, new_frame| {
            old.thumb_distance(old_frame, new, new_frame)
                .unwrap_or(u32::MAX)
        },
    );
    let mut segments = align::refine(
        video,
        old.len(),
        new.len(),
        align::AlignParams::VIDEO.max_gap,
        |old_frame, new_frame| {
            old.thumb_distance(old_frame, new, new_frame)
                .is_some_and(|distance| distance <= THUMB_SIMILARITY)
        },
    );
    let mut audio_segments = 0;
    if !old.audio.is_empty() && !new.audio.is_empty() {
        let old_audio: Vec<u64> = old.audio.iter().map(|hash| *hash as u64).collect();
        let new_audio: Vec<u64> = new.audio.iter().map(|hash| *hash as u64).collect();
        let audio = align::align(
            &old_audio,
            &new_audio,
            align::AlignParams::AUDIO,
            |_, _| 0,
        );
        let audio = align::refine(
            audio,
            old_audio.len() as i64,
            new_audio.len() as i64,
            align::AlignParams::AUDIO.max_gap,
            |old_frame, new_frame| {
                (old_audio[old_frame as usize] ^ new_audio[new_frame as usize]).count_ones()
                    <= align::AlignParams::AUDIO.max_distance
            },
        );
        let extra = fill_gaps(&segments, audio, align::AlignParams::AUDIO.min_run);
        audio_segments = extra.len();
        segments.extend(extra);
        segments.sort_by_key(|segment| segment.old_start);
        segments = align::merge_adjacent(segments);
    }
    let covered: i64 = segments.iter().map(Segment::len).sum();
    ConformAnalysis {
        coverage: if old.is_empty() {
            0.0
        } else {
            (covered as f64 / old.len() as f64) as f32
        },
        segments,
        old_frames: old.len(),
        new_frames: new.len(),
        fps,
        new_fps,
        audio_segments,
    }
}

/// Keeps the parts of `secondary` that `primary` does not already cover, in
/// the old cut as well as in the new one.
fn fill_gaps(primary: &[Segment], secondary: Vec<Segment>, min_run: i64) -> Vec<Segment> {
    let mut extra = Vec::new();
    for candidate in secondary {
        let mut free = vec![(candidate.old_start, candidate.old_end)];
        for taken in primary {
            free = subtract_all(&free, taken.old_start, taken.old_end);
            let shift = taken.new_start - candidate.offset();
            free = subtract_all(&free, shift, shift + taken.len());
            if free.is_empty() {
                break;
            }
        }
        for (start, end) in free {
            if end - start >= min_run {
                extra.push(Segment {
                    old_start: start,
                    old_end: end,
                    new_start: start + candidate.offset(),
                });
            }
        }
    }
    extra
}

fn subtract_all(intervals: &[(i64, i64)], start: i64, end: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::with_capacity(intervals.len() + 1);
    for &(a, b) in intervals {
        if end <= a || start >= b {
            out.push((a, b));
            continue;
        }
        if start > a {
            out.push((a, start));
        }
        if end < b {
            out.push((end, b));
        }
    }
    out
}

/// What happened to a time span of the old cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpanStatus {
    /// Same pictures at the same place.
    Unchanged,
    /// Same pictures, elsewhere.
    Moved,
    /// The pictures were re-cut inside the span: something was inserted,
    /// shortened or reordered under it.
    Modified,
    /// Part of the span no longer exists; the rest was kept.
    Truncated,
    /// None of the pictures remain.
    Deleted,
}

impl SpanStatus {
    pub fn needs_rerecord(self) -> bool {
        matches!(self, Self::Modified | Self::Truncated)
    }
}

/// Where a span of the old cut lands in the new one, in old-cut frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanMapping {
    pub status: SpanStatus,
    /// Absent only when the span was deleted.
    pub new_start: Option<i64>,
    pub new_end: Option<i64>,
}

/// Old-cut frame → new-cut frame, from the shared shots.
#[derive(Clone, Debug, Default)]
pub struct TimeMap {
    segments: Vec<Segment>,
}

impl TimeMap {
    pub fn new(mut segments: Vec<Segment>) -> Self {
        segments.sort_by_key(|segment| segment.old_start);
        Self { segments }
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    fn segment_at(&self, frame: i64) -> Option<&Segment> {
        let index = self
            .segments
            .partition_point(|segment| segment.old_start <= frame);
        let segment = self.segments.get(index.checked_sub(1)?)?;
        segment.contains_old(frame).then_some(segment)
    }

    pub fn map_frame(&self, frame: i64) -> Option<i64> {
        self.segment_at(frame)
            .map(|segment| frame + segment.offset())
    }

    /// Fractional positions keep their fraction inside the frame.
    pub fn map_position(&self, position: f64) -> Option<f64> {
        if !position.is_finite() {
            return None;
        }
        let frame = position.floor() as i64;
        self.segment_at(frame)
            .map(|segment| position + segment.offset() as f64)
    }

    /// Maps `[start, end)`; a zero-length span is treated as one frame.
    pub fn map_span(&self, start: i64, end: i64) -> SpanMapping {
        let end = end.max(start + 1);
        let parts: Vec<(i64, i64, i64)> = self
            .segments
            .iter()
            .filter(|segment| segment.old_start < end && segment.old_end > start)
            .map(|segment| {
                (
                    segment.old_start.max(start),
                    segment.old_end.min(end),
                    segment.offset(),
                )
            })
            .collect();
        if parts.is_empty() {
            return SpanMapping {
                status: SpanStatus::Deleted,
                new_start: None,
                new_end: None,
            };
        }
        let covered: i64 = parts.iter().map(|(a, b, _)| b - a).sum();
        let whole = covered == end - start;
        if parts.len() == 1 && whole {
            let offset = parts[0].2;
            return SpanMapping {
                status: if offset == 0 {
                    SpanStatus::Unchanged
                } else {
                    SpanStatus::Moved
                },
                new_start: Some(start + offset),
                new_end: Some(end + offset),
            };
        }
        let (first_start, _, first_offset) = parts[0];
        let (_, last_end, last_offset) = parts[parts.len() - 1];
        let new_start = first_start + first_offset;
        let new_end = last_end + last_offset;
        let sane = new_end > new_start && new_end - new_start <= 4 * (end - start);
        let (new_start, new_end) = if sane {
            (new_start, new_end)
        } else {
            // The parts were reordered: keep the longest one, the earliest
            // when equal.
            let (a, b, offset) = parts
                .iter()
                .rev()
                .copied()
                .max_by_key(|(a, b, _)| b - a)
                .expect("at least one part");
            (a + offset, b + offset)
        };
        SpanMapping {
            status: if whole && sane {
                SpanStatus::Modified
            } else {
                SpanStatus::Truncated
            },
            new_start: Some(new_start),
            new_end: Some(new_end),
        }
    }
}

/// Which kind of element a report row is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConformKind {
    Line,
    Marker,
    Detection,
    Stroke,
    Take,
}

/// One element of the project, before and after the conformation.
#[derive(Clone, Debug, PartialEq)]
pub struct ConformEntry {
    pub kind: ConformKind,
    /// Text of the dialogue, kind of the marker, name of the take…
    pub label: String,
    /// Old position, in old-cut frames.
    pub old_start: i64,
    /// New position in new-cut frames; absent when deleted.
    pub new_start: Option<i64>,
    pub status: SpanStatus,
}

impl ConformEntry {
    pub fn needs_rerecord(&self) -> bool {
        self.kind == ConformKind::Line && self.status.needs_rerecord()
            || self.kind == ConformKind::Take
                && (self.status.needs_rerecord() || self.status == SpanStatus::Deleted)
    }
}

/// Everything the user is told once the conformation is applied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConformReport {
    pub old_video: String,
    pub new_video: String,
    pub band: String,
    pub fps: f64,
    pub new_fps: f64,
    pub coverage: f32,
    pub entries: Vec<ConformEntry>,
}

impl ConformReport {
    pub fn count(&self, status: SpanStatus) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status == status)
            .count()
    }

    pub fn rerecord_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.needs_rerecord())
            .count()
    }

    /// Entries worth reading: everything that did not stay in place.
    pub fn notable(&self) -> impl Iterator<Item = &ConformEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.status != SpanStatus::Unchanged)
    }

    /// Plain-text version, for the clipboard and the assistant.
    pub fn text(&self) -> String {
        use crate::i18n::t;
        let mut out = String::new();
        out.push_str(&format!(
            "{}: {} → {} ({})\n",
            t("conform.report.title"),
            self.old_video,
            self.new_video,
            self.band
        ));
        out.push_str(&format!(
            "{}: {:.0}%\n",
            t("conform.report.coverage"),
            self.coverage * 100.0
        ));
        out.push_str(&format!(
            "{}: {} · {}: {} · {}: {} · {}: {} · {}: {} · {}: {}\n",
            t("conform.status.unchanged"),
            self.count(SpanStatus::Unchanged),
            t("conform.status.moved"),
            self.count(SpanStatus::Moved),
            t("conform.status.modified"),
            self.count(SpanStatus::Modified),
            t("conform.status.truncated"),
            self.count(SpanStatus::Truncated),
            t("conform.status.deleted"),
            self.count(SpanStatus::Deleted),
            t("conform.status.rerecord"),
            self.rerecord_count()
        ));
        for entry in self.notable() {
            out.push_str(&entry.line(self.fps, self.new_fps));
            out.push('\n');
        }
        out
    }
}

impl ConformEntry {
    /// One report row: status, kind, old → new timecode, label.
    pub fn line(&self, fps: f64, new_fps: f64) -> String {
        use crate::i18n::t;
        let status = t(match self.status {
            SpanStatus::Unchanged => "conform.status.unchanged",
            SpanStatus::Moved => "conform.status.moved",
            SpanStatus::Modified => "conform.status.modified",
            SpanStatus::Truncated => "conform.status.truncated",
            SpanStatus::Deleted => "conform.status.deleted",
        });
        let kind = t(match self.kind {
            ConformKind::Line => "conform.kind.line",
            ConformKind::Marker => "conform.kind.marker",
            ConformKind::Detection => "conform.kind.detection",
            ConformKind::Stroke => "conform.kind.stroke",
            ConformKind::Take => "conform.kind.take",
        });
        let old = crate::project::format_timecode(self.old_start, fps);
        let new = self
            .new_start
            .map(|frame| crate::project::format_timecode(frame, new_fps))
            .unwrap_or_else(|| "—".to_string());
        let rerecord = if self.needs_rerecord() {
            format!(" [{}]", t("conform.status.rerecord"))
        } else {
            String::new()
        };
        format!("{status}{rerecord} · {kind} · {old} → {new} · {}", self.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TimeMap {
        // 0..100 stays, 100..150 is deleted, 150..300 shifts back by 50 and
        // 300..400 moves to the front of the film (new 400..500 → reordered).
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
            Segment {
                old_start: 300,
                old_end: 400,
                new_start: 400,
            },
        ])
    }

    #[test]
    fn frames_follow_their_segment() {
        let map = map();
        assert_eq!(map.map_frame(10), Some(10));
        assert_eq!(map.map_frame(120), None);
        assert_eq!(map.map_frame(160), Some(110));
        assert_eq!(map.map_frame(299), Some(249));
        assert_eq!(map.map_frame(300), Some(400));
        assert_eq!(map.map_frame(400), None);
        assert_eq!(map.map_position(160.4), Some(110.4));
    }

    #[test]
    fn spans_report_what_happened_to_them() {
        let map = map();
        assert_eq!(
            map.map_span(10, 20),
            SpanMapping {
                status: SpanStatus::Unchanged,
                new_start: Some(10),
                new_end: Some(20)
            }
        );
        assert_eq!(
            map.map_span(160, 180),
            SpanMapping {
                status: SpanStatus::Moved,
                new_start: Some(110),
                new_end: Some(130)
            }
        );
        assert_eq!(
            map.map_span(110, 140),
            SpanMapping {
                status: SpanStatus::Deleted,
                new_start: None,
                new_end: None
            }
        );
        // Starts in the kept part, ends in the deleted part.
        assert_eq!(
            map.map_span(90, 120),
            SpanMapping {
                status: SpanStatus::Truncated,
                new_start: Some(90),
                new_end: Some(100)
            }
        );
        // Straddles the deleted scene: both ends kept, middle gone.
        assert_eq!(
            map.map_span(90, 160),
            SpanMapping {
                status: SpanStatus::Truncated,
                new_start: Some(90),
                new_end: Some(110)
            }
        );
        // Crosses a cut whose second half moved to the end: reordered, so the
        // longest part wins.
        assert_eq!(
            map.map_span(280, 320),
            SpanMapping {
                status: SpanStatus::Truncated,
                new_start: Some(230),
                new_end: Some(250)
            }
        );
    }

    #[test]
    fn inserted_frames_inside_a_span_make_it_modified() {
        let map = TimeMap::new(vec![
            Segment {
                old_start: 0,
                old_end: 100,
                new_start: 0,
            },
            Segment {
                old_start: 100,
                old_end: 200,
                new_start: 130,
            },
        ]);
        assert_eq!(
            map.map_span(90, 110),
            SpanMapping {
                status: SpanStatus::Modified,
                new_start: Some(90),
                new_end: Some(140)
            }
        );
    }

    /// End-to-end check on real files, run by hand:
    /// `COQUERYTHMO_CONFORM_OLD=v1.mp4 COQUERYTHMO_CONFORM_NEW=v2.mp4 cargo test
    /// --lib conform::tests::real_videos -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_videos() {
        let old = std::env::var("COQUERYTHMO_CONFORM_OLD").expect("COQUERYTHMO_CONFORM_OLD");
        let new = std::env::var("COQUERYTHMO_CONFORM_NEW").expect("COQUERYTHMO_CONFORM_NEW");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let started = std::time::Instant::now();
        if std::env::var_os("COQUERYTHMO_CONFORM_DEBUG").is_some() {
            let probe = |path: &str| {
                let info = crate::video_proxy::probe_video(Path::new(path)).unwrap();
                signature::compute(Path::new(path), 25.0, info.duration_secs, false, &cancel, &|_| {})
                    .unwrap()
            };
            let a = probe(&old);
            let b = probe(&new);
            for frame in [10i64, 50, 100, 200, 240] {
                println!(
                    "frame {frame}: hamming {} thumb {:?} old {:?} new {:?}",
                    (a.hashes[frame as usize] ^ b.hashes[frame as usize]).count_ones(),
                    a.thumb_distance(frame, &b, frame),
                    a.thumb(frame).unwrap(),
                    b.thumb(frame).unwrap()
                );
            }
            let mut distinct = a.hashes.clone();
            distinct.sort_unstable();
            distinct.dedup();
            println!("{} distinct hashes out of {}", distinct.len(), a.hashes.len());
            for frame in [10usize, 50, 100, 200, 240] {
                let matches = b
                    .hashes
                    .iter()
                    .filter(|hash| (a.hashes[frame] ^ **hash).count_ones() <= 10)
                    .count();
                println!("frame {frame}: {matches} new frames within 10 bits");
            }
            for frame in [10i64, 50, 100] {
                let mut distances: Vec<u32> = (0..b.len())
                    .filter_map(|n| a.thumb_distance(frame, &b, n))
                    .collect();
                distances.sort_unstable();
                println!(
                    "frame {frame}: thumb distances best {:?}, median {}",
                    &distances[..5.min(distances.len())],
                    distances[distances.len() / 2]
                );
            }
            let checked = align::align(&a.hashes, &b.hashes, align::AlignParams::VIDEO, |o, n| {
                a.thumb_distance(o, &b, n).unwrap_or(u32::MAX)
            });
            println!("video alignment: {checked:?}");
        }
        let analysis = analyze(Path::new(&old), Path::new(&new), &cancel, &|_| {}).unwrap();
        println!(
            "{} old frames, {} new frames, {:.3} fps -> {:.3} fps, coverage {:.1}%, {} audio-only segments, {:?}",
            analysis.old_frames,
            analysis.new_frames,
            analysis.fps,
            analysis.new_fps,
            analysis.coverage * 100.0,
            analysis.audio_segments,
            started.elapsed()
        );
        for segment in &analysis.segments {
            println!(
                "old {}..{} -> new {}..{} (offset {:+})",
                segment.old_start,
                segment.old_end,
                segment.new_start,
                segment.new_end(),
                segment.offset()
            );
        }
        assert!(!analysis.segments.is_empty());
    }

    /// Writes a small project on `COQUERYTHMO_CONFORM_OLD` to
    /// `COQUERYTHMO_DEMO_OUT`, to try the window in the application by hand.
    #[test]
    #[ignore]
    fn write_demo_project() {
        use crate::rythmo_line::{MarkerKind, RythmoLine, RythmoMarker};
        let video = std::env::var("COQUERYTHMO_CONFORM_OLD").expect("COQUERYTHMO_CONFORM_OLD");
        let out = std::env::var("COQUERYTHMO_DEMO_OUT").expect("COQUERYTHMO_DEMO_OUT");
        let fps = 25.0;
        let mut project = crate::project::Project::new();
        let line = |id: u64, start: i64, duration: i64, text: &str, note: &str| RythmoLine {
            id,
            start_frame: start,
            duration_frames: duration,
            y_slot: 0.0,
            text: text.into(),
            character_name: "Alice".into(),
            character_color: [0.9, 0.5, 0.3, 1.0],
            kind: Default::default(),
            voice_actor_names: Vec::new(),
            syllable_ratios: Vec::new(),
            karaoke: false,
            note: note.into(),
            presence: Default::default(),
            text_emotions: Vec::new(),
            text_styles: Vec::new(),
        };
        // 0..250 unchanged, 250..350 deleted, 350..625 shifted, 625.. shifted more.
        project.insert_line_at(0, line(1, 50, 40, "Inchangée", "garder le ton"));
        project.insert_line_at(1, line(2, 270, 40, "Coupée au montage", ""));
        project.insert_line_at(2, line(3, 400, 50, "Décalée de quatre secondes", "commentaire déplacé"));
        project.insert_line_at(3, line(4, 230, 60, "À cheval sur la coupe", ""));
        project.insert_line_at(4, line(5, 700, 50, "Après le plan inséré", ""));
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 100,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::Boucle,
            frame: 300,
        });
        project.add_marker(RythmoMarker {
            kind: MarkerKind::SceneChange,
            frame: 500,
        });
        project.add_media_video("v1", &video, None, false).unwrap();
        crate::project_archive::save_bundle(
            &project,
            fps,
            Path::new(&out),
            Path::new(&video),
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    fn audio_fills_only_the_holes_left_by_the_pictures() {
        let primary = vec![Segment {
            old_start: 0,
            old_end: 100,
            new_start: 0,
        }];
        let secondary = vec![Segment {
            old_start: 50,
            old_end: 200,
            new_start: 50,
        }];
        assert_eq!(
            fill_gaps(&primary, secondary, 12),
            vec![Segment {
                old_start: 100,
                old_end: 200,
                new_start: 100
            }]
        );
    }
}
