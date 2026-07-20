from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(content, encoding="utf-8", newline="\n")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one occurrence, found {count}: {old[:100]!r}")
    write(path, text.replace(old, new, 1))


# ---------------------------------------------------------------------------
# Domain helpers
# ---------------------------------------------------------------------------
replace_once(
    "src/detection.rs",
    """impl DetectionKind {
    pub const fn family(self) -> DetectionFamily {
""",
    """impl DetectionKind {
    pub const ALL: [Self; 17] = [
        Self::Labial,
        Self::SemiLabial,
        Self::MouthOpen,
        Self::MouthClosed,
        Self::TeethVisible,
        Self::Breath,
        Self::Reaction,
        Self::SentenceStart,
        Self::SentenceEnd,
        Self::OverlapStart,
        Self::OverlapEnd,
        Self::SpeakerChange,
        Self::OffScreen,
        Self::VoiceOver,
        Self::Telephone,
        Self::Thought,
        Self::Crowd,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Labial => "Labiale",
            Self::SemiLabial => "Semi-labiale",
            Self::MouthOpen => "Bouche ouverte",
            Self::MouthClosed => "Bouche fermée",
            Self::TeethVisible => "Dents visibles",
            Self::Breath => "Respiration",
            Self::Reaction => "Réaction",
            Self::SentenceStart => "Début de phrase",
            Self::SentenceEnd => "Fin de phrase",
            Self::OverlapStart => "Début de chevauchement",
            Self::OverlapEnd => "Fin de chevauchement",
            Self::SpeakerChange => "Changement d'interlocuteur",
            Self::OffScreen => "Hors champ",
            Self::VoiceOver => "Voix off",
            Self::Telephone => "Téléphone",
            Self::Thought => "Pensée",
            Self::Crowd => "Foule",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Labial => "L",
            Self::SemiLabial => "SL",
            Self::MouthOpen => "BO",
            Self::MouthClosed => "BF",
            Self::TeethVisible => "DV",
            Self::Breath => "R",
            Self::Reaction => "!",
            Self::SentenceStart => "DÉB",
            Self::SentenceEnd => "FIN",
            Self::OverlapStart => "CH+",
            Self::OverlapEnd => "CH-",
            Self::SpeakerChange => "INT",
            Self::OffScreen => "HC",
            Self::VoiceOver => "OFF",
            Self::Telephone => "TEL",
            Self::Thought => "PEN",
            Self::Crowd => "FOU",
        }
    }

    pub const fn family(self) -> DetectionFamily {
""",
)

replace_once(
    "src/detection.rs",
    """    pub fn validate(&self) -> Result<(), String> {
        for (line_id, line) in &self.lines {
""",
    """    pub fn scaled_time(&self, ratio: f64) -> Self {
        let mut scaled = Self::default();
        for (line_id, line) in &self.lines {
            scaled.lines.insert(*line_id, line.scaled_time(ratio));
        }
        scaled
    }

    pub fn validate(&self) -> Result<(), String> {
        for (line_id, line) in &self.lines {
""",
)

# ---------------------------------------------------------------------------
# Store detections with each language band through ProjectSettings
# ---------------------------------------------------------------------------
replace_once(
    "src/project.rs",
    """    #[serde(default, skip_serializing_if = "is_default_automation_graph")]
    pub automation: crate::automation::AutomationGraph,
""",
    """    #[serde(
        default,
        skip_serializing_if = "crate::detection::DetectionDocument::is_empty"
    )]
    pub detections: crate::detection::DetectionDocument,
    #[serde(default, skip_serializing_if = "is_default_automation_graph")]
    pub automation: crate::automation::AutomationGraph,
""",
)

replace_once(
    "src/project.rs",
    """    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn syllable_language(&self) -> SyllableLanguage {
""",
    """    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn detections(&self) -> &crate::detection::DetectionDocument {
        &self.settings.detections
    }

    pub(crate) fn apply_detection_change(
        &mut self,
        change: &crate::detection::DetectionChange,
        forward: bool,
    ) -> bool {
        let changed = if forward {
            change.apply(&mut self.settings.detections)
        } else {
            change.unapply(&mut self.settings.detections)
        };
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn syllable_language(&self) -> SyllableLanguage {
""",
)

replace_once(
    "src/project.rs",
    """    pub fn clear_lines(&mut self) {
        self.line_map.clear();
        self.line_order.clear();
        self.known_characters.clear();
        self.bump_revision();
    }
""",
    """    pub fn clear_lines(&mut self) {
        self.line_map.clear();
        self.line_order.clear();
        self.known_characters.clear();
        self.settings.detections = crate::detection::DetectionDocument::default();
        self.bump_revision();
    }
""",
)

# FPS conformation must scale both absolute cues and relative sync positions.
replace_once(
    "src/export.rs",
    """        settings.instrumental_audio_offset_frames =
            (settings.instrumental_audio_offset_frames as f64 * fps_ratio) as i64;
        project.set_settings(settings);
""",
    """        settings.instrumental_audio_offset_frames =
            (settings.instrumental_audio_offset_frames as f64 * fps_ratio) as i64;
        settings.detections = settings.detections.scaled_time(fps_ratio);
        project.set_settings(settings);
""",
)

# ---------------------------------------------------------------------------
# Canonical command/history integration
# ---------------------------------------------------------------------------
replace_once(
    "src/command.rs",
    """    UpdateLineNote {
        line_id: u64,
        old_note: String,
        new_note: String,
    },
    AddDrawingStroke {
""",
    """    UpdateLineNote {
        line_id: u64,
        old_note: String,
        new_note: String,
    },
    Detection {
        change: crate::detection::DetectionChange,
    },
    AddDrawingStroke {
""",
)

replace_once(
    "src/command.rs",
    """            Command::UpdateLineNote {
                line_id, new_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = new_note.clone();
                }
            }
            Command::AddDrawingStroke { stroke } => {
""",
    """            Command::UpdateLineNote {
                line_id, new_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = new_note.clone();
                }
            }
            Command::Detection { change } => {
                project.apply_detection_change(change, true);
            }
            Command::AddDrawingStroke { stroke } => {
""",
)

replace_once(
    "src/command.rs",
    """            Command::UpdateLineNote {
                line_id, old_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = old_note.clone();
                }
            }
        }
""",
    """            Command::UpdateLineNote {
                line_id, old_note, ..
            } => {
                if let Some(l) = project.get_line_mut(*line_id) {
                    l.note = old_note.clone();
                }
            }
            Command::Detection { change } => {
                project.apply_detection_change(change, false);
            }
        }
""",
)

# Collaboration uses a complete sync for semantic detection commands.
replace_once(
    "src/packet.rs",
    """            self,
            Command::InsertLines { .. } | Command::DeleteLines { .. }
""",
    """            self,
            Command::InsertLines { .. }
                | Command::DeleteLines { .. }
                | Command::Detection { .. }
""",
)
replace_once(
    "src/packet.rs",
    """            Command::DeleteLines { .. } => unreachable!("handled as a full sync above"),
            Command::SplitLine {
""",
    """            Command::DeleteLines { .. } => unreachable!("handled as a full sync above"),
            Command::Detection { .. } => unreachable!("handled as a full sync above"),
            Command::SplitLine {
""",
)
replace_once(
    "src/packet.rs",
    """pub struct ProjectData {
    pub lines: Vec<RythmoLine>,
    pub markers: Vec<RythmoMarker>,
    pub known_characters: Vec<CharacterData>,
""",
    """pub struct ProjectData {
    pub lines: Vec<RythmoLine>,
    pub markers: Vec<RythmoMarker>,
    #[serde(default)]
    pub detections: crate::detection::DetectionDocument,
    pub known_characters: Vec<CharacterData>,
""",
)
replace_once(
    "src/packet.rs",
    """            lines: project.lines_vec(),
            markers: project.markers().to_vec(),
            known_characters: project
""",
    """            lines: project.lines_vec(),
            markers: project.markers().to_vec(),
            detections: project.detections().clone(),
            known_characters: project
""",
)
replace_once(
    "src/application/edit_service.rs",
    """        let mut settings = session.project.settings().clone();
        settings.automation = automation;
        session.project.set_settings(settings);
""",
    """        let mut settings = session.project.settings().clone();
        settings.automation = automation;
        settings.detections = data.detections;
        session.project.set_settings(settings);
""",
)

# ---------------------------------------------------------------------------
# Semantic UI actions and dispatcher
# ---------------------------------------------------------------------------
replace_once(
    "src/application/command.rs",
    """    MoveLines {
        moves: Vec<(u64, i64, f32)>,
    },
    UpdateLineText {
""",
    """    MoveLines {
        moves: Vec<(u64, i64, f32)>,
    },
    AddDetection {
        line_id: u64,
        kind: crate::detection::DetectionKind,
        media_tick: crate::detection::MediaTick,
        target: crate::detection::TextAnchor,
    },
    MoveDetection {
        address: crate::detection::DetectionAddress,
        media_tick: crate::detection::MediaTick,
    },
    DeleteDetection {
        address: crate::detection::DetectionAddress,
    },
    NudgeSelectedDetection {
        delta_ticks: i64,
    },
    UpdateLineText {
""",
)

# Mutating-action guard. Keep this replacement intentionally narrow.
replace_once(
    "src/application/command.rs",
    """                | Self::MoveLines { .. }
                | Self::UpdateLineText { .. }
""",
    """                | Self::MoveLines { .. }
                | Self::AddDetection { .. }
                | Self::MoveDetection { .. }
                | Self::DeleteDetection { .. }
                | Self::NudgeSelectedDetection { .. }
                | Self::UpdateLineText { .. }
""",
)

replace_once(
    "src/app/dispatcher.rs",
    """            UiAction::MoveLines { moves } => {
                state.move_lines(moves);
            }
            UiAction::UpdateLineText { id, text } => {
""",
    """            UiAction::MoveLines { moves } => {
                state.move_lines(moves);
            }
            UiAction::AddDetection {
                line_id,
                kind,
                media_tick,
                target,
            } => {
                state.add_detection(line_id, kind, media_tick, target);
            }
            UiAction::MoveDetection {
                address,
                media_tick,
            } => {
                state.move_detection(address, media_tick);
            }
            UiAction::DeleteDetection { address } => {
                state.delete_detection(address);
            }
            UiAction::NudgeSelectedDetection { delta_ticks } => {
                state.nudge_selected_detection(delta_ticks);
            }
            UiAction::UpdateLineText { id, text } => {
""",
)

replace_once(
    "src/app/dispatcher.rs",
    """            UiAction::DeleteSelected => {
                state.delete_selected();
            }
""",
    """            UiAction::DeleteSelected => {
                if state.has_selected_detection() {
                    state.delete_selected_detection();
                } else {
                    state.delete_selected();
                }
            }
""",
)

# ---------------------------------------------------------------------------
# State-level application use cases without bloating state.rs
# ---------------------------------------------------------------------------
write(
    "src/state_detection.rs",
    r'''//! Semantic detection use cases attached to the application state.

use crate::application::edit_service::{EditExecutor, EditOrigin};
use crate::command::Command;
use crate::detection::{
    DetectionAddress, DetectionChange, DetectionCue, DetectionKind, MediaTick, TextAnchor,
};
use crate::state::State;
use crate::workspaces::rythmo::view::Selection;

impl State {
    pub fn has_selected_detection(&self) -> bool {
        matches!(
            self.ui_shell.ui.rythmo_state.selected,
            Some(Selection::Detection(_))
        )
    }

    pub fn rythmo_detection_hovered(&self) -> bool {
        self.ui_shell.ui.rythmo_state.detection_hover.is_some()
    }

    pub fn open_detection_palette_from_hover(&mut self) -> bool {
        self.ui_shell
            .ui
            .rythmo_state
            .open_detection_palette_from_hover()
    }

    pub fn focus_detection_parent_line(&mut self) -> bool {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return false;
        };
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        self.ui_shell.ui.rythmo_state.detection_drag = None;
        true
    }

    pub fn add_detection(
        &mut self,
        line_id: u64,
        kind: DetectionKind,
        media_tick: MediaTick,
        target: TextAnchor,
    ) {
        if self.project_session.project.get_line(line_id).is_none() || target.validate().is_err() {
            return;
        }
        let detection_id = self
            .project_session
            .project
            .detections()
            .line(line_id)
            .and_then(|line| line.next_detection_id())
            .unwrap_or(crate::detection::DetectionCueId(1));
        let address = DetectionAddress {
            line_id,
            detection_id,
        };
        let change = DetectionChange::Add {
            address,
            cue: DetectionCue {
                id: detection_id,
                kind,
                media_tick,
                target,
            },
        };
        EditExecutor::execute(
            &mut self.project_session,
            Command::Detection { change },
            EditOrigin::Local,
        );
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
        self.ui_shell.ui.rythmo_state.detection_menu = None;
    }

    pub fn move_detection(&mut self, address: DetectionAddress, media_tick: MediaTick) {
        let Some(old_tick) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .map(|cue| cue.media_tick)
        else {
            return;
        };
        if old_tick == media_tick {
            return;
        }
        let change = DetectionChange::Move {
            address,
            old_tick,
            new_tick: media_tick,
        };
        let coalesce = matches!(
            self.project_session.history.last(),
            Some(Command::Detection {
                change: DetectionChange::Move {
                    address: previous_address,
                    ..
                }
            }) if *previous_address == address
        );
        let command = Command::Detection {
            change: change.clone(),
        };
        if coalesce {
            EditExecutor::coalesce(
                &mut self.project_session,
                command,
                |last| {
                    if let Command::Detection {
                        change: DetectionChange::Move { new_tick, .. },
                    } = last
                    {
                        *new_tick = media_tick;
                    }
                },
                EditOrigin::Local,
            );
        } else {
            EditExecutor::execute(&mut self.project_session, command, EditOrigin::Local);
        }
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Detection(address));
    }

    pub fn delete_detection(&mut self, address: DetectionAddress) {
        let Some(cue) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .cloned()
        else {
            return;
        };
        EditExecutor::execute(
            &mut self.project_session,
            Command::Detection {
                change: DetectionChange::Remove { address, cue },
            },
            EditOrigin::Local,
        );
        self.ui_shell.ui.rythmo_state.selected = Some(Selection::Line(address.line_id));
        self.ui_shell.ui.rythmo_state.detection_drag = None;
    }

    pub fn delete_selected_detection(&mut self) {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return;
        };
        self.delete_detection(address);
    }

    pub fn nudge_selected_detection(&mut self, delta_ticks: i64) {
        let Some(Selection::Detection(address)) = self.ui_shell.ui.rythmo_state.selected else {
            return;
        };
        let Some(current) = self
            .project_session
            .project
            .detections()
            .detection(address)
            .map(|cue| cue.media_tick)
        else {
            return;
        };
        let Some(line) = self.project_session.project.get_line(address.line_id) else {
            return;
        };
        let min = MediaTick::from_frame(line.start_frame);
        let max = MediaTick::from_frame(line.end_frame());
        let next = MediaTick(current.raw().saturating_add(delta_ticks)).clamp(min, max);
        self.move_detection(address, next);
    }
}
''',
)

replace_once(
    "src/lib.rs",
    """pub mod state;
pub mod syllable;
""",
    """pub mod state;
mod state_detection;
pub mod syllable;
""",
)

# ---------------------------------------------------------------------------
# Editor interaction and editor-only rendering
# ---------------------------------------------------------------------------
write(
    "src/workspaces/rythmo/detection_ui.rs",
    r'''//! Editor-only interaction and rendering for semantic detection cues.

use super::*;
use crate::detection::{DetectionAddress, DetectionKind, MediaTick, TextAnchor};

const DETECTION_BADGE_W: f32 = 24.0;
const DETECTION_BADGE_H: f32 = 16.0;
const DETECTION_BUTTON_SIZE: f32 = 18.0;
const MENU_COLUMNS: usize = 2;
const MENU_ROW_H: f32 = 24.0;
const MENU_PADDING: f32 = 6.0;
const MENU_WIDTH: f32 = 370.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionHover {
    pub line_id: u64,
    pub media_tick: MediaTick,
    pub line_rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionMenu {
    pub line_id: u64,
    pub media_tick: MediaTick,
    pub x: f32,
    pub y: f32,
    pub hover_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionDrag {
    pub address: DetectionAddress,
}

impl RythmoState {
    pub(crate) fn open_detection_palette_from_hover(&mut self) -> bool {
        let Some(hover) = self.detection_hover else {
            return false;
        };
        let button = detection_button_rect(&hover);
        self.detection_menu = Some(DetectionMenu {
            line_id: hover.line_id,
            media_tick: hover.media_tick,
            x: button.x,
            y: button.y + button.height + 2.0,
            hover_index: None,
        });
        true
    }
}

fn tick_x(tick: MediaTick, current_frame: f64, zone: &Rect) -> f32 {
    zone.x
        + zone.width / 2.0
        + (tick.as_frame_position() - current_frame) as f32 * ppf()
}

fn pointer_tick(x: f32, current_frame: f64, zone: &Rect) -> MediaTick {
    let frame = current_frame + ((x - (zone.x + zone.width / 2.0)) / ppf()) as f64;
    MediaTick::from_frame_position(frame)
}

fn clamp_tick_to_line(
    tick: MediaTick,
    line: &crate::rythmo_line::RythmoLine,
) -> MediaTick {
    tick.clamp(
        MediaTick::from_frame(line.start_frame),
        MediaTick::from_frame(line.end_frame()),
    )
}

fn detection_button_rect(hover: &DetectionHover) -> Rect {
    let x = hover
        .line_rect
        .x
        .max(hover.line_rect.x.min(hover.line_rect.x + hover.line_rect.width));
    Rect {
        x: x + (tick_x_with_rect(hover.media_tick, &hover.line_rect) - x)
            .clamp(0.0, hover.line_rect.width)
            - DETECTION_BUTTON_SIZE / 2.0,
        y: hover.line_rect.y + hover.line_rect.height + 2.0,
        width: DETECTION_BUTTON_SIZE,
        height: DETECTION_BUTTON_SIZE,
    }
}

fn tick_x_with_rect(tick: MediaTick, rect: &Rect) -> f32 {
    // The hover tick is already clamped to this line. The caller stores the
    // line rectangle only to keep the D button alive while crossing its gap.
    let _ = tick;
    rect.x + rect.width / 2.0
}

fn actual_detection_button_rect(
    hover: &DetectionHover,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    Rect {
        x: tick_x(hover.media_tick, current_frame, zone) - DETECTION_BUTTON_SIZE / 2.0,
        y: hover.line_rect.y + hover.line_rect.height + 2.0,
        width: DETECTION_BUTTON_SIZE,
        height: DETECTION_BUTTON_SIZE,
    }
}

fn detection_badge_rect(
    cue: &crate::detection::DetectionCue,
    line_rect: Rect,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    Rect {
        x: tick_x(cue.media_tick, current_frame, zone) - DETECTION_BADGE_W / 2.0,
        y: line_rect.y + line_rect.height - DETECTION_BADGE_H - 2.0,
        width: DETECTION_BADGE_W,
        height: DETECTION_BADGE_H,
    }
}

fn menu_rows() -> usize {
    DetectionKind::ALL.len().div_ceil(MENU_COLUMNS)
}

fn menu_rect(menu: &DetectionMenu, zone: &Rect) -> Rect {
    let height = menu_rows() as f32 * MENU_ROW_H + MENU_PADDING * 2.0;
    Rect {
        x: menu.x.clamp(zone.x, (zone.x + zone.width - MENU_WIDTH).max(zone.x)),
        y: menu
            .y
            .clamp(zone.y, (zone.y + zone.height - height).max(zone.y)),
        width: MENU_WIDTH,
        height,
    }
}

fn menu_item_rect(menu: &DetectionMenu, zone: &Rect, index: usize) -> Rect {
    let menu_rect = menu_rect(menu, zone);
    let rows = menu_rows();
    let column = index / rows;
    let row = index % rows;
    let column_width = (menu_rect.width - MENU_PADDING * 2.0) / MENU_COLUMNS as f32;
    Rect {
        x: menu_rect.x + MENU_PADDING + column as f32 * column_width,
        y: menu_rect.y + MENU_PADDING + row as f32 * MENU_ROW_H,
        width: column_width,
        height: MENU_ROW_H,
    }
}

fn hit_existing_detection(
    ctx: &RythmoCtx<'_>,
    state: &RythmoState,
    x: f32,
    y: f32,
) -> Option<DetectionAddress> {
    for line in ctx.project.lines() {
        let line_rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        let Some(data) = ctx.project.detections().line(line.id) else {
            continue;
        };
        for cue in data.detections() {
            if detection_badge_rect(cue, line_rect, ctx.current_frame, ctx.zone).contains(x, y) {
                return Some(DetectionAddress {
                    line_id: line.id,
                    detection_id: cue.id,
                });
            }
        }
    }
    let _ = state;
    None
}

fn anchor_for_tick(
    line: &crate::rythmo_line::RythmoLine,
    tick: MediaTick,
) -> TextAnchor {
    let count = line.text.chars().count();
    if count == 0 || line.duration_frames <= 0 {
        return TextAnchor::BeforeText;
    }
    let frame = tick.as_frame_position();
    let ratio = ((frame - line.start_frame as f64) / line.duration_frames as f64).clamp(0.0, 1.0);
    let index = (ratio * count as f64).round() as usize;
    if index == 0 {
        TextAnchor::BeforeText
    } else if index >= count {
        TextAnchor::AfterText
    } else {
        TextAnchor::Grapheme { index: index as u32 }
    }
}

fn navigate_detection(
    project: &Project,
    state: &mut RythmoState,
    direction: i32,
) -> bool {
    let (line_id, current) = match state.selected {
        Some(Selection::Line(line_id)) => (line_id, None),
        Some(Selection::Detection(address)) => (address.line_id, Some(address.detection_id)),
        _ => return false,
    };
    let Some(data) = project.detections().line(line_id) else {
        return false;
    };
    let cues = data.detections();
    if cues.is_empty() {
        return false;
    }
    let index = if let Some(current) = current {
        let current_index = cues.iter().position(|cue| cue.id == current).unwrap_or(0);
        if direction < 0 {
            current_index.saturating_sub(1)
        } else {
            (current_index + 1).min(cues.len() - 1)
        }
    } else if direction < 0 {
        cues.len() - 1
    } else {
        0
    };
    state.selected = Some(Selection::Detection(DetectionAddress {
        line_id,
        detection_id: cues[index].id,
    }));
    true
}

pub(crate) fn handle_detection_event(
    ctx: &RythmoCtx<'_>,
    event: &UiEvent,
    state: &mut RythmoState,
) -> Option<EventResponse> {
    match event {
        UiEvent::MouseMove { x, y } => {
            if let Some(drag) = state.detection_drag {
                let line = ctx.project.get_line(drag.address.line_id)?;
                let tick = clamp_tick_to_line(pointer_tick(*x, ctx.current_frame, ctx.zone), line);
                return Some(EventResponse::Action(UiAction::MoveDetection {
                    address: drag.address,
                    media_tick: tick,
                }));
            }

            if let Some(mut menu) = state.detection_menu {
                let hover = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                    .map(|(index, _)| index);
                if hover != menu.hover_index {
                    menu.hover_index = hover;
                    state.detection_menu = Some(menu);
                }
                return Some(EventResponse::Consumed);
            }

            if let Some(hover) = state.detection_hover {
                if actual_detection_button_rect(&hover, ctx.current_frame, ctx.zone).contains(*x, *y)
                {
                    return Some(EventResponse::Consumed);
                }
            }

            let found = hit_test_line_and_track(ctx, state, *x, *y).0;
            let next = found.and_then(|line_id| {
                let line = ctx.project.get_line(line_id)?;
                let rect = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
                Some(DetectionHover {
                    line_id,
                    media_tick: clamp_tick_to_line(
                        pointer_tick(*x, ctx.current_frame, ctx.zone),
                        line,
                    ),
                    line_rect: rect,
                })
            });
            if next != state.detection_hover {
                state.detection_hover = next;
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::MousePress { x, y } => {
            if let Some(menu) = state.detection_menu {
                if let Some((_, kind)) = DetectionKind::ALL
                    .iter()
                    .enumerate()
                    .find(|(index, _)| menu_item_rect(&menu, ctx.zone, *index).contains(*x, *y))
                {
                    let line = ctx.project.get_line(menu.line_id)?;
                    let target = anchor_for_tick(line, menu.media_tick);
                    state.detection_menu = None;
                    return Some(EventResponse::Action(UiAction::AddDetection {
                        line_id: menu.line_id,
                        kind: *kind,
                        media_tick: menu.media_tick,
                        target,
                    }));
                }
                state.detection_menu = None;
                return Some(EventResponse::Consumed);
            }

            if let Some(address) = hit_existing_detection(ctx, state, *x, *y) {
                state.selected = Some(Selection::Detection(address));
                state.detection_drag = Some(DetectionDrag { address });
                return Some(EventResponse::Consumed);
            }

            if let Some(hover) = state.detection_hover {
                if actual_detection_button_rect(&hover, ctx.current_frame, ctx.zone).contains(*x, *y)
                {
                    state.open_detection_palette_from_hover();
                    return Some(EventResponse::Consumed);
                }
            }
        }
        UiEvent::MouseRelease { .. } if state.detection_drag.is_some() => {
            state.detection_drag = None;
            return Some(EventResponse::Consumed);
        }
        UiEvent::KeyInput { text } if text.eq_ignore_ascii_case("d") => {
            if state.open_detection_palette_from_hover() {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            if state.detection_menu.take().is_some() {
                return Some(EventResponse::Consumed);
            }
            if let Some(Selection::Detection(address)) = state.selected {
                state.selected = Some(Selection::Line(address.line_id));
                state.detection_drag = None;
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorLeft => {
            if navigate_detection(ctx.project, state, -1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::AltCursorRight => {
            if navigate_detection(ctx.project, state, 1) {
                return Some(EventResponse::Consumed);
            }
        }
        UiEvent::Delete => {
            if let Some(Selection::Detection(address)) = state.selected {
                return Some(EventResponse::Action(UiAction::DeleteDetection { address }));
            }
        }
        _ => {}
    }
    None
}

fn push_quad(quads: &mut Vec<QuadInstance>, rect: Rect, color: [f32; 4], border: [f32; 4]) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color,
        color_bottom: color,
        border_color: border,
        border_width: if border[3] > 0.0 { 1.0 } else { 0.0 },
        border_radius: 3.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0, 0.0, 0.0, 0.22],
        shadow_blur: 2.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

pub(crate) fn render_detection_overlay<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    for line in project.lines() {
        let line_rect = line_rect(project, line, current_frame, zone);
        let Some(data) = project.detections().line(line.id) else {
            continue;
        };
        for cue in data.detections() {
            let x = tick_x(cue.media_tick, current_frame, zone);
            if x < zone.x - DETECTION_BADGE_W || x > zone.x + zone.width + DETECTION_BADGE_W {
                continue;
            }
            let address = DetectionAddress {
                line_id: line.id,
                detection_id: cue.id,
            };
            let selected = matches!(state.selected, Some(Selection::Detection(current)) if current == address);
            let badge = detection_badge_rect(cue, line_rect, current_frame, zone);
            push_quad(
                quads,
                Rect {
                    x: x - 0.75,
                    y: line_rect.y,
                    width: 1.5,
                    height: line_rect.height,
                },
                if selected {
                    [1.0, 0.72, 0.12, 0.95]
                } else {
                    [0.78, 0.78, 0.82, 0.72]
                },
                [0.0; 4],
            );
            push_quad(
                quads,
                badge,
                if selected {
                    [0.40, 0.24, 0.04, 0.98]
                } else {
                    [0.11, 0.11, 0.14, 0.96]
                },
                if selected {
                    [1.0, 0.72, 0.12, 1.0]
                } else {
                    [0.75, 0.75, 0.82, 0.75]
                },
            );
            labels.push(LabelInfo {
                text: cue.kind.short_label(),
                bounds: badge,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 1.0,
                font_size_override: Some(9.0),
                color_override: Some([238, 238, 244]),
                font_family_override: None,
            });
        }
    }

    if let Some(hover) = state.detection_hover {
        let x = tick_x(hover.media_tick, current_frame, zone);
        let mut y = hover.line_rect.y;
        while y < hover.line_rect.y + hover.line_rect.height {
            push_quad(
                quads,
                Rect {
                    x: x - 0.5,
                    y,
                    width: 1.0,
                    height: 3.0_f32.min(hover.line_rect.y + hover.line_rect.height - y),
                },
                [0.65, 0.65, 0.68, 0.72],
                [0.0; 4],
            );
            y += 6.0;
        }
        let button = actual_detection_button_rect(&hover, current_frame, zone);
        push_quad(
            quads,
            button,
            [0.15, 0.15, 0.18, 0.98],
            [0.72, 0.72, 0.78, 0.9],
        );
        labels.push(LabelInfo {
            text: "D",
            bounds: button,
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(11.0),
            color_override: Some([240, 240, 245]),
            font_family_override: None,
        });
    }

    if let Some(menu) = state.detection_menu {
        let outer = menu_rect(&menu, zone);
        push_quad(
            quads,
            outer,
            [0.055, 0.055, 0.07, 0.99],
            [0.48, 0.48, 0.56, 0.9],
        );
        for (index, kind) in DetectionKind::ALL.iter().enumerate() {
            let row = menu_item_rect(&menu, zone, index);
            if menu.hover_index == Some(index) {
                push_quad(
                    quads,
                    row,
                    [0.19, 0.27, 0.46, 0.98],
                    [0.40, 0.58, 0.92, 0.7],
                );
            }
            let sigle = Rect {
                x: row.x + 3.0,
                y: row.y + 3.0,
                width: 30.0,
                height: row.height - 6.0,
            };
            labels.push(LabelInfo {
                text: kind.short_label(),
                bounds: sigle,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 0.0,
                font_size_override: Some(9.0),
                color_override: Some([245, 210, 90]),
                font_family_override: None,
            });
            labels.push(LabelInfo {
                text: kind.display_name(),
                bounds: Rect {
                    x: row.x + 37.0,
                    y: row.y,
                    width: row.width - 40.0,
                    height: row.height,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 2.0,
                font_size_override: Some(11.0),
                color_override: Some([232, 232, 238]),
                font_family_override: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_time_rounds_to_a_tenth_frame() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let center = zone.x + zone.width / 2.0;
        assert_eq!(pointer_tick(center + ppf() * 0.34, 100.0, &zone), MediaTick(1003));
        assert_eq!(pointer_tick(center + ppf() * 0.36, 100.0, &zone), MediaTick(1004));
    }

    #[test]
    fn palette_contains_every_detection_kind() {
        assert_eq!(DetectionKind::ALL.len(), 17);
        assert_eq!(menu_rows(), 9);
    }
}
''',
)

# Register module before state so state.rs can import its public interaction types.
replace_once(
    "src/workspaces/rythmo/view.rs",
    """#[path = "state.rs"]
mod state;
pub use state::*;
""",
    """#[path = "detection_ui.rs"]
mod detection_ui;
pub(crate) use detection_ui::*;
#[path = "state.rs"]
mod state;
pub use state::*;
""",
)

replace_once(
    "src/workspaces/rythmo/state.rs",
    """    Marker(usize),
    AllLines,
""",
    """    Marker(usize),
    Detection(crate::detection::DetectionAddress),
    AllLines,
""",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    """    pub context_menu: Option<LineContextMenu>,
    pub active_stroke: Option<crate::rythmo_drawing::DrawingStroke>,
""",
    """    pub context_menu: Option<LineContextMenu>,
    pub detection_hover: Option<DetectionHover>,
    pub detection_menu: Option<DetectionMenu>,
    pub detection_drag: Option<DetectionDrag>,
    pub active_stroke: Option<crate::rythmo_drawing::DrawingStroke>,
""",
)
replace_once(
    "src/workspaces/rythmo/state.rs",
    """            context_menu: None,
            active_stroke: None,
""",
    """            context_menu: None,
            detection_hover: None,
            detection_menu: None,
            detection_drag: None,
            active_stroke: None,
""",
)

# Let the semantic detector reuse the existing indexed line/track hit test.
replace_once(
    "src/workspaces/rythmo/mouse.rs",
    """fn hit_test_line_and_track(
""",
    """pub(crate) fn hit_test_line_and_track(
""",
)

# Detection controls take priority over syllable handles and line drags.
replace_once(
    "src/workspaces/rythmo/controller.rs",
    """    match event {
        UiEvent::MousePress { x, y } => {
            if let Some(resp) = syllable_mouse_press(&ctx, state, *x, *y, false) {
""",
    """    if let Some(response) = handle_detection_event(&ctx, event, state) {
        return response;
    }

    match event {
        UiEvent::MousePress { x, y } => {
            if let Some(resp) = syllable_mouse_press(&ctx, state, *x, *y, false) {
""",
)
replace_once(
    "src/workspaces/rythmo/controller.rs",
    """    state.context_menu = None;
    if state.is_editing() {
""",
    """    state.context_menu = None;
    state.detection_hover = None;
    state.detection_menu = None;
    state.detection_drag = None;
    if matches!(state.selected, Some(Selection::Detection(_))) {
        state.selected = None;
    }
    if state.is_editing() {
""",
)

# Editor overlay only: CPU/GPU exports never receive these symbols.
replace_once(
    "src/workspaces/rythmo/view.rs",
    """    push_editor_karaoke_texture_prewarm_texts(
        stretched,
""",
    """    if !karaoke_preview {
        render_detection_overlay(zone, project, current_frame, state, quads, labels);
    }

    push_editor_karaoke_texture_prewarm_texts(
        stretched,
""",
)

# ---------------------------------------------------------------------------
# Context-sensitive keyboard routing
# ---------------------------------------------------------------------------
replace_once(
    "src/app/event_loop.rs",
    """                            dispatch_key_action(
                                UiAction::NudgeSelectedLines { delta_frames },
""",
    """                            let action = if state.has_selected_detection() {
                                UiAction::NudgeSelectedDetection {
                                    delta_ticks: delta_frames,
                                }
                            } else {
                                UiAction::NudgeSelectedLines { delta_frames }
                            };
                            dispatch_key_action(
                                action,
""",
)

replace_once(
    "src/app/event_loop.rs",
    """                        let mut contexts = Vec::new();
""",
    """                        if matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && state.has_selected_detection()
                        {
                            state.focus_detection_parent_line();
                            state.request_redraw();
                            return;
                        }
                        if !ctrl_held
                            && !shift_held
                            && !keyboard_modifiers.alt
                            && !event.repeat
                            && !state.captures_modal_input()
                            && !state.is_editing_text()
                            && state.active_workspace() == WorkspaceId::Rythmo
                            && state.rythmo_detection_hovered()
                            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("d"))
                        {
                            state.open_detection_palette_from_hover();
                            state.request_redraw();
                            return;
                        }

                        let mut contexts = Vec::new();
""",
)

print("Detection editor patches applied")
