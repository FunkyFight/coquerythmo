# GPU-Accelerated MP4 Export — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the CPU software renderer with the existing GPU renderer in the MP4 export pipeline, adding pipelined submit/finish for maximum throughput.

**Architecture:** Single `GpuRenderer` instance renders BR frames on the GPU. Double-buffered staging allows pipelining: submit frame N+1 while reading back frame N. CPU fallback if no GPU adapter.

**Tech Stack:** wgpu (GPU), tiny-skia (CPU fallback), ffmpeg (encoding), glyphon (text rasterization)

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src/main.rs:9` | Modify | Add `mod rythmo_gpu_renderer;` and `mod syllable;` declarations |
| `src/video_export.rs` | Modify | Replace CPU render loop with GPU pipelined loop + CPU fallback |
| `src/rythmo_gpu_renderer.rs` | No change | Already has `submit_render` + `finish_render` API |
| `src/rythmo_cpu_renderer.rs` | No change | Kept as fallback |

---

### Task 1: Declare missing modules in main.rs

**Files:**
- Modify: `src/main.rs:9`

The `rythmo_gpu_renderer.rs` and `syllable.rs` files exist but are not declared as modules. The GPU renderer uses `crate::syllable::syllable_breaks` internally, so both must be declared.

- [ ] **Step 1: Add module declarations**

In `src/main.rs`, after the existing `mod rythmo_cpu_renderer;` line (line 9), add the two missing modules:

```rust
mod rythmo_cpu_renderer;
mod rythmo_gpu_renderer;
mod syllable;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no new errors (warnings OK).

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: declare rythmo_gpu_renderer and syllable modules"
```

---

### Task 2: Wire GPU renderer into export_mp4 with pipelining

**Files:**
- Modify: `src/video_export.rs` (full rewrite of Pass 1 loop, lines 95-231)

This is the core change. Replace the CPU multi-threaded render loop with a single-threaded GPU pipelined loop.

- [ ] **Step 1: Add GPU renderer import**

At the top of `src/video_export.rs`, replace:
```rust
use crate::rythmo_cpu_renderer;
```
with:
```rust
use crate::rythmo_cpu_renderer;
use crate::rythmo_gpu_renderer;
```

- [ ] **Step 2: Add br_height helper for GPU path**

The CPU renderer has `rythmo_cpu_renderer::br_height()`. The GPU renderer calculates height internally but doesn't expose a standalone function. Since both use the same formula, keep using the CPU one (it's a pure calculation, no rendering). No change needed — `rythmo_cpu_renderer::br_height` is already correct.

- [ ] **Step 3: Replace Pass 1 render loop**

Replace the entire Pass 1 block (from `{` on line 146 through `}` closing on line 231) with the GPU-pipelined version:

```rust
    {
        let br_stdin = br_encoder.stdin.take().unwrap();
        let mut writer = std::io::BufWriter::with_capacity(yuv_frame_size * 8, br_stdin);

        let mut yuv_buf = vec![0u8; yuv_frame_size];
        let hw = w / 2;
        let u_off = w * h;
        let v_off = u_off + hw * (h / 2);

        // Try GPU renderer, fall back to CPU if unavailable
        match rythmo_gpu_renderer::GpuRenderer::new() {
            Ok(mut gpu) => {
                log::info!("Pass 1: using GPU-accelerated rendering");

                // Pipeline: submit frame 0, then for each subsequent frame,
                // finish previous + YUV + write while submitting next
                gpu.submit_render(project, 0, out_w, fps);

                for frame in 1..total_frames as i64 {
                    // Finish previous frame (GPU → CPU readback)
                    let rgba = gpu.finish_render(out_w, br_h);

                    // Convert RGBA → YUV420p
                    rgba_to_yuv420p(&rgba, &mut yuv_buf, w, h, br_h as usize, hw, u_off, v_off);

                    // Write to ffmpeg
                    if writer.write_all(&yuv_buf).is_err() {
                        break;
                    }

                    // Submit next frame (non-blocking, GPU starts rendering)
                    gpu.submit_render(project, frame, out_w, fps);

                    if frame as u64 % fps as u64 == 0 {
                        progress_cb((frame - 1) as f32 / total_frames as f32 * 0.9);
                    }
                }

                // Finish last frame
                let rgba = gpu.finish_render(out_w, br_h);
                rgba_to_yuv420p(&rgba, &mut yuv_buf, w, h, br_h as usize, hw, u_off, v_off);
                let _ = writer.write_all(&yuv_buf);
            }
            Err(e) => {
                log::warn!("GPU renderer unavailable ({}), falling back to CPU", e);

                let n_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                log::info!("Parallel BR render: {} threads", n_threads);

                let mut renderers: Vec<rythmo_cpu_renderer::CpuRenderer> = (0..n_threads)
                    .map(|_| rythmo_cpu_renderer::CpuRenderer::new())
                    .collect();

                let frame_indices: Vec<i64> = (0..total_frames as i64).collect();

                for batch in frame_indices.chunks(n_threads) {
                    let rendered: Vec<Vec<u8>> = std::thread::scope(|scope| {
                        let handles: Vec<_> = batch.iter()
                            .zip(renderers.iter_mut())
                            .map(|(&frame, renderer)| {
                                scope.spawn(move || {
                                    renderer.render_br(project, frame, out_w, fps)
                                })
                            }).collect();
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
```

- [ ] **Step 4: Extract RGBA-to-YUV420p into a helper function**

Add this function at the bottom of `src/video_export.rs` (before the closing of the file). This extracts the conversion logic currently inline in the loop so both GPU and CPU paths can use it without duplication:

```rust
/// Convert RGBA pixels to YUV420p in-place into `yuv_buf`.
/// `br_h_actual` is the actual BR pixel height (may be less than `h` due to even-alignment padding).
fn rgba_to_yuv420p(
    rgba: &[u8],
    yuv_buf: &mut [u8],
    w: usize,
    h: usize,
    br_h_actual: usize,
    hw: usize,
    u_off: usize,
    v_off: usize,
) {
    // Y plane
    for y in 0..br_h_actual {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let (r, g, b) = (rgba[si] as i32, rgba[si + 1] as i32, rgba[si + 2] as i32);
            yuv_buf[y * w + x] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(16, 235) as u8;
        }
    }
    // Pad Y plane for even-aligned height
    for y in br_h_actual..h {
        for x in 0..w {
            yuv_buf[y * w + x] = 16;
        }
    }
    // U and V planes (chroma subsampled 2x2)
    for cy in 0..h / 2 {
        for cx in 0..hw {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let py = cy * 2 + dy;
                    let px = cx * 2 + dx;
                    let si = (py * w + px) * 4;
                    if py < br_h_actual {
                        r_sum += rgba[si] as i32;
                        g_sum += rgba[si + 1] as i32;
                        b_sum += rgba[si + 2] as i32;
                    }
                }
            }
            let r = r_sum >> 2;
            let g = g_sum >> 2;
            let b = b_sum >> 2;
            yuv_buf[u_off + cy * hw + cx] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
            yuv_buf[v_off + cy * hw + cx] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compiles. The `GpuRenderer::finish_render` takes `(width: u32, height: u32)` — confirm the call uses `(out_w, br_h)` matching the `submit_render` dimensions.

- [ ] **Step 6: Commit**

```bash
git add src/video_export.rs
git commit -m "feat: use GPU renderer for MP4 export with pipelined readback"
```

---

### Task 3: Smoke test the export

This task is manual verification since there's no automated test harness for video export.

- [ ] **Step 1: Build the project**

Run: `cargo build`
Expected: Clean build (warnings OK).

- [ ] **Step 2: Manual test**

1. Open coquerythmo with a video loaded and some rythmo lines
2. Export as MP4
3. Verify: export completes, output video has BR strip, no visual corruption
4. Note: check the log output — it should say "Pass 1: using GPU-accelerated rendering"

- [ ] **Step 3: Verify GPU fallback path (optional)**

If you can test on a machine without GPU, verify the log says "GPU renderer unavailable... falling back to CPU" and export still works.

- [ ] **Step 4: Commit any fixes**

If any adjustments were needed, commit them:
```bash
git add -u
git commit -m "fix: adjustments from GPU export smoke test"
```
