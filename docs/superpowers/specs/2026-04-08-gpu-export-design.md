# GPU-Accelerated MP4 Export

## Problem

The MP4 export pipeline uses `CpuRenderer` (tiny-skia software rasterization) to render the bande rythmo strip frame-by-frame. For a 5-minute video at 240fps, this means 72,000 frames rendered in software with per-frame text rasterization. Export takes 10-30 minutes.

A `GpuRenderer` already exists with instance rendering, text caching, and double-buffered staging, but it is not wired into the export pipeline.

## Solution

Replace `CpuRenderer` with `GpuRenderer` in `export_mp4()`, using pipelined submit/finish to overlap GPU rendering with CPU YUV conversion and ffmpeg writing.

## Architecture

### Current flow (CPU)

```
for each frame:
    render_br(frame) → RGBA pixels   [CPU, ~5-50ms]
    RGBA → YUV420p                    [CPU, ~1-2ms]
    write to ffmpeg pipe              [I/O, ~0.5ms]
```

Threads render batches of N frames in parallel, but each thread does full software rasterization including text layout.

### New flow (GPU pipelined)

```
submit_render(frame 0)
for frame 1..total:
    finish_render(frame-1) → RGBA    [GPU readback, ~0.2ms]
    RGBA → YUV420p                   [CPU, ~1-2ms]
    write to ffmpeg pipe             [I/O, ~0.5ms]
    submit_render(frame)             [GPU, non-blocking]
finish_render(last frame)            [final readback]
```

Single `GpuRenderer` instance (GPU is already parallel internally). The double-buffered staging in `OffscreenTarget` allows submit N+1 while reading back N.

### Key advantages

- **Instance rendering**: All quads in 1 draw call, icons batched by texture hash
- **Text cache**: `HashMap<u64, CachedText>` -- text rasterized once, reused across all frames
- **Pipelining**: GPU renders frame N+1 while CPU processes frame N
- **No thread management**: Single thread, GPU handles parallelism

## Changes

### `video_export.rs`

1. Import `rythmo_gpu_renderer::GpuRenderer` instead of `rythmo_cpu_renderer`
2. Create one `GpuRenderer` at export start
3. Replace the parallel CPU render loop with pipelined GPU loop:
   - `submit_render(frame)` then `finish_render(prev_frame)` + YUV + write
4. Keep RGBA-to-YUV420p conversion unchanged (CPU-side, fast enough)
5. Remove thread pool (`std::thread::scope` batching)
6. Use `rythmo_gpu_renderer` for `br_height()` calculation (or keep the shared `count_used_slots` logic)

### Fallback

If `GpuRenderer::new()` fails (no GPU adapter), fall back to the existing `CpuRenderer` path. Log a warning.

### No changes to

- `rythmo_gpu_renderer.rs` (API already fits: `submit_render` + `finish_render`)
- `rythmo_cpu_renderer.rs` (kept as fallback)
- Pass 2 ffmpeg combine step (unchanged)
- RGBA-to-YUV conversion logic (unchanged)

## Expected performance

| Metric | CPU (current) | GPU (proposed) |
|--------|--------------|----------------|
| Render per frame | 5-50ms | 0.1-0.5ms |
| Text rasterization | Per frame | Once (cached) |
| Parallelism | N CPU threads | GPU + pipeline |
| Export 5min@240fps | 10-30 min | ~1-3 min |

## Risks

- GPU adapter may not be available on all machines (mitigated by CPU fallback)
- `device.poll(Wait)` in `finish_render` is synchronous but masked by pipelining
- Memory: text cache grows unbounded during export (acceptable for export duration)
