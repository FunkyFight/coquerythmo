//! Shared-geometry facade for the historical editor layout helpers.

use super::*;

#[path = "geometry.rs"]
mod implementation;

#[allow(unused_imports)]
pub(crate) use implementation::*;

fn horizontal_geometry(zone: &Rect) -> crate::rendering::rythmo::geometry::HorizontalRythmoGeometry {
    crate::rendering::rythmo::geometry::HorizontalRythmoGeometry::new(
        zone.x,
        zone.width,
        PLAYHEAD_WIDTH,
        ppf(),
    )
}

pub(crate) fn ppf() -> f32 {
    constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

pub(crate) fn render_window(
    zone: &Rect,
    current_frame: f64,
    margin_frames: i64,
) -> (i64, i64) {
    let window = horizontal_geometry(zone).visible_frame_window(current_frame, margin_frames);
    (window.first, window.last)
}

pub(crate) fn frame_to_x(frame: i64, current_frame: f64, zone: &Rect) -> f32 {
    horizontal_geometry(zone).frame_x(frame as f64, current_frame)
}

pub(crate) fn x_to_frame(x: f32, current_frame: f64, zone: &Rect) -> i64 {
    let geometry = horizontal_geometry(zone);
    f64_round_to_i64(
        current_frame
            + (x - geometry.timeline_origin_x) as f64 / geometry.pixels_per_frame as f64,
    )
}

pub(crate) fn line_visual_x_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> (f32, f32) {
    line_visual_x_width_with_karaoke_width(line, current_frame, zone, karaoke_preview, None)
}

pub(crate) fn line_visual_x_width_with_karaoke_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    active_karaoke_width: Option<f32>,
) -> (f32, f32) {
    let geometry = horizontal_geometry(zone);
    if karaoke_preview && line.karaoke_active(current_frame) {
        let width = active_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        return (geometry.centered_karaoke_x(width), width);
    }
    if karaoke_preview {
        let x = geometry.frame_x(line.start_frame as f64, current_frame);
        return (x, (line.duration_frames as f32 * ppf()).max(2.0));
    }

    let x = geometry.frame_x(line.start_frame as f64, current_frame);
    let width = (line.duration_frames as f32 * ppf()).max(2.0);
    (x, width)
}

pub(crate) fn badge_rect_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    badge_rect_for_line_with_karaoke_preview(project, line, current_frame, zone, false)
}

pub(crate) fn badge_rect_for_line_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(
        project,
        line,
        &line.character_name,
        current_frame,
        zone,
        karaoke_preview,
    )
}

pub(crate) fn badge_rect_for_name(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(project, line, name, current_frame, zone, false)
}

pub(crate) fn badge_rect_for_name_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> Rect {
    let (line_x, line_width) =
        line_visual_x_width(line, current_frame, zone, karaoke_preview);
    let body_rect = implementation::editor_track_body_rect_at_frame(
        project,
        line.y_slot,
        current_frame,
        zone,
    );
    let display_name = if matches!(
        line.kind,
        crate::rythmo_line::RythmoLineKind::AmbianceStart
    ) {
        crate::rythmo_line::ambiance_label(name)
    } else {
        name.to_owned()
    };
    let metrics = crate::rendering::rythmo::labels::character_label_metrics(
        &display_name,
        body_rect.height,
        1.0,
        ppf(),
    );
    let line_rect = Rect {
        x: line_x,
        y: body_rect.y,
        width: line_width,
        height: body_rect.height,
    };
    let x = if matches!(
        line.kind,
        crate::rythmo_line::RythmoLineKind::AmbianceStart
    ) {
        crate::rendering::rythmo::labels::ambiance_character_label_x(line_x, metrics.width)
    } else if karaoke_preview && line.karaoke_active(current_frame) {
        crate::rendering::rythmo::labels::centered_karaoke_character_label_x(
            line_rect,
            metrics.width,
            1.0,
        )
    } else {
        crate::rendering::rythmo::labels::normal_character_label_x(
            line_x,
            metrics.width,
            ppf(),
        )
    };
    Rect {
        x,
        y: body_rect.y,
        width: metrics.width,
        height: body_rect.height,
    }
}

pub(crate) fn badge_width(name: &str) -> f32 {
    let row_height = constants::SLOT_HEIGHT;
    crate::rendering::rythmo::labels::character_label_metrics(name, row_height, 1.0, ppf())
        .width
}

pub(crate) fn ambiance_badge_width(name: &str) -> f32 {
    let display = crate::rythmo_line::ambiance_label(name);
    crate::rendering::rythmo::labels::character_label_metrics(
        &display,
        constants::SLOT_HEIGHT,
        1.0,
        ppf(),
    )
    .width
    .max(150.0)
}

#[cfg(test)]
mod shared_geometry_tests {
    use super::*;

    #[test]
    fn editor_frame_origin_matches_configured_playhead() {
        crate::config::init();
        let zone = Rect {
            x: 10.0,
            y: 0.0,
            width: 800.0,
            height: 200.0,
        };
        let geometry = horizontal_geometry(&zone);
        assert_eq!(frame_to_x(100, 100.0, &zone), geometry.timeline_origin_x);
    }

    #[test]
    fn editor_karaoke_remains_physically_centered() {
        crate::config::init();
        let zone = Rect {
            x: 10.0,
            y: 0.0,
            width: 800.0,
            height: 200.0,
        };
        let geometry = horizontal_geometry(&zone);
        let width = 200.0;
        assert_eq!(
            geometry.centered_karaoke_x(width) + width * 0.5,
            geometry.viewport_center_x
        );
    }
}
