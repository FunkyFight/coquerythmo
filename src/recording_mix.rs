//! Recording-workspace mix specification and adapters.
//!
//! Preview playback uses cached PCM assets in the existing CPAL callback.
//! The FFmpeg filter graph remains available for final/offline rendering.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::recording::{AudioAssetId, AudioClipId, AudioTrackId, RecordingProject};

static MIX_TEMP_NONCE: AtomicU64 = AtomicU64::new(0);
pub const REALTIME_SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone, PartialEq)]
pub struct MixClip {
    pub clip_id: AudioClipId,
    pub track_id: AudioTrackId,
    pub path: PathBuf,
    pub source_start_seconds: f64,
    pub duration_seconds: f64,
    pub timeline_start_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordingMixSpec {
    /// Duration of silent source audio (e.g. video duration) in seconds.
    /// If set, this adds a silent source of this duration to the mix,
    /// ensuring the output has at least this duration.
    pub source_duration_seconds: Option<f64>,
    pub clips: Vec<MixClip>,
    pub sample_rate: u32,
    pub source_volume: f32,
    /// If set, the output will be padded with silence to this duration.
    pub total_duration_seconds: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RealtimeRecordingMix {
    clips: Vec<RealtimeMixClip>,
    source_volume: f32,
    output_sample_rate: u32,
}

#[derive(Debug, Clone)]
struct RealtimeMixClip {
    samples: Arc<Vec<f32>>,
    source_start_seconds: f64,
    duration_seconds: f64,
    timeline_start_seconds: f64,
}

impl RealtimeRecordingMix {
    pub fn from_spec(
        spec: &RecordingMixSpec,
        cache: &BTreeMap<PathBuf, Arc<Vec<f32>>>,
        output_sample_rate: u32,
    ) -> Result<Self, String> {
        let clips = spec
            .clips
            .iter()
            .map(|clip| {
                Ok(RealtimeMixClip {
                    samples: cache.get(&clip.path).cloned().ok_or_else(|| {
                        format!("audio asset is not decoded: {}", clip.path.display())
                    })?,
                    source_start_seconds: clip.source_start_seconds,
                    duration_seconds: clip.duration_seconds,
                    timeline_start_seconds: clip.timeline_start_seconds,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            clips,
            source_volume: spec.source_volume,
            output_sample_rate,
        })
    }

    pub fn mix_stereo(&self, timeline_seconds: f64, source: [f32; 2]) -> [f32; 2] {
        let mut mixed = [
            source[0] * self.source_volume,
            source[1] * self.source_volume,
        ];
        let clip_rate = REALTIME_SAMPLE_RATE as f64;
        for clip in &self.clips {
            let clip_seconds = timeline_seconds - clip.timeline_start_seconds;
            if clip_seconds < 0.0 || clip_seconds >= clip.duration_seconds {
                continue;
            }
            let source_seconds = clip.source_start_seconds + clip_seconds;
            let sample_f = source_seconds * clip_rate;
            let sample = sample_f.floor() as usize;
            let frac = sample_f - sample as f64;
            let index = sample.saturating_mul(2);
            if let Some(stereo) = clip.samples.get(index..index + 4) {
                let left = stereo[0] * (1.0 - frac) as f32 + stereo[2] * frac as f32;
                let right = stereo[1] * (1.0 - frac) as f32 + stereo[3] * frac as f32;
                mixed[0] += left;
                mixed[1] += right;
            }
        }
        mixed
    }
}

pub fn decode_realtime_asset(path: &Path, cancel: &AtomicBool) -> Result<Arc<Vec<f32>>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("recording mix cancelled".into());
    }
    // ponytail: recordings are decoded whole once; stream/chunk them if long-form assets
    // make memory use measurable.
    let output = crate::media_binary::command("ffmpeg")
        .args(["-threads", "1", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ar",
            &REALTIME_SAMPLE_RATE.to_string(),
            "-ac",
            "2",
            "pipe:1",
        ])
        .output()
        .map_err(|error| format!("cannot decode recording asset: {error}"))?;
    if cancel.load(Ordering::Relaxed) {
        return Err("recording mix cancelled".into());
    }
    if !output.status.success() {
        return Err(format!(
            "cannot decode recording asset {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() % 4 != 0 {
        return Err(format!("invalid PCM data for {}", path.display()));
    }
    Ok(Arc::new(
        output
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte PCM sample")))
            .collect(),
    ))
}

impl RecordingMixSpec {
    pub fn from_project(
        project: &RecordingProject,
        asset_paths: &BTreeMap<AudioAssetId, PathBuf>,
        source: Option<PathBuf>,
        total_duration_seconds: Option<f64>,
    ) -> Result<Self, String> {
        project.validate().map_err(|error| error.to_string())?;
        let fps = project.timeline_fps();
        if !fps.is_finite() || fps <= 0.0 {
            return Err("invalid recording timeline FPS".into());
        }
        let mut clips = Vec::new();
        for clip in project.clips() {
            if !project
                .is_track_audible(clip.track_id)
                .map_err(|error| error.to_string())?
            {
                continue;
            }
            let path = asset_paths
                .get(&clip.asset_id)
                .cloned()
                .ok_or_else(|| format!("missing FLAC path for asset {}", clip.asset_id))?;
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
            {
                return Err(format!(
                    "audio asset {} does not resolve to a FLAC file",
                    clip.asset_id
                ));
            }
            clips.push(MixClip {
                clip_id: clip.id,
                track_id: clip.track_id,
                path,
                source_start_seconds: clip.source_start_frame as f64 / fps,
                duration_seconds: clip.duration_frames as f64 / fps,
                timeline_start_seconds: clip.start_frame as f64 / fps,
            });
        }
        clips.sort_by(|left, right| {
            left.timeline_start_seconds
                .total_cmp(&right.timeline_start_seconds)
                .then_with(|| left.clip_id.cmp(&right.clip_id))
        });
        Ok(Self {
            source_duration_seconds: None,
            clips,
            sample_rate: 48_000,
            source_volume: 1.0,
            total_duration_seconds,
        })
    }

    pub fn set_source_volume(&mut self, volume: f32) {
        self.source_volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    pub fn is_empty(&self) -> bool {
        self.source_duration_seconds.is_none() && self.clips.is_empty()
    }

    pub fn ffmpeg_filter(&self) -> Result<String, String> {
        if self.is_empty() {
            return Err("recording mix has no audible input".into());
        }
        if self.sample_rate == 0 {
            return Err("recording mix sample rate must be positive".into());
        }
        let mut filters = Vec::new();
        let mut labels = Vec::new();
        let mut input_index = 0usize;
        if let Some(source_duration) = self.source_duration_seconds {
            if source_duration.is_finite() && source_duration > 0.0 {
                filters.push(format!(
                    "anullsrc=r={}:cl=stereo:duration={:.9}[source]",
                    self.sample_rate, source_duration
                ));
                labels.push("[source]".to_string());
            }
        }
        for (clip_index, clip) in self.clips.iter().enumerate() {
            if !clip.source_start_seconds.is_finite()
                || !clip.duration_seconds.is_finite()
                || !clip.timeline_start_seconds.is_finite()
                || clip.source_start_seconds < 0.0
                || clip.duration_seconds <= 0.0
                || clip.timeline_start_seconds < 0.0
            {
                return Err(format!("invalid timing for clip {}", clip.clip_id));
            }
            let label = format!("clip{clip_index}");
            let delay_samples =
                (clip.timeline_start_seconds * f64::from(self.sample_rate)).round() as u64;
            filters.push(format!(
                "[{input_index}:a]aresample={},atrim=start={:.9}:duration={:.9},asetpts=PTS-STARTPTS,adelay={delay_samples}S:all=1[{label}]",
                self.sample_rate, clip.source_start_seconds, clip.duration_seconds
            ));
            labels.push(format!("[{label}]"));
            input_index += 1;
        }
        filters.push(format!(
            "{}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0[mix]",
            labels.concat(),
            labels.len()
        ));
        if let Some(total_duration) = self.total_duration_seconds {
            filters.push(format!("[mix]apad=whole_dur={:.9}[padded]", total_duration));
            filters.push("[padded]anull[mix]".to_string());
        }
        Ok(filters.join(";"))
    }

    pub fn ffmpeg_args(&self, output: &Path) -> Result<Vec<String>, String> {
        let mut args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-n".into(),
        ];
        for clip in &self.clips {
            args.push("-i".into());
            args.push(clip.path.to_string_lossy().into_owned());
        }
        args.extend([
            "-filter_complex".into(),
            self.ffmpeg_filter()?,
            "-map".into(),
            "[mix]".into(),
            "-c:a".into(),
            "flac".into(),
            "-ar".into(),
            self.sample_rate.to_string(),
            output.to_string_lossy().into_owned(),
        ]);
        Ok(args)
    }
}

/// Render through a sibling temporary file so readers never observe a partial
/// preview. The worker polls cancellation while FFmpeg is running.
pub fn render_recording_mix(
    spec: &RecordingMixSpec,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("recording mix cancelled".into());
    }
    if !output
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
    {
        return Err("recording mix output must use the .flac extension".into());
    }
    let parent = output
        .parent()
        .ok_or_else(|| "recording mix output has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create recording mix directory: {error}"))?;
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let nonce = MIX_TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.{}-{nonce}.tmp.flac", std::process::id()));
    let args = spec.ffmpeg_args(&temporary)?;
    let mut child = crate::media_binary::command("ffmpeg")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start FFmpeg recording mix: {error}"))?;
    let stderr = child.stderr.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        "FFmpeg recording mix has no error stream".to_string()
    })?;
    let stderr_reader = match thread::Builder::new()
        .name("recording-mix-stderr".into())
        .spawn(move || {
            let mut stderr = stderr;
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        }) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("cannot monitor FFmpeg recording mix: {error}"));
        }
    };

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.join();
            let _ = std::fs::remove_file(&temporary);
            return Err("recording mix cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                let _ = std::fs::remove_file(&temporary);
                return Err(format!("cannot poll FFmpeg recording mix: {error}"));
            }
        }
    };
    let stderr = stderr_reader
        .join()
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&temporary);
        let detail = String::from_utf8_lossy(&stderr);
        return Err(format!("FFmpeg recording mix failed: {}", detail.trim()));
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&temporary);
        return Err("recording mix cancelled".into());
    }
    if output.exists() {
        if let Err(error) = std::fs::remove_file(output) {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("cannot replace previous recording mix: {error}"));
        }
    }
    if let Err(error) = std::fs::rename(&temporary, output) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot install recording mix: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{AudioAsset, AudioClip, AudioTrack, RecordingOperation, WaveformData};

    #[test]
    fn filter_places_clips_at_their_timeline_offsets() {
        let spec = RecordingMixSpec {
            source_duration_seconds: None,
            clips: vec![MixClip {
                clip_id: AudioClipId::new(3),
                track_id: AudioTrackId::new(1),
                path: PathBuf::from("voice.flac"),
                source_start_seconds: 0.5,
                duration_seconds: 2.0,
                timeline_start_seconds: 1.25,
            }],
            sample_rate: 48_000,
            source_volume: 1.0,
            total_duration_seconds: None,
        };
        let filter = spec.ffmpeg_filter().unwrap();
        assert!(filter.contains("atrim=start=0.500000000:duration=2.000000000"));
        assert!(filter.contains("adelay=60000S:all=1"));
        assert!(filter.contains("amix=inputs=1"));
    }

    #[test]
    fn silent_source_does_not_shift_file_input_indexes() {
        let spec = RecordingMixSpec {
            source_duration_seconds: Some(10.0),
            clips: vec![MixClip {
                clip_id: AudioClipId::new(3),
                track_id: AudioTrackId::new(1),
                path: PathBuf::from("voice.flac"),
                source_start_seconds: 0.0,
                duration_seconds: 2.0,
                timeline_start_seconds: 1.0,
            }],
            sample_rate: 48_000,
            source_volume: 1.0,
            total_duration_seconds: Some(10.0),
        };

        let filter = spec.ffmpeg_filter().unwrap();
        assert!(filter.contains("anullsrc=r=48000:cl=stereo:duration=10.000000000[source]"));
        assert!(filter.contains("[0:a]aresample=48000"));
        assert!(!filter.contains("[1:a]aresample=48000"));
    }

    #[test]
    fn realtime_mix_reads_clip_at_its_timeline_position() {
        let mut cache = BTreeMap::new();
        cache.insert(
            PathBuf::from("voice.flac"),
            Arc::new(vec![0.25, -0.5, 0.75, 0.5]),
        );
        let mix = RealtimeRecordingMix::from_spec(
            &RecordingMixSpec {
                source_duration_seconds: None,
                clips: vec![MixClip {
                    clip_id: AudioClipId::new(3),
                    track_id: AudioTrackId::new(1),
                    path: PathBuf::from("voice.flac"),
                    source_start_seconds: 0.0,
                    duration_seconds: 2.0 / f64::from(REALTIME_SAMPLE_RATE),
                    timeline_start_seconds: 1.0,
                }],
                sample_rate: REALTIME_SAMPLE_RATE,
                source_volume: 0.5,
                total_duration_seconds: None,
            },
            &cache,
            REALTIME_SAMPLE_RATE,
        )
        .unwrap();

        assert_eq!(mix.mix_stereo(0.5, [1.0, 1.0]), [0.5, 0.5]);
        assert_eq!(mix.mix_stereo(1.0, [1.0, 1.0]), [0.75, 0.0]);
    }

    #[test]
    fn realtime_mix_interpolates_at_different_sample_rate() {
        let mut cache = BTreeMap::new();
        // 4 stereo frames: [L0, R0, L1, R1, L2, R2, L3, R3]
        cache.insert(
            PathBuf::from("voice.flac"),
            Arc::new(vec![0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0]),
        );
        let mix = RealtimeRecordingMix::from_spec(
            &RecordingMixSpec {
                source_duration_seconds: None,
                clips: vec![MixClip {
                    clip_id: AudioClipId::new(1),
                    track_id: AudioTrackId::new(1),
                    path: PathBuf::from("voice.flac"),
                    source_start_seconds: 0.0,
                    duration_seconds: 4.0 / f64::from(REALTIME_SAMPLE_RATE),
                    timeline_start_seconds: 0.0,
                }],
                sample_rate: REALTIME_SAMPLE_RATE,
                source_volume: 1.0,
                total_duration_seconds: None,
            },
            &cache,
            44_100, // Different output sample rate
        )
        .unwrap();

        // At t=0, should get first frame [0.0, 0.0]
        let out = mix.mix_stereo(0.0, [0.0, 0.0]);
        assert!((out[0] - 0.0).abs() < 1e-5);
        assert!((out[1] - 0.0).abs() < 1e-5);

        // At t=1/44100, sample_f = 48000/44100 ≈ 1.088
        // sample = 1, frac = 0.088, reads frame 1 [1,1] and frame 2 [0,0]
        // left = 1.0 * (1 - 0.088) + 0.0 * 0.088 ≈ 0.912
        let out = mix.mix_stereo(1.0 / 44_100.0, [0.0, 0.0]);
        let expected = 1.0 - (48_000.0 / 44_100.0 - 1.0); // ~0.912
        assert!((out[0] - expected).abs() < 0.02);
        assert!((out[1] - expected).abs() < 0.02);
    }

    fn add_voice_clip(
        project: &mut RecordingProject,
        track_id: AudioTrackId,
        asset_id: AudioAssetId,
        clip_id: AudioClipId,
        name: &str,
        start_frame: i64,
        source_start_frame: i64,
        duration_frames: i64,
    ) {
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(track_id, name),
                    },
                    RecordingOperation::AddAsset {
                        asset: AudioAsset {
                            id: asset_id,
                            file_name: format!("voice-{}.flac", asset_id.get()),
                            sample_rate: 48_000,
                            channels: 1,
                            sample_count: 96_000,
                            checksum: format!("{:040x}", asset_id.get()),
                            waveform: WaveformData::default(),
                        },
                    },
                    RecordingOperation::AddClip {
                        clip: AudioClip {
                            id: clip_id,
                            asset_id,
                            track_id,
                            start_frame,
                            source_start_frame,
                            duration_frames,
                        },
                    },
                ],
            })
            .unwrap();
    }

    #[test]
    fn project_frames_become_exact_trim_and_placement_times() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = AudioTrackId::new(1);
        let asset_id = AudioAssetId::new(2);
        let clip_id = AudioClipId::new(3);
        add_voice_clip(
            &mut project,
            track_id,
            asset_id,
            clip_id,
            "Voice",
            30,
            12,
            24,
        );
        let paths = BTreeMap::from([(asset_id, PathBuf::from("voice-2.flac"))]);
        let spec = RecordingMixSpec::from_project(&project, &paths, None, None).unwrap();
        assert_eq!(spec.clips.len(), 1);
        assert_eq!(spec.clips[0].source_start_seconds, 0.5);
        assert_eq!(spec.clips[0].duration_seconds, 1.0);
        assert_eq!(spec.clips[0].timeline_start_seconds, 1.25);
        assert!(spec.ffmpeg_filter().unwrap().contains("adelay=60000S"));
    }

    #[test]
    fn solo_and_mute_rules_are_preserved_in_the_mix() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let one = AudioTrackId::new(1);
        let two = AudioTrackId::new(4);
        add_voice_clip(
            &mut project,
            one,
            AudioAssetId::new(2),
            AudioClipId::new(3),
            "One",
            0,
            0,
            24,
        );
        add_voice_clip(
            &mut project,
            two,
            AudioAssetId::new(5),
            AudioClipId::new(6),
            "Two",
            0,
            0,
            24,
        );
        project
            .apply(&RecordingOperation::SetTrackSolo {
                track_id: two,
                solo: true,
            })
            .unwrap();
        let paths = BTreeMap::from([
            (AudioAssetId::new(2), PathBuf::from("voice-2.flac")),
            (AudioAssetId::new(5), PathBuf::from("voice-5.flac")),
        ]);
        let spec = RecordingMixSpec::from_project(&project, &paths, None, None).unwrap();
        assert_eq!(
            spec.clips
                .iter()
                .map(|clip| clip.track_id)
                .collect::<Vec<_>>(),
            vec![two]
        );

        project
            .apply(&RecordingOperation::SetTrackMuted {
                track_id: two,
                muted: true,
            })
            .unwrap();
        let spec = RecordingMixSpec::from_project(
            &project,
            &paths,
            Some(PathBuf::from("source.mp4")),
            None,
        )
        .unwrap();
        assert!(spec.clips.is_empty());
    }

    #[test]
    fn audible_assets_require_a_flac_path() {
        let mut project = RecordingProject::new(24.0).unwrap();
        add_voice_clip(
            &mut project,
            AudioTrackId::new(1),
            AudioAssetId::new(2),
            AudioClipId::new(3),
            "Voice",
            0,
            0,
            24,
        );
        assert!(
            RecordingMixSpec::from_project(&project, &BTreeMap::new(), None, None)
                .unwrap_err()
                .contains("missing FLAC path")
        );
        let wrong_path = BTreeMap::from([(AudioAssetId::new(2), PathBuf::from("voice.wav"))]);
        assert!(
            RecordingMixSpec::from_project(&project, &wrong_path, None, None)
                .unwrap_err()
                .contains("does not resolve to a FLAC")
        );
    }

    #[test]
    fn muted_tracks_are_not_part_of_the_mix_spec() {
        let mut project = RecordingProject::new(24.0).unwrap();
        let track_id = AudioTrackId::new(1);
        let asset_id = AudioAssetId::new(2);
        let clip_id = AudioClipId::new(3);
        project
            .apply(&RecordingOperation::Batch {
                operations: vec![
                    RecordingOperation::AddTrack {
                        track: AudioTrack::new(track_id, "Voice"),
                    },
                    RecordingOperation::AddAsset {
                        asset: AudioAsset {
                            id: asset_id,
                            file_name: "voice.flac".into(),
                            sample_rate: 48_000,
                            channels: 1,
                            sample_count: 48_000,
                            checksum: "0".repeat(40),
                            waveform: WaveformData::default(),
                        },
                    },
                    RecordingOperation::AddClip {
                        clip: AudioClip {
                            id: clip_id,
                            asset_id,
                            track_id,
                            start_frame: 0,
                            source_start_frame: 0,
                            duration_frames: 24,
                        },
                    },
                    RecordingOperation::SetTrackMuted {
                        track_id,
                        muted: true,
                    },
                ],
            })
            .unwrap();
        let paths = BTreeMap::from([(asset_id, PathBuf::from("voice.flac"))]);
        let spec = RecordingMixSpec::from_project(
            &project,
            &paths,
            Some(PathBuf::from("source.mp4")),
            None,
        )
        .unwrap();
        assert!(spec.clips.is_empty());
        // With no source duration and no clips, the mix is empty
        assert!(spec.is_empty());
    }
}
