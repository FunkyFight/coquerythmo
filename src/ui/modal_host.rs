//! Modal host facade.
//!
//! The established modal implementation remains unchanged in
//! `modal_host_base.rs`. This facade reserves the final modal-overlay pass and
//! input priority for the detection palette and information card.

use super::primitives::{EventResponse, LabelInfo, QuadInstance, UiEvent};
use super::{
    comic_dubs_settings_modal, connect_modal, export_modal, invitation_modal, microphone_modal,
    pricing_license_modal, pricing_page, pricing_plan_modal, primitives, project_settings_modal,
    proxy_error_modal, proxy_modal, rename_character_modal, save_prompt_modal, server_browser,
    settings_modal, voice_actor_modal, whats_new_modal,
};
use std::ops::{Deref, DerefMut};

#[path = "modal_host_base.rs"]
pub mod base;

pub use base::ModalOutcome;

pub struct ModalHost(base::ModalHost);

impl ModalHost {
    pub fn new() -> Self {
        Self(base::ModalHost::new())
    }

    /// Detection popups capture input like a real modal surface, so arrows and
    /// Enter reach them before toolbar sliders or the rythmo workspace.
    pub fn captures_input(&self) -> bool {
        crate::detection_foreground::captures_input() || self.0.captures_input()
    }

    /// Detection is the topmost visual layer and therefore owns the first event
    /// routing opportunity as well.
    pub fn handle_topmost_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if self.0.microphone.is_some() {
            return self.0.handle_topmost_event(event, screen_w, screen_h);
        }
        if let Some(response) = crate::detection_foreground::handle_modal_event(event) {
            return match response {
                EventResponse::Consumed => Some(ModalOutcome::Consumed),
                EventResponse::Action(action) => Some(ModalOutcome::Action(action)),
                EventResponse::Actions(actions) => Some(ModalOutcome::Actions(actions)),
                EventResponse::Ignored => None,
            };
        }
        self.0.handle_topmost_event(event, screen_w, screen_h)
    }

    /// Render established modal overlays, then append the detector surface to
    /// the dedicated topmost layer.
    pub fn render_top<'a>(
        &'a self,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        modal_overlay_quads: &mut Vec<QuadInstance>,
        modal_overlay_labels: &mut Vec<LabelInfo<'a>>,
        topmost_quads: &mut Vec<QuadInstance>,
        topmost_labels: &mut Vec<LabelInfo<'a>>,
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
            topmost_quads,
            topmost_labels,
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
