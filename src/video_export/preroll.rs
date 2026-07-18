use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Prepend an independent, one-number-per-second countdown to an already
/// rendered MP4. Pre-roll belongs to the BR rendering timeline and is handled
/// by the main export pipeline.
pub fn prepend_countdown(
    path: &Path,
    countdown_seconds: u32,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if countdown_seconds == 0 {
        return Ok(());
    }
    if !path.is_file() {
        return Err(format!("Countdown input not found: {}", path.display()));
    }

    let countdown_seconds = countdown_seconds.clamp(1, 30);
    let seconds = countdown_seconds as f32;
    let delay_ms = (seconds * 1000.0).round() as u64;
    let has_audio = input_has_audio(path);
    let temp = temporary_path(path);

    let video_filter = {
        // `eif` is evaluated by drawtext for every frame. Colons are escaped
        // for the filter parser (the command is passed directly, not via a shell).
        format!(
            "[0:v]tpad=start_duration={seconds}:start_mode=add:color=black,\
             drawtext=text='%{{eif\\:max(1\\,ceil(({seconds}-t)*{countdown_from}/{seconds}))\\:d}}':\
             fontcolor=white:fontsize=h/5:borderw=4:bordercolor=black@0.8:\
             x=(w-text_w)/2:y=(h-text_h)/2:enable='lt(t,{seconds})'[v]"
        , countdown_from = countdown_seconds)
    };

    let filter = if has_audio {
        format!("{video_filter};[0:a]adelay={delay_ms}:all=1[a]")
    } else {
        video_filter
    };

    let mut command = crate::media_binary::command("ffmpeg");
    command.args(["-v", "warning", "-y", "-i"]).arg(path).args([
        "-filter_complex",
        &filter,
        "-map",
        "[v]",
    ]);
    if has_audio {
        command.args(["-map", "[a]"]);
    }
    command.args([
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
    ]);
    if has_audio {
        command.args(["-c:a", "aac", "-b:a", "192k"]);
    }
    command
        .args(["-movflags", "+faststart"])
        .arg(&temp)
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("ffmpeg pre-roll: {error}"))?;
    let stderr = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&temp);
            return Err(super::EXPORT_CANCELLED_MESSAGE.into());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("ffmpeg pre-roll wait: {error}"))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&temp);
        return Err(if stderr.trim().is_empty() {
            "ffmpeg pre-roll failed".into()
        } else {
            format!("ffmpeg pre-roll failed: {}", stderr.trim())
        });
    }

    replace_file(&temp, path)
}

fn input_has_audio(path: &Path) -> bool {
    crate::media_binary::command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()
        .is_some_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn temporary_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("export");
    parent.join(format!(".{stem}.countdown.{stamp}.mp4"))
}

fn replace_file(temp: &Path, destination: &Path) -> Result<(), String> {
    // Keep the original only while swapping the completed countdown render in.
    // The old `movie.mp4.before_countdown` name looked like a second MP4 export
    // in Explorer when cleanup was delayed or failed.
    let backup = backup_path(destination);
    let legacy_backup = destination.with_extension("mp4.before_countdown");
    let _ = remove_file_with_retry(&legacy_backup);
    let _ = remove_file_with_retry(&backup);
    std::fs::rename(destination, &backup).map_err(|error| format!("pre-roll backup: {error}"))?;
    match std::fs::rename(temp, destination) {
        Ok(()) => {
            if let Err(error) = remove_file_with_retry(&backup) {
                // The export itself is complete.  Do not turn it into a failed
                // delivery merely because Windows still has the backup open;
                // the hidden, timestamped name prevents it from being mistaken
                // for another user-facing export.
                log::warn!(
                    "Could not remove temporary pre-roll backup {}: {error}",
                    backup.display()
                );
            }
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::rename(&backup, destination);
            let _ = remove_file_with_retry(temp);
            Err(format!("pre-roll replace: {error}"))
        }
    }
}

fn backup_path(destination: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let stem = destination
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("export");
    parent.join(format!(".{stem}.before_countdown.{stamp}.mp4"))
}

fn remove_file_with_retry(path: &Path) -> std::io::Result<()> {
    let mut last_error = None;
    for _ in 0..5 {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(last_error.expect("a removal attempt always records an error"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_name_is_a_hidden_mp4_next_to_output() {
        let path = temporary_path(Path::new("C:/exports/movie.mp4"));
        assert_eq!(path.parent(), Some(Path::new("C:/exports")));
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".movie.countdown."));
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("mp4")
        );
    }

    #[test]
    fn backup_name_is_hidden_and_not_an_export_filename() {
        let path = backup_path(Path::new("C:/exports/movie_instrumental.mp4"));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with(".movie_instrumental.before_countdown."));
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("mp4")
        );
        assert!(!name.contains(".mp4.before_countdown"));
    }

    #[test]
    fn replace_file_leaves_only_the_countdown_export() {
        let directory = std::env::temp_dir().join(format!(
            "coquerythmo-preroll-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let destination = directory.join("movie_instrumental.mp4");
        let temp = directory.join("countdown.mp4");
        let legacy_backup = destination.with_extension("mp4.before_countdown");
        std::fs::write(&destination, b"without-countdown").unwrap();
        std::fs::write(&temp, b"with-countdown").unwrap();
        std::fs::write(&legacy_backup, b"stale-without-countdown").unwrap();

        replace_file(&temp, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"with-countdown");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
