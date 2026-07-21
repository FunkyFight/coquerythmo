//! Highest foreground surface for the detection palette and information card.
//!
//! The rythmo renderer still owns signs, guides and hit testing. This module
//! owns the single visible popup and mirrors the legacy menu state so palette
//! and card never render twice.

use crate::accessibility::AccessibilityEvent;
use crate::detection::{
    track_storage_line_id, DetectionAddress, DetectionKind, MediaTick, TextAnchor,
};
use crate::project::Project;
use crate::ui::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{RythmoState, Selection};
use std::sync::{Mutex, OnceLock};

const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const MENU_WIDTH: f32 = MENU_PADDING * 2.0 + MENU_ICON_SIZE * 9.0 + MENU_GAP * 8.0;
const MENU_HEIGHT: f32 = MENU_ICON_SIZE + MENU_PADDING * 2.0;
const POPUP_CURSOR_GAP: f32 = 10.0;
const INFO_WIDTH: f32 = 470.0;
const INFO_HEIGHT: f32 = 176.0;
const INFO_PADDING: f32 = 12.0;
const INFO_IMAGE_SIZE: f32 = 136.0;
const TOOLTIP_WIDTH: f32 = 350.0;
const TOOLTIP_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sign {
    Labial,
    SemiLabial,
    MouthOpen,
    MouthClosed,
    TeethVisible,
    DentalTh,
    Breath,
    Neutral,
    Reaction,
}

impl Sign {
    const ALL: [Self; 9] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpen,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::DentalTh,
        Self::Breath,
        Self::Neutral,
        Self::Reaction,
    ];

    fn from_cue(cue: &crate::detection::DetectionCue) -> Option<Self> {
        let alternate = matches!(&cue.target, TextAnchor::AfterText);
        match (cue.kind, alternate) {
            (DetectionKind::Labial, _) => Some(Self::Labial),
            (DetectionKind::SemiLabial, _) => Some(Self::SemiLabial),
            (DetectionKind::MouthOpen, _) => Some(Self::MouthOpen),
            (DetectionKind::MouthClosed, _) => Some(Self::MouthClosed),
            (DetectionKind::TeethVisible, false) => Some(Self::TeethVisible),
            (DetectionKind::TeethVisible, true) => Some(Self::DentalTh),
            (DetectionKind::Breath, false) => Some(Self::Breath),
            (DetectionKind::Breath, true) => Some(Self::Neutral),
            (DetectionKind::Reaction, _) => Some(Self::Reaction),
            (DetectionKind::TextSyncPoint, _) => None,
        }
    }

    const fn storage(self) -> (DetectionKind, TextAnchor) {
        match self {
            Self::Labial => (DetectionKind::Labial, TextAnchor::BeforeText),
            Self::SemiLabial => (DetectionKind::SemiLabial, TextAnchor::BeforeText),
            Self::MouthOpen => (DetectionKind::MouthOpen, TextAnchor::BeforeText),
            Self::MouthClosed => (DetectionKind::MouthClosed, TextAnchor::BeforeText),
            Self::TeethVisible => (DetectionKind::TeethVisible, TextAnchor::BeforeText),
            Self::DentalTh => (DetectionKind::TeethVisible, TextAnchor::AfterText),
            Self::Breath => (DetectionKind::Breath, TextAnchor::BeforeText),
            Self::Neutral => (DetectionKind::Breath, TextAnchor::AfterText),
            Self::Reaction => (DetectionKind::Reaction, TextAnchor::BeforeText),
        }
    }

    const fn glyph(self) -> &'static str {
        match self {
            Self::Labial => "—",
            Self::SemiLabial => "×",
            Self::MouthOpen => "↑",
            Self::MouthClosed => "↓",
            Self::TeethVisible => "|||",
            Self::DentalTh => "th",
            Self::Breath => "///",
            Self::Neutral => "( )",
            Self::Reaction => "✦",
        }
    }
}

#[derive(Clone, Copy)]
enum Mouth {
    Aa,
    EhAe,
    Fv,
    KstEe,
    Pbm,
    UwOwW,
}

#[derive(Clone, Copy)]
struct Info {
    title: &'static str,
    description: &'static str,
    sounds: &'static str,
    quick_label: &'static str,
    mouth: Mouth,
}

fn info(sign: Sign) -> Info {
    match sign {
        Sign::Labial => Info {
            title: "Labiale",
            description: "Fermeture nette des lèvres.",
            sounds: "P, B, M",
            quick_label: "Labiale (P, B, M)",
            mouth: Mouth::Pbm,
        },
        Sign::SemiLabial => Info {
            title: "Semi-labiale",
            description: "Contact lèvre-dents, fermeture labiale incomplète.",
            sounds: "F, V",
            quick_label: "Semi-labiale (F, V)",
            mouth: Mouth::Fv,
        },
        Sign::MouthOpen => Info {
            title: "Bouche ouverte",
            description: "Ouverture marquée de la bouche.",
            sounds: "A, AN, O ouverts et voyelles larges",
            quick_label: "Bouche ouverte (A, AN, O ouverts)",
            mouth: Mouth::Aa,
        },
        Sign::MouthClosed => Info {
            title: "Bouche fermée",
            description: "Bouche refermée ou occlusion visuelle.",
            sounds: "Fermetures et attaques de consonnes occlusives",
            quick_label: "Bouche fermée (fermetures, consonnes occlusives)",
            mouth: Mouth::Pbm,
        },
        Sign::TeethVisible => Info {
            title: "Dents visibles",
            description: "Dents apparentes, articulation tendue.",
            sounds: "F, V, S, T, EE",
            quick_label: "Dents visibles (F, V, S, T, EE)",
            mouth: Mouth::KstEe,
        },
        Sign::DentalTh => Info {
            title: "TH",
            description: "Articulation dentale appuyée du « th ».",
            sounds: "TH, T et S appuyés",
            quick_label: "TH (TH, T, S appuyés)",
            mouth: Mouth::KstEe,
        },
        Sign::Breath => Info {
            title: "Respiration",
            description: "Souffle ou reprise d’air.",
            sounds: "Respiration, souffle et aspiration",
            quick_label: "Respiration (souffle, aspiration)",
            mouth: Mouth::UwOwW,
        },
        Sign::Neutral => Info {
            title: "Neutre / parenthèses",
            description: "Mouvement neutre ou intermédiaire.",
            sounds: "CH, dentales appuyées et articulation neutre",
            quick_label: "Neutre / parenthèses (CH, dentales, neutre)",
            mouth: Mouth::EhAe,
        },
        Sign::Reaction => Info {
            title: "Réaction",
            description: "Réaction vocale non verbale.",
            sounds: "Rires, exclamations et petits bruits vocaux",
            quick_label: "Réaction (rires, exclamations, bruits vocaux)",
            mouth: Mouth::Aa,
        },
    }
}

#[derive(Clone, Copy)]
struct HoverAnchor {
    track: u8,
    media_tick: MediaTick,
    screen_x: f32,
    screen_y: f32,
    track_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupKind {
    Palette,
    Info,
}

#[derive(Clone, Copy)]
enum Popup {
    None,
    Palette {
        visual: Rect,
        suppressed: Rect,
        track: u8,
        media_tick: MediaTick,
        selected: usize,
    },
    Info {
        visual: Rect,
        suppressed: Rect,
        sign: Sign,
    },
    Dismissed {
        kind: PopupKind,
        suppressed: Rect,
    },
}

#[derive(Clone, Copy)]
struct ForegroundState {
    popup: Popup,
    last_zone: Rect,
    last_hover: Option<HoverAnchor>,
    last_pointer: (f32, f32),
}

impl Default for ForegroundState {
    fn default() -> Self {
        Self {
            popup: Popup::None,
            last_zone: Rect::default(),
            last_hover: None,
            last_pointer: (0.0, 0.0),
        }
    }
}

fn foreground() -> &'static Mutex<ForegroundState> {
    static STATE: OnceLock<Mutex<ForegroundState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ForegroundState::default()))
}

fn lock_state() -> std::sync::MutexGuard<'static, ForegroundState> {
    foreground()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn event_pointer(event: &UiEvent) -> Option<(f32, f32)> {
    match event {
        UiEvent::MouseMove { x, y }
        | UiEvent::MousePress { x, y }
        | UiEvent::MouseRelease { x, y }
        | UiEvent::DoubleClick { x, y }
        | UiEvent::CtrlClick { x, y }
        | UiEvent::ShiftMousePress { x, y }
        | UiEvent::MiddlePress { x, y }
        | UiEvent::MiddleRelease { x, y }
        | UiEvent::ContextMenu { x, y } => Some((*x, *y)),
        UiEvent::Scroll { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

fn clamp_popup(mut rect: Rect, zone: Rect) -> Rect {
    rect.x = rect
        .x
        .clamp(zone.x, (zone.x + zone.width - rect.width).max(zone.x));
    rect.y = rect
        .y
        .clamp(zone.y, (zone.y + zone.height - rect.height).max(zone.y));
    rect
}

fn palette_visual_outer(hover: HoverAnchor, zone: Rect) -> Rect {
    clamp_popup(
        Rect {
            x: hover.screen_x + POPUP_CURSOR_GAP,
            y: hover.screen_y + POPUP_CURSOR_GAP,
            width: MENU_WIDTH,
            height: MENU_HEIGHT,
        },
        zone,
    )
}

fn palette_base_outer(hover: HoverAnchor, zone: Rect) -> Rect {
    let button_x = hover.screen_x - 9.0;
    let button_y = hover.track_rect.y + hover.track_rect.height + 4.0;
    clamp_popup(
        Rect {
            x: button_x,
            y: button_y + 20.0,
            width: MENU_WIDTH,
            height: MENU_HEIGHT,
        },
        zone,
    )
}

fn palette_item_rect(outer: Rect, index: usize) -> Rect {
    Rect {
        x: outer.x + MENU_PADDING + index as f32 * (MENU_ICON_SIZE + MENU_GAP),
        y: outer.y + MENU_PADDING,
        width: MENU_ICON_SIZE,
        height: MENU_ICON_SIZE,
    }
}

fn palette_item_at(outer: Rect, x: f32, y: f32) -> Option<usize> {
    Sign::ALL.iter().enumerate().find_map(|(index, _)| {
        palette_item_rect(outer, index)
            .contains(x, y)
            .then_some(index)
    })
}

fn selected_address(state: &RythmoState) -> Option<DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
}

fn selected_anchor_x(
    project: &Project,
    state: &RythmoState,
    zone: Rect,
    current_frame: f64,
) -> Option<f32> {
    let address = selected_address(state)?;
    address.track()?;
    let cue = project.detections().detection(address)?;
    let ppf = crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed();
    Some(
        zone.x
            + zone.width / 2.0
            + (cue.media_tick.as_frame_position() - current_frame) as f32 * ppf
            + 18.0,
    )
}

fn moved_index(current: usize, direction: i32) -> usize {
    (current as i32 + direction).rem_euclid(Sign::ALL.len() as i32) as usize
}

fn dismiss(kind: PopupKind, suppressed: Rect) {
    lock_state().popup = Popup::Dismissed { kind, suppressed };
}

fn activate_palette_choice(
    track: u8,
    media_tick: MediaTick,
    selected: usize,
    suppressed: Rect,
) -> EventResponse {
    let sign = Sign::ALL[selected.min(Sign::ALL.len() - 1)];
    let (kind, target) = sign.storage();
    dismiss(PopupKind::Palette, suppressed);
    EventResponse::Action(UiAction::AddDetection {
        line_id: track_storage_line_id(track),
        kind,
        media_tick,
        target,
    })
}

fn announce_palette_selection(selected: usize) -> EventResponse {
    let details = info(Sign::ALL[selected.min(Sign::ALL.len() - 1)]);
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Selection {
        label: details.quick_label.to_string(),
    }))
}

pub fn captures_input() -> bool {
    matches!(
        lock_state().popup,
        Popup::Palette { .. } | Popup::Info { .. }
    )
}

pub fn handle_modal_event(event: &UiEvent) -> Option<EventResponse> {
    let popup = lock_state().popup;
    match popup {
        Popup::None | Popup::Dismissed { .. } => None,
        Popup::Palette {
            visual,
            suppressed,
            track,
            media_tick,
            selected,
        } => match event {
            UiEvent::CursorLeft | UiEvent::CursorUp => {
                let next = moved_index(selected, -1);
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = next;
                }
                Some(announce_palette_selection(next))
            }
            UiEvent::CursorRight | UiEvent::CursorDown => {
                let next = moved_index(selected, 1);
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = next;
                }
                Some(announce_palette_selection(next))
            }
            UiEvent::Home => {
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = 0;
                }
                Some(announce_palette_selection(0))
            }
            UiEvent::End => {
                let last = Sign::ALL.len() - 1;
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = last;
                }
                Some(announce_palette_selection(last))
            }
            UiEvent::Activate => Some(activate_palette_choice(
                track, media_tick, selected, suppressed,
            )),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => Some(
                activate_palette_choice(track, media_tick, selected, suppressed),
            ),
            UiEvent::KeyInput { text } if text == "\x1b" => {
                dismiss(PopupKind::Palette, suppressed);
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseMove { x, y } => {
                if let Some(index) = palette_item_at(visual, *x, *y) {
                    if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                        *selected = index;
                    }
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } => {
                if let Some(index) = palette_item_at(visual, *x, *y) {
                    return Some(activate_palette_choice(
                        track, media_tick, index, suppressed,
                    ));
                }
                if !visual.contains(*x, *y) {
                    dismiss(PopupKind::Palette, suppressed);
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
        Popup::Info {
            visual, suppressed, ..
        } => match event {
            // Deletion is a global command. An open mouth information card must
            // not trap it before the selected detection reaches the workspace.
            UiEvent::Delete => None,
            UiEvent::KeyInput { text } if text == "\x1b" => {
                dismiss(PopupKind::Info, suppressed);
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } => {
                if !visual.contains(*x, *y) {
                    dismiss(PopupKind::Info, suppressed);
                }
                Some(EventResponse::Consumed)
            }
            _ => Some(EventResponse::Consumed),
        },
    }
}

pub(crate) fn reconcile_legacy_menu(state: &mut RythmoState) {
    let mut foreground = lock_state();
    if matches!(foreground.popup, Popup::Dismissed { .. }) {
        state.detection_menu = None;
        foreground.popup = Popup::None;
    }
}

pub fn sync_from_state(
    project: &Project,
    state: &RythmoState,
    zone: Rect,
    current_frame: f64,
    event: &UiEvent,
) {
    let pointer = event_pointer(event);
    let hover = state.detection_hover.map(|hover| HoverAnchor {
        track: hover.track,
        media_tick: hover.media_tick,
        screen_x: hover.screen_x,
        screen_y: hover.screen_y,
        track_rect: hover.track_rect,
    });

    let mut foreground = lock_state();
    foreground.last_zone = zone;
    if let Some(pointer) = pointer {
        foreground.last_pointer = pointer;
    }
    if let Some(hover) = hover {
        foreground.last_hover = Some(hover);
    }

    if matches!(foreground.popup, Popup::Dismissed { .. }) {
        return;
    }
    if state.detection_menu.is_none() {
        foreground.popup = Popup::None;
        return;
    }

    if let Some(hover) = hover {
        let (visual, suppressed, mut selected) = match foreground.popup {
            Popup::Palette {
                visual,
                suppressed,
                selected,
                ..
            } => (visual, suppressed, selected),
            _ => (
                palette_visual_outer(hover, zone),
                palette_base_outer(hover, zone),
                0,
            ),
        };
        if let Some((x, y)) = pointer {
            if let Some(index) = palette_item_at(visual, x, y) {
                selected = index;
            }
        }
        foreground.popup = Popup::Palette {
            visual,
            suppressed,
            track: hover.track,
            media_tick: hover.media_tick,
            selected,
        };
        return;
    }

    let Some(address) = selected_address(state) else {
        foreground.popup = Popup::None;
        return;
    };
    let Some(sign) = project
        .detections()
        .detection(address)
        .and_then(Sign::from_cue)
    else {
        foreground.popup = Popup::None;
        return;
    };

    if let Popup::Info {
        visual,
        suppressed,
        sign: previous_sign,
    } = foreground.popup
    {
        if previous_sign == sign {
            foreground.popup = Popup::Info {
                visual,
                suppressed,
                sign,
            };
            return;
        }
    }

    let (x, y) = match event {
        UiEvent::MouseRelease { x, y } => (*x, *y),
        _ => (
            selected_anchor_x(project, state, zone, current_frame)
                .unwrap_or(foreground.last_pointer.0),
            foreground.last_pointer.1,
        ),
    };
    let outer = clamp_popup(
        Rect {
            x: x + 8.0,
            y: y - INFO_HEIGHT - 8.0,
            width: INFO_WIDTH,
            height: INFO_HEIGHT,
        },
        zone,
    );
    foreground.popup = Popup::Info {
        visual: outer,
        suppressed: outer,
        sign,
    };
}

pub fn activate_palette() {
    let mut state = lock_state();
    let Some(hover) = state.last_hover else {
        return;
    };
    let zone = state.last_zone;
    state.popup = Popup::Palette {
        visual: palette_visual_outer(hover, zone),
        suppressed: palette_base_outer(hover, zone),
        track: hover.track,
        media_tick: hover.media_tick,
        selected: 0,
    };
}

pub fn clear() {
    *lock_state() = ForegroundState::default();
}

pub(crate) fn suppressed_popup() -> Option<(PopupKind, Rect)> {
    match lock_state().popup {
        Popup::None => None,
        Popup::Palette { suppressed, .. } => Some((PopupKind::Palette, suppressed)),
        Popup::Info { suppressed, .. } => Some((PopupKind::Info, suppressed)),
        Popup::Dismissed { kind, suppressed } => Some((kind, suppressed)),
    }
}

pub fn selected_info_accessibility_label(project: &Project, state: &RythmoState) -> Option<String> {
    let cue = project.detections().detection(selected_address(state)?)?;
    let details = info(Sign::from_cue(cue)?);
    Some(format!(
        "Fiche de détection. {}. Description : {} Sons correspondants : {}.",
        details.title, details.description, details.sounds
    ))
}

fn push_panel_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], radius: f32) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: radius,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.30],
        shadow_blur: 4.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_flat_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4]) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn push_label<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'static str,
    bounds: Rect,
    size: f32,
    color: [u8; 3],
    h_align: HAlign,
    v_align: VAlign,
) {
    labels.push(LabelInfo {
        text,
        bounds,
        h_align,
        v_align,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(size),
        color_override: Some(color),
        font_family_override: None,
    });
}

struct MouthBitmap {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

fn mouth_bitmap(mouth: Mouth) -> &'static MouthBitmap {
    static AA: OnceLock<MouthBitmap> = OnceLock::new();
    static EH_AE: OnceLock<MouthBitmap> = OnceLock::new();
    static FV: OnceLock<MouthBitmap> = OnceLock::new();
    static KST_EE: OnceLock<MouthBitmap> = OnceLock::new();
    static PBM: OnceLock<MouthBitmap> = OnceLock::new();
    static UW_OW_W: OnceLock<MouthBitmap> = OnceLock::new();

    fn decode(bytes: &[u8]) -> MouthBitmap {
        let source = image::load_from_memory(bytes)
            .expect("Rhubarb mouth PNG should decode")
            .to_rgba8();
        let (source_width, source_height) = source.dimensions();
        let scale = (INFO_IMAGE_SIZE / source_width.max(1) as f32)
            .min(INFO_IMAGE_SIZE / source_height.max(1) as f32);
        let width = ((source_width as f32 * scale).round() as u32).clamp(1, INFO_IMAGE_SIZE as u32);
        let height =
            ((source_height as f32 * scale).round() as u32).clamp(1, INFO_IMAGE_SIZE as u32);
        let resized = image::imageops::resize(
            &source,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        );
        let pixels = resized
            .as_raw()
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect();
        MouthBitmap {
            width,
            height,
            pixels,
        }
    }

    match mouth {
        Mouth::Aa => {
            AA.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/AA.png")))
        }
        Mouth::EhAe => {
            EH_AE.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/EH_AE.png")))
        }
        Mouth::Fv => {
            FV.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/F_V.png")))
        }
        Mouth::KstEe => KST_EE
            .get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/K_S_T_EE.png"))),
        Mouth::Pbm => {
            PBM.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/P_B_M.png")))
        }
        Mouth::UwOwW => UW_OW_W
            .get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/UW_OW_W.png"))),
    }
}

/// Draw a pixel-aligned, already-downsampled mouth. Every run is at least one
/// logical pixel high, avoiding the sub-pixel SDF attenuation that darkened the
/// previous reconstruction.
fn render_mouth(quads: &mut Vec<QuadInstance>, rect: Rect, mouth: Mouth) {
    let bitmap = mouth_bitmap(mouth);
    if bitmap.width == 0 || bitmap.height == 0 {
        return;
    }
    let origin_x = (rect.x + (rect.width - bitmap.width as f32) / 2.0).round();
    let origin_y = (rect.y + (rect.height - bitmap.height as f32) / 2.0).round();

    for y in 0..bitmap.height {
        let mut x = 0;
        while x < bitmap.width {
            let pixel = bitmap.pixels[(y * bitmap.width + x) as usize];
            if pixel[3] < 8 {
                x += 1;
                continue;
            }
            let start = x;
            x += 1;
            while x < bitmap.width && bitmap.pixels[(y * bitmap.width + x) as usize] == pixel {
                x += 1;
            }
            push_flat_quad(
                quads,
                Rect {
                    x: origin_x + start as f32,
                    y: origin_y + y as f32,
                    width: (x - start) as f32,
                    height: 1.0,
                },
                [
                    pixel[0] as f32 / 255.0,
                    pixel[1] as f32 / 255.0,
                    pixel[2] as f32 / 255.0,
                    pixel[3] as f32 / 255.0,
                ],
            );
        }
    }
}

pub fn append_foreground<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let snapshot = *lock_state();
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        width: screen_w,
        height: screen_h,
    };
    match snapshot.popup {
        Popup::None | Popup::Dismissed { .. } => {}
        Popup::Palette {
            visual, selected, ..
        } => {
            let outer = clamp_popup(visual, screen);
            push_panel_quad(quads, outer, [0.035, 0.039, 0.052, 0.999], 8.0);
            for (index, sign) in Sign::ALL.iter().copied().enumerate() {
                let item = palette_item_rect(outer, index);
                if selected == index {
                    push_panel_quad(quads, item, [0.18, 0.32, 0.58, 0.99], 5.0);
                }
                push_label(
                    labels,
                    sign.glyph(),
                    item,
                    if matches!(sign, Sign::DentalTh | Sign::Neutral) {
                        13.0
                    } else {
                        20.0
                    },
                    [244, 246, 252],
                    HAlign::Center,
                    VAlign::Center,
                );
            }

            let details = info(Sign::ALL[selected.min(Sign::ALL.len() - 1)]);
            let tooltip_y = if outer.y + outer.height + 6.0 + TOOLTIP_HEIGHT <= screen_h {
                outer.y + outer.height + 6.0
            } else {
                (outer.y - TOOLTIP_HEIGHT - 6.0).max(0.0)
            };
            let tooltip = Rect {
                x: (outer.x
                    + MENU_PADDING
                    + selected as f32 * (MENU_ICON_SIZE + MENU_GAP)
                    + MENU_ICON_SIZE / 2.0
                    - TOOLTIP_WIDTH / 2.0)
                    .clamp(0.0, (screen_w - TOOLTIP_WIDTH).max(0.0)),
                y: tooltip_y,
                width: TOOLTIP_WIDTH.min(screen_w),
                height: TOOLTIP_HEIGHT,
            };
            push_panel_quad(quads, tooltip, [0.025, 0.028, 0.038, 0.999], 6.0);
            push_label(
                labels,
                details.quick_label,
                tooltip,
                13.0,
                [245, 247, 252],
                HAlign::Center,
                VAlign::Center,
            );
        }
        Popup::Info { visual, sign, .. } => {
            let outer = clamp_popup(visual, screen);
            let details = info(sign);
            push_panel_quad(quads, outer, [0.026, 0.030, 0.042, 0.999], 11.0);
            let image_rect = Rect {
                x: outer.x + INFO_PADDING,
                y: outer.y + (outer.height - INFO_IMAGE_SIZE) / 2.0,
                width: INFO_IMAGE_SIZE,
                height: INFO_IMAGE_SIZE,
            };
            push_panel_quad(quads, image_rect, [0.10, 0.11, 0.14, 1.0], 8.0);
            render_mouth(quads, image_rect, details.mouth);

            let text_x = image_rect.x + image_rect.width + 14.0;
            let text_width = (outer.x + outer.width - INFO_PADDING - text_x).max(0.0);
            push_label(
                labels,
                details.title,
                Rect {
                    x: text_x,
                    y: outer.y + 12.0,
                    width: text_width,
                    height: 28.0,
                },
                18.0,
                [246, 248, 253],
                HAlign::Left,
                VAlign::Center,
            );
            push_label(
                labels,
                "Description",
                Rect {
                    x: text_x,
                    y: outer.y + 48.0,
                    width: text_width,
                    height: 18.0,
                },
                11.0,
                [142, 164, 202],
                HAlign::Left,
                VAlign::Center,
            );
            push_label(
                labels,
                details.description,
                Rect {
                    x: text_x,
                    y: outer.y + 66.0,
                    width: text_width,
                    height: 28.0,
                },
                13.0,
                [222, 227, 238],
                HAlign::Left,
                VAlign::Center,
            );
            push_label(
                labels,
                "Sons correspondants",
                Rect {
                    x: text_x,
                    y: outer.y + 104.0,
                    width: text_width,
                    height: 18.0,
                },
                11.0,
                [142, 164, 202],
                HAlign::Left,
                VAlign::Center,
            );
            push_label(
                labels,
                details.sounds,
                Rect {
                    x: text_x,
                    y: outer.y + 122.0,
                    width: text_width,
                    height: 38.0,
                },
                13.0,
                [242, 244, 249],
                HAlign::Left,
                VAlign::Top,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_navigation_wraps() {
        assert_eq!(moved_index(0, -1), Sign::ALL.len() - 1);
        assert_eq!(moved_index(Sign::ALL.len() - 1, 1), 0);
    }

    #[test]
    fn mouth_is_downsampled_to_pixel_aligned_card_size() {
        let bitmap = mouth_bitmap(Mouth::Pbm);
        assert!(bitmap.width <= INFO_IMAGE_SIZE as u32);
        assert!(bitmap.height <= INFO_IMAGE_SIZE as u32);
        assert!(bitmap.width > 0 && bitmap.height > 0);
    }

    #[test]
    fn delete_passes_through_an_open_information_card() {
        let rect = Rect {
            x: 10.0,
            y: 10.0,
            width: INFO_WIDTH,
            height: INFO_HEIGHT,
        };
        lock_state().popup = Popup::Info {
            visual: rect,
            suppressed: rect,
            sign: Sign::MouthOpen,
        };
        assert!(handle_modal_event(&UiEvent::Delete).is_none());
        clear();
    }
}
