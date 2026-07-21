//! Editor-only rendering and hover information for bande-rythmo diagnostics.
//!
//! The overlay is synchronized by the rythmo controller and composed by the
//! modal host. Export, studio and CPU video renderers never call this module.

use crate::detection::TextAnchor;
use crate::project::Project;
use crate::rythmo_lint::{lint_project, LintScope, LintSeverity};
use crate::ui::primitives::{
    HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{line_rect, RythmoState};
use std::sync::{Mutex, OnceLock};

const WAVE_SEGMENT_WIDTH: f32 = 4.0;
const WAVE_AMPLITUDE: f32 = 1.8;
const WAVE_THICKNESS: f32 = 1.6;
const TOOLTIP_WIDTH: f32 = 500.0;
const TOOLTIP_HEIGHT: f32 = 52.0;

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
struct Tooltip {
    rect: Rect,
    severity: LintSeverity,
    message: &'static str,
}

#[derive(Debug)]
struct OverlayState {
    waves: Vec<Wave>,
    pointer: (f32, f32),
    tooltip: Option<Tooltip>,
    screen: Rect,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            waves: Vec::new(),
            pointer: (0.0, 0.0),
            tooltip: None,
            screen: Rect::default(),
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

fn sync_anchors(project: &Project, line_id: u64, character_count: usize) -> Vec<(usize, f32)> {
    let Some(line) = project.get_line(line_id) else {
        return Vec::new();
    };
    if line.duration_frames <= 0 || character_count == 0 {
        return Vec::new();
    }
    let Some(data) = project.detections().line(line_id) else {
        return Vec::new();
    };
    let mut anchors = data
        .text_sync_cues()
        .filter_map(|cue| {
            let TextAnchor::GraphemeIndex(index) = cue.target else {
                return None;
            };
            let index = index as usize;
            if index >= character_count {
                return None;
            }
            let target = ((cue.media_tick.as_frame_position() - line.start_frame as f64)
                / line.duration_frames as f64) as f32;
            Some((index, target.clamp(0.0, 1.0)))
        })
        .collect::<Vec<_>>();
    anchors.sort_by_key(|(index, _)| *index);
    anchors
}

/// Mirrors the synchronized text layout's fixed endpoints so lint spans stay
/// attached to the same character intervals as the caret and rendered text.
fn character_positions(project: &Project, line_id: u64, count: usize) -> Vec<f32> {
    let base = (0..=count)
        .map(|index| index as f32 / count.max(1) as f32)
        .collect::<Vec<_>>();
    let anchors = sync_anchors(project, line_id, count);
    if anchors.is_empty() || count == 0 {
        return base;
    }

    let mut controls = vec![(0.0_f32, 0.0_f32), (1.0_f32, 1.0_f32)];
    controls.extend(anchors.into_iter().filter_map(|(index, target)| {
        let left = *base.get(index)?;
        let right = *base.get(index + 1)?;
        Some(((left + right) * 0.5, target))
    }));
    controls.sort_by(|left, right| left.0.total_cmp(&right.0));
    controls.dedup_by(|current, previous| {
        if (current.0 - previous.0).abs() <= 0.000_001 {
            previous.1 = previous.1.max(current.1);
            true
        } else {
            false
        }
    });
    let mut previous_target = 0.0_f32;
    for control in &mut controls {
        control.1 = control.1.max(previous_target).clamp(0.0, 1.0);
        previous_target = control.1;
    }
    controls[0] = (0.0, 0.0);
    let last = controls.len() - 1;
    controls[last] = (1.0, 1.0);

    let mut mapped = Vec::with_capacity(base.len());
    for source in base {
        let mut target = source;
        for pair in controls.windows(2) {
            if source <= pair[1].0 || pair[1].0 >= 1.0 {
                let local = ((source - pair[0].0) / (pair[1].0 - pair[0].0).max(0.000_001))
                    .clamp(0.0, 1.0);
                target = pair[0].1 + (pair[1].1 - pair[0].1) * local;
                break;
            }
        }
        mapped.push(target.clamp(0.0, 1.0));
    }
    for index in 1..mapped.len() {
        mapped[index] = mapped[index].max(mapped[index - 1]);
    }
    mapped[0] = 0.0;
    let last = mapped.len() - 1;
    mapped[last] = 1.0;
    mapped
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
    let positions = character_positions(project, line_id, count);
    let start = start_char.min(count);
    let end = end_char.max(start + 1).min(count.max(1));
    let start_ratio = positions.get(start).copied().unwrap_or(0.0);
    let end_ratio = positions
        .get(end)
        .copied()
        .unwrap_or_else(|| (start_ratio + 0.08).min(1.0));
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

pub fn sync_from_state(
    project: &Project,
    _state: &RythmoState,
    zone: Rect,
    current_frame: f64,
    event: &UiEvent,
) {
    let mut state = lock_state();
    state.screen = zone;
    if let Some(pointer) = event_pointer(event) {
        state.pointer = pointer;
    }
    state.waves.clear();

    for diagnostic in lint_project(project) {
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
    let severity = tooltip.severity.accessibility_label();
    labels.push(LabelInfo {
        text: severity,
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
    fn wave_hit_area_is_larger_than_the_visual_stroke() {
        let wave = Wave {
            x1: 10.0,
            x2: 110.0,
            y: 20.0,
            severity: LintSeverity::Warning,
            message: "test",
        };
        assert!(wave.hit_test(60.0, 25.0));
        assert!(!wave.hit_test(60.0, 30.0));
    }

    #[test]
    fn synchronized_character_positions_keep_exact_endpoints() {
        let project = Project::new();
        let positions = character_positions(&project, 123, 8);
        assert_eq!(positions.first().copied(), Some(0.0));
        assert_eq!(positions.last().copied(), Some(1.0));
    }
}
