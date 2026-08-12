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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyEncoder {
    H264,
    Mjpeg,
    ProResProxy,
}

impl ProxyEncoder {
    pub const ALL: [Self; 3] = [Self::H264, Self::Mjpeg, Self::ProResProxy];

    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Mjpeg => "MJPEG",
            Self::ProResProxy => "ProRes Proxy",
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::H264 => "mp4",
            Self::Mjpeg | Self::ProResProxy => "mov",
        }
    }

    fn ffmpeg_args(self, crf: u8) -> Vec<String> {
        match self {
            Self::H264 => [
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
                &crf.to_string(),
                "-pix_fmt",
                "yuv420p",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            Self::Mjpeg => ["-c:v", "mjpeg", "-q:v", "3", "-pix_fmt", "yuvj422p"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            Self::ProResProxy => [
                "-c:v",
                "prores_ks",
                "-profile:v",
                "0",
                "-pix_fmt",
                "yuv422p10le",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

impl Default for ProxyEncoder {
    fn default() -> Self {
        Self::ProResProxy
    }
}

pub fn is_cancelled_error(error: &str) -> bool {
    error == PROXY_CANCELLED_MESSAGE
}

#[derive(Clone, Debug)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration_secs: f64,
    pub fps: f64,
    pub video_codec: String,
    pub bitrate: u64,
    pub file_size: u64,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u32>,
    pub audio_sample_rate: Option<u32>,
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
            "-show_entries",
            "stream=codec_type,codec_name,width,height,avg_frame_rate,bit_rate,channels,sample_rate:format=duration,bit_rate,size",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;

    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe JSON: {e}"))?;
    let streams = json["streams"]
        .as_array()
        .ok_or_else(|| "ffprobe: no streams".to_string())?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or_else(|| "ffprobe: no video stream".to_string())?;
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio");
    let format = &json["format"];
    let parse_u64 = |value: &serde_json::Value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            .unwrap_or(0)
    };
    let parse_f64 = |value: &serde_json::Value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            .unwrap_or(0.0)
    };
    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(parse_fraction)
        .unwrap_or(0.0);

    Ok(VideoInfo {
        width: parse_u64(&video["width"]) as u32,
        height: parse_u64(&video["height"]) as u32,
        duration_secs: parse_f64(&format["duration"]),
        fps,
        video_codec: video["codec_name"].as_str().unwrap_or("—").to_string(),
        bitrate: parse_u64(&video["bit_rate"]).max(parse_u64(&format["bit_rate"])),
        file_size: parse_u64(&format["size"]),
        audio_codec: audio.and_then(|stream| stream["codec_name"].as_str().map(str::to_string)),
        audio_channels: audio.map(|stream| parse_u64(&stream["channels"]) as u32),
        audio_sample_rate: audio.map(|stream| parse_u64(&stream["sample_rate"]) as u32),
    })
}

fn parse_fraction(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator != 0.0).then(|| numerator.parse::<f64>().ok().map(|n| n / denominator))?
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MediaPreference {
    #[serde(default)]
    use_proxy: bool,
    #[serde(default)]
    source_removed: bool,
}

pub fn default_uses_proxy(br_path: &Path) -> bool {
    default_uses_proxy_or(br_path, read_metadata(br_path).is_some())
}

pub fn default_uses_proxy_or(br_path: &Path, fallback: bool) -> bool {
    fs::read_to_string(preference_path(br_path))
        .ok()
        .and_then(|content| serde_json::from_str::<MediaPreference>(&content).ok())
        .map_or(fallback, |preference| preference.use_proxy)
}

pub fn set_default_uses_proxy(br_path: &Path, use_proxy: bool) -> Result<(), String> {
    let mut preference = read_preference(br_path).unwrap_or_default();
    preference.use_proxy = use_proxy;
    preference.source_removed = false;
    write_preference(br_path, &preference)
}

pub fn source_is_removed(br_path: &Path) -> bool {
    read_preference(br_path).is_some_and(|preference| preference.source_removed)
}

pub fn set_source_removed(br_path: &Path, source_removed: bool) -> Result<(), String> {
    let mut preference = read_preference(br_path).unwrap_or_default();
    preference.source_removed = source_removed;
    if source_removed {
        preference.use_proxy = false;
    }
    write_preference(br_path, &preference)
}

fn read_preference(br_path: &Path) -> Option<MediaPreference> {
    fs::read_to_string(preference_path(br_path))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn write_preference(br_path: &Path, preference: &MediaPreference) -> Result<(), String> {
    let path = preference_path(br_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create media settings dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(preference)
        .map_err(|e| format!("media settings serialize: {e}"))?;
    fs::write(path, json).map_err(|e| format!("media settings write: {e}"))
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
    if !default_uses_proxy(br_path) {
        return None;
    }
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

pub fn delete_proxy(br_path: &Path) -> Result<(), String> {
    if let Some(metadata) = read_metadata(br_path) {
        if metadata.proxy_video_path.exists() {
            fs::remove_file(&metadata.proxy_video_path)
                .map_err(|e| format!("delete proxy: {e}"))?;
        }
    }
    let manifest = manifest_path(br_path);
    if manifest.exists() {
        fs::remove_file(manifest).map_err(|e| format!("delete proxy metadata: {e}"))?;
    }
    set_default_uses_proxy(br_path, false)
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
    encoder: ProxyEncoder,
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

    let proxy_path = proxy_file_path(br_path, source_video, width, height, encoder);
    let scale_filter = format!(
        "scale=w={width}:h={height}:force_original_aspect_ratio=decrease:force_divisible_by=2"
    );
    progress_cb(0.01);
    log::info!(
        "Creating {} proxy {}x{} at {}",
        encoder.label(),
        width,
        height,
        proxy_path.display()
    );

    let mut command = crate::media_binary::command("ffmpeg");
    command
        .args(["-v", "error", "-y"])
        .arg("-i")
        .arg(source_video)
        .args(["-map", "0:v:0", "-vf"])
        .arg(&scale_filter)
        .args(encoder.ffmpeg_args(crf))
        .args([
            "-an",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .arg(&proxy_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("ffmpeg proxy: {e}"))?;

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
    set_default_uses_proxy(br_path, true)?;
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

fn preference_path(br_path: &Path) -> PathBuf {
    #[cfg(test)]
    let directory = std::env::temp_dir().join("coquerythmo-media-preferences-tests");
    #[cfg(not(test))]
    let directory = crate::media_binary::user_data_dir().join("media");
    directory.join(format!(
        "{}_{}.media.json",
        safe_stem(br_path),
        stable_hash_hex(&canonical_or_original(br_path).to_string_lossy())
    ))
}

fn proxy_file_path(
    br_path: &Path,
    source_video: &Path,
    width: u32,
    height: u32,
    encoder: ProxyEncoder,
) -> PathBuf {
    let source_key = canonical_or_original(source_video)
        .to_string_lossy()
        .to_string();
    proxies_dir().join(format!(
        "{}_{}_{}_{width}x{height}.{}",
        safe_stem(br_path),
        stable_hash_hex(&canonical_or_original(br_path).to_string_lossy()),
        stable_hash_hex(&source_key),
        encoder.extension()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_encoders_have_the_expected_ffmpeg_codec_and_container() {
        assert_eq!(ProxyEncoder::default(), ProxyEncoder::ProResProxy);
        for (encoder, codec, extension) in [
            (ProxyEncoder::H264, "libx264", "mp4"),
            (ProxyEncoder::Mjpeg, "mjpeg", "mov"),
            (ProxyEncoder::ProResProxy, "prores_ks", "mov"),
        ] {
            assert!(encoder.ffmpeg_args(24).iter().any(|arg| arg == codec));
            assert_eq!(encoder.extension(), extension);
        }
    }

    #[test]
    fn media_preference_keeps_default_and_removed_source_consistent() {
        let br_path = std::env::temp_dir().join(format!(
            "coquerythmo-media-preference-{}-{}.cqr",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        set_default_uses_proxy(&br_path, true).unwrap();
        assert!(default_uses_proxy(&br_path));
        set_source_removed(&br_path, true).unwrap();
        assert!(source_is_removed(&br_path));
        assert!(!default_uses_proxy(&br_path));
        set_source_removed(&br_path, false).unwrap();
        assert!(!source_is_removed(&br_path));
        assert_eq!(parse_fraction("24000/1001"), Some(24000.0 / 1001.0));

        let _ = fs::remove_file(preference_path(&br_path));
    }
}
