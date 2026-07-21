//! Public detector foreground facade.
//!
//! The detector palette and information card remain unchanged, the obsolete
//! hover plus is suppressed, and a keyboard-accessible second row exposes line
//! presentation and ambience semantics.

use crate::accessibility::AccessibilityEvent;
use crate::project::Project;
use crate::rythmo_line_metadata::{
    decode, with_kind, with_presentation, LinePresentation, LineSemanticKind,
};
use crate::ui::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{RythmoState, Selection};
use std::sync::{Mutex, OnceLock};

#[path = "detection_foreground.rs"]
mod legacy;

pub use legacy::{activate_palette, selected_info_accessibility_label};
pub(crate) use legacy::{reconcile_legacy_menu, suppressed_popup, PopupKind};

const EXTRA_ROW_GAP: f32 = 5.0;
const EXTRA_ITEM_GAP: f32 = 4.0;
const EXTRA_ITEM_WIDTH: f32 = 122.0;
const EXTRA_ITEM_HEIGHT: f32 = 30.0;
const EXTRA_LABELS: [&str; 4] = ["OFF", "De dos", "Début ambiance", "Fin ambiance"];

#[derive(Default)]
struct ExtraState {
    line_id: Option<u64>,
    note: String,
    focused: Option<usize>,
    palette_rect: Option<Rect>,
}

fn extra_state() -> &'static Mutex<ExtraState> {
    static STATE: OnceLock<Mutex<ExtraState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ExtraState::default()))
}

fn lock_extra() -> std::sync::MutexGuard<'static, ExtraState> {
    extra_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn selected_line_id(project: &Project, state: &RythmoState) -> Option<u64> {
    let candidate = match state.selected.as_ref() {
        Some(Selection::Line(line_id)) => Some(*line_id),
        Some(Selection::Lines(line_ids)) => line_ids.first().copied(),
        Some(Selection::AllLines) => project.lines().next().map(|line| line.id),
        Some(Selection::Detection(address)) if address.track().is_none() => Some(address.line_id),
        _ => state.hovered_line,
    }?;
    project.get_line(candidate).map(|line| line.id)
}

fn extra_row_rect(palette: Rect) -> Rect {
    Rect {
        x: palette.x,
        y: palette.y + palette.height + EXTRA_ROW_GAP,
        width: EXTRA_ITEM_WIDTH * EXTRA_LABELS.len() as f32
            + EXTRA_ITEM_GAP * (EXTRA_LABELS.len() - 1) as f32,
        height: EXTRA_ITEM_HEIGHT,
    }
}

fn extra_item_rect(row: Rect, index: usize) -> Rect {
    Rect {
        x: row.x + index as f32 * (EXTRA_ITEM_WIDTH + EXTRA_ITEM_GAP),
        y: row.y,
        width: EXTRA_ITEM_WIDTH,
        height: EXTRA_ITEM_HEIGHT,
    }
}

fn extra_item_at(row: Rect, x: f32, y: f32) -> Option<usize> {
    (0..EXTRA_LABELS.len()).find(|index| extra_item_rect(row, *index).contains(x, y))
}

fn extra_item_active(index: usize, note: &str) -> bool {
    let (metadata, _) = decode(note);
    match index {
        0 => metadata.presentation == LinePresentation::Off,
        1 => metadata.presentation == LinePresentation::Back,
        2 => metadata.kind == LineSemanticKind::AmbienceStart,
        3 => metadata.kind == LineSemanticKind::AmbienceEnd,
        _ => false,
    }
}

fn updated_note(index: usize, note: &str) -> String {
    let (metadata, _) = decode(note);
    match index {
        0 => with_presentation(
            note,
            if metadata.presentation == LinePresentation::Off {
                LinePresentation::On
            } else {
                LinePresentation::Off
            },
        ),
        1 => with_presentation(
            note,
            if metadata.presentation == LinePresentation::Back {
                LinePresentation::On
            } else {
                LinePresentation::Back
            },
        ),
        2 => with_kind(
            note,
            if metadata.kind == LineSemanticKind::AmbienceStart {
                LineSemanticKind::Dialogue
            } else {
                LineSemanticKind::AmbienceStart
            },
        ),
        3 => with_kind(
            note,
            if metadata.kind == LineSemanticKind::AmbienceEnd {
                LineSemanticKind::Dialogue
            } else {
                LineSemanticKind::AmbienceEnd
            },
        ),
        _ => note.to_string(),
    }
}

fn focus_announcement(index: usize, note: &str) -> EventResponse {
    let state = if extra_item_active(index, note) {
        "activé"
    } else {
        "désactivé"
    };
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Selection {
        label: format!("{}, {}", EXTRA_LABELS[index], state),
    }))
}

fn activate_extra(index: usize) -> EventResponse {
    let mut state = lock_extra();
    let Some(line_id) = state.line_id else {
        return EventResponse::Consumed;
    };
    let note = updated_note(index, &state.note);
    state.note = note.clone();
    let active = extra_item_active(index, &note);
    EventResponse::Actions(vec![
        UiAction::UpdateLineNote { line_id, note },
        UiAction::Accessibility(AccessibilityEvent::Success {
            message: format!(
                "{} {}",
                EXTRA_LABELS[index],
                if active { "activé" } else { "désactivé" }
            ),
        }),
    ])
}

pub fn captures_input() -> bool {
    legacy::captures_input()
}

pub fn handle_modal_event(event: &UiEvent) -> Option<EventResponse> {
    let (line_id, note, palette_rect, focused) = {
        let state = lock_extra();
        (
            state.line_id,
            state.note.clone(),
            state.palette_rect,
            state.focused,
        )
    };

    if line_id.is_some() {
        if let Some(palette) = palette_rect {
            let row = extra_row_rect(palette);
            match event {
                UiEvent::MouseMove { x, y } => {
                    if let Some(index) = extra_item_at(row, *x, *y) {
                        lock_extra().focused = Some(index);
                        return Some(EventResponse::Consumed);
                    }
                }
                UiEvent::MousePress { x, y } => {
                    if let Some(index) = extra_item_at(row, *x, *y) {
                        lock_extra().focused = Some(index);
                        return Some(activate_extra(index));
                    }
                }
                UiEvent::FocusNext => {
                    let next = focused.map_or(0, |index| (index + 1) % EXTRA_LABELS.len());
                    lock_extra().focused = Some(next);
                    return Some(focus_announcement(next, &note));
                }
                UiEvent::FocusPrevious if focused.is_some() => {
                    let next = focused
                        .map(|index| (index + EXTRA_LABELS.len() - 1) % EXTRA_LABELS.len())
                        .unwrap_or(0);
                    lock_extra().focused = Some(next);
                    return Some(focus_announcement(next, &note));
                }
                UiEvent::CursorLeft if focused.is_some() => {
                    let next = (focused.unwrap() + EXTRA_LABELS.len() - 1) % EXTRA_LABELS.len();
                    lock_extra().focused = Some(next);
                    return Some(focus_announcement(next, &note));
                }
                UiEvent::CursorRight if focused.is_some() => {
                    let next = (focused.unwrap() + 1) % EXTRA_LABELS.len();
                    lock_extra().focused = Some(next);
                    return Some(focus_announcement(next, &note));
                }
                UiEvent::CursorUp if focused.is_some() => {
                    lock_extra().focused = None;
                    return Some(EventResponse::Action(UiAction::Accessibility(
                        AccessibilityEvent::Focus {
                            label: "Signes de détection".to_string(),
                            role: "liste".to_string(),
                        },
                    )));
                }
                UiEvent::Activate if focused.is_some() => {
                    return Some(activate_extra(focused.unwrap()));
                }
                UiEvent::KeyInput { text }
                    if focused.is_some() && (text == "\r" || text == "\n") =>
                {
                    return Some(activate_extra(focused.unwrap()));
                }
                _ => {}
            }
        }
    }

    legacy::handle_modal_event(event)
}

pub fn sync_from_state(
    project: &Project,
    state: &RythmoState,
    zone: Rect,
    current_frame: f64,
    event: &UiEvent,
) {
    legacy::sync_from_state(project, state, zone, current_frame, event);

    let palette_rect = match legacy::suppressed_popup() {
        Some((legacy::PopupKind::Palette, rect)) => Some(rect),
        _ => None,
    };
    let line_id = selected_line_id(project, state);
    let note = line_id
        .and_then(|line_id| project.get_line(line_id))
        .map(|line| line.note.clone())
        .unwrap_or_default();

    let mut extra = lock_extra();
    extra.line_id = line_id;
    extra.note = note;
    extra.palette_rect = palette_rect;
    if palette_rect.is_none() || line_id.is_none() {
        extra.focused = None;
    }
}

pub fn clear() {
    legacy::clear();
    *lock_extra() = ExtraState::default();
}

fn push_panel(quads: &mut Vec<QuadInstance>, rect: Rect, active: bool, focused: bool) {
    let color = if focused {
        [0.18, 0.32, 0.58, 0.998]
    } else if active {
        [0.10, 0.26, 0.20, 0.998]
    } else {
        [0.045, 0.050, 0.065, 0.998]
    };
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: if active {
            [0.34, 0.78, 0.58, 0.9]
        } else {
            [0.28, 0.30, 0.36, 0.8]
        },
        border_width: 1.0,
        border_radius: 5.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.28],
        shadow_blur: 3.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

pub fn append_foreground<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let quad_start = quads.len();
    let label_start = labels.len();
    legacy::append_foreground(quads, labels, screen_w, screen_h);

    // With no popup open, the legacy foreground only emits the obsolete plus.
    if legacy::suppressed_popup().is_none() {
        quads.truncate(quad_start);
        labels.truncate(label_start);
        return;
    }

    let state = lock_extra();
    let (Some(_), Some(palette)) = (state.line_id, state.palette_rect) else {
        return;
    };
    let mut row = extra_row_rect(palette);
    row.x = row
        .x
        .clamp(0.0, (screen_w - row.width).max(0.0));
    row.y = row
        .y
        .clamp(0.0, (screen_h - row.height).max(0.0));

    for (index, label) in EXTRA_LABELS.iter().enumerate() {
        let rect = extra_item_rect(row, index);
        push_panel(
            quads,
            rect,
            extra_item_active(index, &state.note),
            state.focused == Some(index),
        );
        labels.push(LabelInfo {
            text: label,
            bounds: rect,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 4.0,
            font_size_override: Some(12.0),
            color_override: Some([242, 244, 249]),
            font_family_override: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_buttons_toggle_without_losing_note() {
        let note = updated_note(0, "note humaine");
        assert!(extra_item_active(0, &note));
        assert_eq!(decode(&note).1, "note humaine");
        let note = updated_note(0, &note);
        assert!(!extra_item_active(0, &note));
        assert_eq!(note, "note humaine");
    }

    #[test]
    fn ambience_buttons_are_mutually_replaced() {
        let note = updated_note(2, "");
        assert!(extra_item_active(2, &note));
        let note = updated_note(3, &note);
        assert!(!extra_item_active(2, &note));
        assert!(extra_item_active(3, &note));
    }
}
