//! Alignment of two fingerprint sequences (one per frame of each video).
//!
//! Every frame of both cuts is reduced to a small hash. Frames of the old cut
//! that reappear in the new cut keep the same hash up to a few bits, and
//! consecutive frames keep the same offset between the two cuts, so a shared
//! shot shows up as a diagonal run of matches. The alignment collects those
//! runs into non-overlapping [`Segment`]s.

use std::collections::HashMap;

/// A run of old frames that reappears, unchanged and contiguous, in the new
/// cut. `old_end` is exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub old_start: i64,
    pub old_end: i64,
    pub new_start: i64,
}

impl Segment {
    pub fn offset(&self) -> i64 {
        self.new_start - self.old_start
    }

    pub fn len(&self) -> i64 {
        self.old_end - self.old_start
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= 0
    }

    pub fn new_end(&self) -> i64 {
        self.new_start + self.len()
    }

    pub fn contains_old(&self, frame: i64) -> bool {
        frame >= self.old_start && frame < self.old_end
    }

    pub fn contains_new(&self, frame: i64) -> bool {
        frame >= self.new_start && frame < self.new_end()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AlignParams {
    /// Hamming distance under which two hashes count as the same frame.
    pub max_distance: u32,
    /// Shortest run of matching frames worth keeping, in frames.
    pub min_run: i64,
    /// Longest gap of unmatched frames tolerated inside a run.
    pub max_gap: i64,
    /// An old frame matching more new frames than this is a still picture
    /// (black, a title card) and does not vote: [`refine`] carries the
    /// neighbouring diagonal through it afterwards. Below that, a frame found
    /// in several places shares its vote between them.
    pub max_candidates: usize,
    /// Hash buckets larger than this are skipped outright, to bound the work
    /// on long stretches of identical frames.
    pub max_bucket: usize,
    /// Largest picture distance (see [`align`]) of a usable candidate.
    pub max_picture_distance: u32,
    /// A candidate is kept when its picture distance is within this margin of
    /// the best candidate's (plus half of that best distance).
    pub picture_slack: u32,
}

impl AlignParams {
    pub const VIDEO: Self = Self {
        max_distance: 10,
        min_run: 6,
        max_gap: 3,
        max_candidates: 64,
        max_bucket: 4096,
        // Mean of 6 grey levels over a 16×16 thumbnail.
        max_picture_distance: 6 * 256,
        picture_slack: 256,
    };

    pub const AUDIO: Self = Self {
        max_distance: 4,
        min_run: 12,
        max_gap: 4,
        max_candidates: 64,
        max_bucket: 4096,
        max_picture_distance: u32::MAX,
        picture_slack: u32::MAX,
    };
}

/// Index of the new cut: one table per 16-bit chunk of the hash. Two hashes
/// within 3 bits of each other share at least one chunk exactly.
struct ChunkIndex {
    tables: [HashMap<u16, Vec<u32>>; 4],
}

impl ChunkIndex {
    fn build(hashes: &[u64]) -> Self {
        let mut tables: [HashMap<u16, Vec<u32>>; 4] = Default::default();
        for (index, hash) in hashes.iter().enumerate() {
            for (chunk, table) in tables.iter_mut().enumerate() {
                let key = ((hash >> (chunk * 16)) & 0xffff) as u16;
                table.entry(key).or_default().push(index as u32);
            }
        }
        Self { tables }
    }

    fn candidates(&self, hash: u64, max_bucket: usize, out: &mut Vec<u32>) {
        out.clear();
        for (chunk, table) in self.tables.iter().enumerate() {
            let key = ((hash >> (chunk * 16)) & 0xffff) as u16;
            if let Some(bucket) = table.get(&key) {
                if bucket.len() <= max_bucket {
                    out.extend_from_slice(bucket);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// Finds the shots of `old` that reappear in `new`.
///
/// Both slices hold one hash per frame. `picture_distance(old_frame,
/// new_frame)` refines a hash match with whatever finer data exists: the
/// thumbnail difference for pictures, 0 for sound. Only the candidates close
/// to the best one keep voting. The result is sorted by `old_start` and its
/// segments overlap neither in the old cut nor in the new one.
pub fn align(
    old: &[u64],
    new: &[u64],
    params: AlignParams,
    picture_distance: impl Fn(i64, i64) -> u32,
) -> Vec<Segment> {
    if old.is_empty() || new.is_empty() {
        return Vec::new();
    }
    let index = ChunkIndex::build(new);
    let mut votes: HashMap<i64, Vec<(u32, f32)>> = HashMap::new();
    let mut candidates = Vec::new();
    let mut scored: Vec<(u32, u32)> = Vec::new();
    let mut matches: Vec<u32> = Vec::new();
    for (i, hash) in old.iter().enumerate() {
        index.candidates(*hash, params.max_bucket, &mut candidates);
        scored.clear();
        for &j in &candidates {
            if (hash ^ new[j as usize]).count_ones() <= params.max_distance {
                let distance = picture_distance(i as i64, j as i64);
                if distance <= params.max_picture_distance {
                    scored.push((distance, j));
                }
            }
        }
        let Some(best) = scored.iter().map(|(distance, _)| *distance).min() else {
            continue;
        };
        let limit = best
            .saturating_add(best / 2)
            .saturating_add(params.picture_slack);
        matches.clear();
        matches.extend(
            scored
                .iter()
                .filter(|(distance, _)| *distance <= limit)
                .map(|(_, j)| *j),
        );
        // Random hash collisions are rarer than one per film: a frame found
        // in many places is a still picture, which says nothing about the
        // offset. A frame found in a few places shares its vote.
        if matches.is_empty() || matches.len() > params.max_candidates {
            continue;
        }
        let weight = 1.0 / matches.len() as f32;
        for &j in &matches {
            votes
                .entry(j as i64 - i as i64)
                .or_default()
                .push((i as u32, weight));
        }
    }

    // Diagonal runs: for each offset, consecutive old frames that voted.
    let min_score = params.min_run as f32 * 0.5;
    let mut runs: Vec<(f32, Segment)> = Vec::new();
    for (offset, frames) in votes.iter_mut() {
        frames.sort_unstable_by_key(|(frame, _)| *frame);
        let mut start = frames[0].0 as i64;
        let mut last = start;
        let mut score = 0.0f32;
        for &(frame, weight) in frames.iter() {
            let frame = frame as i64;
            if frame - last > params.max_gap + 1 {
                push_run(&mut runs, start, last, *offset, score, params.min_run, min_score);
                start = frame;
                score = 0.0;
            }
            last = frame;
            score += weight;
        }
        push_run(&mut runs, start, last, *offset, score, params.min_run, min_score);
    }

    // Best-supported runs first; later runs only keep the frames nobody
    // claimed, in the old cut as well as in the new one.
    runs.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.len().cmp(&a.1.len()))
            .then(a.1.old_start.cmp(&b.1.old_start))
    });
    let mut accepted: Vec<Segment> = Vec::new();
    for (_, run) in runs {
        let mut free = vec![(run.old_start, run.old_end)];
        for taken in &accepted {
            free = subtract(&free, taken.old_start, taken.old_end);
            // The new-cut footprint of `taken`, expressed in old frames of `run`.
            let shift = taken.new_start - run.offset();
            free = subtract(&free, shift, shift + taken.len());
            if free.is_empty() {
                break;
            }
        }
        for (start, end) in free {
            if end - start >= params.min_run {
                accepted.push(Segment {
                    old_start: start,
                    old_end: end,
                    new_start: start + run.offset(),
                });
            }
        }
    }
    accepted.sort_by_key(|segment| segment.old_start);
    merge_adjacent(accepted)
}

#[allow(clippy::too_many_arguments)]
fn push_run(
    runs: &mut Vec<(f32, Segment)>,
    start: i64,
    last: i64,
    offset: i64,
    score: f32,
    min_run: i64,
    min_score: f32,
) {
    let end = last + 1;
    if end - start >= min_run && score >= min_score {
        runs.push((
            score,
            Segment {
                old_start: start,
                old_end: end,
                new_start: start + offset,
            },
        ));
    }
}

/// Removes `[start, end)` from every interval.
fn subtract(intervals: &[(i64, i64)], start: i64, end: i64) -> Vec<(i64, i64)> {
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

/// Joins segments that continue each other with the same offset.
pub fn merge_adjacent(segments: Vec<Segment>) -> Vec<Segment> {
    let mut merged: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        if let Some(last) = merged.last_mut() {
            if last.old_end == segment.old_start && last.offset() == segment.offset() {
                last.old_end = segment.old_end;
                continue;
            }
        }
        merged.push(segment);
    }
    merged
}

/// Grows every segment frame by frame, at both ends, while `similar(old,
/// new)` holds and no neighbour owns the frame. Fills the gaps that the hash
/// index could not vote on (very dark frames, dissolves).
pub fn refine(
    mut segments: Vec<Segment>,
    old_len: i64,
    new_len: i64,
    max_gap: i64,
    similar: impl Fn(i64, i64) -> bool,
) -> Vec<Segment> {
    segments.sort_by_key(|segment| segment.old_start);
    let count = segments.len();
    for index in 0..count {
        let offset = segments[index].offset();
        // Backwards.
        let old_floor = if index == 0 {
            0
        } else {
            segments[index - 1].old_end
        };
        let mut gap = 0;
        while segments[index].old_start > old_floor {
            let old = segments[index].old_start - 1;
            let new = old + offset;
            if new < 0 || new >= new_len || new_taken(&segments, index, new) {
                break;
            }
            if similar(old, new) {
                gap = 0;
            } else if gap < max_gap {
                gap += 1;
            } else {
                break;
            }
            segments[index].old_start = old;
            segments[index].new_start = new;
        }
        if gap > 0 {
            segments[index].old_start += gap;
            segments[index].new_start += gap;
        }
        // Forwards.
        let old_ceiling = if index + 1 == count {
            old_len
        } else {
            segments[index + 1].old_start
        };
        let mut gap = 0;
        while segments[index].old_end < old_ceiling {
            let old = segments[index].old_end;
            let new = old + offset;
            if new < 0 || new >= new_len || new_taken(&segments, index, new) {
                break;
            }
            if similar(old, new) {
                gap = 0;
            } else if gap < max_gap {
                gap += 1;
            } else {
                break;
            }
            segments[index].old_end = old + 1;
        }
        segments[index].old_end -= gap;
    }
    merge_adjacent(segments)
}

fn new_taken(segments: &[Segment], except: usize, new: i64) -> bool {
    segments
        .iter()
        .enumerate()
        .any(|(index, segment)| index != except && segment.contains_new(new))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashes(seed: u64, count: usize) -> Vec<u64> {
        // Distinct, well-spread hashes: a cheap xorshift.
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            })
            .collect()
    }

    fn flip_bits(hash: u64, bits: u32) -> u64 {
        let mut flipped = hash;
        for bit in 0..bits {
            flipped ^= 1 << (bit * 7 % 64);
        }
        flipped
    }

    #[test]
    fn identical_cuts_give_one_segment() {
        let old = hashes(1, 200);
        let segments = align(&old, &old, AlignParams::VIDEO, |_, _| 0);
        assert_eq!(
            segments,
            vec![Segment {
                old_start: 0,
                old_end: 200,
                new_start: 0
            }]
        );
    }

    #[test]
    fn inserted_scene_shifts_the_rest() {
        let old = hashes(2, 300);
        let inserted = hashes(99, 48);
        let mut new = old[..100].to_vec();
        new.extend_from_slice(&inserted);
        new.extend_from_slice(&old[100..]);
        let segments = align(&old, &new, AlignParams::VIDEO, |_, _| 0);
        assert_eq!(
            segments,
            vec![
                Segment {
                    old_start: 0,
                    old_end: 100,
                    new_start: 0
                },
                Segment {
                    old_start: 100,
                    old_end: 300,
                    new_start: 148
                },
            ]
        );
    }

    #[test]
    fn deleted_scene_leaves_a_hole_in_the_old_cut() {
        let old = hashes(3, 300);
        let mut new = old[..100].to_vec();
        new.extend_from_slice(&old[160..]);
        let segments = align(&old, &new, AlignParams::VIDEO, |_, _| 0);
        assert_eq!(
            segments,
            vec![
                Segment {
                    old_start: 0,
                    old_end: 100,
                    new_start: 0
                },
                Segment {
                    old_start: 160,
                    old_end: 300,
                    new_start: 100
                },
            ]
        );
    }

    #[test]
    fn reordered_scenes_are_both_found() {
        let old = hashes(4, 300);
        let mut new = old[150..].to_vec();
        new.extend_from_slice(&old[..150]);
        let segments = align(&old, &new, AlignParams::VIDEO, |_, _| 0);
        assert_eq!(
            segments,
            vec![
                Segment {
                    old_start: 0,
                    old_end: 150,
                    new_start: 150
                },
                Segment {
                    old_start: 150,
                    old_end: 300,
                    new_start: 0
                },
            ]
        );
    }

    #[test]
    fn re_encoding_noise_is_tolerated() {
        let old = hashes(5, 200);
        let new: Vec<u64> = old
            .iter()
            .enumerate()
            .map(|(index, hash)| flip_bits(*hash, (index % 4) as u32))
            .collect();
        let segments = align(&old, &new, AlignParams::VIDEO, |_, _| 0);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].old_start, 0);
        assert_eq!(segments[0].old_end, 200);
        assert_eq!(segments[0].offset(), 0);
    }

    #[test]
    fn static_shots_do_not_break_the_diagonal() {
        let mut old = hashes(6, 100);
        old.extend(std::iter::repeat(0x5555_5555_5555_5555).take(80));
        old.extend(hashes(7, 100));
        let mut new = hashes(8, 30);
        new.extend_from_slice(&old);
        let segments = align(&old, &new, AlignParams::VIDEO, |_, _| 0);
        // The still picture votes for nothing: both sides of it are found.
        assert_eq!(
            segments,
            vec![
                Segment {
                    old_start: 0,
                    old_end: 100,
                    new_start: 30
                },
                Segment {
                    old_start: 180,
                    old_end: 280,
                    new_start: 210
                },
            ]
        );
        // Growing the runs through the identical frames joins them.
        let refined = refine(segments, 280, 310, 0, |o, n| old[o as usize] == new[n as usize]);
        assert_eq!(
            refined,
            vec![Segment {
                old_start: 0,
                old_end: 280,
                new_start: 30
            }]
        );
    }

    #[test]
    fn short_coincidences_are_ignored() {
        let old = hashes(9, 100);
        let mut new = hashes(10, 100);
        new[40] = old[10];
        new[41] = old[11];
        assert!(align(&old, &new, AlignParams::VIDEO, |_, _| 0).is_empty());
    }

    #[test]
    fn refine_extends_segments_through_similar_frames() {
        let segments = vec![Segment {
            old_start: 10,
            old_end: 20,
            new_start: 15,
        }];
        let refined = refine(segments, 40, 50, 0, |old, new| new - old == 5 && old < 30);
        assert_eq!(
            refined,
            vec![Segment {
                old_start: 0,
                old_end: 30,
                new_start: 5
            }]
        );
    }

    #[test]
    fn refine_never_crosses_a_neighbour() {
        let segments = vec![
            Segment {
                old_start: 0,
                old_end: 10,
                new_start: 0,
            },
            Segment {
                old_start: 20,
                old_end: 30,
                new_start: 40,
            },
        ];
        let refined = refine(segments, 40, 60, 0, |_, _| true);
        assert_eq!(
            refined,
            vec![
                Segment {
                    old_start: 0,
                    old_end: 20,
                    new_start: 0
                },
                Segment {
                    old_start: 20,
                    old_end: 40,
                    new_start: 40
                },
            ]
        );
    }

    #[test]
    fn subtract_splits_intervals() {
        assert_eq!(subtract(&[(0, 10)], 3, 6), vec![(0, 3), (6, 10)]);
        assert_eq!(subtract(&[(0, 10)], 0, 10), Vec::<(i64, i64)>::new());
        assert_eq!(subtract(&[(0, 10)], 10, 20), vec![(0, 10)]);
        assert_eq!(subtract(&[(0, 10)], -5, 4), vec![(4, 10)]);
    }
}
