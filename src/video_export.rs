use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::project::Project;
use crate::rythmo_cpu_renderer;
use crate::rythmo_gpu_renderer;

pub const EXPORT_RENDER_BACKEND_UNKNOWN: u32 = 0;
pub const EXPORT_RENDER_BACKEND_GPU: u32 = 1;
pub const EXPORT_RENDER_BACKEND_CPU: u32 = 2;

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

struct ProgressState {
    callback: Mutex<Box<dyn FnMut(f32) + Send>>,
    reported: AtomicU32,
}

type ProgressCallback = Arc<ProgressState>;

fn emit_progress(progress_cb: &ProgressCallback, progress: f32) {
    let progress = progress.clamp(0.0, 1.0);
    let mut current = progress_cb.reported.load(Ordering::Relaxed);
    loop {
        if progress <= f32::from_bits(current) {
            return;
        }
        match progress_cb.reported.compare_exchange(
            current,
            progress.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                if let Ok(mut cb) = progress_cb.callback.lock() {
                    cb(progress);
                }
                return;
            }
            Err(next) => current = next,
        }
    }
}

fn probe_video_duration(path: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
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
fn has_nvenc() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("h264_nvenc"))
        .unwrap_or(false)
}

/// Check if CUDA hardware-accelerated decoding is available.
fn has_cuda_hwaccel() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-hwaccels"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("cuda"))
        .unwrap_or(false)
}

/// Check if an ffmpeg filter is available.
fn has_filter(name: &str) -> bool {
    Command::new("ffmpeg")
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
fn has_cuda_filter_graph() -> bool {
    has_nvenc()
        && has_cuda_hwaccel()
        && has_filter("scale_cuda")
        && has_filter("overlay_cuda")
        && has_filter("hwupload_cuda")
}

fn experimental_cuda_rgba_enabled() -> bool {
    std::env::var_os("COQUERYTHMO_EXPERIMENTAL_RGBA_CUDA").is_some()
}

fn probe_cuda_rgba_br_graph() -> bool {
    let filter = "[0:v]format=nv12,hwupload_cuda[src];[1:v]format=rgba,hwupload_cuda,scale_cuda=w=16:h=16:format=nv12:passthrough=0[br];[src][br]overlay_cuda=x=0:y=0:shortest=1[out]";
    let mut child = match Command::new("ffmpeg")
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportPipeline {
    Cuda,
    Cpu,
}

impl ExportPipeline {
    fn uses_cuda(self) -> bool {
        matches!(self, Self::Cuda)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cuda => "ffmpeg CUDA scale/overlay",
            Self::Cpu => "ffmpeg CPU filters",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrRenderBackend {
    GpuWgpuNv12,
    GpuWgpuRgbaCuda,
    Cpu,
}

impl BrRenderBackend {
    fn label(self) -> &'static str {
        match self {
            Self::GpuWgpuNv12 => "GPU WGPU->NV12",
            Self::GpuWgpuRgbaCuda => "GPU WGPU->RGBA->CUDA",
            Self::Cpu => "CPU fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrInputFormat {
    Nv12,
    Rgba,
}

impl BrInputFormat {
    fn pix_fmt(self) -> &'static str {
        match self {
            Self::Nv12 => "nv12",
            Self::Rgba => "rgba",
        }
    }

    fn frame_size(self, width: usize, height: usize) -> usize {
        match self {
            Self::Nv12 => width * height * 3 / 2,
            Self::Rgba => width * height * 4,
        }
    }
}

struct BrFrameWriteStats {
    backend: BrRenderBackend,
    frames: u64,
    total: Duration,
    renderer_init: Duration,
    submit: Duration,
    finish_readback: Duration,
    convert: Duration,
    write: Duration,
    cpu_render: Duration,
    gpu_stats: Option<rythmo_gpu_renderer::GpuRenderStats>,
}

impl BrFrameWriteStats {
    fn new() -> Self {
        Self {
            backend: BrRenderBackend::Cpu,
            frames: 0,
            total: Duration::ZERO,
            renderer_init: Duration::ZERO,
            submit: Duration::ZERO,
            finish_readback: Duration::ZERO,
            convert: Duration::ZERO,
            write: Duration::ZERO,
            cpu_render: Duration::ZERO,
            gpu_stats: None,
        }
    }
}

#[derive(Debug)]
struct StdinWriteError {
    kind: ErrorKind,
    message: String,
}

impl StdinWriteError {
    fn new(context: &str, error: std::io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: format!("{context}: {error}"),
        }
    }

    fn render_panic(context: &str) -> Self {
        Self {
            kind: ErrorKind::Other,
            message: context.to_string(),
        }
    }

    fn is_broken_pipe(&self) -> bool {
        self.kind == ErrorKind::BrokenPipe
    }
}

/// Export MP4 with the BR strip baked into the video.
pub fn export_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    export_width: u32,
    export_height: u32,
    replacement_audio: Option<&Path>,
    render_backend_status: Option<Arc<AtomicU32>>,
    progress_cb: impl FnMut(f32) + Send + 'static,
) -> Result<(), String> {
    if !check_ffmpeg() {
        return Err("ffmpeg/ffprobe not found in PATH".into());
    }

    let progress_cb: ProgressCallback = Arc::new(ProgressState {
        callback: Mutex::new(Box::new(progress_cb)),
        reported: AtomicU32::new(0.0_f32.to_bits()),
    });
    if let Some(status) = &render_backend_status {
        status.store(EXPORT_RENDER_BACKEND_UNKNOWN, Ordering::Relaxed);
    }

    export_baked_mp4(
        project,
        source_video,
        output,
        fps,
        source_fps,
        br_scale,
        export_width,
        export_height,
        replacement_audio,
        render_backend_status.as_deref(),
        &progress_cb,
    )
}

fn export_baked_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    export_width: u32,
    export_height: u32,
    replacement_audio: Option<&Path>,
    render_backend_status: Option<&AtomicU32>,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let export_start = Instant::now();
    let fps = valid_export_fps(fps)?;
    let probe_start = Instant::now();
    let info = probe(source_video)?;
    log::info!(
        "Export probe completed in {:.2}ms",
        ms(probe_start.elapsed())
    );
    let out_w = even_dimension(export_width);
    let vid_h = even_dimension(export_height);
    let br_h = rythmo_cpu_renderer::br_height(project, out_w, br_scale);
    let br_h_even = (br_h + 1) & !1;
    let total_frames = (info.duration_secs * fps).ceil() as u64;

    if total_frames == 0 {
        return Err("Video has no duration".into());
    }

    if let Some(audio) = replacement_audio {
        if !audio.is_file() {
            return Err(format!(
                "Replacement audio file not found: {}",
                audio.display()
            ));
        }
        log::info!("Replacing source audio with {}", audio.display());
    }

    let capability_start = Instant::now();
    let use_cuda_graph = has_cuda_filter_graph();
    let use_nvenc = use_cuda_graph || has_nvenc();
    log::info!(
        "Export ffmpeg capability checks completed in {:.2}ms",
        ms(capability_start.elapsed())
    );
    let codec = if use_nvenc { "h264_nvenc" } else { "libx264" };
    log::info!(
        "Using {} encoding, CUDA filter graph={}",
        codec,
        use_cuda_graph
    );
    if use_cuda_graph {
        log::info!("CUDA export graph enabled: scale_cuda + overlay_cuda");
    }
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

    emit_progress(progress_cb, 0.01);

    let use_cuda_rgba_br =
        use_cuda_graph && experimental_cuda_rgba_enabled() && probe_cuda_rgba_br_graph();
    if use_cuda_rgba_br {
        log::info!("Experimental CUDA RGBA BR path enabled: GPU WGPU->RGBA->CUDA");
    } else if use_cuda_graph && experimental_cuda_rgba_enabled() {
        log::warn!("Experimental CUDA RGBA BR path requested but probe failed; using NV12 BR");
    }

    let cuda_filter = if use_cuda_rgba_br {
        cuda_rgba_fit_and_stack_filter(out_w, vid_h, br_h_even, fps, info.duration_secs)
    } else {
        cuda_fit_and_stack_filter(out_w, vid_h, br_h_even, fps, info.duration_secs)
    };
    let cuda_br_input = if use_cuda_rgba_br {
        BrInputFormat::Rgba
    } else {
        BrInputFormat::Nv12
    };
    let cpu_filter = cpu_fit_and_stack_filter(out_w, vid_h, fps, info.duration_secs);
    let replacement_video = replacement_audio.map(|_| temp_video_path(output));
    let video_output = replacement_video.as_deref().unwrap_or(output);
    let include_source_audio = replacement_audio.is_none();

    let result = if use_cuda_graph {
        let r = run_baked_single_pass(
            project,
            source_video,
            video_output,
            &cuda_filter,
            ExportPipeline::Cuda,
            cuda_br_input,
            true,
            fps,
            source_fps,
            br_scale,
            out_w,
            br_h,
            br_h_even,
            total_frames,
            info.duration_secs,
            include_source_audio,
            render_backend_status,
            progress_cb,
        );
        if let Err(e) = r {
            log::warn!("CUDA graph export failed, falling back to CPU filters: {e}");
            emit_progress(progress_cb, 0.01);
            let _ = std::fs::remove_file(video_output);
            run_baked_single_pass(
                project,
                source_video,
                video_output,
                &cpu_filter,
                ExportPipeline::Cpu,
                BrInputFormat::Nv12,
                use_nvenc,
                fps,
                source_fps,
                br_scale,
                out_w,
                br_h,
                br_h_even,
                total_frames,
                info.duration_secs,
                include_source_audio,
                render_backend_status,
                progress_cb,
            )
        } else {
            r
        }
    } else {
        run_baked_single_pass(
            project,
            source_video,
            video_output,
            &cpu_filter,
            ExportPipeline::Cpu,
            BrInputFormat::Nv12,
            use_nvenc,
            fps,
            source_fps,
            br_scale,
            out_w,
            br_h,
            br_h_even,
            total_frames,
            info.duration_secs,
            include_source_audio,
            render_backend_status,
            progress_cb,
        )
    };

    if let Err(e) = result {
        if let Some(temp) = &replacement_video {
            let _ = std::fs::remove_file(temp);
        }
        return Err(e);
    }

    if let (Some(audio), Some(temp_video)) = (replacement_audio, replacement_video.as_deref()) {
        if let Err(e) =
            mux_replacement_audio(temp_video, audio, output, info.duration_secs, progress_cb)
        {
            let _ = std::fs::remove_file(temp_video);
            return Err(e);
        }
        let _ = std::fs::remove_file(temp_video);
    }

    emit_progress(progress_cb, 1.0);
    log::info!(
        "Export complete: {} in {:.2}s",
        output.display(),
        export_start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn valid_export_fps(fps: f64) -> Result<f64, String> {
    if fps.is_finite() && fps > 0.0 {
        Ok(fps)
    } else {
        Err(format!("Invalid export FPS: {fps}"))
    }
}

fn even_dimension(value: u32) -> u32 {
    let clamped = value.clamp(16, 8192);
    if clamped % 2 == 0 {
        clamped
    } else {
        (clamped + 1).min(8192)
    }
}

fn temp_video_path(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    parent.join(format!(
        ".coquerythmo_export_{}_{}.video.mp4",
        std::process::id(),
        stamp
    ))
}

fn cpu_fit_and_stack_filter(out_w: u32, vid_h: u32, fps: f64, duration_secs: f64) -> String {
    format!(
        "[0:v]trim=duration={},setpts=PTS-STARTPTS,scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps={},format=yuv420p[v];[1:v]trim=duration={},setpts=PTS-STARTPTS,format=yuv420p[br];[v][br]vstack=inputs=2:shortest=1[out]",
        duration_secs, out_w, vid_h, out_w, vid_h, fps, duration_secs
    )
}

fn cuda_fit_and_stack_filter(
    out_w: u32,
    vid_h: u32,
    br_h_even: u32,
    fps: f64,
    duration_secs: f64,
) -> String {
    let total_h = vid_h + br_h_even;
    format!(
        "color=c=black:s={}x{}:r={}:d={},format=nv12,hwupload_cuda[canvas];[0:v]scale_cuda=w={}:h={}:format=nv12:force_original_aspect_ratio=decrease:force_divisible_by=2:reset_sar=1[src];[1:v]format=nv12,hwupload_cuda[br];[canvas][src]overlay_cuda=x=(main_w-overlay_w)/2:y=({}-overlay_h)/2:shortest=1[tmp];[tmp][br]overlay_cuda=x=0:y={}:shortest=1[out]",
        out_w, total_h, fps, duration_secs, out_w, vid_h, vid_h, vid_h
    )
}

fn cuda_rgba_fit_and_stack_filter(
    out_w: u32,
    vid_h: u32,
    br_h_even: u32,
    fps: f64,
    duration_secs: f64,
) -> String {
    let total_h = vid_h + br_h_even;
    format!(
        "color=c=black:s={}x{}:r={}:d={},format=nv12,hwupload_cuda[canvas];[0:v]scale_cuda=w={}:h={}:format=nv12:force_original_aspect_ratio=decrease:force_divisible_by=2:reset_sar=1[src];[1:v]format=rgba,hwupload_cuda,scale_cuda=w={}:h={}:format=nv12:passthrough=0[br];[canvas][src]overlay_cuda=x=(main_w-overlay_w)/2:y=({}-overlay_h)/2:shortest=1[tmp];[tmp][br]overlay_cuda=x=0:y={}:shortest=1[out]",
        out_w,
        total_h,
        fps,
        duration_secs,
        out_w,
        vid_h,
        out_w,
        br_h_even,
        vid_h,
        vid_h
    )
}

fn run_baked_single_pass(
    project: &Project,
    source_video: &Path,
    output: &Path,
    filter: &str,
    pipeline: ExportPipeline,
    br_input_format: BrInputFormat,
    use_nvenc: bool,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    out_w: u32,
    br_h: u32,
    br_h_even: u32,
    total_frames: u64,
    duration_secs: f64,
    include_source_audio: bool,
    render_backend_status: Option<&AtomicU32>,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let pass_start = Instant::now();
    let use_cuda = pipeline.uses_cuda();
    let codec = if use_nvenc { "h264_nvenc" } else { "libx264" };
    let raw_size = format!("{}x{}", out_w, br_h_even);
    let fps_arg = fps.to_string();

    let mut cmd = Command::new("ffmpeg");
    if use_cuda {
        cmd.args(["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"]);
    }
    cmd.args(["-thread_queue_size", "1024"])
        .arg("-i")
        .arg(source_video)
        .args([
            "-thread_queue_size",
            "1024",
            "-f",
            "rawvideo",
            "-pix_fmt",
            br_input_format.pix_fmt(),
            "-s",
            &raw_size,
            "-r",
            &fps_arg,
            "-i",
            "pipe:0",
        ]);

    cmd.args(["-filter_complex", filter, "-map", "[out]"])
        .args(if include_source_audio {
            vec!["-map", "0:a?"]
        } else {
            Vec::new()
        })
        .args(if use_nvenc {
            vec![
                "-c:v", codec, "-preset", "p2", "-rc", "constqp", "-qp", "20", "-b:v", "0",
            ]
        } else {
            vec!["-c:v", codec, "-preset", "ultrafast", "-crf", "20"]
        });

    if !use_cuda {
        cmd.args(["-pix_fmt", "yuv420p"]);
    }

    if include_source_audio {
        cmd.args(["-c:a", "copy"]);
    }

    let spawn_start = Instant::now();
    let mut child = cmd
        .args(["-progress", "pipe:1", "-nostats", "-v", "warning", "-y"])
        .arg(output)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg single-pass: {e}"))?;
    log::info!(
        "ffmpeg spawned for {} in {:.2}ms",
        pipeline.label(),
        ms(spawn_start.elapsed())
    );

    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut text);
            text
        })
    });

    let ffmpeg_progress = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
    let stdout_handle = child.stdout.take().map(|stdout| {
        let ffmpeg_progress = ffmpeg_progress.clone();
        let progress_cb = progress_cb.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(progress) = ffmpeg_progress_from_line(&line, duration_secs) {
                    let progress = map_progress(progress, 0.01, 0.985);
                    if store_progress_max(&ffmpeg_progress, progress) {
                        emit_progress(&progress_cb, progress);
                    }
                }
            }
        })
    });

    let Some(br_stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("ffmpeg single-pass: stdin unavailable".into());
    };

    let w = out_w as usize;
    let h = br_h_even as usize;
    let raw_frame_size = br_input_format.frame_size(w, h);
    let mut writer = std::io::BufWriter::with_capacity(raw_frame_size * 8, br_stdin);
    let mut br_stats = None;
    let write_result = write_br_frames(
        project,
        &mut writer,
        br_input_format,
        fps,
        source_fps,
        br_scale,
        out_w,
        br_h,
        br_h_even,
        total_frames,
        &ffmpeg_progress,
        render_backend_status,
        progress_cb,
    );
    let mut flush_duration = Duration::ZERO;
    let write_result = match write_result {
        Ok(stats) => {
            br_stats = Some(stats);
            let flush_start = Instant::now();
            let flush_result = writer
                .flush()
                .map_err(|e| StdinWriteError::new("ffmpeg stdin flush", e));
            flush_duration = flush_start.elapsed();
            flush_result
        }
        Err(e) => Err(e),
    };
    drop(writer);

    if write_result.is_ok() {
        emit_progress(progress_cb, 0.99);
    }

    let wait_start = Instant::now();
    let status = child
        .wait()
        .map_err(|e| format!("ffmpeg single-pass wait: {e}"))?;
    let ffmpeg_wait = wait_start.elapsed();
    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if let Some(stats) = &br_stats {
        log_baked_export_summary(
            stats,
            pipeline,
            codec,
            pass_start.elapsed(),
            ffmpeg_wait,
            flush_duration,
        );
    }

    if !status.success() {
        let details = stderr.trim();
        if details.is_empty() {
            Err(format!(
                "ffmpeg single-pass failed (pipeline={:?}, filter={})",
                pipeline, filter
            ))
        } else {
            Err(format!(
                "ffmpeg single-pass failed (pipeline={:?}, filter={}): {}",
                pipeline, filter, details
            ))
        }
    } else if let Err(e) = write_result {
        if e.is_broken_pipe() {
            log::info!(
                "ffmpeg closed stdin after successful export; treating as complete: {}",
                e.message
            );
            Ok(())
        } else {
            Err(e.message)
        }
    } else {
        Ok(())
    }
}

fn mux_replacement_audio(
    video_only: &Path,
    replacement_audio: &Path,
    output: &Path,
    duration_secs: f64,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let duration_arg = duration_secs.to_string();
    let audio_filter =
        format!("apad=whole_dur={duration_arg},atrim=duration={duration_arg},asetpts=PTS-STARTPTS");

    emit_progress(progress_cb, 0.99);

    let mut child = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_only)
        .arg("-i")
        .arg(replacement_audio)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-af",
            &audio_filter,
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
        .map_err(|e| format!("ffmpeg mux replacement audio: {e}"))?;

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
            if let Some(progress) = ffmpeg_progress_from_line(&line, duration_secs) {
                emit_progress(progress_cb, map_progress(progress, 0.99, 0.999));
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("ffmpeg mux replacement audio wait: {e}"))?;
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    if status.success() {
        Ok(())
    } else {
        let details = stderr.trim();
        if details.is_empty() {
            Err("ffmpeg mux replacement audio failed".into())
        } else {
            Err(format!("ffmpeg mux replacement audio failed: {details}"))
        }
    }
}

fn write_br_frames(
    project: &Project,
    writer: &mut impl Write,
    br_input_format: BrInputFormat,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    out_w: u32,
    br_h: u32,
    br_h_even: u32,
    total_frames: u64,
    ffmpeg_progress: &AtomicU32,
    render_backend_status: Option<&AtomicU32>,
    progress_cb: &ProgressCallback,
) -> Result<BrFrameWriteStats, StdinWriteError> {
    let total_start = Instant::now();
    let mut stats = BrFrameWriteStats::new();
    let w = out_w as usize;
    let h = br_h_even as usize;
    let nv12_frame_size = w * h * 3 / 2;
    let mut nv12_buf = vec![0u8; nv12_frame_size];
    let mut rgba_buf = Vec::new();
    let mut rgba_pipe_buf = Vec::new();
    let uv_off = w * h;
    let frame_ratio = if source_fps.is_finite() && source_fps > 0.0 {
        source_fps / fps
    } else {
        1.0
    };
    let progress_interval = fps.round().max(1.0) as u64;

    log::info!(
        "Frame ratio: source {:.2}fps / export {:.2}fps = {:.4}",
        source_fps,
        fps,
        frame_ratio
    );

    let gpu_init_start = Instant::now();
    let gpu_result = rythmo_gpu_renderer::GpuRenderer::new();
    stats.renderer_init += gpu_init_start.elapsed();

    match gpu_result {
        Ok(mut gpu) => {
            stats.backend = match br_input_format {
                BrInputFormat::Nv12 => BrRenderBackend::GpuWgpuNv12,
                BrInputFormat::Rgba => BrRenderBackend::GpuWgpuRgbaCuda,
            };
            log::info!("Single-pass BR render: {}", stats.backend.label());
            if let Some(status) = render_backend_status {
                status.store(EXPORT_RENDER_BACKEND_GPU, Ordering::Relaxed);
            }
            let scene = rythmo_gpu_renderer::GpuExportScene::new(project);
            let submit_start = Instant::now();
            match br_input_format {
                BrInputFormat::Nv12 => {
                    gpu.submit_render_nv12(&scene, 0.0, out_w, fps, br_scale, br_h_even);
                }
                BrInputFormat::Rgba => gpu.submit_render(&scene, 0.0, out_w, fps, br_scale),
            }
            stats.submit += submit_start.elapsed();

            for frame in 1..total_frames as i64 {
                let finish_start = Instant::now();
                match br_input_format {
                    BrInputFormat::Nv12 => gpu.finish_render_nv12_into(&mut nv12_buf),
                    BrInputFormat::Rgba => gpu.finish_render_into(out_w, br_h, &mut rgba_buf),
                }
                stats.finish_readback += finish_start.elapsed();
                let video_pos = frame as f64 * frame_ratio;
                let submit_start = Instant::now();
                match br_input_format {
                    BrInputFormat::Nv12 => {
                        gpu.submit_render_nv12(&scene, video_pos, out_w, fps, br_scale, br_h_even);
                    }
                    BrInputFormat::Rgba => {
                        gpu.submit_render(&scene, video_pos, out_w, fps, br_scale);
                    }
                }
                stats.submit += submit_start.elapsed();

                let frame_bytes = match br_input_format {
                    BrInputFormat::Nv12 => nv12_buf.as_slice(),
                    BrInputFormat::Rgba => {
                        let pad_start = Instant::now();
                        rgba_pad_to_even(&rgba_buf, &mut rgba_pipe_buf, w, br_h as usize, h);
                        stats.convert += pad_start.elapsed();
                        rgba_pipe_buf.as_slice()
                    }
                };
                let write_start = Instant::now();
                writer
                    .write_all(frame_bytes)
                    .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                stats.write += write_start.elapsed();
                stats.frames += 1;

                if frame as u64 % progress_interval == 0 {
                    report_baked_progress(frame as u64, total_frames, ffmpeg_progress, progress_cb);
                }
            }

            let finish_start = Instant::now();
            match br_input_format {
                BrInputFormat::Nv12 => gpu.finish_render_nv12_into(&mut nv12_buf),
                BrInputFormat::Rgba => gpu.finish_render_into(out_w, br_h, &mut rgba_buf),
            }
            stats.finish_readback += finish_start.elapsed();
            let frame_bytes = match br_input_format {
                BrInputFormat::Nv12 => nv12_buf.as_slice(),
                BrInputFormat::Rgba => {
                    let pad_start = Instant::now();
                    rgba_pad_to_even(&rgba_buf, &mut rgba_pipe_buf, w, br_h as usize, h);
                    stats.convert += pad_start.elapsed();
                    rgba_pipe_buf.as_slice()
                }
            };
            let write_start = Instant::now();
            writer
                .write_all(frame_bytes)
                .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
            stats.write += write_start.elapsed();
            stats.frames += 1;
            stats.gpu_stats = Some(gpu.stats());
            report_baked_progress(total_frames, total_frames, ffmpeg_progress, progress_cb);
        }
        Err(e) => {
            stats.backend = BrRenderBackend::Cpu;
            log::warn!("GPU unavailable ({}), CPU fallback", e);
            if let Some(status) = render_backend_status {
                status.store(EXPORT_RENDER_BACKEND_CPU, Ordering::Relaxed);
            }
            let cpu_init_start = Instant::now();
            let n_threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            log::info!("CPU render: {} threads", n_threads);

            let mut renderers: Vec<rythmo_cpu_renderer::CpuRenderer> = (0..n_threads)
                .map(|_| rythmo_cpu_renderer::CpuRenderer::new())
                .collect();
            stats.renderer_init += cpu_init_start.elapsed();
            let frame_indices: Vec<i64> = (0..total_frames as i64).collect();

            for batch in frame_indices.chunks(n_threads) {
                let render_start = Instant::now();
                let rendered: Result<Vec<Vec<u8>>, StdinWriteError> = std::thread::scope(|scope| {
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
                    handles
                        .into_iter()
                        .map(|h| {
                            h.join().map_err(|_| {
                                StdinWriteError::render_panic("CPU BR renderer panicked")
                            })
                        })
                        .collect()
                });
                stats.cpu_render += render_start.elapsed();
                let rendered = rendered?;

                for (i, rgba) in rendered.iter().enumerate() {
                    let frame = batch[i];
                    let convert_start = Instant::now();
                    let frame_bytes = match br_input_format {
                        BrInputFormat::Nv12 => {
                            rgba_to_nv12(rgba, &mut nv12_buf, w, h, br_h as usize, uv_off);
                            nv12_buf.as_slice()
                        }
                        BrInputFormat::Rgba => {
                            rgba_pad_to_even(rgba, &mut rgba_pipe_buf, w, br_h as usize, h);
                            rgba_pipe_buf.as_slice()
                        }
                    };
                    stats.convert += convert_start.elapsed();
                    let write_start = Instant::now();
                    writer
                        .write_all(frame_bytes)
                        .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                    stats.write += write_start.elapsed();
                    stats.frames += 1;
                    let completed = frame as u64 + 1;
                    if completed % progress_interval == 0 || completed == total_frames {
                        report_baked_progress(
                            completed,
                            total_frames,
                            ffmpeg_progress,
                            progress_cb,
                        );
                    }
                }
            }
        }
    }

    stats.total = total_start.elapsed();
    Ok(stats)
}

fn report_baked_progress(
    completed_frames: u64,
    total_frames: u64,
    ffmpeg_progress: &AtomicU32,
    progress_cb: &ProgressCallback,
) {
    if total_frames == 0 {
        return;
    }
    let raw = (completed_frames.min(total_frames) as f32 / total_frames as f32).clamp(0.0, 1.0);
    let writer_progress = map_progress(raw, 0.01, 0.985);
    let pipe_progress = f32::from_bits(ffmpeg_progress.load(Ordering::Relaxed));
    emit_progress(
        progress_cb,
        writer_progress.max(pipe_progress).clamp(0.01, 0.985),
    );
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn avg_ms(duration: Duration, frames: u64) -> f64 {
    if frames == 0 {
        0.0
    } else {
        ms(duration) / frames as f64
    }
}

fn fps(frames: u64, duration: Duration) -> f64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        0.0
    } else {
        frames as f64 / secs
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn log_baked_export_summary(
    stats: &BrFrameWriteStats,
    pipeline: ExportPipeline,
    codec: &str,
    ffmpeg_total: Duration,
    ffmpeg_wait: Duration,
    flush: Duration,
) {
    log::info!(
        "Export timing summary: pipeline={}, codec={}, BR backend={}, frames={}, BR {:.2}s ({:.1} fps), ffmpeg {:.2}s ({:.1} fps)",
        pipeline.label(),
        codec,
        stats.backend.label(),
        stats.frames,
        stats.total.as_secs_f64(),
        fps(stats.frames, stats.total),
        ffmpeg_total.as_secs_f64(),
        fps(stats.frames, ffmpeg_total),
    );
    log::info!(
        "Export timing detail: init {:.2}ms, submit {:.2}ms/frame, readback {:.2}ms/frame, CPU convert/pad {:.2}ms/frame, pipe write {:.2}ms/frame, CPU render {:.2}ms/frame, stdin flush {:.2}ms, ffmpeg wait {:.2}s",
        ms(stats.renderer_init),
        avg_ms(stats.submit, stats.frames),
        avg_ms(stats.finish_readback, stats.frames),
        avg_ms(stats.convert, stats.frames),
        avg_ms(stats.write, stats.frames),
        avg_ms(stats.cpu_render, stats.frames),
        ms(flush),
        ffmpeg_wait.as_secs_f64(),
    );
    if let Some(gpu) = &stats.gpu_stats {
        let avg_draw_calls = if gpu.frames_submitted == 0 {
            0.0
        } else {
            gpu.draw_calls as f64 / gpu.frames_submitted as f64
        };
        log::info!(
            "GPU export stats: submitted={}, avg_draw_calls={:.1}, last_draw_calls={}, last_quads={}, last_icons={}, last_icon_batches={}, textures={}, bind_groups={}, text_uploads={} ({:.2}ms), icon_uploads={} ({:.2}ms), readback={:.2}MiB total (last {} bytes)",
            gpu.frames_submitted,
            avg_draw_calls,
            gpu.last_frame_draw_calls,
            gpu.last_frame_quads,
            gpu.last_frame_icons,
            gpu.last_frame_icon_batches,
            gpu.texture_creations,
            gpu.bind_groups_created,
            gpu.text_uploads,
            ms(gpu.text_upload_time),
            gpu.icon_uploads,
            ms(gpu.icon_upload_time),
            mib(gpu.total_readback_bytes),
            gpu.last_readback_bytes,
        );
    }
}

fn map_progress(progress: f32, start: f32, end: f32) -> f32 {
    start + progress.clamp(0.0, 1.0) * (end - start)
}

fn store_progress_max(progress: &AtomicU32, value: f32) -> bool {
    let value = value.clamp(0.0, 1.0);
    let mut current = progress.load(Ordering::Relaxed);
    loop {
        if value <= f32::from_bits(current) {
            return false;
        }
        match progress.compare_exchange(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(next) => current = next,
        }
    }
}

fn ffmpeg_progress_from_line(line: &str, duration_secs: f64) -> Option<f32> {
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

fn rgba_pad_to_even(rgba: &[u8], out: &mut Vec<u8>, w: usize, br_h: usize, h: usize) {
    let row_bytes = w * 4;
    let rendered_len = row_bytes * br_h;
    let total = row_bytes * h;

    out.clear();
    out.extend_from_slice(&rgba[..rendered_len.min(rgba.len())]);
    if out.len() < total {
        let first_padded_pixel = out.len() / 4;
        out.resize(total, 0);
        for pixel in first_padded_pixel..(total / 4) {
            out[pixel * 4 + 3] = 255;
        }
    }
}

/// Convert RGBA pixels to NV12.
fn rgba_to_nv12(rgba: &[u8], nv12_buf: &mut [u8], w: usize, h: usize, br_h: usize, uv_off: usize) {
    for y in 0..br_h {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let (r, g, b) = (rgba[si] as i32, rgba[si + 1] as i32, rgba[si + 2] as i32);
            nv12_buf[y * w + x] =
                (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
        }
    }
    for y in br_h..h {
        for x in 0..w {
            nv12_buf[y * w + x] = 16;
        }
    }
    for cy in 0..h / 2 {
        for cx in 0..w / 2 {
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
            let uv_i = uv_off + cy * w + cx * 2;
            nv12_buf[uv_i] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            nv12_buf[uv_i + 1] =
                (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
}
