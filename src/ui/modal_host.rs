//! Modal host facade.
//!
//! The established modal implementation remains unchanged in
//! `modal_host_base.rs`. Detection popups keep the final visual layer, while the
//! passive information card does not block semantic workspace commands.

use super::primitives::{EventResponse, LabelInfo, QuadInstance, Rect, UiEvent};
use super::{
    connect_modal, export_modal, file_explorer, language_modal, pricing_license_modal,
    pricing_page, pricing_plan_modal, primitives, project_settings_modal, proxy_error_modal,
    proxy_modal, rename_character_modal, save_prompt_modal, server_browser, settings_modal,
    voice_actor_modal, whats_new_modal,
};
use std::ops::{Deref, DerefMut};

#[path = "modal_host_base.rs"]
pub mod base;

pub use base::ModalOutcome;

pub struct ModalHost(base::ModalHost);

fn active_detection_popup() -> Option<(crate::detection_foreground::PopupKind, Rect)> {
    crate::detection_foreground::captures_input()
        .then(crate::detection_foreground::suppressed_popup)
        .flatten()
}

fn detection_popup_captures_input(
    popup: Option<(crate::detection_foreground::PopupKind, Rect)>,
) -> bool {
    matches!(
        popup,
        Some((crate::detection_foreground::PopupKind::Palette, _))
    )
}

fn should_route_detection_event(
    event: &UiEvent,
    popup: Option<(crate::detection_foreground::PopupKind, Rect)>,
) -> bool {
    match popup {
        Some((crate::detection_foreground::PopupKind::Palette, _)) => true,
        Some((crate::detection_foreground::PopupKind::Info, visual)) => match event {
            UiEvent::KeyInput { text } if text == "\x1b" => true,
            UiEvent::MousePress { x, y } => !visual.contains(*x, *y),
            _ => false,
        },
        None => false,
    }
}

impl ModalHost {
    pub fn new() -> Self {
        Self(base::ModalHost::new())
    }

    pub fn captures_input(&self) -> bool {
        detection_popup_captures_input(active_detection_popup()) || self.0.captures_input()
    }

    pub fn handle_topmost_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if should_route_detection_event(event, active_detection_popup()) {
            if let Some(response) = crate::detection_foreground::handle_modal_event(event) {
                return match response {
                    EventResponse::Consumed => Some(ModalOutcome::Consumed),
                    EventResponse::Action(action) => Some(ModalOutcome::Action(action)),
                    EventResponse::Actions(actions) => Some(ModalOutcome::Actions(actions)),
                    EventResponse::Ignored => None,
                };
            }
        }
        self.0.handle_topmost_event(event, screen_w, screen_h)
    }

    pub fn render_top<'a>(
        &'a self,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        modal_overlay_quads: &mut Vec<QuadInstance>,
        modal_overlay_labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        self.0.render_top(
            modal_quads,
            modal_labels,
            modal_overlay_quads,
            modal_overlay_labels,
            screen_w,
            screen_h,
        );
        crate::detection_foreground::append_foreground(
            modal_overlay_quads,
            modal_overlay_labels,
            screen_w,
            screen_h,
        );
    }
}

impl Deref for ModalHost {
    type Target = base::ModalHost;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ModalHost {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup(kind: crate::detection_foreground::PopupKind) -> Option<(
        crate::detection_foreground::PopupKind,
        Rect,
    )> {
        Some((
            kind,
            Rect {
                x: 10.0,
                y: 10.0,
                width: 100.0,
                height: 50.0,
            },
        ))
    }

    #[test]
    fn information_card_does_not_capture_delete() {
        let info = popup(crate::detection_foreground::PopupKind::Info);
        assert!(!detection_popup_captures_input(info));
        assert!(!should_route_detection_event(&UiEvent::Delete, info));
    }

    #[test]
    fn information_card_still_handles_escape_and_outside_click() {
        let info = popup(crate::detection_foreground::PopupKind::Info);
        assert!(should_route_detection_event(
            &UiEvent::KeyInput {
                text: "\x1b".to_string(),
            },
            info,
        ));
        assert!(should_route_detection_event(
            &UiEvent::MousePress { x: 0.0, y: 0.0 },
            info,
        ));
    }

    #[test]
    fn palette_keeps_modal_keyboard_capture() {
        let palette = popup(crate::detection_foreground::PopupKind::Palette);
        assert!(detection_popup_captures_input(palette));
        assert!(should_route_detection_event(&UiEvent::Delete, palette));
    }
}
