//! Highest foreground surface for the text-emotion context menu and palette.

use crate::accessibility::AccessibilityEvent;
use crate::project::Project;
use crate::text_emotion::{self, TextEmotion};
use crate::ui::focus::{AccessibleNode, AccessibleRole};
use crate::ui::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{RythmoState, Selection};
use std::sync::{Mutex, MutexGuard, OnceLock};
use unicode_segmentation::UnicodeSegmentation;

const WIDTH: f32 = 260.0;
const ITEM_HEIGHT: f32 = 30.0;
const PADDING: f32 = 5.0;
const TITLE_HEIGHT: f32 = 28.0;
const PARENT_HEIGHT: f32 = ITEM_HEIGHT + PADDING * 2.0;
const PALETTE_HEIGHT: f32 = TITLE_HEIGHT + ITEM_HEIGHT * 10.0 + PADDING * 2.0;
const REMOVE_LABEL: &str = "Retirer l’émotion";
const PARENT_LABEL: &str = "Émotion du texte";

#[derive(Clone, Debug)]
struct Target {
    line_id: u64,
    source_text: String,
    start_grapheme: usize,
    end_grapheme: usize,
}

#[derive(Clone, Debug)]
enum Popup {
    None,
    Parent { rect: Rect, target: Target },
    Palette { rect: Rect, target: Target, selected: usize },
}

impl Default for Popup {
    fn default() -> Self {
        Self::None
    }
}

fn popup() -> &'static Mutex<Popup> {
    static POPUP: OnceLock<Mutex<Popup>> = OnceLock::new();
    POPUP.get_or_init(|| Mutex::new(Popup::None))
}

fn lock_popup() -> MutexGuard<'static, Popup> {
    popup().lock().unwrap_or_else(|error| error.into_inner())
}

pub fn captures_input() -> bool {
    !matches!(*lock_popup(), Popup::None)
}

pub fn clear() {
    *lock_popup() = Popup::None;
}

pub fn selected_index() -> Option<usize> {
    match &*lock_popup() {
        Popup::Palette { selected, .. } => Some(*selected),
        Popup::Parent { .. } => Some(0),
        Popup::None => None,
    }
}

pub fn accessible_nodes() -> Vec<AccessibleNode> {
    match &*lock_popup() {
        Popup::None => Vec::new(),
        Popup::Parent { .. } => vec![AccessibleNode::focusable(
            "text-emotion.parent",
            AccessibleRole::MenuItem,
            "Émotion du texte, sous-menu",
        )
        .with_selected(Some(true))],
        Popup::Palette { selected, .. } => palette_labels()
            .iter()
            .enumerate()
            .map(|(index, label)| {
                AccessibleNode::focusable(
                    format!("text-emotion.{index}"),
                    AccessibleRole::MenuItem,
                    *label,
                )
                .with_selected(Some(index == *selected))
            })
            .collect(),
    }
}

pub fn open_keyboard(
    project: &Project,
    state: &RythmoState,
    x: f32,
    y: f32,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    let Some(target) = target_from_selection(project, state) else {
        return false;
    };
    *lock_popup() = Popup::Palette {
        rect: clamped_rect(x + 8.0, y + 8.0, WIDTH, PALETTE_HEIGHT, screen_w, screen_h),
        target,
        selected: 0,
    };
    true
}

pub fn open_context_parent(
    project: &Project,
    state: &RythmoState,
    line_id: u64,
    x: f32,
    y: f32,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    let Some(target) = target_for_line(project, state, line_id) else {
        return false;
    };
    *lock_popup() = Popup::Parent {
        rect: clamped_rect(x, y, WIDTH, PARENT_HEIGHT, screen_w, screen_h),
        target,
    };
    true
}

pub fn target_is_available(project: &Project, state: &RythmoState) -> bool {
    target_from_selection(project, state).is_some()
}

fn target_from_selection(project: &Project, state: &RythmoState) -> Option<Target> {
    if let Some(line_id) = state.editing_line {
        return target_for_line(project, state, line_id);
    }
    match state.selected.as_ref()? {
        Selection::Line(line_id) => target_for_line(project, state, *line_id),
        Selection::Lines(line_ids) => line_ids
            .iter()
            .find_map(|line_id| target_for_line(project, state, *line_id)),
        _ => None,
    }
}

fn target_for_line(project: &Project, state: &RythmoState, line_id: u64) -> Option<Target> {
    let line = project.get_line(line_id)?;
    if !line.kind.is_dialogue() || line.karaoke || line.text.is_empty() {
        return None;
    }
    let grapheme_count = line.text.graphemes(true).count();
    let (start_grapheme, end_grapheme) = if state.editing_line == Some(line_id) {
        state
            .line_input
            .selection_range()
            .filter(|(start, end)| start < end)
            .map(|(start, end)| {
                (
                    char_boundary_to_grapheme(&line.text, start),
                    char_boundary_to_grapheme(&line.text, end),
                )
            })
            .unwrap_or((0, grapheme_count))
    } else {
        (0, grapheme_count)
    };
    (start_grapheme < end_grapheme).then(|| Target {
        line_id,
        source_text: line.text.clone(),
        start_grapheme,
        end_grapheme,
    })
}

fn char_boundary_to_grapheme(text: &str, char_boundary: usize) -> usize {
    text.grapheme_indices(true)
        .take_while(|(byte, _)| text[..*byte].chars().count() < char_boundary)
        .count()
}

pub fn handle_modal_event(event: &UiEvent) -> Option<EventResponse> {
    let current = lock_popup().clone();
    match current {
        Popup::None => None,
        Popup::Parent { rect, target } => match event {
            UiEvent::Activate | UiEvent::CursorRight => {
                open_palette_from_parent(rect, target);
                Some(opened_response())
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                open_palette_from_parent(rect, target);
                Some(opened_response())
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                clear();
                Some(closed_response())
            }
            UiEvent::MouseMove { .. } => Some(EventResponse::Consumed),
            UiEvent::MousePress { x, y } if rect.contains(*x, *y) => {
                open_palette_from_parent(rect, target);
                Some(opened_response())
            }
            UiEvent::MousePress { .. } => {
                clear();
                Some(closed_response())
            }
            _ => Some(EventResponse::Consumed),
        },
        Popup::Palette {
            rect,
            target,
            selected,
        } => match event {
            UiEvent::CursorUp => Some(move_selection(selected, -1)),
            UiEvent::CursorDown => Some(move_selection(selected, 1)),
            UiEvent::Home => Some(set_selection(0)),
            UiEvent::End => Some(set_selection(9)),
            UiEvent::Activate => Some(activate(target, selected)),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                Some(activate(target, selected))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                clear();
                Some(closed_response())
            }
            UiEvent::MouseMove { x, y } => {
                if let Some(index) = palette_item_at(rect, *x, *y) {
                    let _ = set_selection(index);
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } => {
                if let Some(index) = palette_item_at(rect, *x, *y) {
                    return Some(activate(target, index));
                }
                if !rect.contains(*x, *y) {
                    clear();
                    return Some(closed_response());
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseRelease { .. }
            | UiEvent::Scroll { .. }
            | UiEvent::DoubleClick { .. }
            | UiEvent::CtrlClick { .. }
            | UiEvent::ShiftMousePress { .. }
            | UiEvent::MiddlePress { .. }
            | UiEvent::MiddleRelease { .. }
            | UiEvent::ContextMenu { .. } => Some(EventResponse::Consumed),
            _ => Some(EventResponse::Consumed),
        },
    }
}

fn open_palette_from_parent(rect: Rect, target: Target) {
    *lock_popup() = Popup::Palette {
        rect: Rect {
            height: PALETTE_HEIGHT,
            ..rect
        },
        target,
        selected: 0,
    };
}

fn moved_index(current: usize, direction: i32) -> usize {
    (current as i32 + direction).rem_euclid(10) as usize
}

fn move_selection(current: usize, direction: i32) -> EventResponse {
    set_selection(moved_index(current, direction))
}

fn set_selection(index: usize) -> EventResponse {
    if let Popup::Palette { selected, .. } = &mut *lock_popup() {
        *selected = index.min(9);
    }
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Selection {
        label: palette_labels()[index.min(9)].to_string(),
    }))
}

fn activate(target: Target, index: usize) -> EventResponse {
    let emotion = index
        .checked_sub(1)
        .and_then(|index| TextEmotion::ALL.get(index).copied());
    let label = emotion.map(TextEmotion::label).unwrap_or(REMOVE_LABEL);
    let changed = text_emotion::apply_range(
        target.line_id,
        &target.source_text,
        target.start_grapheme,
        target.end_grapheme,
        emotion,
    );
    clear();
    if !changed {
        return EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Selection {
            label: format!("{label}, déjà appliqué"),
        }));
    }
    EventResponse::Actions(vec![
        // Reuse the established text command so project revision, lint caches,
        // autosave and collaboration invalidation all observe the change.
        UiAction::UpdateLineText {
            id: target.line_id,
            text: target.source_text,
        },
        UiAction::Accessibility(AccessibilityEvent::Success {
            message: format!("Émotion du texte : {label}"),
        }),
        UiAction::Accessibility(AccessibilityEvent::Closed {
            label: "Menu des émotions du texte".to_string(),
        }),
    ])
}

fn opened_response() -> EventResponse {
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Opened {
        label: "Menu des émotions du texte. Utilisez les flèches haut et bas puis Entrée."
            .to_string(),
    }))
}

fn closed_response() -> EventResponse {
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Closed {
        label: "Menu des émotions du texte".to_string(),
    }))
}

fn palette_labels() -> [&'static str; 10] {
    [
        REMOVE_LABEL,
        TextEmotion::Pendulum.label(),
        TextEmotion::Swing.label(),
        TextEmotion::Yay.label(),
        TextEmotion::Bounce.label(),
        TextEmotion::Slide.label(),
        TextEmotion::Oscillation.label(),
        TextEmotion::Wave.label(),
        TextEmotion::Shake.label(),
        TextEmotion::Wiggle.label(),
    ]
}

fn clamped_rect(x: f32, y: f32, width: f32, height: f32, sw: f32, sh: f32) -> Rect {
    let (x, y) = crate::ui::context_menu::clamped_origin(x, y, width, height, sw, sh);
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn palette_item_at(rect: Rect, x: f32, y: f32) -> Option<usize> {
    if x < rect.x || x > rect.x + rect.width {
        return None;
    }
    let local = y - rect.y - PADDING - TITLE_HEIGHT;
    if local < 0.0 {
        return None;
    }
    let index = (local / ITEM_HEIGHT).floor() as usize;
    (index < 10).then_some(index)
}

pub fn render<'a>(quads: &mut Vec<QuadInstance>, labels: &mut Vec<LabelInfo<'a>>) {
    match &*lock_popup() {
        Popup::None => {}
        Popup::Parent { rect, .. } => {
            crate::ui::context_menu::render_panel(quads, *rect);
            crate::ui::context_menu::render_item(
                quads,
                labels,
                Rect {
                    x: rect.x,
                    y: rect.y + PADDING,
                    width: rect.width,
                    height: ITEM_HEIGHT,
                },
                PARENT_LABEL,
                true,
                true,
                14.0,
            );
        }
        Popup::Palette { rect, selected, .. } => {
            crate::ui::context_menu::render_panel(quads, *rect);
            labels.push(LabelInfo {
                text: PARENT_LABEL,
                bounds: Rect {
                    x: rect.x + 10.0,
                    y: rect.y + PADDING,
                    width: rect.width - 20.0,
                    height: TITLE_HEIGHT,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(13.0),
                color_override: Some([170, 170, 184]),
                font_family_override: None,
            });
            for (index, label) in palette_labels().iter().enumerate() {
                crate::ui::context_menu::render_item(
                    quads,
                    labels,
                    Rect {
                        x: rect.x,
                        y: rect.y + PADDING + TITLE_HEIGHT + index as f32 * ITEM_HEIGHT,
                        width: rect.width,
                        height: ITEM_HEIGHT,
                    },
                    label,
                    index == *selected,
                    false,
                    14.0,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_palette_item_is_always_remove() {
        assert_eq!(palette_labels()[0], REMOVE_LABEL);
        assert_eq!(palette_labels().len(), TextEmotion::ALL.len() + 1);
    }

    #[test]
    fn navigation_wraps() {
        assert_eq!(moved_index(0, -1), 9);
        assert_eq!(moved_index(9, 1), 0);
    }
}
