//! High-level video export orchestration.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_is_multiple_of)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::project::Project;
use crate::rythmo_cpu_renderer;

use super::audio::{
    instrumental_output_path, mux_instrumental_audio, mux_source_audio, temp_video_path,
};
use super::capabilities::{
    check_ffmpeg, experimental_cuda_rgba_enabled, has_cuda_filter_graph, probe,
    probe_cuda_rgba_br_graph,
};
use super::ffmpeg::{
    cpu_fit_and_stack_filter, cuda_fit_and_stack_filter, cuda_rgba_fit_and_stack_filter,
    run_baked_single_pass,
};
use super::progress::{
    check_export_cancel, emit_progress, is_cancelled_error, ms, ProgressCallback, ProgressState,
};
use super::types::{BrInputFormat, ExportPipeline};

/// Export MP4 with the BR strip baked into the video.
///
/// When `instrumental_audio` is provided, the selected output uses that audio.
/// If `double_export_instrumental` is true, a normal source-audio output is also
/// written at the selected path and the instrumental output is written next to
/// it with an `_instrumental` suffix.
pub fn export_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    karaoke_text_scale: f32,
    export_width: u32,
    export_height: u32,
    instrumental_audio: Option<&Path>,
    source_audio_offset_frames: i64,
    instrumental_audio_offset_frames: i64,
    double_export_instrumental: bool,
    pre_roll_seconds: f64,
    render_backend_status: Option<Arc<AtomicU32>>,
    cancel: Arc<AtomicBool>,
    progress_cb: impl FnMut(f32) + Send + 'static,
) -> Result<(), String> {
    if !check_ffmpeg() {
        return Err("ffmpeg/ffprobe not found beside app or in PATH".into());
    }
    // The UI scale is relative to the production defaults: the former 50% BR
    // height and 200% karaoke size are now presented as 100%.
    let (br_scale, karaoke_text_scale) = effective_export_scales(br_scale, karaoke_text_scale);

    let progress_cb: ProgressCallback = Arc::new(ProgressState {
        callback: Mutex::new(Box::new(progress_cb)),
        reported: AtomicU32::new(0.0_f32.to_bits()),
    });
    if let Some(status) = &render_backend_status {
        status.store(super::EXPORT_RENDER_BACKEND_UNKNOWN, Ordering::Relaxed);
    }
    check_export_cancel(&cancel)?;

    export_baked_mp4(
        project,
        source_video,
        output,
        fps,
        source_fps,
        br_scale,
        karaoke_text_scale,
        export_width,
        export_height,
        instrumental_audio,
        source_audio_offset_frames,
        instrumental_audio_offset_frames,
        double_export_instrumental,
        pre_roll_seconds,
        render_backend_status.as_deref(),
        &cancel,
        &progress_cb,
    )
}

fn effective_export_scales(br_scale: f32, karaoke_text_scale: f32) -> (f32, f32) {
    let br = if br_scale.is_finite() {
        br_scale.clamp(0.5, 2.0) * 0.5
    } else {
        0.5
    };
    let karaoke = if karaoke_text_scale.is_finite() {
        karaoke_text_scale.clamp(0.5, 2.0) * 2.0
    } else {
        2.0
    };
    (br, karaoke)
}

pub(super) fn export_baked_mp4(
    project: &Project,
    source_video: &Path,
    output: &Path,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    karaoke_text_scale: f32,
    export_width: u32,
    export_height: u32,
    instrumental_audio: Option<&Path>,
    source_audio_offset_frames: i64,
    instrumental_audio_offset_frames: i64,
    double_export_instrumental: bool,
    pre_roll_seconds: f64,
    render_backend_status: Option<&AtomicU32>,
    cancel: &AtomicBool,
    progress_cb: &ProgressCallback,
) -> Result<(), String> {
    let export_start = Instant::now();
    check_export_cancel(cancel)?;
    let fps = valid_export_fps(fps)?;
    let pre_roll_seconds = if pre_roll_seconds.is_finite() {
        pre_roll_seconds.clamp(0.0, 120.0)
    } else {
        0.0
    };
    let probe_start = Instant::now();
    let info = probe(source_video)?;
    check_export_cancel(cancel)?;
    log::info!(
        "Export probe completed in {:.2}ms",
        ms(probe_start.elapsed())
    );
    let out_w = even_dimension(export_width);
    let br_h = rythmo_cpu_renderer::br_height(project, out_w, br_scale);
    let br_h_even = (br_h + 1) & !1;
    let vid_h = (even_dimension(export_height).saturating_sub(br_h_even)).max(2);
    let total_duration_secs = info.duration_secs + pre_roll_seconds;
    let total_frames = (total_duration_secs * fps).ceil() as u64;
    let timeline_start_source_frame = -pre_roll_seconds * source_fps;
    let pre_roll_audio_frames = (pre_roll_seconds * fps).round() as i64;

    if total_frames == 0 {
        return Err("Video has no duration".into());
    }

    if let Some(audio) = instrumental_audio {
        if !audio.is_file() {
            return Err(format!(
                "Instrumental audio file not found: {}",
                audio.display()
            ));
        }
        if double_export_instrumental {
            log::info!(
                "Double MP4 export enabled: normal output={}, instrumental audio={}",
                output.display(),
                audio.display()
            );
        } else {
            log::info!(
                "Instrumental-only MP4 export enabled: output={}, instrumental audio={}",
                output.display(),
                audio.display()
            );
        }
    }

    let capability_start = Instant::now();
    let use_cuda_graph = has_cuda_filter_graph();
    log::info!(
        "Export ffmpeg capability checks completed in {:.2}ms",
        ms(capability_start.elapsed())
    );
    let codec = if use_cuda_graph {
        "h264_nvenc"
    } else {
        "libx264"
    };
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
    check_export_cancel(cancel)?;

    let use_cuda_rgba_br =
        use_cuda_graph && experimental_cuda_rgba_enabled() && probe_cuda_rgba_br_graph();
    if use_cuda_rgba_br {
        log::info!("Experimental CUDA RGBA BR path enabled: GPU WGPU->RGBA->CUDA");
    } else if use_cuda_graph && experimental_cuda_rgba_enabled() {
        log::warn!("Experimental CUDA RGBA BR path requested but probe failed; using NV12 BR");
    }

    let cuda_filter = if use_cuda_rgba_br {
        cuda_rgba_fit_and_stack_filter(
            out_w,
            vid_h,
            br_h_even,
            fps,
            info.duration_secs,
            pre_roll_seconds,
        )
    } else {
        cuda_fit_and_stack_filter(
            out_w,
            vid_h,
            br_h_even,
            fps,
            info.duration_secs,
            pre_roll_seconds,
        )
    };
    let cuda_br_input = if use_cuda_rgba_br {
        BrInputFormat::Rgba
    } else {
        BrInputFormat::Nv12
    };
    let cpu_filter =
        cpu_fit_and_stack_filter(out_w, vid_h, fps, info.duration_secs, pre_roll_seconds);
    let needs_audio_mux =
        instrumental_audio.is_some() || source_audio_offset_frames != 0 || pre_roll_seconds > 0.0;
    let temp_video = needs_audio_mux.then(|| temp_video_path(output));
    let video_output = temp_video.as_deref().unwrap_or(output);
    let include_source_audio = !needs_audio_mux;

    let result = if use_cuda_graph {
        let r = run_baked_single_pass(
            project,
            source_video,
            video_output,
            &cuda_filter,
            ExportPipeline::Cuda,
            cuda_br_input,
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            out_w,
            br_h,
            br_h_even,
            total_frames,
            total_duration_secs,
            timeline_start_source_frame,
            include_source_audio,
            render_backend_status,
            cancel,
            progress_cb,
        );
        if let Err(e) = r {
            if is_cancelled_error(&e) {
                return Err(e);
            }
            log::warn!("CUDA graph export failed, falling back to CPU filters: {e}");
            emit_progress(progress_cb, 0.01);
            let _ = std::fs::remove_file(video_output);
            check_export_cancel(cancel)?;
            run_baked_single_pass(
                project,
                source_video,
                video_output,
                &cpu_filter,
                ExportPipeline::Cpu,
                BrInputFormat::Nv12,
                fps,
                source_fps,
                br_scale,
                karaoke_text_scale,
                out_w,
                br_h,
                br_h_even,
                total_frames,
                total_duration_secs,
                timeline_start_source_frame,
                include_source_audio,
                render_backend_status,
                cancel,
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
            fps,
            source_fps,
            br_scale,
            karaoke_text_scale,
            out_w,
            br_h,
            br_h_even,
            total_frames,
            total_duration_secs,
            timeline_start_source_frame,
            include_source_audio,
            render_backend_status,
            cancel,
            progress_cb,
        )
    };

    if let Err(e) = result {
        if let Some(temp) = &temp_video {
            let _ = std::fs::remove_file(temp);
        }
        return Err(e);
    }

    if let Some(temp) = temp_video.as_deref() {
        check_export_cancel(cancel)?;
        if let Some(audio) = instrumental_audio {
            let instrumental_output = if double_export_instrumental {
                let output_path = instrumental_output_path(output);
                if let Err(e) = mux_source_audio(
                    temp,
                    source_video,
                    output,
                    total_duration_secs,
                    source_audio_offset_frames + pre_roll_audio_frames,
                    fps,
                    cancel,
                ) {
                    let _ = std::fs::remove_file(temp);
                    return Err(e);
                }
                output_path
            } else {
                output.to_path_buf()
            };
            if let Err(e) = mux_instrumental_audio(
                temp,
                audio,
                &instrumental_output,
                total_duration_secs,
                instrumental_audio_offset_frames + pre_roll_audio_frames,
                fps,
                cancel,
                progress_cb,
            ) {
                let _ = std::fs::remove_file(temp);
                return Err(e);
            }
            log::info!(
                "Instrumental MP4 export complete: {}",
                instrumental_output.display()
            );
        } else {
            if let Err(e) = mux_source_audio(
                temp,
                source_video,
                output,
                total_duration_secs,
                source_audio_offset_frames + pre_roll_audio_frames,
                fps,
                cancel,
            ) {
                let _ = std::fs::remove_file(temp);
                return Err(e);
            }
        }
        let _ = std::fs::remove_file(temp);
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

#[cfg(test)]
mod tests {
    use super::effective_export_scales;

    #[test]
    fn one_hundred_percent_uses_new_export_bases() {
        assert_eq!(effective_export_scales(1.0, 1.0), (0.5, 2.0));
        assert_eq!(effective_export_scales(0.5, 0.5), (0.25, 1.0));
        assert_eq!(effective_export_scales(2.0, 2.0), (1.0, 4.0));
    }
}
