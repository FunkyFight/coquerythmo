//! Final foreground layer for detection palettes and information cards.
//!
//! Detection signs stay in the rythmo layer. Only the Alt+D palette, its quick
//! tooltip and the information card are mirrored into the last modal-overlay
//! pass. The module also provides the complete semantic label announced by
//! AccessKit when a card opens.

use crate::detection::{DetectionAddress, DetectionCue, TextAnchor};
use crate::project::Project;
use crate::ui::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::workspaces::rythmo::view::{editor_track_body_rect_at_frame, RythmoState, Selection};
use std::sync::{Mutex, OnceLock};

const SIGN_SIZE: f32 = 26.0;
const SIGN_BOTTOM_MARGIN: f32 = 2.0;
const BUTTON_SIZE: f32 = 18.0;
const BUTTON_GAP: f32 = 4.0;
const MENU_ICON_SIZE: f32 = 30.0;
const MENU_GAP: f32 = 4.0;
const MENU_PADDING: f32 = 6.0;
const MENU_HEIGHT: f32 = MENU_ICON_SIZE + MENU_PADDING * 2.0;
const INFO_WIDTH: f32 = 470.0;
const INFO_HEIGHT: f32 = 176.0;
const INFO_PADDING: f32 = 12.0;
const INFO_IMAGE_SIZE: f32 = 136.0;
const TOOLTIP_WIDTH: f32 = 350.0;
const TOOLTIP_HEIGHT: f32 = 30.0;
const MOUTH_PIXELS: u32 = 44;

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

    fn from_cue(cue: &DetectionCue) -> Option<Self> {
        let alternate = matches!(&cue.target, TextAnchor::AfterText);
        match (cue.kind, alternate) {
            (crate::detection::DetectionKind::Labial, _) => Some(Self::Labial),
            (crate::detection::DetectionKind::SemiLabial, _) => Some(Self::SemiLabial),
            (crate::detection::DetectionKind::MouthOpen, _) => Some(Self::MouthOpen),
            (crate::detection::DetectionKind::MouthClosed, _) => Some(Self::MouthClosed),
            (crate::detection::DetectionKind::TeethVisible, false) => Some(Self::TeethVisible),
            (crate::detection::DetectionKind::TeethVisible, true) => Some(Self::DentalTh),
            (crate::detection::DetectionKind::Breath, false) => Some(Self::Breath),
            (crate::detection::DetectionKind::Breath, true) => Some(Self::Neutral),
            (crate::detection::DetectionKind::Reaction, _) => Some(Self::Reaction),
            (crate::detection::DetectionKind::TextSyncPoint, _) => None,
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
enum Popup {
    None,
    Palette { outer: Rect, hover_index: Option<usize> },
    Info { outer: Rect, sign: Sign },
}

#[derive(Clone, Copy)]
struct ForegroundState {
    popup: Popup,
    last_palette_outer: Option<Rect>,
    selected_anchor_x: Option<f32>,
}

impl Default for ForegroundState {
    fn default() -> Self {
        Self {
            popup: Popup::None,
            last_palette_outer: None,
            selected_anchor_x: None,
        }
    }
}

fn foreground() -> &'static Mutex<ForegroundState> {
    static STATE: OnceLock<Mutex<ForegroundState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ForegroundState::default()))
}

fn lock_state() -> std::sync::MutexGuard<'static, ForegroundState> {
    foreground().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn ppf() -> f32 {
    crate::constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

fn tick_x(tick: crate::detection::MediaTick, current_frame: f64, zone: Rect) -> f32 {
    zone.x + zone.width / 2.0 + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn clamp_popup(mut rect: Rect, zone: Rect) -> Rect {
    rect.x = rect.x.clamp(zone.x, (zone.x + zone.width - rect.width).max(zone.x));
    rect.y = rect.y.clamp(zone.y, (zone.y + zone.height - rect.height).max(zone.y));
    rect
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

fn selected_address(state: &RythmoState) -> Option<DetectionAddress> {
    match state.selected.as_ref() {
        Some(Selection::Detection(address)) => Some(*address),
        _ => None,
    }
}

fn palette_outer(state: &RythmoState, zone: Rect) -> Option<Rect> {
    let hover = state.detection_hover?;
    let button = Rect {
        x: hover.screen_x - BUTTON_SIZE / 2.0,
        y: hover.track_rect.y + hover.track_rect.height + BUTTON_GAP,
        width: BUTTON_SIZE,
        height: BUTTON_SIZE,
    };
    Some(clamp_popup(
        Rect {
            x: button.x,
            y: button.y + button.height + 2.0,
            width: MENU_PADDING * 2.0
                + MENU_ICON_SIZE * Sign::ALL.len() as f32
                + MENU_GAP * (Sign::ALL.len() as f32 - 1.0),
            height: MENU_HEIGHT,
        },
        zone,
    ))
}

fn selected_sign_anchor(
    project: &Project,
    state: &RythmoState,
    zone: Rect,
    current_frame: f64,
) -> Option<f32> {
    let address = selected_address(state)?;
    let track = address.track()? as usize;
    let cue = project.detections().detection(address)?;
    let track_rect = editor_track_body_rect_at_frame(
        project,
        crate::rythmo_layout::y_slot_for_track_index(track),
        current_frame,
        &zone,
    );
    let center = tick_x(cue.media_tick, current_frame, zone);
    let badge_y = (track_rect.y + track_rect.height - SIGN_SIZE - SIGN_BOTTOM_MARGIN)
        .max(track_rect.y);
    let _ = badge_y;
    Some(center + SIGN_SIZE / 2.0 + 8.0)
}

/// Mirror interaction state after each rythmo event.
pub fn sync_from_state(
    project: &Project,
    state: &RythmoState,
    zone: Rect,
    current_frame: f64,
    event: &UiEvent,
) {
    let mut foreground = lock_state();
    let previous_popup = foreground.popup;
    foreground.selected_anchor_x = selected_sign_anchor(project, state, zone, current_frame);
    if let Some(outer) = palette_outer(state, zone) {
        foreground.last_palette_outer = Some(outer);
    }

    if state.detection_menu.is_none() {
        foreground.popup = Popup::None;
        return;
    }

    if state.detection_hover.is_some() {
        let Some(outer) = palette_outer(state, zone).or(foreground.last_palette_outer) else {
            foreground.popup = Popup::None;
            return;
        };
        let hover_index = event_pointer(event).and_then(|(x, y)| {
            Sign::ALL.iter().enumerate().find_map(|(index, _)| {
                let item = Rect {
                    x: outer.x + MENU_PADDING + index as f32 * (MENU_ICON_SIZE + MENU_GAP),
                    y: outer.y + MENU_PADDING,
                    width: MENU_ICON_SIZE,
                    height: MENU_ICON_SIZE,
                };
                item.contains(x, y).then_some(index)
            })
        });
        foreground.popup = Popup::Palette { outer, hover_index };
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
    let preserved = match previous_popup {
        Popup::Info { outer, sign: previous_sign } if previous_sign == sign => Some(outer),
        _ => None,
    };
    let outer = match event {
        UiEvent::MouseRelease { x, y } => clamp_popup(
            Rect {
                x: *x + 8.0,
                y: *y - INFO_HEIGHT - 8.0,
                width: INFO_WIDTH,
                height: INFO_HEIGHT,
            },
            zone,
        ),
        _ => preserved.unwrap_or_else(|| {
            clamp_popup(
                Rect {
                    x: foreground
                        .selected_anchor_x
                        .unwrap_or(zone.x + zone.width / 2.0),
                    y: zone.y + zone.height / 2.0 - INFO_HEIGHT / 2.0,
                    width: INFO_WIDTH,
                    height: INFO_HEIGHT,
                },
                zone,
            )
        }),
    };
    foreground.popup = Popup::Info { outer, sign };
}

/// Called by the Alt+D path, which opens the palette without a pointer event.
pub fn activate_palette() {
    let mut state = lock_state();
    if let Some(outer) = state.last_palette_outer {
        state.popup = Popup::Palette { outer, hover_index: None };
    }
}

pub fn clear() {
    *lock_state() = ForegroundState::default();
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

fn mouth_pixels(mouth: Mouth) -> &'static Vec<[u8; 4]> {
    static AA: OnceLock<Vec<[u8; 4]>> = OnceLock::new();
    static EH_AE: OnceLock<Vec<[u8; 4]>> = OnceLock::new();
    static FV: OnceLock<Vec<[u8; 4]>> = OnceLock::new();
    static KST_EE: OnceLock<Vec<[u8; 4]>> = OnceLock::new();
    static PBM: OnceLock<Vec<[u8; 4]>> = OnceLock::new();
    static UW_OW_W: OnceLock<Vec<[u8; 4]>> = OnceLock::new();

    fn decode(bytes: &[u8]) -> Vec<[u8; 4]> {
        let source = image::load_from_memory(bytes)
            .expect("Rhubarb mouth PNG should decode")
            .to_rgba8();
        let resized = image::imageops::resize(
            &source,
            MOUTH_PIXELS,
            MOUTH_PIXELS,
            image::imageops::FilterType::Triangle,
        );
        resized
            .as_raw()
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect()
    }

    match mouth {
        Mouth::Aa => AA.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/AA.png"))),
        Mouth::EhAe => EH_AE.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/EH_AE.png"))),
        Mouth::Fv => FV.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/F_V.png"))),
        Mouth::KstEe => KST_EE.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/K_S_T_EE.png"))),
        Mouth::Pbm => PBM.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/P_B_M.png"))),
        Mouth::UwOwW => UW_OW_W.get_or_init(|| decode(include_bytes!("icons/detection/rhubarb_lips/UW_OW_W.png"))),
    }
}

fn render_mouth(quads: &mut Vec<QuadInstance>, rect: Rect, mouth: Mouth) {
    let pixels = mouth_pixels(mouth);
    let pixel_w = rect.width / MOUTH_PIXELS as f32;
    let pixel_h = rect.height / MOUTH_PIXELS as f32;
    for y in 0..MOUTH_PIXELS {
        for x in 0..MOUTH_PIXELS {
            let pixel = pixels[(y * MOUTH_PIXELS + x) as usize];
            if pixel[3] < 12 {
                continue;
            }
            push_flat_quad(
                quads,
                Rect {
                    x: rect.x + x as f32 * pixel_w,
                    y: rect.y + y as f32 * pixel_h,
                    width: pixel_w + 0.15,
                    height: pixel_h + 0.15,
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

/// Append palette and card visuals after every other UI layer.
pub fn append_foreground<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    screen_w: f32,
    screen_h: f32,
) {
    let snapshot = *lock_state();
    let screen = Rect { x: 0.0, y: 0.0, width: screen_w, height: screen_h };
    match snapshot.popup {
        Popup::None => {}
        Popup::Palette { outer, hover_index } => {
            let outer = clamp_popup(outer, screen);
            push_panel_quad(quads, outer, [0.035, 0.039, 0.052, 0.999], 8.0);
            for (index, sign) in Sign::ALL.iter().copied().enumerate() {
                let item = Rect {
                    x: outer.x + MENU_PADDING + index as f32 * (MENU_ICON_SIZE + MENU_GAP),
                    y: outer.y + MENU_PADDING,
                    width: MENU_ICON_SIZE,
                    height: MENU_ICON_SIZE,
                };
                if hover_index == Some(index) {
                    push_panel_quad(quads, item, [0.18, 0.32, 0.58, 0.99], 5.0);
                }
                push_label(
                    labels,
                    sign.glyph(),
                    item,
                    if matches!(sign, Sign::DentalTh | Sign::Neutral) { 13.0 } else { 20.0 },
                    [244, 246, 252],
                    HAlign::Center,
                    VAlign::Center,
                );
            }
            if let Some(index) = hover_index {
                let details = info(Sign::ALL[index]);
                let tooltip_y = if outer.y + outer.height + 6.0 + TOOLTIP_HEIGHT <= screen_h {
                    outer.y + outer.height + 6.0
                } else {
                    (outer.y - TOOLTIP_HEIGHT - 6.0).max(0.0)
                };
                let tooltip = Rect {
                    x: (outer.x + MENU_PADDING
                        + index as f32 * (MENU_ICON_SIZE + MENU_GAP)
                        + MENU_ICON_SIZE / 2.0
                        - TOOLTIP_WIDTH / 2.0)
                        .clamp(0.0, (screen_w - TOOLTIP_WIDTH).max(0.0)),
                    y: tooltip_y,
                    width: TOOLTIP_WIDTH.min(screen_w),
                    height: TOOLTIP_HEIGHT,
                };
                push_panel_quad(quads, tooltip, [0.025, 0.028, 0.038, 0.999], 6.0);
                push_label(labels, details.quick_label, tooltip, 13.0, [245, 247, 252], HAlign::Center, VAlign::Center);
            }
        }
        Popup::Info { outer, sign } => {
            let outer = clamp_popup(outer, screen);
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
            push_label(labels, details.title, Rect { x: text_x, y: outer.y + 12.0, width: text_width, height: 28.0 }, 18.0, [246, 248, 253], HAlign::Left, VAlign::Center);
            push_label(labels, "Description", Rect { x: text_x, y: outer.y + 48.0, width: text_width, height: 18.0 }, 11.0, [142, 164, 202], HAlign::Left, VAlign::Center);
            push_label(labels, details.description, Rect { x: text_x, y: outer.y + 66.0, width: text_width, height: 28.0 }, 13.0, [222, 227, 238], HAlign::Left, VAlign::Center);
            push_label(labels, "Sons correspondants", Rect { x: text_x, y: outer.y + 104.0, width: text_width, height: 18.0 }, 11.0, [142, 164, 202], HAlign::Left, VAlign::Center);
            push_label(labels, details.sounds, Rect { x: text_x, y: outer.y + 122.0, width: text_width, height: 38.0 }, 13.0, [242, 244, 249], HAlign::Left, VAlign::Top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_tooltip_contains_name_and_sounds() {
        assert_eq!(info(Sign::Labial).quick_label, "Labiale (P, B, M)");
        assert!(info(Sign::DentalTh).quick_label.contains("TH ("));
    }

    #[test]
    fn accessibility_text_contains_the_entire_card() {
        let details = info(Sign::Reaction);
        let label = format!(
            "Fiche de détection. {}. Description : {} Sons correspondants : {}.",
            details.title, details.description, details.sounds
        );
        assert!(label.contains(details.title));
        assert!(label.contains(details.description));
        assert!(label.contains(details.sounds));
    }
}
