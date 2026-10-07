//! Context menu opened by a right click on a synchronization point.

use super::*;
use crate::detection::DetectionAddress;

const SYNC_MENU_W: f32 = 260.0;
const SYNC_MENU_ITEM_COUNT: usize = 2;
const SYNC_MENU_TOGGLE: usize = 0;
const SYNC_MENU_DELETE: usize = 1;

pub struct SyncPointMenu {
    pub address: DetectionAddress,
    pub x: f32,
    pub y: f32,
    /// Whether the point's character currently stays with the part on its left.
    pub includes_character: bool,
    pub hover: Option<usize>,
}

fn sync_menu_label(menu: &SyncPointMenu, index: usize) -> &'static str {
    match index {
        SYNC_MENU_TOGGLE if menu.includes_character => t("context.sync_point.exclude_character"),
        SYNC_MENU_TOGGLE => t("context.sync_point.include_character"),
        _ => t("context.sync_point.delete"),
    }
}

fn sync_menu_rect(menu: &SyncPointMenu, screen_w: f32, screen_h: f32) -> Rect {
    let height = MENU_ITEM_H * SYNC_MENU_ITEM_COUNT as f32;
    let (x, y) = clamped_menu_origin(menu.x, menu.y, SYNC_MENU_W, height, screen_w, screen_h);
    Rect {
        x,
        y,
        width: SYNC_MENU_W,
        height,
    }
}

fn sync_menu_item_at(
    menu: &SyncPointMenu,
    screen_w: f32,
    screen_h: f32,
    x: f32,
    y: f32,
) -> Option<usize> {
    let rect = sync_menu_rect(menu, screen_w, screen_h);
    rect.contains(x, y)
        .then(|| (((y - rect.y) / MENU_ITEM_H).floor() as usize).min(SYNC_MENU_ITEM_COUNT - 1))
}

/// Opens the menu when the right click lands on a synchronization point.
pub(crate) fn open_sync_point_menu(
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    fps: f64,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> Option<EventResponse> {
    let (address, includes_character) =
        hit_sync_point(project, state, x, y, current_frame, zone, fps)?;
    let menu = SyncPointMenu {
        address,
        x,
        y,
        includes_character,
        hover: Some(SYNC_MENU_TOGGLE),
    };
    let label = format!(
        "{}, {}",
        sync_menu_label(&menu, SYNC_MENU_TOGGLE),
        sync_menu_label(&menu, SYNC_MENU_DELETE)
    );
    state.context_menu = None;
    state.sync_point_menu = Some(menu);
    state.selected = Some(Selection::Detection(address));
    state.dragging = None;
    Some(EventResponse::Action(UiAction::Accessibility(
        crate::accessibility::AccessibilityEvent::Focus {
            label,
            role: "menu".to_string(),
        },
    )))
}

fn activate_sync_menu_item(state: &mut RythmoState, index: usize) -> EventResponse {
    let Some(menu) = state.sync_point_menu.take() else {
        return EventResponse::Consumed;
    };
    match index {
        SYNC_MENU_TOGGLE => {
            state.selected = Some(Selection::Detection(menu.address));
            EventResponse::Action(UiAction::ToggleSelectedSyncAffinity)
        }
        SYNC_MENU_DELETE => EventResponse::Action(UiAction::DeleteDetection {
            address: menu.address,
        }),
        _ => EventResponse::Consumed,
    }
}

/// Handles events while the synchronization point menu is open. Returns `None`
/// when the menu is closed or lets the event through (a new right click).
pub(crate) fn handle_sync_point_menu_event(
    event: &UiEvent,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    let menu = state.sync_point_menu.as_mut()?;
    let response = match event {
        UiEvent::ContextMenu { .. } => {
            state.sync_point_menu = None;
            return None;
        }
        UiEvent::MouseMove { x, y } => {
            menu.hover = sync_menu_item_at(menu, screen_w, screen_h, *x, *y);
            EventResponse::Consumed
        }
        UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
            match sync_menu_item_at(menu, screen_w, screen_h, *x, *y) {
                Some(index) => activate_sync_menu_item(state, index),
                None => {
                    state.sync_point_menu = None;
                    EventResponse::Consumed
                }
            }
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            state.sync_point_menu = None;
            EventResponse::Consumed
        }
        UiEvent::CursorUp | UiEvent::CursorDown => {
            let direction = if matches!(event, UiEvent::CursorDown) {
                1
            } else {
                -1
            };
            let current = menu.hover.unwrap_or(0) as i32;
            let next = (current + direction).rem_euclid(SYNC_MENU_ITEM_COUNT as i32) as usize;
            menu.hover = Some(next);
            EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Selection {
                    label: sync_menu_label(menu, next).to_string(),
                },
            ))
        }
        UiEvent::Activate => match menu.hover {
            Some(index) => activate_sync_menu_item(state, index),
            None => EventResponse::Consumed,
        },
        _ => EventResponse::Consumed,
    };
    Some(response)
}

pub(crate) fn render_sync_point_menu<'a>(
    screen_w: f32,
    screen_h: f32,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let Some(menu) = &state.sync_point_menu else {
        return;
    };
    let rect = sync_menu_rect(menu, screen_w, screen_h);
    render_menu_panel(quads, rect);
    for index in 0..SYNC_MENU_ITEM_COUNT {
        render_menu_item(
            quads,
            labels,
            Rect {
                x: rect.x,
                y: rect.y + MENU_ITEM_H * index as f32,
                width: rect.width,
                height: MENU_ITEM_H,
            },
            sync_menu_label(menu, index),
            menu.hover == Some(index),
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::DetectionCueId;

    fn open_menu(state: &mut RythmoState, includes_character: bool) -> DetectionAddress {
        let address = DetectionAddress {
            line_id: 7,
            detection_id: DetectionCueId(3),
        };
        state.sync_point_menu = Some(SyncPointMenu {
            address,
            x: 100.0,
            y: 100.0,
            includes_character,
            hover: Some(SYNC_MENU_TOGGLE),
        });
        address
    }

    #[test]
    fn toggle_item_selects_the_point_and_toggles_its_affinity() {
        let mut state = RythmoState::default();
        let address = open_menu(&mut state, true);
        let response = handle_sync_point_menu_event(&UiEvent::Activate, 1920.0, 1080.0, &mut state);
        assert_eq!(
            response,
            Some(EventResponse::Action(UiAction::ToggleSelectedSyncAffinity))
        );
        assert_eq!(state.selected, Some(Selection::Detection(address)));
        assert!(state.sync_point_menu.is_none());
    }

    #[test]
    fn keyboard_reaches_delete_item() {
        let mut state = RythmoState::default();
        let address = open_menu(&mut state, false);
        handle_sync_point_menu_event(&UiEvent::CursorDown, 1920.0, 1080.0, &mut state);
        let response = handle_sync_point_menu_event(&UiEvent::Activate, 1920.0, 1080.0, &mut state);
        assert_eq!(
            response,
            Some(EventResponse::Action(UiAction::DeleteDetection { address }))
        );
    }

    #[test]
    fn label_follows_current_side_of_the_character() {
        let mut state = RythmoState::default();
        open_menu(&mut state, true);
        let menu = state.sync_point_menu.as_ref().unwrap();
        assert_eq!(
            sync_menu_label(menu, SYNC_MENU_TOGGLE),
            t("context.sync_point.exclude_character")
        );
    }

    #[test]
    fn click_outside_closes_without_action() {
        let mut state = RythmoState::default();
        open_menu(&mut state, true);
        let response = handle_sync_point_menu_event(
            &UiEvent::MousePress { x: 5.0, y: 5.0 },
            1920.0,
            1080.0,
            &mut state,
        );
        assert_eq!(response, Some(EventResponse::Consumed));
        assert!(state.sync_point_menu.is_none());
    }
}
