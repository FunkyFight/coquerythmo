//! Shared semantic normalization for MP4 CPU and GPU renderers.
//!
//! The returned project is an ephemeral render model. It removes hidden note
//! headers, preserves ordinary badge colours through synthetic label-only lines,
//! paints OFF/back/ambience decorations as exported drawing strokes and colours
//! ambience text red. Document and crossed-dialogue exports never use this copy.

use crate::constants;
use crate::project::Project;
use crate::rythmo_drawing::DrawingStroke;
use crate::rythmo_layout;
use crate::rythmo_line::RythmoLine;
use crate::rythmo_line_metadata::{decode, LinePresentation, LineSemanticKind};

const DIALOGUE_TEXT_COLOR: [f32; 4] = [0.92, 0.92, 0.95, 1.0];
const AMBIENCE_TEXT_COLOR: [f32; 4] = [0.96, 0.16, 0.18, 1.0];
const AMBIENCE_LABEL_COLOR: [f32; 4] = [0.24, 0.56, 0.98, 1.0];
const DECORATION_COLOR: [f32; 4] = [0.93, 0.93, 0.96, 0.95];
const AMBIENCE_SYMBOL_COLOR: [f32; 4] = [0.26, 0.56, 0.96, 0.95];
const LABEL_LEAD_FRAMES: i64 = 4;

#[derive(Clone, Copy)]
struct TrackGeometry {
    center_y: f32,
    underline_y: f32,
    symbol_half_height: f32,
    stroke_radius: f32,
}

fn export_track_geometries(project: &Project) -> Vec<TrackGeometry> {
    let normal_body_h = constants::SLOT_HEIGHT;
    let slot_header_h = constants::BADGE_HEIGHT.max(constants::VOICE_ACTOR_DISPLAY_ICON_SIZE);
    let badge_gap = constants::BADGE_GAP;
    let layouts = rythmo_layout::build_track_layouts(
        project,
        &rythmo_layout::all_track_indices(),
        normal_body_h,
        slot_header_h,
        badge_gap,
        1.0,
    );
    let total_height =
        (constants::RULER_HEIGHT + rythmo_layout::total_tracks_height(&layouts)).max(1.0);
    let symbol_half_height = (7.0 / total_height).max(0.002);
    let stroke_radius = (1.15 / total_height).max(0.0008);

    (0..rythmo_layout::track_count())
        .map(|track_index| {
            let layout = rythmo_layout::track_for_index(&layouts, track_index)
                .expect("all export tracks should have a layout");
            let body_y = constants::RULER_HEIGHT + layout.top + slot_header_h + badge_gap;
            TrackGeometry {
                center_y: (body_y + normal_body_h * 0.5) / total_height,
                underline_y: (body_y + normal_body_h - 2.0) / total_height,
                symbol_half_height,
                stroke_radius,
            }
        })
        .collect()
}

fn add_stroke(
    project: &mut Project,
    points: Vec<(f64, f32)>,
    color: [f32; 4],
    radius_frac: f32,
) {
    if points.is_empty() {
        return;
    }
    let mut stroke = DrawingStroke::new(0, color, radius_frac);
    stroke.points = points;
    project.add_drawing_stroke(stroke);
}

fn add_full_underline(project: &mut Project, line: &RythmoLine, geometry: TrackGeometry) {
    add_stroke(
        project,
        vec![
            (line.start_frame as f64, geometry.underline_y),
            (line.end_frame() as f64, geometry.underline_y),
        ],
        DECORATION_COLOR,
        geometry.stroke_radius,
    );
}

fn add_dashed_underline(project: &mut Project, line: &RythmoLine, geometry: TrackGeometry) {
    let mut start = line.start_frame;
    let end = line.end_frame();
    while start < end {
        let dash_end = start.saturating_add(4).min(end);
        add_stroke(
            project,
            vec![
                (start as f64, geometry.underline_y),
                (dash_end as f64, geometry.underline_y),
            ],
            DECORATION_COLOR,
            geometry.stroke_radius,
        );
        start = start.saturating_add(7);
    }
}

fn add_ambience_symbol(
    project: &mut Project,
    frame: i64,
    geometry: TrackGeometry,
    points_right: bool,
) {
    let x = frame as f64;
    let y = geometry.center_y;
    let h = geometry.symbol_half_height;
    let horizontal = if points_right { 2.0 } else { -2.0 };
    add_stroke(
        project,
        vec![(x, y - h), (x, y + h)],
        AMBIENCE_SYMBOL_COLOR,
        geometry.stroke_radius,
    );
    add_stroke(
        project,
        vec![
            (x - horizontal, y - h),
            (x, y),
            (x - horizontal, y + h),
        ],
        AMBIENCE_SYMBOL_COLOR,
        geometry.stroke_radius,
    );
}

fn add_ambience_label_underline(
    project: &mut Project,
    start_frame: i64,
    end_frame: i64,
    geometry: TrackGeometry,
) {
    let y = geometry.center_y + geometry.symbol_half_height * 0.72;
    add_stroke(
        project,
        vec![(start_frame as f64, y), (end_frame as f64, y)],
        [
            AMBIENCE_LABEL_COLOR[0],
            AMBIENCE_LABEL_COLOR[1],
            AMBIENCE_LABEL_COLOR[2],
            0.72,
        ],
        geometry.stroke_radius,
    );
}

fn label_anchor_frame(line: &RythmoLine) -> i64 {
    line.start_frame
        .saturating_sub(LABEL_LEAD_FRAMES)
        .max(0)
}

fn dialogue_label_start(line: &RythmoLine) -> i64 {
    let badge_gap_frames = (constants::BADGE_GAP
        / (constants::PIXELS_PER_FRAME * crate::config::scroll_speed()).max(0.001))
    .ceil() as i64;
    label_anchor_frame(line).saturating_add(badge_gap_frames)
}

fn ambience_label_interval(line: &RythmoLine) -> (i64, i64) {
    let end = label_anchor_frame(line);
    let width = (line.character_name.chars().count() as i64 * 2).max(8);
    (end.saturating_sub(width).max(0), end.max(1))
}

fn add_dialogue_badge_line(export: &mut Project, original: &RythmoLine) {
    if original.character_name.trim().is_empty() {
        return;
    }
    let start = dialogue_label_start(original);
    export.add_line_full_with_voice_actors(
        start,
        original.duration_frames.max(1),
        original.y_slot,
        String::new(),
        original.character_name.clone(),
        original.character_color,
        original.voice_actor_names.clone(),
    );
}

fn add_ambience_label_line(
    export: &mut Project,
    original: &RythmoLine,
    geometry: TrackGeometry,
) {
    if original.character_name.trim().is_empty() {
        return;
    }
    let (start, end) = ambience_label_interval(original);
    let duration = end.saturating_sub(start).max(1);
    export.add_line_full(
        start,
        duration,
        original.y_slot,
        original.character_name.clone(),
        String::new(),
        AMBIENCE_LABEL_COLOR,
    );
    add_ambience_label_underline(export, start, end, geometry);
}

/// Produce an export-only project whose ordinary primitives express all line
/// semantics identically for the CPU and GPU renderers.
pub fn normalize_for_video(project: &Project) -> Project {
    let originals = project.lines_vec();
    let geometries = export_track_geometries(project);
    let mut export = project.clone();
    let mut settings = export.settings().clone();
    settings.scrolling_text_uses_character_color = true;
    export.set_settings(settings);

    for original in &originals {
        if original.karaoke {
            continue;
        }
        let (metadata, visible_note) = decode(&original.note);
        let track_index = rythmo_layout::track_index_for_y_slot(original.y_slot);
        let geometry = geometries
            .get(track_index)
            .copied()
            .unwrap_or(TrackGeometry {
                center_y: 0.5,
                underline_y: 0.5,
                symbol_half_height: 0.01,
                stroke_radius: 0.001,
            });

        if let Some(line) = export.get_line_mut(original.id) {
            line.note = visible_note.to_string();
            line.character_name.clear();
            line.voice_actor_names.clear();
            line.character_color = if metadata.kind == LineSemanticKind::Dialogue {
                DIALOGUE_TEXT_COLOR
            } else {
                AMBIENCE_TEXT_COLOR
            };
        }

        match metadata.presentation {
            LinePresentation::On => {}
            LinePresentation::Off => add_full_underline(&mut export, original, geometry),
            LinePresentation::Back => add_dashed_underline(&mut export, original, geometry),
        }

        match metadata.kind {
            LineSemanticKind::Dialogue => add_dialogue_badge_line(&mut export, original),
            LineSemanticKind::AmbienceStart => {
                add_ambience_label_line(&mut export, original, geometry);
                add_ambience_symbol(
                    &mut export,
                    original.start_frame.saturating_sub(2).max(0),
                    geometry,
                    true,
                );
            }
            LineSemanticKind::AmbienceEnd => {
                add_ambience_symbol(
                    &mut export,
                    original.end_frame().saturating_add(2),
                    geometry,
                    false,
                );
            }
        }
    }

    export
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rythmo_line_metadata::{with_kind, with_presentation};

    #[test]
    fn normalization_never_mutates_source_project() {
        let mut source = Project::new();
        let id = source.add_line(20, 30, 0.0);
        let line = source.get_line_mut(id).unwrap();
        line.text = "Ambiance".to_string();
        line.character_name = "FOULE".to_string();
        line.note = with_kind("note", LineSemanticKind::AmbienceStart);
        let normalized = normalize_for_video(&source);
        assert_eq!(source.get_line(id).unwrap().character_name, "FOULE");
        assert_eq!(source.get_line(id).unwrap().note, line.note);
        assert_eq!(normalized.get_line(id).unwrap().character_name, "");
        assert_eq!(normalized.get_line(id).unwrap().note, "note");
        assert!(normalized.line_count() > source.line_count());
    }

    #[test]
    fn off_and_back_add_exported_drawing_strokes() {
        let mut source = Project::new();
        let off = source.add_line(0, 24, 0.0);
        let back = source.add_line(30, 24, 0.25);
        source.get_line_mut(off).unwrap().note =
            with_presentation("", LinePresentation::Off);
        source.get_line_mut(back).unwrap().note =
            with_presentation("", LinePresentation::Back);
        let before = source.drawing().strokes.len();
        let normalized = normalize_for_video(&source);
        assert!(normalized.drawing().strokes.len() > before + 1);
    }

    #[test]
    fn production_markers_are_not_promoted_to_export_markers() {
        let mut source = Project::new();
        let original_markers = source.marker_count();
        let normalized = normalize_for_video(&source);
        assert_eq!(normalized.marker_count(), original_markers);
    }
}
