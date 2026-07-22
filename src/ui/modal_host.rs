//! Modal host facade.
//!
//! The established modal implementation remains unchanged in
//! `modal_host_base.rs`. This facade reserves the final modal-overlay pass and
//! input priority for the detection palette, information card and global
//! playhead-position control.

use super::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
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

const OFFSET_BUTTON_W: f32 = 30.0;
const OFFSET_VALUE_W: f32 = 76.0;
const OFFSET_CONTROL_H: f32 = 26.0;
const SETTINGS_FOCUS_COUNT: usize = 7;
const OFFSET_FOCUS_SLOT: usize = 4;

pub struct ModalHost {
    base: base::ModalHost,
    playhead_offset_draft: f32,
    playhead_offset_text: String,
    playhead_offset_focused: bool,
    settings_focus_slot: usize,
}

impl ModalHost {
    pub fn new() -> Self {
        let playhead_offset_draft = crate::config::playhead_offset_percent();
        Self {
            base: base::ModalHost::new(),
            playhead_offset_draft,
            playhead_offset_text: format_offset(playhead_offset_draft),
            playhead_offset_focused: false,
            settings_focus_slot: 0,
        }
    }

    pub fn open_settings(&mut self, fonts: Vec<String>) {
        self.playhead_offset_draft = crate::config::playhead_offset_percent();
        self.refresh_offset_text();
        self.playhead_offset_focused = false;
        self.settings_focus_slot = 0;
        self.base.open_settings(fonts);
    }

    pub fn close_settings(&mut self) {
        self.playhead_offset_focused = false;
        self.settings_focus_slot = 0;
        self.base.close_settings();
    }

    /// Detection popups capture input like a real modal surface, so arrows and
    /// Enter reach them before toolbar sliders or the rythmo workspace.
    pub fn captures_input(&self) -> bool {
        crate::detection_foreground::captures_input() || self.base.captures_input()
    }

    /// Detection is the topmost visual layer and therefore owns the first event
    /// routing opportunity as well.
    pub fn handle_topmost_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if let Some(response) = crate::detection_foreground::handle_modal_event(event) {
            return match response {
                EventResponse::Consumed => Some(ModalOutcome::Consumed),
                EventResponse::Action(action) => Some(ModalOutcome::Action(action)),
                EventResponse::Actions(actions) => Some(ModalOutcome::Actions(actions)),
                EventResponse::Ignored => None,
            };
        }
        self.base.handle_topmost_event(event, screen_w, screen_h)
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<ModalOutcome> {
        if self.base.settings.is_some() {
            if let Some(forward) = settings_focus_direction(event) {
                return Some(self.move_settings_focus(forward, screen_w, screen_h));
            }

            let (_, minus_rect, value_rect, plus_rect) = offset_control_rects(screen_w, screen_h);
            match event {
                UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                    if minus_rect.contains(*x, *y) {
                        self.focus_playhead_offset();
                        self.adjust_playhead_offset(-crate::config::PLAYHEAD_OFFSET_STEP_PERCENT);
                        return Some(self.offset_selection_outcome());
                    }
                    if plus_rect.contains(*x, *y) {
                        self.focus_playhead_offset();
                        self.adjust_playhead_offset(crate::config::PLAYHEAD_OFFSET_STEP_PERCENT);
                        return Some(self.offset_selection_outcome());
                    }
                    if value_rect.contains(*x, *y) {
                        // Clicking the displayed value only focuses it. Resetting
                        // to zero here made an ordinary focus click destructive.
                        self.focus_playhead_offset();
                        return Some(self.offset_focus_outcome());
                    }
                    self.playhead_offset_focused = false;
                    self.settings_focus_slot = 0;
                }
                UiEvent::CursorLeft | UiEvent::CursorDown if self.playhead_offset_focused => {
                    self.adjust_playhead_offset(-crate::config::PLAYHEAD_OFFSET_STEP_PERCENT);
                    return Some(self.offset_selection_outcome());
                }
                UiEvent::CursorRight | UiEvent::CursorUp if self.playhead_offset_focused => {
                    self.adjust_playhead_offset(crate::config::PLAYHEAD_OFFSET_STEP_PERCENT);
                    return Some(self.offset_selection_outcome());
                }
                UiEvent::Home if self.playhead_offset_focused => {
                    self.set_playhead_offset(crate::config::PLAYHEAD_OFFSET_MIN_PERCENT);
                    return Some(self.offset_selection_outcome());
                }
                UiEvent::End if self.playhead_offset_focused => {
                    self.set_playhead_offset(0.0);
                    return Some(self.offset_selection_outcome());
                }
                UiEvent::Activate if self.playhead_offset_focused => {
                    return Some(self.offset_focus_outcome());
                }
                UiEvent::KeyInput { text }
                    if self.playhead_offset_focused
                        && (text == "\r" || text == "\n" || text == " ") =>
                {
                    return Some(self.offset_focus_outcome());
                }
                _ => {}
            }
        }

        let outcome = self.base.handle_event(event, screen_w, screen_h);
        let saves_settings = outcome.as_ref().is_some_and(|outcome| match outcome {
            ModalOutcome::Action(UiAction::SaveSettings { .. }) => true,
            ModalOutcome::Actions(actions) => actions
                .iter()
                .any(|action| matches!(action, UiAction::SaveSettings { .. })),
            ModalOutcome::Consumed | ModalOutcome::Action(_) => false,
        });
        if saves_settings {
            crate::config::set_playhead_offset_percent(self.playhead_offset_draft);
        }
        if self.base.settings.is_none() {
            self.playhead_offset_focused = false;
            self.settings_focus_slot = 0;
        }
        outcome
    }

    pub fn render_base<'a>(
        &'a self,
        modal_quads: &mut Vec<QuadInstance>,
        modal_labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        self.base
            .render_base(modal_quads, modal_labels, screen_w, screen_h);
        if self.base.settings.is_some() {
            self.render_playhead_offset_control(modal_quads, modal_labels, screen_w, screen_h);
        }
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
        self.base.render_top(
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

    fn move_settings_focus(
        &mut self,
        forward: bool,
        screen_w: f32,
        screen_h: f32,
    ) -> ModalOutcome {
        let old_slot = self.settings_focus_slot;
        let new_slot = if forward {
            (old_slot + 1) % SETTINGS_FOCUS_COUNT
        } else {
            (old_slot + SETTINGS_FOCUS_COUNT - 1) % SETTINGS_FOCUS_COUNT
        };
        self.settings_focus_slot = new_slot;
        self.playhead_offset_focused = new_slot == OFFSET_FOCUS_SLOT;

        if new_slot == OFFSET_FOCUS_SLOT {
            return self.offset_focus_outcome();
        }

        // The legacy modal has six focus stops. The added offset stop remains
        // between scroll speed (3) and Save (4); forwarding a synthetic Tab for
        // every other transition keeps both rings synchronized.
        let synthetic = UiEvent::KeyInput {
            text: if forward { "\t" } else { "\u{b}" }.to_string(),
        };
        self.base
            .handle_event(&synthetic, screen_w, screen_h)
            .unwrap_or(ModalOutcome::Consumed)
    }

    fn focus_playhead_offset(&mut self) {
        self.playhead_offset_focused = true;
        self.settings_focus_slot = OFFSET_FOCUS_SLOT;
    }

    fn offset_focus_outcome(&self) -> ModalOutcome {
        ModalOutcome::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Focus {
                label: format!(
                    "{}, {}",
                    playhead_offset_label(),
                    self.playhead_offset_text
                ),
                role: "Slider".to_string(),
            },
        ))
    }

    fn offset_selection_outcome(&self) -> ModalOutcome {
        ModalOutcome::Action(UiAction::Accessibility(
            crate::accessibility::AccessibilityEvent::Selection {
                label: format!(
                    "{}, {}",
                    playhead_offset_label(),
                    self.playhead_offset_text
                ),
            },
        ))
    }

    fn adjust_playhead_offset(&mut self, delta: f32) {
        self.set_playhead_offset(self.playhead_offset_draft + delta);
    }

    fn set_playhead_offset(&mut self, value: f32) {
        self.playhead_offset_draft = value.clamp(
            crate::config::PLAYHEAD_OFFSET_MIN_PERCENT,
            crate::config::PLAYHEAD_OFFSET_MAX_PERCENT,
        );
        self.playhead_offset_draft = (self.playhead_offset_draft
            / crate::config::PLAYHEAD_OFFSET_STEP_PERCENT)
            .round()
            * crate::config::PLAYHEAD_OFFSET_STEP_PERCENT;
        self.refresh_offset_text();
    }

    fn refresh_offset_text(&mut self) {
        self.playhead_offset_text = format_offset(self.playhead_offset_draft);
    }

    fn render_playhead_offset_control<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let (label_rect, minus_rect, value_rect, plus_rect) =
            offset_control_rects(screen_w, screen_h);
        labels.push(LabelInfo {
            text: playhead_offset_label(),
            bounds: label_rect,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([180, 180, 195]),
            font_family_override: None,
        });

        push_control_quad(quads, minus_rect, false);
        push_control_quad(quads, value_rect, self.playhead_offset_focused);
        push_control_quad(quads, plus_rect, false);

        for (text, bounds) in [
            ("−", minus_rect),
            (self.playhead_offset_text.as_str(), value_rect),
            ("+", plus_rect),
        ] {
            labels.push(LabelInfo {
                text,
                bounds,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(12.0),
                color_override: None,
                font_family_override: None,
            });
        }
    }
}

fn settings_focus_direction(event: &UiEvent) -> Option<bool> {
    match event {
        UiEvent::FocusNext => Some(true),
        UiEvent::FocusPrevious => Some(false),
        UiEvent::KeyInput { text } if text == "\t" => Some(true),
        UiEvent::KeyInput { text } if text == "\u{b}" => Some(false),
        _ => None,
    }
}

fn offset_control_rects(screen_w: f32, screen_h: f32) -> (Rect, Rect, Rect, Rect) {
    let card = settings_modal::card_rect(screen_w, screen_h);
    let y = card.y + 126.0 + settings_modal::FONT_LIST_H + 6.0 + 32.0 + 36.0 + 8.0 + 20.0;
    let x = card.x + 222.0;
    let label = Rect {
        x,
        y: y - 20.0,
        width: card.width - (x - card.x) - 20.0,
        height: 18.0,
    };
    let minus = Rect {
        x,
        y,
        width: OFFSET_BUTTON_W,
        height: OFFSET_CONTROL_H,
    };
    let value = Rect {
        x: minus.x + minus.width + 4.0,
        y,
        width: OFFSET_VALUE_W,
        height: OFFSET_CONTROL_H,
    };
    let plus = Rect {
        x: value.x + value.width + 4.0,
        y,
        width: OFFSET_BUTTON_W,
        height: OFFSET_CONTROL_H,
    };
    (label, minus, value, plus)
}

fn push_control_quad(quads: &mut Vec<QuadInstance>, rect: Rect, focused: bool) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: if focused {
            [0.27, 0.26, 0.48, 1.0]
        } else {
            [0.12, 0.12, 0.15, 1.0]
        },
        color_bottom: if focused {
            [0.21, 0.20, 0.40, 1.0]
        } else {
            [0.09, 0.09, 0.12, 1.0]
        },
        border_color: if focused {
            [0.50, 0.45, 0.85, 0.95]
        } else {
            [0.32, 0.32, 0.40, 0.75]
        },
        border_width: if focused { 1.5 } else { 1.0 },
        border_radius: 5.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn format_offset(value: f32) -> String {
    if value.abs() < f32::EPSILON {
        "0,0 %".to_string()
    } else {
        format!("{value:.1} %").replace('.', ",")
    }
}

fn playhead_offset_label() -> &'static str {
    match crate::config::language_or_default().as_str() {
        "en-us" => "Playhead position",
        "es-es" => "Posición de la línea de lectura",
        _ => "Position de la ligne de lecture",
    }
}

impl Deref for ModalHost {
    type Target = base::ModalHost;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for ModalHost {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl Default for ModalHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{OFFSET_FOCUS_SLOT, SETTINGS_FOCUS_COUNT};

    #[test]
    fn offset_focus_sits_between_scroll_speed_and_save() {
        assert_eq!(OFFSET_FOCUS_SLOT, 4);
        assert_eq!(SETTINGS_FOCUS_COUNT, 7);
    }
}
