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
use crate::rythmo_line::LinePresence;
use crate::ui::primitives::{
    EventResponse, HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiAction, UiEvent, VAlign,
};
use crate::workspaces::rythmo::view::{RythmoState, Selection};
use std::sync::{Mutex, OnceLock};

const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const MENU_COLUMNS: usize = 9;
const MENU_WIDTH: f32 = MENU_PADDING * 2.0 + MENU_ICON_SIZE * 9.0 + MENU_GAP * 8.0;
const MENU_HEIGHT: f32 = MENU_ICON_SIZE * 2.0 + MENU_GAP + MENU_PADDING * 2.0;
const POPUP_CURSOR_GAP: f32 = 10.0;
const INFO_DESCRIPTION_HEADING: &str = "Description";
const INFO_SOUNDS_HEADING: &str = "Sons correspondants";
const INFO_WIDTH: f32 = 470.0;
const INFO_PADDING: f32 = 12.0;
const INFO_IMAGE_SIZE: f32 = 136.0;
const TOOLTIP_WIDTH: f32 = 420.0;
const TOOLTIP_GAP: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sign {
    Labial,
    SemiLabial,
    MouthOpen,
    MouthOpenArrow,
    MouthClosed,
    TeethVisible,
    DentalTh,
    Breath,
    Neutral,
    Reaction,
    Pucker,
    ForwardWave,
    Off,
    Back,
    RemoveUnderline,
}

impl Sign {
    const ALL: [Self; 14] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpenArrow,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::DentalTh,
        Self::Breath,
        Self::Neutral,
        Self::Reaction,
        Self::Pucker,
        Self::MouthOpen,
        Self::ForwardWave,
        Self::Off,
        Self::Back,
    ];

    fn from_cue(cue: &crate::detection::DetectionCue) -> Option<Self> {
        let alternate = matches!(&cue.target, TextAnchor::AfterText);
        match (cue.kind, alternate) {
            (DetectionKind::Labial, _) => Some(Self::Labial),
            (DetectionKind::SemiLabial, _) => Some(Self::SemiLabial),
            (DetectionKind::OpeningWave, _) => Some(Self::MouthOpen),
            (DetectionKind::MouthOpen, _) => Some(Self::MouthOpenArrow),
            (DetectionKind::MouthClosed, _) => Some(Self::MouthClosed),
            (DetectionKind::TeethVisible, false) => Some(Self::TeethVisible),
            (DetectionKind::TeethVisible, true) => Some(Self::DentalTh),
            (DetectionKind::Breath, false) => Some(Self::Breath),
            (DetectionKind::Breath, true) => Some(Self::Neutral),
            (DetectionKind::Reaction, _) => Some(Self::Reaction),
            (DetectionKind::Pucker, _) => Some(Self::Pucker),
            (DetectionKind::ForwardWave, _) => Some(Self::ForwardWave),
            (DetectionKind::TextSyncPoint, _) => None,
        }
    }

    const fn storage(self) -> Option<(DetectionKind, TextAnchor)> {
        Some(match self {
            Self::Labial => (DetectionKind::Labial, TextAnchor::BeforeText),
            Self::SemiLabial => (DetectionKind::SemiLabial, TextAnchor::BeforeText),
            Self::MouthOpen => (DetectionKind::OpeningWave, TextAnchor::BeforeText),
            Self::MouthOpenArrow => (DetectionKind::MouthOpen, TextAnchor::BeforeText),
            Self::MouthClosed => (DetectionKind::MouthClosed, TextAnchor::BeforeText),
            Self::TeethVisible => (DetectionKind::TeethVisible, TextAnchor::BeforeText),
            Self::DentalTh => (DetectionKind::TeethVisible, TextAnchor::AfterText),
            Self::Breath => (DetectionKind::Breath, TextAnchor::BeforeText),
            Self::Neutral => (DetectionKind::Breath, TextAnchor::AfterText),
            Self::Reaction => (DetectionKind::Reaction, TextAnchor::BeforeText),
            Self::Pucker => (DetectionKind::Pucker, TextAnchor::BeforeText),
            Self::ForwardWave => (DetectionKind::ForwardWave, TextAnchor::BeforeText),
            Self::Off | Self::Back => return None,
            Self::RemoveUnderline => return None,
        })
    }

    const fn glyph(self) -> &'static str {
        match self {
            Self::Labial => "—",
            Self::SemiLabial => "×",
            Self::MouthOpen => "⌣",
            Self::MouthOpenArrow => "↑",
            Self::MouthClosed => "↓",
            Self::TeethVisible => "|||",
            Self::DentalTh => "th",
            Self::Breath => "///",
            Self::Neutral => "( )",
            Self::Reaction => "✦",
            Self::Pucker => "><",
            Self::Off => "━━━",
            Self::Back => "┄┄┄",
            Self::ForwardWave => "⌢",
            Self::RemoveUnderline => "↶",
        }
    }
}

fn palette_signs(line_presence: Option<LinePresence>) -> Vec<Sign> {
    let mut signs = Sign::ALL[..12].to_vec();
    match line_presence {
        Some(LinePresence::Off) => {
            signs.push(Sign::Back);
            signs.push(Sign::RemoveUnderline);
        }
        Some(LinePresence::Back) => {
            signs.push(Sign::Off);
            signs.push(Sign::RemoveUnderline);
        }
        Some(LinePresence::On) | None => {
            if line_presence.is_some() {
                signs.extend([Sign::Off, Sign::Back]);
            }
        }
    }
    signs
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
    usage: &'static str,
    quick_label: &'static str,
    mouth: Mouth,
}

// Usage follows the UPAD Charte de la détection de doublage, pp. 2–7:
// https://upad.fr/wp-content/uploads/2018/12/Charte-de-la-d%C3%A9tection-de-doublageY.J..pdf
// TeethVisible is an application-specific visual cue, not a charter convention.
fn info(sign: Sign) -> Info {
    match sign {
        Sign::Labial => Info {
            title: "Labiale",
            description: "Fermeture nette des lèvres.",
            sounds: "P, B, M",
            usage: "À placer dès la fermeture complète des lèvres sur P, B ou M, vérifiée à l’image.",
            quick_label: "Labiale (P, B, M)",
            mouth: Mouth::Pbm,
        },
        Sign::SemiLabial => Info {
            title: "Semi-labiale",
            description: "Contact lèvre-dents, fermeture labiale incomplète.",
            sounds: "F, V",
            usage: "À placer sur le contact le plus marqué entre la lèvre inférieure et les dents, pour F ou V.",
            quick_label: "Semi-labiale (F, V)",
            mouth: Mouth::Fv,
        },
        Sign::MouthOpen => Info {
            title: "Vague d'ouverture",
            description: "La bouche s’ouvre ou s’étire nettement.",
            sounds: "a / â ; é / er / ez ; è / ê / ai / ei ; i / y ; in / im / ain / ein ; parfois an / en",
            usage: "À placer sur une ouverture visible (é, i, a…), y compris un mouvement sans parole.",
            quick_label: "Vague d'ouverture",
            mouth: Mouth::Aa,
        },
        Sign::MouthOpenArrow => Info {
            title: "Bouche ouverte",
            description: "Début / fin de phrase, bouche ouverte.",
            sounds: "État de la bouche à l’image",
            usage: "À utiliser au début ou à la fin d’une phrase si la bouche est ouverte à cette image.",
            quick_label: "Bouche ouverte",
            mouth: Mouth::Aa,
        },
        Sign::MouthClosed => Info {
            title: "Bouche fermée",
            description: "Début / fin de phrase, bouche fermée.",
            sounds: "État de la bouche à l’image",
            usage: "À utiliser au début ou à la fin d’une phrase si la bouche est fermée à cette image.",
            quick_label: "Bouche fermée (début ou fin de phrase)",
            mouth: Mouth::Pbm,
        },
        Sign::TeethVisible => Info {
            title: "Dents visibles",
            description: "Dents apparentes, articulation tendue.",
            sounds: "À confirmer d’après l’image et le son",
            usage: "Repère visuel de l’application pour des dents apparentes ; pas de correspondance automatique avec une consonne. Convention à valider en studio.",
            quick_label: "Dents visibles (repère visuel)",
            mouth: Mouth::KstEe,
        },
        Sign::DentalTh => Info {
            title: "TH",
            description: "Articulation dentale appuyée du « th ».",
            sounds: "TH anglais : think, this",
            usage: "Une dentale est un son articulé avec la langue contre les dents supérieures. Pour le TH anglais, placer le signe quand la langue apparaît nettement entre les dents.",
            quick_label: "TH anglais (langue entre les dents)",
            mouth: Mouth::KstEe,
        },
        Sign::Breath => Info {
            title: "Respiration",
            description: "Souffle ou reprise d’air.",
            sounds: "Respiration, souffle et aspiration",
            usage: "À placer sur une inspiration ou une expiration repérable au son et à l’image.",
            quick_label: "Respiration (souffle, aspiration)",
            mouth: Mouth::UwOwW,
        },
        Sign::Neutral => Info {
            title: "Neutre / parenthèses",
            description: "Mouvement neutre ou intermédiaire.",
            sounds: "CH, dentales appuyées et articulation neutre",
            usage: "Pour un CH ou une dentale très appuyée, selon la convention du détecteur. Une dentale est un son articulé avec la langue contre les dents supérieures.",
            quick_label: "Neutre / parenthèses (CH, dentales, neutre)",
            mouth: Mouth::EhAe,
        },
        Sign::Reaction => Info {
            title: "Réaction",
            description: "Réaction vocale non verbale.",
            sounds: "Rires, exclamations et petits bruits vocaux",
            usage: "Pour un rire, cri, pleur ou une toux ; compléter par une phonétique synchrone.",
            quick_label: "Réaction (rires, exclamations, bruits vocaux)",
            mouth: Mouth::Aa,
        },
        Sign::Pucker => Info {
            title: "Cul de poule",
            description: "Les lèvres se resserrent et se projettent en petite moue.",
            sounds: "Lèvres pincées, baiser, petite projection labiale",
            usage: "À placer sur le resserrement des lèvres le plus marqué.",
            quick_label: "Cul de poule",
            mouth: Mouth::UwOwW,
        },
        Sign::ForwardWave => Info {
            title: "Vague d'avancée",
            description: "Les lèvres s’arrondissent et se projettent vers l’avant.",
            sounds: "o ; au / eau ; on / om ; ou ; u ; eu / œu ; parfois w dans oui, quoi, oiseau ou loin",
            usage: "À placer sur une avancée visible des lèvres (on, o, ou, u, eu…), même sans parole.",
            quick_label: "Vague d'avancée",
            mouth: Mouth::UwOwW,
        },
        Sign::Off => Info {
            title: "OFF",
            description: "Réplique hors caméra.",
            sounds: "Soulignage continu à l’export",
            usage: "Pour une voix dont le personnage est absent du plan.",
            quick_label: "Marquer la réplique comme OFF (hors caméra)",
            mouth: Mouth::EhAe,
        },
        Sign::Back => Info {
            title: "De dos",
            description: "Personnage filmé de dos.",
            sounds: "Soulignage pointillé à l’export",
            usage: "Pour un personnage présent de dos ou dont la bouche n’est pas visible.",
            quick_label: "Marquer la réplique comme de dos",
            mouth: Mouth::EhAe,
        },
        Sign::RemoveUnderline => Info {
            title: "Retirer le soulignage",
            description: "Rétablir la ligne comme une réplique active.",
            sounds: "Ligne active",
            usage: "Pour retirer le statut OFF ou de dos de la réplique sélectionnée.",
            quick_label: "Retirer le soulignage",
            mouth: Mouth::EhAe,
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
    line_id: Option<u64>,
    line_presence: Option<LinePresence>,
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
        line_id: Option<u64>,
        line_presence: Option<LinePresence>,
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
    let column = index % MENU_COLUMNS;
    let row = index / MENU_COLUMNS;
    Rect {
        x: outer.x + MENU_PADDING + column as f32 * (MENU_ICON_SIZE + MENU_GAP),
        y: outer.y + MENU_PADDING + row as f32 * (MENU_ICON_SIZE + MENU_GAP),
        width: MENU_ICON_SIZE,
        height: MENU_ICON_SIZE,
    }
}

fn palette_item_at(
    outer: Rect,
    x: f32,
    y: f32,
    line_presence: Option<LinePresence>,
) -> Option<usize> {
    palette_signs(line_presence)
        .iter()
        .enumerate()
        .find_map(|(index, _)| {
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
    let ppf = crate::constants::PIXELS_PER_FRAME * project.settings().scroll_speed;
    Some(
        zone.x
            + zone.width / 2.0
            + (cue.media_tick.as_frame_position() - current_frame) as f32 * ppf
            + 18.0,
    )
}

fn moved_index(current: usize, direction: i32, count: usize) -> usize {
    (current as i32 + direction).rem_euclid(count as i32) as usize
}

fn dismiss(kind: PopupKind, suppressed: Rect) {
    lock_state().popup = Popup::Dismissed { kind, suppressed };
}

fn activate_palette_choice(
    track: u8,
    media_tick: MediaTick,
    selected: usize,
    suppressed: Rect,
    line_id: Option<u64>,
    line_presence: Option<LinePresence>,
) -> EventResponse {
    let signs = palette_signs(line_presence);
    let sign = signs[selected.min(signs.len() - 1)];
    dismiss(PopupKind::Palette, suppressed);
    if matches!(sign, Sign::Off | Sign::Back | Sign::RemoveUnderline) {
        let Some(line_id) = line_id else {
            return EventResponse::Consumed;
        };
        return EventResponse::Action(UiAction::SetLinePresence {
            line_id,
            presence: if sign == Sign::Off {
                LinePresence::Off
            } else if sign == Sign::Back {
                LinePresence::Back
            } else {
                LinePresence::On
            },
        });
    }
    let (kind, target) = sign.storage().expect("detection sign");
    EventResponse::Action(UiAction::AddDetection {
        line_id: track_storage_line_id(track),
        kind,
        media_tick,
        target,
    })
}

fn announce_palette_selection(
    selected: usize,
    line_presence: Option<LinePresence>,
) -> EventResponse {
    let signs = palette_signs(line_presence);
    let details = info(signs[selected.min(signs.len() - 1)]);
    EventResponse::Action(UiAction::Accessibility(AccessibilityEvent::Selection {
        label: format!(
            "{}. {}",
            details.quick_label,
            details.usage.replace('\n', " ")
        ),
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
            line_id,
            line_presence,
        } => match event {
            UiEvent::CursorLeft | UiEvent::CursorUp => {
                let next = moved_index(selected, -1, palette_signs(line_presence).len());
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = next;
                }
                Some(announce_palette_selection(next, line_presence))
            }
            UiEvent::CursorRight | UiEvent::CursorDown => {
                let next = moved_index(selected, 1, palette_signs(line_presence).len());
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = next;
                }
                Some(announce_palette_selection(next, line_presence))
            }
            UiEvent::Home => {
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = 0;
                }
                Some(announce_palette_selection(0, line_presence))
            }
            UiEvent::End => {
                let last = palette_signs(line_presence).len() - 1;
                if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                    *selected = last;
                }
                Some(announce_palette_selection(last, line_presence))
            }
            UiEvent::Activate => Some(activate_palette_choice(
                track,
                media_tick,
                selected,
                suppressed,
                line_id,
                line_presence,
            )),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                Some(activate_palette_choice(
                    track,
                    media_tick,
                    selected,
                    suppressed,
                    line_id,
                    line_presence,
                ))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                dismiss(PopupKind::Palette, suppressed);
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseMove { x, y } => {
                if let Some(index) = palette_item_at(visual, *x, *y, line_presence) {
                    if let Popup::Palette { selected, .. } = &mut lock_state().popup {
                        *selected = index;
                    }
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MousePress { x, y } => {
                if let Some(index) = palette_item_at(visual, *x, *y, line_presence) {
                    return Some(activate_palette_choice(
                        track,
                        media_tick,
                        index,
                        suppressed,
                        line_id,
                        line_presence,
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
    let hover = state.detection_hover.map(|hover| {
        let line = project.lines().find(|line| {
            crate::rythmo_layout::track_index_for_y_slot(line.y_slot) == hover.track as usize
                && hover.media_tick.as_frame_position() >= line.start_frame as f64
                && hover.media_tick.as_frame_position() <= line.end_frame() as f64
        });
        HoverAnchor {
            track: hover.track,
            media_tick: hover.media_tick,
            screen_x: hover.screen_x,
            screen_y: hover.screen_y,
            track_rect: hover.track_rect,
            line_id: line.map(|line| line.id),
            line_presence: line.map(|line| line.presence),
        }
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
        selected = selected.min(palette_signs(hover.line_presence).len() - 1);
        if let Some((x, y)) = pointer {
            if let Some(index) = palette_item_at(visual, x, y, hover.line_presence) {
                selected = index;
            }
        }
        foreground.popup = Popup::Palette {
            visual,
            suppressed,
            track: hover.track,
            media_tick: hover.media_tick,
            selected,
            line_id: hover.line_id,
            line_presence: hover.line_presence,
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
    let layout = info_layout(info(sign), (zone.width - 16.0).max(1.0));
    let outer = clamp_popup(
        Rect {
            x: x + 8.0,
            y: y - layout.height - 8.0,
            width: layout.width,
            height: layout.height,
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
        line_id: hover.line_id,
        line_presence: hover.line_presence,
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
        "Fiche de détection. {}. Description : {} Sons correspondants : {}. {}",
        details.title,
        details.description,
        details.sounds,
        details.usage.replace('\n', " ")
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

fn text_size(text: &str, size: f32, width: f32) -> (f32, f32) {
    crate::ui::renderer::measure_wrapped_text(text, size, width)
}

fn push_wrapped<'a>(
    labels: &mut Vec<LabelInfo<'a>>,
    text: &'static str,
    bounds: Rect,
    size: f32,
    color: [u8; 3],
) {
    push_label(labels, text, bounds, size, color, HAlign::Left, VAlign::Top);
    labels.last_mut().unwrap().overflow = Overflow::Wrap;
}

#[derive(Clone, Copy)]
struct InfoLayout {
    width: f32,
    height: f32,
    image: Rect,
    title: Rect,
    description_heading: Rect,
    description: Rect,
    sounds_heading: Rect,
    sounds: Rect,
    usage: Rect,
}

fn info_layout(details: Info, available_width: f32) -> InfoLayout {
    let natural_text_width = [
        text_size(details.title, 18.0, 100_000.0).0,
        text_size(details.description, 13.0, 100_000.0).0,
        text_size(details.sounds, 13.0, 100_000.0).0,
    ]
    .into_iter()
    .fold(0.0, f32::max);
    let preferred = (natural_text_width + INFO_IMAGE_SIZE + 14.0 + INFO_PADDING * 2.0)
        .max(text_size(details.usage, 13.0, 100_000.0).0 + INFO_PADDING * 2.0)
        .clamp(280.0, INFO_WIDTH);
    let width = preferred.min(available_width.max(1.0));
    let content_width = (width - INFO_PADDING * 2.0).max(1.0);
    let stacked = width < 340.0;
    let image_size = INFO_IMAGE_SIZE.min(content_width);
    let image = Rect {
        x: INFO_PADDING,
        y: INFO_PADDING,
        width: image_size,
        height: image_size,
    };
    let text_x = if stacked {
        INFO_PADDING
    } else {
        INFO_PADDING + image_size + 14.0
    };
    let text_width = if stacked {
        content_width
    } else {
        (width - INFO_PADDING - text_x).max(1.0)
    };
    let mut y = if stacked {
        INFO_PADDING + image_size + 12.0
    } else {
        INFO_PADDING
    };
    let title = Rect {
        x: text_x,
        y,
        width: text_width,
        height: text_size(details.title, 18.0, text_width).1,
    };
    y += title.height + 12.0;
    let description_heading = Rect {
        x: text_x,
        y,
        width: text_width,
        height: text_size(INFO_DESCRIPTION_HEADING, 11.0, text_width).1,
    };
    y += description_heading.height + 4.0;
    let description = Rect {
        x: text_x,
        y,
        width: text_width,
        height: text_size(details.description, 13.0, text_width).1,
    };
    y += description.height + 12.0;
    let sounds_heading = Rect {
        x: text_x,
        y,
        width: text_width,
        height: text_size(INFO_SOUNDS_HEADING, 11.0, text_width).1,
    };
    y += sounds_heading.height + 4.0;
    let sounds = Rect {
        x: text_x,
        y,
        width: text_width,
        height: text_size(details.sounds, 13.0, text_width).1,
    };
    y = (y + sounds.height).max(image.y + image.height) + 14.0;
    let usage = Rect {
        x: INFO_PADDING,
        y,
        width: content_width,
        height: text_size(details.usage, 13.0, content_width).1,
    };
    InfoLayout {
        width,
        height: y + usage.height + INFO_PADDING,
        image,
        title,
        description_heading,
        description,
        sounds_heading,
        sounds,
        usage,
    }
}

fn translated(rect: Rect, outer: Rect) -> Rect {
    Rect {
        x: outer.x + rect.x,
        y: outer.y + rect.y,
        ..rect
    }
}

#[derive(Clone, Copy)]
struct TooltipLayout {
    width: f32,
    height: f32,
    image: Rect,
    title: Rect,
    usage: Rect,
}

fn tooltip_layout(details: Info, available_width: f32) -> TooltipLayout {
    let preferred = (text_size(details.quick_label, 13.0, 100_000.0).0 + 102.0)
        .max(text_size(details.usage, 13.0, 100_000.0).0 + 24.0)
        .clamp(220.0, TOOLTIP_WIDTH);
    let width = preferred.min(available_width.max(1.0));
    let content_width = (width - 24.0).max(1.0);
    let image_size = 80.0_f32.min(content_width);
    let image = Rect {
        x: 12.0,
        y: 12.0,
        width: image_size,
        height: image_size,
    };
    let stacked = width < 220.0;
    let title_x = if stacked {
        12.0
    } else {
        image.x + image.width + 10.0
    };
    let title_width = if stacked {
        content_width
    } else {
        (width - 12.0 - title_x).max(1.0)
    };
    let title = Rect {
        x: title_x,
        y: if stacked {
            image.y + image.height + 8.0
        } else {
            12.0
        },
        width: title_width,
        height: text_size(details.quick_label, 13.0, title_width).1,
    };
    let usage = Rect {
        x: 12.0,
        y: (image.y + image.height).max(title.y + title.height) + 10.0,
        width: content_width,
        height: text_size(details.usage, 13.0, content_width).1,
    };
    TooltipLayout {
        width,
        height: usage.y + usage.height + 12.0,
        image,
        title,
        usage,
    }
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

fn pucker_bitmap() -> &'static MouthBitmap {
    static BITMAP: OnceLock<MouthBitmap> = OnceLock::new();
    BITMAP.get_or_init(|| {
        let tree = resvg::usvg::Tree::from_data(
            include_bytes!("icons/cul_de_poule.svg"),
            &resvg::usvg::Options::default(),
        )
        .expect("cul_de_poule.svg should parse");
        let width = MENU_ICON_SIZE as u32;
        let height = MENU_ICON_SIZE as u32;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
        let size = tree.size();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / size.width(),
                height as f32 / size.height(),
            ),
            &mut pixmap.as_mut(),
        );
        let pixels = pixmap
            .data()
            .chunks_exact(4)
            .map(|pixel| {
                let alpha = pixel[3];
                [alpha, alpha, alpha, alpha]
            })
            .collect();
        MouthBitmap {
            width,
            height,
            pixels,
        }
    })
}

/// Draw a pixel-aligned, already-downsampled mouth. Every run is at least one
/// logical pixel high, avoiding the sub-pixel SDF attenuation that darkened the
/// previous reconstruction.
fn render_mouth(quads: &mut Vec<QuadInstance>, rect: Rect, mouth: Mouth) {
    render_bitmap(quads, rect, mouth_bitmap(mouth));
}

fn render_mouth_scaled(quads: &mut Vec<QuadInstance>, rect: Rect, mouth: Mouth) {
    render_bitmap_scaled(quads, rect, mouth_bitmap(mouth));
}

fn render_bitmap(quads: &mut Vec<QuadInstance>, rect: Rect, bitmap: &MouthBitmap) {
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

fn render_bitmap_scaled(quads: &mut Vec<QuadInstance>, rect: Rect, bitmap: &MouthBitmap) {
    let width = rect.width.round().max(1.0) as u32;
    let height = rect.height.round().max(1.0) as u32;
    if bitmap.width == 0 || bitmap.height == 0 {
        return;
    }

    for y in 0..height {
        let source_y = (y * bitmap.height / height).min(bitmap.height - 1);
        let mut x = 0;
        while x < width {
            let source_x = (x * bitmap.width / width).min(bitmap.width - 1);
            let pixel = bitmap.pixels[(source_y * bitmap.width + source_x) as usize];
            let start = x;
            x += 1;
            while x < width {
                let next_x = (x * bitmap.width / width).min(bitmap.width - 1);
                if bitmap.pixels[(source_y * bitmap.width + next_x) as usize] != pixel {
                    break;
                }
                x += 1;
            }
            if pixel[3] >= 8 {
                push_flat_quad(
                    quads,
                    Rect {
                        x: rect.x + start as f32,
                        y: rect.y + y as f32,
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
            visual,
            selected,
            line_presence,
            ..
        } => {
            let outer = clamp_popup(visual, screen);
            push_panel_quad(quads, outer, [0.035, 0.039, 0.052, 0.999], 8.0);
            let signs = palette_signs(line_presence);
            for (index, sign) in signs.iter().copied().enumerate() {
                let item = palette_item_rect(outer, index);
                if selected == index {
                    push_panel_quad(quads, item, [0.18, 0.32, 0.58, 0.99], 5.0);
                }
                if sign == Sign::Pucker {
                    render_bitmap(quads, item, pucker_bitmap());
                } else {
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
            }

            let details = info(signs[selected.min(signs.len() - 1)]);
            let layout = tooltip_layout(details, (screen_w - 16.0).max(1.0));
            let tooltip_y = if outer.y + outer.height + TOOLTIP_GAP + layout.height <= screen_h {
                outer.y + outer.height + TOOLTIP_GAP
            } else {
                outer.y - layout.height - TOOLTIP_GAP
            };
            let tooltip = clamp_popup(
                Rect {
                    x: palette_item_rect(outer, selected).x + MENU_ICON_SIZE / 2.0
                        - layout.width / 2.0,
                    y: tooltip_y,
                    width: layout.width,
                    height: layout.height,
                },
                screen,
            );
            push_panel_quad(quads, tooltip, [0.025, 0.028, 0.038, 0.999], 6.0);
            render_mouth_scaled(quads, translated(layout.image, tooltip), details.mouth);
            push_wrapped(
                labels,
                details.quick_label,
                translated(layout.title, tooltip),
                13.0,
                [245, 247, 252],
            );
            push_wrapped(
                labels,
                details.usage,
                translated(layout.usage, tooltip),
                13.0,
                [242, 244, 249],
            );
        }
        Popup::Info { visual, sign, .. } => {
            let details = info(sign);
            let layout = info_layout(details, (screen_w - 16.0).max(1.0));
            let outer = clamp_popup(
                Rect {
                    width: layout.width,
                    height: layout.height,
                    ..visual
                },
                screen,
            );
            // Keep pointer dismissal aligned with the actual card after a resize.
            if let Popup::Info { visual, .. } = &mut lock_state().popup {
                *visual = outer;
            }
            push_panel_quad(quads, outer, [0.026, 0.030, 0.042, 0.999], 11.0);
            let image = translated(layout.image, outer);
            push_panel_quad(quads, image, [0.10, 0.11, 0.14, 1.0], 8.0);
            render_mouth(quads, image, details.mouth);
            push_wrapped(
                labels,
                details.title,
                translated(layout.title, outer),
                18.0,
                [246, 248, 253],
            );
            push_wrapped(
                labels,
                INFO_DESCRIPTION_HEADING,
                translated(layout.description_heading, outer),
                11.0,
                [142, 164, 202],
            );
            push_wrapped(
                labels,
                details.description,
                translated(layout.description, outer),
                13.0,
                [222, 227, 238],
            );
            push_wrapped(
                labels,
                INFO_SOUNDS_HEADING,
                translated(layout.sounds_heading, outer),
                11.0,
                [142, 164, 202],
            );
            push_wrapped(
                labels,
                details.sounds,
                translated(layout.sounds, outer),
                13.0,
                [242, 244, 249],
            );
            push_wrapped(
                labels,
                details.usage,
                translated(layout.usage, outer),
                13.0,
                [242, 244, 249],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_panels_fit_all_content_at_wide_and_narrow_widths() {
        for sign in Sign::ALL.into_iter().chain([Sign::RemoveUnderline]) {
            let details = info(sign);
            assert!(!details.usage.contains('\n'));
            for width in [160.0, 280.0, 360.0, 470.0] {
                let card = info_layout(details, width);
                assert!(card.width <= width);
                for rect in [
                    card.image,
                    card.title,
                    card.description_heading,
                    card.description,
                    card.sounds_heading,
                    card.sounds,
                    card.usage,
                ] {
                    assert!(rect.x + rect.width <= card.width);
                    assert!(rect.y + rect.height + INFO_PADDING <= card.height);
                }
                assert!(
                    card.description.y
                        >= card.description_heading.y + card.description_heading.height
                );
                assert!(card.sounds.y >= card.sounds_heading.y + card.sounds_heading.height);
                assert!(card.usage.y >= card.sounds.y + card.sounds.height);
                assert!(card.usage.y >= card.image.y + card.image.height);
                assert_eq!(
                    card.usage.height,
                    text_size(details.usage, 13.0, card.usage.width).1
                );

                let tooltip = tooltip_layout(details, width);
                assert!(tooltip.width <= width);
                assert!(tooltip.usage.y >= tooltip.title.y + tooltip.title.height);
                assert!(tooltip.usage.y >= tooltip.image.y + tooltip.image.height);
                assert_eq!(
                    tooltip.height,
                    tooltip.usage.y + tooltip.usage.height + 12.0
                );
            }
        }
    }

    #[test]
    fn panel_height_adapts_to_text_and_available_width() {
        let short = tooltip_layout(info(Sign::Pucker), 420.0);
        let long = tooltip_layout(info(Sign::DentalTh), 420.0);
        let narrow = tooltip_layout(info(Sign::DentalTh), 160.0);
        assert!(long.height > short.height);
        assert!(narrow.height > long.height);
        assert!(narrow.width < long.width);
    }

    #[test]
    fn palette_navigation_wraps() {
        assert_eq!(moved_index(0, -1, Sign::ALL.len()), Sign::ALL.len() - 1);
        assert_eq!(moved_index(Sign::ALL.len() - 1, 1, Sign::ALL.len()), 0);
    }

    #[test]
    fn line_palette_replaces_active_presence_with_remove_action() {
        let off = palette_signs(Some(LinePresence::Off));
        assert!(!off.contains(&Sign::Off));
        assert!(off.contains(&Sign::Back));
        assert_eq!(off.last(), Some(&Sign::RemoveUnderline));

        let back = palette_signs(Some(LinePresence::Back));
        assert!(!back.contains(&Sign::Back));
        assert!(back.contains(&Sign::Off));
        assert_eq!(back.last(), Some(&Sign::RemoveUnderline));
    }

    #[test]
    fn waves_are_grouped_on_the_second_palette_row() {
        let outer = Rect {
            x: 0.0,
            y: 0.0,
            width: MENU_WIDTH,
            height: MENU_HEIGHT,
        };
        let opening = Sign::ALL
            .iter()
            .position(|sign| *sign == Sign::MouthOpen)
            .unwrap();
        let forward = Sign::ALL
            .iter()
            .position(|sign| *sign == Sign::ForwardWave)
            .unwrap();
        let pucker = Sign::ALL
            .iter()
            .position(|sign| *sign == Sign::Pucker)
            .unwrap();
        assert_eq!(
            palette_item_rect(outer, opening).y,
            palette_item_rect(outer, forward).y
        );
        assert_eq!(
            palette_item_rect(outer, opening).y,
            palette_item_rect(outer, pucker).y
        );
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
            height: info_layout(info(Sign::MouthOpen), INFO_WIDTH).height,
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
