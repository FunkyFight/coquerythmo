//! Comic Dubs studio: layout, interaction and scene generation.
//!
//! Pages and sounds sit on the left, the tool bar above the page, the
//! inspector on the right and the timeline under the page. The inspector
//! follows the selection: a bubble, a camera shot, or the page and project
//! when nothing is selected. Camera shots are always drawn on the page as
//! numbered frames shaped like the video, and can be moved, resized and
//! reordered. While playing or scrubbing, the canvas shows the exact frame the
//! export renders ([`crate::comic_dubs_timeline`]).

use crate::comic_dubs::{
    bubble_bounds, Bubble, BubbleEmphasis, BubbleEntrance, BubbleFx, BubbleId, BubbleLook,
    BubblePreset, BubbleSound, CameraShot, ComicAudioId, ComicDubsProject, Page, PageFx, PageId,
    PageMotion, PageTransition, Point, Region, ScreenEffect, ShotId, ShotMovement, StudioSettings,
    TextAlignment, TextReveal, EMPTY_SHOT_HOLD_MS,
};
use crate::comic_dubs_shapes::{self, ShapeKind};
use crate::comic_dubs_text::{self as text_layout, LineReveal, TextLayout};
use crate::comic_dubs_timeline::{
    self as timeline, BubbleFrame, Camera, LayerFrame, Placement, Timeline,
};
use crate::ui::color_picker::ColorPickerState;
use crate::ui::focus::AccessibleRole;
use crate::ui::primitives::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, StyledText,
    UiAction, UiEvent, VAlign,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

const SIDEBAR_W: f32 = 244.0;
const INSPECTOR_W: f32 = 312.0;
const HEADER_H: f32 = 50.0;
const TOOLBAR_H: f32 = 42.0;
const TIMELINE_H: f32 = 188.0;
const TIMELINE_HEADER_H: f32 = 32.0;
const TIMELINE_LABEL_W: f32 = 84.0;
const INSPECTOR_HEADER_H: f32 = 64.0;
const INSPECTOR_TABS_H: f32 = 36.0;
const TOOL_SIZE: f32 = 38.0;
const SIDEBAR_TABS_H: f32 = 34.0;
const PAGE_CARD_H: f32 = 86.0;
const AUDIO_ROW_H: f32 = 54.0;
const ITEM_GAP: f32 = 8.0;
const DROPDOWN_ROW_H: f32 = 30.0;
const DROPDOWN_MAX_ROWS: usize = 10;
const SHOT_HANDLE: f32 = 12.0;
const SHOT_TAG_H: f32 = 22.0;

const BG: [f32; 4] = [0.045, 0.047, 0.058, 1.0];
const PANEL: [f32; 4] = [0.072, 0.075, 0.092, 1.0];
const PANEL_ALT: [f32; 4] = [0.105, 0.11, 0.136, 1.0];
const PANEL_HOVER: [f32; 4] = [0.14, 0.145, 0.18, 1.0];
const FIELD: [f32; 4] = [0.052, 0.055, 0.068, 1.0];
const BORDER: [f32; 4] = [0.2, 0.21, 0.26, 1.0];
const BORDER_SOFT: [f32; 4] = [0.14, 0.145, 0.18, 1.0];
const ACCENT: [f32; 4] = [0.36, 0.29, 0.86, 1.0];
const ACCENT_SOFT: [f32; 4] = [0.13, 0.11, 0.27, 1.0];
const DANGER: [f32; 4] = [0.62, 0.12, 0.16, 1.0];
const DANGER_SOFT: [f32; 4] = [0.2, 0.06, 0.08, 1.0];
const SHOT_COLOR: [f32; 4] = [1.0, 0.52, 0.1, 1.0];
const SHOT_SOFT: [f32; 4] = [0.3, 0.14, 0.02, 1.0];
const VOICE_COLOR: [f32; 4] = [0.22, 0.74, 0.48, 1.0];
const SFX_COLOR: [f32; 4] = [1.0, 0.72, 0.16, 1.0];
const MUSIC_COLOR: [f32; 4] = [0.33, 0.52, 0.95, 1.0];
const PLAYHEAD_COLOR: [f32; 4] = [0.95, 0.3, 0.46, 1.0];
const RECORD_COLOR: [f32; 4] = [0.85, 0.12, 0.18, 1.0];
const ICON: [f32; 4] = [0.85, 0.86, 0.92, 1.0];
const ICON_MUTED: [f32; 4] = [0.45, 0.46, 0.53, 1.0];
const TEXT: [u8; 3] = [232, 234, 242];
const MUTED: [u8; 3] = [148, 152, 170];
const DIM: [u8; 3] = [98, 102, 118];
const SHOT_TEXT: [u8; 3] = [255, 222, 186];
const VERTEX_EDITOR_HEADER_H: f32 = 52.0;
const VERTEX_EDITOR_TIMELINE_H: f32 = 112.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct ComicDubsLayout {
    pub content: Rect,
    pub sidebar: Rect,
    pub inspector: Rect,
    /// Tool bar above the page: drawing tools, shot visibility, page switcher.
    pub header: Rect,
    /// Shared transport row of the application.
    pub toolbar: Rect,
    pub canvas: Rect,
    pub timeline: Rect,
}

/// Vertical bands of the timeline, left of which sit the row names.
#[derive(Debug, Clone, Copy)]
struct TimelineRows {
    ruler: Rect,
    pages: Rect,
    shots: Rect,
    bubbles: Rect,
    sounds: Rect,
    music: Rect,
}

impl ComicDubsLayout {
    pub fn compute(content: Rect) -> Self {
        let sidebar_w = SIDEBAR_W.min((content.width * 0.24).max(180.0));
        let sidebar = Rect {
            x: content.x,
            y: content.y,
            width: sidebar_w,
            height: content.height,
        };
        let inspector_w = INSPECTOR_W.min((content.width * 0.3).max(236.0));
        let inspector = Rect {
            x: content.x + content.width - inspector_w,
            y: content.y,
            width: inspector_w,
            height: content.height,
        };
        let main = Rect {
            x: sidebar.x + sidebar.width,
            y: content.y,
            width: (content.width - sidebar.width - inspector.width).max(0.0),
            height: content.height,
        };
        let header = Rect {
            height: HEADER_H,
            ..main
        };
        let toolbar = Rect {
            y: header.y + header.height,
            height: TOOLBAR_H,
            ..header
        };
        let body_top = toolbar.y + toolbar.height;
        let timeline_h = if main.height > 620.0 {
            TIMELINE_H
        } else if main.height > 460.0 {
            144.0
        } else {
            104.0
        };
        let timeline = Rect {
            x: main.x,
            y: (main.y + main.height - timeline_h).max(body_top),
            width: main.width,
            height: timeline_h,
        };
        let canvas = Rect {
            x: main.x + 12.0,
            y: body_top + 10.0,
            width: (main.width - 24.0).max(0.0),
            height: (timeline.y - body_top - 20.0).max(0.0),
        };
        Self {
            content,
            sidebar,
            inspector,
            header,
            toolbar,
            canvas,
            timeline,
        }
    }

    fn tool_button(self, index: usize) -> Rect {
        // Gaps separate selection, bubble shapes and the camera.
        let groups = if index >= Tool::SHOT_INDEX {
            2.0
        } else if index >= 1 {
            1.0
        } else {
            0.0
        };
        Rect {
            x: self.header.x + 10.0 + index as f32 * (TOOL_SIZE + 4.0) + groups * 14.0,
            y: self.header.y + (HEADER_H - TOOL_SIZE) * 0.5,
            width: TOOL_SIZE,
            height: TOOL_SIZE,
        }
    }

    fn next_page(self) -> Rect {
        Rect {
            x: self.header.x + self.header.width - 10.0 - 34.0,
            y: self.header.y + (HEADER_H - 34.0) * 0.5,
            width: 34.0,
            height: 34.0,
        }
    }

    fn page_label(self) -> Rect {
        let next = self.next_page();
        Rect {
            x: next.x - 104.0,
            width: 104.0,
            ..next
        }
    }

    fn previous_page(self) -> Rect {
        Rect {
            x: self.page_label().x - 34.0,
            ..self.next_page()
        }
    }

    fn shots_toggle(self) -> Rect {
        let previous = self.previous_page();
        Rect {
            x: previous.x - 14.0 - 104.0,
            width: 104.0,
            ..previous
        }
    }

    fn zoom_button(self) -> Rect {
        let toggle = self.shots_toggle();
        Rect {
            x: toggle.x - 8.0 - 84.0,
            width: 84.0,
            ..toggle
        }
    }

    fn sidebar_tab(self, index: usize) -> Rect {
        let width = (self.sidebar.width - 24.0) * 0.5;
        Rect {
            x: self.sidebar.x + 10.0 + index as f32 * (width + 4.0),
            y: self.sidebar.y + 10.0,
            width,
            height: SIDEBAR_TABS_H,
        }
    }

    fn sidebar_import(self) -> Rect {
        Rect {
            x: self.sidebar.x + 10.0,
            y: self.sidebar.y + self.sidebar.height - 48.0,
            width: self.sidebar.width - 20.0,
            height: 38.0,
        }
    }

    fn sidebar_list(self) -> Rect {
        let top = self.sidebar.y + 10.0 + SIDEBAR_TABS_H + 10.0;
        Rect {
            x: self.sidebar.x + 8.0,
            y: top,
            width: self.sidebar.width - 16.0,
            height: (self.sidebar_import().y - 10.0 - top).max(0.0),
        }
    }

    fn inspector_header(self) -> Rect {
        Rect {
            height: INSPECTOR_HEADER_H,
            ..self.inspector
        }
    }

    /// "Page | Projet" switch shown when nothing is selected.
    fn inspector_tab(self, index: usize) -> Rect {
        let width = (self.inspector.width - 28.0) * 0.5;
        Rect {
            x: self.inspector.x + 12.0 + index as f32 * (width + 4.0),
            y: self.inspector.y + INSPECTOR_HEADER_H,
            width,
            height: INSPECTOR_TABS_H - 6.0,
        }
    }

    fn inspector_body(self, tabs: bool) -> Rect {
        let top =
            self.inspector.y + INSPECTOR_HEADER_H + if tabs { INSPECTOR_TABS_H + 4.0 } else { 4.0 };
        Rect {
            x: self.inspector.x,
            y: top,
            width: self.inspector.width,
            height: (self.inspector.y + self.inspector.height - top - 8.0).max(0.0),
        }
    }

    fn timeline_header(self) -> Rect {
        Rect {
            height: TIMELINE_HEADER_H,
            ..self.timeline
        }
    }

    fn timeline_play(self) -> Rect {
        let header = self.timeline_header();
        Rect {
            x: header.x + 10.0,
            y: header.y + 3.0,
            width: 30.0,
            height: 26.0,
        }
    }

    /// Area where time runs from left to right.
    fn timeline_track(self) -> Rect {
        let top = self.timeline.y + TIMELINE_HEADER_H;
        Rect {
            x: self.timeline.x + TIMELINE_LABEL_W,
            y: top,
            width: (self.timeline.width - TIMELINE_LABEL_W - 14.0).max(1.0),
            height: (self.timeline.y + self.timeline.height - top - 8.0).max(1.0),
        }
    }

    fn timeline_rows(self) -> TimelineRows {
        let track = self.timeline_track();
        let weights = [16.0, 18.0, 26.0, 34.0, 14.0, 14.0];
        let gap = 4.0;
        let scale = ((track.height - gap * 5.0) / weights.iter().sum::<f32>()).clamp(0.4, 1.3);
        let mut y = track.y;
        let mut rows = weights.map(|weight| {
            let row = Rect {
                y,
                height: weight * scale,
                ..track
            };
            y += weight * scale + gap;
            row
        });
        rows[0].height = rows[0].height.max(12.0);
        TimelineRows {
            ruler: rows[0],
            pages: rows[1],
            shots: rows[2],
            bubbles: rows[3],
            sounds: rows[4],
            music: rows[5],
        }
    }
}

/// Canvas tools of the studio tool bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Ellipse,
    Rectangle,
    Shout,
    Thought,
    Narration,
    Polygon,
    Shot,
}

impl Tool {
    pub const ALL: [Self; 8] = [
        Self::Select,
        Self::Ellipse,
        Self::Rectangle,
        Self::Shout,
        Self::Thought,
        Self::Narration,
        Self::Polygon,
        Self::Shot,
    ];
    const SHOT_INDEX: usize = 7;

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Sélection",
            Self::Ellipse => "Bulle ronde",
            Self::Rectangle => "Bulle rectangulaire",
            Self::Shout => "Bulle de cri",
            Self::Thought => "Bulle de pensée",
            Self::Narration => "Cartouche de narration",
            Self::Polygon => "Forme libre",
            Self::Shot => "Plan caméra",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Select => "comic/select",
            Self::Ellipse => "comic/ellipse",
            Self::Rectangle => "comic/rectangle",
            Self::Shout => "comic/shout",
            Self::Thought => "comic/thought",
            Self::Narration => "comic/narration",
            Self::Polygon => "comic/polygon",
            Self::Shot => "comic/shot",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Select => {
                "Cliquez une bulle ou l'étiquette d'un plan • double-clic pour écrire • Maj+clic : ajouter/retirer un sommet"
            }
            Self::Polygon => "Cliquez pour poser les sommets • cliquez le premier point pour fermer • Échap annule",
            Self::Shot => {
                "Glissez sur la page pour cadrer un plan au format de la vidéo • glissez un plan pour le déplacer, un coin pour le redimensionner"
            }
            _ => "Glissez sur la page pour tracer la bulle • un simple clic crée une bulle standard",
        }
    }

    fn shape(self) -> Option<(ShapeKind, BubblePreset)> {
        match self {
            Self::Ellipse => Some((ShapeKind::Ellipse, BubblePreset::Classic)),
            Self::Rectangle => Some((ShapeKind::Rectangle, BubblePreset::Classic)),
            Self::Shout => Some((ShapeKind::Shout, BubblePreset::Shout)),
            Self::Thought => Some((ShapeKind::Thought, BubblePreset::Thought)),
            Self::Narration => Some((ShapeKind::Narration, BubblePreset::Narration)),
            _ => None,
        }
    }
}

/// What the inspector edits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Selection {
    #[default]
    None,
    Bubble(BubbleId),
    Shot(ShotId),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SidebarTab {
    #[default]
    Pages,
    Sounds,
}

/// Inspector content when nothing is selected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum OverviewTab {
    #[default]
    Page,
    Project,
}

#[derive(Debug, Clone)]
pub struct SceneLabel {
    pub text: String,
    pub bounds: Rect,
    pub h_align: HAlign,
    pub font_size: f32,
    pub color: [u8; 3],
    pub font_family: Option<String>,
    pub padding: f32,
    pub letter_spacing: f32,
    pub style: Option<StyledText>,
}

#[derive(Debug, Clone)]
pub struct SceneControl {
    pub id: String,
    pub label: String,
    pub bounds: Rect,
    pub role: AccessibleRole,
    pub selected: bool,
    /// Shown on hover (icon-only buttons).
    pub tooltip: Option<String>,
}

/// An icon of the application atlas, tinted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneIcon {
    pub name: &'static str,
    pub rect: Rect,
    pub tint: [f32; 4],
}

/// A page texture drawn by the renderer, already clipped to the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageLayer {
    pub page_id: PageId,
    pub rect: Rect,
    pub uv: [f32; 4],
    pub tint: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct ComicDubsScene {
    pub quads: Vec<QuadInstance>,
    pub icons: Vec<SceneIcon>,
    pub labels: Vec<SceneLabel>,
    pub overlay_quads: Vec<QuadInstance>,
    pub overlay_icons: Vec<SceneIcon>,
    pub overlay_labels: Vec<SceneLabel>,
    /// Screen flashes, drawn above every other layer of the studio.
    pub top_quads: Vec<QuadInstance>,
    /// Open menus and tooltips.
    pub popup_quads: Vec<QuadInstance>,
    pub popup_icons: Vec<SceneIcon>,
    pub popup_labels: Vec<SceneLabel>,
    pub controls: Vec<SceneControl>,
    pub page_rect: Option<Rect>,
    pub page_id: Option<PageId>,
    pub page_layers: Vec<PageLayer>,
    /// Small page previews of the page list.
    pub thumbnails: Vec<PageLayer>,
}

impl ComicDubsScene {
    /// Atlas icons of a layer, ready for the renderer.
    pub fn icon_instances(icons: &[SceneIcon], uv: impl Fn(&str) -> [f32; 4]) -> Vec<IconInstance> {
        icons
            .iter()
            .map(|icon| IconInstance {
                rect: [icon.rect.x, icon.rect.y, icon.rect.width, icon.rect.height],
                uv_rect: uv(icon.name),
                tint: icon.tint,
                transform: [0.0, 0.0, 0.5, 0.5],
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct BubbleDrag {
    bubble_id: BubbleId,
    anchor: Point,
    original: Vec<Point>,
    delta: Point,
}

#[derive(Debug, Clone)]
struct BubbleVertexDrag {
    bubble_id: BubbleId,
    index: usize,
    keyframe_at_ms: Option<u64>,
    original: Vec<Point>,
    points: Vec<Point>,
}

#[derive(Debug, Clone, Copy)]
struct DraftVertexDrag {
    index: usize,
    original: Point,
    moved: bool,
}

#[derive(Debug, Clone, Copy)]
struct ShapeDrag {
    tool: Tool,
    start: Point,
    current: Point,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ShotDragMode {
    Create,
    Move {
        anchor: Point,
    },
    /// Resizing around the fixed opposite corner.
    Resize {
        fixed: Point,
    },
}

#[derive(Debug, Clone, Copy)]
struct ShotDrag {
    shot_id: Option<ShotId>,
    mode: ShotDragMode,
    original: Region,
    current: Region,
    moved: bool,
}

/// Zoomed editing view: `zoom` over the fitted page, `cx`/`cy` the page
/// point at the center of the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CanvasView {
    zoom: f32,
    cx: f32,
    cy: f32,
}

const MAX_CANVAS_ZOOM: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorTarget {
    Bubble(BubbleId),
    Text(BubbleId),
    Outline(BubbleId),
    TextOutline(BubbleId),
    Background,
}

#[derive(Debug, Clone)]
struct VertexEditor {
    bubble_id: BubbleId,
    playhead_ms: u64,
    selected_keyframe: Option<u64>,
    playing: Option<(Instant, u64)>,
}

#[derive(Debug, Clone, Copy)]
struct VertexEditorLayout {
    header: Rect,
    close: Rect,
    stage: Rect,
    timeline_panel: Rect,
    track: Rect,
    previous: Rect,
    play: Rect,
    next: Rect,
    add: Rect,
    delete: Rect,
}

impl VertexEditorLayout {
    fn compute(layout: ComicDubsLayout) -> Self {
        let header = Rect {
            height: VERTEX_EDITOR_HEADER_H.min(layout.toolbar.y - layout.content.y),
            ..layout.content
        };
        let timeline_panel = Rect {
            y: layout.content.y + layout.content.height - VERTEX_EDITOR_TIMELINE_H,
            height: VERTEX_EDITOR_TIMELINE_H,
            ..layout.content
        };
        let close = Rect {
            x: header.x + header.width - 44.0,
            y: header.y + (header.height - 32.0) * 0.5,
            width: 32.0,
            height: 32.0,
        };
        // The application transport row stays visible under the header.
        let top = layout.toolbar.y + layout.toolbar.height + 12.0;
        let stage = Rect {
            x: layout.content.x + 20.0,
            y: top,
            width: (layout.content.width - 40.0).max(0.0),
            height: (timeline_panel.y - top - 12.0).max(0.0),
        };
        let controls_y = timeline_panel.y + 12.0;
        let button = |x, width| Rect {
            x,
            y: controls_y,
            width,
            height: 30.0,
        };
        let previous = button(timeline_panel.x + 16.0, 42.0);
        let play = button(previous.x + previous.width + 6.0, 74.0);
        let next = button(play.x + play.width + 6.0, 42.0);
        let delete = button(timeline_panel.x + timeline_panel.width - 118.0, 102.0);
        let add = button(delete.x - 150.0, 142.0);
        let track = Rect {
            x: timeline_panel.x + 24.0,
            y: timeline_panel.y + 66.0,
            width: (timeline_panel.width - 48.0).max(1.0),
            height: 12.0,
        };
        Self {
            header,
            close,
            stage,
            timeline_panel,
            track,
            previous,
            play,
            next,
            add,
            delete,
        }
    }
}

/// What an inspector control does when activated.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    None,
    Action(UiAction),
    Local(Local),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Local {
    Color(ColorTarget),
    Tool(Tool),
    EditText,
    Select(Selection),
}

/// Numeric settings edited with a slider.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SliderKind {
    FontSize(BubbleId),
    LetterSpacing(BubbleId),
    LineSpacing(BubbleId),
    TextOutlineWidth(BubbleId),
    OutlineWidth(BubbleId),
    EntranceMs(BubbleId),
    EmphasisStrength(BubbleId),
    ScreenEffectMs(BubbleId),
    VoiceVolume(BubbleId),
    AudioDelay(BubbleId),
    SfxVolume(BubbleId),
    ExtraHold(BubbleId),
    TransitionMs(PageId),
    IntroMs(PageId),
    MotionStrength(PageId),
    ShotMove(ShotId),
    ShotHold(ShotId),
    MusicVolume,
    MusicFade,
    Typewriter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SliderSpec {
    kind: SliderKind,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
}

impl SliderSpec {
    fn ratio(&self) -> f32 {
        ((self.value - self.min) / (self.max - self.min).max(f32::EPSILON)).clamp(0.0, 1.0)
    }

    fn snapped(&self, value: f32) -> f32 {
        let value = value.clamp(self.min, self.max);
        ((value - self.min) / self.step).round() * self.step + self.min
    }

    fn at(&self, track: Rect, x: f32) -> f32 {
        let ratio = ((x - track.x) / track.width.max(1.0)).clamp(0.0, 1.0);
        self.snapped(self.min + ratio * (self.max - self.min))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Normal,
    Primary,
    Danger,
    Selected,
    Recording,
}

#[derive(Debug, Clone, PartialEq)]
struct Segment {
    label: String,
    icon: Option<&'static str>,
    selected: bool,
    command: Command,
    /// Accessible name, also shown as tooltip for icon-only segments.
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct DropdownOption {
    label: String,
    command: Command,
}

#[derive(Debug, Clone)]
enum ItemKind {
    /// Section header; clicking it folds the section.
    Section {
        title: String,
        icon: &'static str,
        collapsed: bool,
    },
    Info(String),
    /// Something the video will get wrong.
    Warning(String),
    Button {
        text: String,
        icon: Option<&'static str>,
        tone: Tone,
        command: Command,
    },
    Segmented(Vec<Segment>),
    Slider {
        name: String,
        display: String,
        spec: SliderSpec,
    },
    Dropdown {
        name: String,
        value: String,
        options: Vec<DropdownOption>,
        selected: Option<usize>,
    },
    Toggle {
        text: String,
        on: bool,
        command: Command,
    },
    Swatch {
        name: String,
        color: Option<[u8; 4]>,
        command: Command,
    },
    TextBox {
        text: String,
        editing: bool,
    },
    ListRow {
        icon: &'static str,
        text: String,
        detail: String,
        selected: bool,
        command: Command,
    },
}

#[derive(Debug, Clone)]
struct Item {
    id: String,
    rect: Rect,
    kind: ItemKind,
}

/// Lays inspector controls out top to bottom.
struct ItemBuilder<'a> {
    x: f32,
    width: f32,
    y: f32,
    items: Vec<Item>,
    collapsed: &'a std::collections::HashSet<String>,
    /// Inside a folded section: items are skipped.
    skipping: bool,
}

impl<'a> ItemBuilder<'a> {
    fn new(x: f32, width: f32, y: f32, collapsed: &'a std::collections::HashSet<String>) -> Self {
        Self {
            x,
            width,
            y,
            items: Vec::new(),
            collapsed,
            skipping: false,
        }
    }

    fn push(&mut self, key: &str, height: f32, kind: ItemKind) {
        if self.skipping {
            return;
        }
        self.items.push(Item {
            id: format!("comic.inspector.{key}"),
            rect: Rect {
                x: self.x,
                y: self.y,
                width: self.width,
                height,
            },
            kind,
        });
        self.y += height + ITEM_GAP;
    }

    fn section(&mut self, key: &str, title: &str, icon: &'static str) {
        if !self.items.is_empty() {
            self.y += 8.0;
        }
        let id = format!("comic.inspector.{key}");
        let collapsed = self.collapsed.contains(&id);
        self.skipping = false;
        self.push(
            key,
            28.0,
            ItemKind::Section {
                title: title.to_uppercase(),
                icon,
                collapsed,
            },
        );
        self.skipping = collapsed;
    }

    fn info(&mut self, key: &str, text: impl Into<String>) {
        let text = text.into();
        let lines = info_lines(&text, self.width).len() as f32;
        self.push(key, INFO_LINE_H * lines + 2.0, ItemKind::Info(text));
    }

    fn warning(&mut self, key: &str, text: impl Into<String>) {
        let text = text.into();
        let lines = info_lines(&text, self.width - 34.0).len() as f32;
        self.push(key, INFO_LINE_H * lines + 16.0, ItemKind::Warning(text));
    }

    fn button(
        &mut self,
        key: &str,
        text: impl Into<String>,
        icon: Option<&'static str>,
        tone: Tone,
        command: Command,
    ) {
        self.push(
            key,
            34.0,
            ItemKind::Button {
                text: text.into(),
                icon,
                tone,
                command,
            },
        );
    }

    /// Several buttons sharing one row.
    fn buttons(&mut self, key: &str, buttons: Vec<(String, Option<&'static str>, Tone, Command)>) {
        if self.skipping {
            return;
        }
        let count = buttons.len().max(1) as f32;
        let gap = 6.0;
        let width = (self.width - gap * (count - 1.0)) / count;
        for (index, (text, icon, tone, command)) in buttons.into_iter().enumerate() {
            self.items.push(Item {
                id: format!("comic.inspector.{key}.{index}"),
                rect: Rect {
                    x: self.x + index as f32 * (width + gap),
                    y: self.y,
                    width,
                    height: 34.0,
                },
                kind: ItemKind::Button {
                    text,
                    icon,
                    tone,
                    command,
                },
            });
        }
        self.y += 34.0 + ITEM_GAP;
    }

    fn segmented(&mut self, key: &str, segments: Vec<Segment>) {
        self.push(key, 32.0, ItemKind::Segmented(segments));
    }

    #[allow(clippy::too_many_arguments)]
    fn slider(
        &mut self,
        key: &str,
        name: &str,
        display: impl Into<String>,
        kind: SliderKind,
        value: f32,
        (min, max, step): (f32, f32, f32),
    ) {
        self.push(
            key,
            40.0,
            ItemKind::Slider {
                name: name.into(),
                display: display.into(),
                spec: SliderSpec {
                    kind,
                    value,
                    min,
                    max,
                    step,
                },
            },
        );
    }

    fn dropdown(
        &mut self,
        key: &str,
        name: &str,
        options: Vec<DropdownOption>,
        selected: Option<usize>,
    ) {
        let value = selected
            .and_then(|index| options.get(index))
            .map_or_else(|| "Choisir…".to_string(), |option| option.label.clone());
        self.push(
            key,
            50.0,
            ItemKind::Dropdown {
                name: name.into(),
                value,
                options,
                selected,
            },
        );
    }

    fn toggle(&mut self, key: &str, text: &str, on: bool, command: Command) {
        self.push(
            key,
            28.0,
            ItemKind::Toggle {
                text: text.into(),
                on,
                command,
            },
        );
    }

    fn swatches(&mut self, key: &str, swatches: Vec<(String, Option<[u8; 4]>, Command)>) {
        if self.skipping {
            return;
        }
        let count = swatches.len().max(1) as f32;
        let gap = 8.0;
        let width = (self.width - gap * (count - 1.0)) / count;
        for (index, (name, color, command)) in swatches.into_iter().enumerate() {
            self.items.push(Item {
                id: format!("comic.inspector.{key}.{index}"),
                rect: Rect {
                    x: self.x + index as f32 * (width + gap),
                    y: self.y,
                    width,
                    height: 50.0,
                },
                kind: ItemKind::Swatch {
                    name,
                    color,
                    command,
                },
            });
        }
        self.y += 50.0 + ITEM_GAP;
    }

    fn text_box(&mut self, key: &str, text: &str, editing: bool) {
        let lines = info_lines(text, self.width - 16.0).len().clamp(2, 5) as f32;
        self.push(
            key,
            lines * 16.0 + 18.0,
            ItemKind::TextBox {
                text: text.into(),
                editing,
            },
        );
    }

    fn list_row(
        &mut self,
        key: &str,
        icon: &'static str,
        text: impl Into<String>,
        detail: impl Into<String>,
        selected: bool,
        command: Command,
    ) {
        self.push(
            key,
            40.0,
            ItemKind::ListRow {
                icon,
                text: text.into(),
                detail: detail.into(),
                selected,
                command,
            },
        );
    }
}

fn segment(label: &str, selected: bool, command: Command) -> Segment {
    Segment {
        label: label.into(),
        icon: None,
        selected,
        command,
        name: label.into(),
    }
}

fn icon_segment(icon: &'static str, name: &str, selected: bool, command: Command) -> Segment {
    Segment {
        label: String::new(),
        icon: Some(icon),
        selected,
        command,
        name: name.into(),
    }
}

fn option(label: impl Into<String>, command: Command) -> DropdownOption {
    DropdownOption {
        label: label.into(),
        command,
    }
}

/// A choice list over one of the studio enums.
fn choice_options<T: Copy + PartialEq>(
    all: &[T],
    current: T,
    label: impl Fn(T) -> &'static str,
    command: impl Fn(T) -> Command,
) -> (Vec<DropdownOption>, Option<usize>) {
    (
        all.iter()
            .map(|value| option(label(*value), command(*value)))
            .collect(),
        all.iter().position(|value| *value == current),
    )
}

/// An open drop-down list.
#[derive(Debug, Clone)]
struct OpenDropdown {
    item_id: String,
    anchor: Rect,
    options: Vec<DropdownOption>,
    selected: Option<usize>,
    highlighted: usize,
    scroll: usize,
}

impl OpenDropdown {
    fn visible_rows(&self) -> usize {
        self.options.len().min(DROPDOWN_MAX_ROWS)
    }

    fn panel(&self, bounds: Rect) -> Rect {
        let height = self.visible_rows() as f32 * DROPDOWN_ROW_H + 8.0;
        let below = self.anchor.y + self.anchor.height + 4.0;
        let y = if below + height <= bounds.y + bounds.height {
            below
        } else {
            (self.anchor.y - height - 4.0).max(bounds.y)
        };
        Rect {
            x: self.anchor.x,
            y,
            width: self.anchor.width,
            height,
        }
    }

    fn row(&self, bounds: Rect, visible: usize) -> Rect {
        let panel = self.panel(bounds);
        Rect {
            x: panel.x + 4.0,
            y: panel.y + 4.0 + visible as f32 * DROPDOWN_ROW_H,
            width: panel.width - 8.0,
            height: DROPDOWN_ROW_H,
        }
    }

    fn keep_highlight_visible(&mut self) {
        let rows = self.visible_rows();
        if self.highlighted < self.scroll {
            self.scroll = self.highlighted;
        } else if self.highlighted >= self.scroll + rows {
            self.scroll = self.highlighted + 1 - rows;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SliderDrag {
    spec: SliderSpec,
    track: Rect,
    /// Whether the gesture already sent its first change.
    started: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct TextEdit {
    bubble_id: BubbleId,
    text: String,
    /// Caret position in characters.
    caret: usize,
}

impl TextEdit {
    fn byte_index(&self, caret: usize) -> usize {
        self.text
            .char_indices()
            .nth(caret)
            .map_or(self.text.len(), |(index, _)| index)
    }

    fn with_caret(&self) -> String {
        let mut text = self.text.clone();
        text.insert(self.byte_index(self.caret), '|');
        text
    }
}

#[derive(Default)]
pub struct ComicDubsWorkspaceUi {
    sidebar_tab: SidebarTab,
    sidebar_scroll: f32,
    selection: Selection,
    overview_tab: OverviewTab,
    draft: Vec<Point>,
    text_edit: Option<TextEdit>,
    dragging_audio: Option<ComicAudioId>,
    drag_position: (f32, f32),
    bubble_drag: Option<BubbleDrag>,
    bubble_vertex_drag: Option<BubbleVertexDrag>,
    draft_vertex_drag: Option<DraftVertexDrag>,
    shot_drag: Option<ShotDrag>,
    slider_drag: Option<SliderDrag>,
    dropdown: Option<OpenDropdown>,
    vertex_editor: Option<VertexEditor>,
    color_target: Option<ColorTarget>,
    color_picker: ColorPickerState,
    pending_audio_imports: usize,
    tool: Tool,
    shape_drag: Option<ShapeDrag>,
    inspector_scroll: f32,
    collapsed_sections: std::collections::HashSet<String>,
    hide_shots: bool,
    canvas_view: Option<CanvasView>,
    view_page: Option<PageId>,
    middle_pan: Option<(f32, f32)>,
    preview_ms: Option<u64>,
    playing: bool,
    scrubbing: bool,
    frame_aspect: Option<f32>,
    recording: Option<(BubbleId, f32)>,
    focused_control: Option<String>,
    hover: Option<(f32, f32)>,
    last_layout: ComicDubsLayout,
    layout_cache: RefCell<HashMap<u64, TextLayout>>,
}

impl ComicDubsWorkspaceUi {
    pub fn ensure_color_picker_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        self.color_picker
            .ensure_textures(device, queue, bind_group_layout, sampler);
    }

    pub fn render_color_picker<'a>(
        &'a self,
        bg: &mut Vec<QuadInstance>,
        textures: &mut Vec<(IconInstance, &'a wgpu::BindGroup)>,
        fg: &mut Vec<QuadInstance>,
    ) {
        self.color_picker.render(bg, textures, fg);
    }

    pub fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    pub fn selected_bubble(&self) -> Option<BubbleId> {
        match self.selection {
            Selection::Bubble(id) => Some(id),
            _ => None,
        }
    }

    pub fn selected_shot(&self) -> Option<ShotId> {
        match self.selection {
            Selection::Shot(id) => Some(id),
            _ => None,
        }
    }

    pub fn select_bubble(&mut self, bubble_id: Option<BubbleId>) {
        self.select(bubble_id.map_or(Selection::None, Selection::Bubble));
    }

    pub fn select_shot(&mut self, shot_id: Option<ShotId>) {
        self.select(shot_id.map_or(Selection::None, Selection::Shot));
    }

    fn select(&mut self, selection: Selection) {
        if self.selection != selection {
            self.inspector_scroll = 0.0;
            self.dropdown = None;
        }
        self.selection = selection;
        if self
            .text_edit
            .as_ref()
            .is_some_and(|edit| selection != Selection::Bubble(edit.bubble_id))
        {
            self.text_edit = None;
        }
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn preview_ms(&self) -> Option<u64> {
        self.preview_ms
    }

    pub fn vertex_editor_open(&self) -> bool {
        self.vertex_editor.is_some()
    }

    pub fn vertex_editor_playing(&self) -> bool {
        self.vertex_editor
            .as_ref()
            .is_some_and(|editor| editor.playing.is_some())
    }

    pub fn needs_animation(&self) -> bool {
        self.vertex_editor_playing() || self.recording.is_some()
    }

    pub fn open_vertex_editor(&mut self, bubble_id: BubbleId) {
        self.select(Selection::Bubble(bubble_id));
        self.text_edit = None;
        self.color_picker.close();
        self.color_target = None;
        self.vertex_editor = Some(VertexEditor {
            bubble_id,
            playhead_ms: 0,
            selected_keyframe: None,
            playing: None,
        });
    }

    pub fn close_vertex_editor(&mut self) -> bool {
        self.bubble_vertex_drag = None;
        self.vertex_editor.take().is_some()
    }

    pub fn set_vertex_editor_playhead(&mut self, at_ms: u64, project: &ComicDubsProject) {
        let Some(editor) = self.vertex_editor.as_mut() else {
            return;
        };
        let Some(bubble) = project.bubble(editor.bubble_id) else {
            self.vertex_editor = None;
            return;
        };
        editor.playhead_ms = at_ms.min(vertex_editor_duration_ms(project, bubble));
        editor.selected_keyframe = bubble
            .vertex_keyframes
            .iter()
            .find(|keyframe| keyframe.at_ms == editor.playhead_ms)
            .map(|keyframe| keyframe.at_ms);
        editor.playing = None;
    }

    pub fn toggle_vertex_editor_preview(&mut self, project: &ComicDubsProject) -> bool {
        let Some(editor) = self.vertex_editor.as_mut() else {
            return false;
        };
        let Some(bubble) = project.bubble(editor.bubble_id) else {
            self.vertex_editor = None;
            return true;
        };
        if editor.playing.take().is_none() {
            let duration = vertex_editor_duration_ms(project, bubble);
            if editor.playhead_ms >= duration {
                editor.playhead_ms = 0;
            }
            editor.playing = Some((Instant::now(), editor.playhead_ms));
        }
        true
    }

    pub fn nudge_vertex_editor(&mut self, delta_ms: i64, project: &ComicDubsProject) -> bool {
        let Some(playhead_ms) = self.vertex_editor.as_ref().map(|editor| editor.playhead_ms) else {
            return false;
        };
        self.set_vertex_editor_playhead(playhead_ms.saturating_add_signed(delta_ms), project);
        true
    }

    pub fn set_pending_audio_imports(&mut self, count: usize) {
        self.pending_audio_imports = count;
        if count > 0 {
            self.sidebar_tab = SidebarTab::Sounds;
        }
    }

    /// Shows the timeline frame at `at_ms` (playback or scrubbing), or the
    /// editor when `None`.
    pub fn set_preview(&mut self, at_ms: Option<u64>, playing: bool) {
        self.preview_ms = at_ms;
        self.playing = playing && at_ms.is_some();
        if at_ms.is_some() {
            self.text_edit = None;
            self.bubble_drag = None;
            self.bubble_vertex_drag = None;
            self.shape_drag = None;
            self.shot_drag = None;
            self.dropdown = None;
        }
    }

    /// Aspect ratio (width / height) of the exported video frame.
    pub fn set_frame_aspect(&mut self, aspect: Option<f32>) {
        self.frame_aspect = aspect.filter(|aspect| aspect.is_finite() && *aspect > 0.0);
    }

    fn aspect(&self) -> f32 {
        self.frame_aspect.unwrap_or(16.0 / 9.0)
    }

    pub fn set_recording(&mut self, recording: Option<(BubbleId, f32)>) {
        self.recording = recording;
    }

    /// Control holding the keyboard focus: arrows move the selected bubble
    /// when it is the canvas (or nothing), and adjust a focused slider.
    pub fn set_focused_control(&mut self, id: Option<&str>) {
        self.focused_control = id.map(str::to_owned);
    }

    fn arrows_move_selection(&self) -> bool {
        self.focused_control
            .as_deref()
            .is_none_or(|id| id.starts_with("comic.canvas."))
    }

    pub fn drop_accepts(&self, layout: ComicDubsLayout, x: f32, y: f32) -> bool {
        layout.sidebar.contains(x, y)
    }

    pub fn begin_text_edit(&mut self, bubble_id: BubbleId, text: String) {
        self.select(Selection::Bubble(bubble_id));
        let caret = text.chars().count();
        self.text_edit = Some(TextEdit {
            bubble_id,
            text,
            caret,
        });
    }

    /// Escape: closes the innermost transient state. Returns whether
    /// something was cancelled.
    pub fn cancel_draft(&mut self) -> bool {
        if self.close_vertex_editor() {
            return true;
        }
        if self.dropdown.take().is_some() {
            return true;
        }
        if self.text_edit.take().is_some() {
            return true;
        }
        if !self.draft.is_empty() || self.shape_drag.is_some() || self.shot_drag.is_some() {
            self.draft.clear();
            self.draft_vertex_drag = None;
            self.shape_drag = None;
            self.shot_drag = None;
            return true;
        }
        if self.preview_ms.is_some() && !self.playing {
            self.preview_ms = None;
            return true;
        }
        if self.tool != Tool::Select {
            self.tool = Tool::Select;
            return true;
        }
        if self.selection != Selection::None {
            self.select(Selection::None);
            return true;
        }
        false
    }

    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.draft.clear();
        self.draft_vertex_drag = None;
        self.shape_drag = None;
        self.shot_drag = None;
        self.text_edit = None;
        self.dropdown = None;
        if tool == Tool::Shot {
            self.hide_shots = false;
        }
    }

    /// Keyboard activation (Enter / Space) of a focused control.
    pub fn control_action(&mut self, id: &str, project: &ComicDubsProject) -> Option<UiAction> {
        if let Some(action) = self.vertex_editor_control(id, project) {
            return action;
        }
        // An open list chooses its highlighted (or activated) entry.
        if let Some(dropdown) = self.dropdown.take() {
            let index = id
                .strip_prefix("comic.dropdown.")
                .and_then(|index| index.parse::<usize>().ok())
                .unwrap_or(dropdown.highlighted);
            let command = dropdown.options.get(index)?.command.clone();
            return match self.run_command(command, project, self.last_layout, dropdown.anchor) {
                EventResponse::Action(action) => Some(action),
                _ => None,
            };
        }
        let number = |prefix: &str| {
            id.strip_prefix(prefix)
                .and_then(|value| value.parse::<u64>().ok())
        };
        if let Some(page_id) = number("comic.page.up.") {
            return Some(UiAction::ComicDubsMovePage { page_id, delta: -1 });
        }
        if let Some(page_id) = number("comic.page.down.") {
            return Some(UiAction::ComicDubsMovePage { page_id, delta: 1 });
        }
        if let Some(page_id) = number("comic.page.delete.") {
            return Some(UiAction::ComicDubsRemovePage(page_id));
        }
        if let Some(page_id) = number("comic.page.") {
            return Some(UiAction::ComicDubsSelectPage(page_id));
        }
        if let Some(audio_id) = number("comic.audio.play.") {
            return Some(UiAction::ComicDubsPlayAudio(audio_id));
        }
        if let Some(audio_id) = number("comic.audio.delete.") {
            return Some(UiAction::ComicDubsRemoveAudio(audio_id));
        }
        if let Some(audio_id) = number("comic.audio.") {
            return Some(match self.selected_bubble() {
                Some(bubble_id) => UiAction::ComicDubsAssignAudio {
                    bubble_id,
                    audio_id: Some(audio_id),
                },
                None => UiAction::ComicDubsPlayAudio(audio_id),
            });
        }
        if let Some(bubble_id) = number("comic.canvas.bubble.") {
            project.bubble(bubble_id)?;
            self.select(Selection::Bubble(bubble_id));
            return None;
        }
        if let Some(shot_id) = number("comic.canvas.shot.") {
            project.shot(shot_id)?;
            self.select(Selection::Shot(shot_id));
            return None;
        }
        if let Some(index) = number("comic.tool.") {
            if let Some(tool) = Tool::ALL.get(index as usize) {
                self.set_tool(*tool);
            }
            return None;
        }
        if let Some(index) = number("comic.sidebar.tab.") {
            self.sidebar_tab = if index == 0 {
                SidebarTab::Pages
            } else {
                SidebarTab::Sounds
            };
            self.sidebar_scroll = 0.0;
            return None;
        }
        if let Some(index) = number("comic.overview.") {
            self.overview_tab = if index == 0 {
                OverviewTab::Page
            } else {
                OverviewTab::Project
            };
            self.inspector_scroll = 0.0;
            return None;
        }
        match id {
            "comic.sidebar.import" => {
                return Some(match self.sidebar_tab {
                    SidebarTab::Pages => UiAction::ComicDubsImportImages,
                    SidebarTab::Sounds => UiAction::ComicDubsImportAudios,
                })
            }
            "comic.header.previous" | "comic.header.next" => {
                return adjacent_page(project, if id.ends_with("next") { 1 } else { -1 })
                    .map(UiAction::ComicDubsSelectPage);
            }
            "comic.header.shots" => {
                self.hide_shots = !self.hide_shots;
                return None;
            }
            "comic.header.zoom" => {
                self.canvas_view = None;
                return None;
            }
            "comic.timeline.play" => return Some(UiAction::ComicDubsTogglePlayback),
            "comic.inspector.deselect" => {
                self.select(Selection::None);
                return None;
            }
            _ => {}
        }
        if let Some(rest) = id.strip_prefix("comic.timeline.cue.") {
            let (page_index, bubble_index) = rest.split_once('.')?;
            let page = project.pages().get(page_index.parse::<usize>().ok()?)?;
            let bubble = page.bubbles.get(bubble_index.parse::<usize>().ok()?)?;
            return self.select_on_page(project, page.id, Selection::Bubble(bubble.id));
        }
        if let Some(shot_id) = number("comic.timeline.shot.") {
            let page_id = project.page_of_shot(shot_id)?;
            return self.select_on_page(project, page_id, Selection::Shot(shot_id));
        }
        if id.starts_with("comic.inspector.") {
            let layout = self.last_layout;
            let items = self.inspector_items(project, layout);
            for item in &items {
                let command = match &item.kind {
                    ItemKind::Button { command, .. }
                    | ItemKind::Toggle { command, .. }
                    | ItemKind::Swatch { command, .. }
                    | ItemKind::ListRow { command, .. }
                        if item.id == id =>
                    {
                        command.clone()
                    }
                    ItemKind::Segmented(segments) => {
                        let Some(index) = id
                            .strip_prefix(&format!("{}.", item.id))
                            .and_then(|index| index.parse::<usize>().ok())
                        else {
                            continue;
                        };
                        segments.get(index)?.command.clone()
                    }
                    ItemKind::Dropdown {
                        options, selected, ..
                    } if item.id == id => {
                        self.open_dropdown(item, options.clone(), *selected);
                        return None;
                    }
                    ItemKind::TextBox { .. } if item.id == id => Command::Local(Local::EditText),
                    ItemKind::Section { .. } if item.id == id => {
                        self.toggle_section(&item.id);
                        return None;
                    }
                    _ => continue,
                };
                return match self.run_command(command, project, layout, item.rect) {
                    EventResponse::Action(action) => Some(action),
                    _ => None,
                };
            }
        }
        None
    }

    fn vertex_editor_control(
        &mut self,
        id: &str,
        project: &ComicDubsProject,
    ) -> Option<Option<UiAction>> {
        if id == "comic.vertex.close" {
            return Some(Some(UiAction::ComicDubsCloseVertexEditor));
        }
        if id == "comic.vertex.play" {
            return Some(Some(UiAction::ComicDubsToggleVertexEditorPreview));
        }
        let editor = self.vertex_editor.as_ref()?;
        let bubble = project.bubble(editor.bubble_id)?;
        let action = match id {
            "comic.vertex.add" => Some(UiAction::ComicDubsSetBubbleVertexKeyframe {
                bubble_id: bubble.id,
                at_ms: editor.playhead_ms,
                points: bubble.points_at(editor.playhead_ms).to_vec(),
            }),
            "comic.vertex.delete" => editor.selected_keyframe.map(|at_ms| {
                UiAction::ComicDubsRemoveBubbleVertexKeyframe {
                    bubble_id: bubble.id,
                    at_ms,
                }
            }),
            "comic.vertex.previous" => Some(UiAction::ComicDubsSetVertexEditorPlayhead(
                previous_keyframe_at(bubble, editor.playhead_ms),
            )),
            "comic.vertex.next" => Some(UiAction::ComicDubsSetVertexEditorPlayhead(
                next_keyframe_at(
                    bubble,
                    editor.playhead_ms,
                    vertex_editor_duration_ms(project, bubble),
                ),
            )),
            _ => {
                let at_ms = id
                    .strip_prefix("comic.vertex.marker.")
                    .and_then(|value| value.parse().ok())?;
                Some(UiAction::ComicDubsSetVertexEditorPlayhead(at_ms))
            }
        };
        Some(action)
    }

    /// Selects something that may live on another page, switching to it.
    fn select_on_page(
        &mut self,
        project: &ComicDubsProject,
        page_id: PageId,
        selection: Selection,
    ) -> Option<UiAction> {
        self.select(selection);
        if !self.playing {
            self.preview_ms = None;
        }
        (project.active_page_id() != Some(page_id))
            .then_some(UiAction::ComicDubsSelectPage(page_id))
    }

    pub fn clear_document_state(&mut self) {
        let frame_aspect = self.frame_aspect;
        *self = Self::default();
        self.frame_aspect = frame_aspect;
    }

    pub fn sync(&mut self, project: &ComicDubsProject, layout: ComicDubsLayout) {
        self.last_layout = layout;
        if let Some(bubble_id) = self.vertex_editor.as_ref().map(|editor| editor.bubble_id) {
            if let Some(bubble) = project.bubble(bubble_id) {
                let duration = vertex_editor_duration_ms(project, bubble);
                let editor = self.vertex_editor.as_mut().unwrap();
                if let Some((started, from_ms)) = editor.playing {
                    editor.playhead_ms = from_ms
                        .saturating_add(started.elapsed().as_millis() as u64)
                        .min(duration);
                    if editor.playhead_ms >= duration {
                        editor.playing = None;
                    }
                    editor.selected_keyframe = bubble
                        .vertex_keyframes
                        .iter()
                        .find(|keyframe| keyframe.at_ms == editor.playhead_ms)
                        .map(|keyframe| keyframe.at_ms);
                }
            } else {
                self.vertex_editor = None;
            }
        }
        // The selection always belongs to the page on screen.
        let active = project.active_page_id();
        if self.view_page != active {
            self.view_page = active;
            self.canvas_view = None;
        }
        let valid = match self.selection {
            Selection::None => true,
            Selection::Bubble(id) => project
                .page_of_bubble(id)
                .is_some_and(|page| Some(page) == active),
            Selection::Shot(id) => project
                .page_of_shot(id)
                .is_some_and(|page| Some(page) == active),
        };
        if !valid {
            self.select(Selection::None);
        }
        if let Some(edit) = self.text_edit.as_mut() {
            if project.bubble(edit.bubble_id).is_none() {
                self.text_edit = None;
            } else {
                edit.caret = edit.caret.min(edit.text.chars().count());
            }
        }
        self.sidebar_scroll = self
            .sidebar_scroll
            .clamp(0.0, self.sidebar_max_scroll(project, layout));
    }

    fn sidebar_max_scroll(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> f32 {
        let (count, row) = match self.sidebar_tab {
            SidebarTab::Pages => (project.pages().len(), PAGE_CARD_H),
            SidebarTab::Sounds => (project.audios().len(), AUDIO_ROW_H),
        };
        (count as f32 * row - layout.sidebar_list().height).max(0.0)
    }

    fn toggle_section(&mut self, id: &str) {
        if !self.collapsed_sections.remove(id) {
            self.collapsed_sections.insert(id.to_string());
        }
    }

    /// Page rectangle on the canvas, zoomed and panned.
    fn page_rect(&self, canvas: Rect, page: &Page) -> Rect {
        let fit = image_rect(canvas, page);
        let Some(view) = self.canvas_view else {
            return fit;
        };
        let (width, height) = (fit.width * view.zoom, fit.height * view.zoom);
        Rect {
            x: canvas.x + canvas.width * 0.5 - view.cx * width,
            y: canvas.y + canvas.height * 0.5 - view.cy * height,
            width,
            height,
        }
    }

    fn zoom(&self) -> f32 {
        self.canvas_view.map_or(1.0, |view| view.zoom)
    }

    /// Zooms by `factor`, keeping the page point under `anchor` in place.
    fn zoom_canvas(&mut self, canvas: Rect, page: &Page, factor: f32, anchor: (f32, f32)) {
        let rect = self.page_rect(canvas, page);
        let point = (
            (anchor.0 - rect.x) / rect.width.max(1.0),
            (anchor.1 - rect.y) / rect.height.max(1.0),
        );
        let zoom = (self.zoom() * factor).clamp(1.0, MAX_CANVAS_ZOOM);
        if zoom <= 1.001 {
            self.canvas_view = None;
            return;
        }
        let fit = image_rect(canvas, page);
        let (width, height) = (fit.width * zoom, fit.height * zoom);
        self.canvas_view = Some(CanvasView {
            zoom,
            cx: (point.0 - (anchor.0 - canvas.x - canvas.width * 0.5) / width).clamp(0.0, 1.0),
            cy: (point.1 - (anchor.1 - canvas.y - canvas.height * 0.5) / height).clamp(0.0, 1.0),
        });
    }

    fn pan_canvas(&mut self, canvas: Rect, page: &Page, dx: f32, dy: f32) {
        let rect = self.page_rect(canvas, page);
        if let Some(view) = self.canvas_view.as_mut() {
            view.cx = (view.cx - dx / rect.width.max(1.0)).clamp(0.0, 1.0);
            view.cy = (view.cy - dy / rect.height.max(1.0)).clamp(0.0, 1.0);
        }
    }

    fn inspector_has_tabs(&self) -> bool {
        self.selection == Selection::None
    }
}

/// Page `delta` steps away from the active one.
fn adjacent_page(project: &ComicDubsProject, delta: isize) -> Option<PageId> {
    let active = project
        .active_page_id()
        .and_then(|id| project.pages().iter().position(|page| page.id == id))?;
    project
        .pages()
        .get(active.checked_add_signed(delta)?)
        .map(|page| page.id)
}

impl ComicDubsWorkspaceUi {
    pub fn handle_event(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> EventResponse {
        self.sync(project, layout);
        if self.vertex_editor.is_some() {
            return self.handle_vertex_editor_event(event, project, layout);
        }
        if let UiEvent::MouseMove { x, y } = event {
            self.hover = Some((*x, *y));
        }
        if self.color_picker.active {
            let before = self.color_picker.current_color();
            if self.color_picker.handle_event(event) {
                let after = self.color_picker.current_color();
                let target = self.color_target;
                if !self.color_picker.active {
                    self.color_target = None;
                }
                return target
                    .filter(|_| before != after)
                    .and_then(|target| color_action(target, rgba8(after), project))
                    .map_or(EventResponse::Consumed, EventResponse::Action);
            }
        }
        if let Some(response) = self.handle_dropdown(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_slider_drag(event, project) {
            return response;
        }
        if let Some(response) = self.handle_text_edit(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_slider_keys(event, project, layout) {
            return response;
        }

        let page = project.active_page();
        if let Some(page) = page.filter(|_| self.preview_ms.is_none()) {
            if let Some(response) = self.handle_view(event, layout, page) {
                return response;
            }
        }
        let page_rect = page.map(|page| self.page_rect(layout.canvas, page));

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b")
            && (!self.draft.is_empty() || self.shape_drag.is_some() || self.shot_drag.is_some())
        {
            self.cancel_draft();
            return EventResponse::Consumed;
        }
        if let Some(response) = self.handle_header(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_sidebar(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_inspector(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_timeline(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_drags(event, project, page, page_rect) {
            return response;
        }
        if let (Some(page), Some(page_rect)) = (page, page_rect) {
            if let Some(response) = self.handle_canvas(event, project, layout, page, page_rect) {
                return response;
            }
        }
        if matches!(event, UiEvent::Delete) {
            match self.selection {
                Selection::Bubble(id) => {
                    self.select(Selection::None);
                    return EventResponse::Action(UiAction::ComicDubsRemoveBubble(id));
                }
                Selection::Shot(id) => {
                    self.select(Selection::None);
                    return EventResponse::Action(UiAction::ComicDubsRemoveShot(id));
                }
                Selection::None => {}
            }
        }
        if let Some(bubble_id) = self
            .selected_bubble()
            .filter(|_| self.arrows_move_selection())
        {
            let delta = match event {
                UiEvent::CursorLeft => Some((-0.004, 0.0)),
                UiEvent::CursorRight => Some((0.004, 0.0)),
                UiEvent::CursorUp => Some((0.0, -0.004)),
                UiEvent::CursorDown => Some((0.0, 0.004)),
                UiEvent::ShiftCursorLeft => Some((-0.02, 0.0)),
                UiEvent::ShiftCursorRight => Some((0.02, 0.0)),
                _ => None,
            };
            if let Some((dx, dy)) = delta {
                return EventResponse::Action(UiAction::ComicDubsNudgeBubble { bubble_id, dx, dy });
            }
        }
        if let UiEvent::Scroll { x, y, delta, .. } = event {
            let step = if *delta > 0.0 { -56.0 } else { 56.0 };
            if layout.sidebar.contains(*x, *y) {
                self.sidebar_scroll = (self.sidebar_scroll + step)
                    .clamp(0.0, self.sidebar_max_scroll(project, layout));
                return EventResponse::Consumed;
            }
            if layout.inspector.contains(*x, *y) {
                let (_, content_height) = self.inspector_content(project, layout);
                let visible = layout.inspector_body(self.inspector_has_tabs()).height;
                self.inspector_scroll =
                    (self.inspector_scroll + step).clamp(0.0, (content_height - visible).max(0.0));
                return EventResponse::Consumed;
            }
        }
        EventResponse::Ignored
    }

    /// Canvas zoom (Ctrl + wheel) and panning (wheel, middle button).
    fn handle_view(
        &mut self,
        event: &UiEvent,
        layout: ComicDubsLayout,
        page: &Page,
    ) -> Option<EventResponse> {
        let canvas = layout.canvas;
        match event {
            UiEvent::Scroll {
                x, y, delta, ctrl, ..
            } if canvas.contains(*x, *y) => {
                if *ctrl {
                    let factor = if *delta > 0.0 { 1.12 } else { 1.0 / 1.12 };
                    self.zoom_canvas(canvas, page, factor, (*x, *y));
                } else if self.canvas_view.is_some() {
                    self.pan_canvas(canvas, page, 0.0, delta.signum() * 60.0);
                } else {
                    return None;
                }
                Some(EventResponse::Consumed)
            }
            UiEvent::MiddlePress { x, y } if canvas.contains(*x, *y) => {
                self.middle_pan = Some((*x, *y));
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseMove { x, y } if self.middle_pan.is_some() => {
                let (from_x, from_y) = self.middle_pan.unwrap_or((*x, *y));
                self.pan_canvas(canvas, page, x - from_x, y - from_y);
                self.middle_pan = Some((*x, *y));
                Some(EventResponse::Consumed)
            }
            UiEvent::MiddleRelease { .. } if self.middle_pan.is_some() => {
                self.middle_pan = None;
                Some(EventResponse::Consumed)
            }
            _ => None,
        }
    }

    fn open_dropdown(
        &mut self,
        item: &Item,
        options: Vec<DropdownOption>,
        selected: Option<usize>,
    ) {
        let mut dropdown = OpenDropdown {
            item_id: item.id.clone(),
            anchor: dropdown_field(item.rect),
            options,
            selected,
            highlighted: selected.unwrap_or(0),
            scroll: 0,
        };
        dropdown.keep_highlight_visible();
        self.dropdown = Some(dropdown);
    }

    fn handle_dropdown(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let dropdown = self.dropdown.as_mut()?;
        let bounds = layout.content;
        let panel = dropdown.panel(bounds);
        let row_at = |dropdown: &OpenDropdown, y: f32| {
            let visible = ((y - panel.y - 4.0) / DROPDOWN_ROW_H).floor();
            (visible >= 0.0 && (visible as usize) < dropdown.visible_rows())
                .then(|| visible as usize + dropdown.scroll)
        };
        let choose = |this: &mut Self, index: usize| {
            let dropdown = this.dropdown.take()?;
            let command = dropdown.options.get(index)?.command.clone();
            Some(this.run_command(command, project, layout, dropdown.anchor))
        };
        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
                if panel.contains(*x, *y) {
                    if let Some(index) = row_at(dropdown, *y) {
                        return Some(choose(self, index).unwrap_or(EventResponse::Consumed));
                    }
                    return Some(EventResponse::Consumed);
                }
                self.dropdown = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseMove { x, y } => {
                if panel.contains(*x, *y) {
                    if let Some(index) = row_at(dropdown, *y) {
                        dropdown.highlighted = index;
                    }
                }
                None
            }
            UiEvent::MouseRelease { .. } => Some(EventResponse::Consumed),
            UiEvent::Scroll { delta, .. } => {
                let max = dropdown.options.len() - dropdown.visible_rows();
                dropdown.scroll = if *delta > 0.0 {
                    dropdown.scroll.saturating_sub(1)
                } else {
                    (dropdown.scroll + 1).min(max)
                };
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorUp | UiEvent::CursorDown => {
                let last = dropdown.options.len().saturating_sub(1);
                dropdown.highlighted = if matches!(event, UiEvent::CursorUp) {
                    dropdown.highlighted.saturating_sub(1)
                } else {
                    (dropdown.highlighted + 1).min(last)
                };
                dropdown.keep_highlight_visible();
                Some(EventResponse::Consumed)
            }
            UiEvent::Activate => {
                let index = dropdown.highlighted;
                Some(choose(self, index).unwrap_or(EventResponse::Consumed))
            }
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" || text == " " => {
                let index = dropdown.highlighted;
                Some(choose(self, index).unwrap_or(EventResponse::Consumed))
            }
            UiEvent::KeyInput { text } if text == "\x1b" => {
                self.dropdown = None;
                Some(EventResponse::Consumed)
            }
            _ => None,
        }
    }

    fn handle_slider_drag(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
    ) -> Option<EventResponse> {
        let drag = self.slider_drag?;
        match event {
            UiEvent::MouseMove { x, .. } => {
                let value = drag.spec.at(drag.track, *x);
                if value == drag.spec.value {
                    return Some(EventResponse::Consumed);
                }
                if let Some(active) = self.slider_drag.as_mut() {
                    active.spec.value = value;
                    active.started = true;
                }
                Some(slider_response(
                    drag.spec.kind,
                    value,
                    project,
                    !drag.started,
                ))
            }
            UiEvent::MouseRelease { .. } => {
                self.slider_drag = None;
                Some(EventResponse::Consumed)
            }
            _ => None,
        }
    }

    /// Arrow keys on a focused slider.
    fn handle_slider_keys(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let direction = match event {
            UiEvent::CursorLeft | UiEvent::CursorDown => -1.0,
            UiEvent::CursorRight | UiEvent::CursorUp => 1.0,
            UiEvent::ShiftCursorLeft => -10.0,
            UiEvent::ShiftCursorRight => 10.0,
            _ => return None,
        };
        let focused = self.focused_control.as_deref()?;
        if !focused.starts_with("comic.inspector.") {
            return None;
        }
        let items = self.inspector_items(project, layout);
        let spec = items.iter().find_map(|item| match &item.kind {
            ItemKind::Slider { spec, .. } if item.id == focused => Some(*spec),
            _ => None,
        })?;
        let value = spec.snapped(spec.value + spec.step * direction);
        if value == spec.value {
            return Some(EventResponse::Consumed);
        }
        Some(slider_response(spec.kind, value, project, true))
    }

    fn handle_text_edit(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        if let UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } = event {
            if self.text_edit.is_some() {
                let in_box = self
                    .visible_inspector_items(project, layout)
                    .iter()
                    .any(|item| {
                        matches!(item.kind, ItemKind::TextBox { .. }) && item.rect.contains(*x, *y)
                    });
                if in_box {
                    return Some(EventResponse::Consumed);
                }
                self.text_edit = None;
            }
            return None;
        }
        let edit = self.text_edit.as_mut()?;
        let length = edit.text.chars().count();
        let changed = |edit: &TextEdit| {
            Some(EventResponse::Action(UiAction::ComicDubsSetBubbleText {
                bubble_id: edit.bubble_id,
                text: edit.text.clone(),
            }))
        };
        match event {
            UiEvent::KeyInput { text } if text == "\x1b" || text == "\r" || text == "\n" => {
                self.text_edit = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text } if text == "\x08" || text == "\x7f" => {
                if edit.caret == 0 {
                    return Some(EventResponse::Consumed);
                }
                edit.caret -= 1;
                let index = edit.byte_index(edit.caret);
                edit.text.remove(index);
                changed(edit)
            }
            UiEvent::Delete => {
                if edit.caret >= length {
                    return Some(EventResponse::Consumed);
                }
                let index = edit.byte_index(edit.caret);
                edit.text.remove(index);
                changed(edit)
            }
            UiEvent::KeyInput { text } => {
                let room = 500usize.saturating_sub(length);
                let inserted = text
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(room)
                    .collect::<String>();
                if inserted.is_empty() {
                    return Some(EventResponse::Consumed);
                }
                let index = edit.byte_index(edit.caret);
                edit.text.insert_str(index, &inserted);
                edit.caret += inserted.chars().count();
                changed(edit)
            }
            UiEvent::CursorLeft | UiEvent::ShiftCursorLeft => {
                edit.caret = edit.caret.saturating_sub(1);
                Some(EventResponse::Consumed)
            }
            UiEvent::CursorRight | UiEvent::ShiftCursorRight => {
                edit.caret = (edit.caret + 1).min(length);
                Some(EventResponse::Consumed)
            }
            UiEvent::Home | UiEvent::CursorUp => {
                edit.caret = 0;
                Some(EventResponse::Consumed)
            }
            UiEvent::End | UiEvent::CursorDown => {
                edit.caret = length;
                Some(EventResponse::Consumed)
            }
            UiEvent::MouseMove { .. }
            | UiEvent::MouseRelease { .. }
            | UiEvent::Scroll { .. }
            | UiEvent::FocusNext
            | UiEvent::FocusPrevious => None,
            _ => Some(EventResponse::Consumed),
        }
    }

    fn handle_header(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        if !layout.header.contains(*x, *y) {
            return None;
        }
        if let Some(index) =
            (0..Tool::ALL.len()).find(|index| layout.tool_button(*index).contains(*x, *y))
        {
            self.set_tool(Tool::ALL[index]);
            return Some(EventResponse::Consumed);
        }
        if layout.shots_toggle().contains(*x, *y) {
            self.hide_shots = !self.hide_shots;
            return Some(EventResponse::Consumed);
        }
        if layout.zoom_button().contains(*x, *y) {
            self.canvas_view = None;
            return Some(EventResponse::Consumed);
        }
        for (rect, delta) in [(layout.previous_page(), -1), (layout.next_page(), 1)] {
            if rect.contains(*x, *y) {
                return Some(
                    adjacent_page(project, delta).map_or(EventResponse::Consumed, |page| {
                        EventResponse::Action(UiAction::ComicDubsSelectPage(page))
                    }),
                );
            }
        }
        Some(EventResponse::Consumed)
    }

    fn handle_sidebar(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        if !layout.sidebar.contains(*x, *y) {
            return None;
        }
        for (index, tab) in [SidebarTab::Pages, SidebarTab::Sounds]
            .into_iter()
            .enumerate()
        {
            if layout.sidebar_tab(index).contains(*x, *y) {
                self.sidebar_tab = tab;
                self.sidebar_scroll = 0.0;
                return Some(EventResponse::Consumed);
            }
        }
        if layout.sidebar_import().contains(*x, *y) {
            return Some(EventResponse::Action(match self.sidebar_tab {
                SidebarTab::Pages => UiAction::ComicDubsImportImages,
                SidebarTab::Sounds => UiAction::ComicDubsImportAudios,
            }));
        }
        let list = layout.sidebar_list();
        if !list.contains(*x, *y) {
            return Some(EventResponse::Consumed);
        }
        match self.sidebar_tab {
            SidebarTab::Pages => {
                for (index, page) in project.pages().iter().enumerate() {
                    let card = self.page_card(layout, index);
                    if !card.contains(*x, *y) {
                        continue;
                    }
                    if project.active_page_id() == Some(page.id) {
                        for (rect, action) in [
                            (
                                page_card_button(card, 0),
                                UiAction::ComicDubsMovePage {
                                    page_id: page.id,
                                    delta: -1,
                                },
                            ),
                            (
                                page_card_button(card, 1),
                                UiAction::ComicDubsMovePage {
                                    page_id: page.id,
                                    delta: 1,
                                },
                            ),
                            (
                                page_card_button(card, 2),
                                UiAction::ComicDubsRemovePage(page.id),
                            ),
                        ] {
                            if rect.contains(*x, *y) {
                                return Some(EventResponse::Action(action));
                            }
                        }
                    }
                    if !self.playing {
                        self.preview_ms = None;
                    }
                    return Some(EventResponse::Action(UiAction::ComicDubsSelectPage(
                        page.id,
                    )));
                }
            }
            SidebarTab::Sounds => {
                for (index, audio) in project.audios().iter().enumerate() {
                    let row = self.audio_row(layout, index);
                    if !row.contains(*x, *y) {
                        continue;
                    }
                    if audio_row_button(row, 0).contains(*x, *y) {
                        return Some(EventResponse::Action(UiAction::ComicDubsPlayAudio(
                            audio.id,
                        )));
                    }
                    if audio_row_button(row, 1).contains(*x, *y) {
                        return Some(EventResponse::Action(UiAction::ComicDubsRemoveAudio(
                            audio.id,
                        )));
                    }
                    self.dragging_audio = Some(audio.id);
                    self.drag_position = (*x, *y);
                    return Some(EventResponse::Consumed);
                }
            }
        }
        Some(EventResponse::Consumed)
    }

    fn page_card(&self, layout: ComicDubsLayout, index: usize) -> Rect {
        let list = layout.sidebar_list();
        Rect {
            x: list.x,
            y: list.y + index as f32 * PAGE_CARD_H - self.sidebar_scroll,
            width: list.width,
            height: PAGE_CARD_H - 6.0,
        }
    }

    fn audio_row(&self, layout: ComicDubsLayout, index: usize) -> Rect {
        let list = layout.sidebar_list();
        Rect {
            x: list.x,
            y: list.y + index as f32 * AUDIO_ROW_H - self.sidebar_scroll,
            width: list.width,
            height: AUDIO_ROW_H - 6.0,
        }
    }

    fn handle_inspector(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let (UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }) = event else {
            return None;
        };
        if !layout.inspector.contains(*x, *y) {
            return None;
        }
        if self.inspector_has_tabs() {
            for (index, tab) in [OverviewTab::Page, OverviewTab::Project]
                .into_iter()
                .enumerate()
            {
                if layout.inspector_tab(index).contains(*x, *y) {
                    self.overview_tab = tab;
                    self.inspector_scroll = 0.0;
                    return Some(EventResponse::Consumed);
                }
            }
        } else if inspector_close(layout).contains(*x, *y) {
            self.select(Selection::None);
            return Some(EventResponse::Consumed);
        }
        let items = self.visible_inspector_items(project, layout);
        let Some(item) = items.iter().find(|item| item.rect.contains(*x, *y)) else {
            return Some(EventResponse::Consumed);
        };
        let command = match &item.kind {
            ItemKind::Button { command, .. }
            | ItemKind::Toggle { command, .. }
            | ItemKind::Swatch { command, .. }
            | ItemKind::ListRow { command, .. } => command.clone(),
            ItemKind::Segmented(segments) => {
                let index = segment_at(item.rect, segments.len(), *x);
                segments
                    .get(index)
                    .map_or(Command::None, |segment| segment.command.clone())
            }
            ItemKind::Slider { spec, .. } => {
                let track = slider_track(item.rect);
                let value = spec.at(track, *x);
                self.slider_drag = Some(SliderDrag {
                    spec: SliderSpec { value, ..*spec },
                    track,
                    started: value != spec.value,
                });
                return Some(if value == spec.value {
                    EventResponse::Consumed
                } else {
                    slider_response(spec.kind, value, project, true)
                });
            }
            ItemKind::Dropdown {
                options, selected, ..
            } => {
                if dropdown_field(item.rect).contains(*x, *y) {
                    self.open_dropdown(item, options.clone(), *selected);
                }
                return Some(EventResponse::Consumed);
            }
            ItemKind::TextBox { .. } => Command::Local(Local::EditText),
            ItemKind::Section { .. } => {
                self.toggle_section(&item.id);
                return Some(EventResponse::Consumed);
            }
            ItemKind::Info(_) | ItemKind::Warning(_) => Command::None,
        };
        Some(self.run_command(command, project, layout, item.rect))
    }

    fn run_command(
        &mut self,
        command: Command,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        anchor: Rect,
    ) -> EventResponse {
        match command {
            Command::None => EventResponse::Consumed,
            Command::Action(action) => {
                if matches!(
                    action,
                    UiAction::ComicDubsRemoveBubble(_) | UiAction::ComicDubsRemoveShot(_)
                ) {
                    self.select(Selection::None);
                }
                EventResponse::Action(action)
            }
            Command::Local(Local::Tool(tool)) => {
                self.set_tool(tool);
                EventResponse::Consumed
            }
            Command::Local(Local::Select(selection)) => {
                self.select(selection);
                if !self.playing {
                    self.preview_ms = None;
                }
                EventResponse::Consumed
            }
            Command::Local(Local::EditText) => {
                if let Some(bubble) = self.selected_bubble().and_then(|id| project.bubble(id)) {
                    self.begin_text_edit(bubble.id, bubble.text.clone());
                }
                EventResponse::Consumed
            }
            Command::Local(Local::Color(target)) => {
                let Some(color) = current_color(target, project) else {
                    return EventResponse::Consumed;
                };
                let (picker_w, picker_h) = ColorPickerState::panel_size();
                let picker_x = (layout.inspector.x - picker_w - 8.0).max(4.0);
                let picker_y = anchor
                    .y
                    .min(layout.content.y + layout.content.height - picker_h - 4.0)
                    .max(layout.content.y + 4.0);
                if matches!(target, ColorTarget::Bubble(_)) {
                    self.color_picker
                        .open_with_transparency(picker_x, picker_y, rgba(color), true);
                } else {
                    self.color_picker.open(picker_x, picker_y, rgba(color));
                }
                self.color_target = Some(target);
                EventResponse::Consumed
            }
        }
    }

    fn handle_timeline(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let track = layout.timeline_track();
        match event {
            UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y }
                if layout.timeline.contains(*x, *y) =>
            {
                if layout.timeline_play().contains(*x, *y) {
                    return Some(EventResponse::Action(UiAction::ComicDubsTogglePlayback));
                }
                let plan = Timeline::build(project, None, 40);
                if plan.is_empty() || *x < track.x - 4.0 {
                    return Some(EventResponse::Consumed);
                }
                let rows = layout.timeline_rows();
                if rows.shots.contains(*x, *y) {
                    if let Some((page, shot)) = shot_block_at(project, &plan, track, *x) {
                        let action = self.select_on_page(project, page, Selection::Shot(shot));
                        return Some(action.map_or(EventResponse::Consumed, EventResponse::Action));
                    }
                }
                if rows.bubbles.contains(*x, *y) || rows.sounds.contains(*x, *y) {
                    if let Some((page_index, cue)) = plan
                        .cues()
                        .find(|(_, cue)| cue_span(&plan, track, cue).contains(*x, rows.bubbles.y))
                    {
                        let page = &project.pages()[page_index];
                        if let Some(bubble) = page.bubbles.get(cue.bubble_index) {
                            let action =
                                self.select_on_page(project, page.id, Selection::Bubble(bubble.id));
                            return Some(
                                action.map_or(EventResponse::Consumed, EventResponse::Action),
                            );
                        }
                    }
                }
                if rows.pages.contains(*x, *y) {
                    if let Some(span) = plan.pages.iter().find(|span| {
                        let (start, end) = (
                            time_x(&plan, track, span.start_ms),
                            time_x(&plan, track, span.end_ms),
                        );
                        (start..end).contains(x)
                    }) {
                        if let Some(page) = project.pages().get(span.page_index) {
                            let action = self.select_on_page(project, page.id, Selection::None);
                            return Some(
                                action.map_or(EventResponse::Consumed, EventResponse::Action),
                            );
                        }
                    }
                }
                // Anywhere else: scrub the rendered video.
                let at_ms = time_at(&plan, track, *x);
                self.scrubbing = true;
                self.preview_ms = Some(at_ms);
                Some(EventResponse::Action(UiAction::ComicDubsSeek(at_ms)))
            }
            UiEvent::MouseMove { x, .. } if self.scrubbing => {
                let plan = Timeline::build(project, None, 40);
                let at_ms = time_at(&plan, track, *x);
                self.preview_ms = Some(at_ms);
                Some(EventResponse::Action(UiAction::ComicDubsSeek(at_ms)))
            }
            UiEvent::MouseRelease { .. } if self.scrubbing => {
                self.scrubbing = false;
                Some(EventResponse::Consumed)
            }
            _ => None,
        }
    }

    /// Pointer moves and releases of the canvas drags.
    fn handle_drags(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        page: Option<&Page>,
        page_rect: Option<Rect>,
    ) -> Option<EventResponse> {
        match event {
            UiEvent::MouseMove { x, y } => {
                if self.dragging_audio.is_some() {
                    self.drag_position = (*x, *y);
                    return Some(EventResponse::Consumed);
                }
                let (page, rect) = (page?, page_rect?);
                let pointer = point_at(rect, *x, *y);
                if let Some(drag) = self.shot_drag.as_mut() {
                    drag.current = match drag.mode {
                        ShotDragMode::Create => aspect_region(
                            page,
                            drag.original.center(),
                            pointer,
                            self.frame_aspect.unwrap_or(16.0 / 9.0),
                        ),
                        ShotDragMode::Resize { fixed } => aspect_region(
                            page,
                            fixed,
                            pointer,
                            self.frame_aspect.unwrap_or(16.0 / 9.0),
                        ),
                        ShotDragMode::Move { anchor } => Region {
                            x: (drag.original.x + pointer.x - anchor.x)
                                .clamp(0.0, (1.0 - drag.original.width).max(0.0)),
                            y: (drag.original.y + pointer.y - anchor.y)
                                .clamp(0.0, (1.0 - drag.original.height).max(0.0)),
                            ..drag.original
                        },
                    };
                    drag.moved |= drag.current != drag.original;
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.shape_drag.as_mut() {
                    drag.current = pointer;
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.draft_vertex_drag.as_mut() {
                    drag.moved |= (pointer.x - drag.original.x).abs() > 0.001
                        || (pointer.y - drag.original.y).abs() > 0.001;
                    self.draft[drag.index] = pointer;
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.bubble_vertex_drag.as_mut() {
                    drag.points[drag.index] = pointer;
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.bubble_drag.as_mut() {
                    let bounds = bubble_bounds(&drag.original);
                    drag.delta = Point {
                        x: (pointer.x - drag.anchor.x)
                            .clamp(-bounds.x, 1.0 - bounds.x - bounds.width),
                        y: (pointer.y - drag.anchor.y)
                            .clamp(-bounds.y, 1.0 - bounds.y - bounds.height),
                    };
                    return Some(EventResponse::Consumed);
                }
                None
            }
            UiEvent::MouseRelease { x, y } => {
                if let Some(audio_id) = self.dragging_audio.take() {
                    if let Some(response) = self.drop_audio_on_inspector(audio_id, project, *x, *y)
                    {
                        return Some(response);
                    }
                    let bubble_id = page
                        .zip(page_rect)
                        .and_then(|(page, rect)| bubble_at(page, rect, *x, *y));
                    return Some(bubble_id.map_or(EventResponse::Consumed, |bubble_id| {
                        EventResponse::Action(UiAction::ComicDubsAssignAudio {
                            bubble_id,
                            audio_id: Some(audio_id),
                        })
                    }));
                }
                if let Some(drag) = self.shot_drag.take() {
                    return Some(self.finish_shot_drag(drag, project, page?));
                }
                if let Some(drag) = self.shape_drag.take() {
                    return Some(self.finish_shape_drag(drag, page));
                }
                if let Some(drag) = self.draft_vertex_drag.take() {
                    if drag.index == 0 && !drag.moved && self.draft.len() >= 3 {
                        let points = std::mem::take(&mut self.draft);
                        self.tool = Tool::Select;
                        return Some(EventResponse::Action(UiAction::ComicDubsAddBubble {
                            page_id: page?.id,
                            points,
                        }));
                    }
                    return Some(EventResponse::Consumed);
                }
                if let Some(drag) = self.bubble_vertex_drag.take() {
                    return Some(if drag.points == drag.original {
                        EventResponse::Consumed
                    } else if let Some(at_ms) = drag.keyframe_at_ms {
                        EventResponse::Action(UiAction::ComicDubsSetBubbleVertexKeyframe {
                            bubble_id: drag.bubble_id,
                            at_ms,
                            points: drag.points,
                        })
                    } else {
                        EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                            bubble_id: drag.bubble_id,
                            points: drag.points,
                        })
                    });
                }
                if let Some(drag) = self.bubble_drag.take() {
                    let points = translated_drag_points(&drag);
                    return Some(if points == drag.original {
                        EventResponse::Consumed
                    } else {
                        EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                            bubble_id: drag.bubble_id,
                            points,
                        })
                    });
                }
                None
            }
            _ => None,
        }
    }

    fn handle_canvas(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        page: &Page,
        rect: Rect,
    ) -> Option<EventResponse> {
        let previewing = self.preview_ms.is_some();
        let (x, y) = match event {
            UiEvent::MousePress { x, y }
            | UiEvent::DoubleClick { x, y }
            | UiEvent::ShiftMousePress { x, y }
            | UiEvent::CtrlClick { x, y }
            | UiEvent::ContextMenu { x, y } => (*x, *y),
            _ => return None,
        };
        if !layout.canvas.contains(x, y) {
            return None;
        }
        if previewing {
            if !self.playing {
                self.preview_ms = None;
            }
            return Some(EventResponse::Consumed);
        }
        let point = point_at(rect, x, y);
        if let UiEvent::ContextMenu { .. } = event {
            let bubble_id = bubble_at(page, rect, x, y)?;
            self.select(Selection::Bubble(bubble_id));
            return Some(
                project
                    .bubble(bubble_id)
                    .and_then(|bubble| bubble.audio_id)
                    .map_or(EventResponse::Consumed, |audio_id| {
                        EventResponse::Action(UiAction::ComicDubsPlayAudio(audio_id))
                    }),
            );
        }
        if let UiEvent::DoubleClick { .. } = event {
            if let Some(id) = bubble_at(page, rect, x, y) {
                self.bubble_drag = None;
                let text = project.bubble(id).map(|bubble| bubble.text.clone())?;
                self.begin_text_edit(id, text);
                return Some(EventResponse::Consumed);
            }
        }
        // Polygon drafts.
        if !self.draft.is_empty() {
            if let Some(index) = vertex_at(rect, &self.draft, x, y) {
                self.draft_vertex_drag = Some(DraftVertexDrag {
                    index,
                    original: self.draft[index],
                    moved: false,
                });
            } else if self.draft.len() < 128 && rect.contains(x, y) {
                self.draft.push(point);
            }
            return Some(EventResponse::Consumed);
        }
        if matches!(event, UiEvent::CtrlClick { .. }) || self.tool == Tool::Polygon {
            if rect.contains(x, y) {
                self.draft.push(point);
                self.select(Selection::None);
            }
            return Some(EventResponse::Consumed);
        }
        // Camera shots: handles and tags first, whatever the tool.
        if let Some(response) = self.press_on_shots(page, rect, x, y, point) {
            return Some(response);
        }
        if self.tool.shape().is_some() {
            if rect.contains(x, y) {
                self.shape_drag = Some(ShapeDrag {
                    tool: self.tool,
                    start: point,
                    current: point,
                });
            }
            return Some(EventResponse::Consumed);
        }
        if let UiEvent::ShiftMousePress { .. } = event {
            if let Some(bubble) = self.selected_bubble().and_then(|id| project.bubble(id)) {
                if let Some(index) = vertex_at(rect, &bubble.points, x, y) {
                    return Some(EventResponse::Action(
                        UiAction::ComicDubsRemoveBubbleVertex {
                            bubble_id: bubble.id,
                            index,
                        },
                    ));
                }
                if let Some((after, point)) = edge_at(rect, &bubble.points, x, y) {
                    return Some(EventResponse::Action(
                        UiAction::ComicDubsInsertBubbleVertex {
                            bubble_id: bubble.id,
                            after,
                            point,
                        },
                    ));
                }
            }
        }
        if let Some(bubble) = self.selected_bubble().and_then(|id| project.bubble(id)) {
            if let Some(index) = vertex_at(rect, &bubble.points, x, y) {
                self.bubble_vertex_drag = Some(BubbleVertexDrag {
                    bubble_id: bubble.id,
                    index,
                    keyframe_at_ms: None,
                    original: bubble.points.clone(),
                    points: bubble.points.clone(),
                });
                return Some(EventResponse::Consumed);
            }
        }
        if let Some(bubble_id) = bubble_at(page, rect, x, y) {
            self.select(Selection::Bubble(bubble_id));
            self.bubble_drag = project.bubble(bubble_id).map(|bubble| BubbleDrag {
                bubble_id,
                anchor: point,
                original: bubble.points.clone(),
                delta: Point { x: 0.0, y: 0.0 },
            });
            return Some(EventResponse::Consumed);
        }
        // The edge of a shot selects it too.
        if !self.hide_shots {
            if let Some(shot) = self.shot_edge_at(page, rect, x, y) {
                self.select(Selection::Shot(shot.id));
                self.begin_shot_move(shot, point);
                return Some(EventResponse::Consumed);
            }
        }
        self.select(Selection::None);
        Some(EventResponse::Consumed)
    }

    /// Handles, tags and (with the shot tool) the inside of camera shots.
    fn press_on_shots(
        &mut self,
        page: &Page,
        rect: Rect,
        x: f32,
        y: f32,
        point: Point,
    ) -> Option<EventResponse> {
        if self.hide_shots && self.tool != Tool::Shot {
            return None;
        }
        let aspect = self.aspect();
        if let Some(shot) = self.selected_shot().and_then(|id| page_shot(page, id)) {
            if let Some(region) = shot.region {
                let screen = region_rect(rect, shot_view(page, Some(region), aspect));
                for (corner, fixed) in shot_corners(screen)
                    .into_iter()
                    .zip(opposite_corners(region))
                {
                    if (corner.0 - x).abs() <= SHOT_HANDLE && (corner.1 - y).abs() <= SHOT_HANDLE {
                        self.shot_drag = Some(ShotDrag {
                            shot_id: Some(shot.id),
                            mode: ShotDragMode::Resize { fixed },
                            original: region,
                            current: region,
                            moved: false,
                        });
                        return Some(EventResponse::Consumed);
                    }
                }
            }
        }
        for (index, shot) in page.shots.iter().enumerate().rev() {
            let screen = region_rect(rect, shot_view(page, shot.region, aspect));
            if shot_tag(screen, index, shot.region.is_none()).contains(x, y) {
                self.select(Selection::Shot(shot.id));
                self.begin_shot_move(shot, point);
                return Some(EventResponse::Consumed);
            }
        }
        if self.tool != Tool::Shot {
            return None;
        }
        if let Some(shot) = self.selected_shot().and_then(|id| page_shot(page, id)) {
            let screen = region_rect(rect, shot_view(page, shot.region, aspect));
            if shot.region.is_some() && screen.contains(x, y) {
                self.begin_shot_move(shot, point);
                return Some(EventResponse::Consumed);
            }
        }
        if rect.contains(x, y) {
            let start = Region {
                x: point.x,
                y: point.y,
                width: 0.0,
                height: 0.0,
            };
            self.shot_drag = Some(ShotDrag {
                shot_id: None,
                mode: ShotDragMode::Create,
                original: start,
                current: start,
                moved: false,
            });
        }
        Some(EventResponse::Consumed)
    }

    fn begin_shot_move(&mut self, shot: &CameraShot, anchor: Point) {
        if let Some(region) = shot.region {
            self.shot_drag = Some(ShotDrag {
                shot_id: Some(shot.id),
                mode: ShotDragMode::Move { anchor },
                original: region,
                current: region,
                moved: false,
            });
        }
    }

    fn shot_edge_at<'a>(
        &self,
        page: &'a Page,
        rect: Rect,
        x: f32,
        y: f32,
    ) -> Option<&'a CameraShot> {
        let aspect = self.aspect();
        page.shots.iter().rev().find(|shot| {
            shot.region.is_some() && {
                let screen = region_rect(rect, shot_view(page, shot.region, aspect));
                let near_x = x >= screen.x - 6.0 && x <= screen.x + screen.width + 6.0;
                let near_y = y >= screen.y - 6.0 && y <= screen.y + screen.height + 6.0;
                let on_vertical = near_y
                    && ((x - screen.x).abs() <= 6.0 || (x - screen.x - screen.width).abs() <= 6.0);
                let on_horizontal = near_x
                    && ((y - screen.y).abs() <= 6.0 || (y - screen.y - screen.height).abs() <= 6.0);
                on_vertical || on_horizontal
            }
        })
    }

    fn finish_shot_drag(
        &mut self,
        drag: ShotDrag,
        project: &ComicDubsProject,
        page: &Page,
    ) -> EventResponse {
        match (drag.mode, drag.shot_id) {
            (ShotDragMode::Create, _) => {
                let region = if drag.current.width < 0.03 {
                    // A click frames a standard shot around the pointer.
                    let center = drag.original.center();
                    Region {
                        x: center.x - 0.2,
                        y: center.y - 0.1,
                        width: 0.4,
                        height: 0.2,
                    }
                    .fitted(page.width, page.height, self.aspect())
                } else {
                    drag.current
                };
                EventResponse::Action(UiAction::ComicDubsAddShot {
                    page_id: page.id,
                    region: Some(region),
                })
            }
            (_, Some(shot_id)) if drag.moved => {
                let Some(shot) = project.shot(shot_id) else {
                    return EventResponse::Consumed;
                };
                EventResponse::Action(UiAction::ComicDubsSetShot(CameraShot {
                    region: Some(drag.current),
                    ..*shot
                }))
            }
            _ => EventResponse::Consumed,
        }
    }

    fn finish_shape_drag(&mut self, drag: ShapeDrag, page: Option<&Page>) -> EventResponse {
        let Some(page) = page else {
            return EventResponse::Consumed;
        };
        let Some((kind, preset)) = drag.tool.shape() else {
            return EventResponse::Consumed;
        };
        let tiny = (drag.current.x - drag.start.x).abs() < 0.01
            && (drag.current.y - drag.start.y).abs() < 0.01;
        let (start, end) = if tiny {
            // A plain click creates a standard-size bubble centered on it.
            let half_w = 0.11;
            let half_h = 0.11 * page.width as f32 / page.height.max(1) as f32 * 0.6;
            let cx = drag.start.x.clamp(half_w, 1.0 - half_w);
            let cy = drag.start.y.clamp(half_h, 1.0 - half_h);
            (
                Point {
                    x: cx - half_w,
                    y: cy - half_h,
                },
                Point {
                    x: cx + half_w,
                    y: cy + half_h,
                },
            )
        } else {
            (drag.start, drag.current)
        };
        let Some(points) = comic_dubs_shapes::shape_points(kind, start, end) else {
            return EventResponse::Consumed;
        };
        // Like most editors: draw, then type and adjust with the selection tool.
        self.tool = Tool::Select;
        EventResponse::Action(UiAction::ComicDubsAddStyledBubble {
            page_id: page.id,
            points,
            preset,
        })
    }

    fn drop_audio_on_inspector(
        &mut self,
        audio_id: ComicAudioId,
        project: &ComicDubsProject,
        x: f32,
        y: f32,
    ) -> Option<EventResponse> {
        let layout = self.last_layout;
        if !layout.inspector.contains(x, y) {
            return None;
        }
        let items = self.visible_inspector_items(project, layout);
        let item = items.iter().find(|item| item.rect.contains(x, y))?;
        let bubble = self.selected_bubble().and_then(|id| project.bubble(id));
        let action = match (item.id.as_str(), bubble) {
            ("comic.inspector.sound.voice", Some(bubble)) => UiAction::ComicDubsAssignAudio {
                bubble_id: bubble.id,
                audio_id: Some(audio_id),
            },
            ("comic.inspector.sound.sfx", Some(bubble)) => UiAction::ComicDubsSetBubbleSound {
                bubble_id: bubble.id,
                sound: BubbleSound {
                    sfx_audio_id: Some(audio_id),
                    ..bubble.sound
                },
            },
            ("comic.inspector.project.music", _) => UiAction::ComicDubsSetStudio(StudioSettings {
                music_audio_id: Some(audio_id),
                ..*project.studio()
            }),
            _ => return Some(EventResponse::Consumed),
        };
        Some(EventResponse::Action(action))
    }
}

/// Slider edits are coalesced into one undo step per gesture.
fn slider_response(
    kind: SliderKind,
    value: f32,
    project: &ComicDubsProject,
    first: bool,
) -> EventResponse {
    slider_action(kind, value, project).map_or(EventResponse::Consumed, |action| {
        EventResponse::Action(UiAction::ComicDubsGesture {
            first,
            action: Box::new(action),
        })
    })
}

fn slider_action(kind: SliderKind, value: f32, project: &ComicDubsProject) -> Option<UiAction> {
    let ms = value.round().max(0.0) as u64;
    let bubble = |id: BubbleId| project.bubble(id);
    let fx = |id: BubbleId, edit: &dyn Fn(&mut BubbleFx)| {
        let mut fx = bubble(id)?.fx;
        edit(&mut fx);
        Some(UiAction::ComicDubsSetBubbleFx { bubble_id: id, fx })
    };
    let sound = |id: BubbleId, edit: &dyn Fn(&mut BubbleSound)| {
        let mut sound = bubble(id)?.sound;
        edit(&mut sound);
        Some(UiAction::ComicDubsSetBubbleSound {
            bubble_id: id,
            sound,
        })
    };
    let look = |id: BubbleId, edit: &dyn Fn(&mut BubbleLook)| {
        let mut look = bubble(id)?.look;
        edit(&mut look);
        Some(UiAction::ComicDubsSetBubbleLook {
            bubble_id: id,
            look,
        })
    };
    let page_fx = |id: PageId, edit: &dyn Fn(&mut PageFx)| {
        let mut fx = project.page(id)?.fx;
        edit(&mut fx);
        Some(UiAction::ComicDubsSetPageFx { page_id: id, fx })
    };
    let shot = |id: ShotId, edit: &dyn Fn(&mut CameraShot)| {
        let mut shot = *project.shot(id)?;
        edit(&mut shot);
        Some(UiAction::ComicDubsSetShot(shot))
    };
    let studio = |edit: &dyn Fn(&mut StudioSettings)| {
        let mut studio = *project.studio();
        edit(&mut studio);
        Some(UiAction::ComicDubsSetStudio(studio))
    };
    match kind {
        SliderKind::FontSize(id) => Some(UiAction::ComicDubsSetBubbleFontSize {
            bubble_id: id,
            font_size: value,
        }),
        SliderKind::LetterSpacing(id) => Some(UiAction::ComicDubsSetBubbleLetterSpacing {
            bubble_id: id,
            spacing: value,
        }),
        SliderKind::LineSpacing(id) => Some(UiAction::ComicDubsSetBubbleLineSpacing {
            bubble_id: id,
            spacing: value,
        }),
        SliderKind::TextOutlineWidth(id) => look(id, &|look| look.text_outline_width = value),
        SliderKind::OutlineWidth(id) => look(id, &|look| look.outline_width = value),
        SliderKind::EntranceMs(id) => fx(id, &|fx| fx.entrance_ms = ms),
        SliderKind::EmphasisStrength(id) => fx(id, &|fx| fx.emphasis_strength = value),
        SliderKind::ScreenEffectMs(id) => fx(id, &|fx| fx.screen_effect_ms = ms),
        SliderKind::VoiceVolume(id) => sound(id, &|sound| sound.voice_volume = value),
        SliderKind::AudioDelay(id) => sound(id, &|sound| sound.audio_delay_ms = ms),
        SliderKind::SfxVolume(id) => sound(id, &|sound| sound.sfx_volume = value),
        SliderKind::ExtraHold(id) => sound(id, &|sound| sound.extra_hold_ms = ms),
        SliderKind::TransitionMs(id) => page_fx(id, &|fx| fx.transition_ms = ms),
        SliderKind::IntroMs(id) => page_fx(id, &|fx| fx.intro_ms = ms),
        SliderKind::MotionStrength(id) => page_fx(id, &|fx| fx.motion_strength = value),
        SliderKind::ShotMove(id) => shot(id, &|shot| shot.move_ms = ms),
        SliderKind::ShotHold(id) => shot(id, &|shot| shot.hold_ms = ms),
        SliderKind::MusicVolume => studio(&|studio| studio.music_volume = value),
        SliderKind::MusicFade => studio(&|studio| studio.music_fade_out_ms = ms),
        SliderKind::Typewriter => studio(&|studio| studio.typewriter_cps = value),
    }
}

/// Region of `aspect` spanned from `fixed` toward `pointer`, kept on the page.
fn aspect_region(page: &Page, fixed: Point, pointer: Point, aspect: f32) -> Region {
    let (page_w, page_h) = (page.width.max(1) as f32, page.height.max(1) as f32);
    let dx = (pointer.x - fixed.x) * page_w;
    let dy = (pointer.y - fixed.y) * page_h;
    let (right, down) = (dx >= 0.0, dy >= 0.0);
    let room_w = if right { 1.0 - fixed.x } else { fixed.x } * page_w;
    let room_h = if down { 1.0 - fixed.y } else { fixed.y } * page_h;
    let width = dx
        .abs()
        .max(dy.abs() * aspect)
        .min(room_w)
        .min(room_h * aspect)
        .max(0.0);
    let height = width / aspect;
    let x = if right {
        fixed.x * page_w
    } else {
        fixed.x * page_w - width
    };
    let y = if down {
        fixed.y * page_h
    } else {
        fixed.y * page_h - height
    };
    Region {
        x: x / page_w,
        y: y / page_h,
        width: width / page_w,
        height: height / page_h,
    }
}

/// Whether a bubble shows inside the shot that plays it.
fn bubble_in_shot(page: &Page, bubble_index: usize, shot: Option<usize>, aspect: f32) -> bool {
    let Some(region) = shot
        .and_then(|index| page.shots.get(index))
        .and_then(|shot| shot.region)
    else {
        return true;
    };
    page.bubbles.get(bubble_index).is_none_or(|bubble| {
        shot_view(page, Some(region), aspect)
            .contains(crate::comic_dubs::bubble_center(&bubble.points))
    })
}

fn page_shot(page: &Page, id: ShotId) -> Option<&CameraShot> {
    page.shots.iter().find(|shot| shot.id == id)
}

/// Page area a shot shows in the video (the page itself for a full-page shot).
fn shot_view(page: &Page, region: Option<Region>, aspect: f32) -> Region {
    match region {
        None => Region {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        Some(region) => {
            let (page_w, page_h) = (page.width.max(1) as f32, page.height.max(1) as f32);
            Camera::for_shot(Some(region), page_w, page_h, aspect)
                .visible_region(page_w, page_h, aspect)
        }
    }
}

fn region_rect(page_rect: Rect, region: Region) -> Rect {
    Rect {
        x: page_rect.x + region.x * page_rect.width,
        y: page_rect.y + region.y * page_rect.height,
        width: region.width * page_rect.width,
        height: region.height * page_rect.height,
    }
}

fn shot_corners(rect: Rect) -> [(f32, f32); 4] {
    [
        (rect.x, rect.y),
        (rect.x + rect.width, rect.y),
        (rect.x + rect.width, rect.y + rect.height),
        (rect.x, rect.y + rect.height),
    ]
}

fn opposite_corners(region: Region) -> [Point; 4] {
    let (x0, y0) = (region.x, region.y);
    let (x1, y1) = (region.x + region.width, region.y + region.height);
    [
        Point { x: x1, y: y1 },
        Point { x: x0, y: y1 },
        Point { x: x0, y: y0 },
        Point { x: x1, y: y0 },
    ]
}

fn shot_tag_text(index: usize, full_page: bool) -> String {
    if full_page {
        format!("PLAN {} · PAGE ENTIÈRE", index + 1)
    } else {
        format!("PLAN {}", index + 1)
    }
}

/// Label tab of a shot, in its bottom-left corner (speech bubbles usually
/// sit at the top of panels).
fn shot_tag(screen: Rect, index: usize, full_page: bool) -> Rect {
    let text = shot_tag_text(index, full_page);
    Rect {
        x: screen.x + 4.0,
        y: screen.y + screen.height - 4.0 - SHOT_TAG_H,
        width: 26.0 + text.chars().count() as f32 * 6.4,
        height: SHOT_TAG_H,
    }
}

fn inspector_close(layout: ComicDubsLayout) -> Rect {
    let header = layout.inspector_header();
    Rect {
        x: header.x + header.width - 42.0,
        y: header.y + 14.0,
        width: 30.0,
        height: 30.0,
    }
}

fn page_card_button(card: Rect, index: usize) -> Rect {
    Rect {
        x: card.x + card.width - 6.0 - (3 - index) as f32 * 26.0,
        y: card.y + 5.0,
        width: 24.0,
        height: 24.0,
    }
}

fn audio_row_button(row: Rect, index: usize) -> Rect {
    Rect {
        x: row.x + row.width - 6.0 - (2 - index) as f32 * 32.0,
        y: row.y + (row.height - 28.0) * 0.5,
        width: 28.0,
        height: 28.0,
    }
}

fn segment_at(rect: Rect, count: usize, x: f32) -> usize {
    (((x - rect.x) / rect.width.max(1.0)) * count as f32)
        .floor()
        .clamp(0.0, count.saturating_sub(1) as f32) as usize
}

fn segment_rect(rect: Rect, count: usize, index: usize) -> Rect {
    let width = rect.width / count.max(1) as f32;
    Rect {
        x: rect.x + index as f32 * width,
        width,
        ..rect
    }
}

fn slider_track(rect: Rect) -> Rect {
    Rect {
        x: rect.x + 2.0,
        y: rect.y + 24.0,
        width: rect.width - 4.0,
        height: 14.0,
    }
}

fn dropdown_field(rect: Rect) -> Rect {
    Rect {
        y: rect.y + 16.0,
        height: rect.height - 16.0,
        ..rect
    }
}

fn time_x(plan: &Timeline, track: Rect, at_ms: u64) -> f32 {
    track.x + at_ms.min(plan.total_ms) as f32 / plan.total_ms.max(1) as f32 * track.width
}

fn time_at(plan: &Timeline, track: Rect, x: f32) -> u64 {
    (((x - track.x) / track.width.max(1.0)).clamp(0.0, 1.0) * plan.total_ms as f32) as u64
}

/// Horizontal extent of a cue on the timeline (full track height).
fn cue_span(plan: &Timeline, track: Rect, cue: &timeline::Cue) -> Rect {
    let x = time_x(plan, track, cue.start_ms);
    let end = time_x(plan, track, cue.end_ms);
    Rect {
        x: x + 0.5,
        y: track.y,
        width: (end - x - 1.0).max(2.0),
        height: track.height,
    }
}

/// Shot block under `x` in the shots row: `(page id, shot id)`.
fn shot_block_at(
    project: &ComicDubsProject,
    plan: &Timeline,
    track: Rect,
    x: f32,
) -> Option<(PageId, ShotId)> {
    plan.pages.iter().find_map(|span| {
        let page = project.pages().get(span.page_index)?;
        span.shots.iter().find_map(|cue| {
            let (start, end) = (
                time_x(plan, track, cue.start_ms),
                time_x(plan, track, cue.end_ms),
            );
            ((start..end.max(start + 2.0)).contains(&x))
                .then(|| Some((page.id, page.shots.get(cue.shot_index)?.id)))
                .flatten()
        })
    })
}

impl ComicDubsWorkspaceUi {
    fn inspector_items(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> Vec<Item> {
        self.inspector_content(project, layout).0
    }

    /// Items positioned with the current scroll, and the content height.
    fn inspector_content(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> (Vec<Item>, f32) {
        let body = layout.inspector_body(self.inspector_has_tabs());
        let mut builder = ItemBuilder::new(
            layout.inspector.x + 14.0,
            (layout.inspector.width - 30.0).max(40.0),
            body.y + 6.0 - self.inspector_scroll,
            &self.collapsed_sections,
        );
        let plan = Timeline::build(project, None, 40);
        match self.selection {
            Selection::Bubble(id) => {
                if let Some(bubble) = project.bubble(id) {
                    self.bubble_items(&mut builder, project, bubble, &plan);
                }
            }
            Selection::Shot(id) => {
                if let Some(page) = project.page_of_shot(id).and_then(|page| project.page(page)) {
                    if let Some(index) = page.shot_index(id) {
                        self.shot_items(&mut builder, project, page, index, &plan);
                    }
                }
            }
            Selection::None => match self.overview_tab {
                OverviewTab::Page => self.page_items(&mut builder, project, &plan),
                OverviewTab::Project => self.project_items(&mut builder, project, &plan),
            },
        }
        let height = builder.y + self.inspector_scroll - body.y;
        (builder.items, height)
    }

    fn visible_inspector_items(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Vec<Item> {
        let body = layout.inspector_body(self.inspector_has_tabs());
        self.inspector_items(project, layout)
            .into_iter()
            .filter(|item| {
                item.rect.y >= body.y - 1.0
                    && item.rect.y + item.rect.height <= body.y + body.height + 1.0
            })
            .collect()
    }

    fn bubble_items(
        &self,
        builder: &mut ItemBuilder<'_>,
        project: &ComicDubsProject,
        bubble: &Bubble,
        plan: &Timeline,
    ) {
        let id = bubble.id;
        let look = bubble.look;
        let fx = bubble.fx;
        let sound = bubble.sound;
        let set_look = |look: BubbleLook| {
            Command::Action(UiAction::ComicDubsSetBubbleLook {
                bubble_id: id,
                look,
            })
        };
        let set_fx =
            |fx: BubbleFx| Command::Action(UiAction::ComicDubsSetBubbleFx { bubble_id: id, fx });
        let set_sound = |sound: BubbleSound| {
            Command::Action(UiAction::ComicDubsSetBubbleSound {
                bubble_id: id,
                sound,
            })
        };

        // Text.
        builder.section("text", "Texte", "comic/text");
        let editing = self.text_edit.as_ref().filter(|edit| edit.bubble_id == id);
        let shown = editing.map_or_else(|| bubble.text.clone(), TextEdit::with_caret);
        builder.text_box("text.content", &shown, editing.is_some());
        builder.slider(
            "text.size",
            "Taille du texte",
            format!("{} px", bubble.font_size.round()),
            SliderKind::FontSize(id),
            bubble.font_size,
            (6.0, 72.0, 1.0),
        );
        let style = |bold: bool, strikethrough: bool, underline: bool| {
            Command::Action(UiAction::ComicDubsSetBubbleTextStyle {
                bubble_id: id,
                bold,
                strikethrough,
                underline,
            })
        };
        let (bold, strike, underline) = (bubble.bold, bubble.strikethrough, bubble.underline);
        builder.segmented(
            "text.style",
            vec![
                segment("Gras", bold, style(!bold, strike, underline)),
                segment(
                    "Italique",
                    look.italic,
                    set_look(BubbleLook {
                        italic: !look.italic,
                        ..look
                    }),
                ),
                segment("Barré", strike, style(bold, !strike, underline)),
                segment("Souligné", underline, style(bold, strike, !underline)),
            ],
        );
        builder.segmented(
            "text.align",
            [
                ("comic/align-left", "Aligner à gauche", TextAlignment::Left),
                ("comic/align-center", "Centrer", TextAlignment::Center),
                (
                    "comic/align-right",
                    "Aligner à droite",
                    TextAlignment::Right,
                ),
            ]
            .into_iter()
            .map(|(icon, name, alignment)| {
                icon_segment(
                    icon,
                    name,
                    bubble.text_alignment == alignment,
                    Command::Action(UiAction::ComicDubsSetBubbleTextAlignment {
                        bubble_id: id,
                        alignment,
                    }),
                )
            })
            .collect(),
        );
        builder.slider(
            "text.letter_spacing",
            "Espacement des lettres",
            format!("{:.1} px", bubble.letter_spacing),
            SliderKind::LetterSpacing(id),
            bubble.letter_spacing,
            (0.0, 12.0, 0.5),
        );
        builder.slider(
            "text.line_spacing",
            "Interligne",
            format!("{:.2}×", bubble.line_spacing),
            SliderKind::LineSpacing(id),
            bubble.line_spacing,
            (0.8, 2.0, 0.05),
        );
        builder.swatches(
            "text.colors",
            vec![
                (
                    "Couleur du texte".into(),
                    Some(effective_text_color(bubble)),
                    Command::Local(Local::Color(ColorTarget::Text(id))),
                ),
                (
                    "Contour du texte".into(),
                    look.text_outline_color,
                    Command::Local(Local::Color(ColorTarget::TextOutline(id))),
                ),
            ],
        );
        if look.text_outline_color.is_some() {
            builder.slider(
                "text.outline_width",
                "Épaisseur du contour du texte",
                format!("{:.1} px", look.text_outline_width),
                SliderKind::TextOutlineWidth(id),
                look.text_outline_width,
                (0.5, 8.0, 0.5),
            );
            builder.button(
                "text.outline_remove",
                "Retirer le contour du texte",
                Some("comic/close"),
                Tone::Normal,
                set_look(BubbleLook {
                    text_outline_color: None,
                    ..look
                }),
            );
        }

        // Look.
        builder.section("style", "Apparence", "comic/brush");
        for (row, presets) in BubblePreset::ALL.chunks(2).enumerate() {
            builder.buttons(
                &format!("style.preset{row}"),
                presets
                    .iter()
                    .map(|preset| {
                        (
                            preset.label().to_string(),
                            None,
                            Tone::Normal,
                            Command::Action(UiAction::ComicDubsApplyPreset {
                                bubble_id: id,
                                preset: *preset,
                            }),
                        )
                    })
                    .collect(),
            );
        }
        builder.swatches(
            "style.colors",
            vec![
                (
                    "Fond".into(),
                    Some(bubble.color),
                    Command::Local(Local::Color(ColorTarget::Bubble(id))),
                ),
                (
                    "Contour".into(),
                    Some(look.outline_color.unwrap_or([225, 225, 235, 255])),
                    Command::Local(Local::Color(ColorTarget::Outline(id))),
                ),
            ],
        );
        builder.slider(
            "style.outline_width",
            "Épaisseur du contour",
            if look.outline_width <= 0.0 {
                "Aucun".to_string()
            } else {
                format!("{:.1} px", look.outline_width)
            },
            SliderKind::OutlineWidth(id),
            look.outline_width,
            (0.0, 12.0, 0.5),
        );
        builder.toggle(
            "style.shadow",
            "Ombre portée",
            look.shadow,
            set_look(BubbleLook {
                shadow: !look.shadow,
                ..look
            }),
        );
        builder.buttons(
            "style.shape",
            vec![
                (
                    "Ajouter une queue".into(),
                    Some("comic/tail"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsAddBubbleTail(id)),
                ),
                (
                    "Arrondir".into(),
                    Some("comic/smooth"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsSmoothBubble(id)),
                ),
            ],
        );
        builder.button(
            "style.vertices",
            "Animer la forme (poses)…",
            Some("comic/vertices"),
            Tone::Normal,
            Command::Action(UiAction::ComicDubsOpenVertexEditor(id)),
        );
        builder.buttons(
            "style.clipboard",
            vec![
                (
                    "Copier le style".into(),
                    Some("comic/copy"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsCopyStyle(id)),
                ),
                (
                    "Coller le style".into(),
                    Some("comic/paste"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsPasteStyle(id)),
                ),
            ],
        );
        builder.button(
            "style.apply_page",
            "Appliquer ce style à toute la page",
            Some("comic/layers"),
            Tone::Normal,
            Command::Action(UiAction::ComicDubsApplyStyleToPage(id)),
        );

        // Animation.
        builder.section("anim", "Animation", "comic/sparkle");
        let (options, selected) = choice_options(
            BubbleEntrance::ALL,
            fx.entrance,
            BubbleEntrance::label,
            |entrance| set_fx(BubbleFx { entrance, ..fx }),
        );
        builder.dropdown("anim.entrance", "Apparition", options, selected);
        if fx.entrance != BubbleEntrance::Cut {
            builder.slider(
                "anim.entrance_ms",
                "Durée de l'apparition",
                format_duration(fx.entrance_ms),
                SliderKind::EntranceMs(id),
                fx.entrance_ms as f32,
                (50.0, 2_000.0, 10.0),
            );
        }
        builder.toggle(
            "anim.whole",
            "Cacher toute la bulle avant son tour",
            fx.whole_bubble,
            set_fx(BubbleFx {
                whole_bubble: !fx.whole_bubble,
                ..fx
            }),
        );
        let (options, selected) = choice_options(
            TextReveal::ALL,
            fx.text_reveal,
            TextReveal::label,
            |text_reveal| set_fx(BubbleFx { text_reveal, ..fx }),
        );
        builder.dropdown("anim.reveal", "Affichage du texte", options, selected);
        let (options, selected) = choice_options(
            BubbleEmphasis::ALL,
            fx.emphasis,
            BubbleEmphasis::label,
            |emphasis| set_fx(BubbleFx { emphasis, ..fx }),
        );
        builder.dropdown("anim.emphasis", "Pendant la réplique", options, selected);
        if fx.emphasis != BubbleEmphasis::None {
            builder.slider(
                "anim.emphasis_strength",
                "Intensité",
                format!("{:.0} %", fx.emphasis_strength * 100.0),
                SliderKind::EmphasisStrength(id),
                fx.emphasis_strength,
                (0.25, 3.0, 0.05),
            );
        }
        let (options, selected) = choice_options(
            ScreenEffect::ALL,
            fx.screen_effect,
            ScreenEffect::label,
            |screen_effect| {
                set_fx(BubbleFx {
                    screen_effect,
                    ..fx
                })
            },
        );
        builder.dropdown(
            "anim.screen",
            "Effet d'écran à l'apparition",
            options,
            selected,
        );
        if fx.screen_effect != ScreenEffect::None {
            builder.slider(
                "anim.screen_ms",
                "Durée de l'effet",
                format_duration(fx.screen_effect_ms),
                SliderKind::ScreenEffectMs(id),
                fx.screen_effect_ms as f32,
                (100.0, 3_000.0, 10.0),
            );
        }
        builder.toggle(
            "anim.exit",
            "Disparaît après sa réplique",
            fx.exit_after,
            set_fx(BubbleFx {
                exit_after: !fx.exit_after,
                ..fx
            }),
        );
        builder.button(
            "anim.preview",
            "Aperçu de la bulle",
            Some("comic/play"),
            Tone::Primary,
            Command::Action(UiAction::ComicDubsPreviewBubble(id)),
        );

        // Sound.
        builder.section("sound", "Son", "comic/speaker");
        let (options, selected) = audio_options(project, bubble.audio_id, "Aucune voix", |audio| {
            Command::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: audio,
            })
        });
        builder.dropdown("sound.voice", "Voix", options, selected);
        let recording = self.recording.filter(|(bubble_id, _)| *bubble_id == id);
        builder.buttons(
            "sound.voice_actions",
            vec![
                (
                    "Écouter".into(),
                    Some("comic/play"),
                    Tone::Normal,
                    bubble.audio_id.map_or(Command::None, |audio| {
                        Command::Action(UiAction::ComicDubsPlayAudio(audio))
                    }),
                ),
                match recording {
                    Some((_, seconds)) => (
                        format!("Arrêter · {}", format_seconds((seconds * 1_000.0) as u64)),
                        Some("comic/stop"),
                        Tone::Recording,
                        Command::Action(UiAction::ComicDubsToggleVoiceRecording(id)),
                    ),
                    None => (
                        "Enregistrer".into(),
                        Some("comic/record"),
                        Tone::Normal,
                        Command::Action(UiAction::ComicDubsToggleVoiceRecording(id)),
                    ),
                },
            ],
        );
        builder.slider(
            "sound.voice_volume",
            "Volume de la voix",
            format!("{:.0} %", sound.voice_volume * 100.0),
            SliderKind::VoiceVolume(id),
            sound.voice_volume,
            (0.0, 2.0, 0.05),
        );
        builder.slider(
            "sound.delay",
            "Délai avant la voix",
            format_duration(sound.audio_delay_ms),
            SliderKind::AudioDelay(id),
            sound.audio_delay_ms as f32,
            (0.0, 3_000.0, 10.0),
        );
        let (options, selected) =
            audio_options(project, sound.sfx_audio_id, "Aucun bruitage", |audio| {
                set_sound(BubbleSound {
                    sfx_audio_id: audio,
                    ..sound
                })
            });
        builder.dropdown("sound.sfx", "Bruitage à l'apparition", options, selected);
        if sound.sfx_audio_id.is_some() {
            builder.slider(
                "sound.sfx_volume",
                "Volume du bruitage",
                format!("{:.0} %", sound.sfx_volume * 100.0),
                SliderKind::SfxVolume(id),
                sound.sfx_volume,
                (0.0, 2.0, 0.05),
            );
        }
        builder.slider(
            "sound.hold",
            "Temps de lecture après la réplique",
            format_duration(sound.extra_hold_ms),
            SliderKind::ExtraHold(id),
            sound.extra_hold_ms as f32,
            (0.0, 5_000.0, 50.0),
        );
        builder.info(
            "sound.hint",
            "Astuce : glissez un son de la bibliothèque sur « Voix » ou « Bruitage ».",
        );

        // Camera.
        let located = locate_bubble(project, id);
        builder.section("camera", "Caméra", "comic/shot");
        if let Some((page_index, bubble_index)) = located {
            let page = &project.pages()[page_index];
            if page.shots.is_empty() {
                builder.info(
                    "camera.none",
                    "Aucun plan sur cette page : la vidéo montre la page entière pendant cette bulle.",
                );
            } else {
                let shown = page.bubble_shots()[bubble_index];
                if !bubble_in_shot(page, bubble_index, shown, self.aspect()) {
                    builder.warning(
                        "camera.outside",
                        format!(
                            "Cette bulle est lue pendant le plan {}, mais elle est hors de son cadre : elle ne sera pas visible dans la vidéo. Cadrez-la dans un plan ou choisissez un autre plan.",
                            shown.unwrap_or(0) + 1
                        ),
                    );
                }
                let mut options = vec![option(
                    match (fx.shot, shown) {
                        (None, Some(shot)) => format!("Automatique (plan {})", shot + 1),
                        _ => "Automatique".to_string(),
                    },
                    set_fx(BubbleFx { shot: None, ..fx }),
                )];
                options.extend(page.shots.iter().enumerate().map(|(index, shot)| {
                    option(
                        shot_name(index, shot),
                        set_fx(BubbleFx {
                            shot: Some(shot.id),
                            ..fx
                        }),
                    )
                }));
                let selected = fx
                    .shot
                    .and_then(|shot| page.shot_index(shot))
                    .map_or(0, |index| index + 1);
                builder.dropdown(
                    "camera.shot",
                    "Plan qui montre la bulle",
                    options,
                    Some(selected),
                );
                if let Some(shot) = shown.and_then(|index| page.shots.get(index)) {
                    builder.button(
                        "camera.show",
                        format!("Sélectionner le plan {}", shown.unwrap_or(0) + 1),
                        Some("comic/shot"),
                        Tone::Normal,
                        Command::Local(Local::Select(Selection::Shot(shot.id))),
                    );
                }
            }
        }
        builder.button(
            "camera.around",
            "Nouveau plan cadré sur cette bulle",
            Some("comic/shot-add"),
            Tone::Primary,
            Command::Action(UiAction::ComicDubsAddShotAroundBubble(id)),
        );

        // Order.
        builder.section("order", "Organisation", "comic/layers");
        if let Some((page_index, bubble_index)) = located {
            let count = project.pages()[page_index].bubbles.len();
            let mut text = format!(
                "Bulle {} sur {} dans l'ordre de lecture.",
                bubble_index + 1,
                count
            );
            if let Some(cue) = plan.cue_for(page_index, bubble_index) {
                text.push_str(&format!(
                    " Apparaît à {}, occupe {}.",
                    format_time_ms(cue.reveal_ms),
                    format_seconds(cue.end_ms - cue.start_ms)
                ));
            }
            builder.info("order.info", text);
        }
        builder.buttons(
            "order.move",
            vec![
                (
                    "Lire plus tôt".into(),
                    Some("comic/arrow-up"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsMoveBubble {
                        bubble_id: id,
                        delta: -1,
                    }),
                ),
                (
                    "Lire plus tard".into(),
                    Some("comic/arrow-down"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsMoveBubble {
                        bubble_id: id,
                        delta: 1,
                    }),
                ),
            ],
        );
        builder.buttons(
            "order.edit",
            vec![
                (
                    "Dupliquer".into(),
                    Some("comic/duplicate"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsDuplicateBubble(id)),
                ),
                (
                    "Supprimer".into(),
                    Some("comic/trash"),
                    Tone::Danger,
                    Command::Action(UiAction::ComicDubsRemoveBubble(id)),
                ),
            ],
        );
    }

    fn shot_items(
        &self,
        builder: &mut ItemBuilder<'_>,
        project: &ComicDubsProject,
        page: &Page,
        index: usize,
        plan: &Timeline,
    ) {
        let shot = page.shots[index];
        let set = |shot: CameraShot| Command::Action(UiAction::ComicDubsSetShot(shot));
        let aspect = self.aspect();
        let assigned = page
            .bubble_shots()
            .iter()
            .enumerate()
            .filter(|(_, assigned)| **assigned == Some(index))
            .map(|(bubble, _)| bubble)
            .collect::<Vec<_>>();

        builder.section("shot.frame", "Cadrage", "comic/shot");
        let zone = shot.region.unwrap_or_else(|| {
            Region {
                x: 0.3,
                y: 0.3,
                width: 0.4,
                height: 0.4,
            }
            .fitted(page.width, page.height, aspect)
        });
        builder.segmented(
            "shot.kind",
            vec![
                segment(
                    "Zone de la page",
                    shot.region.is_some(),
                    set(CameraShot {
                        region: Some(zone),
                        ..shot
                    }),
                ),
                segment(
                    "Page entière",
                    shot.region.is_none(),
                    set(CameraShot {
                        region: None,
                        ..shot
                    }),
                ),
            ],
        );
        match shot.region {
            Some(region) => {
                builder.info(
                    "shot.frame_hint",
                    "Glissez le plan (ou son étiquette) pour le déplacer, un coin pour le redimensionner. Le cadre suit le format de la vidéo.",
                );
                let fitted = region.fitted(page.width, page.height, aspect);
                if (fitted.width - region.width).abs() > 0.002
                    || (fitted.height - region.height).abs() > 0.002
                {
                    builder.button(
                        "shot.fit",
                        "Ajuster au format de la vidéo",
                        Some("comic/shot"),
                        Tone::Normal,
                        set(CameraShot {
                            region: Some(fitted),
                            ..shot
                        }),
                    );
                }
            }
            None => builder.info("shot.frame_hint", "Le plan montre toute la page."),
        }

        builder.section("shot.motion", "Arrivée sur le plan", "comic/sparkle");
        if index == 0 {
            builder.info(
                "shot.first",
                "Premier plan de la page : la page s'ouvre directement dessus.",
            );
        } else {
            builder.segmented(
                "shot.movement",
                ShotMovement::ALL
                    .iter()
                    .map(|movement| {
                        segment(
                            movement.label(),
                            shot.movement == *movement,
                            set(CameraShot {
                                movement: *movement,
                                ..shot
                            }),
                        )
                    })
                    .collect(),
            );
            if shot.movement == ShotMovement::Smooth {
                builder.slider(
                    "shot.move_ms",
                    "Durée du mouvement",
                    format_duration(shot.move_ms),
                    SliderKind::ShotMove(shot.id),
                    shot.move_ms as f32,
                    (100.0, 5_000.0, 50.0),
                );
            }
        }
        builder.slider(
            "shot.hold",
            if assigned.is_empty() {
                "Durée du plan"
            } else {
                "Pause avant la première bulle"
            },
            if assigned.is_empty() && shot.hold_ms == 0 {
                format!("{} (par défaut)", format_duration(EMPTY_SHOT_HOLD_MS))
            } else {
                format_duration(shot.hold_ms)
            },
            SliderKind::ShotHold(shot.id),
            shot.hold_ms as f32,
            (0.0, 10_000.0, 100.0),
        );

        builder.section("shot.bubbles", "Bulles montrées", "comic/ellipse");
        if assigned.is_empty() {
            builder.info(
                "shot.no_bubble",
                "Aucune bulle dans ce plan : il est tenu seul, comme un plan d'ensemble.",
            );
        }
        for bubble_index in assigned {
            let bubble = &page.bubbles[bubble_index];
            let inside = bubble_in_shot(page, bubble_index, Some(index), aspect);
            builder.list_row(
                &format!("shot.bubble.{bubble_index}"),
                "comic/ellipse",
                format!("Bulle {}", bubble_index + 1),
                if !inside {
                    "Hors du cadre".to_string()
                } else if bubble.text.trim().is_empty() {
                    "Sans texte".to_string()
                } else {
                    ellipsize(&bubble.text, 34)
                },
                false,
                Command::Local(Local::Select(Selection::Bubble(bubble.id))),
            );
        }

        builder.section("shot.order", "Organisation", "comic/layers");
        let mut info = format!("Plan {} sur {} de la page.", index + 1, page.shots.len());
        let page_index = project
            .pages()
            .iter()
            .position(|candidate| candidate.id == page.id);
        let cue = page_index
            .and_then(|page_index| plan.span_for_page(page_index))
            .and_then(|span| span.shots.iter().find(|cue| cue.shot_index == index));
        if let Some(cue) = cue {
            info.push_str(&format!(
                " Commence à {}, dure {}.",
                format_time_ms(cue.start_ms),
                format_seconds(cue.end_ms - cue.start_ms)
            ));
        }
        builder.info("shot.info", info);
        builder.buttons(
            "shot.move",
            vec![
                (
                    "Plus tôt".into(),
                    Some("comic/arrow-up"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsMoveShot {
                        shot_id: shot.id,
                        delta: -1,
                    }),
                ),
                (
                    "Plus tard".into(),
                    Some("comic/arrow-down"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsMoveShot {
                        shot_id: shot.id,
                        delta: 1,
                    }),
                ),
            ],
        );
        builder.buttons(
            "shot.actions",
            vec![
                (
                    "Aperçu".into(),
                    Some("comic/play"),
                    Tone::Primary,
                    cue.map_or(Command::None, |cue| {
                        Command::Action(UiAction::ComicDubsPlayFrom(cue.start_ms))
                    }),
                ),
                (
                    "Supprimer".into(),
                    Some("comic/trash"),
                    Tone::Danger,
                    Command::Action(UiAction::ComicDubsRemoveShot(shot.id)),
                ),
            ],
        );
    }

    fn page_items(
        &self,
        builder: &mut ItemBuilder<'_>,
        project: &ComicDubsProject,
        plan: &Timeline,
    ) {
        let Some((index, page)) = project.active_page_id().and_then(|id| {
            project
                .pages()
                .iter()
                .enumerate()
                .find(|(_, page)| page.id == id)
        }) else {
            builder.section("page.empty_section", "Pour commencer", "comic/image");
            for (key, step) in [
                ("page.step1", "1. Importez les planches de votre BD."),
                ("page.step2", "2. Tracez les bulles et écrivez leur texte."),
                (
                    "page.step3",
                    "3. Enregistrez les voix, cadrez les plans caméra.",
                ),
                ("page.step4", "4. Exportez la vidéo (menu Export)."),
            ] {
                builder.info(key, step);
            }
            builder.button(
                "page.import",
                "Importer des planches…",
                Some("comic/upload"),
                Tone::Primary,
                Command::Action(UiAction::ComicDubsImportImages),
            );
            return;
        };
        let fx = page.fx;
        let id = page.id;
        let set = |fx: PageFx| Command::Action(UiAction::ComicDubsSetPageFx { page_id: id, fx });

        builder.section("page.shots", "Plans caméra", "comic/shot");
        if page.shots.is_empty() {
            builder.info(
                "page.shots_hint",
                "Sans plan, la vidéo montre la page entière. Ajoutez des plans pour zoomer sur les cases dans l'ordre de lecture : chaque plan montre ensuite ses bulles.",
            );
        }
        let assignment = page.bubble_shots();
        for (shot_index, shot) in page.shots.iter().enumerate() {
            let bubbles = assignment
                .iter()
                .filter(|assigned| **assigned == Some(shot_index))
                .count();
            builder.list_row(
                &format!("page.shot.{shot_index}"),
                "comic/shot",
                shot_name(shot_index, shot),
                match bubbles {
                    0 => "Aucune bulle".to_string(),
                    1 => "1 bulle".to_string(),
                    count => format!("{count} bulles"),
                },
                false,
                Command::Local(Local::Select(Selection::Shot(shot.id))),
            );
        }
        builder.buttons(
            "page.shot_add",
            vec![
                (
                    "Tracer un plan".into(),
                    Some("comic/shot"),
                    Tone::Primary,
                    Command::Local(Local::Tool(Tool::Shot)),
                ),
                (
                    "Page entière".into(),
                    Some("comic/page"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsAddShot {
                        page_id: id,
                        region: None,
                    }),
                ),
            ],
        );

        builder.section("page.transition", "Transition d'entrée", "comic/sparkle");
        let (options, selected) = choice_options(
            PageTransition::ALL,
            fx.transition,
            PageTransition::label,
            |transition| set(PageFx { transition, ..fx }),
        );
        builder.dropdown("page.transition_kind", "Transition", options, selected);
        if fx.transition != PageTransition::Cut {
            builder.slider(
                "page.transition_ms",
                "Durée de la transition",
                format_duration(fx.transition_ms),
                SliderKind::TransitionMs(id),
                fx.transition_ms as f32,
                (100.0, 3_000.0, 50.0),
            );
        }

        builder.section("page.rhythm", "Rythme", "comic/wave");
        builder.slider(
            "page.intro",
            "Pause avant la première bulle",
            format_duration(fx.intro_ms),
            SliderKind::IntroMs(id),
            fx.intro_ms as f32,
            (0.0, 10_000.0, 100.0),
        );
        let (options, selected) =
            choice_options(PageMotion::ALL, fx.motion, PageMotion::label, |motion| {
                set(PageFx { motion, ..fx })
            });
        builder.dropdown(
            "page.motion",
            "Mouvement lent de la caméra",
            options,
            selected,
        );
        if fx.motion != PageMotion::None {
            builder.slider(
                "page.motion_strength",
                "Amplitude du mouvement",
                format!("{:.0} %", fx.motion_strength * 100.0),
                SliderKind::MotionStrength(id),
                fx.motion_strength,
                (0.25, 3.0, 0.05),
            );
        }
        match plan.span_for_page(index) {
            Some(span) => {
                builder.info(
                    "page.timing",
                    format!(
                        "Sur la timeline : de {} à {} ({}).",
                        format_time_ms(span.start_ms),
                        format_time_ms(span.end_ms),
                        format_seconds(span.end_ms - span.start_ms)
                    ),
                );
                builder.button(
                    "page.play",
                    "Lire depuis cette page",
                    Some("comic/play"),
                    Tone::Primary,
                    Command::Action(UiAction::ComicDubsPlayFrom(span.start_ms)),
                );
            }
            None => builder.info(
                "page.timing",
                "Cette page n'a ni bulle ni plan : elle est ignorée à la lecture.",
            ),
        }
    }

    fn project_items(
        &self,
        builder: &mut ItemBuilder<'_>,
        project: &ComicDubsProject,
        plan: &Timeline,
    ) {
        let studio = *project.studio();
        let set = |studio: StudioSettings| Command::Action(UiAction::ComicDubsSetStudio(studio));
        builder.section("project.music_section", "Musique de fond", "comic/music");
        let (options, selected) =
            audio_options(project, studio.music_audio_id, "Aucune musique", |audio| {
                set(StudioSettings {
                    music_audio_id: audio,
                    ..studio
                })
            });
        builder.dropdown("project.music", "Musique", options, selected);
        if studio.music_audio_id.is_some() {
            builder.slider(
                "project.music_volume",
                "Volume de la musique",
                format!("{:.0} %", studio.music_volume * 100.0),
                SliderKind::MusicVolume,
                studio.music_volume,
                (0.0, 1.0, 0.01),
            );
            builder.toggle(
                "project.music_loop",
                "Lecture en boucle",
                studio.music_loop,
                set(StudioSettings {
                    music_loop: !studio.music_loop,
                    ..studio
                }),
            );
            builder.toggle(
                "project.music_ducking",
                "Baisser la musique sous les voix",
                studio.music_ducking,
                set(StudioSettings {
                    music_ducking: !studio.music_ducking,
                    ..studio
                }),
            );
            builder.slider(
                "project.music_fade",
                "Fondu de fin",
                format_duration(studio.music_fade_out_ms),
                SliderKind::MusicFade,
                studio.music_fade_out_ms as f32,
                (0.0, 10_000.0, 100.0),
            );
        }

        builder.section("project.render_section", "Rendu", "comic/image");
        let [r, g, b] = studio.background;
        builder.swatches(
            "project.colors",
            vec![(
                "Fond de la vidéo".into(),
                Some([r, g, b, 255]),
                Command::Local(Local::Color(ColorTarget::Background)),
            )],
        );
        builder.slider(
            "project.typewriter",
            "Vitesse de la machine à écrire",
            format!("{:.0} car./s", studio.typewriter_cps),
            SliderKind::Typewriter,
            studio.typewriter_cps,
            (5.0, 120.0, 1.0),
        );

        builder.section("project.stats_section", "Bilan", "comic/check");
        let bubbles = project
            .pages()
            .iter()
            .flat_map(|page| &page.bubbles)
            .collect::<Vec<_>>();
        let voiced = bubbles
            .iter()
            .filter(|bubble| bubble.audio_id.is_some())
            .count();
        let silent_text = bubbles
            .iter()
            .filter(|bubble| bubble.text.trim().is_empty())
            .count();
        let shots = project
            .pages()
            .iter()
            .map(|page| page.shots.len())
            .sum::<usize>();
        builder.info(
            "project.stats",
            format!(
                "Durée {} • {} page(s) • {} bulle(s) dont {} avec voix • {} plan(s)",
                format_time_ms(plan.total_ms),
                project.pages().len(),
                bubbles.len(),
                voiced,
                shots
            ),
        );
        if bubbles.len() > voiced || silent_text > 0 {
            builder.info(
                "project.warnings",
                format!(
                    "À vérifier : {} bulle(s) sans voix, {} sans texte.",
                    bubbles.len() - voiced,
                    silent_text
                ),
            );
        }

        builder.section(
            "project.script_section",
            "Script et sous-titres",
            "comic/script",
        );
        builder.buttons(
            "project.script",
            vec![
                (
                    "Importer le script".into(),
                    Some("comic/upload"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsImportScript),
                ),
                (
                    "Exporter".into(),
                    Some("comic/download"),
                    Tone::Normal,
                    Command::Action(UiAction::ComicDubsExportScript),
                ),
            ],
        );
        builder.button(
            "project.srt",
            "Exporter les sous-titres (SRT)",
            Some("comic/subtitles"),
            Tone::Normal,
            Command::Action(UiAction::ComicDubsExportSrt),
        );
        builder.info(
            "project.script_hint",
            "Le script liste une ligne par bulle dans l'ordre de lecture, idéal pour les traducteurs et les comédiens.",
        );
    }
}

/// "No audio" then every library audio.
fn audio_options(
    project: &ComicDubsProject,
    current: Option<ComicAudioId>,
    none: &str,
    command: impl Fn(Option<ComicAudioId>) -> Command,
) -> (Vec<DropdownOption>, Option<usize>) {
    let mut options = vec![option(none, command(None))];
    options.extend(project.audios().iter().map(|audio| {
        option(
            format!(
                "{} ({})",
                audio.file_name,
                format_seconds(audio.duration_ms())
            ),
            command(Some(audio.id)),
        )
    }));
    let selected = match current {
        None => Some(0),
        Some(id) => project
            .audios()
            .iter()
            .position(|audio| audio.id == id)
            .map(|index| index + 1),
    };
    (options, selected)
}

fn shot_name(index: usize, shot: &CameraShot) -> String {
    if shot.region.is_none() {
        format!("Plan {} · page entière", index + 1)
    } else {
        format!("Plan {}", index + 1)
    }
}

fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms} ms")
    } else {
        format_seconds(ms)
    }
}

impl ComicDubsWorkspaceUi {
    pub fn scene(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> ComicDubsScene {
        if self.vertex_editor.is_some() {
            return self.vertex_editor_scene(project, layout);
        }
        let mut scene = ComicDubsScene::default();
        let plan = Timeline::build(project, None, 40);
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.sidebar, PANEL, [0.0; 4], 0.0));
        scene
            .quads
            .push(quad(layout.inspector, PANEL, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.header, PANEL, [0.0; 4], 0.0));
        scene
            .quads
            .push(quad(layout.timeline, PANEL, [0.0; 4], 0.0));
        for (x, y, width, height) in [
            (
                layout.sidebar.x + layout.sidebar.width - 1.0,
                layout.sidebar.y,
                1.0,
                layout.sidebar.height,
            ),
            (
                layout.inspector.x,
                layout.inspector.y,
                1.0,
                layout.inspector.height,
            ),
            (
                layout.header.x,
                layout.header.y + layout.header.height - 1.0,
                layout.header.width,
                1.0,
            ),
            (
                layout.timeline.x,
                layout.timeline.y,
                layout.timeline.width,
                1.0,
            ),
        ] {
            scene.quads.push(quad(
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
                BORDER_SOFT,
                [0.0; 4],
                0.0,
            ));
        }
        self.render_sidebar(project, layout, &mut scene);
        self.render_header(project, layout, &mut scene);
        self.render_inspector(project, layout, &plan, &mut scene);
        self.render_timeline(project, layout, &plan, &mut scene);
        match self.preview_ms.filter(|_| !plan.is_empty()) {
            Some(at_ms) => self.render_preview(project, layout, &plan, at_ms, &mut scene),
            None => self.render_editor_canvas(project, layout, &mut scene),
        }
        if let Some((_, seconds)) = self.recording {
            let badge = Rect {
                x: layout.canvas.x + layout.canvas.width * 0.5 - 140.0,
                y: layout.canvas.y + 8.0,
                width: 280.0,
                height: 32.0,
            };
            scene
                .overlay_quads
                .push(quad(badge, [0.45, 0.04, 0.07, 0.95], RECORD_COLOR, 16.0));
            scene.overlay_icons.push(SceneIcon {
                name: "comic/record",
                rect: Rect {
                    x: badge.x + 14.0,
                    y: badge.y + 9.0,
                    width: 14.0,
                    height: 14.0,
                },
                tint: [1.0, 0.45, 0.5, 1.0],
            });
            overlay_label(
                &mut scene,
                &format!(
                    "Enregistrement de la voix · {}",
                    format_seconds((seconds * 1_000.0) as u64)
                ),
                Rect {
                    x: badge.x + 30.0,
                    width: badge.width - 36.0,
                    ..badge
                },
                HAlign::Center,
                12.0,
                TEXT,
            );
        }
        if let Some(audio_id) = self.dragging_audio {
            let name = project
                .audio(audio_id)
                .map(|audio| audio.file_name.as_str())
                .unwrap_or("Audio");
            let ghost = Rect {
                x: self.drag_position.0 + 12.0,
                y: self.drag_position.1 + 10.0,
                width: 210.0,
                height: 32.0,
            };
            scene
                .popup_quads
                .push(quad(ghost, [0.13, 0.11, 0.26, 0.97], ACCENT, 8.0));
            scene.popup_icons.push(SceneIcon {
                name: "comic/audio",
                rect: Rect {
                    x: ghost.x + 10.0,
                    y: ghost.y + 8.0,
                    width: 16.0,
                    height: 16.0,
                },
                tint: ICON,
            });
            popup_label(
                &mut scene,
                &ellipsize(name, 26),
                Rect {
                    x: ghost.x + 30.0,
                    width: ghost.width - 34.0,
                    ..ghost
                },
                HAlign::Left,
                12.0,
                TEXT,
            );
        }
        self.render_dropdown(layout, &mut scene);
        scene
    }

    fn hovered(&self, rect: Rect) -> bool {
        self.hover.is_some_and(|(x, y)| rect.contains(x, y))
            && self.dropdown.is_none()
            && self.slider_drag.is_none()
    }

    // ------------------------------------------------------------- sidebar

    fn render_sidebar(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        for (index, (tab, icon, name, count)) in [
            (
                SidebarTab::Pages,
                "comic/image",
                "Planches",
                project.pages().len(),
            ),
            (
                SidebarTab::Sounds,
                "comic/audio",
                "Sons",
                project.audios().len(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = layout.sidebar_tab(index);
            let active = self.sidebar_tab == tab;
            scene.quads.push(quad(
                rect,
                if active {
                    ACCENT_SOFT
                } else if self.hovered(rect) {
                    PANEL_HOVER
                } else {
                    PANEL_ALT
                },
                if active { ACCENT } else { [0.0; 4] },
                8.0,
            ));
            icon_text(
                scene,
                rect,
                Some(icon),
                &format!("{name} ({count})"),
                12.0,
                if active { TEXT } else { MUTED },
                if active { ICON } else { ICON_MUTED },
            );
            push_control(
                scene,
                &format!("comic.sidebar.tab.{index}"),
                &format!("{name}, {count} élément(s)"),
                rect,
                AccessibleRole::Tab,
                active,
                None,
            );
        }
        let list = layout.sidebar_list();
        let visible = |rect: Rect| {
            rect.y >= list.y - 1.0 && rect.y + rect.height <= list.y + list.height + 1.0
        };
        match self.sidebar_tab {
            SidebarTab::Pages => {
                if project.pages().is_empty() {
                    info_block(
                        scene,
                        "Aucune planche pour l'instant. Importez vos pages de BD ou glissez les images ici.",
                        Rect {
                            y: list.y + 20.0,
                            height: 60.0,
                            ..list
                        },
                    );
                }
                for (index, page) in project.pages().iter().enumerate() {
                    let card = self.page_card(layout, index);
                    if !visible(card) {
                        continue;
                    }
                    self.render_page_card(
                        scene,
                        page,
                        index,
                        card,
                        project.active_page_id() == Some(page.id),
                    );
                }
            }
            SidebarTab::Sounds => {
                if project.audios().is_empty() {
                    info_block(
                        scene,
                        "Aucun son. Importez des voix, des bruitages ou une musique, ou enregistrez directement une bulle.",
                        Rect {
                            y: list.y + 20.0,
                            height: 60.0,
                            ..list
                        },
                    );
                }
                for (index, audio) in project.audios().iter().enumerate() {
                    let row = self.audio_row(layout, index);
                    if !visible(row) {
                        continue;
                    }
                    let music = project.studio().music_audio_id == Some(audio.id);
                    scene.quads.push(quad(
                        row,
                        if self.hovered(row) {
                            PANEL_HOVER
                        } else {
                            PANEL_ALT
                        },
                        [0.0; 4],
                        8.0,
                    ));
                    scene.icons.push(SceneIcon {
                        name: if music { "comic/music" } else { "comic/audio" },
                        rect: Rect {
                            x: row.x + 10.0,
                            y: row.y + (row.height - 20.0) * 0.5,
                            width: 20.0,
                            height: 20.0,
                        },
                        tint: if music { MUSIC_COLOR } else { VOICE_COLOR },
                    });
                    let text = Rect {
                        x: row.x + 36.0,
                        width: row.width - 36.0 - 70.0,
                        ..row
                    };
                    label(
                        scene,
                        &ellipsize(&audio.file_name, 24),
                        Rect {
                            y: row.y + 4.0,
                            height: 20.0,
                            ..text
                        },
                        HAlign::Left,
                        12.0,
                        TEXT,
                    );
                    let uses = project
                        .pages()
                        .iter()
                        .flat_map(|page| &page.bubbles)
                        .filter(|bubble| {
                            bubble.audio_id == Some(audio.id)
                                || bubble.sound.sfx_audio_id == Some(audio.id)
                        })
                        .count();
                    let mut details = format_seconds(audio.duration_ms());
                    if uses > 0 {
                        details.push_str(&format!(" • {uses} bulle(s)"));
                    }
                    if music {
                        details.push_str(" • musique");
                    }
                    label(
                        scene,
                        &details,
                        Rect {
                            y: row.y + 24.0,
                            height: 18.0,
                            ..text
                        },
                        HAlign::Left,
                        10.0,
                        MUTED,
                    );
                    for (index, icon, name, id) in [
                        (
                            0,
                            "comic/play",
                            "Écouter",
                            format!("comic.audio.play.{}", audio.id),
                        ),
                        (
                            1,
                            "comic/trash",
                            "Retirer de la bibliothèque",
                            format!("comic.audio.delete.{}", audio.id),
                        ),
                    ] {
                        let button = audio_row_button(row, index);
                        self.icon_button(scene, button, icon, name, &id, false, index == 1);
                    }
                    push_control(
                        scene,
                        &format!("comic.audio.{}", audio.id),
                        &format!(
                            "{} ; glissez-le sur une bulle, Entrée l'attribue à la bulle sélectionnée",
                            audio.file_name
                        ),
                        Rect {
                            width: row.width - 70.0,
                            ..row
                        },
                        AccessibleRole::ListItem,
                        false,
                        None,
                    );
                }
                if self.pending_audio_imports > 0 {
                    let status = Rect {
                        y: layout.sidebar_import().y - 40.0,
                        height: 32.0,
                        ..layout.sidebar_import()
                    };
                    scene.quads.push(quad(status, ACCENT_SOFT, ACCENT, 8.0));
                    label(
                        scene,
                        &format!("Chargement de {} son(s)…", self.pending_audio_imports),
                        status,
                        HAlign::Center,
                        11.0,
                        TEXT,
                    );
                }
            }
        }
        let import = layout.sidebar_import();
        scene.quads.push(quad(
            import,
            if self.hovered(import) {
                PANEL_HOVER
            } else {
                PANEL_ALT
            },
            BORDER,
            8.0,
        ));
        let text = match self.sidebar_tab {
            SidebarTab::Pages => "Importer des planches…",
            SidebarTab::Sounds => "Importer des sons…",
        };
        icon_text(scene, import, Some("comic/upload"), text, 12.0, TEXT, ICON);
        push_control(
            scene,
            "comic.sidebar.import",
            text,
            import,
            AccessibleRole::Button,
            false,
            None,
        );
    }

    fn render_page_card(
        &self,
        scene: &mut ComicDubsScene,
        page: &Page,
        index: usize,
        card: Rect,
        active: bool,
    ) {
        scene.quads.push(quad(
            card,
            if active {
                ACCENT_SOFT
            } else if self.hovered(card) {
                PANEL_HOVER
            } else {
                PANEL_ALT
            },
            if active { ACCENT } else { [0.0; 4] },
            8.0,
        ));
        let thumb_h = card.height - 14.0;
        let thumb_w = (thumb_h * page.width as f32 / page.height.max(1) as f32).min(62.0);
        let thumb_h = thumb_w * page.height as f32 / page.width.max(1) as f32;
        let thumb = Rect {
            x: card.x + 7.0 + (62.0 - thumb_w) * 0.5,
            y: card.y + (card.height - thumb_h) * 0.5,
            width: thumb_w,
            height: thumb_h,
        };
        scene.quads.push(quad(
            Rect {
                x: thumb.x - 1.0,
                y: thumb.y - 1.0,
                width: thumb.width + 2.0,
                height: thumb.height + 2.0,
            },
            FIELD,
            BORDER,
            2.0,
        ));
        scene.thumbnails.push(PageLayer {
            page_id: page.id,
            rect: thumb,
            uv: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
        });
        let text = Rect {
            x: card.x + 76.0,
            width: card.width - 82.0,
            ..card
        };
        label(
            scene,
            &format!("Page {}", index + 1),
            Rect {
                y: card.y + 6.0,
                height: 22.0,
                width: if active {
                    text.width - 80.0
                } else {
                    text.width
                },
                ..text
            },
            HAlign::Left,
            13.0,
            TEXT,
        );
        label(
            scene,
            &ellipsize(&page.file_name, 22),
            Rect {
                y: card.y + 30.0,
                height: 16.0,
                ..text
            },
            HAlign::Left,
            10.0,
            MUTED,
        );
        let details = format!(
            "{} bulle{} · {} plan{}",
            page.bubbles.len(),
            if page.bubbles.len() > 1 { "s" } else { "" },
            page.shots.len(),
            if page.shots.len() > 1 { "s" } else { "" }
        );
        label(
            scene,
            &details,
            Rect {
                y: card.y + 48.0,
                height: 16.0,
                ..text
            },
            HAlign::Left,
            10.0,
            MUTED,
        );
        push_control(
            scene,
            &format!("comic.page.{}", page.id),
            &format!("Page {}, {}, {details}", index + 1, page.file_name),
            card,
            AccessibleRole::ListItem,
            active,
            None,
        );
        if active {
            for (button_index, icon, name, id) in [
                (
                    0,
                    "comic/arrow-up",
                    "Monter la page",
                    format!("comic.page.up.{}", page.id),
                ),
                (
                    1,
                    "comic/arrow-down",
                    "Descendre la page",
                    format!("comic.page.down.{}", page.id),
                ),
                (
                    2,
                    "comic/trash",
                    "Supprimer la page",
                    format!("comic.page.delete.{}", page.id),
                ),
            ] {
                let button = page_card_button(card, button_index);
                self.icon_button(scene, button, icon, name, &id, false, button_index == 2);
            }
        }
    }

    /// Square icon-only button with a tooltip.
    #[allow(clippy::too_many_arguments)]
    fn icon_button(
        &self,
        scene: &mut ComicDubsScene,
        rect: Rect,
        icon: &'static str,
        name: &str,
        id: &str,
        selected: bool,
        danger: bool,
    ) {
        let hovered = self.hovered(rect);
        scene.quads.push(quad(
            rect,
            if selected {
                ACCENT
            } else if hovered && danger {
                DANGER_SOFT
            } else if hovered {
                PANEL_HOVER
            } else {
                [0.0; 4]
            },
            if hovered && !selected {
                BORDER
            } else {
                [0.0; 4]
            },
            6.0,
        ));
        let size = (rect.width.min(rect.height) * 0.58).round();
        scene.icons.push(SceneIcon {
            name: icon,
            rect: Rect {
                x: rect.x + (rect.width - size) * 0.5,
                y: rect.y + (rect.height - size) * 0.5,
                width: size,
                height: size,
            },
            tint: if selected {
                [1.0; 4]
            } else if danger && hovered {
                [1.0, 0.55, 0.58, 1.0]
            } else {
                ICON
            },
        });
        push_control(
            scene,
            id,
            name,
            rect,
            AccessibleRole::Button,
            selected,
            Some(name),
        );
    }

    // -------------------------------------------------------------- header

    fn render_header(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        for (index, tool) in Tool::ALL.iter().enumerate() {
            let rect = layout.tool_button(index);
            if rect.x + rect.width > layout.zoom_button().x - 8.0 {
                break;
            }
            let active = self.tool == *tool;
            let hovered = self.hovered(rect);
            scene.quads.push(quad(
                rect,
                if active {
                    if *tool == Tool::Shot {
                        SHOT_COLOR
                    } else {
                        ACCENT
                    }
                } else if hovered {
                    PANEL_HOVER
                } else {
                    [0.0; 4]
                },
                if hovered && !active { BORDER } else { [0.0; 4] },
                8.0,
            ));
            scene.icons.push(SceneIcon {
                name: tool.icon(),
                rect: Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 8.0,
                    width: rect.width - 16.0,
                    height: rect.height - 16.0,
                },
                tint: if active {
                    [1.0; 4]
                } else if *tool == Tool::Shot {
                    [1.0, 0.7, 0.4, 1.0]
                } else {
                    ICON
                },
            });
            push_control(
                scene,
                &format!("comic.tool.{index}"),
                &format!("Outil {}", tool.label()),
                rect,
                AccessibleRole::Button,
                active,
                Some(tool.label()),
            );
            if index == 0 || index + 1 == Tool::SHOT_INDEX {
                scene.quads.push(quad(
                    Rect {
                        x: rect.x + rect.width + 8.0,
                        y: rect.y + 6.0,
                        width: 1.0,
                        height: rect.height - 12.0,
                    },
                    BORDER,
                    [0.0; 4],
                    0.0,
                ));
            }
        }
        let zoom = layout.zoom_button();
        let zoomed = self.canvas_view.is_some();
        scene.quads.push(quad(
            zoom,
            if self.hovered(zoom) {
                PANEL_HOVER
            } else {
                PANEL_ALT
            },
            if zoomed { ACCENT } else { BORDER },
            8.0,
        ));
        let zoom_text = format!("{:.0} %", self.zoom() * 100.0);
        icon_text(
            scene,
            zoom,
            Some("comic/page"),
            &zoom_text,
            11.0,
            if zoomed { TEXT } else { MUTED },
            if zoomed { ICON } else { ICON_MUTED },
        );
        push_control(
            scene,
            "comic.header.zoom",
            &format!("Zoom {zoom_text}, activer pour revenir à 100 %"),
            zoom,
            AccessibleRole::Button,
            zoomed,
            Some("Ctrl + molette : zoomer • molette ou clic molette : se déplacer • clic : revenir à 100 %"),
        );
        let toggle = layout.shots_toggle();
        let visible = !self.hide_shots;
        scene.quads.push(quad(
            toggle,
            if visible {
                SHOT_SOFT
            } else if self.hovered(toggle) {
                PANEL_HOVER
            } else {
                PANEL_ALT
            },
            if visible { SHOT_COLOR } else { BORDER },
            8.0,
        ));
        icon_text(
            scene,
            toggle,
            Some("comic/eye"),
            "Plans",
            12.0,
            if visible { SHOT_TEXT } else { MUTED },
            if visible { SHOT_COLOR } else { ICON_MUTED },
        );
        push_control(
            scene,
            "comic.header.shots",
            if visible {
                "Masquer les plans caméra"
            } else {
                "Afficher les plans caméra"
            },
            toggle,
            AccessibleRole::Checkbox,
            visible,
            Some(if visible {
                "Masquer les plans caméra sur la page"
            } else {
                "Afficher les plans caméra sur la page"
            }),
        );
        let position = project
            .active_page_id()
            .and_then(|id| project.pages().iter().position(|page| page.id == id));
        self.icon_button(
            scene,
            layout.previous_page(),
            "comic/chevron-left",
            "Page précédente",
            "comic.header.previous",
            false,
            false,
        );
        self.icon_button(
            scene,
            layout.next_page(),
            "comic/chevron-right",
            "Page suivante",
            "comic.header.next",
            false,
            false,
        );
        label(
            scene,
            &match position {
                Some(index) => format!("Page {} / {}", index + 1, project.pages().len()),
                None => "Aucune page".into(),
            },
            layout.page_label(),
            HAlign::Center,
            13.0,
            TEXT,
        );
    }

    // --------------------------------------------------------------- canvas

    fn render_editor_canvas(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        let Some(page) = project.active_page() else {
            let card = Rect {
                x: layout.canvas.x + layout.canvas.width * 0.5 - 220.0,
                y: layout.canvas.y + layout.canvas.height * 0.5 - 70.0,
                width: 440.0,
                height: 140.0,
            };
            scene.quads.push(quad(card, PANEL, BORDER, 12.0));
            scene.icons.push(SceneIcon {
                name: "comic/image",
                rect: Rect {
                    x: card.x + card.width * 0.5 - 18.0,
                    y: card.y + 18.0,
                    width: 36.0,
                    height: 36.0,
                },
                tint: ICON_MUTED,
            });
            label(
                scene,
                "Importez ou déposez les planches de votre BD",
                Rect {
                    y: card.y + 64.0,
                    height: 24.0,
                    ..card
                },
                HAlign::Center,
                15.0,
                TEXT,
            );
            label(
                scene,
                "Menu Imports, bouton « Importer des planches » ou glisser-déposer",
                Rect {
                    y: card.y + 92.0,
                    height: 20.0,
                    ..card
                },
                HAlign::Center,
                11.0,
                MUTED,
            );
            return;
        };
        let rect = self.page_rect(layout.canvas, page);
        scene.page_rect = Some(rect);
        scene.page_id = Some(page.id);
        if let Some(visible) = intersect(rect, layout.canvas) {
            scene.page_layers.push(PageLayer {
                page_id: page.id,
                rect: visible,
                uv: [
                    (visible.x - rect.x) / rect.width.max(1.0),
                    (visible.y - rect.y) / rect.height.max(1.0),
                    (visible.x + visible.width - rect.x) / rect.width.max(1.0),
                    (visible.y + visible.height - rect.y) / rect.height.max(1.0),
                ],
                tint: [1.0; 4],
            });
        }
        if self.canvas_view.is_none() {
            scene.quads.push(quad(
                Rect {
                    x: rect.x - 1.0,
                    y: rect.y - 1.0,
                    width: rect.width + 2.0,
                    height: rect.height + 2.0,
                },
                [0.03, 0.03, 0.04, 1.0],
                BORDER,
                2.0,
            ));
        }
        let placement = Placement {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        };
        let assignment = page.bubble_shots();
        for (index, bubble) in page.bubbles.iter().enumerate() {
            let points = self
                .bubble_vertex_drag
                .as_ref()
                .filter(|drag| drag.bubble_id == bubble.id)
                .map(|drag| drag.points.clone())
                .or_else(|| {
                    self.bubble_drag
                        .as_ref()
                        .filter(|drag| drag.bubble_id == bubble.id)
                        .map(translated_drag_points)
                })
                .unwrap_or_else(|| bubble.points.clone());
            let text = self
                .text_edit
                .as_ref()
                .filter(|edit| edit.bubble_id == bubble.id)
                .map(TextEdit::with_caret);
            let selected = self.selection == Selection::Bubble(bubble.id);
            self.draw_bubble(
                scene,
                project,
                page,
                bubble,
                &edit_frame(index),
                placement,
                DrawOptions {
                    clip: layout.canvas,
                    opacity: 1.0,
                    brightness: 1.0,
                    selected,
                    edit: true,
                    text: text.as_deref(),
                    points: Some(&points),
                },
            );
            let shot = (!self.hide_shots)
                .then(|| assignment[index])
                .flatten()
                .map(|shot| (shot, bubble_in_shot(page, index, Some(shot), self.aspect())));
            render_bubble_badge(
                scene,
                rect,
                layout.canvas,
                &points,
                index + 1,
                bubble.audio_id.is_some(),
                selected,
                shot,
            );
        }
        if !self.hide_shots || self.tool == Tool::Shot {
            self.render_shots(scene, page, rect, layout.canvas);
        }
        if let Some(drag) = self.shape_drag {
            if let Some((kind, _)) = drag.tool.shape() {
                if let Some(points) =
                    comic_dubs_shapes::shape_points(kind, drag.start, drag.current)
                {
                    for (a, b) in points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .take(points.len())
                    {
                        scene
                            .overlay_quads
                            .push(line_quad(rect, *a, *b, ACCENT, 2.0));
                    }
                }
            }
        }
        if !self.draft.is_empty() {
            for edge in self.draft.windows(2) {
                scene
                    .overlay_quads
                    .push(line_quad(rect, edge[0], edge[1], ACCENT, 2.0));
            }
            if self.draft.len() >= 3 {
                scene.overlay_quads.push(line_quad(
                    rect,
                    *self.draft.last().unwrap(),
                    self.draft[0],
                    [ACCENT[0], ACCENT[1], ACCENT[2], 0.45],
                    1.0,
                ));
            }
            for (index, point) in self.draft.iter().enumerate() {
                let center = screen_point(rect, *point);
                scene.overlay_quads.push(quad(
                    Rect {
                        x: center.0 - 5.0,
                        y: center.1 - 5.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    if index == 0 {
                        [0.3, 0.9, 0.6, 1.0]
                    } else {
                        ACCENT
                    },
                    [1.0; 4],
                    5.0,
                ));
            }
        }
    }

    /// One line of contextual help, shown in the timeline header.
    fn status_hint(&self) -> &'static str {
        if self.playing {
            "Lecture en cours • Espace ou le bouton Lire pour arrêter"
        } else if self.preview_ms.is_some() {
            "Aperçu du rendu final • cliquez sur la page ou Échap pour revenir à l'édition"
        } else if !self.draft.is_empty() {
            Tool::Polygon.hint()
        } else if self.text_edit.is_some() {
            "Écrivez le texte de la bulle • Entrée ou Échap pour terminer • les flèches déplacent le curseur"
        } else if self.selected_shot().is_some() && self.tool == Tool::Select {
            "Glissez le plan pour le déplacer, un coin pour le redimensionner • Suppr le retire"
        } else {
            self.tool.hint()
        }
    }

    fn render_shots(&self, scene: &mut ComicDubsScene, page: &Page, rect: Rect, clip: Rect) {
        let aspect = self.aspect();
        let selected = self.selected_shot();
        let dragged = self
            .shot_drag
            .and_then(|drag| drag.shot_id.map(|id| (id, drag.current)));
        let mut selected_screen = None;
        for (index, shot) in page.shots.iter().enumerate() {
            let region = match dragged {
                Some((id, current)) if id == shot.id => Some(current),
                _ => shot.region,
            };
            let screen = region_rect(rect, shot_view(page, region, aspect));
            let screen = if region.is_none() {
                Rect {
                    x: screen.x + 3.0,
                    y: screen.y + 3.0,
                    width: screen.width - 6.0,
                    height: screen.height - 6.0,
                }
            } else {
                screen
            };
            let is_selected = selected == Some(shot.id);
            if is_selected {
                selected_screen = Some((screen, region.is_some()));
            }
            let color = if is_selected {
                SHOT_COLOR
            } else {
                [SHOT_COLOR[0], SHOT_COLOR[1], SHOT_COLOR[2], 0.75]
            };
            outline_rect(
                scene,
                screen,
                color,
                if is_selected { 3.0 } else { 2.0 },
                clip,
                region.is_none(),
            );
            let tag = shot_tag(screen, index, region.is_none());
            if !contains_rect(clip, tag) {
                continue;
            }
            scene.overlay_quads.push(quad(
                tag,
                if is_selected {
                    SHOT_COLOR
                } else {
                    [0.2, 0.09, 0.01, 0.92]
                },
                SHOT_COLOR,
                5.0,
            ));
            scene.overlay_icons.push(SceneIcon {
                name: "comic/shot",
                rect: Rect {
                    x: tag.x + 5.0,
                    y: tag.y + 4.0,
                    width: 14.0,
                    height: 14.0,
                },
                tint: if is_selected {
                    [0.15, 0.06, 0.0, 1.0]
                } else {
                    SHOT_COLOR
                },
            });
            overlay_label_padded(
                scene,
                &shot_tag_text(index, region.is_none()),
                Rect {
                    x: tag.x + 21.0,
                    width: tag.width - 22.0,
                    ..tag
                },
                HAlign::Left,
                10.5,
                if is_selected { [40, 16, 0] } else { SHOT_TEXT },
                0.0,
            );
            push_control(
                scene,
                &format!("comic.canvas.shot.{}", shot.id),
                &shot_name(index, shot),
                tag,
                AccessibleRole::Button,
                is_selected,
                Some("Plan caméra : glissez pour le déplacer"),
            );
        }
        if let Some((screen, movable)) = selected_screen {
            // Darken what the shot leaves out.
            let shade = [0.0, 0.0, 0.0, 0.62];
            for part in [
                Rect {
                    height: (screen.y - rect.y).max(0.0),
                    ..rect
                },
                Rect {
                    y: screen.y + screen.height,
                    height: (rect.y + rect.height - screen.y - screen.height).max(0.0),
                    ..rect
                },
                Rect {
                    y: screen.y,
                    width: (screen.x - rect.x).max(0.0),
                    height: screen.height,
                    ..rect
                },
                Rect {
                    x: screen.x + screen.width,
                    y: screen.y,
                    width: (rect.x + rect.width - screen.x - screen.width).max(0.0),
                    height: screen.height,
                },
            ] {
                if let Some(part) = intersect(part, rect).and_then(|part| intersect(part, clip)) {
                    scene
                        .overlay_quads
                        .insert(0, quad(part, shade, [0.0; 4], 0.0));
                }
            }
            if movable {
                for (x, y) in shot_corners(screen) {
                    if !clip.contains(x, y) {
                        continue;
                    }
                    scene.overlay_quads.push(quad(
                        Rect {
                            x: x - SHOT_HANDLE * 0.5,
                            y: y - SHOT_HANDLE * 0.5,
                            width: SHOT_HANDLE,
                            height: SHOT_HANDLE,
                        },
                        [1.0; 4],
                        SHOT_COLOR,
                        2.0,
                    ));
                }
            }
        }
        if let Some(drag) = self
            .shot_drag
            .filter(|drag| drag.mode == ShotDragMode::Create)
        {
            let screen = region_rect(rect, drag.current);
            outline_rect(scene, screen, SHOT_COLOR, 2.0, clip, true);
            let tag = Rect {
                x: screen.x + 4.0,
                y: screen.y + screen.height - 4.0 - SHOT_TAG_H,
                width: 104.0,
                height: SHOT_TAG_H,
            };
            scene
                .overlay_quads
                .push(quad(tag, SHOT_COLOR, SHOT_COLOR, 5.0));
            overlay_label_padded(
                scene,
                "NOUVEAU PLAN",
                tag,
                HAlign::Center,
                10.5,
                [40, 16, 0],
                0.0,
            );
        }
    }

    // ------------------------------------------------------------ inspector

    fn render_inspector(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        plan: &Timeline,
        scene: &mut ComicDubsScene,
    ) {
        let header = layout.inspector_header();
        let (icon, tint, title, subtitle) = self.inspector_title(project, plan);
        scene.icons.push(SceneIcon {
            name: icon,
            rect: Rect {
                x: header.x + 16.0,
                y: header.y + 20.0,
                width: 24.0,
                height: 24.0,
            },
            tint,
        });
        label(
            scene,
            &title,
            Rect {
                x: header.x + 48.0,
                y: header.y + 12.0,
                width: header.width - 96.0,
                height: 22.0,
            },
            HAlign::Left,
            15.0,
            TEXT,
        );
        label(
            scene,
            &subtitle,
            Rect {
                x: header.x + 48.0,
                y: header.y + 34.0,
                width: header.width - 96.0,
                height: 18.0,
            },
            HAlign::Left,
            10.5,
            MUTED,
        );
        if self.inspector_has_tabs() {
            for (index, (tab, name)) in [
                (OverviewTab::Page, "Page"),
                (OverviewTab::Project, "Projet"),
            ]
            .into_iter()
            .enumerate()
            {
                let rect = layout.inspector_tab(index);
                let active = self.overview_tab == tab;
                scene.quads.push(quad(
                    rect,
                    if active {
                        ACCENT
                    } else if self.hovered(rect) {
                        PANEL_HOVER
                    } else {
                        PANEL_ALT
                    },
                    [0.0; 4],
                    7.0,
                ));
                label(
                    scene,
                    name,
                    rect,
                    HAlign::Center,
                    12.0,
                    if active { TEXT } else { MUTED },
                );
                push_control(
                    scene,
                    &format!("comic.overview.{index}"),
                    &format!("Réglages {}", name.to_lowercase()),
                    rect,
                    AccessibleRole::Tab,
                    active,
                    None,
                );
            }
        } else {
            self.icon_button(
                scene,
                inspector_close(layout),
                "comic/close",
                "Désélectionner",
                "comic.inspector.deselect",
                false,
                false,
            );
        }
        let body = layout.inspector_body(self.inspector_has_tabs());
        let (items, content_height) = self.inspector_content(project, layout);
        let open = self
            .dropdown
            .as_ref()
            .map(|dropdown| dropdown.item_id.as_str());
        for item in items.iter().filter(|item| {
            item.rect.y >= body.y - 1.0
                && item.rect.y + item.rect.height <= body.y + body.height + 1.0
        }) {
            self.render_item(scene, item, open == Some(item.id.as_str()));
        }
        if content_height > body.height + 1.0 {
            let ratio = body.height / content_height;
            let thumb_h = (body.height * ratio).max(28.0);
            let max_scroll = (content_height - body.height).max(1.0);
            let thumb_y = body.y + (body.height - thumb_h) * (self.inspector_scroll / max_scroll);
            scene.quads.push(quad(
                Rect {
                    x: layout.inspector.x + layout.inspector.width - 8.0,
                    y: thumb_y,
                    width: 4.0,
                    height: thumb_h,
                },
                [0.3, 0.31, 0.38, 0.9],
                [0.0; 4],
                2.0,
            ));
        }
    }

    fn inspector_title(
        &self,
        project: &ComicDubsProject,
        plan: &Timeline,
    ) -> (&'static str, [f32; 4], String, String) {
        match self.selection {
            Selection::Bubble(id) => {
                let Some((page_index, bubble_index)) = locate_bubble(project, id) else {
                    return ("comic/ellipse", ICON, "Bulle".into(), String::new());
                };
                let page = &project.pages()[page_index];
                let mut subtitle = format!("Page {}", page_index + 1);
                if let Some(shot) = page.bubble_shots()[bubble_index] {
                    subtitle.push_str(&format!(" · plan {}", shot + 1));
                }
                if let Some(cue) = plan.cue_for(page_index, bubble_index) {
                    subtitle.push_str(&format!(" · à {}", format_time_ms(cue.reveal_ms)));
                }
                (
                    "comic/ellipse",
                    ACCENT_LIGHT,
                    format!("Bulle {}", bubble_index + 1),
                    subtitle,
                )
            }
            Selection::Shot(id) => {
                let page_index = project.page_of_shot(id).and_then(|page| {
                    project
                        .pages()
                        .iter()
                        .position(|candidate| candidate.id == page)
                });
                let Some(page_index) = page_index else {
                    return ("comic/shot", SHOT_COLOR, "Plan".into(), String::new());
                };
                let page = &project.pages()[page_index];
                let index = page.shot_index(id).unwrap_or(0);
                let bubbles = page
                    .bubble_shots()
                    .iter()
                    .filter(|shot| **shot == Some(index))
                    .count();
                (
                    "comic/shot",
                    SHOT_COLOR,
                    format!("Plan {}", index + 1),
                    format!(
                        "Page {} · {}",
                        page_index + 1,
                        match bubbles {
                            0 => "aucune bulle".to_string(),
                            1 => "1 bulle".to_string(),
                            count => format!("{count} bulles"),
                        }
                    ),
                )
            }
            Selection::None => match project.active_page_id().and_then(|id| {
                project
                    .pages()
                    .iter()
                    .position(|page| page.id == id)
                    .map(|index| (index, &project.pages()[index]))
            }) {
                Some((index, page)) => (
                    "comic/page",
                    ICON,
                    format!("Page {} / {}", index + 1, project.pages().len()),
                    ellipsize(&page.file_name, 34),
                ),
                None => (
                    "comic/image",
                    ICON,
                    "Comic Dubs".into(),
                    "Aucune planche importée".into(),
                ),
            },
        }
    }

    fn render_item(&self, scene: &mut ComicDubsScene, item: &Item, open: bool) {
        let rect = item.rect;
        match &item.kind {
            ItemKind::Section {
                title,
                icon,
                collapsed,
            } => {
                if self.hovered(rect) {
                    scene
                        .quads
                        .push(quad(rect, [1.0, 1.0, 1.0, 0.04], [0.0; 4], 5.0));
                }
                scene.icons.push(SceneIcon {
                    name: icon,
                    rect: Rect {
                        x: rect.x,
                        y: rect.y + 6.0,
                        width: 15.0,
                        height: 15.0,
                    },
                    tint: ACCENT_LIGHT,
                });
                scene.icons.push(SceneIcon {
                    name: if *collapsed {
                        "comic/chevron-right"
                    } else {
                        "comic/chevron-down"
                    },
                    rect: Rect {
                        x: rect.x + rect.width - 18.0,
                        y: rect.y + 7.0,
                        width: 13.0,
                        height: 13.0,
                    },
                    tint: ICON_MUTED,
                });
                push_control(
                    scene,
                    &item.id,
                    &format!(
                        "Section {}, {}",
                        title.to_lowercase(),
                        if *collapsed { "repliée" } else { "dépliée" }
                    ),
                    rect,
                    AccessibleRole::Button,
                    !*collapsed,
                    None,
                );
                scene.labels.push(SceneLabel {
                    text: title.clone(),
                    bounds: Rect {
                        x: rect.x + 22.0,
                        width: rect.width - 44.0,
                        height: rect.height - 2.0,
                        ..rect
                    },
                    h_align: HAlign::Left,
                    font_size: 11.0,
                    color: [196, 190, 255],
                    font_family: None,
                    padding: 0.0,
                    letter_spacing: 0.8,
                    style: None,
                });
                scene.quads.push(quad(
                    Rect {
                        y: rect.y + rect.height - 1.0,
                        height: 1.0,
                        ..rect
                    },
                    BORDER_SOFT,
                    [0.0; 4],
                    0.0,
                ));
            }
            ItemKind::Info(text) => {
                for (index, line) in info_lines(text, rect.width).into_iter().enumerate() {
                    scene.labels.push(SceneLabel {
                        text: line,
                        bounds: Rect {
                            y: rect.y + index as f32 * INFO_LINE_H,
                            height: INFO_LINE_H,
                            ..rect
                        },
                        h_align: HAlign::Left,
                        font_size: 10.5,
                        color: MUTED,
                        font_family: None,
                        padding: 0.0,
                        letter_spacing: 0.0,
                        style: None,
                    });
                }
            }
            ItemKind::Warning(text) => {
                scene
                    .quads
                    .push(quad(rect, DANGER_SOFT, [0.75, 0.25, 0.3, 1.0], 7.0));
                scene.icons.push(SceneIcon {
                    name: "comic/eye",
                    rect: Rect {
                        x: rect.x + 9.0,
                        y: rect.y + 9.0,
                        width: 16.0,
                        height: 16.0,
                    },
                    tint: [1.0, 0.55, 0.58, 1.0],
                });
                for (index, line) in info_lines(text, rect.width - 34.0).into_iter().enumerate() {
                    label_padded(
                        scene,
                        &line,
                        Rect {
                            x: rect.x + 30.0,
                            y: rect.y + 8.0 + index as f32 * INFO_LINE_H,
                            width: rect.width - 36.0,
                            height: INFO_LINE_H,
                        },
                        HAlign::Left,
                        10.5,
                        [255, 200, 204],
                        0.0,
                    );
                }
                push_control(
                    scene,
                    &item.id,
                    text,
                    rect,
                    AccessibleRole::Region,
                    false,
                    None,
                );
            }
            ItemKind::Button {
                text,
                icon,
                tone,
                command,
            } => {
                let enabled = *command != Command::None || *tone == Tone::Selected;
                let hovered = enabled && self.hovered(rect);
                let (fill, border, text_color, tint) = match (tone, enabled) {
                    (_, false) => (FIELD, BORDER_SOFT, DIM, ICON_MUTED),
                    (Tone::Primary, _) => (
                        if hovered { ACCENT_HOVER } else { ACCENT },
                        [0.0; 4],
                        [255, 255, 255],
                        [1.0; 4],
                    ),
                    (Tone::Danger, _) => (
                        if hovered { DANGER } else { DANGER_SOFT },
                        DANGER,
                        [255, 205, 210],
                        [1.0, 0.7, 0.72, 1.0],
                    ),
                    (Tone::Selected, _) => (ACCENT_SOFT, ACCENT, TEXT, ICON),
                    (Tone::Recording, _) => (
                        RECORD_COLOR,
                        [1.0, 0.5, 0.55, 1.0],
                        [255, 255, 255],
                        [1.0; 4],
                    ),
                    (Tone::Normal, _) => (
                        if hovered { PANEL_HOVER } else { PANEL_ALT },
                        BORDER,
                        TEXT,
                        ICON,
                    ),
                };
                scene.quads.push(quad(rect, fill, border, 7.0));
                icon_text(scene, rect, *icon, text, 11.5, text_color, tint);
                push_control(
                    scene,
                    &item.id,
                    text,
                    rect,
                    AccessibleRole::Button,
                    *tone == Tone::Selected,
                    None,
                );
            }
            ItemKind::Segmented(segments) => {
                scene.quads.push(quad(rect, FIELD, BORDER, 7.0));
                for (index, segment) in segments.iter().enumerate() {
                    let part = segment_rect(rect, segments.len(), index);
                    let inner = Rect {
                        x: part.x + 2.0,
                        y: part.y + 2.0,
                        width: part.width - 4.0,
                        height: part.height - 4.0,
                    };
                    if segment.selected {
                        scene.quads.push(quad(inner, ACCENT, [0.0; 4], 5.0));
                    } else if self.hovered(part) {
                        scene.quads.push(quad(inner, PANEL_HOVER, [0.0; 4], 5.0));
                    }
                    if index > 0 && !segment.selected && !segments[index - 1].selected {
                        scene.quads.push(quad(
                            Rect {
                                x: part.x,
                                y: part.y + 7.0,
                                width: 1.0,
                                height: part.height - 14.0,
                            },
                            BORDER,
                            [0.0; 4],
                            0.0,
                        ));
                    }
                    match segment.icon {
                        Some(icon) => {
                            let size = 16.0;
                            scene.icons.push(SceneIcon {
                                name: icon,
                                rect: Rect {
                                    x: part.x + (part.width - size) * 0.5,
                                    y: part.y + (part.height - size) * 0.5,
                                    width: size,
                                    height: size,
                                },
                                tint: if segment.selected { [1.0; 4] } else { ICON },
                            });
                        }
                        None => label_padded(
                            scene,
                            &segment.label,
                            part,
                            HAlign::Center,
                            fitted_font(&segment.label, part.width - 6.0, 11.5),
                            if segment.selected {
                                [255, 255, 255]
                            } else {
                                TEXT
                            },
                            0.0,
                        ),
                    }
                    push_control(
                        scene,
                        &format!("{}.{index}", item.id),
                        &segment.name,
                        part,
                        AccessibleRole::Button,
                        segment.selected,
                        segment.icon.map(|_| segment.name.as_str()),
                    );
                }
            }
            ItemKind::Slider {
                name,
                display,
                spec,
            } => {
                let top = Rect {
                    height: 20.0,
                    ..rect
                };
                label_padded(scene, name, top, HAlign::Left, 11.0, MUTED, 0.0);
                label_padded(scene, display, top, HAlign::Right, 11.5, TEXT, 0.0);
                let track = slider_track(rect);
                let bar = Rect {
                    y: track.y + track.height * 0.5 - 2.5,
                    height: 5.0,
                    ..track
                };
                scene.quads.push(quad(bar, FIELD, BORDER, 2.5));
                let ratio = spec.ratio();
                scene.quads.push(quad(
                    Rect {
                        width: (bar.width * ratio).max(5.0),
                        ..bar
                    },
                    ACCENT,
                    [0.0; 4],
                    2.5,
                ));
                let dragging = self
                    .slider_drag
                    .is_some_and(|drag| drag.spec.kind == spec.kind);
                let thumb = if dragging || self.hovered(rect) {
                    16.0
                } else {
                    14.0
                };
                scene.quads.push(quad(
                    Rect {
                        x: bar.x + bar.width * ratio - thumb * 0.5,
                        y: bar.y + bar.height * 0.5 - thumb * 0.5,
                        width: thumb,
                        height: thumb,
                    },
                    [0.94, 0.94, 0.98, 1.0],
                    ACCENT,
                    thumb * 0.5,
                ));
                push_control(
                    scene,
                    &item.id,
                    &format!("{name} : {display} (flèches gauche et droite pour régler)"),
                    rect,
                    AccessibleRole::Slider,
                    false,
                    None,
                );
            }
            ItemKind::Dropdown { name, value, .. } => {
                label_padded(
                    scene,
                    name,
                    Rect {
                        height: 16.0,
                        ..rect
                    },
                    HAlign::Left,
                    11.0,
                    MUTED,
                    0.0,
                );
                let field = dropdown_field(rect);
                let hovered = self.hovered(field);
                scene.quads.push(quad(
                    field,
                    if hovered { PANEL_HOVER } else { PANEL_ALT },
                    if open { ACCENT } else { BORDER },
                    7.0,
                ));
                label(
                    scene,
                    &ellipsize(value, 34),
                    Rect {
                        x: field.x + 4.0,
                        width: field.width - 32.0,
                        ..field
                    },
                    HAlign::Left,
                    12.0,
                    TEXT,
                );
                scene.icons.push(SceneIcon {
                    name: if open {
                        "comic/chevron-up"
                    } else {
                        "comic/chevron-down"
                    },
                    rect: Rect {
                        x: field.x + field.width - 26.0,
                        y: field.y + (field.height - 16.0) * 0.5,
                        width: 16.0,
                        height: 16.0,
                    },
                    tint: ICON,
                });
                push_control(
                    scene,
                    &item.id,
                    &format!("{name} : {value}"),
                    field,
                    AccessibleRole::MenuButton,
                    open,
                    None,
                );
            }
            ItemKind::Toggle { text, on, .. } => {
                label_padded(
                    scene,
                    text,
                    Rect {
                        width: rect.width - 48.0,
                        ..rect
                    },
                    HAlign::Left,
                    11.5,
                    TEXT,
                    0.0,
                );
                let switch = Rect {
                    x: rect.x + rect.width - 38.0,
                    y: rect.y + (rect.height - 20.0) * 0.5,
                    width: 38.0,
                    height: 20.0,
                };
                scene.quads.push(quad(
                    switch,
                    if *on { ACCENT } else { FIELD },
                    if *on { [0.0; 4] } else { BORDER },
                    10.0,
                ));
                scene.quads.push(quad(
                    Rect {
                        x: if *on {
                            switch.x + switch.width - 18.0
                        } else {
                            switch.x + 2.0
                        },
                        y: switch.y + 2.0,
                        width: 16.0,
                        height: 16.0,
                    },
                    if *on {
                        [1.0; 4]
                    } else {
                        [0.6, 0.61, 0.68, 1.0]
                    },
                    [0.0; 4],
                    8.0,
                ));
                push_control(
                    scene,
                    &item.id,
                    &format!("{text}, {}", if *on { "activé" } else { "désactivé" }),
                    rect,
                    AccessibleRole::Checkbox,
                    *on,
                    None,
                );
            }
            ItemKind::Swatch { name, color, .. } => {
                label_padded(
                    scene,
                    name,
                    Rect {
                        height: 16.0,
                        ..rect
                    },
                    HAlign::Left,
                    11.0,
                    MUTED,
                    0.0,
                );
                let chip = Rect {
                    y: rect.y + 18.0,
                    height: rect.height - 18.0,
                    ..rect
                };
                match color {
                    Some(color) if color[3] != 0 => {
                        scene.quads.push(quad(chip, gpu_rgba(*color), BORDER, 7.0));
                    }
                    _ => {
                        scene.quads.push(quad(chip, FIELD, BORDER, 7.0));
                        label(
                            scene,
                            if color.is_some() {
                                "Transparent"
                            } else {
                                "Aucun"
                            },
                            chip,
                            HAlign::Center,
                            11.0,
                            MUTED,
                        );
                    }
                }
                if self.hovered(chip) {
                    scene
                        .quads
                        .push(quad(chip, [1.0, 1.0, 1.0, 0.06], [0.7, 0.7, 0.8, 1.0], 7.0));
                }
                push_control(
                    scene,
                    &item.id,
                    &format!(
                        "{name}, {}",
                        color.filter(|color| color[3] != 0).map_or(
                            "aucune couleur".to_string(),
                            |color| format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
                        )
                    ),
                    chip,
                    AccessibleRole::Button,
                    false,
                    None,
                );
            }
            ItemKind::TextBox { text, editing } => {
                scene.quads.push(quad(
                    rect,
                    FIELD,
                    if *editing {
                        ACCENT
                    } else if self.hovered(rect) {
                        [0.34, 0.35, 0.42, 1.0]
                    } else {
                        BORDER
                    },
                    7.0,
                ));
                let inner = Rect {
                    x: rect.x + 8.0,
                    y: rect.y + 8.0,
                    width: rect.width - 16.0,
                    height: rect.height - 16.0,
                };
                if text.trim().is_empty() {
                    label_padded(
                        scene,
                        "Cliquez ici (ou double-cliquez la bulle) pour écrire",
                        Rect {
                            height: 16.0,
                            ..inner
                        },
                        HAlign::Left,
                        11.5,
                        DIM,
                        0.0,
                    );
                } else {
                    let lines = info_lines(text, inner.width);
                    let max = ((inner.height / 16.0).floor() as usize).max(1);
                    for (index, line) in lines.iter().take(max).enumerate() {
                        let line = if index + 1 == max && lines.len() > max {
                            format!("{line}…")
                        } else {
                            line.clone()
                        };
                        label_padded(
                            scene,
                            &line,
                            Rect {
                                y: inner.y + index as f32 * 16.0,
                                height: 16.0,
                                ..inner
                            },
                            HAlign::Left,
                            12.0,
                            TEXT,
                            0.0,
                        );
                    }
                }
                push_control(
                    scene,
                    &item.id,
                    &if text.trim().is_empty() {
                        "Texte de la bulle, vide".to_string()
                    } else {
                        format!("Texte de la bulle : {text}")
                    },
                    rect,
                    AccessibleRole::TextField,
                    *editing,
                    None,
                );
            }
            ItemKind::ListRow {
                icon,
                text,
                detail,
                selected,
                ..
            } => {
                scene.quads.push(quad(
                    rect,
                    if *selected {
                        ACCENT_SOFT
                    } else if self.hovered(rect) {
                        PANEL_HOVER
                    } else {
                        PANEL_ALT
                    },
                    if *selected { ACCENT } else { [0.0; 4] },
                    7.0,
                ));
                scene.icons.push(SceneIcon {
                    name: icon,
                    rect: Rect {
                        x: rect.x + 10.0,
                        y: rect.y + (rect.height - 18.0) * 0.5,
                        width: 18.0,
                        height: 18.0,
                    },
                    tint: if *icon == "comic/shot" {
                        SHOT_COLOR
                    } else {
                        ACCENT_LIGHT
                    },
                });
                label_padded(
                    scene,
                    text,
                    Rect {
                        x: rect.x + 36.0,
                        width: rect.width * 0.45,
                        ..rect
                    },
                    HAlign::Left,
                    12.0,
                    TEXT,
                    0.0,
                );
                label_padded(
                    scene,
                    detail,
                    Rect {
                        x: rect.x + rect.width * 0.45,
                        width: rect.width * 0.55 - 30.0,
                        ..rect
                    },
                    HAlign::Right,
                    10.5,
                    MUTED,
                    0.0,
                );
                scene.icons.push(SceneIcon {
                    name: "comic/chevron-right",
                    rect: Rect {
                        x: rect.x + rect.width - 24.0,
                        y: rect.y + (rect.height - 14.0) * 0.5,
                        width: 14.0,
                        height: 14.0,
                    },
                    tint: ICON_MUTED,
                });
                push_control(
                    scene,
                    &item.id,
                    &format!("{text}, {detail}"),
                    rect,
                    AccessibleRole::ListItem,
                    *selected,
                    None,
                );
            }
        }
    }

    fn render_dropdown(&self, layout: ComicDubsLayout, scene: &mut ComicDubsScene) {
        let Some(dropdown) = self.dropdown.as_ref() else {
            return;
        };
        let panel = dropdown.panel(layout.content);
        scene.popup_quads.push(QuadInstance {
            shadow_offset: [0.0, 6.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            shadow_blur: 18.0,
            ..quad(panel, [0.09, 0.094, 0.115, 0.99], BORDER, 8.0)
        });
        for (visible, index) in (dropdown.scroll..dropdown.options.len())
            .take(dropdown.visible_rows())
            .enumerate()
        {
            let row = dropdown.row(layout.content, visible);
            let option = &dropdown.options[index];
            let selected = dropdown.selected == Some(index);
            if index == dropdown.highlighted {
                scene
                    .popup_quads
                    .push(quad(row, [0.2, 0.18, 0.4, 1.0], [0.0; 4], 5.0));
            }
            popup_label(
                scene,
                &ellipsize(&option.label, 40),
                Rect {
                    x: row.x + 4.0,
                    width: row.width - 30.0,
                    ..row
                },
                HAlign::Left,
                12.0,
                if selected { [255, 255, 255] } else { TEXT },
            );
            if selected {
                scene.popup_icons.push(SceneIcon {
                    name: "comic/check",
                    rect: Rect {
                        x: row.x + row.width - 22.0,
                        y: row.y + (row.height - 14.0) * 0.5,
                        width: 14.0,
                        height: 14.0,
                    },
                    tint: ACCENT_LIGHT,
                });
            }
            push_control(
                scene,
                &format!("comic.dropdown.{index}"),
                &option.label,
                row,
                AccessibleRole::MenuItem,
                selected,
                None,
            );
        }
        if dropdown.options.len() > dropdown.visible_rows() {
            let ratio = dropdown.visible_rows() as f32 / dropdown.options.len() as f32;
            let height = (panel.height - 8.0) * ratio;
            let offset = (panel.height - 8.0 - height) * dropdown.scroll as f32
                / (dropdown.options.len() - dropdown.visible_rows()) as f32;
            scene.popup_quads.push(quad(
                Rect {
                    x: panel.x + panel.width - 6.0,
                    y: panel.y + 4.0 + offset,
                    width: 3.0,
                    height,
                },
                [0.4, 0.41, 0.5, 1.0],
                [0.0; 4],
                1.5,
            ));
        }
    }

    // ------------------------------------------------------------- timeline

    fn render_timeline(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        plan: &Timeline,
        scene: &mut ComicDubsScene,
    ) {
        let strip = layout.timeline;
        if strip.height <= 0.0 || strip.width <= 0.0 {
            return;
        }
        let play = layout.timeline_play();
        self.icon_button(
            scene,
            play,
            if self.playing {
                "comic/pause"
            } else {
                "comic/play"
            },
            if self.playing {
                "Arrêter la lecture (Espace)"
            } else {
                "Lire (Espace)"
            },
            "comic.timeline.play",
            self.playing,
            false,
        );
        let header = layout.timeline_header();
        label(
            scene,
            &match self.preview_ms {
                Some(at_ms) => format!(
                    "{}  /  {}",
                    format_time_ms(at_ms),
                    format_time_ms(plan.total_ms)
                ),
                None => format!("Durée totale {}", format_time_ms(plan.total_ms)),
            },
            Rect {
                x: play.x + play.width + 8.0,
                width: 220.0,
                ..header
            },
            HAlign::Left,
            12.0,
            TEXT,
        );
        label(
            scene,
            self.status_hint(),
            Rect {
                x: header.x + 250.0,
                width: (header.width - 262.0).max(0.0),
                ..header
            },
            HAlign::Right,
            10.5,
            MUTED,
        );
        let rows = layout.timeline_rows();
        for (row, name, icon, tint) in [
            (rows.pages, "Pages", "comic/page", ICON_MUTED),
            (rows.shots, "Plans", "comic/shot", SHOT_COLOR),
            (rows.bubbles, "Bulles", "comic/ellipse", ACCENT_LIGHT),
            (rows.sounds, "Sons", "comic/wave", VOICE_COLOR),
            (rows.music, "Musique", "comic/music", MUSIC_COLOR),
        ] {
            let size = row.height.min(14.0);
            scene.icons.push(SceneIcon {
                name: icon,
                rect: Rect {
                    x: strip.x + 10.0,
                    y: row.y + (row.height - size) * 0.5,
                    width: size,
                    height: size,
                },
                tint,
            });
            label_padded(
                scene,
                name,
                Rect {
                    x: strip.x + 28.0,
                    width: TIMELINE_LABEL_W - 30.0,
                    ..row
                },
                HAlign::Left,
                10.5,
                MUTED,
                0.0,
            );
            scene.quads.push(quad(
                Rect {
                    x: row.x,
                    width: row.width,
                    ..row
                },
                [0.058, 0.06, 0.074, 1.0],
                [0.0; 4],
                4.0,
            ));
        }
        let track = layout.timeline_track();
        if plan.is_empty() {
            label(
                scene,
                "Tracez des bulles ou des plans pour construire la vidéo",
                Rect {
                    y: rows.shots.y,
                    height: rows.bubbles.y + rows.bubbles.height - rows.shots.y,
                    ..track
                },
                HAlign::Center,
                12.0,
                MUTED,
            );
            return;
        }
        let total = plan.total_ms.max(1);
        let x_at = |at_ms: u64| time_x(plan, track, at_ms);
        // Ruler.
        let step = [
            500, 1_000, 2_000, 5_000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000,
        ]
        .into_iter()
        .find(|step| (total / step) as f32 * 58.0 <= track.width)
        .unwrap_or(600_000);
        let mut tick = 0;
        while tick <= total {
            let x = x_at(tick);
            scene.quads.push(quad(
                Rect {
                    x,
                    y: rows.ruler.y + rows.ruler.height - 5.0,
                    width: 1.0,
                    height: 5.0,
                },
                BORDER,
                [0.0; 4],
                0.0,
            ));
            label_padded(
                scene,
                &format_short_time(tick),
                Rect {
                    x: x + 3.0,
                    y: rows.ruler.y,
                    width: 60.0,
                    height: rows.ruler.height - 2.0,
                },
                HAlign::Left,
                9.5,
                DIM,
                0.0,
            );
            tick += step;
        }
        let active_page = project.active_page_id();
        for span in &plan.pages {
            let Some(page) = project.pages().get(span.page_index) else {
                continue;
            };
            let block = Rect {
                x: x_at(span.start_ms) + 1.0,
                width: (x_at(span.end_ms) - x_at(span.start_ms) - 2.0).max(1.0),
                ..rows.pages
            };
            let active = active_page == Some(page.id);
            scene.quads.push(quad(
                block,
                if active { ACCENT_SOFT } else { PANEL_ALT },
                if active { ACCENT } else { BORDER_SOFT },
                4.0,
            ));
            if span.transition_ms > 0 {
                scene.quads.push(quad(
                    Rect {
                        width: (x_at(span.start_ms + span.transition_ms) - x_at(span.start_ms))
                            .max(2.0),
                        ..block
                    },
                    [0.55, 0.45, 1.0, 0.4],
                    [0.0; 4],
                    4.0,
                ));
            }
            if block.width > 40.0 {
                label_padded(
                    scene,
                    &format!("Page {}", span.page_index + 1),
                    Rect {
                        x: block.x + 6.0,
                        width: block.width - 8.0,
                        ..block
                    },
                    HAlign::Left,
                    10.0,
                    if active { TEXT } else { MUTED },
                    0.0,
                );
            }
            // Shots.
            for cue in &span.shots {
                let Some(shot) = page.shots.get(cue.shot_index) else {
                    continue;
                };
                let block = Rect {
                    x: x_at(cue.start_ms) + 1.0,
                    width: (x_at(cue.end_ms) - x_at(cue.start_ms) - 2.0).max(2.0),
                    ..rows.shots
                };
                let selected = self.selection == Selection::Shot(shot.id);
                scene.quads.push(quad(
                    block,
                    if selected {
                        [0.45, 0.2, 0.02, 1.0]
                    } else {
                        [0.2, 0.1, 0.02, 1.0]
                    },
                    if selected {
                        SHOT_COLOR
                    } else {
                        [0.5, 0.26, 0.06, 1.0]
                    },
                    4.0,
                ));
                if cue.arrive_ms > cue.start_ms {
                    scene.quads.push(quad(
                        Rect {
                            width: (x_at(cue.arrive_ms) - x_at(cue.start_ms)).max(2.0),
                            ..block
                        },
                        [1.0, 0.55, 0.12, 0.35],
                        [0.0; 4],
                        4.0,
                    ));
                }
                if block.width > 34.0 {
                    label_padded(
                        scene,
                        &shot_name(cue.shot_index, shot),
                        Rect {
                            x: block.x + 6.0,
                            width: block.width - 8.0,
                            ..block
                        },
                        HAlign::Left,
                        10.0,
                        SHOT_TEXT,
                        0.0,
                    );
                }
                push_control(
                    scene,
                    &format!("comic.timeline.shot.{}", shot.id),
                    &format!(
                        "Page {}, {} à {}",
                        span.page_index + 1,
                        shot_name(cue.shot_index, shot),
                        format_time_ms(cue.start_ms)
                    ),
                    block,
                    AccessibleRole::Button,
                    selected,
                    None,
                );
            }
            // Bubbles and their sounds.
            for cue in &span.cues {
                let Some(bubble) = page.bubbles.get(cue.bubble_index) else {
                    continue;
                };
                let block = Rect {
                    x: x_at(cue.start_ms) + 1.0,
                    width: (x_at(cue.end_ms) - x_at(cue.start_ms) - 2.0).max(2.0),
                    ..rows.bubbles
                };
                let selected = self.selection == Selection::Bubble(bubble.id);
                let inside = bubble_in_shot(
                    page,
                    cue.bubble_index,
                    page.bubble_shots()[cue.bubble_index],
                    self.aspect(),
                );
                scene.quads.push(quad(
                    block,
                    [0.085, 0.088, 0.11, 1.0],
                    if !inside {
                        [1.0, 0.35, 0.4, 1.0]
                    } else if selected {
                        ACCENT_LIGHT
                    } else {
                        BORDER_SOFT
                    },
                    4.0,
                ));
                scene.quads.push(quad(
                    Rect {
                        width: (x_at(cue.speak_end_ms) - block.x).clamp(1.0, block.width),
                        ..block
                    },
                    if bubble.text.trim().is_empty() {
                        [0.16, 0.16, 0.2, 1.0]
                    } else if selected {
                        [0.3, 0.26, 0.62, 1.0]
                    } else {
                        [0.2, 0.18, 0.4, 1.0]
                    },
                    [0.0; 4],
                    4.0,
                ));
                if block.width > 18.0 {
                    let text = if bubble.text.trim().is_empty() || block.width < 70.0 {
                        (cue.bubble_index + 1).to_string()
                    } else {
                        format!(
                            "{} · {}",
                            cue.bubble_index + 1,
                            ellipsize(&bubble.text, (block.width / 6.5) as usize)
                        )
                    };
                    label_padded(
                        scene,
                        &text,
                        Rect {
                            x: block.x + 5.0,
                            width: block.width - 7.0,
                            ..block
                        },
                        HAlign::Left,
                        10.0,
                        TEXT,
                        0.0,
                    );
                }
                push_control(
                    scene,
                    &format!(
                        "comic.timeline.cue.{}.{}",
                        span.page_index, cue.bubble_index
                    ),
                    &format!(
                        "Page {}, bulle {} à {} : {}",
                        span.page_index + 1,
                        cue.bubble_index + 1,
                        format_time_ms(cue.reveal_ms),
                        if bubble.text.trim().is_empty() {
                            "sans texte".to_string()
                        } else {
                            ellipsize(&bubble.text, 60)
                        }
                    ),
                    block,
                    AccessibleRole::Button,
                    selected,
                    None,
                );
                if cue.voice_ms > 0 {
                    scene.quads.push(quad(
                        Rect {
                            x: x_at(cue.voice_start_ms),
                            y: rows.sounds.y + 2.0,
                            width: (x_at(cue.voice_start_ms + cue.voice_ms)
                                - x_at(cue.voice_start_ms))
                            .max(2.0),
                            height: rows.sounds.height - 4.0,
                        },
                        [0.1, 0.36, 0.22, 1.0],
                        VOICE_COLOR,
                        3.0,
                    ));
                }
                if cue.sfx_audio.is_some() {
                    let size = rows.sounds.height.min(12.0);
                    scene.icons.push(SceneIcon {
                        name: "comic/sfx",
                        rect: Rect {
                            x: x_at(cue.reveal_ms) - size * 0.5,
                            y: rows.sounds.y + (rows.sounds.height - size) * 0.5,
                            width: size,
                            height: size,
                        },
                        tint: SFX_COLOR,
                    });
                }
            }
        }
        if let Some(music) = project
            .studio()
            .music_audio_id
            .and_then(|id| project.audio(id))
        {
            let length = if project.studio().music_loop {
                total
            } else {
                music.duration_ms().min(total)
            };
            let bar = Rect {
                width: (x_at(length) - track.x).max(1.0),
                ..rows.music
            };
            scene
                .quads
                .push(quad(bar, [0.1, 0.16, 0.32, 1.0], MUSIC_COLOR, 3.0));
            if project.studio().music_ducking {
                for (start, end) in plan.voice_intervals() {
                    scene.quads.push(quad(
                        Rect {
                            x: x_at(start),
                            width: (x_at(end) - x_at(start)).max(1.0),
                            y: bar.y + bar.height * 0.5,
                            height: bar.height * 0.5,
                        },
                        [0.05, 0.07, 0.13, 1.0],
                        [0.0; 4],
                        0.0,
                    ));
                }
            }
            if bar.width > 80.0 {
                label_padded(
                    scene,
                    &ellipsize(&music.file_name, 40),
                    Rect {
                        x: bar.x + 6.0,
                        width: bar.width - 8.0,
                        ..bar
                    },
                    HAlign::Left,
                    9.5,
                    [200, 214, 255],
                    0.0,
                );
            }
        }
        if let Some(at_ms) = self.preview_ms {
            let x = x_at(at_ms);
            scene.overlay_quads.push(quad(
                Rect {
                    x: x - 1.0,
                    y: rows.ruler.y,
                    width: 2.0,
                    height: rows.music.y + rows.music.height - rows.ruler.y,
                },
                PLAYHEAD_COLOR,
                [0.0; 4],
                1.0,
            ));
            scene.overlay_quads.push(quad(
                Rect {
                    x: x - 6.0,
                    y: rows.ruler.y - 2.0,
                    width: 12.0,
                    height: 10.0,
                },
                PLAYHEAD_COLOR,
                [0.0; 4],
                3.0,
            ));
        }
    }
}

struct DrawOptions<'a> {
    clip: Rect,
    opacity: f32,
    brightness: f32,
    selected: bool,
    /// Editor rendering: rest pose, handles and full text.
    edit: bool,
    text: Option<&'a str>,
    points: Option<&'a [Point]>,
}

const ACCENT_LIGHT: [f32; 4] = [0.62, 0.56, 1.0, 1.0];
const ACCENT_HOVER: [f32; 4] = [0.44, 0.37, 0.95, 1.0];
const INFO_LINE_H: f32 = 15.0;

/// Word-wraps inspector help text; UI labels never wrap on their own.
fn info_lines(text: &str, width: f32) -> Vec<String> {
    text_layout::wrap_text(text, ((width - 4.0) / 5.6).floor().max(8.0) as usize)
}

fn info_block(scene: &mut ComicDubsScene, text: &str, rect: Rect) {
    for (index, line) in info_lines(text, rect.width - 16.0).into_iter().enumerate() {
        label_padded(
            scene,
            &line,
            Rect {
                x: rect.x + 8.0,
                y: rect.y + index as f32 * INFO_LINE_H,
                width: rect.width - 16.0,
                height: INFO_LINE_H,
            },
            HAlign::Center,
            10.5,
            MUTED,
            0.0,
        );
    }
}

/// Font size fitting `text` in `width`, at most `size`.
fn fitted_font(text: &str, width: f32, size: f32) -> f32 {
    (width / (text.chars().count().max(1) as f32 * 0.58)).clamp(8.5, size)
}

/// Optional icon followed by text, centered as a group in `rect`.
fn icon_text(
    scene: &mut ComicDubsScene,
    rect: Rect,
    icon: Option<&'static str>,
    text: &str,
    font_size: f32,
    color: [u8; 3],
    tint: [f32; 4],
) {
    let icon_size = (font_size + 4.0).round();
    let gap = if icon.is_some() && !text.is_empty() {
        7.0
    } else {
        0.0
    };
    let available = rect.width - 16.0 - if icon.is_some() { icon_size + gap } else { 0.0 };
    let font_size = fitted_font(text, available, font_size);
    let text_width = (text.chars().count() as f32 * font_size * 0.56).min(available);
    let group = text_width + if icon.is_some() { icon_size + gap } else { 0.0 };
    let start = rect.x + (rect.width - group) * 0.5;
    if let Some(icon) = icon {
        scene.icons.push(SceneIcon {
            name: icon,
            rect: Rect {
                x: start,
                y: rect.y + (rect.height - icon_size) * 0.5,
                width: icon_size,
                height: icon_size,
            },
            tint,
        });
    }
    if !text.is_empty() {
        let text_x = start + if icon.is_some() { icon_size + gap } else { 0.0 };
        label_padded(
            scene,
            text,
            Rect {
                x: text_x - 2.0,
                width: (rect.x + rect.width - text_x).max(0.0),
                ..rect
            },
            HAlign::Left,
            font_size,
            color,
            0.0,
        );
    }
}

fn push_control(
    scene: &mut ComicDubsScene,
    id: &str,
    label: &str,
    bounds: Rect,
    role: AccessibleRole,
    selected: bool,
    tooltip: Option<&str>,
) {
    scene.controls.push(SceneControl {
        id: id.into(),
        label: label.into(),
        bounds,
        role,
        selected,
        tooltip: tooltip.map(str::to_owned),
    });
}

/// Outline of a rectangle, clipped; `dashed` for full-page and new shots.
fn outline_rect(
    scene: &mut ComicDubsScene,
    rect: Rect,
    color: [f32; 4],
    thickness: f32,
    clip: Rect,
    dashed: bool,
) {
    let corners = shot_corners(rect);
    for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)).take(4) {
        if !dashed {
            if let Some((a, b)) = clip_segment(*a, *b, clip) {
                scene
                    .overlay_quads
                    .push(screen_line_quad(a, b, color, thickness));
            }
            continue;
        }
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        let dashes = (length / 10.0).ceil().max(1.0) as usize;
        for dash in (0..dashes).step_by(2) {
            let t0 = dash as f32 / dashes as f32;
            let t1 = ((dash + 1) as f32 / dashes as f32).min(1.0);
            let from = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
            let to = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
            if let Some((from, to)) = clip_segment(from, to, clip) {
                scene
                    .overlay_quads
                    .push(screen_line_quad(from, to, color, thickness));
            }
        }
    }
}

/// Reading-order number next to a bubble, outside its shape, with a voice
/// marker and the shot that shows it.
#[allow(clippy::too_many_arguments)]
fn render_bubble_badge(
    scene: &mut ComicDubsScene,
    page_rect: Rect,
    clip: Rect,
    points: &[Point],
    order: usize,
    has_voice: bool,
    selected: bool,
    shot: Option<(usize, bool)>,
) {
    let bounds = polygon_bounds(page_rect, points);
    let size = 20.0;
    let x = (bounds.x - size * 0.6).clamp(
        page_rect.x + 2.0,
        page_rect.x + page_rect.width - size - 2.0,
    );
    let y = (bounds.y - size * 0.6).clamp(
        page_rect.y + 2.0,
        page_rect.y + page_rect.height - size - 2.0,
    );
    let circle = Rect {
        x,
        y,
        width: size,
        height: size,
    };
    if !contains_rect(clip, circle) {
        return;
    }
    scene.overlay_quads.push(quad(
        circle,
        if selected {
            ACCENT
        } else {
            [0.1, 0.1, 0.14, 0.94]
        },
        if selected { [1.0; 4] } else { ACCENT_LIGHT },
        size * 0.5,
    ));
    overlay_label_padded(
        scene,
        &order.to_string(),
        circle,
        HAlign::Center,
        11.0,
        TEXT,
        0.0,
    );
    let mut chip_x = circle.x + size + 3.0;
    if has_voice {
        let chip = Rect {
            x: chip_x,
            y: circle.y + 2.0,
            width: 16.0,
            height: 16.0,
        };
        scene
            .overlay_quads
            .push(quad(chip, [0.05, 0.2, 0.12, 0.94], VOICE_COLOR, 8.0));
        scene.overlay_icons.push(SceneIcon {
            name: "comic/mic",
            rect: Rect {
                x: chip.x + 3.0,
                y: chip.y + 3.0,
                width: 10.0,
                height: 10.0,
            },
            tint: VOICE_COLOR,
        });
        chip_x += 19.0;
    }
    if let Some((shot, inside)) = shot {
        let text = if inside {
            format!("P{}", shot + 1)
        } else {
            format!("P{} · hors du cadre", shot + 1)
        };
        let chip = Rect {
            x: chip_x,
            y: circle.y + 2.0,
            width: 12.0 + text.chars().count() as f32 * 5.6,
            height: 16.0,
        };
        let (fill, border, color) = if inside {
            ([0.2, 0.09, 0.01, 0.94], SHOT_COLOR, SHOT_TEXT)
        } else {
            (
                [0.35, 0.03, 0.06, 0.96],
                [1.0, 0.35, 0.4, 1.0],
                [255, 210, 214],
            )
        };
        scene.overlay_quads.push(quad(chip, fill, border, 8.0));
        overlay_label_padded(scene, &text, chip, HAlign::Center, 9.5, color, 0.0);
    }
}

fn contains_rect(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn quad(rect: Rect, color: [f32; 4], border: [f32; 4], radius: f32) -> QuadInstance {
    QuadInstance {
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
    }
}

fn make_label(
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
    padding: f32,
) -> SceneLabel {
    SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
        font_family: None,
        padding,
        letter_spacing: 0.0,
        style: None,
    }
}

fn label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene
        .labels
        .push(make_label(text, bounds, h_align, font_size, color, 6.0));
}

fn label_padded(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
    padding: f32,
) {
    scene
        .labels
        .push(make_label(text, bounds, h_align, font_size, color, padding));
}

fn overlay_label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene
        .overlay_labels
        .push(make_label(text, bounds, h_align, font_size, color, 6.0));
}

fn overlay_label_padded(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
    padding: f32,
) {
    scene
        .overlay_labels
        .push(make_label(text, bounds, h_align, font_size, color, padding));
}

fn popup_label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene
        .popup_labels
        .push(make_label(text, bounds, h_align, font_size, color, 4.0));
}

#[cfg(test)]
fn opaque_rgba(color: [u8; 4]) -> [f32; 4] {
    let mut color = gpu_rgba(color);
    color[3] = 1.0;
    color
}

fn label_info(label: &SceneLabel) -> LabelInfo<'_> {
    LabelInfo {
        text: &label.text,
        bounds: label.bounds,
        h_align: label.h_align,
        v_align: VAlign::Center,
        overflow: match label.style {
            Some(style) => Overflow::Styled(style),
            None if label.letter_spacing == 0.0 => Overflow::Clip,
            None => Overflow::ClipWithLetterSpacing(label.letter_spacing),
        },
        padding: label.padding,
        font_size_override: Some(label.font_size),
        color_override: Some(label.color),
        font_family_override: label.font_family.as_deref(),
    }
}

pub fn append_scene<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    scene: &'a ComicDubsScene,
) {
    quads.extend(scene.quads.iter().copied());
    labels.extend(scene.labels.iter().map(label_info));
}

pub fn append_overlay<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    scene: &'a ComicDubsScene,
) {
    quads.extend(scene.overlay_quads.iter().copied());
    labels.extend(scene.overlay_labels.iter().map(label_info));
}

pub fn append_popup<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    scene: &'a ComicDubsScene,
) {
    quads.extend(scene.top_quads.iter().copied());
    quads.extend(scene.popup_quads.iter().copied());
    labels.extend(scene.popup_labels.iter().map(label_info));
}

impl ComicDubsWorkspaceUi {
    fn render_preview(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        plan: &Timeline,
        at_ms: u64,
        scene: &mut ComicDubsScene,
    ) {
        let view = frame_rect(layout.canvas, self.frame_aspect);
        let [r, g, b] = project.studio().background;
        scene
            .quads
            .push(quad(view, gpu_rgba([r, g, b, 255]), BORDER, 0.0));
        let frame = timeline::evaluate(project, plan, at_ms, view.width / view.height.max(1.0));
        for layer in &frame.layers {
            let Some(page) = project.pages().get(layer.page_index) else {
                continue;
            };
            let placement = Placement::compute(
                (view.x, view.y, view.width, view.height),
                page.width,
                page.height,
                layer,
                frame.shake,
            );
            let page_rect = Rect {
                x: placement.x,
                y: placement.y,
                width: placement.width,
                height: placement.height,
            };
            // Each layer lives in its own (possibly sliding) video frame.
            let Some(layer_clip) = intersect(
                view,
                Rect {
                    x: view.x + layer.offset.0 * view.width,
                    y: view.y + layer.offset.1 * view.height,
                    ..view
                },
            ) else {
                continue;
            };
            if let Some(visible) = intersect(page_rect, layer_clip) {
                let brightness = layer.brightness.clamp(0.0, 1.0);
                scene.page_layers.push(PageLayer {
                    page_id: page.id,
                    rect: visible,
                    uv: [
                        (visible.x - page_rect.x) / page_rect.width.max(1.0),
                        (visible.y - page_rect.y) / page_rect.height.max(1.0),
                        (visible.x + visible.width - page_rect.x) / page_rect.width.max(1.0),
                        (visible.y + visible.height - page_rect.y) / page_rect.height.max(1.0),
                    ],
                    tint: [
                        brightness,
                        brightness,
                        brightness,
                        layer.opacity.clamp(0.0, 1.0),
                    ],
                });
                scene.page_rect = Some(page_rect);
                scene.page_id = Some(page.id);
            }
            self.draw_layer_bubbles(scene, project, page, layer, placement, layer_clip);
        }
        if frame.flash > 0.0 {
            scene.top_quads.push(quad(
                view,
                [1.0, 1.0, 1.0, frame.flash.min(1.0)],
                [0.0; 4],
                0.0,
            ));
        }
        let badge = Rect {
            x: view.x + 8.0,
            y: view.y + 8.0,
            width: 158.0,
            height: 26.0,
        };
        scene
            .overlay_quads
            .push(quad(badge, [0.06, 0.06, 0.09, 0.82], [0.0; 4], 13.0));
        scene.overlay_icons.push(SceneIcon {
            name: if self.playing {
                "comic/play"
            } else {
                "comic/eye"
            },
            rect: Rect {
                x: badge.x + 10.0,
                y: badge.y + 6.0,
                width: 14.0,
                height: 14.0,
            },
            tint: if self.playing { PLAYHEAD_COLOR } else { ICON },
        });
        overlay_label(
            scene,
            &format!(
                "{} {}",
                if self.playing { "Lecture" } else { "Aperçu" },
                format_time_ms(at_ms)
            ),
            Rect {
                x: badge.x + 26.0,
                width: badge.width - 28.0,
                ..badge
            },
            HAlign::Left,
            11.0,
            TEXT,
        );
    }

    fn draw_layer_bubbles(
        &self,
        scene: &mut ComicDubsScene,
        project: &ComicDubsProject,
        page: &Page,
        layer: &LayerFrame,
        placement: Placement,
        view: Rect,
    ) {
        for frame in &layer.bubbles {
            let Some(bubble) = page.bubbles.get(frame.bubble_index) else {
                continue;
            };
            self.draw_bubble(
                scene,
                project,
                page,
                bubble,
                frame,
                placement,
                DrawOptions {
                    clip: view,
                    opacity: layer.opacity,
                    brightness: layer.brightness,
                    selected: false,
                    edit: false,
                    text: None,
                    points: None,
                },
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bubble(
        &self,
        scene: &mut ComicDubsScene,
        project: &ComicDubsProject,
        page: &Page,
        bubble: &Bubble,
        frame: &BubbleFrame,
        placement: Placement,
        options: DrawOptions<'_>,
    ) {
        let rest = options
            .points
            .map(<[Point]>::to_vec)
            .unwrap_or_else(|| bubble.points_at(frame.pose_ms).to_vec());
        let animated = if options.edit {
            rest.clone()
        } else {
            timeline::animated_points(bubble, frame)
        };
        let screen = animated
            .iter()
            .map(|point| placement.map(*point))
            .collect::<Vec<_>>();
        let opacity = (frame.opacity * options.opacity).clamp(0.0, 1.0);
        let dim = options.brightness.clamp(0.0, 1.0);
        let scale = placement.font_scale() * frame.scale;
        if frame.show_background && opacity > 0.0 {
            if bubble.look.shadow {
                let shadow = screen
                    .iter()
                    .map(|(x, y)| (x + 6.0 * scale, y + 9.0 * scale))
                    .collect::<Vec<_>>();
                scene.overlay_quads.extend(fill_screen_polygon(
                    &shadow,
                    [0.0, 0.0, 0.0, 0.35 * opacity],
                    options.clip,
                ));
            }
            if bubble.color[3] != 0 {
                let mut color = gpu_rgba(bubble.color);
                for channel in color.iter_mut().take(3) {
                    *channel *= dim;
                }
                color[3] = opacity;
                scene
                    .overlay_quads
                    .extend(fill_screen_polygon(&screen, color, options.clip));
            }
            let (mut border, thickness) = if options.selected {
                (ACCENT, 2.5_f32.max(bubble.look.outline_width * scale))
            } else {
                match bubble.look.outline_color {
                    Some(color) => (gpu_rgba(color), bubble.look.outline_width * scale),
                    None => ([0.88, 0.88, 0.92, 0.72], bubble.look.outline_width.min(1.0)),
                }
            };
            if options.selected || bubble.look.outline_width > 0.0 {
                for channel in border.iter_mut().take(3) {
                    *channel *= dim;
                }
                border[3] *= opacity;
                for (a, b) in screen
                    .iter()
                    .zip(screen.iter().cycle().skip(1))
                    .take(screen.len())
                {
                    if let Some((a, b)) = clip_segment(*a, *b, options.clip) {
                        scene.overlay_quads.push(screen_line_quad(
                            a,
                            b,
                            border,
                            thickness.max(1.0),
                        ));
                    }
                }
            }
        }
        if options.selected && options.edit {
            let size = if screen.len() > 16 { 8.0 } else { 12.0 };
            for (x, y) in screen.iter().filter(|(x, y)| options.clip.contains(*x, *y)) {
                scene.overlay_quads.push(quad(
                    Rect {
                        x: x - size * 0.5,
                        y: y - size * 0.5,
                        width: size,
                        height: size,
                    },
                    ACCENT,
                    [1.0; 4],
                    size * 0.5,
                ));
            }
        }
        let text = options.text.unwrap_or(&bubble.text);
        if (frame.show_text || options.text.is_some()) && opacity > 0.0 {
            self.draw_bubble_text(
                scene, project, page, bubble, frame, &rest, text, placement, &options,
            );
        }
        let bounds = screen_bounds(&screen);
        if let Some(bounds) = intersect(bounds, options.clip) {
            scene.controls.push(SceneControl {
                id: format!("comic.canvas.bubble.{}", bubble.id),
                label: if bubble.text.trim().is_empty() {
                    "Bulle sans texte".into()
                } else {
                    bubble.text.clone()
                },
                bounds,
                role: AccessibleRole::Button,
                selected: options.selected,
                tooltip: None,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bubble_text(
        &self,
        scene: &mut ComicDubsScene,
        project: &ComicDubsProject,
        page: &Page,
        bubble: &Bubble,
        frame: &BubbleFrame,
        pose: &[Point],
        text: &str,
        placement: Placement,
        options: &DrawOptions<'_>,
    ) {
        let layout = self.text_layout(project, page, bubble, text, pose);
        let count = pose.len().max(1) as f32;
        let bubble_center = Point {
            x: pose.iter().map(|point| point.x).sum::<f32>() / count,
            y: pose.iter().map(|point| point.y).sum::<f32>() / count,
        };
        let text_center = layout.center();
        let transform = |point: Point| {
            let text = Point {
                x: text_center.x
                    + (point.x - text_center.x) * frame.text_scale
                    + frame.text_offset.0,
                y: text_center.y
                    + (point.y - text_center.y) * frame.text_scale
                    + frame.text_offset.1,
            };
            placement.map(Point {
                x: bubble_center.x + (text.x - bubble_center.x) * frame.scale + frame.offset.0,
                y: bubble_center.y + (text.y - bubble_center.y) * frame.scale + frame.offset.1,
            })
        };
        let pixel_scale = placement.font_scale() * frame.scale * frame.text_scale;
        let font_px = layout.font_px * pixel_scale;
        if font_px < 2.5 {
            return;
        }
        let opacity = (frame.opacity * frame.text_opacity * options.opacity).clamp(0.0, 1.0);
        let dim = options.brightness.clamp(0.0, 1.0);
        let shade = |color: [u8; 4]| {
            [
                (f32::from(color[0]) * dim) as u8,
                (f32::from(color[1]) * dim) as u8,
                (f32::from(color[2]) * dim) as u8,
            ]
        };
        let color = shade(effective_text_color(bubble));
        let alpha = (opacity * 255.0).round() as u8;
        let family = project.font_family();
        let spacing = layout.letter_spacing * pixel_scale;
        let line_height = layout.line_height * pixel_scale;
        let reveals = if options.edit {
            vec![LineReveal::Full; layout.lines.len()]
        } else {
            text_layout::line_reveals(&layout.lines, &bubble.text, frame.reveal)
        };
        let style = StyledText {
            letter_spacing: spacing,
            clip: options.clip,
            alpha,
            bold: bubble.bold,
            italic: bubble.look.italic,
        };
        for (index, (line, reveal)) in layout.lines.iter().zip(reveals).enumerate() {
            let visible = match reveal {
                LineReveal::Full => line.clone(),
                LineReveal::Partial(prefix) => prefix,
                LineReveal::Hidden => continue,
            };
            if visible.trim().is_empty() {
                continue;
            }
            let origin = transform(layout.line_origin(index, bubble.text_alignment));
            let width = layout.widths.get(index).copied().unwrap_or(0.0) * pixel_scale;
            let bounds = Rect {
                x: origin.0,
                y: origin.1,
                width: width + font_px * 2.0,
                height: line_height.max(font_px),
            };
            if intersect(bounds, options.clip).is_none() {
                continue;
            }
            if let Some(outline) = bubble.look.text_outline_color {
                let radius = (bubble.look.text_outline_width * pixel_scale).max(0.7);
                let outline_color = shade(outline);
                for step in 0..8 {
                    let angle = step as f32 * std::f32::consts::FRAC_PI_4;
                    scene.overlay_labels.push(SceneLabel {
                        text: visible.clone(),
                        bounds: Rect {
                            x: bounds.x + angle.cos() * radius,
                            y: bounds.y + angle.sin() * radius,
                            ..bounds
                        },
                        h_align: HAlign::Left,
                        font_size: font_px,
                        color: outline_color,
                        font_family: family.map(str::to_owned),
                        padding: 0.0,
                        letter_spacing: spacing,
                        style: Some(style),
                    });
                }
            }
            scene.overlay_labels.push(SceneLabel {
                text: visible.clone(),
                bounds,
                h_align: HAlign::Left,
                font_size: font_px,
                color,
                font_family: family.map(str::to_owned),
                padding: 0.0,
                letter_spacing: spacing,
                style: Some(style),
            });
            let decoration_width = if visible.len() == line.len() {
                width
            } else {
                text_layout::measure(&visible, font_px, spacing, family, bubble.bold)
            };
            for y in [
                bubble
                    .strikethrough
                    .then_some(bounds.y + bounds.height * 0.5),
                bubble
                    .underline
                    .then_some(bounds.y + bounds.height * 0.5 + font_px * 0.45),
            ]
            .into_iter()
            .flatten()
            {
                let line = Rect {
                    x: bounds.x,
                    y,
                    width: decoration_width,
                    height: (font_px * 0.07).max(1.0),
                };
                if let Some(line) = intersect(line, options.clip) {
                    let mut color = gpu_rgba([color[0], color[1], color[2], 255]);
                    color[3] = opacity;
                    scene.overlay_quads.push(quad(line, color, [0.0; 4], 0.0));
                }
            }
        }
    }

    fn text_layout(
        &self,
        project: &ComicDubsProject,
        page: &Page,
        bubble: &Bubble,
        text: &str,
        pose: &[Point],
    ) -> TextLayout {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        for point in pose {
            point.x.to_bits().hash(&mut hasher);
            point.y.to_bits().hash(&mut hasher);
        }
        (
            bubble.font_size.to_bits(),
            bubble.letter_spacing.to_bits(),
            bubble.line_spacing.to_bits(),
            bubble.bold,
            page.width,
            page.height,
            project.font_family(),
        )
            .hash(&mut hasher);
        let key = hasher.finish();
        if let Some(layout) = self.layout_cache.borrow().get(&key) {
            return layout.clone();
        }
        let layout = text_layout::layout(
            bubble,
            text,
            pose,
            page.width,
            page.height,
            project.font_family(),
        );
        let mut cache = self.layout_cache.borrow_mut();
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, layout.clone());
        layout
    }

    fn handle_vertex_editor_event(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> EventResponse {
        let Some(editor) = self.vertex_editor.as_ref() else {
            return EventResponse::Ignored;
        };
        let bubble_id = editor.bubble_id;
        let playhead_ms = editor.playhead_ms;
        let selected_keyframe = editor.selected_keyframe;
        let Some(bubble) = project.bubble(bubble_id) else {
            self.vertex_editor = None;
            return EventResponse::Consumed;
        };
        let duration_ms = vertex_editor_duration_ms(project, bubble);
        let editor_layout = VertexEditorLayout::compute(layout);
        let page_rect = project
            .active_page()
            .map(|page| image_rect(editor_layout.stage, page));

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b") {
            self.close_vertex_editor();
            return EventResponse::Consumed;
        }
        if let UiEvent::MousePress { x, y } = event {
            if editor_layout.close.contains(*x, *y) {
                self.close_vertex_editor();
                return EventResponse::Consumed;
            }
            if editor_layout.previous.contains(*x, *y) {
                self.set_vertex_editor_playhead(previous_keyframe_at(bubble, playhead_ms), project);
                return EventResponse::Consumed;
            }
            if editor_layout.next.contains(*x, *y) {
                self.set_vertex_editor_playhead(
                    next_keyframe_at(bubble, playhead_ms, duration_ms),
                    project,
                );
                return EventResponse::Consumed;
            }
            if editor_layout.play.contains(*x, *y) {
                self.toggle_vertex_editor_preview(project);
                return EventResponse::Consumed;
            }
            if editor_layout.add.contains(*x, *y) {
                self.vertex_editor.as_mut().unwrap().selected_keyframe = Some(playhead_ms);
                return EventResponse::Action(UiAction::ComicDubsSetBubbleVertexKeyframe {
                    bubble_id,
                    at_ms: playhead_ms,
                    points: bubble.points_at(playhead_ms).to_vec(),
                });
            }
            if editor_layout.delete.contains(*x, *y) {
                return selected_keyframe.map_or(EventResponse::Consumed, |at_ms| {
                    self.vertex_editor.as_mut().unwrap().selected_keyframe = None;
                    EventResponse::Action(UiAction::ComicDubsRemoveBubbleVertexKeyframe {
                        bubble_id,
                        at_ms,
                    })
                });
            }
            if editor_layout.track.contains(*x, *y) {
                let at_ms = (((*x - editor_layout.track.x) / editor_layout.track.width)
                    .clamp(0.0, 1.0)
                    * duration_ms as f32)
                    .round() as u64;
                let marker = bubble
                    .vertex_keyframes
                    .iter()
                    .min_by_key(|keyframe| keyframe.at_ms.abs_diff(at_ms))
                    .filter(|keyframe| {
                        keyframe.at_ms.abs_diff(at_ms) as f32 / duration_ms.max(1) as f32
                            * editor_layout.track.width
                            <= 10.0
                    })
                    .map(|keyframe| keyframe.at_ms);
                self.set_vertex_editor_playhead(marker.unwrap_or(at_ms), project);
                return EventResponse::Consumed;
            }
            if let Some(rect) = page_rect {
                let points = bubble.points_at(playhead_ms).to_vec();
                if let Some(index) = vertex_at(rect, &points, *x, *y) {
                    self.vertex_editor.as_mut().unwrap().playing = None;
                    self.bubble_vertex_drag = Some(BubbleVertexDrag {
                        bubble_id,
                        index,
                        keyframe_at_ms: Some(playhead_ms),
                        original: points.clone(),
                        points,
                    });
                }
            }
            return EventResponse::Consumed;
        }
        if let (UiEvent::MouseMove { x, y }, Some(rect), Some(drag)) =
            (event, page_rect, self.bubble_vertex_drag.as_mut())
        {
            drag.points[drag.index] = point_at(rect, *x, *y);
            return EventResponse::Consumed;
        }
        if matches!(event, UiEvent::MouseRelease { .. }) {
            if let Some(drag) = self.bubble_vertex_drag.take() {
                if drag.points != drag.original {
                    self.vertex_editor.as_mut().unwrap().selected_keyframe = drag.keyframe_at_ms;
                    return EventResponse::Action(UiAction::ComicDubsSetBubbleVertexKeyframe {
                        bubble_id: drag.bubble_id,
                        at_ms: drag.keyframe_at_ms.unwrap(),
                        points: drag.points,
                    });
                }
            }
            return EventResponse::Consumed;
        }
        if matches!(event, UiEvent::Delete)
            || matches!(event, UiEvent::KeyInput { text } if text == "\x7f")
        {
            if let Some(at_ms) = selected_keyframe {
                self.vertex_editor.as_mut().unwrap().selected_keyframe = None;
                return EventResponse::Action(UiAction::ComicDubsRemoveBubbleVertexKeyframe {
                    bubble_id,
                    at_ms,
                });
            }
            return EventResponse::Consumed;
        }
        match event {
            UiEvent::CursorLeft => {
                self.set_vertex_editor_playhead(playhead_ms.saturating_sub(50), project)
            }
            UiEvent::CursorRight => self.set_vertex_editor_playhead(
                playhead_ms.saturating_add(50).min(duration_ms),
                project,
            ),
            UiEvent::Home => self.set_vertex_editor_playhead(0, project),
            UiEvent::End => self.set_vertex_editor_playhead(duration_ms, project),
            UiEvent::PageUp => {
                self.set_vertex_editor_playhead(previous_keyframe_at(bubble, playhead_ms), project)
            }
            UiEvent::PageDown => self.set_vertex_editor_playhead(
                next_keyframe_at(bubble, playhead_ms, duration_ms),
                project,
            ),
            UiEvent::KeyInput { text } if text == "\r" || text == "\n" => {
                self.vertex_editor.as_mut().unwrap().selected_keyframe = Some(playhead_ms);
                return EventResponse::Action(UiAction::ComicDubsSetBubbleVertexKeyframe {
                    bubble_id,
                    at_ms: playhead_ms,
                    points: bubble.points_at(playhead_ms).to_vec(),
                });
            }
            _ => {}
        }
        EventResponse::Consumed
    }

    fn vertex_editor_scene(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> ComicDubsScene {
        let mut scene = ComicDubsScene::default();
        let Some(editor) = self.vertex_editor.as_ref() else {
            return scene;
        };
        let Some(bubble) = project.bubble(editor.bubble_id) else {
            return scene;
        };
        let Some(page) = project.active_page() else {
            return scene;
        };
        let editor_layout = VertexEditorLayout::compute(layout);
        let duration_ms = vertex_editor_duration_ms(project, bubble);
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene
            .quads
            .push(quad(editor_layout.header, PANEL, BORDER, 0.0));
        scene
            .quads
            .push(quad(editor_layout.timeline_panel, PANEL, BORDER, 0.0));
        label(
            &mut scene,
            "ANIMATION DES SOMMETS DE LA BULLE",
            Rect {
                x: editor_layout.header.x + 16.0,
                width: editor_layout.header.width - 72.0,
                ..editor_layout.header
            },
            HAlign::Left,
            14.0,
            TEXT,
        );
        label(
            &mut scene,
            "Poses sans interpolation • glissez un sommet • Entrée ajoute une pose • Espace lit • Ctrl+flèches : 50 ms • Échap ferme",
            Rect {
                x: editor_layout.header.x + 310.0,
                width: (editor_layout.header.width - 382.0).max(0.0),
                ..editor_layout.header
            },
            HAlign::Right,
            10.0,
            MUTED,
        );
        scene
            .quads
            .push(quad(editor_layout.close, PANEL_ALT, BORDER, 6.0));
        scene.icons.push(SceneIcon {
            name: "comic/close",
            rect: Rect {
                x: editor_layout.close.x + 8.0,
                y: editor_layout.close.y + 8.0,
                width: 16.0,
                height: 16.0,
            },
            tint: ICON,
        });
        push_control(
            &mut scene,
            "comic.vertex.close",
            "Fermer l’éditeur de poses",
            editor_layout.close,
            AccessibleRole::Button,
            false,
            Some("Fermer (Échap)"),
        );

        let rect = image_rect(editor_layout.stage, page);
        scene.page_rect = Some(rect);
        scene.page_id = Some(page.id);
        scene.page_layers.push(PageLayer {
            page_id: page.id,
            rect,
            uv: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0; 4],
        });
        scene
            .quads
            .push(quad(rect, [0.04, 0.04, 0.05, 1.0], BORDER, 2.0));
        let points = self
            .bubble_vertex_drag
            .as_ref()
            .filter(|drag| drag.bubble_id == bubble.id)
            .map(|drag| drag.points.clone())
            .unwrap_or_else(|| bubble.points_at(editor.playhead_ms).to_vec());
        if points != bubble.points {
            for (a, b) in bubble
                .points
                .iter()
                .zip(bubble.points.iter().cycle().skip(1))
                .take(bubble.points.len())
            {
                scene
                    .overlay_quads
                    .push(line_quad(rect, *a, *b, [0.55, 0.56, 0.64, 0.45], 1.0));
            }
        }
        let bubble_index = page
            .bubbles
            .iter()
            .position(|candidate| candidate.id == bubble.id)
            .unwrap_or(0);
        self.draw_bubble(
            &mut scene,
            project,
            page,
            bubble,
            &edit_frame(bubble_index),
            Placement {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            },
            DrawOptions {
                clip: editor_layout.stage,
                opacity: 1.0,
                brightness: 1.0,
                selected: true,
                edit: true,
                text: None,
                points: Some(&points),
            },
        );

        let playing = editor.playing.is_some();
        for (bounds, text, icon, id, name, selected) in [
            (
                editor_layout.previous,
                "",
                Some("comic/first"),
                "comic.vertex.previous",
                "Pose précédente",
                false,
            ),
            (
                editor_layout.play,
                if playing { "Pause" } else { "Lire" },
                Some(if playing { "comic/pause" } else { "comic/play" }),
                "comic.vertex.play",
                if playing { "Pause" } else { "Lire l'animation" },
                playing,
            ),
            (
                editor_layout.next,
                "",
                Some("comic/last"),
                "comic.vertex.next",
                "Pose suivante",
                false,
            ),
            (
                editor_layout.add,
                if editor.selected_keyframe == Some(editor.playhead_ms) {
                    "Mettre à jour"
                } else {
                    "Ajouter une pose"
                },
                Some("comic/plus"),
                "comic.vertex.add",
                "Ajouter une pose à cet instant",
                false,
            ),
            (
                editor_layout.delete,
                "Supprimer",
                Some("comic/trash"),
                "comic.vertex.delete",
                "Supprimer la pose sélectionnée",
                false,
            ),
        ] {
            let enabled = id != "comic.vertex.delete" || editor.selected_keyframe.is_some();
            scene.quads.push(quad(
                bounds,
                if selected {
                    ACCENT
                } else if enabled {
                    PANEL_ALT
                } else {
                    FIELD
                },
                BORDER,
                6.0,
            ));
            icon_text(
                &mut scene,
                bounds,
                icon,
                text,
                11.0,
                if enabled { TEXT } else { MUTED },
                if enabled { ICON } else { ICON_MUTED },
            );
            push_control(
                &mut scene,
                id,
                name,
                bounds,
                AccessibleRole::Button,
                selected,
                text.is_empty().then_some(name),
            );
        }

        scene
            .quads
            .push(quad(editor_layout.track, PANEL_ALT, BORDER, 6.0));
        for keyframe in &bubble.vertex_keyframes {
            let x = editor_layout.track.x
                + keyframe.at_ms.min(duration_ms) as f32 / duration_ms.max(1) as f32
                    * editor_layout.track.width;
            let marker = Rect {
                x: x - 6.0,
                y: editor_layout.track.y - 5.0,
                width: 12.0,
                height: 22.0,
            };
            let selected = editor.selected_keyframe == Some(keyframe.at_ms);
            scene.quads.push(quad(
                marker,
                if selected {
                    ACCENT
                } else {
                    [0.72, 0.63, 1.0, 1.0]
                },
                [1.0; 4],
                4.0,
            ));
            push_control(
                &mut scene,
                &format!("comic.vertex.marker.{}", keyframe.at_ms),
                &format!("Pose à {}", format_time_ms(keyframe.at_ms)),
                marker,
                AccessibleRole::Button,
                selected,
                None,
            );
        }
        let playhead_x = editor_layout.track.x
            + editor.playhead_ms.min(duration_ms) as f32 / duration_ms.max(1) as f32
                * editor_layout.track.width;
        scene.quads.push(quad(
            Rect {
                x: playhead_x - 1.5,
                y: editor_layout.track.y - 10.0,
                width: 3.0,
                height: 32.0,
            },
            PLAYHEAD_COLOR,
            [0.0; 4],
            1.5,
        ));
        label(
            &mut scene,
            "0:00.000",
            Rect {
                x: editor_layout.track.x,
                y: editor_layout.track.y + 20.0,
                width: 90.0,
                height: 20.0,
            },
            HAlign::Left,
            10.0,
            MUTED,
        );
        label(
            &mut scene,
            &format_time_ms(editor.playhead_ms),
            Rect {
                x: playhead_x - 55.0,
                y: editor_layout.track.y - 30.0,
                width: 110.0,
                height: 20.0,
            },
            HAlign::Center,
            10.0,
            TEXT,
        );
        label(
            &mut scene,
            &format_time_ms(duration_ms),
            Rect {
                x: editor_layout.track.x + editor_layout.track.width - 90.0,
                y: editor_layout.track.y + 20.0,
                width: 90.0,
                height: 20.0,
            },
            HAlign::Right,
            10.0,
            MUTED,
        );
        scene
    }
}

fn edit_frame(bubble_index: usize) -> BubbleFrame {
    BubbleFrame {
        bubble_index,
        pose_ms: 0,
        show_background: true,
        show_text: true,
        opacity: 1.0,
        scale: 1.0,
        offset: (0.0, 0.0),
        text_opacity: 1.0,
        text_scale: 1.0,
        text_offset: (0.0, 0.0),
        reveal: None,
        speaking: false,
    }
}

fn color_action(
    target: ColorTarget,
    color: [u8; 4],
    project: &ComicDubsProject,
) -> Option<UiAction> {
    Some(match target {
        ColorTarget::Bubble(bubble_id) => UiAction::ComicDubsSetBubbleColor { bubble_id, color },
        ColorTarget::Text(bubble_id) => UiAction::ComicDubsSetBubbleTextColor { bubble_id, color },
        ColorTarget::Outline(bubble_id) => {
            let bubble = project.bubble(bubble_id)?;
            UiAction::ComicDubsSetBubbleLook {
                bubble_id,
                look: BubbleLook {
                    outline_color: Some(color),
                    outline_width: if bubble.look.outline_width <= 0.0 {
                        2.0
                    } else {
                        bubble.look.outline_width
                    },
                    ..bubble.look
                },
            }
        }
        ColorTarget::TextOutline(bubble_id) => {
            let bubble = project.bubble(bubble_id)?;
            UiAction::ComicDubsSetBubbleLook {
                bubble_id,
                look: BubbleLook {
                    text_outline_color: Some(color),
                    ..bubble.look
                },
            }
        }
        ColorTarget::Background => UiAction::ComicDubsSetStudio(StudioSettings {
            background: [color[0], color[1], color[2]],
            ..*project.studio()
        }),
    })
}

fn current_color(target: ColorTarget, project: &ComicDubsProject) -> Option<[u8; 4]> {
    Some(match target {
        ColorTarget::Bubble(id) => project.bubble(id)?.color,
        ColorTarget::Text(id) => effective_text_color(project.bubble(id)?),
        ColorTarget::Outline(id) => project
            .bubble(id)?
            .look
            .outline_color
            .unwrap_or([225, 225, 235, 255]),
        ColorTarget::TextOutline(id) => project
            .bubble(id)?
            .look
            .text_outline_color
            .unwrap_or([20, 20, 24, 255]),
        ColorTarget::Background => {
            let [r, g, b] = project.studio().background;
            [r, g, b, 255]
        }
    })
}

fn locate_bubble(project: &ComicDubsProject, id: BubbleId) -> Option<(usize, usize)> {
    project
        .pages()
        .iter()
        .enumerate()
        .find_map(|(page, content)| {
            content
                .bubbles
                .iter()
                .position(|bubble| bubble.id == id)
                .map(|bubble| (page, bubble))
        })
}

fn ellipsize(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(limit).collect::<String>())
    }
}

fn effective_text_color(bubble: &Bubble) -> [u8; 4] {
    bubble.text_color.unwrap_or_else(|| {
        if luminance(rgba(bubble.color)) > 0.55 {
            [24, 24, 30, 255]
        } else {
            [244, 244, 248, 255]
        }
    })
}

fn vertex_editor_duration_ms(project: &ComicDubsProject, bubble: &Bubble) -> u64 {
    bubble
        .audio_id
        .and_then(|id| project.audio(id))
        .map_or(0, |audio| audio.duration_ms())
        .max(bubble.vertex_animation_duration_ms())
        .clamp(2_000, 86_400_000)
}

fn previous_keyframe_at(bubble: &Bubble, playhead_ms: u64) -> u64 {
    bubble
        .vertex_keyframes
        .iter()
        .rev()
        .find(|keyframe| keyframe.at_ms < playhead_ms)
        .map_or(0, |keyframe| keyframe.at_ms)
}

fn next_keyframe_at(bubble: &Bubble, playhead_ms: u64, duration_ms: u64) -> u64 {
    bubble
        .vertex_keyframes
        .iter()
        .find(|keyframe| keyframe.at_ms > playhead_ms)
        .map_or(duration_ms, |keyframe| keyframe.at_ms)
}

fn format_time_ms(at_ms: u64) -> String {
    format!(
        "{}:{:02}.{:03}",
        at_ms / 60_000,
        at_ms / 1_000 % 60,
        at_ms % 1_000
    )
}

fn format_short_time(at_ms: u64) -> String {
    format!("{}:{:02}", at_ms / 60_000, at_ms / 1_000 % 60)
}

fn format_seconds(duration_ms: u64) -> String {
    format!("{:.1} s", duration_ms as f64 / 1_000.0).replace('.', ",")
}

fn frame_rect(canvas: Rect, aspect: Option<f32>) -> Rect {
    let Some(aspect) = aspect else {
        return canvas;
    };
    let (width, height) = if canvas.width / canvas.height.max(1.0) > aspect {
        (canvas.height * aspect, canvas.height)
    } else {
        (canvas.width, canvas.width / aspect)
    };
    Rect {
        x: canvas.x + (canvas.width - width) * 0.5,
        y: canvas.y + (canvas.height - height) * 0.5,
        width,
        height,
    }
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    (x1 > x0 && y1 > y0).then_some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

fn screen_bounds(points: &[(f32, f32)]) -> Rect {
    let min_x = points.iter().map(|point| point.0).fold(f32::MAX, f32::min);
    let max_x = points.iter().map(|point| point.0).fold(f32::MIN, f32::max);
    let min_y = points.iter().map(|point| point.1).fold(f32::MAX, f32::min);
    let max_y = points.iter().map(|point| point.1).fold(f32::MIN, f32::max);
    Rect {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    }
}

fn clip_segment(a: (f32, f32), b: (f32, f32), clip: Rect) -> Option<((f32, f32), (f32, f32))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for (p, q) in [
        (-dx, a.0 - clip.x),
        (dx, clip.x + clip.width - a.0),
        (-dy, a.1 - clip.y),
        (dy, clip.y + clip.height - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            t0 = t0.max(r);
        } else {
            t1 = t1.min(r);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((
        (a.0 + dx * t0, a.1 + dy * t0),
        (a.0 + dx * t1, a.1 + dy * t1),
    ))
}

fn image_rect(canvas: Rect, page: &Page) -> Rect {
    let scale = (canvas.width / page.width as f32)
        .min(canvas.height / page.height as f32)
        .max(0.0);
    let width = page.width as f32 * scale;
    let height = page.height as f32 * scale;
    Rect {
        x: canvas.x + (canvas.width - width) * 0.5,
        y: canvas.y + (canvas.height - height) * 0.5,
        width,
        height,
    }
}

fn point_at(rect: Rect, x: f32, y: f32) -> Point {
    Point {
        x: ((x - rect.x) / rect.width.max(1.0)).clamp(0.0, 1.0),
        y: ((y - rect.y) / rect.height.max(1.0)).clamp(0.0, 1.0),
    }
}

fn screen_point(rect: Rect, point: Point) -> (f32, f32) {
    (
        rect.x + point.x * rect.width,
        rect.y + point.y * rect.height,
    )
}

fn near_vertex(rect: Rect, point: Point, x: f32, y: f32) -> bool {
    let point = screen_point(rect, point);
    (point.0 - x).hypot(point.1 - y) <= 12.0
}

fn vertex_at(rect: Rect, points: &[Point], x: f32, y: f32) -> Option<usize> {
    points
        .iter()
        .position(|point| near_vertex(rect, *point, x, y))
}

fn edge_at(rect: Rect, points: &[Point], x: f32, y: f32) -> Option<(usize, Point)> {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .enumerate()
        .filter_map(|(index, (a, b))| {
            let (ax, ay) = screen_point(rect, *a);
            let (bx, by) = screen_point(rect, *b);
            let (dx, dy) = (bx - ax, by - ay);
            let length = dx * dx + dy * dy;
            if length < 1.0 {
                return None;
            }
            let t = (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0);
            let (px, py) = (ax + dx * t, ay + dy * t);
            let distance = (px - x).hypot(py - y);
            (distance <= 8.0).then(|| (distance, index, point_at(rect, px, py)))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, index, point)| (index, point))
}

fn bubble_at(page: &Page, rect: Rect, x: f32, y: f32) -> Option<BubbleId> {
    if !rect.contains(x, y) {
        return None;
    }
    let point = point_at(rect, x, y);
    page.bubbles
        .iter()
        .rev()
        .find(|bubble| point_in_polygon(point, &bubble.points))
        .map(|bubble| bubble.id)
}

pub(crate) fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    text_layout::point_in_polygon(point, polygon)
}

fn polygon_bounds(rect: Rect, points: &[Point]) -> Rect {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    Rect {
        x: rect.x + min_x * rect.width,
        y: rect.y + min_y * rect.height,
        width: (max_x - min_x) * rect.width,
        height: (max_y - min_y) * rect.height,
    }
}

fn translated_drag_points(drag: &BubbleDrag) -> Vec<Point> {
    drag.original
        .iter()
        .map(|point| Point {
            x: point.x + drag.delta.x,
            y: point.y + drag.delta.y,
        })
        .collect()
}

/// Scanline fill of a screen-space polygon, clipped to `clip`.
fn fill_screen_polygon(points: &[(f32, f32)], color: [f32; 4], clip: Rect) -> Vec<QuadInstance> {
    let mut quads = Vec::new();
    if points.len() < 3 || color[3] <= 0.0 {
        return quads;
    }
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::MAX, f32::min)
        .max(clip.y);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f32::MIN, f32::max)
        .min(clip.y + clip.height);
    // ponytail: 3 px scanlines keep concave fills dependency-free; triangulate only if
    // very large pages or hundreds of simultaneous bubbles become a measured bottleneck.
    // Rows start on whole pixels and overlap by one pixel so the anti-aliased
    // quad edges never leave visible seams.
    let step = 3.0;
    let mut y = min_y.floor();
    let mut intersections = Vec::new();
    while y < max_y {
        let sample_y = (y + step * 0.5).min(max_y);
        intersections.clear();
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            if (a.1 > sample_y) != (b.1 > sample_y) {
                intersections.push(a.0 + (sample_y - a.1) * (b.0 - a.0) / (b.1 - a.1));
            }
        }
        intersections.sort_by(f32::total_cmp);
        for span in intersections.chunks_exact(2) {
            let start = span[0].max(clip.x);
            let end = span[1].min(clip.x + clip.width);
            if end <= start {
                continue;
            }
            let top = y.max(min_y);
            quads.push(quad(
                Rect {
                    x: start,
                    y: top,
                    width: end - start,
                    height: (max_y - top).min(step + 1.0),
                },
                color,
                [0.0; 4],
                0.0,
            ));
        }
        y += step;
    }
    quads
}

fn line_quad(rect: Rect, a: Point, b: Point, color: [f32; 4], thickness: f32) -> QuadInstance {
    screen_line_quad(
        screen_point(rect, a),
        screen_point(rect, b),
        color,
        thickness,
    )
}

fn screen_line_quad(a: (f32, f32), b: (f32, f32), color: [f32; 4], thickness: f32) -> QuadInstance {
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    QuadInstance {
        rect: [
            (a.0 + b.0 - length) * 0.5,
            (a.1 + b.1 - thickness) * 0.5,
            length,
            thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: thickness * 0.5,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: (b.1 - a.1).atan2(b.0 - a.0),
        _padding: [0.0; 2],
    }
}

fn rgba(color: [u8; 4]) -> [f32; 4] {
    color.map(|channel| channel as f32 / 255.0)
}

fn rgba8(color: [f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0).round() as u8,
        (color[1] * 255.0).round() as u8,
        (color[2] * 255.0).round() as u8,
        (color[3] * 255.0).round() as u8,
    ]
}

fn gpu_rgba(color: [u8; 4]) -> [f32; 4] {
    crate::ui::color_picker::srgb_to_linear(rgba(color))
}

fn luminance(color: [f32; 4]) -> f32 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{RecordedAudio, WaveformData};

    fn project() -> ComicDubsProject {
        let mut project = ComicDubsProject::default();
        project.add_page("page.jpg".into(), "page.png".into(), 1_000, 1_000);
        project
    }

    fn layout() -> ComicDubsLayout {
        ComicDubsLayout::compute(Rect {
            x: 0.0,
            y: 0.0,
            width: 1_440.0,
            height: 900.0,
        })
    }

    fn square(x: f32, y: f32, size: f32) -> Vec<Point> {
        vec![
            Point { x, y },
            Point { x: x + size, y },
            Point {
                x: x + size,
                y: y + size,
            },
            Point { x, y: y + size },
        ]
    }

    fn recorded() -> RecordedAudio {
        RecordedAudio {
            file_name: "line.flac".into(),
            sample_rate: 48_000,
            channels: 1,
            sample_count: 48_000,
            checksum: "a".repeat(40),
            waveform: WaveformData::default(),
        }
    }

    fn click(
        ui: &mut ComicDubsWorkspaceUi,
        project: &ComicDubsProject,
        x: f32,
        y: f32,
    ) -> EventResponse {
        ui.handle_event(&UiEvent::MousePress { x, y }, project, layout())
    }

    fn event(
        ui: &mut ComicDubsWorkspaceUi,
        project: &ComicDubsProject,
        event: UiEvent,
    ) -> EventResponse {
        ui.handle_event(&event, project, layout())
    }

    fn drag(
        ui: &mut ComicDubsWorkspaceUi,
        project: &ComicDubsProject,
        from: (f32, f32),
        to: (f32, f32),
    ) -> EventResponse {
        click(ui, project, from.0, from.1);
        event(ui, project, UiEvent::MouseMove { x: to.0, y: to.1 });
        event(ui, project, UiEvent::MouseRelease { x: to.0, y: to.1 })
    }

    /// Scrolls the inspector until the item is visible.
    fn item(ui: &mut ComicDubsWorkspaceUi, project: &ComicDubsProject, id: &str) -> Item {
        for _ in 0..60 {
            if let Some(item) = ui
                .visible_inspector_items(project, layout())
                .into_iter()
                .find(|item| item.id == id)
            {
                return item;
            }
            let inspector = layout().inspector;
            event(
                ui,
                project,
                UiEvent::Scroll {
                    x: inspector.x + 20.0,
                    y: inspector.y + 200.0,
                    delta: -1.0,
                    fast: false,
                    ctrl: false,
                },
            );
        }
        panic!("missing inspector item {id}")
    }

    fn page_rect(project: &ComicDubsProject) -> Rect {
        image_rect(layout().canvas, project.active_page().unwrap())
    }

    fn at(rect: Rect, x: f32, y: f32) -> (f32, f32) {
        (rect.x + rect.width * x, rect.y + rect.height * y)
    }

    #[test]
    fn studio_regions_never_overlap() {
        let layout = layout();
        assert_eq!(layout.toolbar.y, layout.header.y + layout.header.height);
        assert_eq!(layout.toolbar.height, 42.0);
        assert!(layout.canvas.y >= layout.toolbar.y + layout.toolbar.height);
        assert!(layout.canvas.y + layout.canvas.height <= layout.timeline.y);
        assert!(layout.sidebar.x + layout.sidebar.width <= layout.canvas.x);
        assert!(layout.canvas.x + layout.canvas.width <= layout.inspector.x);
        assert!(layout.timeline.x + layout.timeline.width <= layout.inspector.x);
        let last_tool = layout.tool_button(Tool::ALL.len() - 1);
        assert!(last_tool.x + last_tool.width < layout.shots_toggle().x);
        assert!(layout.shots_toggle().x + layout.shots_toggle().width < layout.previous_page().x);
        let rows = layout.timeline_rows();
        assert!(rows.music.y + rows.music.height <= layout.timeline.y + layout.timeline.height);
        assert!(rows.shots.y >= rows.pages.y + rows.pages.height);
        assert!(
            layout.sidebar_list().y + layout.sidebar_list().height <= layout.sidebar_import().y
        );
    }

    #[test]
    fn ctrl_click_then_clicks_close_a_polygon_on_the_first_vertex() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        assert_eq!(
            event(
                &mut ui,
                &project,
                UiEvent::CtrlClick {
                    x: page.x + 100.0,
                    y: page.y + 100.0
                }
            ),
            EventResponse::Consumed
        );
        for (x, y) in [
            (page.x + 300.0, page.y + 100.0),
            (page.x + 200.0, page.y + 300.0),
        ] {
            assert_eq!(click(&mut ui, &project, x, y), EventResponse::Consumed);
        }
        click(&mut ui, &project, page.x + 101.0, page.y + 101.0);
        assert!(matches!(
            event(
                &mut ui,
                &project,
                UiEvent::MouseRelease {
                    x: page.x + 101.0,
                    y: page.y + 101.0
                }
            ),
            EventResponse::Action(UiAction::ComicDubsAddBubble { .. })
        ));
    }

    #[test]
    fn polygon_tool_starts_a_draft_without_ctrl() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        assert!(ui.control_action("comic.tool.6", &project).is_none());
        assert_eq!(ui.tool(), Tool::Polygon);
        click(&mut ui, &project, page.x + 20.0, page.y + 20.0);
        click(&mut ui, &project, page.x + 120.0, page.y + 20.0);
        assert_eq!(ui.draft.len(), 2);
        assert!(ui.cancel_draft());
        assert!(ui.draft.is_empty());
        assert!(ui.cancel_draft());
        assert_eq!(ui.tool(), Tool::Select);
    }

    #[test]
    fn a_draft_vertex_can_be_moved_before_closing() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        let (x, y) = at(page, 0.2, 0.2);
        event(&mut ui, &project, UiEvent::CtrlClick { x, y });
        for (x, y) in [at(page, 0.6, 0.2), at(page, 0.4, 0.6)] {
            click(&mut ui, &project, x, y);
        }
        let second = screen_point(page, ui.draft[1]);
        let moved = (second.0 + page.width * 0.1, second.1 + page.height * 0.1);
        drag(&mut ui, &project, second, moved);
        assert!(ui.draft[1].x > 0.69 && ui.draft[1].y > 0.29);
        assert!(ui.cancel_draft());
        assert!(ui.draft.is_empty());
    }

    #[test]
    fn shape_tools_drag_or_click_to_create_styled_bubbles() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.control_action("comic.tool.3", &project);
        assert_eq!(ui.tool(), Tool::Shout);
        click(
            &mut ui,
            &project,
            at(page, 0.2, 0.2).0,
            at(page, 0.2, 0.2).1,
        );
        let end = at(page, 0.6, 0.5);
        event(&mut ui, &project, UiEvent::MouseMove { x: end.0, y: end.1 });
        assert!(!ui.scene(&project, layout()).overlay_quads.is_empty());
        let response = event(
            &mut ui,
            &project,
            UiEvent::MouseRelease { x: end.0, y: end.1 },
        );
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsAddStyledBubble {
                preset: BubblePreset::Shout,
                ref points,
                ..
            }) if points.len() == 36
                && points.iter().all(|point| point.x >= 0.19 && point.x <= 0.61)
        ));

        ui.control_action("comic.tool.2", &project);
        let center = at(page, 0.5, 0.5);
        assert!(matches!(
            drag(&mut ui, &project, center, center),
            EventResponse::Action(UiAction::ComicDubsAddStyledBubble {
                preset: BubblePreset::Classic,
                ..
            })
        ));
    }

    #[test]
    fn shot_tool_draws_shots_shaped_like_the_video() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_frame_aspect(Some(16.0 / 9.0));
        ui.control_action("comic.tool.7", &project);
        assert_eq!(ui.tool(), Tool::Shot);
        let response = drag(&mut ui, &project, at(page, 0.1, 0.1), at(page, 0.5, 0.2));
        let EventResponse::Action(UiAction::ComicDubsAddShot {
            region: Some(region),
            ..
        }) = response
        else {
            panic!("expected a new shot, got {response:?}");
        };
        assert!((region.x - 0.1).abs() < 0.01 && (region.y - 0.1).abs() < 0.01);
        assert!((region.width - 0.4).abs() < 0.01);
        assert!((region.width / region.height - 16.0 / 9.0).abs() < 0.01);

        // A plain click frames a standard shot around the pointer.
        let response = drag(&mut ui, &project, at(page, 0.5, 0.5), at(page, 0.5, 0.5));
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsAddShot { region: Some(region), .. })
                if region.contains(Point { x: 0.5, y: 0.5 })
                    && (region.width / region.height - 16.0 / 9.0).abs() < 0.01
        ));
    }

    #[test]
    fn shots_are_selected_by_their_tag_then_moved_and_resized() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let region = Region {
            x: 0.1,
            y: 0.1,
            width: 0.4,
            height: 0.225,
        };
        let shot = project.add_shot(page_id, Some(region)).unwrap();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_frame_aspect(Some(16.0 / 9.0));
        let screen = region_rect(page, region);
        let tag = shot_tag(screen, 0, false);
        let grab = (tag.x + 10.0, tag.y + 10.0);
        let response = drag(
            &mut ui,
            &project,
            grab,
            (grab.0 + page.width * 0.2, grab.1 + page.height * 0.1),
        );
        assert_eq!(ui.selected_shot(), Some(shot));
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsSetShot(CameraShot {
                region: Some(moved),
                ..
            })) if (moved.x - 0.3).abs() < 0.01 && (moved.y - 0.2).abs() < 0.01
                && moved.width == region.width
        ));
        // Bottom-right handle: the size changes, the video shape stays.
        let corner = (screen.x + screen.width, screen.y + screen.height);
        let response = drag(
            &mut ui,
            &project,
            corner,
            (corner.0 + page.width * 0.2, corner.1),
        );
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsSetShot(CameraShot {
                region: Some(resized),
                ..
            })) if (resized.x - 0.1).abs() < 0.001
                && (resized.width - 0.6).abs() < 0.01
                && (resized.width / resized.height - 16.0 / 9.0).abs() < 0.01
        ));
        assert_eq!(
            event(&mut ui, &project, UiEvent::Delete),
            EventResponse::Action(UiAction::ComicDubsRemoveShot(shot))
        );
    }

    #[test]
    fn shots_are_always_visible_numbered_and_can_be_hidden() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        project.add_shot(page_id, None);
        project.add_shot(
            page_id,
            Some(Region {
                x: 0.5,
                y: 0.5,
                width: 0.4,
                height: 0.225,
            }),
        );
        let bubble = project
            .add_bubble(page_id, square(0.55, 0.55, 0.1))
            .unwrap();
        project.set_bubble_text(bubble, "Salut".into());
        let mut ui = ComicDubsWorkspaceUi::default();
        let scene = ui.scene(&project, layout());
        let texts = scene
            .overlay_labels
            .iter()
            .map(|label| label.text.as_str())
            .collect::<Vec<_>>();
        assert!(texts.contains(&"PLAN 1 · PAGE ENTIÈRE"));
        assert!(texts.contains(&"PLAN 2"));
        // The bubble tells which shot shows it.
        assert!(texts.contains(&"P2"));
        assert!(scene
            .controls
            .iter()
            .any(|control| control.id.starts_with("comic.canvas.shot.")));
        ui.control_action("comic.header.shots", &project);
        let hidden = ui.scene(&project, layout());
        assert!(hidden
            .overlay_labels
            .iter()
            .all(|label| !label.text.starts_with("PLAN")));
    }

    #[test]
    fn polygon_hit_test_rejects_its_bounding_box_corners() {
        let triangle = [
            Point { x: 0.5, y: 0.1 },
            Point { x: 0.9, y: 0.9 },
            Point { x: 0.1, y: 0.9 },
        ];
        assert!(point_in_polygon(Point { x: 0.5, y: 0.5 }, &triangle));
        assert!(!point_in_polygon(Point { x: 0.1, y: 0.1 }, &triangle));
    }

    #[test]
    fn page_image_is_not_covered_by_an_opaque_overlay() {
        let project = project();
        let scene = ComicDubsWorkspaceUi::default().scene(&project, layout());
        let page = scene.page_rect.unwrap();
        assert_eq!(scene.page_layers.len(), 1);
        assert_eq!(scene.page_layers[0].uv, [0.0, 0.0, 1.0, 1.0]);
        assert!(!scene.overlay_quads.iter().any(|quad| {
            quad.rect == [page.x, page.y, page.width, page.height] && quad.color[3] == 1.0
        }));
        // The page list shows a thumbnail of every page.
        assert_eq!(scene.thumbnails.len(), 1);
    }

    #[test]
    fn bubble_fill_and_edges_share_the_polygon_geometry() {
        let rect = Rect {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 300.0,
        };
        let points = [
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.4 },
            Point { x: 0.4, y: 0.8 },
        ];
        let screen = points
            .iter()
            .map(|point| screen_point(rect, *point))
            .collect::<Vec<_>>();
        let fill = fill_screen_polygon(&screen, opaque_rgba([255, 80, 40, 20]), rect);
        assert!(!fill.is_empty());
        assert!(fill.iter().all(|quad| quad.color[3] == 1.0));

        let edge = line_quad(rect, points[0], points[1], ACCENT, 2.0);
        let a = screen_point(rect, points[0]);
        let b = screen_point(rect, points[1]);
        assert!((edge.rect[0] + edge.rect[2] * 0.5 - (a.0 + b.0) * 0.5).abs() < 0.01);
        assert!((edge.rect[1] + edge.rect[3] * 0.5 - (a.1 + b.1) * 0.5).abs() < 0.01);
    }

    #[test]
    fn fills_and_edges_are_clipped_to_the_canvas() {
        let clip = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let fill = fill_screen_polygon(
            &[
                (-50.0, -50.0),
                (150.0, -50.0),
                (150.0, 150.0),
                (-50.0, 150.0),
            ],
            [1.0; 4],
            clip,
        );
        assert!(fill
            .iter()
            .all(|quad| quad.rect[0] >= 0.0 && quad.rect[0] + quad.rect[2] <= 100.0));
        assert_eq!(
            clip_segment((-50.0, 50.0), (150.0, 50.0), clip),
            Some(((0.0, 50.0), (100.0, 50.0)))
        );
        assert_eq!(clip_segment((-50.0, -5.0), (-10.0, -5.0), clip), None);
        let frame = frame_rect(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 400.0,
            },
            Some(16.0 / 9.0),
        );
        assert_eq!((frame.width, frame.height.round()), (400.0, 225.0));
    }

    #[test]
    fn single_click_selects_and_drag_moves_without_editing_text() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.4, y: 0.2 },
                    Point { x: 0.3, y: 0.4 },
                ],
            )
            .unwrap();
        let page = page_rect(&project);
        let start = screen_point(page, Point { x: 0.3, y: 0.28 });
        let mut ui = ComicDubsWorkspaceUi::default();
        let end = (start.0 + page.width * 0.1, start.1 + page.height * 0.1);
        let response = drag(&mut ui, &project, start, end);
        assert_eq!(ui.selected_bubble(), Some(bubble_id));
        assert!(!ui.is_editing_text());
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                bubble_id: id,
                points
            }) if id == bubble_id && points[0].x > 0.29 && points[0].y > 0.29
        ));
        ui.set_focused_control(Some("comic.sidebar.import"));
        assert_eq!(
            event(&mut ui, &project, UiEvent::CursorRight),
            EventResponse::Ignored
        );
        ui.set_focused_control(None);
        assert_eq!(
            event(&mut ui, &project, UiEvent::CursorRight),
            EventResponse::Action(UiAction::ComicDubsNudgeBubble {
                bubble_id,
                dx: 0.004,
                dy: 0.0,
            })
        );
        assert_eq!(
            event(&mut ui, &project, UiEvent::Delete),
            EventResponse::Action(UiAction::ComicDubsRemoveBubble(bubble_id))
        );
    }

    #[test]
    fn shift_click_adds_and_removes_vertices_of_the_selected_bubble() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.4)).unwrap();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble_id));
        let edge = screen_point(page, Point { x: 0.4, y: 0.2 });
        assert!(matches!(
            event(&mut ui, &project, UiEvent::ShiftMousePress { x: edge.0, y: edge.1 }),
            EventResponse::Action(UiAction::ComicDubsInsertBubbleVertex {
                bubble_id: id,
                after: 0,
                point,
            }) if id == bubble_id && (point.x - 0.4).abs() < 0.01
        ));
        let corner = screen_point(page, Point { x: 0.6, y: 0.6 });
        assert_eq!(
            event(
                &mut ui,
                &project,
                UiEvent::ShiftMousePress {
                    x: corner.0,
                    y: corner.1
                }
            ),
            EventResponse::Action(UiAction::ComicDubsRemoveBubbleVertex {
                bubble_id,
                index: 2,
            })
        );
    }

    #[test]
    fn selected_vertices_move_independently_and_empty_canvas_deselects() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.5, y: 0.2 },
                    Point { x: 0.35, y: 0.5 },
                ],
            )
            .unwrap();
        let page = page_rect(&project);
        let center = screen_point(page, Point { x: 0.35, y: 0.3 });
        let vertex = screen_point(page, Point { x: 0.2, y: 0.2 });
        let mut ui = ComicDubsWorkspaceUi::default();
        drag(&mut ui, &project, center, center);
        let moved = (vertex.0 + page.width * 0.05, vertex.1 + page.height * 0.05);
        assert!(matches!(
            drag(&mut ui, &project, vertex, moved),
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints { bubble_id: id, points })
                if id == bubble_id && points[0].x > 0.24 && points[1].x == 0.5
        ));

        ui.begin_text_edit(bubble_id, "Texte".into());
        let empty = at(page, 0.9, 0.9);
        assert_eq!(
            click(&mut ui, &project, empty.0, empty.1),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), None);
        assert!(!ui.is_editing_text());
    }

    #[test]
    fn text_is_edited_with_a_movable_caret() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.4)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.begin_text_edit(bubble_id, "Salut".into());
        event(&mut ui, &project, UiEvent::CursorLeft);
        event(&mut ui, &project, UiEvent::CursorLeft);
        let text = |response: EventResponse| match response {
            EventResponse::Action(UiAction::ComicDubsSetBubbleText { text, .. }) => text,
            other => panic!("expected a text edit, got {other:?}"),
        };
        assert_eq!(
            text(event(
                &mut ui,
                &project,
                UiEvent::KeyInput { text: "é".into() }
            )),
            "Saléut"
        );
        assert_eq!(
            text(event(
                &mut ui,
                &project,
                UiEvent::KeyInput {
                    text: "\x08".into()
                }
            )),
            "Salut"
        );
        assert_eq!(text(event(&mut ui, &project, UiEvent::Delete)), "Salt");
        // The caret is shown in the bubble and in the inspector.
        let scene = ui.scene(&project, layout());
        assert!(scene
            .overlay_labels
            .iter()
            .any(|label| label.text == "Sal|t"));
        assert!(scene.labels.iter().any(|label| label.text == "Sal|t"));
        assert_eq!(
            event(&mut ui, &project, UiEvent::KeyInput { text: "\r".into() }),
            EventResponse::Consumed
        );
        assert!(!ui.is_editing_text());
    }

    #[test]
    fn edited_text_renders_above_an_opaque_bubble_scaled_like_the_export() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.6)).unwrap();
        project.set_bubble_font_size(bubble_id, 18.0);
        project.set_bubble_letter_spacing(bubble_id, 6.0);
        project.set_bubble_look(
            bubble_id,
            BubbleLook {
                italic: true,
                ..BubbleLook::default()
            },
        );
        project.set_settings(Some("Arial".into()), 250, 250, 24.0);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.begin_text_edit(bubble_id, "Nouveau texte".into());
        let scene = ui.scene(&project, layout());
        let page = scene.page_rect.unwrap();
        let scale = page.height / 1_080.0;
        let label = scene
            .overlay_labels
            .iter()
            .find(|label| label.text.contains("Nouveau"))
            .unwrap();
        assert!(label.text.ends_with('|'));
        assert!(label.font_size <= 18.0 * scale + 0.01);
        assert!((label.letter_spacing - 6.0 * scale).abs() < 0.01);
        assert_eq!(label.font_family.as_deref(), Some("Arial"));
        assert!(label
            .style
            .is_some_and(|style| style.italic && style.alpha == 255));
        let mut quads = Vec::new();
        let mut labels = Vec::new();
        append_overlay(&mut quads, &mut labels, &scene);
        assert!(labels
            .iter()
            .any(|label| matches!(label.overflow, Overflow::Styled(style) if style.italic)));
        assert!(scene.overlay_quads.iter().any(|quad| quad.color[3] == 1.0));
    }

    #[test]
    fn sliders_drag_as_one_gesture_and_follow_the_keyboard() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.4)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble_id));
        let size = item(&mut ui, &project, "comic.inspector.text.size");
        let track = slider_track(size.rect);
        let first = click(&mut ui, &project, track.x + track.width, track.y + 4.0);
        assert!(matches!(
            first,
            EventResponse::Action(UiAction::ComicDubsGesture { first: true, ref action })
                if **action == UiAction::ComicDubsSetBubbleFontSize {
                    bubble_id,
                    font_size: 72.0,
                }
        ));
        let next = event(
            &mut ui,
            &project,
            UiEvent::MouseMove {
                x: track.x,
                y: track.y + 4.0,
            },
        );
        assert!(matches!(
            next,
            EventResponse::Action(UiAction::ComicDubsGesture { first: false, ref action })
                if **action == UiAction::ComicDubsSetBubbleFontSize {
                    bubble_id,
                    font_size: 6.0,
                }
        ));
        event(
            &mut ui,
            &project,
            UiEvent::MouseRelease {
                x: track.x,
                y: track.y,
            },
        );
        // A drag starting on the current value opens its gesture on the
        // first real change.
        let thumb = track.x + track.width * ((24.0 - 6.0) / 66.0);
        assert_eq!(
            click(&mut ui, &project, thumb, track.y + 4.0),
            EventResponse::Consumed
        );
        assert!(matches!(
            event(
                &mut ui,
                &project,
                UiEvent::MouseMove {
                    x: track.x,
                    y: track.y + 4.0
                }
            ),
            EventResponse::Action(UiAction::ComicDubsGesture { first: true, .. })
        ));
        event(
            &mut ui,
            &project,
            UiEvent::MouseRelease {
                x: track.x,
                y: track.y,
            },
        );
        ui.set_focused_control(Some("comic.inspector.text.size"));
        assert!(matches!(
            event(&mut ui, &project, UiEvent::CursorRight),
            EventResponse::Action(UiAction::ComicDubsGesture { first: true, ref action })
                if **action == UiAction::ComicDubsSetBubbleFontSize {
                    bubble_id,
                    font_size: 25.0,
                }
        ));
    }

    #[test]
    fn dropdowns_list_every_choice_and_apply_the_one_clicked() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.4)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble_id));
        let entrance = item(&mut ui, &project, "comic.inspector.anim.entrance");
        // Scroll the item into view if needed, then open it.
        let field = dropdown_field(entrance.rect);
        assert_eq!(
            click(&mut ui, &project, field.x + 10.0, field.y + 10.0),
            EventResponse::Consumed
        );
        let scene = ui.scene(&project, layout());
        for choice in BubbleEntrance::ALL {
            assert!(scene
                .popup_labels
                .iter()
                .any(|label| label.text == choice.label()));
        }
        let dropdown = ui.dropdown.clone().unwrap();
        let row = dropdown.row(layout().content, 1);
        assert!(matches!(
            click(&mut ui, &project, row.x + 10.0, row.y + 10.0),
            EventResponse::Action(UiAction::ComicDubsSetBubbleFx {
                fx: BubbleFx {
                    entrance: BubbleEntrance::Fade,
                    ..
                },
                ..
            })
        ));
        assert!(ui.dropdown.is_none());
        // Keyboard: open, move down, activate.
        assert_eq!(
            ui.control_action("comic.inspector.anim.reveal", &project),
            None
        );
        assert!(ui.dropdown.is_some());
        event(&mut ui, &project, UiEvent::CursorDown);
        assert!(matches!(
            event(&mut ui, &project, UiEvent::Activate),
            EventResponse::Action(UiAction::ComicDubsSetBubbleFx {
                fx: BubbleFx {
                    text_reveal: TextReveal::Typewriter,
                    ..
                },
                ..
            })
        ));
        // Enter on the focused list chooses the highlighted entry.
        ui.control_action("comic.inspector.anim.reveal", &project);
        event(&mut ui, &project, UiEvent::CursorDown);
        event(&mut ui, &project, UiEvent::CursorDown);
        assert!(matches!(
            ui.control_action("comic.inspector.anim.reveal", &project),
            Some(UiAction::ComicDubsSetBubbleFx {
                fx: BubbleFx {
                    text_reveal: TextReveal::Words,
                    ..
                },
                ..
            })
        ));
        // Escape closes an open list first.
        ui.control_action("comic.inspector.anim.reveal", &project);
        assert!(ui.cancel_draft());
        assert!(ui.dropdown.is_none());
        assert_eq!(ui.selected_bubble(), Some(bubble_id));
    }

    #[test]
    fn inspector_swatches_open_the_color_picker() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.6)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble_id));
        assert_eq!(
            ui.control_action("comic.inspector.style.colors.0", &project),
            None
        );
        assert!(ui.color_picker.active);
        assert!(ui.color_picker.can_be_transparent);
        let transparent = ui.color_picker.transparent_rect();
        assert_eq!(
            click(&mut ui, &project, transparent.x + 1.0, transparent.y + 1.0),
            EventResponse::Action(UiAction::ComicDubsSetBubbleColor {
                bubble_id,
                color: [255, 255, 255, 0],
            })
        );
        ui.color_picker.close();
        ui.control_action("comic.inspector.style.colors.1", &project);
        assert!(ui.color_picker.active && !ui.color_picker.can_be_transparent);
        let origin = ui.color_picker.origin;
        assert!(matches!(
            click(&mut ui, &project, origin.0 + 80.0, origin.1 + 80.0),
            EventResponse::Action(UiAction::ComicDubsSetBubbleLook {
                bubble_id: id,
                look,
            }) if id == bubble_id && look.outline_color.is_some()
        ));
    }

    #[test]
    fn keyboard_activates_presets_toggles_and_segments() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.3)).unwrap();
        let audio = project.add_audio("music.wav".into(), "music.flac".into(), recorded());
        project.set_studio(StudioSettings {
            music_audio_id: Some(audio),
            ..StudioSettings::default()
        });
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble_id));
        assert_eq!(
            ui.control_action("comic.inspector.style.preset0.1", &project),
            Some(UiAction::ComicDubsApplyPreset {
                bubble_id,
                preset: BubblePreset::Shout,
            })
        );
        assert_eq!(
            ui.control_action("comic.inspector.text.align.2", &project),
            Some(UiAction::ComicDubsSetBubbleTextAlignment {
                bubble_id,
                alignment: TextAlignment::Right,
            })
        );
        ui.control_action("comic.inspector.deselect", &project);
        assert_eq!(ui.selected_bubble(), None);
        ui.control_action("comic.overview.1", &project);
        assert!(matches!(
            ui.control_action("comic.inspector.project.music_ducking", &project),
            Some(UiAction::ComicDubsSetStudio(StudioSettings {
                music_ducking: false,
                ..
            }))
        ));
    }

    #[test]
    fn page_settings_are_shown_when_nothing_is_selected() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let shot = project.add_shot(page_id, None).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.sync(&project, layout());
        let scene = ui.scene(&project, layout());
        assert!(scene.labels.iter().any(|label| label.text == "Page 1 / 1"));
        // Shots are listed and selectable from the page settings.
        assert_eq!(
            ui.control_action("comic.inspector.page.shot.0", &project),
            None
        );
        assert_eq!(ui.selected_shot(), Some(shot));
        ui.select_shot(None);
        ui.control_action("comic.inspector.page.transition_kind", &project);
        event(&mut ui, &project, UiEvent::CursorDown);
        assert!(matches!(
            event(&mut ui, &project, UiEvent::Activate),
            EventResponse::Action(UiAction::ComicDubsSetPageFx {
                fx: PageFx {
                    transition: PageTransition::FadeBlack,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            ui.control_action("comic.inspector.page.shot_add.1", &project),
            Some(UiAction::ComicDubsAddShot {
                page_id,
                region: None
            })
        );
        ui.control_action("comic.inspector.page.shot_add.0", &project);
        assert_eq!(ui.tool(), Tool::Shot);
    }

    #[test]
    fn shot_inspector_edits_movement_order_and_lists_its_bubbles() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        project.add_shot(page_id, None);
        let bubble = project.add_bubble(page_id, square(0.6, 0.6, 0.1)).unwrap();
        let shot = project.add_shot_around_bubble(bubble, 16.0 / 9.0).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_shot(Some(shot));
        assert!(matches!(
            ui.control_action("comic.inspector.shot.movement.1", &project),
            Some(UiAction::ComicDubsSetShot(CameraShot {
                movement: ShotMovement::Cut,
                ..
            }))
        ));
        assert_eq!(
            ui.control_action("comic.inspector.shot.move.0", &project),
            Some(UiAction::ComicDubsMoveShot {
                shot_id: shot,
                delta: -1
            })
        );
        assert!(matches!(
            ui.control_action("comic.inspector.shot.kind.1", &project),
            Some(UiAction::ComicDubsSetShot(CameraShot { region: None, .. }))
        ));
        assert_eq!(
            ui.control_action("comic.inspector.shot.bubble.0", &project),
            None
        );
        assert_eq!(ui.selected_bubble(), Some(bubble));
    }

    #[test]
    fn vertex_editor_replaces_panels_and_dragging_creates_a_step_pose() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project
            .add_bubble(
                page_id,
                vec![
                    Point { x: 0.2, y: 0.2 },
                    Point { x: 0.8, y: 0.2 },
                    Point { x: 0.5, y: 0.8 },
                ],
            )
            .unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.open_vertex_editor(bubble_id);
        let scene = ui.scene(&project, layout());
        assert!(scene
            .labels
            .iter()
            .any(|label| label.text == "ANIMATION DES SOMMETS DE LA BULLE"));
        assert!(!scene
            .controls
            .iter()
            .any(|control| control.id.starts_with("comic.overview.")));
        assert!(scene.icons.iter().any(|icon| icon.name == "comic/close"));

        let page = scene.page_rect.unwrap();
        let vertex = screen_point(page, project.bubble(bubble_id).unwrap().points[0]);
        let moved = (vertex.0 + page.width * 0.05, vertex.1 + page.height * 0.05);
        assert!(matches!(
            drag(&mut ui, &project, vertex, moved),
            EventResponse::Action(UiAction::ComicDubsSetBubbleVertexKeyframe {
                bubble_id: id,
                at_ms: 0,
                points,
            }) if id == bubble_id && points[0].x > 0.24
        ));
        assert!(ui.cancel_draft());
        assert!(!ui.vertex_editor_open());
    }

    #[test]
    fn audio_import_has_a_visible_loading_state() {
        let project = project();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_pending_audio_imports(2);
        let scene = ui.scene(&project, layout());
        assert_eq!(ui.sidebar_tab, SidebarTab::Sounds);
        assert!(scene
            .labels
            .iter()
            .any(|label| label.text == "Chargement de 2 son(s)…"));
    }

    #[test]
    fn persisted_srgb_bubble_color_is_linearized_for_gpu() {
        let color = opaque_rgba([128, 64, 32, 255]);
        assert!((color[0] - 0.21586).abs() < 0.0001);
        assert!((color[1] - 0.05127).abs() < 0.0001);
        assert!((color[2] - 0.01444).abs() < 0.0001);
        assert_eq!(color[3], 1.0);
    }

    #[test]
    fn preview_reveals_bubbles_in_reading_order_inside_the_video_frame() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        for (index, text) in ["Première", "Deuxième"].into_iter().enumerate() {
            let id = project
                .add_bubble(page_id, square(0.1 + index as f32 * 0.4, 0.2, 0.3))
                .unwrap();
            project.set_bubble_text(id, text.into());
        }
        let plan = Timeline::build(&project, None, 40);
        let second = plan.pages[0].cues[1].reveal_ms;
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_frame_aspect(Some(16.0 / 9.0));
        ui.set_preview(Some(10), false);
        let first = ui.scene(&project, layout());
        assert!(first
            .overlay_labels
            .iter()
            .any(|label| label.text == "Première"));
        assert!(first
            .overlay_labels
            .iter()
            .all(|label| label.text != "Deuxième"));
        // No editing chrome while previewing.
        assert!(first.overlay_labels.iter().all(|label| label.text != "1"));
        let view = frame_rect(layout().canvas, Some(16.0 / 9.0));
        assert!(first.page_layers.iter().all(|layer| {
            layer.rect.x >= view.x - 0.01
                && layer.rect.x + layer.rect.width <= view.x + view.width + 0.01
        }));

        ui.set_preview(Some(second + 10), true);
        let later = ui.scene(&project, layout());
        assert!(later
            .overlay_labels
            .iter()
            .any(|label| label.text == "Deuxième"));
        assert!(later
            .labels
            .iter()
            .any(|label| label.text.contains("Lecture en cours")));
    }

    #[test]
    fn preview_crops_the_page_on_shots_and_draws_flashes_on_top() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble = project.add_bubble(page_id, square(0.1, 0.1, 0.15)).unwrap();
        project.set_bubble_text(bubble, "Zoom".into());
        project.set_bubble_fx(
            bubble,
            BubbleFx {
                screen_effect: ScreenEffect::Flash,
                screen_effect_ms: 1_000,
                ..BubbleFx::default()
            },
        );
        project.add_shot_around_bubble(bubble, 16.0 / 9.0);
        let plan = Timeline::build(&project, None, 40);
        let reveal = plan.pages[0].cues[0].reveal_ms;
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_frame_aspect(Some(16.0 / 9.0));
        ui.set_preview(Some(reveal + 20), false);
        let scene = ui.scene(&project, layout());
        let layer = scene.page_layers[0];
        assert!(layer.uv[2] - layer.uv[0] < 0.9);
        assert!(scene.top_quads.iter().any(|quad| quad.color[3] > 0.5));
    }

    #[test]
    fn timeline_blocks_select_and_the_ruler_scrubs() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let first = project.add_bubble(page_id, square(0.1, 0.1, 0.2)).unwrap();
        let second = project.add_bubble(page_id, square(0.5, 0.5, 0.2)).unwrap();
        for id in [first, second] {
            project.set_bubble_text(id, "Texte".into());
        }
        let shot = project.add_shot(page_id, None).unwrap();
        let plan = Timeline::build(&project, None, 40);
        let layout = layout();
        let track = layout.timeline_track();
        let rows = layout.timeline_rows();
        let cue = &plan.pages[0].cues[1];
        let block = cue_span(&plan, track, cue);
        let mut ui = ComicDubsWorkspaceUi::default();
        assert_eq!(
            click(
                &mut ui,
                &project,
                block.x + block.width * 0.5,
                rows.bubbles.y + 4.0
            ),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), Some(second));
        assert_eq!(ui.preview_ms(), None);
        let shot_block = plan.pages[0].shots[0];
        click(
            &mut ui,
            &project,
            time_x(&plan, track, shot_block.start_ms) + 4.0,
            rows.shots.y + 4.0,
        );
        assert_eq!(ui.selected_shot(), Some(shot));

        let response = click(
            &mut ui,
            &project,
            track.x + track.width * 0.5,
            rows.ruler.y + 4.0,
        );
        assert!(matches!(
            response,
            EventResponse::Action(UiAction::ComicDubsSeek(_))
        ));
        assert!(ui.preview_ms().is_some());
        event(
            &mut ui,
            &project,
            UiEvent::MouseRelease {
                x: track.x,
                y: rows.ruler.y,
            },
        );
        let canvas = layout.canvas;
        click(&mut ui, &project, canvas.x + 5.0, canvas.y + 5.0);
        assert_eq!(ui.preview_ms(), None);
        assert_eq!(ui.control_action("comic.timeline.cue.0.0", &project), None);
        assert_eq!(ui.selected_bubble(), Some(first));
        assert_eq!(
            ui.control_action("comic.timeline.play", &project),
            Some(UiAction::ComicDubsTogglePlayback)
        );
    }

    #[test]
    fn bubble_badges_sit_outside_the_bubble_and_hide_during_playback() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let first = project.add_bubble(page_id, square(0.3, 0.3, 0.3)).unwrap();
        project
            .add_bubble(page_id, square(0.65, 0.65, 0.2))
            .unwrap();
        let audio = project.add_audio("line.wav".into(), "line.flac".into(), recorded());
        project.assign_audio(first, Some(audio));
        let mut ui = ComicDubsWorkspaceUi::default();
        let editing = ui.scene(&project, layout());
        let page = editing.page_rect.unwrap();
        let badge = editing
            .overlay_labels
            .iter()
            .find(|label| label.text == "1")
            .unwrap();
        let bubble_top_left = screen_point(page, Point { x: 0.3, y: 0.3 });
        assert!(badge.bounds.x < bubble_top_left.0 && badge.bounds.y < bubble_top_left.1);
        assert!(editing.overlay_labels.iter().any(|label| label.text == "2"));
        assert!(editing
            .overlay_icons
            .iter()
            .any(|icon| icon.name == "comic/mic"));

        ui.set_preview(Some(10), true);
        let playback = ui.scene(&project, layout());
        assert!(playback
            .overlay_labels
            .iter()
            .all(|label| label.text != "1" && label.text != "2"));
    }

    #[test]
    fn sliding_pages_keep_their_bubbles_inside_their_own_frame() {
        let mut project = ComicDubsProject::default();
        for (name, color) in [("1.png", [250, 0, 0, 255]), ("2.png", [0, 0, 250, 255])] {
            let page = project.add_page(name.into(), name.into(), 4_000, 1_000);
            let bubble = project
                .add_bubble(
                    page,
                    vec![
                        Point { x: 0.02, y: 0.3 },
                        Point { x: 0.98, y: 0.3 },
                        Point { x: 0.98, y: 0.7 },
                        Point { x: 0.02, y: 0.7 },
                    ],
                )
                .unwrap();
            project.set_bubble_color(bubble, color);
            project.set_bubble_text(bubble, "Texte".into());
            project.set_page_fx(
                page,
                PageFx {
                    transition: PageTransition::SlideLeft,
                    transition_ms: 1_000,
                    motion: PageMotion::ZoomOut,
                    ..PageFx::default()
                },
            );
        }
        let plan = Timeline::build(&project, None, 40);
        let middle = plan.pages[1].start_ms + 500;
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_preview(Some(middle), false);
        let scene = ui.scene(&project, layout());
        let view = layout().canvas;
        let red = gpu_rgba([250, 0, 0, 255]);
        let blue = gpu_rgba([0, 0, 250, 255]);
        let is = |quad: &QuadInstance, color: [f32; 4]| {
            (quad.color[0] - color[0]).abs() < 0.001 && (quad.color[2] - color[2]).abs() < 0.001
        };
        let split = view.x + view.width * 0.5;
        let reds = scene
            .overlay_quads
            .iter()
            .filter(|quad| is(quad, red))
            .collect::<Vec<_>>();
        let blues = scene
            .overlay_quads
            .iter()
            .filter(|quad| is(quad, blue))
            .collect::<Vec<_>>();
        assert!(!reds.is_empty() && !blues.is_empty());
        assert!(reds
            .iter()
            .all(|quad| quad.rect[0] + quad.rect[2] <= split + 1.5));
        assert!(blues.iter().all(|quad| quad.rect[0] >= split - 1.5));
        assert_eq!(scene.page_layers.len(), 2);
    }

    #[test]
    fn audio_drops_assign_voices_sfx_and_music() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.6)).unwrap();
        let audio_id = project.add_audio("line.wav".into(), "line.flac".into(), recorded());
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.control_action("comic.sidebar.tab.1", &project);
        let row = ui.audio_row(layout(), 0);
        assert_eq!(
            click(&mut ui, &project, row.x + 40.0, row.y + 10.0),
            EventResponse::Consumed
        );
        let center = at(page, 0.5, 0.5);
        assert!(matches!(
            event(&mut ui, &project, UiEvent::MouseRelease { x: center.0, y: center.1 }),
            EventResponse::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: Some(audio)
            }) if id == bubble_id && audio == audio_id
        ));
        project.assign_audio(bubble_id, Some(audio_id));
        assert_eq!(
            event(
                &mut ui,
                &project,
                UiEvent::ContextMenu {
                    x: center.0,
                    y: center.1
                }
            ),
            EventResponse::Action(UiAction::ComicDubsPlayAudio(audio_id))
        );
        assert_eq!(ui.selected_bubble(), Some(bubble_id));
        // Drop on the sound settings.
        let target = item(&mut ui, &project, "comic.inspector.sound.sfx").rect;
        click(&mut ui, &project, row.x + 40.0, row.y + 10.0);
        assert!(matches!(
            event(
                &mut ui,
                &project,
                UiEvent::MouseRelease {
                    x: target.x + target.width * 0.5,
                    y: target.y + 30.0
                }
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubbleSound {
                bubble_id: id,
                sound: BubbleSound {
                    sfx_audio_id: Some(audio),
                    ..
                },
            }) if id == bubble_id && audio == audio_id
        ));
    }

    #[test]
    fn bubbles_read_outside_their_shot_are_flagged() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        project.add_shot(
            page_id,
            Some(Region {
                x: 0.5,
                y: 0.5,
                width: 0.4,
                height: 0.225,
            }),
        );
        let outside = project
            .add_bubble(page_id, square(0.05, 0.05, 0.1))
            .unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_frame_aspect(Some(16.0 / 9.0));
        let scene = ui.scene(&project, layout());
        assert!(scene
            .overlay_labels
            .iter()
            .any(|label| label.text == "P1 · hors du cadre"));
        ui.select_bubble(Some(outside));
        assert!(ui
            .inspector_items(&project, layout())
            .iter()
            .any(|item| item.id == "comic.inspector.camera.outside"));
        // Framing it in a new shot is one click away.
        assert_eq!(
            ui.control_action("comic.inspector.camera.around", &project),
            Some(UiAction::ComicDubsAddShotAroundBubble(outside))
        );
    }

    #[test]
    fn vertex_editor_leaves_the_transport_row_visible() {
        let editor = VertexEditorLayout::compute(layout());
        let toolbar = layout().toolbar;
        assert!(editor.header.y + editor.header.height <= toolbar.y);
        assert!(editor.stage.y >= toolbar.y + toolbar.height);
        assert!(editor.stage.y + editor.stage.height <= editor.timeline_panel.y);
    }

    #[test]
    fn ctrl_wheel_zooms_around_the_pointer_and_edits_follow_the_zoom() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble = project.add_bubble(page_id, square(0.6, 0.6, 0.1)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        let before = page_rect(&project);
        let anchor = at(before, 0.65, 0.65);
        for _ in 0..8 {
            event(
                &mut ui,
                &project,
                UiEvent::Scroll {
                    x: anchor.0,
                    y: anchor.1,
                    delta: 1.0,
                    fast: false,
                    ctrl: true,
                },
            );
        }
        let zoomed = ui.page_rect(layout().canvas, project.active_page().unwrap());
        assert!(zoomed.width > before.width * 2.0);
        // The page point under the pointer stays under it.
        let point = point_at(zoomed, anchor.0, anchor.1);
        assert!((point.x - 0.65).abs() < 0.01 && (point.y - 0.65).abs() < 0.01);
        // Clicking still hits the bubble in the zoomed view.
        click(&mut ui, &project, anchor.0, anchor.1);
        assert_eq!(ui.selected_bubble(), Some(bubble));
        let scene = ui.scene(&project, layout());
        let layer = scene.page_layers[0];
        assert!(layer.uv[3] - layer.uv[1] < 0.6);
        assert!(contains_rect(layout().canvas, layer.rect));
        // The zoom button fits the page again.
        assert_eq!(ui.control_action("comic.header.zoom", &project), None);
        assert_eq!(
            ui.page_rect(layout().canvas, project.active_page().unwrap()),
            before
        );
    }

    #[test]
    fn drawing_a_bubble_returns_to_the_selection_tool() {
        let project = project();
        let page = page_rect(&project);
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.control_action("comic.tool.1", &project);
        drag(&mut ui, &project, at(page, 0.2, 0.2), at(page, 0.4, 0.3));
        assert_eq!(ui.tool(), Tool::Select);
        // The shot tool stays active to draw several shots in a row.
        ui.control_action("comic.tool.7", &project);
        drag(&mut ui, &project, at(page, 0.2, 0.2), at(page, 0.4, 0.3));
        assert_eq!(ui.tool(), Tool::Shot);
    }

    #[test]
    fn inspector_sections_fold_and_stay_folded() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble = project.add_bubble(page_id, square(0.2, 0.2, 0.3)).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.select_bubble(Some(bubble));
        let count = ui.inspector_items(&project, layout()).len();
        let section = item(&mut ui, &project, "comic.inspector.style");
        click(
            &mut ui,
            &project,
            section.rect.x + 30.0,
            section.rect.y + 10.0,
        );
        let folded = ui.inspector_items(&project, layout());
        assert!(folded.len() < count);
        assert!(folded
            .iter()
            .all(|item| !item.id.starts_with("comic.inspector.style.")));
        // Another bubble keeps the same folded sections.
        ui.select_bubble(None);
        ui.select_bubble(Some(bubble));
        assert_eq!(ui.inspector_items(&project, layout()).len(), folded.len());
        ui.control_action("comic.inspector.style", &project);
        assert_eq!(ui.inspector_items(&project, layout()).len(), count);
    }

    #[test]
    fn no_button_uses_a_glyph_or_emoji_as_its_label() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble = project.add_bubble(page_id, square(0.2, 0.2, 0.3)).unwrap();
        let audio = project.add_audio("line.wav".into(), "line.flac".into(), recorded());
        project.assign_audio(bubble, Some(audio));
        let shot = project.add_shot(page_id, None).unwrap();
        let mut ui = ComicDubsWorkspaceUi::default();
        let mut scenes = vec![ui.scene(&project, layout())];
        ui.control_action("comic.overview.1", &project);
        scenes.push(ui.scene(&project, layout()));
        ui.select_bubble(Some(bubble));
        scenes.push(ui.scene(&project, layout()));
        ui.control_action("comic.inspector.anim.entrance", &project);
        scenes.push(ui.scene(&project, layout()));
        ui.cancel_draft();
        ui.select_shot(Some(shot));
        scenes.push(ui.scene(&project, layout()));
        ui.control_action("comic.sidebar.tab.1", &project);
        scenes.push(ui.scene(&project, layout()));
        ui.open_vertex_editor(bubble);
        scenes.push(ui.scene(&project, layout()));
        let glyph = |character: char| {
            matches!(character as u32,
                0x2190..=0x21FF | 0x2300..=0x23FF | 0x25A0..=0x25FF | 0x2600..=0x27BF
                | 0x2B00..=0x2BFF | 0x1F000..=0x1FFFF)
        };
        for scene in &scenes {
            for label in scene
                .labels
                .iter()
                .chain(&scene.overlay_labels)
                .chain(&scene.popup_labels)
            {
                assert!(
                    !label.text.chars().any(glyph),
                    "glyph in label {:?}",
                    label.text
                );
            }
        }
        assert!(scenes[2].icons.len() > 10);
    }
}
