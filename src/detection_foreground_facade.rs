//! Public detector foreground facade.
//!
//! The detector palette and information card remain unchanged, but the
//! hover-following plus control is intentionally suppressed. Keyboard access
//! through the existing semantic shortcut remains available.

use crate::ui::primitives::{LabelInfo, QuadInstance};

#[path = "detection_foreground.rs"]
mod legacy;

pub use legacy::{
    activate_palette, captures_input, clear, handle_modal_event,
    selected_info_accessibility_label, sync_from_state,
};
pub(crate) use legacy::{reconcile_legacy_menu, suppressed_popup, PopupKind};

pub fn append_foreground<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let quad_start = quads.len();
    let label_start = labels.len();
    legacy::append_foreground(quads, labels, screen_w, screen_h);

    // With no popup open, the legacy foreground only emits the hover plus.
    // Discard that output while preserving the palette and information card.
    if legacy::suppressed_popup().is_none() {
        quads.truncate(quad_start);
        labels.truncate(label_start);
    }
}
