//! FFmpeg process and filter graph helpers.
#![allow(clippy::too_many_arguments)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::media_binary;
use crate::project::Project;

use super::frame_source;
use super::progress;
use super::progress::ProgressCallback;
use super::types::{BrFrameWriteStats, BrInputFormat, ExportPipeline, StdinWriteError};

pub(super) fn cpu_fit_and_stack_filter(
    out_w: u32,
    vid_h: u32,
    fps: f64,
    duration_secs: f64,
) -> String {
    format!(
        "[0:v]trim=duration={},setpts=PTS-STARTPTS,scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps={},format=yuv420p[v];[1:v]trim=duration={},setpts=PTS-STARTPTS,format=yuv420p[br];[v][br]vstack=inputs=2:shortest=1[out]",
        duration_secs, out_w, vid_h, out_w, vid_h, fps, duration_secs
    )
}

pub(super) fn cuda_fit_and_stack_filter(
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

pub(super) fn cuda_rgba_fit_and_stack_filter(
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

pub(super) fn run_baked_single_pass(
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
    karaoke_text_scale: f32,
    out_w: u32,
    br_h: u32,
    br_h_even: u32,
    total_frames: u64,
    duration_secs: f64,
    include_source_audio: bool,
    render_backend_status: Option<&AtomicU32>,
    cancel: &AtomicBool,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let pass_start = Instant::now();
    progress::check_export_cancel(cancel)?;
    let use_cuda = pipeline.uses_cuda();
    let codec = if use_nvenc { "h264_nvenc" } else { "libx264" };
    let raw_size = format!("{}x{}", out_w, br_h_even);
    let fps_arg = fps.to_string();

    let mut cmd = media_binary::command("ffmpeg");
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
        progress::ms(spawn_start.elapsed())
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
                if let Some(progress) = progress::ffmpeg_progress_from_line(&line, duration_secs) {
                    let progress = progress::map_progress(progress, 0.01, 0.985);
                    if progress::store_progress_max(&ffmpeg_progress, progress) {
                        progress::emit_progress(&progress_cb, progress);
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
    let write_result = frame_source::write_br_frames(
        project,
        &mut writer,
        br_input_format,
        fps,
        source_fps,
        br_scale,
        karaoke_text_scale,
        out_w,
        br_h,
        br_h_even,
        total_frames,
        &ffmpeg_progress,
        render_backend_status,
        cancel,
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

    let cancelled = write_result
        .as_ref()
        .err()
        .is_some_and(StdinWriteError::is_cancelled)
        || progress::export_cancelled(cancel);
    if cancelled {
        let _ = child.kill();
    }

    if write_result.is_ok() && !cancelled {
        progress::emit_progress(progress_cb, 0.99);
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

    if cancelled {
        let _ = std::fs::remove_file(output);
        return Err(super::EXPORT_CANCELLED_MESSAGE.into());
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
        progress::fps(stats.frames, stats.total),
        ffmpeg_total.as_secs_f64(),
        progress::fps(stats.frames, ffmpeg_total),
    );
    log::info!(
        "Export timing detail: init {:.2}ms, submit {:.2}ms/frame, readback {:.2}ms/frame, CPU convert/pad {:.2}ms/frame, pipe write {:.2}ms/frame, CPU render {:.2}ms/frame, stdin flush {:.2}ms, ffmpeg wait {:.2}s",
        progress::ms(stats.renderer_init),
        progress::avg_ms(stats.submit, stats.frames),
        progress::avg_ms(stats.finish_readback, stats.frames),
        progress::avg_ms(stats.convert, stats.frames),
        progress::avg_ms(stats.write, stats.frames),
        progress::avg_ms(stats.cpu_render, stats.frames),
        progress::ms(flush),
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
            progress::ms(gpu.text_upload_time),
            gpu.icon_uploads,
            progress::ms(gpu.icon_upload_time),
            progress::mib(gpu.total_readback_bytes),
            gpu.last_readback_bytes,
        );
    }
}
