//! Voice actor editor modal.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::text_input::{self, TextInputAction, TextInputMetrics, TextInputState};

use crate::i18n::t;

const CARD_W: f32 = 520.0;
const CARD_H: f32 = 260.0;
const FIELD_FONT_SIZE: f32 = 12.0;
const FIELD_PADDING_X: f32 = 10.0;

#[derive(Clone, Copy, PartialEq)]
enum ActiveField {
    Name,
    IconPath,
}

pub struct VoiceActorModal {
    pub name: String,
    pub icon_path: String,
    active_field: Option<ActiveField>,
    name_input: TextInputState,
    icon_input: TextInputState,
    selecting_field: Option<ActiveField>,
}

impl Default for VoiceActorModal {
    fn default() -> Self {
        Self::new()
    }
}

pub enum VoiceActorModalResult {
    Consumed,
    Close,
    PickIcon,
    Clipboard(String),
    Create { name: String, icon_path: String },
}

impl VoiceActorModal {
    pub fn new() -> Self {
        let mut modal = Self {
            name: String::new(),
            icon_path: String::new(),
            active_field: Some(ActiveField::Name),
            name_input: TextInputState::new(),
            icon_input: TextInputState::new(),
            selecting_field: None,
        };
        modal.name_input.activate("");
        modal
    }

    pub fn set_icon_path(&mut self, path: impl Into<String>) {
        self.icon_path = path.into();
        self.activate_field(ActiveField::IconPath);
    }

    fn card_rect(screen_w: f32, screen_h: f32) -> Rect {
        Rect {
            x: (screen_w - CARD_W) / 2.0,
            y: (screen_h - CARD_H) / 2.0,
            width: CARD_W,
            height: CARD_H,
        }
    }

    fn name_rect(card: Rect) -> Rect {
        Rect {
            x: card.x + 24.0,
            y: card.y + 78.0,
            width: card.width - 48.0,
            height: 32.0,
        }
    }

    fn icon_rects(card: Rect) -> (Rect, Rect) {
        let browse_w = 96.0;
        let gap = 8.0;
        let field = Rect {
            x: card.x + 24.0,
            y: card.y + 140.0,
            width: card.width - 48.0 - browse_w - gap,
            height: 32.0,
        };
        let browse = Rect {
            x: field.x + field.width + gap,
            y: field.y,
            width: browse_w,
            height: 32.0,
        };
        (field, browse)
    }

    fn button_rects(card: Rect) -> (Rect, Rect) {
        let y = card.y + CARD_H - 52.0;
        let cancel = Rect {
            x: card.x + card.width - 232.0,
            y,
            width: 92.0,
            height: 36.0,
        };
        let create = Rect {
            x: card.x + card.width - 128.0,
            y,
            width: 104.0,
            height: 36.0,
        };
        (cancel, create)
    }

    fn create_result(&self) -> VoiceActorModalResult {
        if self.name.trim().is_empty() {
            VoiceActorModalResult::Consumed
        } else {
            VoiceActorModalResult::Create {
                name: self.name.clone(),
                icon_path: self.icon_path.clone(),
            }
        }
    }

    fn activate_field(&mut self, field: ActiveField) {
        self.active_field = Some(field);
        match field {
            ActiveField::Name => {
                self.icon_input.deactivate();
                self.name_input.activate(&self.name);
            }
            ActiveField::IconPath => {
                self.name_input.deactivate();
                self.icon_input.activate(&self.icon_path);
            }
        }
    }

    fn deactivate_fields(&mut self) {
        self.active_field = None;
        self.selecting_field = None;
        self.name_input.deactivate();
        self.icon_input.deactivate();
    }

    fn field_for_point(card: Rect, x: f32, y: f32) -> Option<ActiveField> {
        if Self::name_rect(card).contains(x, y) {
            Some(ActiveField::Name)
        } else if Self::icon_rects(card).0.contains(x, y) {
            Some(ActiveField::IconPath)
        } else {
            None
        }
    }

    fn field_rect(card: Rect, field: ActiveField) -> Rect {
        match field {
            ActiveField::Name => Self::name_rect(card),
            ActiveField::IconPath => Self::icon_rects(card).0,
        }
    }

    fn cursor_pos_from_x(value: &str, field: Rect, x: f32) -> usize {
        text_input::cursor_pos_from_x(value, field, x, input_metrics())
    }

    fn start_mouse_selection(&mut self, field: ActiveField, card: Rect, x: f32, double: bool) {
        self.activate_field(field);
        let rect = Self::field_rect(card, field);
        match field {
            ActiveField::Name => {
                if double {
                    self.name_input.select_all(&self.name);
                } else {
                    let pos = Self::cursor_pos_from_x(&self.name, rect, x);
                    self.name_input.start_selection(pos);
                    self.selecting_field = Some(field);
                }
            }
            ActiveField::IconPath => {
                if double {
                    self.icon_input.select_all(&self.icon_path);
                } else {
                    let pos = Self::cursor_pos_from_x(&self.icon_path, rect, x);
                    self.icon_input.start_selection(pos);
                    self.selecting_field = Some(field);
                }
            }
        }
    }

    fn update_mouse_selection(&mut self, card: Rect, x: f32) -> bool {
        let Some(field) = self.selecting_field else {
            return false;
        };
        let rect = Self::field_rect(card, field);
        match field {
            ActiveField::Name => {
                let pos = Self::cursor_pos_from_x(&self.name, rect, x);
                self.name_input.update_selection(pos);
            }
            ActiveField::IconPath => {
                let pos = Self::cursor_pos_from_x(&self.icon_path, rect, x);
                self.icon_input.update_selection(pos);
            }
        }
        true
    }

    fn sanitize_change(value: String, max_len: usize) -> String {
        let mut out: String = value.chars().filter(|c| !c.is_control()).collect();
        if out.len() > max_len {
            out.truncate(max_len);
        }
        out
    }

    fn handle_active_key(&mut self, text: &str) {
        match self.active_field {
            Some(ActiveField::Name) => {
                if let Some(TextInputAction::Changed(name)) =
                    self.name_input.handle_key(text, &self.name)
                {
                    self.name = Self::sanitize_change(name, 80);
                }
            }
            Some(ActiveField::IconPath) => {
                if let Some(TextInputAction::Changed(path)) =
                    self.icon_input.handle_key(text, &self.icon_path)
                {
                    self.icon_path = Self::sanitize_change(path, 1024);
                }
            }
            None => {}
        }
    }

    fn copy_selection(&self) -> Option<String> {
        match self.active_field {
            Some(ActiveField::Name) => self.name_input.selected_text(&self.name),
            Some(ActiveField::IconPath) => self.icon_input.selected_text(&self.icon_path),
            None => None,
        }
    }

    fn cut_selection(&mut self) -> Option<String> {
        let clipboard = self.copy_selection()?;
        self.handle_active_key("\x08");
        Some(clipboard)
    }

    fn undo_active(&mut self) {
        match self.active_field {
            Some(ActiveField::Name) => {
                if let Some(name) = self.name_input.undo(&self.name) {
                    self.name = name;
                }
            }
            Some(ActiveField::IconPath) => {
                if let Some(path) = self.icon_input.undo(&self.icon_path) {
                    self.icon_path = path;
                }
            }
            None => {}
        }
    }

    fn select_all_active(&mut self) {
        match self.active_field {
            Some(ActiveField::Name) => self.name_input.select_all(&self.name),
            Some(ActiveField::IconPath) => self.icon_input.select_all(&self.icon_path),
            None => {}
        }
    }

    fn move_cursor_active(&mut self, dir: i32, shift: bool) {
        match self.active_field {
            Some(ActiveField::Name) => {
                if dir < 0 {
                    if shift {
                        self.name_input.move_left_shift();
                    } else {
                        self.name_input.move_left();
                    }
                } else if shift {
                    self.name_input.move_right_shift(&self.name);
                } else {
                    self.name_input.move_right(&self.name);
                }
            }
            Some(ActiveField::IconPath) => {
                if dir < 0 {
                    if shift {
                        self.icon_input.move_left_shift();
                    } else {
                        self.icon_input.move_left();
                    }
                } else if shift {
                    self.icon_input.move_right_shift(&self.icon_path);
                } else {
                    self.icon_input.move_right(&self.icon_path);
                }
            }
            None => {}
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> VoiceActorModalResult {
        let card = Self::card_rect(screen_w, screen_h);
        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if !card.contains(*x, *y) {
                    return VoiceActorModalResult::Close;
                }
            }
            _ => {}
        }

        match event {
            UiEvent::KeyInput { text } => {
                if text == "\x1b" {
                    return VoiceActorModalResult::Close;
                }
                if text == "\r" || text == "\n" {
                    return self.create_result();
                }
                self.handle_active_key(text);
                VoiceActorModalResult::Consumed
            }
            UiEvent::MousePress { x, y } => {
                if let Some(field) = Self::field_for_point(card, *x, *y) {
                    self.start_mouse_selection(field, card, *x, false);
                    return VoiceActorModalResult::Consumed;
                }

                let (_, browse_rect) = Self::icon_rects(card);
                if browse_rect.contains(*x, *y) {
                    self.deactivate_fields();
                    return VoiceActorModalResult::PickIcon;
                }

                let (cancel, create) = Self::button_rects(card);
                if cancel.contains(*x, *y) {
                    return VoiceActorModalResult::Close;
                }
                if create.contains(*x, *y) {
                    return self.create_result();
                }

                self.deactivate_fields();
                VoiceActorModalResult::Consumed
            }
            UiEvent::DoubleClick { x, y } => {
                if let Some(field) = Self::field_for_point(card, *x, *y) {
                    self.start_mouse_selection(field, card, *x, true);
                    return VoiceActorModalResult::Consumed;
                }
                VoiceActorModalResult::Consumed
            }
            UiEvent::MouseMove { x, .. } => {
                if self.update_mouse_selection(card, *x) {
                    return VoiceActorModalResult::Consumed;
                }
                VoiceActorModalResult::Consumed
            }
            UiEvent::MouseRelease { .. } => {
                self.selecting_field = None;
                VoiceActorModalResult::Consumed
            }
            UiEvent::CursorLeft => {
                self.move_cursor_active(-1, false);
                VoiceActorModalResult::Consumed
            }
            UiEvent::CursorRight => {
                self.move_cursor_active(1, false);
                VoiceActorModalResult::Consumed
            }
            UiEvent::ShiftCursorLeft => {
                self.move_cursor_active(-1, true);
                VoiceActorModalResult::Consumed
            }
            UiEvent::ShiftCursorRight => {
                self.move_cursor_active(1, true);
                VoiceActorModalResult::Consumed
            }
            UiEvent::SelectAll => {
                self.select_all_active();
                VoiceActorModalResult::Consumed
            }
            UiEvent::Copy => self
                .copy_selection()
                .map(VoiceActorModalResult::Clipboard)
                .unwrap_or(VoiceActorModalResult::Consumed),
            UiEvent::Cut => self
                .cut_selection()
                .map(VoiceActorModalResult::Clipboard)
                .unwrap_or(VoiceActorModalResult::Consumed),
            UiEvent::UndoTextEdit => {
                self.undo_active();
                VoiceActorModalResult::Consumed
            }
            _ => VoiceActorModalResult::Consumed,
        }
    }

    pub fn render<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let card = Self::card_rect(screen_w, screen_h);

        overlay_quads.push(quad(
            0.0,
            0.0,
            screen_w,
            screen_h,
            [0.0, 0.0, 0.0, 0.72],
            0.0,
        ));
        overlay_quads.push(card_quad(card));

        labels.push(LabelInfo {
            text: t("voice_actor_modal.title"),
            bounds: Rect {
                x: card.x,
                y: card.y + 12.0,
                width: card.width,
                height: 28.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(17.0),
            color_override: None,
            font_family_override: None,
        });
        labels.push(LabelInfo {
            text: t("voice_actor_modal.description"),
            bounds: Rect {
                x: card.x + 24.0,
                y: card.y + 42.0,
                width: card.width - 48.0,
                height: 22.0,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([170, 170, 185]),
            font_family_override: None,
        });

        self.render_field(
            overlay_quads,
            labels,
            card,
            t("voice_actor_modal.name"),
            Self::name_rect(card),
            &self.name,
            &self.name_input,
            ActiveField::Name,
        );
        let (icon_rect, browse_rect) = Self::icon_rects(card);
        self.render_field(
            overlay_quads,
            labels,
            card,
            t("voice_actor_modal.icon_path"),
            icon_rect,
            &self.icon_path,
            &self.icon_input,
            ActiveField::IconPath,
        );
        render_button(
            overlay_quads,
            labels,
            browse_rect,
            t("voice_actor_modal.browse"),
            false,
        );

        let (cancel, create) = Self::button_rects(card);
        render_button(
            overlay_quads,
            labels,
            cancel,
            t("voice_actor_modal.cancel"),
            false,
        );
        render_button(
            overlay_quads,
            labels,
            create,
            t("voice_actor_modal.create"),
            true,
        );
    }

    fn render_field<'a>(
        &'a self,
        overlay_quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        card: Rect,
        label: &'a str,
        field: Rect,
        value: &'a str,
        input: &TextInputState,
        field_kind: ActiveField,
    ) {
        labels.push(LabelInfo {
            text: label,
            bounds: Rect {
                x: field.x,
                y: field.y - 24.0,
                width: card.width - 48.0,
                height: 20.0,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([190, 190, 205]),
            font_family_override: None,
        });
        let active = self.active_field == Some(field_kind);
        overlay_quads.push(input_quad(field, active));
        render_text_selection_and_cursor(overlay_quads, field, value, input, active);
        labels.push(LabelInfo {
            text: if value.is_empty() { "" } else { value },
            bounds: field,
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: FIELD_PADDING_X,
            font_size_override: Some(FIELD_FONT_SIZE),
            color_override: Some([226, 226, 235]),
            font_family_override: None,
        });
    }
}

fn quad(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
        rect: [x, y, w, h],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn card_quad(card: Rect) -> QuadInstance {
    QuadInstance {
        rect: [card.x, card.y, card.width, card.height],
        color: [0.22, 0.22, 0.26, 1.0],
        color_bottom: [0.16, 0.16, 0.19, 1.0],
        border_color: [0.45, 0.45, 0.52, 0.8],
        border_width: 1.5,
        border_radius: 14.0,
        shadow_offset: [0.0, 4.0],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        shadow_blur: 10.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn input_quad(rect: Rect, active: bool) -> QuadInstance {
    QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.10, 0.10, 0.13, 1.0],
        color_bottom: [0.08, 0.08, 0.11, 1.0],
        border_color: if active {
            [0.45, 0.62, 0.95, 0.9]
        } else {
            [0.35, 0.35, 0.42, 0.9]
        },
        border_width: if active { 1.5 } else { 1.0 },
        border_radius: 7.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    }
}

fn render_text_selection_and_cursor(
    overlay_quads: &mut Vec<QuadInstance>,
    rect: Rect,
    value: &str,
    input: &TextInputState,
    active: bool,
) {
    text_input::render_selection_and_cursor(
        overlay_quads,
        rect,
        value,
        input,
        active,
        input_metrics(),
        6.0,
        6.0,
        [0.25, 0.45, 0.95, 0.42],
        [0.90, 0.90, 0.96, 1.0],
    );
}

fn input_metrics() -> TextInputMetrics {
    TextInputMetrics::left(FIELD_FONT_SIZE, FIELD_PADDING_X)
}

fn render_button<'a>(
    overlay_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    primary: bool,
) {
    overlay_quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: if primary {
            [0.34, 0.47, 0.82, 1.0]
        } else {
            [0.30, 0.30, 0.36, 1.0]
        },
        color_bottom: if primary {
            [0.25, 0.36, 0.70, 1.0]
        } else {
            [0.22, 0.22, 0.27, 1.0]
        },
        border_color: [0.55, 0.55, 0.65, 0.55],
        border_width: 1.0,
        border_radius: 8.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.25],
        shadow_blur: 4.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    labels.push(LabelInfo {
        text,
        bounds: rect,
        h_align: HAlign::Center,
        v_align: VAlign::Center,
        overflow: Overflow::Clip,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: None,
        font_family_override: None,
    });
}
