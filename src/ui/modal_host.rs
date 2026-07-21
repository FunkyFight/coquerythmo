//! Modal host facade.
//!
//! The established modal implementation remains unchanged in
//! `modal_host_base.rs`. This facade only reserves the final modal-overlay pass
//! for detection signs and popovers, guaranteeing that the Alt+D palette and
//! information card render above every ordinary UI layer.

use super::{
    connect_modal, export_modal, file_explorer, language_modal, pricing_license_modal,
    pricing_page, pricing_plan_modal, primitives, project_settings_modal, proxy_error_modal,
    proxy_modal, rename_character_modal, save_prompt_modal, server_browser, settings_modal,
    voice_actor_modal, whats_new_modal,
};
use super::primitives::{LabelInfo, QuadInstance};
use std::ops::{Deref, DerefMut};

#[path = "modal_host_base.rs"]
pub mod base;

pub use base::ModalOutcome;

pub struct ModalHost(base::ModalHost);

impl ModalHost {
    pub fn new() -> Self {
        Self(base::ModalHost::new())
    }

    /// Render every established top-level modal first, then append the detector
    /// surface to the final overlay arrays. Its backgrounds, mouth image,
    /// glyphs and labels therefore share one coherent highest z-layer.
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
