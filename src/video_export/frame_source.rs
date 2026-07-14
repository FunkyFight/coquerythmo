//! BR frame generation for the export pipeline.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_is_multiple_of)]

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use crate::project::Project;
use crate::{rythmo_cpu_renderer, rythmo_gpu_renderer};

use super::progress::{check_stdin_cancel, report_baked_progress, ProgressCallback};
use super::types::{BrFrameWriteStats, BrInputFormat, BrRenderBackend, StdinWriteError};

pub(super) fn write_br_frames(
    project: &Project,
    writer: &mut impl Write,
    br_input_format: BrInputFormat,
    fps: f64,
    source_fps: f64,
    br_scale: f32,
    karaoke_text_scale: f32,
    out_w: u32,
    br_h: u32,
    br_h_even: u32,
    total_frames: u64,
    timeline_start_source_frame: f64,
    ffmpeg_progress: &AtomicU32,
    render_backend_status: Option<&AtomicU32>,
    cancel: &AtomicBool,
    progress_cb: &ProgressCallback,
) -> Result<BrFrameWriteStats, StdinWriteError> {
    let total_start = Instant::now();
    check_stdin_cancel(cancel)?;
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
    check_stdin_cancel(cancel)?;

    match gpu_result {
        Ok(mut gpu) => {
            stats.backend = match br_input_format {
                BrInputFormat::Nv12 => BrRenderBackend::GpuWgpuNv12,
                BrInputFormat::Rgba => BrRenderBackend::GpuWgpuRgbaCuda,
            };
            log::info!("Single-pass BR render: {}", stats.backend.label());
            if let Some(status) = render_backend_status {
                status.store(super::EXPORT_RENDER_BACKEND_GPU, Ordering::Relaxed);
            }
            let scene = rythmo_gpu_renderer::GpuExportScene::new(project);
            check_stdin_cancel(cancel)?;
            let submit_start = Instant::now();
            match br_input_format {
                BrInputFormat::Nv12 => {
                    gpu.submit_render_nv12(
                        &scene,
                        timeline_start_source_frame,
                        out_w,
                        fps,
                        source_fps,
                        br_scale,
                        karaoke_text_scale,
                        br_h_even,
                    );
                }
                BrInputFormat::Rgba => gpu.submit_render(
                    &scene,
                    timeline_start_source_frame,
                    out_w,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                ),
            }
            stats.submit += submit_start.elapsed();

            for frame in 1..total_frames as i64 {
                check_stdin_cancel(cancel)?;
                if br_input_format == BrInputFormat::Nv12 {
                    let finish_start = Instant::now();
                    gpu.finish_render_nv12_into(&mut nv12_buf);
                    stats.finish_readback += finish_start.elapsed();

                    check_stdin_cancel(cancel)?;
                    let video_pos = timeline_start_source_frame + frame as f64 * frame_ratio;
                    let submit_start = Instant::now();
                    gpu.submit_render_nv12(
                        &scene,
                        video_pos,
                        out_w,
                        fps,
                        source_fps,
                        br_scale,
                        karaoke_text_scale,
                        br_h_even,
                    );
                    stats.submit += submit_start.elapsed();

                    let write_start = Instant::now();
                    check_stdin_cancel(cancel)?;
                    writer
                        .write_all(nv12_buf.as_slice())
                        .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                    stats.write += write_start.elapsed();
                    stats.frames += 1;

                    if frame as u64 % progress_interval == 0 {
                        report_baked_progress(
                            frame as u64,
                            total_frames,
                            ffmpeg_progress,
                            progress_cb,
                        );
                    }
                    continue;
                }

                let finish_start = Instant::now();
                gpu.finish_render_into(out_w, br_h, &mut rgba_buf);
                stats.finish_readback += finish_start.elapsed();
                check_stdin_cancel(cancel)?;
                let video_pos = timeline_start_source_frame + frame as f64 * frame_ratio;
                let submit_start = Instant::now();
                gpu.submit_render(
                    &scene,
                    video_pos,
                    out_w,
                    fps,
                    source_fps,
                    br_scale,
                    karaoke_text_scale,
                );
                stats.submit += submit_start.elapsed();

                let pad_start = Instant::now();
                rgba_pad_to_even(&rgba_buf, &mut rgba_pipe_buf, w, br_h as usize, h);
                stats.convert += pad_start.elapsed();
                let frame_bytes = rgba_pipe_buf.as_slice();
                let write_start = Instant::now();
                check_stdin_cancel(cancel)?;
                writer
                    .write_all(frame_bytes)
                    .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                stats.write += write_start.elapsed();
                stats.frames += 1;

                if frame as u64 % progress_interval == 0 {
                    report_baked_progress(frame as u64, total_frames, ffmpeg_progress, progress_cb);
                }
            }

            check_stdin_cancel(cancel)?;
            if br_input_format == BrInputFormat::Nv12 {
                let finish_start = Instant::now();
                gpu.finish_render_nv12_into(&mut nv12_buf);
                stats.finish_readback += finish_start.elapsed();
                let write_start = Instant::now();
                check_stdin_cancel(cancel)?;
                writer
                    .write_all(nv12_buf.as_slice())
                    .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                stats.write += write_start.elapsed();
            } else {
                let finish_start = Instant::now();
                gpu.finish_render_into(out_w, br_h, &mut rgba_buf);
                stats.finish_readback += finish_start.elapsed();
                let pad_start = Instant::now();
                rgba_pad_to_even(&rgba_buf, &mut rgba_pipe_buf, w, br_h as usize, h);
                stats.convert += pad_start.elapsed();
                let write_start = Instant::now();
                check_stdin_cancel(cancel)?;
                writer
                    .write_all(rgba_pipe_buf.as_slice())
                    .map_err(|e| StdinWriteError::new("ffmpeg stdin", e))?;
                stats.write += write_start.elapsed();
            }
            stats.frames += 1;
            stats.gpu_stats = Some(gpu.stats());
            report_baked_progress(total_frames, total_frames, ffmpeg_progress, progress_cb);
        }
        Err(e) => {
            stats.backend = BrRenderBackend::Cpu;
            log::warn!("GPU unavailable ({}), CPU fallback", e);
            if let Some(status) = render_backend_status {
                status.store(super::EXPORT_RENDER_BACKEND_CPU, Ordering::Relaxed);
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
                check_stdin_cancel(cancel)?;
                let render_start = Instant::now();
                let rendered: Result<Vec<Vec<u8>>, StdinWriteError> = std::thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter()
                        .zip(renderers.iter_mut())
                        .map(|(&frame, renderer)| {
                            let vf = (timeline_start_source_frame + frame as f64 * frame_ratio)
                                .round() as i64;
                            scope.spawn(move || {
                                renderer.render_br(
                                    project,
                                    vf,
                                    out_w,
                                    source_fps,
                                    br_scale,
                                    karaoke_text_scale,
                                )
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
                    check_stdin_cancel(cancel)?;
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
                    check_stdin_cancel(cancel)?;
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
