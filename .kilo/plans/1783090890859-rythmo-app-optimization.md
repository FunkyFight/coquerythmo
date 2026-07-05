# Rythmo App Smoothness And Optimization Plan

## Goal
Fix jerky in-app bande rythmo playback while keeping MP4 export behavior unchanged. Apply the fix to all interactive app render paths that draw a bande rythmo: editor view and studio mode. Keep changes portable across Windows, macOS, and Linux.

## Current Findings
- `State::render()` passes `self.current_frame()` as an `i64` to the main UI render path, so editor bande rythmo positions update in whole video-frame steps.
- `VideoPlayer::current_frame_interpolated()` still returns an `i64`, so it is not truly sub-frame interpolation.
- Studio mode already uses `current_frame_interpolated()`, but it is still integer based and `render_studio_rythmo()` also takes `i64`.
- The app render path scans `project.lines()` and `project.markers` in `src/ui/rythmo.rs` every frame. `ProjectRenderIndex` already exists and is used by the GPU export renderer, but not by the interactive app renderers.
- Export renderers already accept fractional frame values in important paths and should remain untouched except for shared helper compatibility if unavoidable.

## Decisions
- Use a visual playback frame as `f64` for app rendering and app bande hit-testing.
- Keep project data, timeline events, seek targets, marker frames, line start/duration frames, and export frame iteration as integer frames.
- Quantize from visual `f64` to `i64` only at action boundaries that mutate state or seek.
- Reuse `ProjectRenderIndex` for visible line and marker filtering in editor and studio render paths.
- Do not introduce OS-specific APIs. Use only portable `std::time::Instant`, existing audio clock logic, `winit`, `wgpu`, and Rust data structures.
- Do not change `rythmo_gpu_renderer::export_backends()` or MP4 export semantics.

## Implementation Steps
1. Add a true fractional render frame API to `src/video.rs`.
   - Add a helper that computes playback elapsed seconds from the same source as `VideoPlayer::tick()`: audio clock when present, wall clock otherwise.
   - Add `VideoPlayer::current_frame_for_render(&self) -> f64`.
   - Return `current_frame as f64` when not playing or when no playback start exists.
   - While playing, return `playback_start_frame as f64 + elapsed * fps`, clamped to the valid video frame range when `total_frames > 0`.
   - Keep `current_frame()` and existing integer behavior for decoded frame tracking and timeline events.

2. Thread the visual frame through app render paths.
   - In `State::render()` and `State::render_studio()`, compute both `current_frame_i64` and `render_frame_f64`.
   - Keep progress bars and stored UI frame values on `current_frame_i64` unless they are purely visual bande positions.
   - Update `Ui::render()` and `Ui::render_studio()` signatures to accept the visual frame in addition to the integer frame where needed.
   - Pass the visual frame to bande rendering functions.

3. Thread the visual frame through app input hit-testing.
   - Update `State::handle_ui_event()` and `Ui::handle_event()` to pass the same visual frame used for rendering into bande-specific hit tests.
   - Update `RythmoCtx` in `src/ui/rythmo.rs` to store a visual `current_frame: f64` for geometry.
   - Convert x-to-frame calculations to round or floor only when returning `UiAction` values that require integer frames, such as create line, move marker, seek, and drag operations.
   - Ensure displayed line rectangles, badges, marker hit boxes, autocomplete anchors, and context menu anchors use the same visual frame as render.

4. Convert app-only bande geometry helpers to support fractional frames.
   - Add or change helpers in `src/ui/rythmo.rs`, for example `frame_to_x(frame: i64, current_frame: f64, zone: &Rect) -> f32` and `x_to_frame(x: f32, current_frame: f64, zone: &Rect) -> i64`.
   - Update editor render helpers: `line_visual_x_width_with_karaoke_width`, `line_rect*`, `badge_rect*`, `render_rythmo_base`, `render_lines`, `render_markers`, `render_autocomplete`, and related render-only calls.
   - Update studio helpers: `render_studio_rythmo`, tick positions, line positions, marker positions, karaoke dot/count-in progress, and texture prewarm inputs.
   - Keep tests that require integer frames by passing whole-number `f64` values or by keeping small integer wrapper helpers for tests where clearer.

5. Update karaoke visual logic for fractional frames.
   - Change app-side `karaoke_count_in_progress`, `karaoke_count_in_visible`, `KaraokeUiIndex::prestart_scroll_visible`, and `KaraokeUiIndex::upcoming_stack_visible` to accept visual frames where they drive rendering.
   - Keep `max_gap_frames` and count-in durations as integer frame counts.
   - Use fractional frames for `line.karaoke_active`, `line.karaoke_progress`, dot bounce, color fill clipping, and count-in dot movement.

6. Use `ProjectRenderIndex` in interactive app rendering.
   - Pass `&ProjectRenderIndex` from `State` into `Ui::render()` and `Ui::render_studio()` after `refresh(&project)`.
   - In editor `render_lines`, compute a visible frame window around the visual frame.
   - Include a render margin at least `max(karaoke_adjacent_max_gap_frames(fps), karaoke_count_in_frames(fps), fps * 10s)` so karaoke prestart/upcoming lines are not accidentally culled.
   - Iterate `render_index.visible_line_ids(project, first_visible_frame, last_visible_frame)` instead of all `project.lines()` for normal line rendering.
   - For active karaoke playhead skip ranges, use `visible_line_ids(project, current_i64, current_i64)` or an equivalent narrow indexed query rather than scanning all lines.
   - In editor `render_markers` and studio marker rendering, use `visible_marker_indices(first_marker_frame, last_marker_frame)`.
   - Preserve full scans only where the operation is inherently project-wide and not per-frame hot, such as index rebuilds on project revision changes.

7. Reduce avoidable per-frame work in the touched paths.
   - Hoist repeated `crate::config::get().lang.clone()` calls out of per-line inner loops where practical.
   - Avoid recomputing track layouts more than once per render path when the same `KaraokeUiIndex` already has track flags.
   - Keep existing text texture cache and prewarm behavior, but ensure fractional x positions do not enter texture cache keys.
   - Preallocate local vectors where visible counts are known or easy to estimate, without adding broad new abstractions.

8. Preserve export behavior.
   - Do not change CPU/GPU export frame stepping, export FPS, or export renderer backend selection.
   - If helper signatures shared with export require changes, update export call sites so exported output remains frame-deterministic and uses the same numeric values as before.

## Validation Plan
- Run `cargo fmt`.
- Run `cargo test`.
- Add or adjust unit tests around fractional `frame_to_x`/`x_to_frame` behavior and visible-index bounds if straightforward.
- Manually validate in the app:
  - Editor playback at 24/25/30 fps shows smooth bande motion.
  - Studio mode bande motion remains smooth.
  - Textures are not recreated every frame during smooth motion.
  - Selecting, dragging, resizing, marker hit-testing, context menu placement, autocomplete placement, and progress-bar scrubbing still align with the rendered bande.
  - Karaoke active text, color fill, bouncing dot, count-in dot, and adjacent karaoke preview still behave correctly.
  - Export MP4 output path still runs and is visually unchanged.
- Cross-platform compatibility:
  - Ensure the implementation uses no platform-specific APIs.
  - Run current-platform tests locally.
  - If CI or target machines are available, run `cargo check` or `cargo test` on macOS and Linux as well.
  - If macOS/Linux cannot be run locally, document that the code path is portable but only the available platform was executed.

## Risks And Mitigations
- Risk: render and hit-test frames diverge, causing click/drag offsets while playing.
  Mitigation: pass the same visual `f64` frame through both render and bande event geometry.
- Risk: using a render frame ahead of decoded video exposes decoder stalls as audio/video mismatch.
  Mitigation: derive render frame from the same audio clock source already used by `tick()`, and keep decoded `current_frame` as the authoritative frame event source.
- Risk: visible-index culling hides karaoke preview lines with long adjacent gaps.
  Mitigation: use a margin at least as large as the karaoke adjacent max gap.
- Risk: changing helper signatures touches many tests.
  Mitigation: keep small wrappers where useful and update tests with whole-number `f64` inputs.
- Risk: over-optimizing by changing present mode or refresh cadence could behave differently across platforms.
  Mitigation: do not change present mode or platform presentation behavior in the first implementation unless profiling after the main fix shows it is still necessary.

## Out Of Scope
- MP4 export visual changes.
- New renderer architecture.
- OS-specific timing or presentation code.
- Large UI redesign.
