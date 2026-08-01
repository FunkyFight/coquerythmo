use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

pub const PROXY_CANCELLED_MESSAGE: &str = "Proxy canceled";

pub fn is_cancelled_error(error: &str) -> bool {
    error == PROXY_CANCELLED_MESSAGE
}

#[derive(Clone, Debug)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration_secs: f64,
}

#[derive(Clone, Debug)]
pub struct ProxyLink {
    pub source_video_path: PathBuf,
    pub proxy_video_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProxyMetadata {
    br_path: PathBuf,
    source_video_path: PathBuf,
    proxy_video_path: PathBuf,
    width: u32,
    height: u32,
    crf: u8,
    source_len: u64,
    source_modified_secs: u64,
}

pub fn probe_video(path: &Path) -> Result<VideoInfo, String> {
    let out = crate::media_binary::command("ffprobe")
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
    let width = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1920);
    let height = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    let duration_secs = lines
        .get(1)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0);

    Ok(VideoInfo {
        width,
        height,
        duration_secs,
    })
}

pub fn default_proxy_size(width: u32, height: u32) -> (u32, u32) {
    fit_to_max_height(width, height, 1080)
}

pub fn fit_to_max_height(width: u32, height: u32, max_height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (1920, 1080);
    }

    let target_height = height.min(max_height).clamp(16, 8192);
    let target_width =
        ((width as f64 * target_height as f64 / height as f64).round() as u32).clamp(16, 8192);
    (even_dimension(target_width), even_dimension(target_height))
}

pub fn linked_proxy_path(br_path: &Path, source_video: &Path) -> Option<PathBuf> {
    let metadata = read_metadata(br_path)?;

    if !paths_match(&metadata.source_video_path, source_video) {
        return None;
    }

    validate_source_signature(source_video, &metadata)?;

    if metadata.proxy_video_path.exists() {
        Some(metadata.proxy_video_path)
    } else {
        None
    }
}

pub fn proxy_link_for_br(br_path: &Path) -> Option<ProxyLink> {
    let metadata = read_metadata(br_path)?;

    if !metadata.source_video_path.exists() {
        log::warn!(
            "Ignoring proxy metadata because source video is missing: {}",
            metadata.source_video_path.display()
        );
        return None;
    }

    validate_source_signature(&metadata.source_video_path, &metadata)?;

    if !metadata.proxy_video_path.exists() {
        log::warn!(
            "Ignoring proxy metadata because proxy video is missing: {}",
            metadata.proxy_video_path.display()
        );
        return None;
    }

    Some(ProxyLink {
        source_video_path: metadata.source_video_path,
        proxy_video_path: metadata.proxy_video_path,
    })
}

pub fn create_proxy(
    source_video: &Path,
    br_path: &Path,
    target_width: u32,
    target_height: u32,
    crf: u8,
    cancel: Arc<AtomicBool>,
    mut progress_cb: impl FnMut(f32),
) -> Result<PathBuf, String> {
    if !crate::video_export::check_ffmpeg() {
        return Err("ffmpeg/ffprobe not found beside app or in PATH".into());
    }

    let info = probe_video(source_video)?;
    if info.duration_secs <= 0.0 {
        return Err("Video has no duration".into());
    }

    let (width, height) = fit_to_dimensions_without_upscale(
        info.width,
        info.height,
        even_dimension(target_width),
        even_dimension(target_height),
    );
    let crf = crf.clamp(18, 32);
    let dir = proxies_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create proxies dir: {e}"))?;

    let proxy_path = proxy_file_path(br_path, source_video, width, height);
    let scale_filter = format!(
        "scale=w={width}:h={height}:force_original_aspect_ratio=decrease:force_divisible_by=2"
    );
    let crf_text = crf.to_string();

    progress_cb(0.01);
    log::info!(
        "Creating proxy {}x{} CRF {} at {}",
        width,
        height,
        crf,
        proxy_path.display()
    );

    let mut child = crate::media_binary::command("ffmpeg")
        .args(["-v", "error", "-y"])
        .arg("-i")
        .arg(source_video)
        .args(["-map", "0:v:0", "-vf"])
        .arg(&scale_filter)
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "fastdecode",
            "-profile:v",
            "baseline",
            "-g",
            "30",
            "-bf",
            "0",
            "-refs",
            "1",
            "-crf",
            &crf_text,
            "-pix_fmt",
            "yuv420p",
            "-an",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .arg(&proxy_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg proxy: {e}"))?;

    let mut stderr_handle = child.stderr.take().map(|stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        })
    });

    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let mut stdout_handle = child.stdout.take().map(|stdout| {
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(progress) = progress_from_ffmpeg_line(&line, info.duration_secs) {
                    let _ = progress_tx.send(progress.clamp(0.01, 0.99));
                }
            }
        })
    });

    let status = loop {
        while let Ok(progress) = progress_rx.try_recv() {
            progress_cb(progress);
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(handle) = stdout_handle.take() {
                let _ = handle.join();
            }
            let _ = stderr_handle.take().and_then(|handle| handle.join().ok());
            let _ = fs::remove_file(&proxy_path);
            return Err(PROXY_CANCELLED_MESSAGE.into());
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("ffmpeg proxy wait: {e}"))?
        {
            break status;
        }

        thread::sleep(Duration::from_millis(50));
    };

    while let Ok(progress) = progress_rx.try_recv() {
        progress_cb(progress);
    }
    if let Some(handle) = stdout_handle.take() {
        let _ = handle.join();
    }
    let stderr = stderr_handle
        .take()
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if !status.success() {
        let _ = fs::remove_file(&proxy_path);
        let details = stderr.trim();
        if details.is_empty() {
            return Err(format!("ffmpeg exited with status {status}"));
        }
        return Err(format!("ffmpeg: {details}"));
    }

    progress_cb(1.0);
    write_metadata(br_path, source_video, &proxy_path, width, height, crf)?;
    Ok(proxy_path)
}

fn write_metadata(
    br_path: &Path,
    source_video: &Path,
    proxy_path: &Path,
    width: u32,
    height: u32,
    crf: u8,
) -> Result<(), String> {
    let (source_len, source_modified_secs) = source_signature(source_video).unwrap_or((0, 0));
    let metadata = ProxyMetadata {
        br_path: canonical_or_original(br_path),
        source_video_path: canonical_or_original(source_video),
        proxy_video_path: canonical_or_original(proxy_path),
        width,
        height,
        crf,
        source_len,
        source_modified_secs,
    };
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("proxy metadata serialize: {e}"))?;
    fs::write(manifest_path(br_path), json).map_err(|e| format!("proxy metadata write: {e}"))
}

fn progress_from_ffmpeg_line(line: &str, duration_secs: f64) -> Option<f32> {
    if duration_secs <= 0.0 {
        return None;
    }

    if let Some(raw) = line
        .strip_prefix("out_time_ms=")
        .or_else(|| line.strip_prefix("out_time_us="))
    {
        let micros = raw.trim().parse::<f64>().ok()?;
        return Some((micros / 1_000_000.0 / duration_secs) as f32);
    }

    if let Some(raw) = line.strip_prefix("out_time=") {
        let secs = parse_ffmpeg_time(raw.trim())?;
        return Some((secs / duration_secs) as f32);
    }

    None
}

fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn fit_to_dimensions_without_upscale(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32) {
    if source_width == 0 || source_height == 0 {
        return (target_width, target_height);
    }

    let scale_w = target_width as f64 / source_width as f64;
    let scale_h = target_height as f64 / source_height as f64;
    let scale = scale_w.min(scale_h).min(1.0);
    let width = (source_width as f64 * scale).round() as u32;
    let height = (source_height as f64 * scale).round() as u32;
    (
        even_dimension(width.max(16)),
        even_dimension(height.max(16)),
    )
}

fn even_dimension(value: u32) -> u32 {
    let clamped = value.clamp(16, 8192);
    if clamped.is_multiple_of(2) {
        clamped
    } else {
        (clamped + 1).min(8192)
    }
}

fn proxies_dir() -> PathBuf {
    crate::media_binary::installation_temp_dir().join("proxies")
}

fn manifest_path(br_path: &Path) -> PathBuf {
    proxies_dir().join(format!(
        "{}_{}.proxy.json",
        safe_stem(br_path),
        stable_hash_hex(&canonical_or_original(br_path).to_string_lossy())
    ))
}

fn proxy_file_path(br_path: &Path, source_video: &Path, width: u32, height: u32) -> PathBuf {
    let source_key = canonical_or_original(source_video)
        .to_string_lossy()
        .to_string();
    proxies_dir().join(format!(
        "{}_{}_{}_{width}x{height}.mp4",
        safe_stem(br_path),
        stable_hash_hex(&canonical_or_original(br_path).to_string_lossy()),
        stable_hash_hex(&source_key)
    ))
}

fn safe_stem(path: &Path) -> String {
    let raw = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "br".into());
    let mut safe = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            safe.push(ch);
        } else {
            safe.push('_');
        }
    }
    if safe.is_empty() {
        "br".to_string()
    } else {
        safe
    }
}

fn stable_hash_hex(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn paths_match(a: &Path, b: &Path) -> bool {
    canonical_or_original(a) == canonical_or_original(b)
}

fn read_metadata(br_path: &Path) -> Option<ProxyMetadata> {
    let manifest = manifest_path(br_path);
    let content = fs::read_to_string(&manifest).ok()?;
    serde_json::from_str(&content).ok()
}

fn validate_source_signature(source_video: &Path, metadata: &ProxyMetadata) -> Option<()> {
    if let Ok((len, modified_secs)) = source_signature(source_video) {
        if metadata.source_len != len || metadata.source_modified_secs != modified_secs {
            log::warn!(
                "Ignoring stale proxy metadata for {}",
                source_video.display()
            );
            return None;
        }
    }
    Some(())
}

fn source_signature(path: &Path) -> Result<(u64, u64), String> {
    let metadata = fs::metadata(path).map_err(|e| format!("metadata: {e}"))?;
    let modified_secs = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    Ok((metadata.len(), modified_secs))
}
