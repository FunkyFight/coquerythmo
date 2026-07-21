//! Detection UI facade.
//!
//! The established detector owns signs, guides and synchronization points. This
//! facade removes its legacy popup tail, then lets the single modal foreground
//! own palette/card rendering and interaction.

use super::*;

#[path = "detection_ui_base.rs"]
mod base;

pub use base::{DetectionDrag, DetectionHover, DetectionMenu};
pub(crate) use base::{
    decode_sync_syllable_drag_line_id, encode_sync_syllable_drag_line_id,
    line_has_visible_sync_points, render_sync_text_segments, sync_syllable_boundary_ratios,
};

const SIGN_BADGE_SIZE: f32 = 26.0;
const SIGN_BOTTOM_MARGIN: f32 = 2.0;

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    crate::detection_foreground::reconcile_legacy_menu(state);
    base::handle_detection_event(ctx, event, state)
}

fn selected_detection(state: &RythmoState) -> Option<crate::detection::DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
}

fn sign_badge_rect(
    tick: crate::detection::MediaTick,
    track_rect: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    let x = zone.x
        + zone.width / 2.0
        + (tick.as_frame_position() - current_frame) as f32 * ppf();
    Rect {
        x: x - SIGN_BADGE_SIZE / 2.0,
        y: (track_rect.y + track_rect.height - SIGN_BADGE_SIZE - SIGN_BOTTOM_MARGIN)
            .max(track_rect.y),
        width: SIGN_BADGE_SIZE,
        height: SIGN_BADGE_SIZE,
    }
}

fn rect_center(rect: [f32; 4]) -> (f32, f32) {
    (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0)
}

fn pop_icon_tail_inside(icons: &mut Vec<IconInstance>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = icons.last() else {
            break;
        };
        let (x, y) = rect_center(last.rect);
        if !outer.contains(x, y) {
            break;
        }
        icons.pop();
    }
}

fn pop_quad_tail_inside(quads: &mut Vec<QuadInstance>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = quads.last() else {
            break;
        };
        let (x, y) = rect_center(last.rect);
        if !outer.contains(x, y) {
            break;
        }
        quads.pop();
    }
}

fn pop_label_tail_inside<'a>(labels: &mut Vec<LabelInfo<'a>>, outer: Rect, maximum: usize) {
    for _ in 0..maximum {
        let Some(last) = labels.last() else {
            break;
        };
        let x = last.bounds.x + last.bounds.width / 2.0;
        let y = last.bounds.y + last.bounds.height / 2.0;
        if !outer.contains(x, y) {
            break;
        }
        labels.pop();
    }
}

fn strip_legacy_popup<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
) {
    let Some((kind, outer)) = crate::detection_foreground::suppressed_popup() else {
        return;
    };
    match kind {
        crate::detection_foreground::PopupKind::Palette => {
            pop_icon_tail_inside(icons, outer, 9);
            pop_quad_tail_inside(quads, outer, 2);
        }
        crate::detection_foreground::PopupKind::Info => {
            pop_icon_tail_inside(icons, outer, 1);
            pop_label_tail_inside(labels, outer, 5);
            pop_quad_tail_inside(quads, outer, 2);
        }
    }
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    icons: &mut Vec<IconInstance>,
    detection_uvs: [[f32; 4]; 7],
) {
    let mut detector_quads = Vec::new();
    let mut detector_labels = Vec::new();
    let mut detector_icons = Vec::new();
    base::render_detection_overlay(
        zone,
        project,
        current_frame,
        state,
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
        detection_uvs,
    );
    strip_legacy_popup(
        &mut detector_quads,
        &mut detector_labels,
        &mut detector_icons,
    );
    quads.extend(detector_quads);
    labels.extend(detector_labels);
    icons.extend(detector_icons);

    // Mask the vertical guide in the badge itself. Icons are a later renderer
    // stage, so the original SVG remains untouched above this opaque quad.
    let selected = selected_detection(state);
    for track in 0..rythmo_layout::track_count() {
        let line_id = crate::detection::track_storage_line_id(track as u8);
        let Some(data) = project.detections().line(line_id) else {
            continue;
        };
        let track_rect = editor_track_body_rect_at_frame(
            project,
            rythmo_layout::y_slot_for_track_index(track),
            current_frame,
            zone,
        );
        for cue in data.source_detections() {
            let rect = sign_badge_rect(cue.media_tick, track_rect, current_frame, zone);
            if rect.x + rect.width < zone.x || rect.x > zone.x + zone.width {
                continue;
            }
            let address = crate::detection::DetectionAddress {
                line_id,
                detection_id: cue.id,
            };
            let color = if selected == Some(address) {
                [0.09, 0.16, 0.29, 0.998]
            } else {
                [0.055, 0.059, 0.074, 0.998]
            };
            quads.push(QuadInstance {
                rect: [rect.x, rect.y, rect.width, rect.height],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: rect.width / 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0, 0.0, 0.0, 0.22],
                shadow_blur: 1.5,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_and_guide_share_the_same_axis() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let track = Rect {
            x: 0.0,
            y: 20.0,
            width: 800.0,
            height: 50.0,
        };
        let rect = sign_badge_rect(crate::detection::MediaTick::ZERO, track, 0.0, &zone);
        assert_eq!(rect.x + rect.width / 2.0, zone.width / 2.0);
    }
}
