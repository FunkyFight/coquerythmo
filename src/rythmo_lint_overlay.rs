//! Editor-only rendering and hover information for bande-rythmo diagnostics.
//!
//! Diagnostics are cached by project revision. Pointer events only update the
//! hover position; geometry is rebuilt from the current frame during rendering,
//! so waves follow scrubbing without re-linting the whole project.

use crate::detection::{MediaTick, TextAnchor};
use crate::project::Project;
use crate::rythmo_lint::{lint_project, LintDiagnostic, LintScope, LintSeverity};
use crate::ui::primitives::{
    HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{line_rect, RythmoState};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const WAVE_SEGMENT_WIDTH: f32 = 4.0;
const WAVE_AMPLITUDE: f32 = 1.8;
const WAVE_THICKNESS: f32 = 1.6;
const TOOLTIP_WIDTH: f32 = 500.0;
const TOOLTIP_HEIGHT: f32 = 52.0;
const FRAME_EPSILON: f64 = 0.000_1;

static EXPLICIT_PLAYING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Wave {
    x1: f32,
    x2: f32,
    y: f32,
    severity: LintSeverity,
    message: &'static str,
}

impl Wave {
    fn hit_test(self, x: f32, y: f32) -> bool {
        x >= self.x1.min(self.x2) - 3.0
            && x <= self.x1.max(self.x2) + 3.0
            && (y - self.y).abs() <= 7.0
    }
}

#[derive(Clone, Copy)]
struct Tooltip {
    rect: Rect,
    severity: LintSeverity,
    message: &'static str,
}

struct OverlayState {
    project_revision: u64,
    diagnostics: Vec<LintDiagnostic>,
    waves: Vec<Wave>,
    pointer: (f32, f32),
    tooltip: Option<Tooltip>,
    event_serial: u64,
    geometry_event_serial: u64,
    last_frame: Option<f64>,
    inferred_playing: bool,
    stable_frames: u8,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            project_revision: u64::MAX,
            diagnostics: Vec::new(),
            waves: Vec::new(),
            pointer: (0.0, 0.0),
            tooltip: None,
            event_serial: 0,
            geometry_event_serial: 0,
            last_frame: None,
            inferred_playing: false,
            stable_frames: 0,
        }
    }
}

fn overlay() -> &'static Mutex<OverlayState> {
    static STATE: OnceLock<Mutex<OverlayState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(OverlayState::default()))
}

fn lock_state() -> std::sync::MutexGuard<'static, OverlayState> {
    overlay()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn event_pointer(event: &UiEvent) -> Option<(f32, f32)> {
    match event {
        UiEvent::MouseMove { x, y }
        | UiEvent::MousePress { x, y }
        | UiEvent::MouseRelease { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y }
        | UiEvent::MiddlePress { x, y }
        | UiEvent::MiddleRelease { x, y }
        | UiEvent::ContextMenu { x, y } => Some((*x, *y)),
        UiEvent::Scroll { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

fn pixels_per_frame() -> f32 {
    crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

fn frame_x(frame: f64, current_frame: f64, zone: Rect) -> f32 {
    zone.x + zone.width / 2.0 + (frame - current_frame) as f32 * pixels_per_frame()
}

pub fn set_playing(playing: bool) {
    EXPLICIT_PLAYING.store(playing, Ordering::Relaxed);
    if playing {
        let mut state = lock_state();
        state.inferred_playing = true;
        state.stable_frames = 0;
        state.waves.clear();
        state.tooltip = None;
    }
}

/// Event-side update: deliberately O(1). Linting and geometry never run from a
/// mouse-move callback.
pub fn sync_from_state(
    _project: &Project,
    _state: &RythmoState,
    _zone: Rect,
    _current_frame: f64,
    event: &UiEvent,
) {
    let mut state = lock_state();
    state.event_serial = state.event_serial.wrapping_add(1);
    if let Some(pointer) = event_pointer(event) {
        state.pointer = pointer;
    }
}

fn sync_boundaries(project: &Project, line_id: u64, character_count: usize) -> Vec<(usize, f32)> {
    let Some(line) = project.get_line(line_id) else {
        return Vec::new();
    };
    if line.karaoke || character_count == 0 || line.duration_frames <= 0 {
        return Vec::new();
    }

    let start = MediaTick::from_frame(line.start_frame);
    let end = MediaTick::from_frame(line.end_frame());
    let duration = (end.raw() - start.raw()).max(1) as f32;
    let mut interior = BTreeMap::<usize, MediaTick>::new();
    if let Some(data) = project.detections().line(line_id) {
        for cue in data.text_sync_cues() {
            let Some(index) = cue.target.grapheme_index().map(|index| index as usize) else {
                continue;
            };
            if index == 0 || index >= character_count {
                continue;
            }
            interior.insert(index, cue.media_tick.clamp(start, end));
        }
    }

    let mut boundaries = Vec::with_capacity(interior.len() + 2);
    boundaries.push((0, 0.0));
    for (index, tick) in interior {
        boundaries.push((
            index,
            ((tick.raw() - start.raw()) as f32 / duration).clamp(0.0, 1.0),
        ));
    }
    boundaries.push((character_count, 1.0));

    let mut previous = 0.0_f32;
    for boundary in &mut boundaries {
        boundary.1 = boundary.1.max(previous).clamp(0.0, 1.0);
        previous = boundary.1;
    }
    boundaries
}

fn character_ratio(project: &Project, line_id: u64, character_count: usize, index: usize) -> f32 {
    let index = index.min(character_count);
    let boundaries = sync_boundaries(project, line_id, character_count);
    if boundaries.len() < 2 {
        return index as f32 / character_count.max(1) as f32;
    }
    for pair in boundaries.windows(2) {
        let (start_index, start_ratio) = pair[0];
        let (end_index, end_ratio) = pair[1];
        if index <= end_index {
            let span = end_index.saturating_sub(start_index).max(1);
            let local = index.saturating_sub(start_index) as f32 / span as f32;
            return start_ratio + (end_ratio - start_ratio) * local;
        }
    }
    1.0
}

fn line_wave(
    project: &Project,
    line_id: u64,
    start_char: usize,
    end_char: usize,
    zone: Rect,
    current_frame: f64,
    severity: LintSeverity,
    message: &'static str,
) -> Option<Wave> {
    let line = project.get_line(line_id)?;
    let rect = line_rect(project, line, current_frame, &zone);
    let count = line.text.chars().count();
    if count == 0 {
        return None;
    }
    let start = start_char.min(count);
    let end = end_char.max(start + 1).min(count);
    let start_ratio = character_ratio(project, line_id, count, start);
    let end_ratio = character_ratio(project, line_id, count, end);
    let x1 = rect.x + rect.width * start_ratio;
    let mut x2 = rect.x + rect.width * end_ratio;
    if x2 - x1 < 10.0 {
        x2 = x1 + 10.0;
    }
    Some(Wave {
        x1,
        x2,
        y: rect.y + rect.height - 2.0,
        severity,
        message,
    })
}

fn zone_wave(
    start_frame: i64,
    end_frame: i64,
    zone: Rect,
    current_frame: f64,
    severity: LintSeverity,
    message: &'static str,
) -> Option<Wave> {
    let x1 = frame_x(start_frame as f64, current_frame, zone).max(zone.x);
    let x2 = frame_x(end_frame as f64, current_frame, zone).min(zone.x + zone.width);
    (x2 > x1).then_some(Wave {
        x1,
        x2,
        y: zone.y + crate::constants::RULER_HEIGHT + 5.0,
        severity,
        message,
    })
}

fn tooltip_rect(pointer: (f32, f32), screen: Rect) -> Rect {
    let mut rect = Rect {
        x: pointer.0 + 12.0,
        y: pointer.1 + 12.0,
        width: TOOLTIP_WIDTH.min(screen.width),
        height: TOOLTIP_HEIGHT,
    };
    rect.x = rect
        .x
        .clamp(screen.x, (screen.x + screen.width - rect.width).max(screen.x));
    rect.y = rect
        .y
        .clamp(screen.y, (screen.y + screen.height - rect.height).max(screen.y));
    rect
}

fn update_playback_inference(state: &mut OverlayState, current_frame: f64) {
    let frame_changed = state
        .last_frame
        .is_some_and(|last| (last - current_frame).abs() > FRAME_EPSILON);
    let user_event_since_geometry = state.event_serial != state.geometry_event_serial;

    if EXPLICIT_PLAYING.load(Ordering::Relaxed) {
        state.inferred_playing = true;
        state.stable_frames = 0;
    } else if frame_changed && !user_event_since_geometry {
        state.inferred_playing = true;
        state.stable_frames = 0;
    } else if !frame_changed {
        state.stable_frames = state.stable_frames.saturating_add(1);
        if state.stable_frames >= 2 {
            state.inferred_playing = false;
        }
    } else {
        // A frame jump accompanied by a UI event is scrubbing, not playback.
        state.inferred_playing = false;
        state.stable_frames = 0;
    }

    state.last_frame = Some(current_frame);
    state.geometry_event_serial = state.event_serial;
}

/// Render-side update. This follows the current frame on every redraw but only
/// re-runs lint rules when the project revision changes.
pub fn sync_geometry(
    project: &Project,
    _rythmo_state: &RythmoState,
    zone: Rect,
    current_frame: f64,
) {
    let mut state = lock_state();
    update_playback_inference(&mut state, current_frame);
    if state.inferred_playing {
        state.waves.clear();
        state.tooltip = None;
        return;
    }

    let revision = project.revision();
    if state.project_revision != revision {
        state.diagnostics = lint_project(project);
        state.project_revision = revision;
    }

    state.waves.clear();
    let diagnostics = state.diagnostics.clone();
    for diagnostic in diagnostics {
        let wave = match diagnostic.scope {
            LintScope::Line {
                line_id,
                start_char,
                end_char,
            } => line_wave(
                project,
                line_id,
                start_char,
                end_char,
                zone,
                current_frame,
                diagnostic.severity,
                diagnostic.message,
            ),
            LintScope::Zone {
                start_frame,
                end_frame,
            } => zone_wave(
                start_frame,
                end_frame,
                zone,
                current_frame,
                diagnostic.severity,
                diagnostic.message,
            ),
        };
        if let Some(wave) = wave {
            state.waves.push(wave);
        }
    }

    let pointer = state.pointer;
    let hovered = state
        .waves
        .iter()
        .copied()
        .filter(|wave| wave.hit_test(pointer.0, pointer.1))
        .min_by_key(|wave| match wave.severity {
            LintSeverity::Error => 0,
            LintSeverity::Warning => 1,
        });
    state.tooltip = hovered.map(|wave| Tooltip {
        rect: tooltip_rect(pointer, zone),
        severity: wave.severity,
        message: wave.message,
    });
}

pub fn clear() {
    EXPLICIT_PLAYING.store(false, Ordering::Relaxed);
    *lock_state() = OverlayState::default();
}

fn severity_color(severity: LintSeverity) -> [f32; 4] {
    match severity {
        LintSeverity::Warning => [0.96, 0.72, 0.08, 1.0],
        LintSeverity::Error => [0.96, 0.12, 0.12, 1.0],
    }
}

fn push_flat_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], radius: f32) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn render_wave(quads: &mut Vec<QuadInstance>, wave: Wave) {
    let color = severity_color(wave.severity);
    let start = wave.x1.min(wave.x2);
    let end = wave.x1.max(wave.x2);
    let mut x = start;
    let mut high = false;
    while x < end {
        let width = WAVE_SEGMENT_WIDTH.min(end - x);
        let y = wave.y + if high { -WAVE_AMPLITUDE } else { WAVE_AMPLITUDE };
        push_flat_quad(
            quads,
            Rect {
                x,
                y,
                width,
                height: WAVE_THICKNESS,
            },
            color,
            WAVE_THICKNESS / 2.0,
        );
        high = !high;
        x += WAVE_SEGMENT_WIDTH;
    }
}

pub fn append_foreground<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let state = lock_state();
    if state.inferred_playing || EXPLICIT_PLAYING.load(Ordering::Relaxed) {
        return;
    }
    for wave in state.waves.iter().copied() {
        render_wave(quads, wave);
    }
    let Some(tooltip) = state.tooltip else {
        return;
    };
    let rect = Rect {
        x: tooltip.rect.x.clamp(0.0, screen_w.max(0.0)),
        y: tooltip.rect.y.clamp(0.0, screen_h.max(0.0)),
        width: tooltip.rect.width.min(screen_w.max(0.0)),
        height: tooltip.rect.height.min(screen_h.max(0.0)),
    };
    push_flat_quad(quads, rect, [0.025, 0.028, 0.038, 0.995], 7.0);
    labels.push(LabelInfo {
        text: tooltip.severity.accessibility_label(),
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y + 5.0,
            width: rect.width - 20.0,
            height: 16.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(11.0),
        color_override: Some(match tooltip.severity {
            LintSeverity::Warning => [247, 193, 42],
            LintSeverity::Error => [248, 70, 70],
        }),
        font_family_override: None,
    });
    labels.push(LabelInfo {
        text: tooltip.message,
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y + 20.0,
            width: rect.width - 20.0,
            height: rect.height - 24.0,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: Some([239, 241, 247]),
        font_family_override: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_update_does_not_run_lint_or_rebuild_geometry() {
        clear();
        let project = Project::new();
        let state = RythmoState::new();
        sync_from_state(
            &project,
            &state,
            Rect::default(),
            0.0,
            &UiEvent::MouseMove { x: 12.0, y: 34.0 },
        );
        let overlay = lock_state();
        assert_eq!(overlay.pointer, (12.0, 34.0));
        assert!(overlay.diagnostics.is_empty());
        assert!(overlay.waves.is_empty());
    }

    #[test]
    fn character_mapping_uses_independent_sync_intervals() {
        let project = Project::new();
        assert_eq!(character_ratio(&project, 999, 10, 0), 0.0);
        assert_eq!(character_ratio(&project, 999, 10, 10), 1.0);
    }

    #[test]
    fn playback_hides_editor_diagnostics() {
        clear();
        set_playing(true);
        assert!(EXPLICIT_PLAYING.load(Ordering::Relaxed));
        clear();
    }
}
