use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::project::Project;
use crate::rythmo_cpu_renderer;

const OUTPUT_FPS: u32 = 240;

struct VideoInfo {
    width: u32,
    height: u32,
    duration_secs: f64,
}

fn probe(path: &Path) -> Result<VideoInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-show_entries", "format=duration",
            "-of", "csv=p=0:s=,"])
        .arg(path)
        .output()
        .map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.trim().lines().collect();
    if lines.is_empty() { return Err("ffprobe: no output".into()); }
    let parts: Vec<&str> = lines[0].split(',').collect();
    let width: u32 = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let height: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    let duration_secs: f64 = lines.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
    Ok(VideoInfo { width, height, duration_secs })
}

/// Check if nvenc is available
fn has_nvenc() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("h264_nvenc"))
        .unwrap_or(false)
}

/// Export MP4 with BR strip.
/// - BR frames cached (only re-rendered when source frame changes) → ~10x faster
/// - NVENC hardware encoding if available
pub fn export_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    mut progress_cb: impl FnMut(f32) + Send,
) -> Result<(), String> {
    let info = probe(source_video)?;
    let out_w = info.width;
    let br_h = rythmo_cpu_renderer::br_height(project, out_w);
    let vid_h = info.height;
    let total_frames = (info.duration_secs * OUTPUT_FPS as f64) as u64;

    if total_frames == 0 {
        return Err("Video has no duration".into());
    }

    // Pick encoder: nvenc (GPU) or libx264 (CPU fallback)
    let use_nvenc = has_nvenc();
    let codec = if use_nvenc { "h264_nvenc" } else { "libx264" };
    log::info!("Using {} encoding", codec);

    log::info!("Export: {}x{} video + {}px BR, {} frames at {}fps, codec={}",
        out_w, vid_h, br_h, total_frames, OUTPUT_FPS, codec);

    // Video on top, BR on bottom
    let filter = format!(
        "[0:v]scale={}:{},fps={}[v];[v][1:v]vstack=inputs=2[out]",
        out_w, vid_h, OUTPUT_FPS
    );

    let mut encoder = Command::new("ffmpeg")
        .arg("-i").arg(source_video)
        .args([
            "-f", "rawvideo", "-pix_fmt", "rgba",
            "-s", &format!("{}x{}", out_w, br_h),
            "-r", &OUTPUT_FPS.to_string(),
            "-i", "pipe:0",
        ])
        .args(["-filter_complex", &filter, "-map", "[out]", "-map", "0:a?"])
        .args(if use_nvenc {
            vec!["-c:v", codec, "-preset", "p1", "-rc", "constqp", "-qp", "20", "-b:v", "0"]
        } else {
            vec!["-c:v", codec, "-preset", "ultrafast", "-crf", "20"]
        })
        .args(["-pix_fmt", "yuv420p", "-r", &OUTPUT_FPS.to_string(), "-c:a", "copy", "-shortest", "-v", "error", "-y"])
        .arg(output)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    let encoder_stdin = encoder.stdin.take().unwrap();
    let mut writer = std::io::BufWriter::with_capacity((out_w * br_h * 4) as usize * 2, encoder_stdin);

    // Stateful renderer (reuses font system across frames)
    let mut cpu_renderer = rythmo_cpu_renderer::CpuRenderer::new();

    // BR frame cache: only re-render when source frame changes
    let mut cached_br: Vec<u8> = Vec::new();
    let mut cached_source_frame: i64 = -1;

    for frame_num in 0..total_frames {
        let source_frame = (frame_num as f64 / OUTPUT_FPS as f64 * fps) as i64;

        // Only re-render BR when source frame changes
        if source_frame != cached_source_frame {
            cached_br = cpu_renderer.render_br(project, source_frame, out_w, fps);
            cached_source_frame = source_frame;
        }

        if writer.write_all(&cached_br).is_err() {
            break;
        }

        // Progress every second of output
        if frame_num % (OUTPUT_FPS as u64) == 0 {
            progress_cb(frame_num as f32 / total_frames as f32);
        }
    }

    let _ = writer.flush();
    drop(writer);

    let result = encoder.wait_with_output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !result.status.success() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&result.stderr)));
    }

    progress_cb(1.0);
    log::info!("Export complete: {}", output.display());
    Ok(())
}
