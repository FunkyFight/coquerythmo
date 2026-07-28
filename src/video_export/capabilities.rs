use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use crate::media_binary;

/// Check if ffmpeg and ffprobe are available beside the app or in PATH.
pub fn check_ffmpeg() -> bool {
    let ffmpeg_ok = media_binary::can_run("ffmpeg");
    let ffprobe_ok = media_binary::can_run("ffprobe");
    if !ffmpeg_ok {
        log::error!("ffmpeg not found beside app or in PATH - video features unavailable");
    }
    if !ffprobe_ok {
        log::error!("ffprobe not found beside app or in PATH - video features unavailable");
    }
    ffmpeg_ok && ffprobe_ok
}

pub(super) struct VideoInfo {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) duration_secs: f64,
}

pub fn probe_video_duration(path: &Path) -> Option<f64> {
    let out = media_binary::command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .lines()
        .find_map(|line| {
            let duration = line.trim().parse::<f64>().ok()?;
            (duration.is_finite() && duration > 0.0).then_some(duration)
        })
}

pub(super) fn probe(path: &Path) -> Result<VideoInfo, String> {
    let out = media_binary::command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0:s=,",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.trim().lines().collect();
    if lines.is_empty() {
        return Err("ffprobe: no output".into());
    }
    let parts: Vec<&str> = lines[0].split(',').collect();
    let width: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1920);
    let height: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    let format_duration_secs: f64 = lines
        .get(1)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0);
    let duration_secs = probe_video_duration(path).unwrap_or(format_duration_secs);
    Ok(VideoInfo {
        width,
        height,
        duration_secs,
    })
}

/// Check if nvenc is available.
pub(super) fn has_nvenc() -> bool {
    media_binary::command("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("h264_nvenc"))
        .unwrap_or(false)
}

/// Check if CUDA hardware-accelerated decoding is available.
fn has_cuda_hwaccel() -> bool {
    media_binary::command("ffmpeg")
        .args(["-hide_banner", "-hwaccels"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("cuda"))
        .unwrap_or(false)
}

/// Check if an ffmpeg filter is available.
fn has_filter(name: &str) -> bool {
    media_binary::command("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|line| line.split_whitespace().nth(1) == Some(name))
        })
        .unwrap_or(false)
}

/// Check if the complete CUDA filter graph can be used end-to-end.
pub(super) fn has_cuda_filter_graph() -> bool {
    has_nvenc()
        && has_cuda_hwaccel()
        && has_filter("scale_cuda")
        && has_filter("overlay_cuda")
        && has_filter("hwupload_cuda")
}

pub(super) fn experimental_cuda_rgba_enabled() -> bool {
    std::env::var_os("COQUERYTHMO_EXPERIMENTAL_RGBA_CUDA").is_some()
}

pub(super) fn probe_cuda_rgba_br_graph() -> bool {
    let filter = "[0:v]format=nv12,hwupload_cuda[src];[1:v]format=rgba,hwupload_cuda,scale_cuda=w=16:h=16:format=nv12:passthrough=0[br];[src][br]overlay_cuda=x=0:y=0:shortest=1[out]";
    let mut child = match media_binary::command("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:r=1:d=1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            "16x16",
            "-r",
            "1",
            "-i",
            "pipe:0",
            "-filter_complex",
            filter,
            "-map",
            "[out]",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            log::warn!("CUDA RGBA graph probe could not start ffmpeg: {e}");
            return false;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let frame = vec![0u8; 16 * 16 * 4];
        if let Err(e) = stdin.write_all(&frame) {
            log::warn!("CUDA RGBA graph probe could not write frame: {e}");
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
    }

    match child.wait_with_output() {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let details = String::from_utf8_lossy(&output.stderr);
            log::warn!(
                "CUDA RGBA graph probe failed: {}",
                details
                    .trim()
                    .lines()
                    .next()
                    .unwrap_or("ffmpeg rejected graph")
            );
            false
        }
        Err(e) => {
            log::warn!("CUDA RGBA graph probe failed to wait for ffmpeg: {e}");
            false
        }
    }
}
