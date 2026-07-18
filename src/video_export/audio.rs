//! Audio extraction and muxing for video export.
#![allow(clippy::too_many_arguments)]

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::media_binary;

use super::progress;
use super::progress::ProgressCallback;

pub(super) fn temp_video_path(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    parent.join(format!(
        ".coquerythmo_export_{}_{}.video.mp4",
        std::process::id(),
        stamp
    ))
}

pub(super) fn instrumental_output_path(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("export");
    let extension = output
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("mp4");
    parent.join(format!("{stem}_instrumental.{extension}"))
}

pub(super) fn mux_source_audio(
    video_only: &Path,
    source_video: &Path,
    output: &Path,
    duration_secs: f64,
    offset_frames: i64,
    fps: f64,
    announcer_audio: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    progress::check_export_cancel(cancel)?;
    let duration_arg = duration_secs.to_string();
    let audio_filter = audio_offset_filter(duration_secs, offset_frames, fps);

    let mut command = media_binary::command("ffmpeg");
    command
        .arg("-i")
        .arg(video_only)
        .arg("-i")
        .arg(source_video);
    if let Some(announcer) = announcer_audio {
        command.arg("-i").arg(announcer);
        let cue_filter = format!("[1:a]{}[base];[2:a]apad=whole_dur={duration_arg},atrim=duration={duration_arg}[cue];[base][cue]amix=inputs=2:normalize=0[aout]", audio_filter);
        command.args([
            "-filter_complex",
            &cue_filter,
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
        ]);
    } else {
        command.args(["-map", "0:v:0", "-map", "1:a?", "-af", &audio_filter]);
    }
    let mut child = command
        .args([
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-t",
            &duration_arg,
            "-movflags",
            "+faststart",
            "-nostats",
            "-v",
            "warning",
            "-y",
        ])
        .arg(output)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg mux source audio: {e}"))?;

    let mut stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        })
    });

    let status = loop {
        if progress::export_cancelled(cancel) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_handle.take().and_then(|handle| handle.join().ok());
            let _ = std::fs::remove_file(output);
            return Err(super::EXPORT_CANCELLED_MESSAGE.into());
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("ffmpeg mux source audio wait: {e}"))?
        {
            break status;
        }

        std::thread::sleep(Duration::from_millis(50));
    };

    let stderr = stderr_handle
        .take()
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if status.success() {
        Ok(())
    } else {
        let details = stderr.trim();
        if details.is_empty() {
            Err("ffmpeg mux source audio failed".into())
        } else {
            Err(format!("ffmpeg mux source audio failed: {details}"))
        }
    }
}

pub(super) fn mux_instrumental_audio(
    normal_video: &Path,
    instrumental_audio: &Path,
    output: &Path,
    duration_secs: f64,
    offset_frames: i64,
    fps: f64,
    announcer_audio: Option<&Path>,
    cancel: &AtomicBool,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let duration_arg = duration_secs.to_string();
    let audio_filter = audio_offset_filter(duration_secs, offset_frames, fps);

    progress::emit_progress(progress_cb, 0.99);
    progress::check_export_cancel(cancel)?;

    let mut command = media_binary::command("ffmpeg");
    command
        .arg("-i")
        .arg(normal_video)
        .arg("-i")
        .arg(instrumental_audio);
    if let Some(announcer) = announcer_audio {
        command.arg("-i").arg(announcer);
        let cue_filter = format!("[1:a]{}[base];[2:a]apad=whole_dur={duration_arg},atrim=duration={duration_arg}[cue];[base][cue]amix=inputs=2:normalize=0[aout]", audio_filter);
        command.args([
            "-filter_complex",
            &cue_filter,
            "-map",
            "0:v:0",
            "-map",
            "[aout]",
        ]);
    } else {
        command.args(["-map", "0:v:0", "-map", "1:a:0", "-af", &audio_filter]);
    }
    let mut child = command
        .args([
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-t",
            &duration_arg,
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "-v",
            "warning",
            "-y",
        ])
        .arg(output)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg mux instrumental audio: {e}"))?;

    let mut stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        })
    });

    let mut stdout_handle = child.stdout.take().map(|stdout| {
        let progress_cb = progress_cb.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(progress) = progress::ffmpeg_progress_from_line(&line, duration_secs) {
                    progress::emit_progress(
                        &progress_cb,
                        progress::map_progress(progress, 0.99, 0.999),
                    );
                }
            }
        })
    });

    let status = loop {
        if progress::export_cancelled(cancel) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(handle) = stdout_handle.take() {
                let _ = handle.join();
            }
            let _ = stderr_handle.take().and_then(|handle| handle.join().ok());
            let _ = std::fs::remove_file(output);
            return Err(super::EXPORT_CANCELLED_MESSAGE.into());
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("ffmpeg mux instrumental audio wait: {e}"))?
        {
            break status;
        }

        std::thread::sleep(Duration::from_millis(50));
    };
    if let Some(handle) = stdout_handle.take() {
        let _ = handle.join();
    }
    let stderr = stderr_handle
        .take()
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if status.success() {
        Ok(())
    } else {
        let details = stderr.trim();
        if details.is_empty() {
            Err("ffmpeg mux instrumental audio failed".into())
        } else {
            Err(format!("ffmpeg mux instrumental audio failed: {details}"))
        }
    }
}

fn audio_offset_filter(duration_secs: f64, offset_frames: i64, fps: f64) -> String {
    let duration_arg = duration_secs.to_string();
    let offset_secs = if fps.is_finite() && fps > 0.0 {
        offset_frames as f64 / fps
    } else {
        0.0
    };
    if offset_secs > 0.0 {
        let delay_ms = (offset_secs * 1000.0).round().max(0.0) as i64;
        format!(
            "adelay={delay_ms}:all=1,apad=whole_dur={duration_arg},atrim=duration={duration_arg},asetpts=PTS-STARTPTS"
        )
    } else if offset_secs < 0.0 {
        let start = (-offset_secs).to_string();
        format!(
            "atrim=start={start},apad=whole_dur={duration_arg},atrim=duration={duration_arg},asetpts=PTS-STARTPTS"
        )
    } else {
        format!("apad=whole_dur={duration_arg},atrim=duration={duration_arg},asetpts=PTS-STARTPTS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrumental_output_path_adds_suffix_before_extension() {
        assert_eq!(
            instrumental_output_path(Path::new("movie.mp4")),
            PathBuf::from("movie_instrumental.mp4")
        );
    }

    #[test]
    fn instrumental_output_path_defaults_to_mp4_extension() {
        assert_eq!(
            instrumental_output_path(Path::new("movie")),
            PathBuf::from("movie_instrumental.mp4")
        );
    }

    #[test]
    fn audio_offset_uses_the_source_frame_rate() {
        assert_eq!(
            audio_offset_filter(10.0, 24, 24.0),
            "adelay=1000:all=1,apad=whole_dur=10,atrim=duration=10,asetpts=PTS-STARTPTS"
        );
    }
}
