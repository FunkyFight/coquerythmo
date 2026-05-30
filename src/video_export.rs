use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::project::Project;
use crate::rythmo_cpu_renderer;
use crate::rythmo_gpu_renderer;

/// Check if ffmpeg and ffprobe are available in PATH.
pub fn check_ffmpeg() -> bool {
    let ffmpeg_ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let ffprobe_ok = std::process::Command::new("ffprobe")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ffmpeg_ok {
        log::error!("ffmpeg not found in PATH — video features unavailable");
    }
    if !ffprobe_ok {
        log::error!("ffprobe not found in PATH — video features unavailable");
    }
    ffmpeg_ok && ffprobe_ok
}

struct VideoInfo {
    width: u32,
    height: u32,
    duration_secs: f64,
}

fn probe(path: &Path) -> Result<VideoInfo, String> {
    let out = Command::new("ffprobe")
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
    let width: u32 = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let height: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    let duration_secs: f64 = lines
        .get(1)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0);
    Ok(VideoInfo {
        width,
        height,
        duration_secs,
    })
}

/// Check if nvenc is available
fn has_nvenc() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("h264_nvenc"))
        .unwrap_or(false)
}

/// Check if CUDA hardware-accelerated decoding is available
fn has_cuda_hwaccel() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-hwaccels"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("cuda"))
        .unwrap_or(false)
}

/// Export MP4 with BR strip (two-pass).
/// - Pass 1: render BR frames → encode to temp video (pipe only carries BR, no source decoding)
/// - Pass 2: combine source + BR temp file (both file inputs, no pipe bottleneck)
/// - CUDA hwaccel + NVENC when available
pub fn export_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    export_width: u32,
    export_height: u32,
    mut progress_cb: impl FnMut(f32) + Send,
) -> Result<(), String> {
    if !check_ffmpeg() {
        return Err("ffmpeg/ffprobe not found in PATH".into());
    }
    let info = probe(source_video)?;
    let out_w = even_dimension(export_width);
    let vid_h = even_dimension(export_height);
    let br_h = rythmo_cpu_renderer::br_height(project, out_w, br_scale);
    let total_frames = (info.duration_secs * fps) as u64;

    if total_frames == 0 {
        return Err("Video has no duration".into());
    }

    let use_nvenc = has_nvenc();
    let use_cuda = use_nvenc && has_cuda_hwaccel();
    let codec = if use_nvenc { "h264_nvenc" } else { "libx264" };
    log::info!("Using {} encoding, CUDA hwaccel={}", codec, use_cuda);
    log::info!(
        "Export: source {}x{} -> {}x{} video + {}px BR, {} frames at {}fps, codec={}",
        info.width,
        info.height,
        out_w,
        vid_h,
        br_h,
        total_frames,
        fps,
        codec
    );

    // === Pass 1: Render BR strip → temp video file ===
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("cannot locate exe: {e}"))?
        .parent()
        .ok_or("cannot get exe directory")?
        .to_path_buf();
    let temp_dir = exe_dir.join("temp");
    let _ = std::fs::create_dir_all(&temp_dir);
    let temp_br = temp_dir.join("br_temp.mp4");
    log::info!(
        "Pass 1: encoding {} BR frames at {}fps to {}",
        total_frames,
        fps,
        temp_br.display()
    );

    // Ensure even dimensions for YUV420p (chroma subsampling requires 2x2 blocks)
    let br_h_even = (br_h + 1) & !1;
    let w = out_w as usize;
    let h = br_h_even as usize;
    let yuv_frame_size = w * h * 3 / 2; // Y: w*h, U: w/2*h/2, V: w/2*h/2

    let mut br_encoder = Command::new("ffmpeg")
        .args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-s",
            &format!("{}x{}", w, h),
            "-r",
            &fps.to_string(),
            "-i",
            "pipe:0",
        ])
        .args(if use_nvenc {
            vec!["-c:v", "hevc_nvenc", "-preset", "p1", "-tune", "lossless"]
        } else {
            vec!["-c:v", "libx264", "-preset", "ultrafast", "-qp", "0"]
        })
        .args(["-v", "error", "-y"])
        .arg(&temp_br)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg pass 1: {e}"))?;

    {
        let br_stdin = br_encoder.stdin.take().unwrap();
        let mut writer = std::io::BufWriter::with_capacity(yuv_frame_size * 8, br_stdin);

        let mut yuv_buf = vec![0u8; yuv_frame_size];
        let hw = w / 2;
        let u_off = w * h;
        let v_off = u_off + hw * (h / 2);

        // Ratio to convert export frame index → video source frame position
        let frame_ratio = source_fps / fps;
        log::info!(
            "Frame ratio: source {:.2}fps / export {:.2}fps = {:.4}",
            source_fps,
            fps,
            frame_ratio
        );

        match rythmo_gpu_renderer::GpuRenderer::new() {
            Ok(mut gpu) => {
                log::info!("Pass 1: GPU pipelined");

                gpu.submit_render(project, 0.0, out_w, fps, br_scale);

                for frame in 1..total_frames as i64 {
                    let rgba = gpu.finish_render(out_w, br_h);
                    let video_pos = frame as f64 * frame_ratio;
                    gpu.submit_render(project, video_pos, out_w, fps, br_scale);

                    rgba_to_yuv420p(&rgba, &mut yuv_buf, w, h, br_h as usize, hw, u_off, v_off);

                    if writer.write_all(&yuv_buf).is_err() {
                        break;
                    }

                    if frame as u64 % fps as u64 == 0 {
                        progress_cb((frame - 1) as f32 / total_frames as f32 * 0.9);
                    }
                }

                let rgba = gpu.finish_render(out_w, br_h);
                rgba_to_yuv420p(&rgba, &mut yuv_buf, w, h, br_h as usize, hw, u_off, v_off);
                let _ = writer.write_all(&yuv_buf);
            }
            Err(e) => {
                log::warn!("GPU unavailable ({}), CPU fallback", e);
                let n_threads = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4);
                log::info!("CPU render: {} threads", n_threads);

                let mut renderers: Vec<rythmo_cpu_renderer::CpuRenderer> = (0..n_threads)
                    .map(|_| rythmo_cpu_renderer::CpuRenderer::new())
                    .collect();
                let frame_indices: Vec<i64> = (0..total_frames as i64).collect();

                for batch in frame_indices.chunks(n_threads) {
                    let rendered: Vec<Vec<u8>> = std::thread::scope(|scope| {
                        let handles: Vec<_> = batch
                            .iter()
                            .zip(renderers.iter_mut())
                            .map(|(&frame, renderer)| {
                                let vf = (frame as f64 * frame_ratio) as i64;
                                scope.spawn(move || {
                                    renderer.render_br(project, vf, out_w, source_fps, br_scale)
                                })
                            })
                            .collect();
                        handles.into_iter().map(|h| h.join().unwrap()).collect()
                    });

                    for (i, rgba) in rendered.iter().enumerate() {
                        let frame = batch[i];
                        rgba_to_yuv420p(rgba, &mut yuv_buf, w, h, br_h as usize, hw, u_off, v_off);
                        if writer.write_all(&yuv_buf).is_err() {
                            break;
                        }
                        if frame as u64 % fps as u64 == 0 {
                            progress_cb(frame as f32 / total_frames as f32 * 0.9);
                        }
                    }
                }
            }
        }

        let _ = writer.flush();
    }

    let result = br_encoder
        .wait_with_output()
        .map_err(|e| format!("ffmpeg pass 1: {e}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&temp_br);
        return Err(format!(
            "ffmpeg pass 1: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    log::info!("Pass 1 complete, BR temp: {}", temp_br.display());

    // === Pass 2: Combine source video + BR temp file (no pipe) ===
    log::info!("Pass 2: combining source + BR strip");
    progress_cb(0.9);

    // Try CUDA hwaccel first, fallback to CPU filters if it fails
    let cuda_filter = fit_and_stack_filter(out_w, vid_h, true);
    let cpu_filter = fit_and_stack_filter(out_w, vid_h, false);

    let result = if use_cuda {
        // Try CUDA-accelerated decoding first, then preserve the source aspect ratio in CPU filters.
        let r = run_pass2(
            source_video,
            &temp_br,
            output,
            &cuda_filter,
            true,
            use_nvenc,
            codec,
            fps,
            info.duration_secs,
            &mut progress_cb,
        );
        if r.is_err() {
            log::warn!("CUDA pass 2 failed, falling back to CPU filters");
            progress_cb(0.9);
            let _ = std::fs::remove_file(output);
            run_pass2(
                source_video,
                &temp_br,
                output,
                &cpu_filter,
                false,
                use_nvenc,
                codec,
                fps,
                info.duration_secs,
                &mut progress_cb,
            )
        } else {
            r
        }
    } else {
        run_pass2(
            source_video,
            &temp_br,
            output,
            &cpu_filter,
            false,
            use_nvenc,
            codec,
            fps,
            info.duration_secs,
            &mut progress_cb,
        )
    };

    let _ = std::fs::remove_file(&temp_br);
    result?;

    progress_cb(1.0);
    log::info!("Export complete: {}", output.display());
    Ok(())
}

fn even_dimension(value: u32) -> u32 {
    let clamped = value.clamp(16, 8192);
    if clamped % 2 == 0 {
        clamped
    } else {
        (clamped + 1).min(8192)
    }
}

fn fit_and_stack_filter(out_w: u32, vid_h: u32, from_cuda_frames: bool) -> String {
    let source_prefix = if from_cuda_frames {
        "hwdownload,format=nv12,"
    } else {
        ""
    };
    let cpu_filter = format!(
        "[0:v]{}scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,format=yuv420p[v];[1:v]format=yuv420p[br];[v][br]vstack=inputs=2[out]",
        source_prefix, out_w, vid_h, out_w, vid_h
    );
    cpu_filter
}

fn run_pass2(
    source_video: &Path,
    temp_br: &Path,
    output: &Path,
    filter: &str,
    use_cuda: bool,
    use_nvenc: bool,
    codec: &str,
    fps: f64,
    duration_secs: f64,
    progress_cb: &mut impl FnMut(f32),
) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    if use_cuda {
        cmd.args(["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"]);
    }
    let combine = cmd
        .arg("-i")
        .arg(source_video)
        .arg("-i")
        .arg(temp_br)
        .args(["-filter_complex", filter, "-map", "[out]", "-map", "0:a?"])
        .args(if use_nvenc {
            vec![
                "-c:v", codec, "-preset", "p1", "-rc", "constqp", "-qp", "20", "-b:v", "0",
            ]
        } else {
            vec!["-c:v", codec, "-preset", "ultrafast", "-crf", "20"]
        })
        .args([
            "-pix_fmt",
            "yuv420p",
            "-r",
            &fps.to_string(),
            "-c:a",
            "copy",
            "-shortest",
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
        .map_err(|e| format!("ffmpeg pass 2: {e}"))?;

    let mut child = combine;
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(progress) = pass2_progress_from_ffmpeg_line(&line, duration_secs) {
                progress_cb(0.9 + progress.clamp(0.0, 1.0) * 0.095);
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("ffmpeg pass 2 wait: {e}"))?;
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if !status.success() {
        let details = stderr.trim();
        if details.is_empty() {
            Err(format!(
                "ffmpeg pass 2 failed (cuda={}, filter={})",
                use_cuda, filter
            ))
        } else {
            Err(format!(
                "ffmpeg pass 2 failed (cuda={}, filter={}): {}",
                use_cuda, filter, details
            ))
        }
    } else {
        progress_cb(0.995);
        Ok(())
    }
}

fn pass2_progress_from_ffmpeg_line(line: &str, duration_secs: f64) -> Option<f32> {
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

/// Convert RGBA pixels to YUV420p.
fn rgba_to_yuv420p(
    rgba: &[u8],
    yuv_buf: &mut [u8],
    w: usize,
    h: usize,
    br_h: usize,
    hw: usize,
    u_off: usize,
    v_off: usize,
) {
    for y in 0..br_h {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let (r, g, b) = (rgba[si] as i32, rgba[si + 1] as i32, rgba[si + 2] as i32);
            yuv_buf[y * w + x] =
                (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
        }
    }
    for y in br_h..h {
        for x in 0..w {
            yuv_buf[y * w + x] = 16;
        }
    }
    for cy in 0..h / 2 {
        for cx in 0..hw {
            let mut rs = 0i32;
            let mut gs = 0i32;
            let mut bs = 0i32;
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let py = cy * 2 + dy;
                    let px = cx * 2 + dx;
                    if py < br_h {
                        let si = (py * w + px) * 4;
                        rs += rgba[si] as i32;
                        gs += rgba[si + 1] as i32;
                        bs += rgba[si + 2] as i32;
                    }
                }
            }
            let r = rs >> 2;
            let g = gs >> 2;
            let b = bs >> 2;
            yuv_buf[u_off + cy * hw + cx] =
                (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            yuv_buf[v_off + cy * hw + cx] =
                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
}
