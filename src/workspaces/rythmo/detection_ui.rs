//! Detection UI facade.
//!
//! Interaction and the established detector renderer remain in
//! `detection_ui_base.rs`. This facade inserts one opaque badge background into
//! the same rythmo layer after the vertical guide and before icon rendering, so
//! the guide never shows through the sign while the original SVG remains
//! untouched above it.

use super::*;

#[path = "detection_ui_base.rs"]
mod base;

pub use base::{DetectionDrag, DetectionHover, DetectionMenu};
pub(crate) use base::{
    decode_sync_syllable_drag_line_id, encode_sync_syllable_drag_line_id,
    handle_detection_event, line_has_visible_sync_points, render_sync_text_segments,
    sync_syllable_boundary_ratios,
};

const SIGN_BADGE_SIZE: f32 = 26.0;
const SIGN_BOTTOM_MARGIN: f32 = 2.0;

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
    base::render_detection_overlay(
        zone,
        project,
        current_frame,
        state,
        quads,
        labels,
        icons,
        detection_uvs,
    );

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
