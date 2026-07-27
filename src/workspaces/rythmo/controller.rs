//! Rythmo controller facade for detection foreground and accessibility.

use super::*;

#[path = "controller_base.rs"]
mod base;

pub(crate) use base::RythmoCtx;
pub use base::RythmoInteractionMode;

pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
    active_mode: ToolMode,
    brush_color: [f32; 4],
    brush_radius_frac: f32,
    erasing: bool,
    interaction_mode: RythmoInteractionMode,
) -> EventResponse {
    let had_information_card = state.detection_menu.is_some() && state.detection_hover.is_none();
    let response = base::handle_rythmo_event(
        event,
        zone,
        project,
        render_index,
        current_frame,
        karaoke_preview,
        fps,
        state,
        active_mode,
        brush_color,
        brush_radius_frac,
        erasing,
        interaction_mode,
    );

    if interaction_mode == RythmoInteractionMode::ReadOnly {
        crate::detection_foreground::clear();
        return response;
    }

    crate::detection_foreground::sync_from_state(project, state, *zone, current_frame, event);

    let information_card_open = state.detection_menu.is_some() && state.detection_hover.is_none();
    if !had_information_card && information_card_open {
        if let Some(label) =
            crate::detection_foreground::selected_info_accessibility_label(project, state)
        {
            return EventResponse::Action(UiAction::Accessibility(
                crate::accessibility::AccessibilityEvent::Opened { label },
            ));
        }
    }

    response
}
