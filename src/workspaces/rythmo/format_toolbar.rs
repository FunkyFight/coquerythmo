//! Floating formatting bar for non-karaoke dialogue text.
//!
//! It appears above the line being edited, or above the only selected line,
//! and toggles bold, italic, underline and strikethrough on the text
//! selection, or on the whole line when nothing is selected.

use super::*;
use crate::rythmo_line::TextStyleKind;

const TOOLBAR_BUTTON_W: f32 = 28.0;
const TOOLBAR_H: f32 = 26.0;
const TOOLBAR_PADDING: f32 = 3.0;
const TOOLBAR_GAP: f32 = 4.0;

fn short_label(kind: TextStyleKind) -> &'static str {
    match kind {
        TextStyleKind::Bold => t("text_style.bold_short"),
        TextStyleKind::Italic => t("text_style.italic_short"),
        TextStyleKind::Underline => t("text_style.underline_short"),
        TextStyleKind::Strikethrough => t("text_style.strikethrough_short"),
    }
}

/// Line and character range the formatting bar acts on. `None` as range
/// stands for the whole line.
pub(crate) fn format_toolbar_target(
    project: &Project,
    state: &RythmoState,
) -> Option<(u64, Option<(usize, usize)>)> {
    // Only a finished text selection inside the line being edited shows the
    // bar; a selected line or a caret alone does not.
    let line_id = state.editing_line?;
    if state.dragging.is_some() {
        return None;
    }
    let range = state
        .line_input
        .selection_range()
        .filter(|(start, end)| start < end)?;
    let line = project.get_line(line_id)?;
    if !line.can_have_text_styles() || line.text.is_empty() {
        return None;
    }
    Some((line_id, Some(range)))
}

struct FormatToolbarLayout {
    bar: Rect,
    line_id: u64,
    range: Option<(usize, usize)>,
}

fn format_toolbar_layout(
    project: &Project,
    state: &RythmoState,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
) -> Option<FormatToolbarLayout> {
    if state.has_context_menu() || state.editing_character.is_some() {
        return None;
    }
    let (line_id, range) = format_toolbar_target(project, state)?;
    let line = project.get_line(line_id)?;
    let line_rect = line_rect(
        project,
        line,
        current_frame,
        zone,
        crate::config::reading_bar_offset_seconds(),
        fps,
    );
    if line_rect.x + line_rect.width < zone.x || line_rect.x > zone.x + zone.width {
        return None;
    }
    let count = TextStyleKind::ALL.len() as f32;
    let width = TOOLBAR_PADDING * 2.0 + TOOLBAR_BUTTON_W * count;
    let max_x = (zone.x + zone.width - width - TOOLBAR_GAP).max(zone.x);
    let x = line_rect.x.clamp(zone.x + TOOLBAR_GAP, max_x);
    let above = line_rect.y - TOOLBAR_H - TOOLBAR_GAP;
    let y = if above >= zone.y {
        above
    } else {
        line_rect.y + line_rect.height + TOOLBAR_GAP
    };
    Some(FormatToolbarLayout {
        bar: Rect {
            x,
            y,
            width,
            height: TOOLBAR_H,
        },
        line_id,
        range,
    })
}

fn button_rect(bar: Rect, index: usize) -> Rect {
    Rect {
        x: bar.x + TOOLBAR_PADDING + TOOLBAR_BUTTON_W * index as f32,
        y: bar.y + TOOLBAR_PADDING,
        width: TOOLBAR_BUTTON_W,
        height: bar.height - TOOLBAR_PADDING * 2.0,
    }
}

fn button_at(bar: Rect, x: f32, y: f32) -> Option<usize> {
    (0..TextStyleKind::ALL.len()).find(|index| button_rect(bar, *index).contains(x, y))
}

/// Pointer handling for the bar. Returns `None` for events it lets through.
pub(crate) fn handle_format_toolbar_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    let Some(layout) = format_toolbar_layout(project, state, current_frame, zone, fps) else {
        state.format_toolbar_hover = None;
        return None;
    };
    match event {
        UiEvent::MouseMove { x, y } => {
            state.format_toolbar_hover = button_at(layout.bar, *x, *y);
            None
        }
        UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
            if layout.bar.contains(*x, *y) =>
        {
            Some(match button_at(layout.bar, *x, *y) {
                Some(index) => EventResponse::Action(UiAction::ToggleTextStyle {
                    kind: TextStyleKind::ALL[index],
                }),
                None => EventResponse::Consumed,
            })
        }
        UiEvent::MouseRelease { x, y } if layout.bar.contains(*x, *y) => {
            Some(EventResponse::Consumed)
        }
        _ => None,
    }
}

/// Full name of the hovered button, for the tooltip.
pub(crate) fn format_toolbar_tooltip(
    project: &Project,
    state: &RythmoState,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    x: f32,
    y: f32,
) -> Option<&'static str> {
    let layout = format_toolbar_layout(project, state, current_frame, zone, fps)?;
    button_at(layout.bar, x, y).map(|index| t(TextStyleKind::ALL[index].i18n_key()))
}

pub(crate) fn render_format_toolbar(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'_>>,
) {
    let Some(layout) = format_toolbar_layout(project, state, current_frame, zone, fps) else {
        return;
    };
    let Some(line) = project.get_line(layout.line_id) else {
        return;
    };
    crate::ui::context_menu::render_panel(quads, layout.bar);
    let (start, end) = layout.range.unwrap_or((0, 0));
    for (index, kind) in TextStyleKind::ALL.into_iter().enumerate() {
        let rect = button_rect(layout.bar, index);
        let active = line.text_style_active(start, end, kind);
        let hovered = state.format_toolbar_hover == Some(index);
        if active || hovered {
            let color = if active {
                [0.31, 0.40, 0.72, 0.95]
            } else {
                [0.30, 0.30, 0.36, 0.85]
            };
            quads.push(QuadInstance {
                rect: [rect.x + 1.0, rect.y, rect.width - 2.0, rect.height],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 3.0,
                shadow_offset: [0.0, 0.0],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        labels.push(LabelInfo {
            text: short_label(kind),
            bounds: rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(13.0),
            color_override: Some([235, 235, 242]),
            font_family_override: None,
        });
        // The letters are drawn plain: underline and strike them so each
        // button shows its effect.
        let decoration_y = match kind {
            TextStyleKind::Underline => Some(rect.y + rect.height * 0.5 + 7.0),
            TextStyleKind::Strikethrough => Some(rect.y + rect.height * 0.5),
            _ => None,
        };
        if let Some(y) = decoration_y {
            quads.push(QuadInstance {
                rect: [rect.x + rect.width * 0.5 - 6.0, y, 12.0, 1.5],
                color: [0.92, 0.92, 0.95, 1.0],
                color_bottom: [0.92, 0.92, 0.95, 1.0],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0, 0.0],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_line(karaoke: bool) -> (Project, u64) {
        let mut project = Project::new();
        let id = project.add_line_full(0, 48, 0.5, "bonjour".into(), "A".into(), [1.0; 4]);
        project.get_line_mut(id).unwrap().karaoke = karaoke;
        (project, id)
    }

    fn editing(id: u64, selection: Option<(usize, usize)>) -> RythmoState {
        let mut state = RythmoState::default();
        state.selected = Some(Selection::Line(id));
        state.editing_line = Some(id);
        state.line_input.activate("bonjour");
        if let Some((start, end)) = selection {
            state.line_input.select_range(start, end);
        }
        state
    }

    #[test]
    fn appears_only_for_a_text_selection() {
        let (project, id) = project_with_line(false);
        assert_eq!(
            format_toolbar_target(&project, &editing(id, Some((1, 4)))),
            Some((id, Some((1, 4))))
        );
        assert_eq!(format_toolbar_target(&project, &editing(id, None)), None);
        let mut selected_only = RythmoState::default();
        selected_only.selected = Some(Selection::Line(id));
        assert_eq!(format_toolbar_target(&project, &selected_only), None);
    }

    #[test]
    fn ignores_karaoke_lines() {
        let (project, id) = project_with_line(true);
        assert_eq!(
            format_toolbar_target(&project, &editing(id, Some((0, 3)))),
            None
        );
    }
}
