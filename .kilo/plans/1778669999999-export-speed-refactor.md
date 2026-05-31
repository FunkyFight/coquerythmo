# Refactor export for speed

## Goal
Make MP4 export dramatically faster by removing the current two-pass pipeline, lowering default export FPS to source FPS, and adding a non-baked ultra-fast sidecar mode.

## Constraints
- Keep baked MP4 export visually equivalent: original video stacked with the rendered bande rythmo strip.
- Preserve export with original source video, never proxy.
- Keep progress reporting responsive until completion.
- Avoid temp `br_temp.mp4` and avoid re-decoding a temporary BR video.
- Prefer NVENC when available, but keep CPU fallback reliable.

## Changes

### 1. Add export mode
Files:
- `src/video_export.rs`
- `src/ui/widget.rs`
- `src/ui/export_modal.rs`
- `src/main.rs`

Add:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    Baked,
    Sidecar,
}
```

Extend `UiAction::StartExport` and `ExportModalResult::Export` with `mode: ExportMode`.

### 2. Single-pass baked export
File: `src/video_export.rs`

Replace the current pipeline:
- Pass 1 renders BR to `temp/br_temp.mp4`.
- Pass 2 reads source + temp BR and re-encodes.

With one FFmpeg process:
```text
ffmpeg \
  -thread_queue_size 1024 -i source_video \
  -thread_queue_size 1024 -f rawvideo -pix_fmt yuv420p -s WIDTHxBR_H -r FPS -i pipe:0 \
  -filter_complex "[0:v]scale=...pad=...format=yuv420p[v];[1:v]format=yuv420p[br];[v][br]vstack=inputs=2[out]" \
  -map [out] -map 0:a? \
  -c:v h264_nvenc/libx264 ... \
  -c:a copy -shortest -progress pipe:1 -nostats -y output.mp4
```

Implementation details:
- Spawn FFmpeg with `stdin`, `stdout`, and `stderr` piped.
- Render BR frames as YUV420p directly into FFmpeg stdin.
- Keep the existing GPU BR renderer path.
- Keep CPU parallel fallback.
- Remove temp dir/temp file usage.
- Use writer progress as the main progress signal because pipe back-pressure tracks FFmpeg consumption.
- Drain FFmpeg `stdout` progress in a small reader thread and merge it opportunistically.
- Drain `stderr` in a thread and include it in failures.

Progress mapping:
- During raw BR frame writing: `0.01 -> 0.985`.
- After stdin flush/close: `0.99`.
- After FFmpeg success: `1.0`.
- On CUDA attempt failure, reset to `0.01` and retry CPU decode.

### 3. Hardware acceleration policy
File: `src/video_export.rs`

Use:
- NVENC for final encode when available: `h264_nvenc -preset p1 -rc constqp -qp 20 -b:v 0`.
- CPU encode fallback: `libx264 -preset ultrafast -crf 20`.
- CUDA decode can be attempted only when NVENC and CUDA hwaccel are available.
- If CUDA decode/filter graph fails, rerun single-pass with CPU decode.

Note: this FFmpeg build has `scale_cuda` and `overlay_cuda`, but no `vstack_cuda`/`pad_cuda`, so a fully GPU-resident stack is not safely available. The practical GPU win is NVENC plus optional CUDA decode.

### 4. Ultra-fast sidecar export
File: `src/video_export.rs`

For `ExportMode::Sidecar`:
- Do not render BR frames.
- Do not re-encode video.
- Run FFmpeg remux/copy:
```text
ffmpeg -i source_video -map 0 -c copy -movflags +faststart -progress pipe:1 -nostats -y output.mp4
```
- Write a sidecar JSON next to the MP4:
```text
output_name.br.json
```
- Use `JsonExporter.export(project, source_fps, sidecar_path)`.
- If `output` canonical path equals `source_video`, fail to avoid overwriting the source.

Progress mapping:
- Copy/remux: `0.01 -> 0.95` via FFmpeg `out_time`.
- JSON sidecar write: `0.98`.
- Done: `1.0`.

### 5. Default export FPS = source FPS
Files:
- `src/state.rs`
- `src/ui/mod.rs`
- `src/ui/export_modal.rs`

Change:
```rust
ExportModal::new(video_width, video_height)
```
to:
```rust
ExportModal::new(video_width, video_height, source_fps)
```

In `State::open_export_modal`, pass `self.fps()`.

In `ExportModal::new`, set:
```rust
let fps = source_fps.round().clamp(1.0, 480.0) as u32;
```

Change FPS +/- buttons from `30` step to `1` step and allow minimum `1`, so 24/25 fps are possible.

### 6. Export modal mode toggle
File: `src/ui/export_modal.rs`

Add `mode: ExportMode` to `ExportModal`.

Add a mode row:
- `Incrusté MP4` = baked single-pass.
- `Ultra rapide + BR JSON` = sidecar mode.

When sidecar mode is selected:
- Keep FPS visible because it is used for BR JSON frame timing.
- Resolution and BR zoom can stay visible, but they are ignored by sidecar export. Optional hint text can say so.

Translations:
- `export_modal.mode`
- `export_modal.mode_baked`
- `export_modal.mode_sidecar`
- `export_modal.mode_sidecar_hint`

### 7. Main export dispatch
File: `src/main.rs`

Pass `mode` into `video_export::export_mp4`.

Keep the progress atomic behavior unchanged:
- `0.0`: no overlay yet.
- `0.01..1.0`: overlay active.
- `2.0`: sentinel done.

### 8. Validation
Run:
```text
cargo fmt
cargo check
```

Manual checks:
- Baked export no longer creates `temp/br_temp.mp4`.
- Progress continuously advances throughout export.
- Baked output still includes original audio via `-c:a copy`.
- Sidecar mode produces `output.mp4` fast and `output.br.json` next to it.
- Default FPS in modal matches source FPS instead of 60/240.
