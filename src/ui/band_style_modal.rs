//! "Band style" window: band and reading bar colours, reading bar width,
//! karaoke dot texture (default dot and named dot groups, each a shape, an
//! image or a ground/jump image pair) and style presets. Every change is previewed live on the band, which stays
//! visible beside the window; "Cancel" restores the style it opened with.

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use super::hsv_picker::HsvPicker;
use crate::band_style::{self, BandStyle, BandStylePreset, KaraokeDot};
use crate::i18n::t;

const CARD_W: f32 = 460.0;
const CARD_H: f32 = 606.0 + DOT_EXTRA_H;
const CARD_TOP: f32 = 48.0;
const CARD_MARGIN: f32 = 16.0;
const PAD: f32 = 22.0;
const CHIP_W: f32 = 46.0;
const PICKER_W: f32 = 236.0;
const PICKER_H: f32 = 214.0;
const PICKER_PAD: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColorTarget {
    Background,
    Playhead,
}
const PRESET_ROW_H: f32 = 26.0;
const PRESET_ROWS: usize = 5;
const DOT_CELL: f32 = 30.0;
const DOT_CELL_GAP: f32 = 7.0;
/// Built-in dots plus the custom image cell.
const DOT_CELLS: usize = KaraokeDot::BUILTINS.len() + 1;
/// Height of the image pair and dot group rows under the dot cells.
const DOT_EXTRA_H: f32 = 72.0;
/// Colour of the built-in shapes in the window: any light colour shows the
/// shape; on the band they take each line's character colour.
const DOT_PREVIEW_TINT: [f32; 4] = [0.95, 0.95, 1.0, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    BackgroundHex,
    PlayheadHex,
    Width,
    /// Which dot the dot row edits: the default dot or a group.
    KaraokeDotTarget,
    KaraokeDot,
    KaraokeDotImage,
    KaraokeDotPair,
    KaraokeDotJumpImage,
    DotGroupName,
    NewDotGroup,
    DeleteDotGroup,
    Presets,
    PresetName,
    SavePreset,
    DeletePreset,
    Import,
    Export,
    Cancel,
    Apply,
}

const FOCUS_ORDER: [Focus; 19] = [
    Focus::BackgroundHex,
    Focus::PlayheadHex,
    Focus::Width,
    Focus::KaraokeDotTarget,
    Focus::KaraokeDot,
    Focus::KaraokeDotImage,
    Focus::KaraokeDotPair,
    Focus::KaraokeDotJumpImage,
    Focus::DotGroupName,
    Focus::NewDotGroup,
    Focus::DeleteDotGroup,
    Focus::Presets,
    Focus::PresetName,
    Focus::SavePreset,
    Focus::DeletePreset,
    Focus::Import,
    Focus::Export,
    Focus::Cancel,
    Focus::Apply,
];

struct PresetEntry {
    preset: BandStylePreset,
    builtin: bool,
}

impl PresetEntry {
    fn display_name(&self) -> &str {
        if self.builtin {
            t(&self.preset.name)
        } else {
            &self.preset.name
        }
    }
}

pub struct BandStyleModal {
    original: BandStyle,
    style: BandStyle,
    background_hex: String,
    playhead_hex: String,
    /// Colour picker opened from a colour chip.
    color_picker: Option<(ColorTarget, HsvPicker)>,
    width_text: String,
    presets: Vec<PresetEntry>,
    selected_preset: Option<usize>,
    preset_scroll: usize,
    preset_name: String,
    /// Selectable dots: the built-ins, then the last custom image if any.
    dot_choices: Vec<KaraokeDot>,
    /// Dot edited by the dot row: `None` for the default dot, else the id
    /// of a dot group.
    dot_target: Option<u32>,
    focus: Focus,
    hovered: Option<Rect>,
}

/// A karaoke dot drawn as a texture inside the window (the classic circle
/// is drawn with quads).
pub struct KaraokeDotPreview<'a> {
    pub rect: Rect,
    pub dot: &'a KaraokeDot,
    pub tint: [f32; 4],
}

pub enum BandStyleModalResult {
    Consumed,
    /// Show this style on the band without saving it.
    Preview(BandStyle),
    /// Close and restore the style the window opened with.
    Cancel(BandStyle),
    /// Close and keep this style.
    Apply {
        style: BandStyle,
        original: BandStyle,
    },
    /// The user presets changed and must be saved.
    SavePresets(Vec<BandStylePreset>),
    Import,
    Export(BandStylePreset),
    /// Pick an image file for the karaoke dot.
    PickKaraokeDotImage,
    /// Pick the jump image of the karaoke dot's image pair.
    PickKaraokeDotJumpImage,
}

impl BandStyleModal {
    pub fn new(style: BandStyle, user_presets: Vec<BandStylePreset>) -> Self {
        let mut presets: Vec<PresetEntry> = band_style::builtin_presets()
            .into_iter()
            .map(|preset| PresetEntry {
                preset,
                builtin: true,
            })
            .collect();
        presets.extend(user_presets.into_iter().map(|preset| PresetEntry {
            preset,
            builtin: false,
        }));
        let selected_preset = presets
            .iter()
            .position(|entry| entry.preset.style.preset_part() == style.preset_part());
        let mut modal = Self {
            original: style.clone(),
            style: style.clone(),
            background_hex: String::new(),
            playhead_hex: String::new(),
            color_picker: None,
            width_text: String::new(),
            presets,
            selected_preset,
            preset_scroll: 0,
            preset_name: String::new(),
            dot_choices: KaraokeDot::BUILTINS.to_vec(),
            dot_target: None,
            focus: Focus::BackgroundHex,
            hovered: None,
        };
        modal.remember_custom_dot();
        modal.sync_texts();
        modal
    }

    pub fn style(&self) -> BandStyle {
        self.style.clone()
    }

    /// Dots the window can show, for texture caches.
    pub fn karaoke_dot_choices(&self) -> &[KaraokeDot] {
        &self.dot_choices
    }

    /// Keeps the current custom image selectable after picking another dot.
    fn remember_custom_dot(&mut self) {
        if self.target_dot().is_custom() {
            let dot = self.target_dot().clone();
            self.dot_choices.truncate(KaraokeDot::BUILTINS.len());
            self.dot_choices.push(dot);
        }
    }

    /// Dot edited by the dot row.
    fn target_dot(&self) -> &KaraokeDot {
        self.dot_target
            .and_then(|id| self.style.dot_group(id))
            .map(|group| &group.dot)
            .unwrap_or(&self.style.karaoke_dot)
    }

    /// Name of the dot edited by the dot row.
    fn target_name(&self) -> &str {
        self.dot_target
            .and_then(|id| self.style.dot_group(id))
            .map(|group| group.name.as_str())
            .unwrap_or_else(|| t("band_style.karaoke_dot.default_target"))
    }

    fn style_with_target_dot(&self, dot: KaraokeDot) -> BandStyle {
        let mut style = self.style.clone();
        match self
            .dot_target
            .and_then(|id| style.karaoke_dot_groups.iter_mut().find(|g| g.id == id))
        {
            Some(group) => group.dot = dot,
            None => style.karaoke_dot = dot,
        }
        style
    }

    /// Uses a freshly picked image as the edited dot (keeping its jump image
    /// if it has one); returns the style to preview.
    pub fn set_custom_karaoke_dot(&mut self, png_base64: String) -> BandStyle {
        self.focus = Focus::KaraokeDot;
        let jump_png_base64 = match self.target_dot() {
            KaraokeDot::Custom {
                jump_png_base64, ..
            } => jump_png_base64.clone(),
            _ => None,
        };
        let style = self.style_with_target_dot(KaraokeDot::Custom {
            png_base64,
            jump_png_base64,
        });
        let _ = self.set_style(style);
        self.style.clone()
    }

    /// Uses a freshly picked image as the jump image of the edited dot,
    /// turning it into an image pair; returns the style to preview.
    pub fn set_custom_karaoke_dot_jump(&mut self, jump_png_base64: String) -> BandStyle {
        self.focus = Focus::KaraokeDotPair;
        let png_base64 = match self.target_dot() {
            KaraokeDot::Custom { png_base64, .. } => png_base64.clone(),
            // Without a ground image the picked image serves both phases.
            _ => jump_png_base64.clone(),
        };
        let style = self.style_with_target_dot(KaraokeDot::Custom {
            png_base64,
            jump_png_base64: Some(jump_png_base64),
        });
        let _ = self.set_style(style);
        self.style.clone()
    }

    /// Checking "two images" asks for the jump image; unchecking keeps the
    /// ground image only.
    fn toggle_image_pair(&mut self) -> BandStyleModalResult {
        match self.target_dot().clone() {
            KaraokeDot::Custom {
                png_base64,
                jump_png_base64: Some(_),
            } => self.set_style(self.style_with_target_dot(KaraokeDot::custom(png_base64))),
            KaraokeDot::Custom { .. } => BandStyleModalResult::PickKaraokeDotJumpImage,
            _ => BandStyleModalResult::Consumed,
        }
    }

    fn select_dot(&mut self, index: usize) -> BandStyleModalResult {
        let Some(dot) = self.dot_choices.get(index).cloned() else {
            return BandStyleModalResult::Consumed;
        };
        self.set_style(self.style_with_target_dot(dot))
    }

    fn cycle_dot(&mut self, delta: i32) -> BandStyleModalResult {
        let len = self.dot_choices.len() as i32;
        let current = self
            .dot_choices
            .iter()
            .position(|dot| dot == self.target_dot())
            .unwrap_or(0) as i32;
        self.select_dot((current + delta).rem_euclid(len.max(1)) as usize)
    }

    /// Switches the dot row between the default dot and the groups.
    fn cycle_dot_target(&mut self, delta: i32) -> BandStyleModalResult {
        let targets: Vec<Option<u32>> = std::iter::once(None)
            .chain(self.style.karaoke_dot_groups.iter().map(|group| Some(group.id)))
            .collect();
        let current = targets
            .iter()
            .position(|target| *target == self.dot_target)
            .unwrap_or(0) as i32;
        self.dot_target = targets[(current + delta).rem_euclid(targets.len() as i32) as usize];
        self.remember_custom_dot();
        BandStyleModalResult::Consumed
    }

    fn new_dot_group(&mut self) -> BandStyleModalResult {
        let mut style = self.style.clone();
        let name = t("band_style.karaoke_dot.group_default_name")
            .replace("{n}", &(style.karaoke_dot_groups.len() + 1).to_string());
        let dot = self.target_dot().clone();
        let Some(id) = style.add_dot_group(name, dot) else {
            return BandStyleModalResult::Consumed;
        };
        self.dot_target = Some(id);
        self.focus = Focus::DotGroupName;
        self.set_style(style)
    }

    fn delete_dot_group(&mut self) -> BandStyleModalResult {
        let Some(id) = self.dot_target else {
            return BandStyleModalResult::Consumed;
        };
        let mut style = self.style.clone();
        style.remove_dot_group(id);
        self.dot_target = None;
        self.set_style(style)
    }

    fn edit_dot_group_name(&mut self, text: &str) -> BandStyleModalResult {
        let Some(id) = self.dot_target else {
            return BandStyleModalResult::Consumed;
        };
        let mut style = self.style.clone();
        let Some(group) = style.karaoke_dot_groups.iter_mut().find(|g| g.id == id) else {
            return BandStyleModalResult::Consumed;
        };
        if text == "\x08" {
            group.name.pop();
        } else {
            for c in text.chars().filter(|c| !c.is_control()) {
                if group.name.chars().count() < band_style::MAX_DOT_GROUP_NAME_LEN {
                    group.name.push(c);
                }
            }
        }
        self.set_style(style)
    }

    fn sync_texts(&mut self) {
        self.background_hex = band_style::to_hex(self.style.background);
        self.playhead_hex = band_style::to_hex(self.style.playhead);
        self.sync_width_text();
    }

    fn sync_width_text(&mut self) {
        self.width_text = format!("{:.0} px", self.style.playhead_width);
    }

    fn user_presets(&self) -> Vec<BandStylePreset> {
        self.presets
            .iter()
            .filter(|entry| !entry.builtin)
            .map(|entry| entry.preset.clone())
            .collect()
    }

    fn set_style(&mut self, style: BandStyle) -> BandStyleModalResult {
        self.style = style.normalized();
        if self
            .dot_target
            .is_some_and(|id| self.style.dot_group(id).is_none())
        {
            self.dot_target = None;
        }
        self.remember_custom_dot();
        self.sync_texts();
        BandStyleModalResult::Preview(self.style.clone())
    }

    fn select_preset(&mut self, index: usize) -> BandStyleModalResult {
        let Some(entry) = self.presets.get(index) else {
            return BandStyleModalResult::Consumed;
        };
        // Presets never change which dot each character of the project uses.
        let style = BandStyle {
            character_dots: self.style.character_dots.clone(),
            ..entry.preset.style.clone()
        };
        self.preset_name = if entry.builtin {
            String::new()
        } else {
            entry.preset.name.clone()
        };
        self.selected_preset = Some(index);
        if index < self.preset_scroll {
            self.preset_scroll = index;
        } else if index >= self.preset_scroll + PRESET_ROWS {
            self.preset_scroll = index + 1 - PRESET_ROWS;
        }
        self.set_style(style)
    }

    /// Adds an imported preset to the user presets and previews it.
    pub fn import_preset(&mut self, mut preset: BandStylePreset) -> Vec<BandStylePreset> {
        let base = preset.name.clone();
        let mut suffix = 2;
        while self
            .presets
            .iter()
            .any(|entry| entry.display_name() == preset.name)
        {
            preset.name = format!("{base} ({suffix})");
            suffix += 1;
        }
        self.presets.push(PresetEntry {
            preset,
            builtin: false,
        });
        let index = self.presets.len() - 1;
        let _ = self.select_preset(index);
        self.user_presets()
    }

    fn save_preset(&mut self) -> BandStyleModalResult {
        let name = self.preset_name.trim().to_string();
        if name.is_empty() {
            self.focus = Focus::PresetName;
            return BandStyleModalResult::Consumed;
        }
        let style = self.style.preset_part();
        if let Some(index) = self
            .presets
            .iter()
            .position(|entry| !entry.builtin && entry.preset.name == name)
        {
            self.presets[index].preset.style = style;
            self.selected_preset = Some(index);
        } else {
            self.presets.push(PresetEntry {
                preset: BandStylePreset { name, style },
                builtin: false,
            });
            let index = self.presets.len() - 1;
            self.selected_preset = Some(index);
            self.preset_scroll = index.saturating_sub(PRESET_ROWS - 1);
        }
        BandStyleModalResult::SavePresets(self.user_presets())
    }

    fn delete_preset(&mut self) -> BandStyleModalResult {
        let Some(index) = self.selected_preset else {
            return BandStyleModalResult::Consumed;
        };
        if self.presets.get(index).is_none_or(|entry| entry.builtin) {
            return BandStyleModalResult::Consumed;
        }
        self.presets.remove(index);
        self.selected_preset = None;
        self.preset_name.clear();
        self.preset_scroll = self
            .preset_scroll
            .min(self.presets.len().saturating_sub(PRESET_ROWS));
        BandStyleModalResult::SavePresets(self.user_presets())
    }

    fn export_preset(&self) -> BandStyleModalResult {
        let name = if !self.preset_name.trim().is_empty() {
            self.preset_name.trim().to_string()
        } else {
            self.selected_preset
                .and_then(|index| self.presets.get(index))
                .filter(|entry| entry.preset.style.preset_part() == self.style.preset_part())
                .map(|entry| entry.display_name().to_string())
                .unwrap_or_else(|| t("band_style.default_preset_name").to_string())
        };
        BandStyleModalResult::Export(BandStylePreset {
            name,
            style: self.style.preset_part(),
        })
    }

    fn adjust_width(&mut self, direction: f32) -> BandStyleModalResult {
        let mut style = self.style.clone();
        style.playhead_width = (style.playhead_width + direction).clamp(
            band_style::MIN_PLAYHEAD_WIDTH,
            band_style::MAX_PLAYHEAD_WIDTH,
        );
        self.set_style(style)
    }

    fn edit_text(&mut self, text: &str) -> BandStyleModalResult {
        let (field, max_len, hex) = match self.focus {
            Focus::BackgroundHex => (&mut self.background_hex, 7, true),
            Focus::PlayheadHex => (&mut self.playhead_hex, 7, true),
            Focus::PresetName => (&mut self.preset_name, 40, false),
            _ => return BandStyleModalResult::Consumed,
        };
        if text == "\x08" {
            field.pop();
        } else {
            for c in text.chars() {
                let allowed = if hex {
                    c.is_ascii_hexdigit() || (c == '#' && field.is_empty())
                } else {
                    !c.is_control()
                };
                if allowed && field.chars().count() < max_len {
                    field.push(if hex { c.to_ascii_uppercase() } else { c });
                }
            }
        }
        if !hex {
            return BandStyleModalResult::Consumed;
        }
        let Some(color) = band_style::parse_hex(field) else {
            return BandStyleModalResult::Consumed;
        };
        if let Some((target, picker)) = self.color_picker.as_mut() {
            let follows = matches!(
                (*target, self.focus),
                (ColorTarget::Background, Focus::BackgroundHex)
                    | (ColorTarget::Playhead, Focus::PlayheadHex)
            );
            if follows {
                picker.set_color(color);
            }
        }
        let mut style = self.style.clone();
        if self.focus == Focus::BackgroundHex {
            style.background = color;
        } else {
            style.playhead = color;
        }
        self.style = style.normalized();
        BandStyleModalResult::Preview(self.style.clone())
    }

    fn move_focus(&mut self, delta: i32) -> BandStyleModalResult {
        // Leaving a colour field restores its text when it is not a colour.
        self.sync_texts();
        let index = FOCUS_ORDER
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0) as i32;
        let next = (index + delta).rem_euclid(FOCUS_ORDER.len() as i32) as usize;
        self.focus = FOCUS_ORDER[next];
        BandStyleModalResult::Consumed
    }

    fn activate(&mut self) -> BandStyleModalResult {
        match self.focus {
            Focus::Presets => match self.selected_preset {
                Some(index) => self.select_preset(index),
                None => self.select_preset(0),
            },
            Focus::SavePreset | Focus::PresetName => self.save_preset(),
            Focus::DeletePreset => self.delete_preset(),
            Focus::Import => BandStyleModalResult::Import,
            Focus::Export => self.export_preset(),
            Focus::KaraokeDotImage => BandStyleModalResult::PickKaraokeDotImage,
            Focus::KaraokeDotPair => self.toggle_image_pair(),
            Focus::KaraokeDotJumpImage => {
                if self.target_dot().is_custom() {
                    BandStyleModalResult::PickKaraokeDotJumpImage
                } else {
                    BandStyleModalResult::Consumed
                }
            }
            Focus::KaraokeDotTarget => self.cycle_dot_target(1),
            Focus::NewDotGroup => self.new_dot_group(),
            Focus::DeleteDotGroup => self.delete_dot_group(),
            Focus::DotGroupName => {
                self.focus = Focus::KaraokeDot;
                BandStyleModalResult::Consumed
            }
            Focus::Cancel => BandStyleModalResult::Cancel(self.original.clone()),
            Focus::Apply
            | Focus::BackgroundHex
            | Focus::PlayheadHex
            | Focus::Width
            | Focus::KaraokeDot => BandStyleModalResult::Apply {
                style: self.style.clone(),
                original: self.original.clone(),
            },
        }
    }

    pub fn keyboard_focus_label(&self) -> String {
        match self.focus {
            Focus::BackgroundHex => {
                format!("{} {}", t("band_style.background"), self.background_hex)
            }
            Focus::PlayheadHex => format!("{} {}", t("band_style.playhead"), self.playhead_hex),
            Focus::Width => format!("{} {}", t("band_style.playhead_width"), self.width_text),
            Focus::KaraokeDotTarget => format!(
                "{} : {}",
                t("band_style.karaoke_dot.target"),
                self.target_name()
            ),
            Focus::KaraokeDot => format!(
                "{} ({}) : {}",
                t("band_style.karaoke_dot"),
                self.target_name(),
                t(self.target_dot().label_key())
            ),
            Focus::KaraokeDotImage => t("band_style.karaoke_dot.choose_image").to_string(),
            Focus::KaraokeDotPair => format!(
                "{} : {}",
                t("band_style.karaoke_dot.image_pair"),
                if self.target_dot().is_image_pair() {
                    t("band_style.karaoke_dot.checked")
                } else if self.target_dot().is_custom() {
                    t("band_style.karaoke_dot.unchecked")
                } else {
                    t("band_style.karaoke_dot.image_pair_needs_image")
                }
            ),
            Focus::KaraokeDotJumpImage => t("band_style.karaoke_dot.choose_jump_image").to_string(),
            Focus::DotGroupName => match self.dot_target {
                Some(_) => format!(
                    "{} : {}",
                    t("band_style.karaoke_dot.group_name"),
                    self.target_name()
                ),
                None => t("band_style.karaoke_dot.group_name_needs_group").to_string(),
            },
            Focus::NewDotGroup => t("band_style.karaoke_dot.new_group").to_string(),
            Focus::DeleteDotGroup => t("band_style.karaoke_dot.delete_group").to_string(),
            Focus::Presets => format!(
                "{}, {}",
                t("band_style.presets"),
                self.selected_preset
                    .and_then(|index| self.presets.get(index))
                    .map(PresetEntry::display_name)
                    .unwrap_or_else(|| t("band_style.no_preset"))
            ),
            Focus::PresetName => format!("{} : {}", t("band_style.preset_name"), self.preset_name),
            Focus::SavePreset => t("band_style.save_preset").to_string(),
            Focus::DeletePreset => t("band_style.delete_preset").to_string(),
            Focus::Import => t("band_style.import").to_string(),
            Focus::Export => t("band_style.export").to_string(),
            Focus::Cancel => t("band_style.cancel").to_string(),
            Focus::Apply => t("band_style.apply").to_string(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        screen_w: f32,
        screen_h: f32,
    ) -> BandStyleModalResult {
        let layout = Layout::new(screen_w, screen_h);
        if let Some(result) = self.handle_picker_event(&layout, event) {
            return result;
        }
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                BandStyleModalResult::Cancel(self.original.clone())
            }
            UiEvent::FocusNext => self.move_focus(1),
            UiEvent::FocusPrevious => self.move_focus(-1),
            UiEvent::KeyInput { text } if text == "\t" => self.move_focus(1),
            UiEvent::KeyInput { text } if text == "\u{b}" => self.move_focus(-1),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => self.activate(),
            UiEvent::Activate => self.activate(),
            UiEvent::CursorUp | UiEvent::CursorDown if self.focus == Focus::Presets => {
                let delta = if matches!(event, UiEvent::CursorDown) {
                    1
                } else {
                    -1
                };
                let len = self.presets.len() as i32;
                let current = self.selected_preset.map_or(-1, |index| index as i32);
                let next = (current + delta).rem_euclid(len.max(1)) as usize;
                self.select_preset(next)
            }
            UiEvent::CursorUp => self.move_focus(-1),
            UiEvent::CursorDown => self.move_focus(1),
            UiEvent::CursorLeft if self.focus == Focus::Width => self.adjust_width(-1.0),
            UiEvent::CursorRight if self.focus == Focus::Width => self.adjust_width(1.0),
            UiEvent::CursorLeft if self.focus == Focus::KaraokeDot => self.cycle_dot(-1),
            UiEvent::CursorRight if self.focus == Focus::KaraokeDot => self.cycle_dot(1),
            UiEvent::CursorLeft if self.focus == Focus::KaraokeDotTarget => {
                self.cycle_dot_target(-1)
            }
            UiEvent::CursorRight if self.focus == Focus::KaraokeDotTarget => {
                self.cycle_dot_target(1)
            }
            UiEvent::KeyInput { text }
                if self.focus == Focus::DotGroupName
                    && text != "\r"
                    && text != "\n"
                    && text != "\t"
                    && text != "\u{b}"
                    && text != "\x1b" =>
            {
                self.edit_dot_group_name(text)
            }
            UiEvent::KeyInput { text } if text == " " && !self.focus_is_text() => self.activate(),
            UiEvent::KeyInput { text } => self.edit_text(text),
            UiEvent::Delete if self.focus == Focus::Presets => self.delete_preset(),
            UiEvent::MouseMove { x, y } => {
                self.hovered = layout.hit_rect(self, *x, *y);
                BandStyleModalResult::Consumed
            }
            UiEvent::Scroll { x, y, delta, .. } if layout.preset_list.contains(*x, *y) => {
                let max = self.presets.len().saturating_sub(PRESET_ROWS);
                self.preset_scroll = if *delta > 0.0 {
                    self.preset_scroll.saturating_sub(1)
                } else {
                    (self.preset_scroll + 1).min(max)
                };
                BandStyleModalResult::Consumed
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                self.press(&layout, *x, *y)
            }
            _ => BandStyleModalResult::Consumed,
        }
    }

    fn target_color(&self, target: ColorTarget) -> [f32; 4] {
        match target {
            ColorTarget::Background => self.style.background,
            ColorTarget::Playhead => self.style.playhead,
        }
    }

    fn set_target_color(&mut self, target: ColorTarget, color: [f32; 4]) -> BandStyleModalResult {
        let mut style = self.style.clone();
        match target {
            ColorTarget::Background => style.background = color,
            ColorTarget::Playhead => style.playhead = color,
        }
        self.set_style(style)
    }

    /// Opens the picker for `target`, or closes it when it is already open.
    fn toggle_picker(&mut self, target: ColorTarget) {
        self.sync_texts();
        self.focus = match target {
            ColorTarget::Background => Focus::BackgroundHex,
            ColorTarget::Playhead => Focus::PlayheadHex,
        };
        if self.color_picker.is_some_and(|(open, _)| open == target) {
            self.color_picker = None;
        } else {
            self.color_picker = Some((target, HsvPicker::new(self.target_color(target))));
        }
    }

    /// Events for the open colour picker. `None` lets the event through.
    fn handle_picker_event(
        &mut self,
        layout: &Layout,
        event: &UiEvent,
    ) -> Option<BandStyleModalResult> {
        let (target, mut picker) = self.color_picker?;
        let area = layout.picker_area(target);
        let result = match event {
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.color_picker = None;
                return Some(BandStyleModalResult::Consumed);
            }
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                if layout.picker_panel(target).contains(*x, *y) =>
            {
                match picker.press(area, *x, *y) {
                    Some(color) => {
                        self.color_picker = Some((target, picker));
                        return Some(self.set_target_color(target, color));
                    }
                    None => BandStyleModalResult::Consumed,
                }
            }
            UiEvent::MouseMove { x, y } if picker.is_dragging() => {
                let changed = picker.drag(area, *x, *y);
                self.color_picker = Some((target, picker));
                return Some(match changed {
                    Some(color) => self.set_target_color(target, color),
                    None => BandStyleModalResult::Consumed,
                });
            }
            UiEvent::MouseRelease { .. } if picker.release() => BandStyleModalResult::Consumed,
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                // A click on a colour chip toggles the picker itself; any
                // other click closes it and keeps its normal effect.
                let on_chip = layout.chip(ColorTarget::Background).contains(*x, *y)
                    || layout.chip(ColorTarget::Playhead).contains(*x, *y);
                if !on_chip {
                    self.color_picker = None;
                }
                return None;
            }
            _ => return None,
        };
        self.color_picker = Some((target, picker));
        Some(result)
    }

    fn focus_is_text(&self) -> bool {
        matches!(
            self.focus,
            Focus::BackgroundHex | Focus::PlayheadHex | Focus::PresetName | Focus::DotGroupName
        )
    }

    fn press(&mut self, layout: &Layout, x: f32, y: f32) -> BandStyleModalResult {
        if !layout.card.contains(x, y) {
            // The band stays usable for looking around, but the window keeps
            // the input until it is closed.
            return BandStyleModalResult::Consumed;
        }
        for target in [ColorTarget::Background, ColorTarget::Playhead] {
            if layout.chip(target).contains(x, y) {
                self.toggle_picker(target);
                return BandStyleModalResult::Consumed;
            }
        }
        if layout.background_hex.contains(x, y) {
            self.sync_texts();
            self.focus = Focus::BackgroundHex;
            return BandStyleModalResult::Consumed;
        }
        if layout.playhead_hex.contains(x, y) {
            self.sync_texts();
            self.focus = Focus::PlayheadHex;
            return BandStyleModalResult::Consumed;
        }
        if layout.width_minus.contains(x, y) {
            self.focus = Focus::Width;
            return self.adjust_width(-1.0);
        }
        if layout.width_plus.contains(x, y) {
            self.focus = Focus::Width;
            return self.adjust_width(1.0);
        }
        for index in 0..self.dot_choices.len() {
            if layout.dot_cell(index).contains(x, y) {
                self.sync_texts();
                self.focus = Focus::KaraokeDot;
                return self.select_dot(index);
            }
        }
        for (rect, delta) in [(layout.dot_target_prev, -1), (layout.dot_target_next, 1)] {
            if rect.contains(x, y) {
                self.sync_texts();
                self.focus = Focus::KaraokeDotTarget;
                return self.cycle_dot_target(delta);
            }
        }
        if layout.dot_group_name.contains(x, y) {
            self.sync_texts();
            if self.dot_target.is_some() {
                self.focus = Focus::DotGroupName;
            }
            return BandStyleModalResult::Consumed;
        }
        if layout.preset_list.contains(x, y) {
            self.focus = Focus::Presets;
            let row = ((y - layout.preset_list.y) / PRESET_ROW_H).floor() as usize;
            return self.select_preset(self.preset_scroll + row);
        }
        self.sync_texts();
        let buttons = [
            (layout.dot_image, Focus::KaraokeDotImage),
            (layout.dot_pair, Focus::KaraokeDotPair),
            (layout.dot_jump_image, Focus::KaraokeDotJumpImage),
            (layout.new_dot_group, Focus::NewDotGroup),
            (layout.delete_dot_group, Focus::DeleteDotGroup),
            (layout.preset_name, Focus::PresetName),
            (layout.save_preset, Focus::SavePreset),
            (layout.delete_preset, Focus::DeletePreset),
            (layout.import, Focus::Import),
            (layout.export, Focus::Export),
            (layout.cancel, Focus::Cancel),
            (layout.apply, Focus::Apply),
        ];
        for (rect, focus) in buttons {
            if rect.contains(x, y) {
                self.focus = focus;
                if focus == Focus::PresetName {
                    return BandStyleModalResult::Consumed;
                }
                return self.activate();
            }
        }
        BandStyleModalResult::Consumed
    }

    pub fn render<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let layout = Layout::new(screen_w, screen_h);
        let card = layout.card;
        push_quad(
            quads,
            card,
            [0.18, 0.18, 0.22, 0.97],
            [0.45, 0.45, 0.52, 0.8],
            12.0,
        );
        if let Some(last) = quads.last_mut() {
            last.shadow_offset = [0.0, 6.0];
            last.shadow_color = [0.0, 0.0, 0.0, 0.5];
            last.shadow_blur = 18.0;
        }
        push_label(
            labels,
            t("band_style.title"),
            Rect {
                y: card.y + 10.0,
                height: 28.0,
                ..card
            },
            HAlign::Center,
            16.0,
            None,
        );

        self.render_color_block(
            quads,
            labels,
            &layout,
            t("band_style.background"),
            layout.background_label_y,
            self.style.background,
            layout.background_hex,
            &self.background_hex,
            self.focus == Focus::BackgroundHex,
        );
        self.render_color_block(
            quads,
            labels,
            &layout,
            t("band_style.playhead"),
            layout.playhead_label_y,
            self.style.playhead,
            layout.playhead_hex,
            &self.playhead_hex,
            self.focus == Focus::PlayheadHex,
        );

        push_label(
            labels,
            t("band_style.playhead_width"),
            Rect {
                x: layout.width_minus.x - 150.0,
                width: 144.0,
                ..layout.width_minus
            },
            HAlign::Right,
            12.0,
            Some([180, 180, 195]),
        );
        self.push_button(quads, labels, layout.width_minus, "-", false);
        push_label(
            labels,
            &self.width_text,
            Rect {
                x: layout.width_minus.x + layout.width_minus.width,
                width: layout.width_plus.x - layout.width_minus.x - layout.width_minus.width,
                ..layout.width_minus
            },
            HAlign::Center,
            12.0,
            None,
        );
        self.push_button(quads, labels, layout.width_plus, "+", false);
        if self.focus == Focus::Width {
            push_outline(
                quads,
                Rect {
                    x: layout.width_minus.x - 3.0,
                    y: layout.width_minus.y - 3.0,
                    width: layout.width_plus.x + layout.width_plus.width - layout.width_minus.x
                        + 6.0,
                    height: layout.width_minus.height + 6.0,
                },
            );
        }

        self.render_karaoke_dot_row(quads, labels, &layout);

        push_label(
            labels,
            t("band_style.presets"),
            Rect {
                x: card.x + PAD,
                y: layout.preset_list.y - 24.0,
                width: card.width - PAD * 2.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        push_quad(
            quads,
            layout.preset_list,
            [0.08, 0.08, 0.10, 1.0],
            [0.30, 0.30, 0.36, 0.5],
            4.0,
        );
        for row in 0..PRESET_ROWS {
            let index = self.preset_scroll + row;
            let Some(entry) = self.presets.get(index) else {
                break;
            };
            let rect = Rect {
                x: layout.preset_list.x + 2.0,
                y: layout.preset_list.y + PRESET_ROW_H * row as f32,
                width: layout.preset_list.width - 4.0,
                height: PRESET_ROW_H,
            };
            if self.selected_preset == Some(index) {
                push_quad(quads, rect, [0.31, 0.40, 0.72, 0.85], [0.0; 4], 3.0);
            } else if self.hovered == Some(rect) {
                push_quad(quads, rect, [0.25, 0.25, 0.30, 0.9], [0.0; 4], 3.0);
            }
            let preview = Rect {
                x: rect.x + 6.0,
                y: rect.y + 5.0,
                width: 30.0,
                height: rect.height - 10.0,
            };
            push_quad(
                quads,
                preview,
                entry.preset.style.background,
                [0.5, 0.5, 0.55, 0.8],
                2.0,
            );
            push_quad(
                quads,
                Rect {
                    x: preview.x + preview.width / 2.0 - 1.5,
                    width: 3.0,
                    ..preview
                },
                entry.preset.style.playhead,
                [0.0; 4],
                0.0,
            );
            push_label(
                labels,
                entry.display_name(),
                Rect {
                    x: preview.x + preview.width + 6.0,
                    width: rect.width - preview.width - 18.0,
                    ..rect
                },
                HAlign::Left,
                12.0,
                if entry.builtin {
                    Some([200, 200, 215])
                } else {
                    None
                },
            );
        }
        if self.focus == Focus::Presets {
            push_outline(quads, layout.preset_list);
        }

        push_text_field(
            quads,
            labels,
            layout.preset_name,
            &self.preset_name,
            t("band_style.preset_name"),
            self.focus == Focus::PresetName,
        );
        let can_delete = self
            .selected_preset
            .and_then(|index| self.presets.get(index))
            .is_some_and(|entry| !entry.builtin);
        for (rect, text, focus, enabled) in [
            (
                layout.save_preset,
                t("band_style.save_preset"),
                Focus::SavePreset,
                true,
            ),
            (
                layout.delete_preset,
                t("band_style.delete_preset"),
                Focus::DeletePreset,
                can_delete,
            ),
            (layout.import, t("band_style.import"), Focus::Import, true),
            (layout.export, t("band_style.export"), Focus::Export, true),
            (layout.cancel, t("band_style.cancel"), Focus::Cancel, true),
            (layout.apply, t("band_style.apply"), Focus::Apply, true),
        ] {
            self.push_button(quads, labels, rect, text, focus == Focus::Apply);
            if !enabled {
                if let Some(label) = labels.last_mut() {
                    label.color_override = Some([110, 110, 120]);
                }
            }
            if self.focus == focus {
                push_outline(quads, rect);
            }
        }
        self.render_picker(quads, &layout);
    }

    fn render_karaoke_dot_row<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &Layout,
    ) {
        push_label(
            labels,
            t("band_style.karaoke_dot"),
            Rect {
                x: layout.card.x + PAD,
                y: layout.dot_label_y,
                width: layout.card.width - PAD * 2.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        for (index, dot) in self.dot_choices.iter().enumerate() {
            let rect = layout.dot_cell(index);
            let selected = dot == self.target_dot();
            let hovered = self.hovered == Some(rect);
            push_quad(
                quads,
                rect,
                if hovered {
                    [0.16, 0.16, 0.20, 1.0]
                } else {
                    [0.08, 0.08, 0.10, 1.0]
                },
                if selected {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.45, 0.45, 0.52, 0.8]
                },
                4.0,
            );
            if selected {
                if let Some(last) = quads.last_mut() {
                    last.border_width = 2.0;
                }
            }
            if dot.is_circle() {
                let inner = dot_preview_rect(rect);
                push_quad(
                    quads,
                    inner,
                    DOT_PREVIEW_TINT,
                    [0.0; 4],
                    inner.width / 2.0,
                );
            }
        }
        self.push_button(
            quads,
            labels,
            layout.dot_image,
            t("band_style.karaoke_dot.choose_image"),
            false,
        );
        if self.focus == Focus::KaraokeDotImage {
            push_outline(quads, layout.dot_image);
        }
        self.render_dot_group_rows(quads, labels, layout);
        if self.focus == Focus::KaraokeDot {
            let last = layout.dot_cell(self.dot_choices.len().saturating_sub(1));
            let first = layout.dot_cell(0);
            push_outline(
                quads,
                Rect {
                    x: first.x - 3.0,
                    y: first.y - 3.0,
                    width: last.x + last.width - first.x + 6.0,
                    height: first.height + 6.0,
                },
            );
        }
    }

    /// Target selector, image pair row and dot group row.
    fn render_dot_group_rows<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &Layout,
    ) {
        let dim = Some([110, 110, 120]);
        // Which dot the row edits.
        self.push_button(quads, labels, layout.dot_target_prev, "<", false);
        push_label(
            labels,
            self.target_name(),
            Rect {
                x: layout.dot_target_prev.x + layout.dot_target_prev.width,
                width: layout.dot_target_next.x
                    - layout.dot_target_prev.x
                    - layout.dot_target_prev.width,
                ..layout.dot_target_prev
            },
            HAlign::Center,
            12.0,
            None,
        );
        self.push_button(quads, labels, layout.dot_target_next, ">", false);
        if self.focus == Focus::KaraokeDotTarget {
            push_outline(
                quads,
                Rect {
                    x: layout.dot_target_prev.x - 3.0,
                    y: layout.dot_target_prev.y - 3.0,
                    width: layout.dot_target_next.x + layout.dot_target_next.width
                        - layout.dot_target_prev.x
                        + 6.0,
                    height: layout.dot_target_prev.height + 6.0,
                },
            );
        }

        // "Two images" checkbox and jump image button.
        let dot = self.target_dot();
        let custom = dot.is_custom();
        let pair = dot.is_image_pair();
        let box_rect = Rect {
            x: layout.dot_pair.x + 2.0,
            y: layout.dot_pair.y + 5.0,
            width: 18.0,
            height: 18.0,
        };
        push_quad(
            quads,
            box_rect,
            if pair {
                [0.20, 0.43, 0.82, 1.0]
            } else {
                [0.08, 0.08, 0.10, 1.0]
            },
            [0.45, 0.45, 0.52, 0.8],
            3.0,
        );
        if pair {
            push_quad(
                quads,
                Rect {
                    x: box_rect.x + 5.0,
                    y: box_rect.y + 5.0,
                    width: 8.0,
                    height: 8.0,
                },
                [1.0; 4],
                [0.0; 4],
                1.5,
            );
        }
        push_label(
            labels,
            t("band_style.karaoke_dot.image_pair"),
            Rect {
                x: box_rect.x + box_rect.width + 2.0,
                width: layout.dot_pair.width - box_rect.width - 4.0,
                ..layout.dot_pair
            },
            HAlign::Left,
            12.0,
            if custom { None } else { dim },
        );
        if self.focus == Focus::KaraokeDotPair {
            push_outline(quads, layout.dot_pair);
        }
        self.push_button(
            quads,
            labels,
            layout.dot_jump_image,
            t("band_style.karaoke_dot.choose_jump_image"),
            false,
        );
        if !custom {
            if let Some(label) = labels.last_mut() {
                label.color_override = dim;
            }
        }
        if self.focus == Focus::KaraokeDotJumpImage {
            push_outline(quads, layout.dot_jump_image);
        }

        // Group name, new group, delete group.
        let group_name = match self.dot_target {
            Some(_) => self.target_name(),
            None => "",
        };
        push_text_field(
            quads,
            labels,
            layout.dot_group_name,
            group_name,
            t("band_style.karaoke_dot.group_name"),
            self.focus == Focus::DotGroupName,
        );
        for (rect, text, focus, enabled) in [
            (
                layout.new_dot_group,
                t("band_style.karaoke_dot.new_group"),
                Focus::NewDotGroup,
                self.style.karaoke_dot_groups.len() < band_style::MAX_DOT_GROUPS,
            ),
            (
                layout.delete_dot_group,
                t("band_style.karaoke_dot.delete_group"),
                Focus::DeleteDotGroup,
                self.dot_target.is_some(),
            ),
        ] {
            self.push_button(quads, labels, rect, text, false);
            if !enabled {
                if let Some(label) = labels.last_mut() {
                    label.color_override = dim;
                }
            }
            if self.focus == focus {
                push_outline(quads, rect);
            }
        }
    }

    /// Textured dot previews to draw over the window's quads.
    pub fn karaoke_dot_previews(&self, screen_w: f32, screen_h: f32) -> Vec<KaraokeDotPreview<'_>> {
        let layout = Layout::new(screen_w, screen_h);
        self.dot_choices
            .iter()
            .enumerate()
            .filter(|(_, dot)| !dot.is_circle())
            .map(|(index, dot)| KaraokeDotPreview {
                rect: dot_preview_rect(layout.dot_cell(index)),
                dot,
                tint: if crate::karaoke_dot::is_tinted(dot) {
                    DOT_PREVIEW_TINT
                } else {
                    [1.0; 4]
                },
            })
            .collect()
    }

    fn render_picker(&self, quads: &mut Vec<QuadInstance>, layout: &Layout) {
        let Some((target, picker)) = self.color_picker else {
            return;
        };
        let panel = layout.picker_panel(target);
        push_quad(quads, panel, [0.16, 0.16, 0.20, 0.98], [0.45, 0.45, 0.52, 0.9], 10.0);
        if let Some(last) = quads.last_mut() {
            last.shadow_offset = [0.0, 6.0];
            last.shadow_color = [0.0, 0.0, 0.0, 0.5];
            last.shadow_blur = 16.0;
        }
        picker.render(quads, layout.picker_area(target));
    }

    #[allow(clippy::too_many_arguments)]
    fn render_color_block<'a>(
        &'a self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        layout: &Layout,
        title: &'a str,
        label_y: f32,
        current: [f32; 4],
        hex_rect: Rect,
        hex: &'a str,
        focused: bool,
    ) {
        push_label(
            labels,
            title,
            Rect {
                x: layout.card.x + PAD,
                y: label_y,
                width: layout.card.width - PAD * 2.0,
                height: 20.0,
            },
            HAlign::Left,
            12.0,
            Some([180, 180, 195]),
        );
        // The chip shows the colour like the band does (sRGB on an sRGB
        // surface) and opens the colour picker.
        let chip = Layout::chip_for(hex_rect);
        push_quad(
            quads,
            chip,
            super::color_picker::srgb_to_linear(current),
            [0.45, 0.45, 0.52, 0.8],
            4.0,
        );
        push_text_field(quads, labels, hex_rect, hex, "#RRGGBB", focused);
    }

    fn push_button<'a>(
        &self,
        quads: &mut Vec<QuadInstance>,
        labels: &mut Vec<LabelInfo<'a>>,
        rect: Rect,
        text: &'a str,
        accent: bool,
    ) {
        let hovered = self.hovered == Some(rect);
        let color = match (accent, hovered) {
            (true, true) => [0.28, 0.50, 0.90, 1.0],
            (true, false) => [0.20, 0.43, 0.82, 1.0],
            (false, true) => [0.22, 0.22, 0.27, 1.0],
            (false, false) => [0.15, 0.15, 0.18, 1.0],
        };
        push_quad(quads, rect, color, [0.35, 0.35, 0.42, 0.8], 6.0);
        push_label(labels, text, rect, HAlign::Center, 12.0, None);
    }
}

struct Layout {
    card: Rect,
    background_label_y: f32,
    background_hex: Rect,
    playhead_label_y: f32,
    playhead_hex: Rect,
    width_minus: Rect,
    width_plus: Rect,
    dot_label_y: f32,
    dot_cells_y: f32,
    dot_image: Rect,
    dot_target_prev: Rect,
    dot_target_next: Rect,
    dot_pair: Rect,
    dot_jump_image: Rect,
    dot_group_name: Rect,
    new_dot_group: Rect,
    delete_dot_group: Rect,
    preset_list: Rect,
    preset_name: Rect,
    save_preset: Rect,
    delete_preset: Rect,
    import: Rect,
    export: Rect,
    cancel: Rect,
    apply: Rect,
}

impl Layout {
    fn new(screen_w: f32, screen_h: f32) -> Self {
        let card = Rect {
            x: (screen_w - CARD_W - CARD_MARGIN).max(CARD_MARGIN),
            y: CARD_TOP.min((screen_h - CARD_H).max(0.0)),
            width: CARD_W,
            height: CARD_H,
        };
        let left = card.x + PAD;
        let inner_w = card.width - PAD * 2.0;
        let row = |y: f32, x: f32, width: f32| Rect {
            x,
            y: card.y + y,
            width,
            height: 28.0,
        };
        let half = (inner_w - 10.0) / 2.0;
        Self {
            card,
            background_label_y: card.y + 46.0,
            background_hex: row(68.0, left, 110.0),
            playhead_label_y: card.y + 140.0,
            playhead_hex: row(162.0, left, 110.0),
            width_minus: row(194.0, card.x + card.width - PAD - 126.0, 28.0),
            width_plus: row(194.0, card.x + card.width - PAD - 28.0, 28.0),
            dot_label_y: card.y + 234.0,
            dot_cells_y: card.y + 256.0,
            dot_image: {
                let x = left + (DOT_CELL + DOT_CELL_GAP) * DOT_CELLS as f32;
                row(257.0, x, left + inner_w - x)
            },
            dot_target_prev: Rect {
                x: card.x + card.width - PAD - 220.0,
                y: card.y + 230.0,
                width: 24.0,
                height: 24.0,
            },
            dot_target_next: Rect {
                x: card.x + card.width - PAD - 24.0,
                y: card.y + 230.0,
                width: 24.0,
                height: 24.0,
            },
            dot_pair: row(294.0, left, inner_w - 190.0),
            dot_jump_image: row(294.0, card.x + card.width - PAD - 180.0, 180.0),
            dot_group_name: row(328.0, left, inner_w - 220.0),
            new_dot_group: row(328.0, card.x + card.width - PAD - 210.0, 100.0),
            delete_dot_group: row(328.0, card.x + card.width - PAD - 100.0, 100.0),
            preset_list: Rect {
                x: left,
                y: card.y + 328.0 + DOT_EXTRA_H,
                width: inner_w,
                height: PRESET_ROW_H * PRESET_ROWS as f32,
            },
            preset_name: row(468.0 + DOT_EXTRA_H, left, inner_w - 220.0),
            save_preset: row(468.0 + DOT_EXTRA_H, card.x + card.width - PAD - 210.0, 100.0),
            delete_preset: row(468.0 + DOT_EXTRA_H, card.x + card.width - PAD - 100.0, 100.0),
            import: row(506.0 + DOT_EXTRA_H, left, half),
            export: row(506.0 + DOT_EXTRA_H, left + half + 10.0, half),
            cancel: row(CARD_H - 50.0, card.x + card.width - PAD - 230.0, 110.0),
            apply: row(CARD_H - 50.0, card.x + card.width - PAD - 110.0, 110.0),
        }
    }

    fn chip_for(hex_rect: Rect) -> Rect {
        Rect {
            x: hex_rect.x + hex_rect.width + 10.0,
            y: hex_rect.y + 2.0,
            width: CHIP_W,
            height: hex_rect.height - 4.0,
        }
    }

    fn chip(&self, target: ColorTarget) -> Rect {
        Self::chip_for(match target {
            ColorTarget::Background => self.background_hex,
            ColorTarget::Playhead => self.playhead_hex,
        })
    }

    /// Floating panel of the colour picker, beside the window (on its left,
    /// where the band usually is not), level with the chip that opened it.
    fn picker_panel(&self, target: ColorTarget) -> Rect {
        let chip = self.chip(target);
        let left = self.card.x - PICKER_W - 12.0;
        let x = if left >= 8.0 { left } else { chip.x + chip.width + 10.0 };
        Rect {
            x,
            y: chip.y - 8.0,
            width: PICKER_W,
            height: PICKER_H,
        }
    }

    fn picker_area(&self, target: ColorTarget) -> Rect {
        let panel = self.picker_panel(target);
        Rect {
            x: panel.x + PICKER_PAD,
            y: panel.y + PICKER_PAD,
            width: panel.width - PICKER_PAD * 2.0,
            height: panel.height - PICKER_PAD * 2.0,
        }
    }

    fn dot_cell(&self, index: usize) -> Rect {
        Rect {
            x: self.card.x + PAD + (DOT_CELL + DOT_CELL_GAP) * index as f32,
            y: self.dot_cells_y,
            width: DOT_CELL,
            height: DOT_CELL,
        }
    }

    /// Hoverable control under the pointer, for hover highlights.
    fn hit_rect(&self, modal: &BandStyleModal, x: f32, y: f32) -> Option<Rect> {
        if let Some(cell) = (0..modal.dot_choices.len())
            .map(|index| self.dot_cell(index))
            .find(|cell| cell.contains(x, y))
        {
            return Some(cell);
        }
        if self.preset_list.contains(x, y) {
            let row = ((y - self.preset_list.y) / PRESET_ROW_H).floor() as usize;
            if modal.preset_scroll + row < modal.presets.len() {
                return Some(Rect {
                    x: self.preset_list.x + 2.0,
                    y: self.preset_list.y + PRESET_ROW_H * row as f32,
                    width: self.preset_list.width - 4.0,
                    height: PRESET_ROW_H,
                });
            }
            return None;
        }
        [
            self.width_minus,
            self.width_plus,
            self.dot_image,
            self.dot_target_prev,
            self.dot_target_next,
            self.dot_jump_image,
            self.new_dot_group,
            self.delete_dot_group,
            self.save_preset,
            self.delete_preset,
            self.import,
            self.export,
            self.cancel,
            self.apply,
        ]
        .into_iter()
        .find(|rect| rect.contains(x, y))
    }
}

/// Area of a dot cell where the dot itself is drawn.
fn dot_preview_rect(cell: Rect) -> Rect {
    let inset = 6.0;
    Rect {
        x: cell.x + inset,
        y: cell.y + inset,
        width: cell.width - inset * 2.0,
        height: cell.height - inset * 2.0,
    }
}

fn push_text_field<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    placeholder: &'a str,
    focused: bool,
) {
    push_quad(
        quads,
        rect,
        [0.08, 0.08, 0.10, 1.0],
        if focused {
            [0.38, 0.65, 1.0, 1.0]
        } else {
            [0.30, 0.30, 0.36, 0.5]
        },
        4.0,
    );
    let empty = text.is_empty();
    push_label(
        labels,
        if empty { placeholder } else { text },
        rect,
        HAlign::Left,
        12.0,
        if empty { Some([110, 110, 120]) } else { None },
    );
    if focused {
        // Caret at the end of the text: the fields are short and only grow
        // or shrink from the end.
        let width = if empty {
            0.0
        } else {
            crate::vector_text::measure_ui_text_layout_standalone(text, 12.0).0
        };
        push_quad(
            quads,
            Rect {
                x: (rect.x + 6.0 + width + 1.0).min(rect.x + rect.width - 4.0),
                y: rect.y + 6.0,
                width: 1.5,
                height: rect.height - 12.0,
            },
            [0.9, 0.9, 0.95, 1.0],
            [0.0; 4],
            0.0,
        );
    }
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'a str,
    bounds: Rect,
    align: HAlign,
    size: f32,
    color: Option<[u8; 3]>,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align: align,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 6.0,
        font_size_override: Some(size),
        color_override: color,
        font_family_override: None,
    });
}

fn push_quad(
    quads: &mut Vec<QuadInstance>,
    rect: Rect,
    color: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: radius,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_outline(quads: &mut Vec<QuadInstance>, rect: Rect) {
    push_quad(quads, rect, [0.0; 4], [0.38, 0.65, 1.0, 1.0], 8.0);
    if let Some(last) = quads.last_mut() {
        last.border_width = 2.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modal() -> BandStyleModal {
        BandStyleModal::new(BandStyle::default(), Vec::new())
    }

    #[test]
    fn colour_chip_opens_a_picker_that_previews_live() {
        let mut modal = modal();
        let layout = Layout::new(1920.0, 1080.0);
        let chip = layout.chip(ColorTarget::Playhead);
        let _ = modal.handle_event(
            &UiEvent::MousePress {
                x: chip.x + 2.0,
                y: chip.y + 2.0,
            },
            1920.0,
            1080.0,
        );
        assert!(matches!(modal.color_picker, Some((ColorTarget::Playhead, _))));
        let (square, _) = HsvPicker::layout(layout.picker_area(ColorTarget::Playhead));
        // Top-left of the square: white.
        let result = modal.handle_event(
            &UiEvent::MousePress {
                x: square.x,
                y: square.y,
            },
            1920.0,
            1080.0,
        );
        let BandStyleModalResult::Preview(style) = result else {
            panic!("expected a live preview");
        };
        assert_eq!(band_style::to_hex(style.playhead), "#FFFFFF");
        // Dragging to the bottom makes it black.
        let result = modal.handle_event(
            &UiEvent::MouseMove {
                x: square.x + 1.0,
                y: square.y + square.height + 20.0,
            },
            1920.0,
            1080.0,
        );
        let BandStyleModalResult::Preview(style) = result else {
            panic!("expected a live preview while dragging");
        };
        assert_eq!(band_style::to_hex(style.playhead), "#000000");
        let _ = modal.handle_event(&UiEvent::MouseRelease { x: 0.0, y: 0.0 }, 1920.0, 1080.0);
        // Escape closes the picker before the window.
        let result = modal.handle_event(
            &UiEvent::KeyInput {
                text: "\x1b".into(),
            },
            1920.0,
            1080.0,
        );
        assert!(matches!(result, BandStyleModalResult::Consumed));
        assert!(modal.color_picker.is_none());
    }

    #[test]
    fn typing_a_hex_colour_previews_it() {
        let mut modal = modal();
        modal.background_hex.clear();
        let result = modal.edit_text("#102030");
        let BandStyleModalResult::Preview(style) = result else {
            panic!("expected a preview");
        };
        assert_eq!(band_style::to_hex(style.background), "#102030");
    }

    #[test]
    fn escape_restores_the_original_style() {
        let mut modal = modal();
        let _ = modal.adjust_width(2.0);
        assert_eq!(modal.style().playhead_width, 5.0);
        let result = modal.handle_event(
            &UiEvent::KeyInput {
                text: "\x1b".into(),
            },
            1920.0,
            1080.0,
        );
        assert!(
            matches!(result, BandStyleModalResult::Cancel(style) if style == BandStyle::default())
        );
    }

    fn custom_png() -> String {
        use base64::Engine as _;
        let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(&mut png),
            image.as_raw(),
            4,
            4,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        base64::engine::general_purpose::STANDARD.encode(png)
    }

    #[test]
    fn clicking_a_dot_previews_it_and_keeps_focus_labels() {
        let mut modal = modal();
        let layout = Layout::new(1920.0, 1080.0);
        let star = KaraokeDot::BUILTINS
            .iter()
            .position(|dot| *dot == KaraokeDot::Star)
            .unwrap();
        let cell = layout.dot_cell(star);
        let result = modal.handle_event(
            &UiEvent::MousePress {
                x: cell.x + 5.0,
                y: cell.y + 5.0,
            },
            1920.0,
            1080.0,
        );
        let BandStyleModalResult::Preview(style) = result else {
            panic!("expected a preview");
        };
        assert_eq!(style.karaoke_dot, KaraokeDot::Star);
        assert!(modal.keyboard_focus_label().contains(t("band_style.karaoke_dot.star")));
        // The arrow keys cycle through the dots.
        let BandStyleModalResult::Preview(style) =
            modal.handle_event(&UiEvent::CursorRight, 1920.0, 1080.0)
        else {
            panic!("expected a preview");
        };
        assert_eq!(style.karaoke_dot, KaraokeDot::BUILTINS[star + 1]);
        // Only textured dots need a texture preview.
        let previews = modal.karaoke_dot_previews(1920.0, 1080.0);
        assert_eq!(previews.len(), KaraokeDot::BUILTINS.len() - 1);
    }

    #[test]
    fn picked_image_is_previewed_and_stays_selectable() {
        let mut modal = modal();
        let style = modal.set_custom_karaoke_dot(custom_png());
        assert!(style.karaoke_dot.is_custom());
        assert_eq!(modal.karaoke_dot_choices().len(), DOT_CELLS);
        let _ = modal.select_dot(0);
        assert_eq!(modal.style().karaoke_dot, KaraokeDot::Circle);
        let BandStyleModalResult::Preview(style) = modal.select_dot(DOT_CELLS - 1) else {
            panic!("expected a preview");
        };
        assert!(style.karaoke_dot.is_custom());
        let preview = modal
            .karaoke_dot_previews(1920.0, 1080.0)
            .into_iter()
            .find(|preview| preview.dot.is_custom())
            .unwrap();
        assert_eq!(preview.tint, [1.0; 4]);
        // Garbage images are refused by normalization.
        let style = modal.set_custom_karaoke_dot("garbage".into());
        assert_eq!(style.karaoke_dot, KaraokeDot::Circle);
    }

    #[test]
    fn image_button_asks_for_a_file_and_layout_does_not_overlap() {
        let mut modal = modal();
        let layout = Layout::new(1920.0, 1080.0);
        let button = layout.dot_image;
        let result = modal.handle_event(
            &UiEvent::MousePress {
                x: button.x + 4.0,
                y: button.y + 4.0,
            },
            1920.0,
            1080.0,
        );
        assert!(matches!(result, BandStyleModalResult::PickKaraokeDotImage));
        let last_cell = layout.dot_cell(DOT_CELLS - 1);
        assert!(last_cell.x + last_cell.width < button.x);
        assert!(button.x + button.width <= layout.card.x + CARD_W - PAD + 0.5);
        assert!(layout.width_minus.y + layout.width_minus.height < layout.dot_label_y);
        assert!(last_cell.y + last_cell.height < layout.dot_pair.y);
        assert!(layout.dot_target_prev.x > layout.card.x + PAD + 100.0);
        assert!(layout.dot_pair.y + layout.dot_pair.height < layout.dot_group_name.y);
        assert!(layout.dot_pair.x + layout.dot_pair.width < layout.dot_jump_image.x);
        assert!(layout.dot_group_name.y + layout.dot_group_name.height < layout.preset_list.y - 24.0);
        assert!(layout.dot_group_name.x + layout.dot_group_name.width < layout.new_dot_group.x);
        assert!(layout.import.y + layout.import.height < layout.cancel.y);
    }

    #[test]
    fn dot_groups_are_created_renamed_edited_and_deleted() {
        let mut modal = modal();
        let BandStyleModalResult::Preview(style) = modal.new_dot_group() else {
            panic!("expected a preview");
        };
        assert_eq!(style.karaoke_dot_groups.len(), 1);
        let id = style.karaoke_dot_groups[0].id;
        assert_eq!(modal.dot_target, Some(id));
        assert_eq!(modal.focus, Focus::DotGroupName);
        // Typing renames the group, space included.
        for text in ["\x08", "\x08", " ", "Z"] {
            let _ = modal.handle_event(&UiEvent::KeyInput { text: text.into() }, 1920.0, 1080.0);
        }
        assert!(modal.style().karaoke_dot_groups[0].name.ends_with(" Z"));
        // The dot row now edits the group, not the default dot.
        let star = KaraokeDot::BUILTINS
            .iter()
            .position(|dot| *dot == KaraokeDot::Star)
            .unwrap();
        let _ = modal.select_dot(star);
        assert_eq!(modal.style().karaoke_dot_groups[0].dot, KaraokeDot::Star);
        assert_eq!(modal.style().karaoke_dot, KaraokeDot::Circle);
        assert!(modal.keyboard_focus_label().contains(" Z"));
        // The target selector goes back to the default dot.
        modal.focus = Focus::KaraokeDotTarget;
        let _ = modal.handle_event(&UiEvent::CursorLeft, 1920.0, 1080.0);
        assert_eq!(modal.dot_target, None);
        let _ = modal.select_dot(star + 1);
        assert_eq!(modal.style().karaoke_dot, KaraokeDot::BUILTINS[star + 1]);
        assert_eq!(modal.style().karaoke_dot_groups[0].dot, KaraokeDot::Star);
        // Deleting a group drops its character assignments.
        let _ = modal.cycle_dot_target(1);
        let mut style = modal.style();
        style.set_character_dot("ALICE", Some(band_style::CharacterDot::Group(id)));
        let _ = modal.set_style(style);
        let _ = modal.delete_dot_group();
        assert!(modal.style().karaoke_dot_groups.is_empty());
        assert!(modal.style().character_dots.is_empty());
        assert_eq!(modal.dot_target, None);
    }

    #[test]
    fn image_pair_checkbox_asks_for_the_jump_image() {
        let mut modal = modal();
        // Without an image the checkbox does nothing.
        modal.focus = Focus::KaraokeDotPair;
        assert!(matches!(modal.activate(), BandStyleModalResult::Consumed));
        let _ = modal.set_custom_karaoke_dot(custom_png());
        modal.focus = Focus::KaraokeDotPair;
        assert!(matches!(
            modal.activate(),
            BandStyleModalResult::PickKaraokeDotJumpImage
        ));
        let style = modal.set_custom_karaoke_dot_jump(custom_png());
        assert!(style.karaoke_dot.is_image_pair());
        // Picking a new ground image keeps the jump image.
        let style = modal.set_custom_karaoke_dot(custom_png());
        assert!(style.karaoke_dot.is_image_pair());
        // Unchecking keeps the ground image only.
        modal.focus = Focus::KaraokeDotPair;
        let BandStyleModalResult::Preview(style) = modal.activate() else {
            panic!("expected a preview");
        };
        assert!(style.karaoke_dot.is_custom());
        assert!(!style.karaoke_dot.is_image_pair());
    }

    #[test]
    fn presets_keep_the_character_dots_of_the_project() {
        let mut style = BandStyle::default();
        style.set_character_dot(
            "ALICE",
            Some(band_style::CharacterDot::Dot(KaraokeDot::Heart)),
        );
        let mut modal = BandStyleModal::new(style.clone(), Vec::new());
        // The default preset still matches: character dots are not part of it.
        assert_eq!(modal.selected_preset, Some(0));
        let BandStyleModalResult::Preview(applied) = modal.select_preset(2) else {
            panic!("expected a preview");
        };
        assert_eq!(applied.character_dots, style.character_dots);
        let BandStyleModalResult::Export(preset) = modal.export_preset() else {
            panic!("expected an export");
        };
        assert!(preset.style.character_dots.is_empty());
    }

    #[test]
    fn saving_needs_a_name_and_overwrites_same_name() {
        let mut modal = modal();
        assert!(matches!(
            modal.save_preset(),
            BandStyleModalResult::Consumed
        ));
        modal.preset_name = "Doublage".into();
        let BandStyleModalResult::SavePresets(presets) = modal.save_preset() else {
            panic!("expected presets to save");
        };
        assert_eq!(presets.len(), 1);
        let _ = modal.adjust_width(1.0);
        let BandStyleModalResult::SavePresets(presets) = modal.save_preset() else {
            panic!("expected presets to save");
        };
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].style.playhead_width, 4.0);
    }

    #[test]
    fn builtin_presets_cannot_be_deleted() {
        let mut modal = modal();
        let _ = modal.select_preset(0);
        assert!(matches!(
            modal.delete_preset(),
            BandStyleModalResult::Consumed
        ));
        assert_eq!(modal.presets.len(), band_style::builtin_presets().len());
    }

    #[test]
    fn imported_presets_get_a_unique_name() {
        let mut modal = modal();
        let preset = BandStylePreset {
            name: "Mon style".into(),
            style: BandStyle::default(),
        };
        modal.import_preset(preset.clone());
        let presets = modal.import_preset(preset);
        assert_eq!(presets[1].name, "Mon style (2)");
    }
}
