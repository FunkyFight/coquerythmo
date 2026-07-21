//! Rythmo view facade.
//!
//! The established renderer remains the source of geometry and text data. This
//! boundary removes syllable authoring chrome from ordinary adaptation lines
//! and composes semantic OFF, back-facing and ambience visuals.

#[path = "view.rs"]
mod legacy;

pub use legacy::*;

use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_line_metadata::{decode, user_note, LinePresentation, LineSemanticKind};
use crate::ui::primitives::{
    HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, VAlign,
};
use crate::ui::renderer::StretchedText;

fn quad_center(quad: &QuadInstance) -> (f32, f32) {
    (
        quad.rect[0] + quad.rect[2] * 0.5,
        quad.rect[1] + quad.rect[3] * 0.5,
    )
}

fn is_syllable_handle(quad: &QuadInstance) -> bool {
    quad.color[0] >= 0.94
        && quad.color[1] <= 0.10
        && quad.color[2] <= 0.06
        && quad.rect[2] > 0.0
        && quad.rect[3] > 0.0
}

fn strip_normal_line_syllable_handles(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    first_new_quad: usize,
    syllable_quads: &mut Vec<QuadInstance>,
) {
    let normal_rects = project
        .lines()
        .filter(|line| !line.karaoke)
        .map(|line| legacy::line_rect(project, line, current_frame, zone))
        .collect::<Vec<_>>();

    let mut index = first_new_quad.min(syllable_quads.len());
    while index < syllable_quads.len() {
        let quad = &syllable_quads[index];
        let (x, y) = quad_center(quad);
        let belongs_to_normal_line = normal_rects.iter().any(|rect| rect.contains(x, y));
        if is_syllable_handle(quad) && belongs_to_normal_line {
            syllable_quads.remove(index);
        } else {
            index += 1;
        }
    }
}

fn push_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], radius: f32) {
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

fn push_dashed_underline(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4]) {
    let mut x = rect.x;
    let end = rect.x + rect.width;
    while x < end {
        let width = 8.0_f32.min(end - x);
        push_quad(
            quads,
            Rect {
                x,
                y: rect.y + rect.height - 2.0,
                width,
                height: 2.0,
            },
            color,
            1.0,
        );
        x += 13.0;
    }
}

fn push_ambience_symbol(quads: &mut Vec<QuadInstance>, x: f32, y: f32, points_right: bool) {
    let color = [0.26, 0.56, 0.96, 0.95];
    push_quad(
        quads,
        Rect {
            x: x - 1.0,
            y: y - 8.0,
            width: 2.0,
            height: 16.0,
        },
        color,
        1.0,
    );
    let direction = if points_right { 1.0 } else { -1.0 };
    for dy in [-5.0_f32, 5.0] {
        quads.push(QuadInstance {
            rect: [x + direction * 3.0 - 5.0, y + dy - 1.0, 10.0, 2.0],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 1.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: if points_right { dy.signum() * 0.55 } else { -dy.signum() * 0.55 },
            _padding: [0.0; 2],
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_line_semantics<'a>(
    project: &'a Project,
    current_frame: f64,
    zone: &Rect,
    first_quad: usize,
    first_label: usize,
    first_stretched: usize,
    first_note_icon: usize,
    first_actor_icon: usize,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
) {
    for line in project.lines().filter(|line| !line.karaoke) {
        let (metadata, visible_note) = decode(&line.note);
        if metadata.presentation == LinePresentation::On
            && metadata.kind == LineSemanticKind::Dialogue
            && visible_note == line.note
        {
            continue;
        }

        let line_rect = legacy::line_rect(project, line, current_frame, zone);
        let badge_rect = legacy::badge_rect_for_line(project, line, current_frame, zone);

        // Keep the serialized metadata header out of the visible note editor.
        for label in labels.iter_mut().skip(first_label) {
            if label.text == line.note.as_str() {
                label.text = user_note(&line.note);
                if label.text.is_empty() {
                    label.bounds.width = 0.0;
                    label.bounds.height = 0.0;
                }
            }
        }
        if visible_note.is_empty() {
            note_icons[first_note_icon.min(note_icons.len())..].iter_mut().for_each(|icon| {
                let x = icon.rect[0] + icon.rect[2] * 0.5;
                let y = icon.rect[1] + icon.rect[3] * 0.5;
                if badge_rect.contains(x, y) {
                    icon.rect[2] = 0.0;
                    icon.rect[3] = 0.0;
                }
            });
        }

        match metadata.presentation {
            LinePresentation::On => {}
            LinePresentation::Off => push_quad(
                quads,
                Rect {
                    x: line_rect.x,
                    y: line_rect.y + line_rect.height - 2.0,
                    width: line_rect.width,
                    height: 2.0,
                },
                [0.93, 0.93, 0.96, 0.95],
                1.0,
            ),
            LinePresentation::Back => {
                push_dashed_underline(quads, line_rect, [0.93, 0.93, 0.96, 0.95])
            }
        }

        if metadata.kind == LineSemanticKind::Dialogue {
            continue;
        }

        // Ambience lines do not use the normal colored character badge.
        quads[first_quad.min(quads.len())..].iter_mut().for_each(|quad| {
            let (x, y) = quad_center(quad);
            if badge_rect.contains(x, y) {
                quad.rect[2] = 0.0;
                quad.rect[3] = 0.0;
            }
        });
        for label in labels.iter_mut().skip(first_label) {
            let center_x = label.bounds.x + label.bounds.width * 0.5;
            let center_y = label.bounds.y + label.bounds.height * 0.5;
            if badge_rect.contains(center_x, center_y) {
                label.bounds.width = 0.0;
                label.bounds.height = 0.0;
            }
        }
        actor_icons[first_actor_icon.min(actor_icons.len())..]
            .iter_mut()
            .filter(|draw| {
                let center_y = draw.rect.y + draw.rect.height * 0.5;
                center_y >= badge_rect.y && center_y <= badge_rect.y + badge_rect.height
            })
            .for_each(|draw| {
                draw.rect.width = 0.0;
                draw.rect.height = 0.0;
            });

        // The descriptive text of every ambience is red in the editor.
        for text in stretched.iter_mut().skip(first_stretched) {
            let center_x = text.dest_rect.x + text.dest_rect.width * 0.5;
            let center_y = text.dest_rect.y + text.dest_rect.height * 0.5;
            if line_rect.contains(center_x, center_y) {
                text.tint = [0.96, 0.16, 0.18, 1.0];
            }
        }

        let symbol_y = line_rect.y + line_rect.height * 0.5;
        match metadata.kind {
            LineSemanticKind::Dialogue => {}
            LineSemanticKind::AmbienceStart => {
                let label_rect = Rect {
                    x: line_rect.x - 116.0,
                    y: line_rect.y,
                    width: 104.0,
                    height: line_rect.height,
                };
                push_quad(quads, label_rect, [0.14, 0.34, 0.72, 0.34], 3.0);
                labels.push(LabelInfo {
                    text: &line.character_name,
                    bounds: label_rect,
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Ellipsis,
                    padding: 4.0,
                    font_size_override: Some(12.0),
                    color_override: Some([88, 158, 255]),
                    font_family_override: None,
                });
                push_ambience_symbol(quads, line_rect.x - 6.0, symbol_y, true);
            }
            LineSemanticKind::AmbienceEnd => {
                push_ambience_symbol(
                    quads,
                    line_rect.x + line_rect.width + 6.0,
                    symbol_y,
                    false,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
    detection_uvs: [[f32; 4]; 7],
) -> Option<(
    u64,
    usize,
    Option<(usize, usize)>,
    f32,
    f32,
    f32,
    f32,
    Option<Vec<CursorSegmentInfo>>,
)> {
    let first_quad = quads.len();
    let first_new_quad = syllable_quads.len();
    let first_label = labels.len();
    let first_stretched = stretched.len();
    let first_note_icon = note_icons.len();
    let first_actor_icon = actor_icons.len();
    let result = legacy::render_lines(
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        quads,
        syllable_quads,
        labels,
        stretched,
        note_icons,
        actor_icons,
        note_uv,
        detection_uvs,
    );
    strip_normal_line_syllable_handles(
        project,
        current_frame,
        zone,
        first_new_quad,
        syllable_quads,
    );
    apply_line_semantics(
        project,
        current_frame,
        zone,
        first_quad,
        first_label,
        first_stretched,
        first_note_icon,
        first_actor_icon,
        quads,
        labels,
        stretched,
        note_icons,
        actor_icons,
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_handle(rect: [f32; 4]) -> QuadInstance {
        QuadInstance {
            rect,
            color: [0.95, 0.08, 0.03, 1.0],
            color_bottom: [0.95, 0.08, 0.03, 1.0],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }
    }

    #[test]
    fn handle_classifier_only_accepts_red_syllable_chrome() {
        assert!(is_syllable_handle(&red_handle([0.0, 0.0, 10.0, 3.0])));
        let mut other = red_handle([0.0, 0.0, 10.0, 3.0]);
        other.color = [0.48, 0.72, 1.0, 1.0];
        assert!(!is_syllable_handle(&other));
    }
}
