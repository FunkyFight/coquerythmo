//! Comic Dubs studio: layout, interaction and scene generation.
//!
//! The canvas has two modes. While editing, every bubble is drawn in place
//! with its handles. While playing or scrubbing the timeline strip, the
//! canvas shows the exact frame the export renders: camera, page
//! transitions, bubble animations and screen effects all come from
//! [`crate::comic_dubs_timeline`].

use crate::comic_dubs::{
    Bubble, BubbleEmphasis, BubbleEntrance, BubbleFx, BubbleId, BubbleLook, BubblePreset,
    BubbleSound, CameraFocus, ComicAudioId, ComicDubsProject, Page, PageFx, PageId, PageMotion,
    PageTransition, Point, Region, ScreenEffect, StudioSettings, TextAlignment,
};
use crate::comic_dubs_shapes::{self, ShapeKind};
use crate::comic_dubs_text::{self as text_layout, LineReveal, TextLayout};
use crate::comic_dubs_timeline::{
    self as timeline, BubbleFrame, CameraTarget, LayerFrame, Placement, Timeline,
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

const SIDEBAR_W: f32 = 292.0;
const INSPECTOR_W: f32 = 276.0;
const HEADER_H: f32 = 52.0;
const TOOLBAR_H: f32 = 42.0;
const ROW_H: f32 = 44.0;
const TOOLS_W: f32 = 38.0;
const TOOL_BUTTON: f32 = 32.0;
const TIMELINE_H: f32 = 104.0;
const TAB_H: f32 = 26.0;
const INSPECTOR_BODY_TOP: f32 = 10.0 + TAB_H * 2.0 + 4.0 + 12.0;
const BG: [f32; 4] = [0.052, 0.055, 0.07, 1.0];
const PANEL: [f32; 4] = [0.082, 0.087, 0.108, 1.0];
const PANEL_ALT: [f32; 4] = [0.115, 0.12, 0.15, 1.0];
const PANEL_DISABLED: [f32; 4] = [0.075, 0.078, 0.09, 1.0];
const BORDER: [f32; 4] = [0.24, 0.25, 0.31, 0.9];
const ACCENT: [f32; 4] = [0.38, 0.31, 0.88, 1.0];
const ACCENT_SOFT: [f32; 4] = [0.16, 0.14, 0.30, 1.0];
const DANGER: [f32; 4] = [0.38, 0.10, 0.14, 1.0];
const CAMERA_COLOR: [f32; 4] = [1.0, 0.55, 0.12, 1.0];
const VOICE_COLOR: [f32; 4] = [0.25, 0.78, 0.52, 1.0];
const SFX_COLOR: [f32; 4] = [1.0, 0.62, 0.2, 1.0];
const MUSIC_COLOR: [f32; 4] = [0.35, 0.55, 0.95, 1.0];
const PLAYHEAD_COLOR: [f32; 4] = [0.95, 0.38, 0.55, 1.0];
const TEXT: [u8; 3] = [232, 234, 242];
const MUTED: [u8; 3] = [151, 155, 172];
const VERTEX_EDITOR_HEADER_H: f32 = 52.0;
const VERTEX_EDITOR_TIMELINE_H: f32 = 112.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct ComicDubsLayout {
    pub content: Rect,
    pub sidebar: Rect,
    pub inspector: Rect,
    pub header: Rect,
    pub toolbar: Rect,
    pub tools: Rect,
    pub canvas: Rect,
    pub timeline: Rect,
}

impl ComicDubsLayout {
    pub fn compute(content: Rect) -> Self {
        let sidebar_w = SIDEBAR_W.min((content.width * 0.34).max(210.0));
        let sidebar = Rect {
            x: content.x,
            y: content.y,
            width: sidebar_w,
            height: content.height,
        };
        let inspector_w = INSPECTOR_W.min((content.width * 0.3).max(220.0));
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
            x: main.x,
            y: main.y,
            width: main.width,
            height: HEADER_H,
        };
        let toolbar = Rect {
            y: header.y + header.height,
            height: TOOLBAR_H,
            ..header
        };
        let body_top = toolbar.y + toolbar.height + 10.0;
        let timeline_h = if main.height > 460.0 {
            TIMELINE_H
        } else {
            72.0
        };
        let timeline = Rect {
            x: main.x + 10.0,
            y: (main.y + main.height - timeline_h - 8.0).max(body_top),
            width: (main.width - 20.0).max(0.0),
            height: timeline_h,
        };
        let tools = Rect {
            x: main.x + 8.0,
            y: body_top,
            width: TOOLS_W,
            height: (timeline.y - 10.0 - body_top).max(0.0),
        };
        let canvas_x = tools.x + TOOLS_W + 8.0;
        let canvas = Rect {
            x: canvas_x,
            y: body_top,
            width: (main.x + main.width - 10.0 - canvas_x).max(0.0),
            height: tools.height,
        };
        Self {
            content,
            sidebar,
            inspector,
            header,
            toolbar,
            tools,
            canvas,
            timeline,
        }
    }

    fn image_tab(self) -> Rect {
        Rect {
            x: self.sidebar.x + 10.0,
            y: self.sidebar.y + 10.0,
            width: (self.sidebar.width - 24.0) * 0.5,
            height: 32.0,
        }
    }

    fn audio_tab(self) -> Rect {
        let image = self.image_tab();
        Rect {
            x: image.x + image.width + 4.0,
            ..image
        }
    }

    fn previous(self) -> Rect {
        header_button(self.header, 10.0, 56.0)
    }

    fn next(self) -> Rect {
        header_button(self.header, 70.0, 56.0)
    }

    fn tool_button(self, index: usize) -> Rect {
        Rect {
            x: self.tools.x + (TOOLS_W - TOOL_BUTTON) * 0.5,
            y: self.tools.y + 6.0 + index as f32 * (TOOL_BUTTON + 6.0),
            width: TOOL_BUTTON,
            height: TOOL_BUTTON,
        }
    }

    fn timeline_track(self) -> Rect {
        Rect {
            x: self.timeline.x + 12.0,
            y: self.timeline.y + 20.0,
            width: (self.timeline.width - 24.0).max(1.0),
            height: (self.timeline.height - 26.0).max(1.0),
        }
    }

    fn inspector_tab(self, tab: Tab) -> Rect {
        let index = Tab::ALL.iter().position(|value| *value == tab).unwrap_or(0);
        let (row, column, columns) = if index < 4 {
            (0, index, 4)
        } else {
            (1, index - 4, 3)
        };
        let gap = 4.0;
        let width = (self.inspector.width - 24.0 - gap * (columns - 1) as f32) / columns as f32;
        Rect {
            x: self.inspector.x + 12.0 + column as f32 * (width + gap),
            y: self.inspector.y + 10.0 + row as f32 * (TAB_H + 4.0),
            width,
            height: TAB_H,
        }
    }

    fn inspector_body(self) -> Rect {
        Rect {
            x: self.inspector.x,
            y: self.inspector.y + INSPECTOR_BODY_TOP,
            width: self.inspector.width,
            height: (self.inspector.height - INSPECTOR_BODY_TOP - 8.0).max(0.0),
        }
    }
}

fn header_button(header: Rect, offset: f32, width: f32) -> Rect {
    Rect {
        x: header.x + offset,
        y: header.y + 10.0,
        width,
        height: 32.0,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum MediaTab {
    #[default]
    Images,
    Audios,
}

/// Canvas tools of the studio toolbox.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Polygon,
    Ellipse,
    Rectangle,
    Shout,
    Thought,
    Narration,
    Camera,
}

impl Tool {
    pub const ALL: [Self; 8] = [
        Self::Select,
        Self::Polygon,
        Self::Ellipse,
        Self::Rectangle,
        Self::Shout,
        Self::Thought,
        Self::Narration,
        Self::Camera,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Sélection",
            Self::Polygon => "Polygone libre",
            Self::Ellipse => "Bulle ronde",
            Self::Rectangle => "Bulle rectangulaire",
            Self::Shout => "Bulle de cri",
            Self::Thought => "Bulle de pensée",
            Self::Narration => "Cartouche de narration",
            Self::Camera => "Cadrage caméra",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Select => "Cliquez une bulle • glissez pour la déplacer • Maj+clic : ajouter/retirer un sommet",
            Self::Polygon => "Cliquez pour poser les sommets • cliquez le premier pour fermer",
            Self::Camera => "Tracez la zone que la caméra doit cadrer pour la bulle sélectionnée",
            _ => "Glissez sur la page pour tracer la bulle • un clic crée une bulle standard",
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

/// Inspector tabs; the first five edit the selected bubble.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Text,
    Style,
    Anim,
    Camera,
    Sound,
    Page,
    Project,
}

impl Tab {
    const ALL: [Self; 7] = [
        Self::Text,
        Self::Style,
        Self::Anim,
        Self::Camera,
        Self::Sound,
        Self::Page,
        Self::Project,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Text => "Texte",
            Self::Style => "Style",
            Self::Anim => "Anim",
            Self::Camera => "Caméra",
            Self::Sound => "Son",
            Self::Page => "Page",
            Self::Project => "Projet",
        }
    }

    fn is_bubble(self) -> bool {
        !matches!(self, Self::Page | Self::Project)
    }
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
    pub overlay_quads: Vec<QuadInstance>,
    pub labels: Vec<SceneLabel>,
    pub overlay_labels: Vec<SceneLabel>,
    /// Drawn above every label (screen flashes).
    pub top_quads: Vec<QuadInstance>,
    pub controls: Vec<SceneControl>,
    pub page_rect: Option<Rect>,
    pub page_id: Option<PageId>,
    pub page_layers: Vec<PageLayer>,
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
            height: VERTEX_EDITOR_HEADER_H,
            ..layout.content
        };
        let timeline_panel = Rect {
            y: layout.content.y + layout.content.height - VERTEX_EDITOR_TIMELINE_H,
            height: VERTEX_EDITOR_TIMELINE_H,
            ..layout.content
        };
        let close = Rect {
            x: header.x + header.width - 44.0,
            y: header.y + 10.0,
            width: 32.0,
            height: 32.0,
        };
        let stage = Rect {
            x: layout.content.x + 20.0,
            y: header.y + header.height + 12.0,
            width: (layout.content.width - 40.0).max(0.0),
            height: (timeline_panel.y - header.y - header.height - 24.0).max(0.0),
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
}

#[derive(Debug, Clone)]
enum ItemKind {
    Section(String),
    Info(String),
    Button {
        text: String,
        selected: bool,
        danger: bool,
        command: Command,
    },
    Stepper {
        name: String,
        value: String,
        minus: Command,
        plus: Command,
    },
    Choice {
        name: String,
        value: String,
        previous: Command,
        next: Command,
    },
    Swatch {
        name: String,
        color: Option<[u8; 4]>,
        command: Command,
    },
    Toggle {
        text: String,
        on: bool,
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
struct ItemBuilder {
    x: f32,
    width: f32,
    y: f32,
    items: Vec<Item>,
}

impl ItemBuilder {
    fn new(x: f32, width: f32, y: f32) -> Self {
        Self {
            x,
            width,
            y,
            items: Vec::new(),
        }
    }

    fn push(&mut self, key: &str, height: f32, kind: ItemKind) {
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
        self.y += height + 6.0;
    }

    fn section(&mut self, key: &str, text: &str) {
        self.y += 4.0;
        self.push(key, 18.0, ItemKind::Section(text.to_uppercase()));
    }

    fn info(&mut self, key: &str, text: impl Into<String>) {
        let text = text.into();
        let lines = info_lines(&text, self.width).len() as f32;
        self.push(key, INFO_LINE_H * lines + 4.0, ItemKind::Info(text));
    }

    fn stepper(
        &mut self,
        key: &str,
        name: &str,
        value: impl Into<String>,
        minus: Command,
        plus: Command,
    ) {
        self.push(
            key,
            40.0,
            ItemKind::Stepper {
                name: name.into(),
                value: value.into(),
                minus,
                plus,
            },
        );
    }

    fn choice(
        &mut self,
        key: &str,
        name: &str,
        value: impl Into<String>,
        previous: Command,
        next: Command,
    ) {
        self.push(
            key,
            40.0,
            ItemKind::Choice {
                name: name.into(),
                value: value.into(),
                previous,
                next,
            },
        );
    }

    fn button(&mut self, key: &str, text: impl Into<String>, command: Command, danger: bool) {
        self.push(
            key,
            32.0,
            ItemKind::Button {
                text: text.into(),
                selected: false,
                danger,
                command,
            },
        );
    }

    fn buttons(&mut self, key: &str, buttons: Vec<(String, bool, Command)>) {
        let count = buttons.len().max(1) as f32;
        let gap = 6.0;
        let width = (self.width - gap * (count - 1.0)) / count;
        for (index, (text, selected, command)) in buttons.into_iter().enumerate() {
            self.items.push(Item {
                id: format!("comic.inspector.{key}.{index}"),
                rect: Rect {
                    x: self.x + index as f32 * (width + gap),
                    y: self.y,
                    width,
                    height: 32.0,
                },
                kind: ItemKind::Button {
                    text,
                    selected,
                    danger: false,
                    command,
                },
            });
        }
        self.y += 32.0 + 6.0;
    }

    fn toggle(&mut self, key: &str, text: &str, on: bool, command: Command) {
        self.push(
            key,
            30.0,
            ItemKind::Toggle {
                text: text.into(),
                on,
                command,
            },
        );
    }

    fn swatches(&mut self, key: &str, swatches: Vec<(String, Option<[u8; 4]>, Command)>) {
        let count = swatches.len().max(1) as f32;
        let gap = 8.0;
        let width = (self.width - gap * (count - 1.0)) / count;
        for (index, (name, color, command)) in swatches.into_iter().enumerate() {
            self.items.push(Item {
                id: format!("comic.inspector.{key}.{index}"),
                rect: Rect {
                    x: self.x + index as f32 * (width + gap),
                    y: self.y + 16.0,
                    width,
                    height: 30.0,
                },
                kind: ItemKind::Swatch {
                    name,
                    color,
                    command,
                },
            });
        }
        self.y += 16.0 + 30.0 + 6.0;
    }
}

#[derive(Default)]
pub struct ComicDubsWorkspaceUi {
    media_tab: MediaTab,
    media_scroll: usize,
    selected_bubble: Option<BubbleId>,
    draft: Vec<Point>,
    text_edit: Option<(BubbleId, String)>,
    dragging_audio: Option<ComicAudioId>,
    drag_position: (f32, f32),
    bubble_drag: Option<BubbleDrag>,
    bubble_vertex_drag: Option<BubbleVertexDrag>,
    draft_vertex_drag: Option<DraftVertexDrag>,
    vertex_editor: Option<VertexEditor>,
    color_target: Option<ColorTarget>,
    color_picker: ColorPickerState,
    pending_audio_imports: usize,
    tool: Tool,
    shape_drag: Option<ShapeDrag>,
    inspector_tab: Tab,
    inspector_scroll: f32,
    preview_ms: Option<u64>,
    playing: bool,
    scrubbing: bool,
    frame_aspect: Option<f32>,
    recording: Option<(BubbleId, f32)>,
    arrow_nudge: bool,
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
        self.selected_bubble
    }

    pub fn select_bubble(&mut self, bubble_id: Option<BubbleId>) {
        self.selected_bubble = bubble_id;
        self.text_edit = None;
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
        self.selected_bubble = Some(bubble_id);
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
            self.media_tab = MediaTab::Audios;
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
        }
    }

    /// Aspect ratio (width / height) of the exported video frame.
    pub fn set_frame_aspect(&mut self, aspect: Option<f32>) {
        self.frame_aspect = aspect.filter(|aspect| aspect.is_finite() && *aspect > 0.0);
    }

    pub fn set_recording(&mut self, recording: Option<(BubbleId, f32)>) {
        self.recording = recording;
    }

    /// Whether arrow keys move the selected bubble (canvas keyboard focus).
    pub fn set_arrow_nudge(&mut self, enabled: bool) {
        self.arrow_nudge = enabled;
    }

    pub fn drop_accepts(&self, layout: ComicDubsLayout, x: f32, y: f32) -> bool {
        layout.sidebar.contains(x, y)
    }

    pub fn begin_text_edit(&mut self, bubble_id: BubbleId, text: String) {
        self.selected_bubble = Some(bubble_id);
        self.text_edit = Some((bubble_id, text));
    }

    /// Escape: closes the innermost transient state. Returns whether
    /// something was cancelled.
    pub fn cancel_draft(&mut self) -> bool {
        if self.close_vertex_editor() {
            return true;
        }
        if !self.draft.is_empty() || self.shape_drag.is_some() {
            self.draft.clear();
            self.draft_vertex_drag = None;
            self.shape_drag = None;
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
        self.selected_bubble.take().is_some()
    }

    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.draft.clear();
        self.draft_vertex_drag = None;
        self.shape_drag = None;
        self.text_edit = None;
        if tool == Tool::Camera {
            self.inspector_tab = Tab::Camera;
            self.inspector_scroll = 0.0;
        }
    }

    pub fn control_action(&mut self, id: &str, project: &ComicDubsProject) -> Option<UiAction> {
        if id == "comic.vertex.close" {
            return Some(UiAction::ComicDubsCloseVertexEditor);
        }
        if id == "comic.vertex.play" {
            return Some(UiAction::ComicDubsToggleVertexEditorPreview);
        }
        if let Some(editor) = self.vertex_editor.as_ref() {
            let bubble = project.bubble(editor.bubble_id)?;
            if id == "comic.vertex.add" {
                return Some(UiAction::ComicDubsSetBubbleVertexKeyframe {
                    bubble_id: bubble.id,
                    at_ms: editor.playhead_ms,
                    points: bubble.points_at(editor.playhead_ms).to_vec(),
                });
            }
            if id == "comic.vertex.delete" {
                return editor.selected_keyframe.map(|at_ms| {
                    UiAction::ComicDubsRemoveBubbleVertexKeyframe {
                        bubble_id: bubble.id,
                        at_ms,
                    }
                });
            }
            if id == "comic.vertex.previous" {
                return Some(UiAction::ComicDubsSetVertexEditorPlayhead(
                    previous_keyframe_at(bubble, editor.playhead_ms),
                ));
            }
            if id == "comic.vertex.next" {
                return Some(UiAction::ComicDubsSetVertexEditorPlayhead(
                    next_keyframe_at(
                        bubble,
                        editor.playhead_ms,
                        vertex_editor_duration_ms(project, bubble),
                    ),
                ));
            }
            if let Some(at_ms) = id
                .strip_prefix("comic.vertex.marker.")
                .and_then(|value| value.parse().ok())
            {
                return Some(UiAction::ComicDubsSetVertexEditorPlayhead(at_ms));
            }
        }
        if let Some(id) = id
            .strip_prefix("comic.page.")
            .and_then(|id| id.parse().ok())
        {
            return Some(UiAction::ComicDubsSelectPage(id));
        }
        if let Some(id) = id
            .strip_prefix("comic.bubble.")
            .or_else(|| id.strip_prefix("comic.canvas.bubble."))
            .and_then(|id| id.parse().ok())
        {
            project.bubble(id)?;
            self.selected_bubble = Some(id);
            return None;
        }
        if let Some(audio_id) = id
            .strip_prefix("comic.audio.")
            .and_then(|id| id.parse().ok())
        {
            if let Some(bubble_id) = self.selected_bubble {
                return Some(UiAction::ComicDubsAssignAudio {
                    bubble_id,
                    audio_id: Some(audio_id),
                });
            }
        }
        if let Some(index) = id
            .strip_prefix("comic.tool.")
            .and_then(|index| index.parse::<usize>().ok())
        {
            if let Some(tool) = Tool::ALL.get(index) {
                self.set_tool(*tool);
            }
            return None;
        }
        if let Some(index) = id
            .strip_prefix("comic.tab.")
            .and_then(|index| index.parse::<usize>().ok())
        {
            if let Some(tab) = Tab::ALL.get(index) {
                self.inspector_tab = *tab;
                self.inspector_scroll = 0.0;
            }
            return None;
        }
        if let Some(rest) = id.strip_prefix("comic.timeline.cue.") {
            let (page_index, bubble_index) = rest.split_once('.')?;
            let (page_index, bubble_index) = (
                page_index.parse::<usize>().ok()?,
                bubble_index.parse::<usize>().ok()?,
            );
            let plan = Timeline::build(project, None, 40);
            let cue = plan.cue_for(page_index, bubble_index)?;
            self.selected_bubble = project
                .pages()
                .get(page_index)
                .and_then(|page| page.bubbles.get(bubble_index))
                .map(|bubble| bubble.id);
            return Some(UiAction::ComicDubsSeek(cue.reveal_ms));
        }
        if id.starts_with("comic.inspector.") {
            let layout = self.last_layout;
            let items = self.inspector_items(project, layout);
            for item in &items {
                let (command, anchor) = match &item.kind {
                    ItemKind::Button { command, .. }
                    | ItemKind::Toggle { command, .. }
                    | ItemKind::Swatch { command, .. }
                        if item.id == id =>
                    {
                        (command.clone(), item.rect)
                    }
                    ItemKind::Stepper { minus, plus, .. } => {
                        if id == format!("{}.minus", item.id) {
                            (minus.clone(), item.rect)
                        } else if id == format!("{}.plus", item.id) {
                            (plus.clone(), item.rect)
                        } else {
                            continue;
                        }
                    }
                    ItemKind::Choice { previous, next, .. } => {
                        if id == format!("{}.previous", item.id) {
                            (previous.clone(), item.rect)
                        } else if id == format!("{}.next", item.id) {
                            (next.clone(), item.rect)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                return match self.run_command(command, project, layout, anchor) {
                    EventResponse::Action(action) => Some(action),
                    _ => None,
                };
            }
        }
        None
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
        if self
            .selected_bubble
            .is_some_and(|id| project.bubble(id).is_none())
        {
            self.selected_bubble = None;
            self.text_edit = None;
        }
        let count = match self.media_tab {
            MediaTab::Images => project.pages().len(),
            MediaTab::Audios => project.audios().len(),
        };
        self.media_scroll = self
            .media_scroll
            .min(count.saturating_sub(visible_media_rows(layout)));
    }

    fn effective_tab(&self) -> Tab {
        if self.inspector_tab.is_bubble() && self.selected_bubble.is_none() {
            Tab::Page
        } else {
            self.inspector_tab
        }
    }

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
        if let Some(response) = self.handle_text_edit(event) {
            return response;
        }

        let page = project.active_page();
        let page_rect = page.map(|page| image_rect(layout.canvas, page));
        let previewing = self.preview_ms.is_some();

        if let (UiEvent::ContextMenu { x, y }, Some(page), Some(rect), false) =
            (event, page, page_rect, previewing)
        {
            let Some(bubble_id) = bubble_at(page, rect, *x, *y) else {
                return EventResponse::Ignored;
            };
            self.selected_bubble = Some(bubble_id);
            return project
                .bubble(bubble_id)
                .and_then(|bubble| bubble.audio_id)
                .map_or(EventResponse::Consumed, |audio_id| {
                    EventResponse::Action(UiAction::ComicDubsPlayAudio(audio_id))
                });
        }

        if matches!(event, UiEvent::KeyInput { text } if text == "\x1b")
            && (!self.draft.is_empty() || self.shape_drag.is_some())
        {
            self.cancel_draft();
            return EventResponse::Consumed;
        }

        if let Some(response) = self.handle_tools(event, layout) {
            return response;
        }
        if let Some(response) = self.handle_timeline(event, project, layout) {
            return response;
        }

        if previewing {
            if let UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } = event {
                if layout.canvas.contains(*x, *y) {
                    if !self.playing {
                        self.preview_ms = None;
                    }
                    return EventResponse::Consumed;
                }
            }
        }

        if let (UiEvent::CtrlClick { x, y }, Some(_page), Some(rect)) = (event, page, page_rect) {
            if rect.contains(*x, *y) && !previewing {
                self.draft.clear();
                self.draft.push(point_at(rect, *x, *y));
                self.selected_bubble = None;
                return EventResponse::Consumed;
            }
        }

        if let (UiEvent::MousePress { x, y }, Some(_page), Some(rect)) = (event, page, page_rect) {
            if rect.contains(*x, *y) && !previewing {
                if !self.draft.is_empty() {
                    let point = point_at(rect, *x, *y);
                    if let Some(index) = vertex_at(rect, &self.draft, *x, *y) {
                        self.draft_vertex_drag = Some(DraftVertexDrag {
                            index,
                            original: self.draft[index],
                            moved: false,
                        });
                        return EventResponse::Consumed;
                    }
                    if self.draft.len() < 128 {
                        self.draft.push(point);
                    }
                    return EventResponse::Consumed;
                }
                if self.tool == Tool::Polygon {
                    self.draft.push(point_at(rect, *x, *y));
                    self.selected_bubble = None;
                    return EventResponse::Consumed;
                }
                if self.tool.shape().is_some() || self.tool == Tool::Camera {
                    let point = point_at(rect, *x, *y);
                    self.shape_drag = Some(ShapeDrag {
                        tool: self.tool,
                        start: point,
                        current: point,
                    });
                    return EventResponse::Consumed;
                }
            }
        }

        if let UiEvent::MousePress { x, y } = event {
            if layout.image_tab().contains(*x, *y) {
                self.media_tab = MediaTab::Images;
                self.media_scroll = 0;
                return EventResponse::Consumed;
            }
            if layout.audio_tab().contains(*x, *y) {
                self.media_tab = MediaTab::Audios;
                self.media_scroll = 0;
                return EventResponse::Consumed;
            }
        }

        if let Some(response) = self.handle_header(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_inspector(event, project, layout) {
            return response;
        }
        if let Some(response) = self.handle_media(event, project, layout) {
            return response;
        }

        if let UiEvent::MouseMove { x, y } = event {
            if self.dragging_audio.is_some() {
                self.drag_position = (*x, *y);
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.shape_drag.as_mut(), page_rect) {
                drag.current = point_at(rect, *x, *y);
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.draft_vertex_drag.as_mut(), page_rect) {
                let point = point_at(rect, *x, *y);
                drag.moved |= (point.x - drag.original.x).abs() > 0.001
                    || (point.y - drag.original.y).abs() > 0.001;
                self.draft[drag.index] = point;
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.bubble_vertex_drag.as_mut(), page_rect) {
                drag.points[drag.index] = point_at(rect, *x, *y);
                return EventResponse::Consumed;
            }
            if let (Some(drag), Some(rect)) = (self.bubble_drag.as_mut(), page_rect) {
                let pointer = point_at(rect, *x, *y);
                let min_x = drag
                    .original
                    .iter()
                    .map(|point| point.x)
                    .fold(1.0, f32::min);
                let max_x = drag
                    .original
                    .iter()
                    .map(|point| point.x)
                    .fold(0.0, f32::max);
                let min_y = drag
                    .original
                    .iter()
                    .map(|point| point.y)
                    .fold(1.0, f32::min);
                let max_y = drag
                    .original
                    .iter()
                    .map(|point| point.y)
                    .fold(0.0, f32::max);
                drag.delta = Point {
                    x: (pointer.x - drag.anchor.x).clamp(-min_x, 1.0 - max_x),
                    y: (pointer.y - drag.anchor.y).clamp(-min_y, 1.0 - max_y),
                };
                return EventResponse::Consumed;
            }
        }
        if let UiEvent::MouseRelease { x, y } = event {
            if let Some(audio_id) = self.dragging_audio.take() {
                if let Some(response) = self.drop_audio_on_inspector(audio_id, project, *x, *y) {
                    return response;
                }
                let bubble_id = page
                    .zip(page_rect)
                    .and_then(|(page, rect)| bubble_at(page, rect, *x, *y));
                return bubble_id.map_or(EventResponse::Consumed, |bubble_id| {
                    EventResponse::Action(UiAction::ComicDubsAssignAudio {
                        bubble_id,
                        audio_id: Some(audio_id),
                    })
                });
            }
            if let Some(drag) = self.shape_drag.take() {
                return self.finish_shape_drag(drag, project, page);
            }
            if let Some(drag) = self.draft_vertex_drag.take() {
                if drag.index == 0 && !drag.moved && self.draft.len() >= 3 {
                    let points = std::mem::take(&mut self.draft);
                    return EventResponse::Action(UiAction::ComicDubsAddBubble {
                        page_id: page.unwrap().id,
                        points,
                    });
                }
                return EventResponse::Consumed;
            }
            if let Some(drag) = self.bubble_vertex_drag.take() {
                return if drag.points == drag.original {
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
                };
            }
            if let Some(drag) = self.bubble_drag.take() {
                let points = translated_drag_points(&drag);
                return if points == drag.original {
                    EventResponse::Consumed
                } else {
                    EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                        bubble_id: drag.bubble_id,
                        points,
                    })
                };
            }
        }

        if let (UiEvent::DoubleClick { x, y }, Some(page), Some(rect)) = (event, page, page_rect) {
            if let Some(id) = bubble_at(page, rect, *x, *y) {
                self.bubble_drag = None;
                let text = project.bubble(id).unwrap().text.clone();
                self.begin_text_edit(id, text);
                return EventResponse::Consumed;
            }
        }
        if let (UiEvent::ShiftMousePress { x, y }, Some(_page), Some(rect)) =
            (event, page, page_rect)
        {
            if let Some(bubble) = self.selected_bubble.and_then(|id| project.bubble(id)) {
                if let Some(index) = vertex_at(rect, &bubble.points, *x, *y) {
                    return EventResponse::Action(UiAction::ComicDubsRemoveBubbleVertex {
                        bubble_id: bubble.id,
                        index,
                    });
                }
                if let Some((after, point)) = edge_at(rect, &bubble.points, *x, *y) {
                    return EventResponse::Action(UiAction::ComicDubsInsertBubbleVertex {
                        bubble_id: bubble.id,
                        after,
                        point,
                    });
                }
            }
        }
        if let (
            UiEvent::MousePress { x, y } | UiEvent::ShiftMousePress { x, y },
            Some(page),
            Some(rect),
        ) = (event, page, page_rect)
        {
            if rect.contains(*x, *y) {
                if let Some(bubble_id) = self.selected_bubble {
                    let bubble = project.bubble(bubble_id).unwrap();
                    if let Some(index) = vertex_at(rect, &bubble.points, *x, *y) {
                        self.bubble_vertex_drag = Some(BubbleVertexDrag {
                            bubble_id,
                            index,
                            keyframe_at_ms: None,
                            original: bubble.points.clone(),
                            points: bubble.points.clone(),
                        });
                        return EventResponse::Consumed;
                    }
                }
                let hit = bubble_at(page, rect, *x, *y);
                self.selected_bubble = hit;
                self.bubble_drag = self.selected_bubble.and_then(|bubble_id| {
                    project.bubble(bubble_id).map(|bubble| BubbleDrag {
                        bubble_id,
                        anchor: point_at(rect, *x, *y),
                        original: bubble.points.clone(),
                        delta: Point { x: 0.0, y: 0.0 },
                    })
                });
                return EventResponse::Consumed;
            }
        }
        if matches!(event, UiEvent::Delete) {
            if let Some(id) = self.selected_bubble.take() {
                return EventResponse::Action(UiAction::ComicDubsRemoveBubble(id));
            }
        }
        if let Some(bubble_id) = self.selected_bubble.filter(|_| self.arrow_nudge) {
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
            if layout.sidebar.contains(*x, *y) {
                let count = match self.media_tab {
                    MediaTab::Images => project.pages().len(),
                    MediaTab::Audios => project.audios().len(),
                };
                self.media_scroll = scroll_rows(
                    self.media_scroll,
                    *delta,
                    count.saturating_sub(visible_media_rows(layout)),
                );
                return EventResponse::Consumed;
            }
            if layout.inspector.contains(*x, *y) {
                let (_, content_height) = self.inspector_content(project, layout);
                let visible = layout.inspector_body().height;
                let step = if *delta > 0.0 { -48.0 } else { 48.0 };
                self.inspector_scroll =
                    (self.inspector_scroll + step).clamp(0.0, (content_height - visible).max(0.0));
                return EventResponse::Consumed;
            }
        }
        if matches!(event, UiEvent::MousePress { .. }) {
            self.selected_bubble = None;
            self.text_edit = None;
            self.bubble_drag = None;
        }
        EventResponse::Ignored
    }

    fn finish_shape_drag(
        &mut self,
        drag: ShapeDrag,
        project: &ComicDubsProject,
        page: Option<&Page>,
    ) -> EventResponse {
        let Some(page) = page else {
            return EventResponse::Consumed;
        };
        let tiny = (drag.current.x - drag.start.x).abs() < 0.01
            && (drag.current.y - drag.start.y).abs() < 0.01;
        if drag.tool == Tool::Camera {
            let Some(bubble_id) = self.selected_bubble else {
                return EventResponse::Consumed;
            };
            let (Some(bubble), Some(region), false) = (
                project.bubble(bubble_id),
                Region::from_corners(drag.start, drag.current),
                tiny,
            ) else {
                return EventResponse::Consumed;
            };
            return EventResponse::Action(UiAction::ComicDubsSetBubbleFx {
                bubble_id,
                fx: BubbleFx {
                    camera: CameraFocus::Region,
                    camera_region: Some(region),
                    ..bubble.fx
                },
            });
        }
        let Some((kind, preset)) = drag.tool.shape() else {
            return EventResponse::Consumed;
        };
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
        comic_dubs_shapes::shape_points(kind, start, end).map_or(
            EventResponse::Consumed,
            |points| {
                EventResponse::Action(UiAction::ComicDubsAddStyledBubble {
                    page_id: page.id,
                    points,
                    preset,
                })
            },
        )
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
        let bubble = self.selected_bubble.and_then(|id| project.bubble(id));
        let action = match (item.id.as_str(), bubble) {
            ("comic.inspector.voice", Some(bubble)) => UiAction::ComicDubsAssignAudio {
                bubble_id: bubble.id,
                audio_id: Some(audio_id),
            },
            ("comic.inspector.sfx", Some(bubble)) => UiAction::ComicDubsSetBubbleSound {
                bubble_id: bubble.id,
                sound: BubbleSound {
                    sfx_audio_id: Some(audio_id),
                    ..bubble.sound
                },
            },
            ("comic.inspector.music", _) => UiAction::ComicDubsSetStudio(StudioSettings {
                music_audio_id: Some(audio_id),
                ..*project.studio()
            }),
            _ => return Some(EventResponse::Consumed),
        };
        Some(EventResponse::Action(action))
    }

    fn handle_tools(&mut self, event: &UiEvent, layout: ComicDubsLayout) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        if !layout.tools.contains(*x, *y) {
            return None;
        }
        if let Some(tool) = Tool::ALL
            .iter()
            .enumerate()
            .find(|(index, _)| layout.tool_button(*index).contains(*x, *y))
            .map(|(_, tool)| *tool)
        {
            self.set_tool(tool);
        }
        Some(EventResponse::Consumed)
    }

    fn handle_timeline(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let track = layout.timeline_track();
        match event {
            UiEvent::MousePress { x, y } if layout.timeline.contains(*x, *y) => {
                let plan = Timeline::build(project, None, 40);
                if plan.is_empty() {
                    return Some(EventResponse::Consumed);
                }
                let at_ms = time_at(&plan, track, *x);
                // Clicking a cue block selects its bubble.
                if let Some((page_index, cue)) = plan
                    .cues()
                    .find(|(_, cue)| {
                        let rect = cue_rect(&plan, track, cue);
                        rect.contains(*x, *y)
                    })
                    .map(|(page_index, cue)| (page_index, cue.clone()))
                {
                    self.selected_bubble = project
                        .pages()
                        .get(page_index)
                        .and_then(|page| page.bubbles.get(cue.bubble_index))
                        .map(|bubble| bubble.id);
                }
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

    fn handle_header(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        let active = project
            .active_page_id()
            .and_then(|id| project.pages().iter().position(|page| page.id == id));
        if layout.previous().contains(*x, *y) {
            let page = active
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| project.pages().get(index));
            return Some(page.map_or(EventResponse::Consumed, |page| {
                EventResponse::Action(UiAction::ComicDubsSelectPage(page.id))
            }));
        }
        if layout.next().contains(*x, *y) {
            let page = active.and_then(|index| project.pages().get(index + 1));
            return Some(page.map_or(EventResponse::Consumed, |page| {
                EventResponse::Action(UiAction::ComicDubsSelectPage(page.id))
            }));
        }
        None
    }

    fn handle_inspector(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        if !layout.inspector.contains(*x, *y) {
            return None;
        }
        if let Some(tab) = Tab::ALL
            .iter()
            .find(|tab| layout.inspector_tab(**tab).contains(*x, *y))
        {
            if !tab.is_bubble() || self.selected_bubble.is_some() {
                self.inspector_tab = *tab;
                self.inspector_scroll = 0.0;
            }
            return Some(EventResponse::Consumed);
        }
        let items = self.visible_inspector_items(project, layout);
        let Some(item) = items.iter().find(|item| item.rect.contains(*x, *y)) else {
            return Some(EventResponse::Consumed);
        };
        let rect = item.rect;
        let command = match &item.kind {
            ItemKind::Button { command, .. }
            | ItemKind::Toggle { command, .. }
            | ItemKind::Swatch { command, .. } => command.clone(),
            ItemKind::Stepper { minus, plus, .. } => {
                if *x < rect.x + 34.0 {
                    minus.clone()
                } else if *x > rect.x + rect.width - 34.0 {
                    plus.clone()
                } else {
                    Command::None
                }
            }
            ItemKind::Choice { previous, next, .. } => {
                if *x < rect.x + 34.0 {
                    previous.clone()
                } else {
                    next.clone()
                }
            }
            ItemKind::Section(_) | ItemKind::Info(_) => Command::None,
        };
        Some(self.run_command(command, project, layout, rect))
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
                if matches!(action, UiAction::ComicDubsRemoveBubble(_)) {
                    self.selected_bubble = None;
                }
                EventResponse::Action(action)
            }
            Command::Local(Local::Tool(tool)) => {
                self.set_tool(tool);
                EventResponse::Consumed
            }
            Command::Local(Local::EditText) => {
                if let Some(bubble) = self.selected_bubble.and_then(|id| project.bubble(id)) {
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

    fn handle_media(
        &mut self,
        event: &UiEvent,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Option<EventResponse> {
        let UiEvent::MousePress { x, y } = event else {
            return None;
        };
        let body_y = layout.sidebar.y + 52.0;
        if *y < body_y || !layout.sidebar.contains(*x, *y) {
            return None;
        }
        let row = ((*y - body_y) / ROW_H).floor().max(0.0) as usize + self.media_scroll;
        let local_x = *x - layout.sidebar.x;
        match self.media_tab {
            MediaTab::Images => {
                let page = project.pages().get(row)?;
                if local_x >= layout.sidebar.width - 34.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsRemovePage(
                        page.id,
                    )));
                }
                if local_x >= layout.sidebar.width - 66.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsMovePage {
                        page_id: page.id,
                        delta: 1,
                    }));
                }
                if local_x >= layout.sidebar.width - 98.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsMovePage {
                        page_id: page.id,
                        delta: -1,
                    }));
                }
                Some(EventResponse::Action(UiAction::ComicDubsSelectPage(
                    page.id,
                )))
            }
            MediaTab::Audios => {
                let audio = project.audios().get(row)?;
                if local_x >= layout.sidebar.width - 34.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsRemoveAudio(
                        audio.id,
                    )));
                }
                if local_x >= layout.sidebar.width - 66.0 {
                    return Some(EventResponse::Action(UiAction::ComicDubsPlayAudio(
                        audio.id,
                    )));
                }
                self.dragging_audio = Some(audio.id);
                self.drag_position = (*x, *y);
                Some(EventResponse::Consumed)
            }
        }
    }

    fn handle_text_edit(&mut self, event: &UiEvent) -> Option<EventResponse> {
        if matches!(
            event,
            UiEvent::MousePress { .. } | UiEvent::DoubleClick { .. }
        ) {
            self.text_edit = None;
            return None;
        }
        let (id, text) = self.text_edit.as_mut()?;
        match event {
            UiEvent::KeyInput { text: input } if input == "\x1b" => {
                self.text_edit = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } if input == "\r" || input == "\n" => {
                self.text_edit = None;
                Some(EventResponse::Consumed)
            }
            UiEvent::KeyInput { text: input } if input == "\x08" || input == "\x7f" => {
                text.pop();
                Some(EventResponse::Action(UiAction::ComicDubsSetBubbleText {
                    bubble_id: *id,
                    text: text.clone(),
                }))
            }
            UiEvent::KeyInput { text: input } => {
                text.extend(
                    input
                        .chars()
                        .filter(|character| !character.is_control())
                        .take(500 - text.chars().count().min(500)),
                );
                Some(EventResponse::Action(UiAction::ComicDubsSetBubbleText {
                    bubble_id: *id,
                    text: text.clone(),
                }))
            }
            _ => Some(EventResponse::Consumed),
        }
    }

    // ---------------------------------------------------------------- inspector

    fn inspector_items(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> Vec<Item> {
        self.inspector_content(project, layout).0
    }

    /// Items positioned with the current scroll, and the content height.
    fn inspector_content(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> (Vec<Item>, f32) {
        let body = layout.inspector_body();
        let mut builder = ItemBuilder::new(
            layout.inspector.x + 12.0,
            (layout.inspector.width - 24.0).max(40.0),
            body.y - self.inspector_scroll,
        );
        let bubble = self.selected_bubble.and_then(|id| project.bubble(id));
        let plan = Timeline::build(project, None, 40);
        match (self.effective_tab(), bubble) {
            (Tab::Text, Some(bubble)) => self.text_items(&mut builder, bubble),
            (Tab::Style, Some(bubble)) => self.style_items(&mut builder, bubble),
            (Tab::Anim, Some(bubble)) => self.anim_items(&mut builder, bubble),
            (Tab::Camera, Some(bubble)) => self.camera_items(&mut builder, bubble),
            (Tab::Sound, Some(bubble)) => self.sound_items(&mut builder, project, bubble, &plan),
            (Tab::Project, _) => self.project_items(&mut builder, project, &plan),
            _ => self.page_items(&mut builder, project, &plan),
        }
        let height = builder.y + self.inspector_scroll - body.y;
        (builder.items, height)
    }

    fn visible_inspector_items(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
    ) -> Vec<Item> {
        let body = layout.inspector_body();
        self.inspector_items(project, layout)
            .into_iter()
            .filter(|item| {
                item.rect.y >= body.y - 1.0
                    && item.rect.y + item.rect.height <= body.y + body.height + 1.0
            })
            .collect()
    }

    fn text_items(&self, builder: &mut ItemBuilder, bubble: &Bubble) {
        let id = bubble.id;
        builder.section("text.section", "Texte de la bulle");
        builder.button(
            "text.edit",
            if bubble.text.trim().is_empty() {
                "Écrire le texte…".to_string()
            } else {
                "Modifier le texte…".to_string()
            },
            Command::Local(Local::EditText),
            false,
        );
        if !bubble.text.trim().is_empty() {
            builder.info(
                "text.preview",
                format!("« {} »", ellipsize(&bubble.text, 110)),
            );
        }
        builder.stepper(
            "text.size",
            "Taille du texte (1080p)",
            format!("{} px", bubble.font_size.round()),
            Command::Action(UiAction::ComicDubsSetBubbleFontSize {
                bubble_id: id,
                font_size: bubble.font_size - 2.0,
            }),
            Command::Action(UiAction::ComicDubsSetBubbleFontSize {
                bubble_id: id,
                font_size: bubble.font_size + 2.0,
            }),
        );
        builder.stepper(
            "text.letter_spacing",
            "Espacement des lettres",
            format!("{:.1} px", bubble.letter_spacing),
            Command::Action(UiAction::ComicDubsSetBubbleLetterSpacing {
                bubble_id: id,
                spacing: bubble.letter_spacing - 0.5,
            }),
            Command::Action(UiAction::ComicDubsSetBubbleLetterSpacing {
                bubble_id: id,
                spacing: bubble.letter_spacing + 0.5,
            }),
        );
        builder.stepper(
            "text.line_spacing",
            "Interligne",
            format!("{:.1}×", bubble.line_spacing),
            Command::Action(UiAction::ComicDubsSetBubbleLineSpacing {
                bubble_id: id,
                spacing: bubble.line_spacing - 0.1,
            }),
            Command::Action(UiAction::ComicDubsSetBubbleLineSpacing {
                bubble_id: id,
                spacing: bubble.line_spacing + 0.1,
            }),
        );
        let style = |bold: bool, strike: bool, underline: bool| {
            Command::Action(UiAction::ComicDubsSetBubbleTextStyle {
                bubble_id: id,
                bold,
                strikethrough: strike,
                underline,
            })
        };
        builder.buttons(
            "text.style",
            vec![
                (
                    "Gras".into(),
                    bubble.bold,
                    style(!bubble.bold, bubble.strikethrough, bubble.underline),
                ),
                (
                    "Italique".into(),
                    bubble.look.italic,
                    Command::Action(UiAction::ComicDubsSetBubbleLook {
                        bubble_id: id,
                        look: BubbleLook {
                            italic: !bubble.look.italic,
                            ..bubble.look
                        },
                    }),
                ),
                (
                    "Barré".into(),
                    bubble.strikethrough,
                    style(bubble.bold, !bubble.strikethrough, bubble.underline),
                ),
                (
                    "Souligné".into(),
                    bubble.underline,
                    style(bubble.bold, bubble.strikethrough, !bubble.underline),
                ),
            ],
        );
        builder.buttons(
            "text.alignment",
            [
                ("Gauche", TextAlignment::Left),
                ("Centre", TextAlignment::Center),
                ("Droite", TextAlignment::Right),
            ]
            .into_iter()
            .map(|(text, alignment)| {
                (
                    text.to_string(),
                    bubble.text_alignment == alignment,
                    Command::Action(UiAction::ComicDubsSetBubbleTextAlignment {
                        bubble_id: id,
                        alignment,
                    }),
                )
            })
            .collect(),
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
                    bubble.look.text_outline_color,
                    Command::Local(Local::Color(ColorTarget::TextOutline(id))),
                ),
            ],
        );
        if bubble.look.text_outline_color.is_some() {
            let look = bubble.look;
            builder.stepper(
                "text.outline_width",
                "Épaisseur du contour du texte",
                format!("{:.1} px", look.text_outline_width),
                Command::Action(UiAction::ComicDubsSetBubbleLook {
                    bubble_id: id,
                    look: BubbleLook {
                        text_outline_width: look.text_outline_width - 0.5,
                        ..look
                    },
                }),
                Command::Action(UiAction::ComicDubsSetBubbleLook {
                    bubble_id: id,
                    look: BubbleLook {
                        text_outline_width: look.text_outline_width + 0.5,
                        ..look
                    },
                }),
            );
            builder.button(
                "text.outline_remove",
                "Retirer le contour du texte",
                Command::Action(UiAction::ComicDubsSetBubbleLook {
                    bubble_id: id,
                    look: BubbleLook {
                        text_outline_color: None,
                        ..look
                    },
                }),
                false,
            );
        }
    }

    fn style_items(&self, builder: &mut ItemBuilder, bubble: &Bubble) {
        let id = bubble.id;
        let look = bubble.look;
        let set_look = |look: BubbleLook| {
            Command::Action(UiAction::ComicDubsSetBubbleLook {
                bubble_id: id,
                look,
            })
        };
        builder.section("style.presets_section", "Préréglages");
        for (row, presets) in BubblePreset::ALL.chunks(3).enumerate() {
            builder.buttons(
                &format!("style.preset{row}"),
                presets
                    .iter()
                    .map(|preset| {
                        (
                            preset.label().to_string(),
                            false,
                            Command::Action(UiAction::ComicDubsApplyPreset {
                                bubble_id: id,
                                preset: *preset,
                            }),
                        )
                    })
                    .collect(),
            );
        }
        builder.section("style.bubble_section", "Bulle");
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
        builder.stepper(
            "style.outline_width",
            "Épaisseur du contour",
            if look.outline_width <= 0.0 {
                "aucun".to_string()
            } else {
                format!("{:.1} px", look.outline_width)
            },
            set_look(BubbleLook {
                outline_width: look.outline_width - 0.5,
                ..look
            }),
            set_look(BubbleLook {
                outline_width: look.outline_width + 0.5,
                ..look
            }),
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
        builder.section("style.shape_section", "Forme");
        builder.buttons(
            "style.shape",
            vec![
                (
                    "Ajouter une queue".into(),
                    false,
                    Command::Action(UiAction::ComicDubsAddBubbleTail(id)),
                ),
                (
                    "Arrondir".into(),
                    false,
                    Command::Action(UiAction::ComicDubsSmoothBubble(id)),
                ),
            ],
        );
        builder.info(
            "style.shape_hint",
            "Maj+clic sur un bord ajoute un sommet, Maj+clic sur un sommet le retire.",
        );
        builder.button(
            "style.vertex_editor",
            "Animer les sommets…",
            Command::Action(UiAction::ComicDubsOpenVertexEditor(id)),
            false,
        );
        builder.section("style.organize_section", "Organiser");
        builder.buttons(
            "style.order",
            vec![
                (
                    "Ordre ↑".into(),
                    false,
                    Command::Action(UiAction::ComicDubsMoveBubble {
                        bubble_id: id,
                        delta: -1,
                    }),
                ),
                (
                    "Ordre ↓".into(),
                    false,
                    Command::Action(UiAction::ComicDubsMoveBubble {
                        bubble_id: id,
                        delta: 1,
                    }),
                ),
                (
                    "Dupliquer".into(),
                    false,
                    Command::Action(UiAction::ComicDubsDuplicateBubble(id)),
                ),
            ],
        );
        builder.buttons(
            "style.clipboard",
            vec![
                (
                    "Copier le style".into(),
                    false,
                    Command::Action(UiAction::ComicDubsCopyStyle(id)),
                ),
                (
                    "Coller le style".into(),
                    false,
                    Command::Action(UiAction::ComicDubsPasteStyle(id)),
                ),
            ],
        );
        builder.button(
            "style.apply_page",
            "Appliquer ce style à toute la page",
            Command::Action(UiAction::ComicDubsApplyStyleToPage(id)),
            false,
        );
        builder.button(
            "style.delete",
            "Supprimer la bulle",
            Command::Action(UiAction::ComicDubsRemoveBubble(id)),
            true,
        );
    }

    fn anim_items(&self, builder: &mut ItemBuilder, bubble: &Bubble) {
        let id = bubble.id;
        let fx = bubble.fx;
        let set =
            |fx: BubbleFx| Command::Action(UiAction::ComicDubsSetBubbleFx { bubble_id: id, fx });
        builder.section("anim.entrance_section", "Apparition");
        builder.choice(
            "anim.entrance",
            "Apparition",
            fx.entrance.label(),
            set(BubbleFx {
                entrance: fx.entrance.cycled(-1),
                ..fx
            }),
            set(BubbleFx {
                entrance: fx.entrance.cycled(1),
                ..fx
            }),
        );
        if fx.entrance != BubbleEntrance::Cut {
            builder.stepper(
                "anim.entrance_ms",
                "Durée de l'apparition",
                format!("{} ms", fx.entrance_ms),
                set(BubbleFx {
                    entrance_ms: fx.entrance_ms.saturating_sub(50),
                    ..fx
                }),
                set(BubbleFx {
                    entrance_ms: fx.entrance_ms + 50,
                    ..fx
                }),
            );
        }
        builder.toggle(
            "anim.whole",
            "Bulle entière, cachée avant son tour",
            fx.whole_bubble,
            set(BubbleFx {
                whole_bubble: !fx.whole_bubble,
                ..fx
            }),
        );
        builder.choice(
            "anim.reveal",
            "Révélation du texte",
            fx.text_reveal.label(),
            set(BubbleFx {
                text_reveal: fx.text_reveal.cycled(-1),
                ..fx
            }),
            set(BubbleFx {
                text_reveal: fx.text_reveal.cycled(1),
                ..fx
            }),
        );
        builder.section("anim.line_section", "Pendant la réplique");
        builder.choice(
            "anim.emphasis",
            "Emphase",
            fx.emphasis.label(),
            set(BubbleFx {
                emphasis: fx.emphasis.cycled(-1),
                ..fx
            }),
            set(BubbleFx {
                emphasis: fx.emphasis.cycled(1),
                ..fx
            }),
        );
        if fx.emphasis != BubbleEmphasis::None {
            builder.stepper(
                "anim.emphasis_strength",
                "Intensité",
                format!("{:.0} %", fx.emphasis_strength * 100.0),
                set(BubbleFx {
                    emphasis_strength: fx.emphasis_strength - 0.25,
                    ..fx
                }),
                set(BubbleFx {
                    emphasis_strength: fx.emphasis_strength + 0.25,
                    ..fx
                }),
            );
        }
        builder.choice(
            "anim.screen",
            "Effet d'écran",
            fx.screen_effect.label(),
            set(BubbleFx {
                screen_effect: fx.screen_effect.cycled(-1),
                ..fx
            }),
            set(BubbleFx {
                screen_effect: fx.screen_effect.cycled(1),
                ..fx
            }),
        );
        if fx.screen_effect != ScreenEffect::None {
            builder.stepper(
                "anim.screen_ms",
                "Durée de l'effet",
                format!("{} ms", fx.screen_effect_ms),
                set(BubbleFx {
                    screen_effect_ms: fx.screen_effect_ms.saturating_sub(50),
                    ..fx
                }),
                set(BubbleFx {
                    screen_effect_ms: fx.screen_effect_ms + 50,
                    ..fx
                }),
            );
        }
        builder.section("anim.after_section", "Après la réplique");
        builder.toggle(
            "anim.exit",
            "Disparaît après sa réplique",
            fx.exit_after,
            set(BubbleFx {
                exit_after: !fx.exit_after,
                ..fx
            }),
        );
        builder.button(
            "anim.preview",
            "▶ Aperçu de la bulle",
            Command::Action(UiAction::ComicDubsPreviewBubble(id)),
            false,
        );
    }

    fn camera_items(&self, builder: &mut ItemBuilder, bubble: &Bubble) {
        let id = bubble.id;
        let fx = bubble.fx;
        let set =
            |fx: BubbleFx| Command::Action(UiAction::ComicDubsSetBubbleFx { bubble_id: id, fx });
        builder.section("camera.section", "Cadrage de la caméra");
        builder.choice(
            "camera.focus",
            "Cadrage",
            fx.camera.label(),
            set(BubbleFx {
                camera: fx.camera.cycled(-1),
                ..fx
            }),
            set(BubbleFx {
                camera: fx.camera.cycled(1),
                ..fx
            }),
        );
        builder.button(
            "camera.draw",
            if self.tool == Tool::Camera {
                "Tracez la zone sur la page…"
            } else {
                "Dessiner la zone à cadrer"
            },
            Command::Local(Local::Tool(Tool::Camera)),
            false,
        );
        builder.info(
            "camera.region",
            match (fx.camera, fx.camera_region) {
                (CameraFocus::Region, Some(region)) => format!(
                    "Zone : {:.0} % × {:.0} % de la page",
                    region.width * 100.0,
                    region.height * 100.0
                ),
                (CameraFocus::Region, None) => {
                    "Aucune zone : tracez un rectangle avec l'outil Caméra.".to_string()
                }
                (CameraFocus::Keep, _) => {
                    "La caméra garde le cadrage de la bulle précédente.".to_string()
                }
                (CameraFocus::FullPage, _) => "La caméra revient sur toute la page.".to_string(),
                (CameraFocus::Bubble, _) => {
                    "La caméra zoome automatiquement sur la bulle.".to_string()
                }
            },
        );
        builder.stepper(
            "camera.duration",
            "Durée du mouvement",
            format!("{} ms", fx.camera_ms),
            set(BubbleFx {
                camera_ms: fx.camera_ms.saturating_sub(100),
                ..fx
            }),
            set(BubbleFx {
                camera_ms: fx.camera_ms + 100,
                ..fx
            }),
        );
        builder.info(
            "camera.hint",
            "Le mouvement démarre au tour de la bulle ; le texte et la voix arrivent une fois la caméra en place.",
        );
        builder.button(
            "camera.preview",
            "▶ Aperçu de la bulle",
            Command::Action(UiAction::ComicDubsPreviewBubble(id)),
            false,
        );
    }

    fn sound_items(
        &self,
        builder: &mut ItemBuilder,
        project: &ComicDubsProject,
        bubble: &Bubble,
        plan: &Timeline,
    ) {
        let id = bubble.id;
        let sound = bubble.sound;
        let set = |sound: BubbleSound| {
            Command::Action(UiAction::ComicDubsSetBubbleSound {
                bubble_id: id,
                sound,
            })
        };
        let audio_name = |audio: Option<ComicAudioId>| {
            audio
                .and_then(|audio| project.audio(audio))
                .map_or("Aucune".to_string(), |audio| audio.file_name.clone())
        };
        builder.section("sound.voice_section", "Voix");
        builder.choice(
            "voice",
            "Voix",
            audio_name(bubble.audio_id),
            Command::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: cycle_audio(project, bubble.audio_id, -1),
            }),
            Command::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: cycle_audio(project, bubble.audio_id, 1),
            }),
        );
        let recording = self.recording.filter(|(bubble_id, _)| *bubble_id == id);
        builder.buttons(
            "sound.voice_actions",
            vec![
                (
                    "▶ Écouter".into(),
                    false,
                    bubble.audio_id.map_or(Command::None, |audio| {
                        Command::Action(UiAction::ComicDubsPlayAudio(audio))
                    }),
                ),
                (
                    match recording {
                        Some((_, seconds)) => format!("■ Arrêter {seconds:.1} s"),
                        None => "● Enregistrer".into(),
                    },
                    recording.is_some(),
                    Command::Action(UiAction::ComicDubsToggleVoiceRecording(id)),
                ),
            ],
        );
        builder.stepper(
            "sound.voice_volume",
            "Volume de la voix",
            format!("{:.0} %", sound.voice_volume * 100.0),
            set(BubbleSound {
                voice_volume: sound.voice_volume - 0.1,
                ..sound
            }),
            set(BubbleSound {
                voice_volume: sound.voice_volume + 0.1,
                ..sound
            }),
        );
        builder.stepper(
            "sound.delay",
            "Délai avant la voix",
            format!("{} ms", sound.audio_delay_ms),
            set(BubbleSound {
                audio_delay_ms: sound.audio_delay_ms.saturating_sub(100),
                ..sound
            }),
            set(BubbleSound {
                audio_delay_ms: sound.audio_delay_ms + 100,
                ..sound
            }),
        );
        builder.section("sound.sfx_section", "Effet sonore à l'apparition");
        builder.choice(
            "sfx",
            "Effet sonore",
            audio_name(sound.sfx_audio_id),
            set(BubbleSound {
                sfx_audio_id: cycle_audio(project, sound.sfx_audio_id, -1),
                ..sound
            }),
            set(BubbleSound {
                sfx_audio_id: cycle_audio(project, sound.sfx_audio_id, 1),
                ..sound
            }),
        );
        if sound.sfx_audio_id.is_some() {
            builder.stepper(
                "sound.sfx_volume",
                "Volume de l'effet",
                format!("{:.0} %", sound.sfx_volume * 100.0),
                set(BubbleSound {
                    sfx_volume: sound.sfx_volume - 0.1,
                    ..sound
                }),
                set(BubbleSound {
                    sfx_volume: sound.sfx_volume + 0.1,
                    ..sound
                }),
            );
        }
        builder.section("sound.timing_section", "Rythme");
        builder.stepper(
            "sound.hold",
            "Maintien après la réplique",
            format!("{} ms", sound.extra_hold_ms),
            set(BubbleSound {
                extra_hold_ms: sound.extra_hold_ms.saturating_sub(250),
                ..sound
            }),
            set(BubbleSound {
                extra_hold_ms: sound.extra_hold_ms + 250,
                ..sound
            }),
        );
        let located = locate_bubble(project, id);
        if let Some(cue) = located.and_then(|(page, index)| plan.cue_for(page, index)) {
            builder.info(
                "sound.duration",
                format!(
                    "Sur la timeline : apparaît à {}, occupe {}.",
                    format_time_ms(cue.reveal_ms),
                    format_seconds(cue.end_ms - cue.start_ms)
                ),
            );
        }
        builder.info(
            "sound.drop_hint",
            "Astuce : glissez un audio de la médiathèque sur « Voix » ou « Effet sonore ».",
        );
    }

    fn page_items(&self, builder: &mut ItemBuilder, project: &ComicDubsProject, plan: &Timeline) {
        let Some((index, page)) = project.active_page_id().and_then(|id| {
            project
                .pages()
                .iter()
                .enumerate()
                .find(|(_, page)| page.id == id)
        }) else {
            builder.section("page.empty_section", "Page");
            builder.info(
                "page.empty",
                "Importez des images de planches pour commencer votre Comic Dub.",
            );
            builder.button(
                "page.import",
                "Importer des images…",
                Command::Action(UiAction::ComicDubsImportImages),
                false,
            );
            return;
        };
        if self.inspector_tab.is_bubble() && self.selected_bubble.is_none() {
            builder.info(
                "page.select_hint",
                "Sélectionnez une bulle pour régler son texte, son style, ses animations, sa caméra et son son.",
            );
        }
        let fx = page.fx;
        let id = page.id;
        let set = |fx: PageFx| Command::Action(UiAction::ComicDubsSetPageFx { page_id: id, fx });
        builder.section(
            "page.section",
            &format!("Page {}/{}", index + 1, project.pages().len()),
        );
        builder.info(
            "page.info",
            format!(
                "{} • {}×{} • {} bulle(s)",
                ellipsize(&page.file_name, 40),
                page.width,
                page.height,
                page.bubbles.len()
            ),
        );
        builder.choice(
            "page.transition",
            "Transition d'entrée",
            fx.transition.label(),
            set(PageFx {
                transition: fx.transition.cycled(-1),
                ..fx
            }),
            set(PageFx {
                transition: fx.transition.cycled(1),
                ..fx
            }),
        );
        if fx.transition != PageTransition::Cut {
            builder.stepper(
                "page.transition_ms",
                "Durée de la transition",
                format!("{} ms", fx.transition_ms),
                set(PageFx {
                    transition_ms: fx.transition_ms.saturating_sub(100),
                    ..fx
                }),
                set(PageFx {
                    transition_ms: fx.transition_ms + 100,
                    ..fx
                }),
            );
        }
        builder.stepper(
            "page.intro",
            "Pause avant la 1re bulle",
            format!("{} ms", fx.intro_ms),
            set(PageFx {
                intro_ms: fx.intro_ms.saturating_sub(250),
                ..fx
            }),
            set(PageFx {
                intro_ms: fx.intro_ms + 250,
                ..fx
            }),
        );
        builder.choice(
            "page.motion",
            "Mouvement de caméra",
            fx.motion.label(),
            set(PageFx {
                motion: fx.motion.cycled(-1),
                ..fx
            }),
            set(PageFx {
                motion: fx.motion.cycled(1),
                ..fx
            }),
        );
        if fx.motion != PageMotion::None {
            builder.stepper(
                "page.motion_strength",
                "Amplitude du mouvement",
                format!("{:.0} %", fx.motion_strength * 100.0),
                set(PageFx {
                    motion_strength: fx.motion_strength - 0.25,
                    ..fx
                }),
                set(PageFx {
                    motion_strength: fx.motion_strength + 0.25,
                    ..fx
                }),
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
                    "▶ Lire depuis cette page",
                    Command::Action(UiAction::ComicDubsPlayFrom(span.start_ms)),
                    false,
                );
            }
            None => builder.info(
                "page.timing",
                "Cette page n'a pas de bulle : elle est ignorée à la lecture.",
            ),
        }
    }

    fn project_items(
        &self,
        builder: &mut ItemBuilder,
        project: &ComicDubsProject,
        plan: &Timeline,
    ) {
        let studio = *project.studio();
        let set = |studio: StudioSettings| Command::Action(UiAction::ComicDubsSetStudio(studio));
        builder.section("project.music_section", "Musique de fond");
        builder.choice(
            "music",
            "Musique",
            studio
                .music_audio_id
                .and_then(|id| project.audio(id))
                .map_or("Aucune".to_string(), |audio| audio.file_name.clone()),
            set(StudioSettings {
                music_audio_id: cycle_audio(project, studio.music_audio_id, -1),
                ..studio
            }),
            set(StudioSettings {
                music_audio_id: cycle_audio(project, studio.music_audio_id, 1),
                ..studio
            }),
        );
        builder.stepper(
            "project.music_volume",
            "Volume de la musique",
            format!("{:.0} %", studio.music_volume * 100.0),
            set(StudioSettings {
                music_volume: studio.music_volume - 0.05,
                ..studio
            }),
            set(StudioSettings {
                music_volume: studio.music_volume + 0.05,
                ..studio
            }),
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
        builder.stepper(
            "project.music_fade",
            "Fondu de fin",
            format!("{} ms", studio.music_fade_out_ms),
            set(StudioSettings {
                music_fade_out_ms: studio.music_fade_out_ms.saturating_sub(250),
                ..studio
            }),
            set(StudioSettings {
                music_fade_out_ms: studio.music_fade_out_ms + 250,
                ..studio
            }),
        );
        builder.section("project.render_section", "Rendu");
        let [r, g, b] = studio.background;
        builder.swatches(
            "project.colors",
            vec![(
                "Couleur de fond".into(),
                Some([r, g, b, 255]),
                Command::Local(Local::Color(ColorTarget::Background)),
            )],
        );
        builder.stepper(
            "project.typewriter",
            "Vitesse machine à écrire",
            format!("{:.0} car./s", studio.typewriter_cps),
            set(StudioSettings {
                typewriter_cps: studio.typewriter_cps - 2.0,
                ..studio
            }),
            set(StudioSettings {
                typewriter_cps: studio.typewriter_cps + 2.0,
                ..studio
            }),
        );
        builder.section("project.stats_section", "Bilan");
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
        builder.info(
            "project.stats",
            format!(
                "Durée totale {} • {} page(s) • {} bulle(s) • {} avec voix",
                format_time_ms(plan.total_ms),
                project.pages().len(),
                bubbles.len(),
                voiced
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
        builder.section("project.script_section", "Script et sous-titres");
        builder.buttons(
            "project.script",
            vec![
                (
                    "Importer…".into(),
                    false,
                    Command::Action(UiAction::ComicDubsImportScript),
                ),
                (
                    "Exporter…".into(),
                    false,
                    Command::Action(UiAction::ComicDubsExportScript),
                ),
            ],
        );
        builder.button(
            "project.srt",
            "Exporter les sous-titres (SRT)…",
            Command::Action(UiAction::ComicDubsExportSrt),
            false,
        );
        builder.info(
            "project.script_hint",
            "Le script liste une ligne par bulle dans l'ordre de lecture, idéal pour les traducteurs et les comédiens.",
        );
    }

    // -------------------------------------------------------------------- scene

    pub fn scene(&self, project: &ComicDubsProject, layout: ComicDubsLayout) -> ComicDubsScene {
        if self.vertex_editor.is_some() {
            return self.vertex_editor_scene(project, layout);
        }
        let mut scene = ComicDubsScene::default();
        let plan = Timeline::build(project, None, 40);
        scene.quads.push(quad(layout.content, BG, [0.0; 4], 0.0));
        scene.quads.push(quad(layout.sidebar, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.inspector, PANEL, BORDER, 0.0));
        scene.quads.push(quad(layout.header, PANEL, BORDER, 0.0));
        self.render_media(project, layout, &mut scene);
        self.render_header(project, layout, &plan, &mut scene);
        self.render_tools(layout, &mut scene);
        self.render_inspector(project, layout, &mut scene);
        self.render_timeline(project, layout, &plan, &mut scene);
        match self.preview_ms.filter(|_| !plan.is_empty()) {
            Some(at_ms) => self.render_preview(project, layout, &plan, at_ms, &mut scene),
            None => self.render_editor_canvas(project, layout, &mut scene),
        }
        if let Some((_, seconds)) = self.recording {
            let badge = Rect {
                x: layout.canvas.x + layout.canvas.width * 0.5 - 130.0,
                y: layout.canvas.y + 8.0,
                width: 260.0,
                height: 30.0,
            };
            scene.overlay_quads.push(quad(
                badge,
                [0.55, 0.06, 0.1, 0.94],
                [1.0, 0.4, 0.45, 1.0],
                15.0,
            ));
            overlay_label(
                &mut scene,
                &format!("● Enregistrement de la voix {seconds:.1} s"),
                badge,
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
            let drag = Rect {
                x: self.drag_position.0 + 10.0,
                y: self.drag_position.1 + 10.0,
                width: 190.0,
                height: 30.0,
            };
            scene
                .overlay_quads
                .push(quad(drag, [0.15, 0.13, 0.28, 0.96], ACCENT, 6.0));
            overlay_label(&mut scene, name, drag, HAlign::Center, 12.0, TEXT);
        }
        scene
    }

    fn render_editor_canvas(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        let Some(page) = project.active_page() else {
            label(
                scene,
                "Importez ou déposez des images pour commencer votre Comic Dub",
                layout.canvas,
                HAlign::Center,
                20.0,
                MUTED,
            );
            return;
        };
        let rect = image_rect(layout.canvas, page);
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
        let placement = Placement {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        };
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
                .filter(|(id, _)| *id == bubble.id)
                .map(|(_, text)| format!("{text}|"));
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
                    selected: self.selected_bubble == Some(bubble.id),
                    edit: true,
                    text: text.as_deref(),
                    points: Some(&points),
                },
            );
            render_reading_order_badge(
                scene,
                rect,
                &points,
                index + 1,
                bubble.audio_id.is_some(),
                bubble.sound.sfx_audio_id.is_some(),
                !bubble.fx.is_default(),
            );
        }
        if let Some(bubble) = self.selected_bubble.and_then(|id| project.bubble(id)) {
            if matches!(self.inspector_tab, Tab::Camera) || self.tool == Tool::Camera {
                if let Some(CameraTarget::Region(region)) = timeline::camera_target(bubble) {
                    draw_region(scene, rect, region, "CAMÉRA", layout.canvas);
                }
            }
        }
        if let Some(drag) = self.shape_drag {
            let region = Region::from_corners(drag.start, drag.current);
            if drag.tool == Tool::Camera {
                if let Some(region) = region {
                    draw_region(scene, rect, region, "NOUVEAU CADRAGE", layout.canvas);
                }
            } else if let Some((kind, _)) = drag.tool.shape() {
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
            label(
                scene,
                "Cliquez pour ajouter un sommet • cliquez le premier point pour fermer • Échap pour annuler",
                Rect {
                    x: rect.x,
                    y: rect.y - 28.0,
                    width: rect.width,
                    height: 24.0,
                },
                HAlign::Center,
                12.0,
                TEXT,
            );
        }
    }

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
            width: 150.0,
            height: 24.0,
        };
        scene
            .overlay_quads
            .push(quad(badge, [0.06, 0.06, 0.09, 0.78], [0.0; 4], 12.0));
        overlay_label(
            scene,
            &format!(
                "{} {}",
                if self.playing {
                    "▶ LECTURE"
                } else {
                    "APERÇU"
                },
                format_time_ms(at_ms)
            ),
            badge,
            HAlign::Center,
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
            for (x, y) in &screen {
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
            "Interpolation instantanée • glissez un sommet • Entrée ajoute • Espace lit • Ctrl+←/→ 50 ms • Échap ferme",
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
        label(
            &mut scene,
            "×",
            editor_layout.close,
            HAlign::Center,
            20.0,
            TEXT,
        );
        scene.controls.push(SceneControl {
            id: "comic.vertex.close".into(),
            label: "Fermer l’éditeur de sommets".into(),
            bounds: editor_layout.close,
            role: AccessibleRole::Button,
            selected: false,
        });

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

        for (bounds, text, id, selected) in [
            (editor_layout.previous, "|◀", "comic.vertex.previous", false),
            (
                editor_layout.play,
                if editor.playing.is_some() {
                    "Pause"
                } else {
                    "Lire"
                },
                "comic.vertex.play",
                editor.playing.is_some(),
            ),
            (editor_layout.next, "▶|", "comic.vertex.next", false),
            (
                editor_layout.add,
                if editor.selected_keyframe == Some(editor.playhead_ms) {
                    "Mettre à jour"
                } else {
                    "Ajouter une pose"
                },
                "comic.vertex.add",
                false,
            ),
            (
                editor_layout.delete,
                "Supprimer",
                "comic.vertex.delete",
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
                    PANEL_DISABLED
                },
                BORDER,
                5.0,
            ));
            label(
                &mut scene,
                text,
                bounds,
                HAlign::Center,
                11.0,
                if enabled { TEXT } else { MUTED },
            );
            scene.controls.push(SceneControl {
                id: id.into(),
                label: text.into(),
                bounds,
                role: AccessibleRole::Button,
                selected,
            });
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
            scene.controls.push(SceneControl {
                id: format!("comic.vertex.marker.{}", keyframe.at_ms),
                label: format!("Pose à {}", format_time_ms(keyframe.at_ms)),
                bounds: marker,
                role: AccessibleRole::Button,
                selected,
            });
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

    fn render_media(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        for (rect, text, active) in [
            (
                layout.image_tab(),
                "Images",
                self.media_tab == MediaTab::Images,
            ),
            (
                layout.audio_tab(),
                "Audios",
                self.media_tab == MediaTab::Audios,
            ),
        ] {
            scene.quads.push(quad(
                rect,
                if active { ACCENT } else { PANEL_ALT },
                BORDER,
                6.0,
            ));
            label(scene, text, rect, HAlign::Center, 13.0, TEXT);
        }
        match self.media_tab {
            MediaTab::Images => {
                for (row, page) in project
                    .pages()
                    .iter()
                    .skip(self.media_scroll)
                    .take(visible_media_rows(layout))
                    .enumerate()
                {
                    let rect = media_row(layout, row);
                    let selected = project.active_page_id() == Some(page.id);
                    scene.quads.push(quad(
                        rect,
                        if selected { ACCENT_SOFT } else { PANEL_ALT },
                        if selected { ACCENT } else { [0.0; 4] },
                        5.0,
                    ));
                    label(
                        scene,
                        &format!("{}  {}×{}", page.file_name, page.width, page.height),
                        Rect {
                            width: rect.width - 102.0,
                            height: 24.0,
                            ..rect
                        },
                        HAlign::Left,
                        12.0,
                        TEXT,
                    );
                    let mut details = format!("{} bulle(s)", page.bubbles.len());
                    if page.fx.transition != PageTransition::Cut {
                        details.push_str(&format!(" • {}", page.fx.transition.label()));
                    }
                    label(
                        scene,
                        &details,
                        Rect {
                            y: rect.y + 22.0,
                            height: 18.0,
                            width: rect.width - 102.0,
                            ..rect
                        },
                        HAlign::Left,
                        10.0,
                        MUTED,
                    );
                    label(
                        scene,
                        "↑  ↓  ×",
                        Rect {
                            x: rect.x + rect.width - 96.0,
                            width: 92.0,
                            ..rect
                        },
                        HAlign::Center,
                        15.0,
                        MUTED,
                    );
                    scene.controls.push(SceneControl {
                        id: format!("comic.page.{}", page.id),
                        label: page.file_name.clone(),
                        bounds: rect,
                        role: AccessibleRole::Button,
                        selected,
                    });
                }
            }
            MediaTab::Audios => {
                for (row, audio) in project
                    .audios()
                    .iter()
                    .skip(self.media_scroll)
                    .take(visible_media_rows(layout))
                    .enumerate()
                {
                    let rect = media_row(layout, row);
                    let music = project.studio().music_audio_id == Some(audio.id);
                    scene.quads.push(quad(
                        rect,
                        if music {
                            [0.1, 0.14, 0.26, 1.0]
                        } else {
                            PANEL_ALT
                        },
                        [0.0; 4],
                        5.0,
                    ));
                    label(
                        scene,
                        &audio.file_name,
                        Rect {
                            width: rect.width - 70.0,
                            height: 24.0,
                            ..rect
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
                    let mut details = format!("{:.1} s", audio.duration_ms() as f64 / 1_000.0);
                    if uses > 0 {
                        details.push_str(&format!(" • {uses} bulle(s)"));
                    }
                    if music {
                        details.push_str(" • musique de fond");
                    }
                    label(
                        scene,
                        &details,
                        Rect {
                            y: rect.y + 22.0,
                            height: 18.0,
                            width: rect.width - 70.0,
                            ..rect
                        },
                        HAlign::Left,
                        10.0,
                        MUTED,
                    );
                    label(
                        scene,
                        "▶",
                        Rect {
                            x: rect.x + rect.width - 66.0,
                            width: 30.0,
                            ..rect
                        },
                        HAlign::Center,
                        13.0,
                        MUTED,
                    );
                    label(
                        scene,
                        "×",
                        Rect {
                            x: rect.x + rect.width - 34.0,
                            width: 30.0,
                            ..rect
                        },
                        HAlign::Center,
                        18.0,
                        MUTED,
                    );
                    scene.controls.push(SceneControl {
                        id: format!("comic.audio.{}", audio.id),
                        label: format!("{}; glisser sur une bulle", audio.file_name),
                        bounds: rect,
                        role: AccessibleRole::Button,
                        selected: false,
                    });
                }
            }
        }
        if self.media_tab == MediaTab::Audios && self.pending_audio_imports > 0 {
            let status = Rect {
                x: layout.sidebar.x + 10.0,
                y: layout.sidebar.y + layout.sidebar.height - 82.0,
                width: layout.sidebar.width - 20.0,
                height: 30.0,
            };
            scene.quads.push(quad(status, ACCENT_SOFT, ACCENT, 6.0));
            label(
                scene,
                &format!("Chargement de {} audio(s)…", self.pending_audio_imports),
                status,
                HAlign::Center,
                11.0,
                TEXT,
            );
        }
        let hint = match self.media_tab {
            MediaTab::Images => "Déposez des images ici • conversion PNG automatique",
            MediaTab::Audios => {
                "Déposez des audios ici • glissez-les sur une bulle, « Voix », « Effet sonore » ou « Musique »"
            }
        };
        label(
            scene,
            hint,
            Rect {
                x: layout.sidebar.x + 10.0,
                y: layout.sidebar.y + layout.sidebar.height - 48.0,
                width: layout.sidebar.width - 20.0,
                height: 40.0,
            },
            HAlign::Center,
            10.0,
            MUTED,
        );
    }

    fn render_header(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        plan: &Timeline,
        scene: &mut ComicDubsScene,
    ) {
        for (rect, text) in [(layout.previous(), "←"), (layout.next(), "→")] {
            scene.quads.push(quad(rect, PANEL_ALT, BORDER, 6.0));
            label(scene, text, rect, HAlign::Center, 16.0, TEXT);
        }
        let active = project
            .active_page_id()
            .and_then(|id| project.pages().iter().position(|page| page.id == id))
            .map(|index| index + 1)
            .unwrap_or(0);
        label(
            scene,
            &format!("Page {active}/{}", project.pages().len()),
            Rect {
                x: layout.header.x + 136.0,
                width: 100.0,
                ..layout.previous()
            },
            HAlign::Left,
            13.0,
            TEXT,
        );
        let time_width = 150.0;
        let hint = if self.playing {
            "Lecture en cours • Espace pour arrêter".to_string()
        } else if self.preview_ms.is_some() {
            "Aperçu du rendu • Échap ou clic sur la page pour revenir à l'édition".to_string()
        } else {
            format!("{} — {}", self.tool.label(), self.tool.hint())
        };
        label(
            scene,
            &hint,
            Rect {
                x: layout.header.x + 236.0,
                width: (layout.header.width - 236.0 - time_width - 8.0).max(0.0),
                ..layout.previous()
            },
            HAlign::Left,
            10.0,
            MUTED,
        );
        label(
            scene,
            &match self.preview_ms {
                Some(at_ms) => format!(
                    "{} / {}",
                    format_time_ms(at_ms),
                    format_time_ms(plan.total_ms)
                ),
                None => format!("Durée {}", format_time_ms(plan.total_ms)),
            },
            Rect {
                x: layout.header.x + layout.header.width - time_width - 8.0,
                width: time_width,
                ..layout.previous()
            },
            HAlign::Right,
            12.0,
            TEXT,
        );
    }

    fn render_tools(&self, layout: ComicDubsLayout, scene: &mut ComicDubsScene) {
        if layout.tools.height <= 0.0 {
            return;
        }
        scene.quads.push(quad(layout.tools, PANEL, BORDER, 8.0));
        for (index, tool) in Tool::ALL.iter().enumerate() {
            let rect = layout.tool_button(index);
            if rect.y + rect.height > layout.tools.y + layout.tools.height {
                break;
            }
            let active = self.tool == *tool;
            scene.quads.push(quad(
                rect,
                if active { ACCENT } else { PANEL_ALT },
                BORDER,
                6.0,
            ));
            draw_tool_icon(scene, *tool, rect);
            scene.controls.push(SceneControl {
                id: format!("comic.tool.{index}"),
                label: format!("Outil {}", tool.label()),
                bounds: rect,
                role: AccessibleRole::Button,
                selected: active,
            });
        }
    }

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
        scene.quads.push(quad(strip, PANEL, BORDER, 8.0));
        let track = layout.timeline_track();
        if plan.is_empty() {
            label(
                scene,
                "TIMELINE • ajoutez des bulles pour construire la lecture",
                strip,
                HAlign::Center,
                11.0,
                MUTED,
            );
            return;
        }
        let total = plan.total_ms.max(1);
        let x_at = |at_ms: u64| track.x + at_ms.min(total) as f32 / total as f32 * track.width;
        // Ruler.
        let step = [
            1_000, 2_000, 5_000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000,
        ]
        .into_iter()
        .find(|step| (total / step) as f32 * 64.0 <= track.width)
        .unwrap_or(600_000);
        let mut tick = 0;
        while tick <= total {
            let x = x_at(tick);
            scene.quads.push(quad(
                Rect {
                    x,
                    y: strip.y + 14.0,
                    width: 1.0,
                    height: 5.0,
                },
                BORDER,
                [0.0; 4],
                0.0,
            ));
            label(
                scene,
                &format_short_time(tick),
                Rect {
                    x: x - 30.0,
                    y: strip.y + 1.0,
                    width: 60.0,
                    height: 14.0,
                },
                HAlign::Center,
                9.0,
                MUTED,
            );
            tick += step;
        }
        let compact = track.height < 60.0;
        let page_row = Rect {
            height: if compact { 14.0 } else { 18.0 },
            ..track
        };
        let cue_row = Rect {
            y: page_row.y + page_row.height + 4.0,
            height: if compact { 20.0 } else { 28.0 },
            ..track
        };
        let music_row = Rect {
            y: cue_row.y + cue_row.height + 4.0,
            height: if compact { 6.0 } else { 12.0 },
            ..track
        };
        let active_page = project.active_page_id();
        for span in &plan.pages {
            let Some(page) = project.pages().get(span.page_index) else {
                continue;
            };
            let rect = Rect {
                x: x_at(span.start_ms) + 1.0,
                width: (x_at(span.end_ms) - x_at(span.start_ms) - 2.0).max(1.0),
                ..page_row
            };
            let active = active_page == Some(page.id);
            scene.quads.push(quad(
                rect,
                if active { ACCENT_SOFT } else { PANEL_ALT },
                if active { ACCENT } else { BORDER },
                4.0,
            ));
            if span.transition_ms > 0 {
                scene.quads.push(quad(
                    Rect {
                        width: (x_at(span.start_ms + span.transition_ms) - x_at(span.start_ms))
                            .max(2.0),
                        ..rect
                    },
                    [0.55, 0.45, 1.0, 0.45],
                    [0.0; 4],
                    4.0,
                ));
            }
            if rect.width > 24.0 {
                label(
                    scene,
                    &format!("P{}", span.page_index + 1),
                    rect,
                    HAlign::Left,
                    9.0,
                    TEXT,
                );
            }
        }
        for (page_index, cue) in plan.cues() {
            let Some(bubble) = project
                .pages()
                .get(page_index)
                .and_then(|page| page.bubbles.get(cue.bubble_index))
            else {
                continue;
            };
            let rect = Rect {
                y: cue_row.y,
                height: cue_row.height,
                ..cue_rect(plan, track, cue)
            };
            let selected = self.selected_bubble == Some(bubble.id);
            let speaking = Rect {
                width: (x_at(cue.speak_end_ms) - rect.x).clamp(1.0, rect.width),
                ..rect
            };
            scene.quads.push(quad(
                rect,
                [0.1, 0.105, 0.13, 1.0],
                if selected { ACCENT } else { BORDER },
                3.0,
            ));
            scene.quads.push(quad(
                speaking,
                if bubble.text.trim().is_empty() {
                    [0.2, 0.2, 0.24, 1.0]
                } else {
                    [0.22, 0.2, 0.42, 1.0]
                },
                [0.0; 4],
                3.0,
            ));
            if cue.voice_ms > 0 {
                scene.quads.push(quad(
                    Rect {
                        x: x_at(cue.voice_start_ms),
                        y: rect.y + rect.height - 5.0,
                        width: (x_at(cue.voice_start_ms + cue.voice_ms) - x_at(cue.voice_start_ms))
                            .max(1.0),
                        height: 3.0,
                    },
                    VOICE_COLOR,
                    [0.0; 4],
                    1.5,
                ));
            }
            if cue.sfx_audio.is_some() {
                scene.quads.push(quad(
                    Rect {
                        x: rect.x + 1.0,
                        y: rect.y + 2.0,
                        width: 5.0,
                        height: 5.0,
                    },
                    SFX_COLOR,
                    [0.0; 4],
                    2.5,
                ));
            }
            if cue.reveal_ms > cue.start_ms {
                scene.quads.push(quad(
                    Rect {
                        x: rect.x,
                        width: (x_at(cue.reveal_ms) - rect.x).max(1.0),
                        y: rect.y,
                        height: 3.0,
                    },
                    CAMERA_COLOR,
                    [0.0; 4],
                    1.0,
                ));
            }
            if rect.width > 16.0 {
                label(
                    scene,
                    &(cue.bubble_index + 1).to_string(),
                    Rect {
                        height: rect.height - 4.0,
                        ..rect
                    },
                    HAlign::Center,
                    9.0,
                    TEXT,
                );
            }
            scene.controls.push(SceneControl {
                id: format!("comic.timeline.cue.{page_index}.{}", cue.bubble_index),
                label: format!(
                    "Page {}, bulle {} à {} : {}",
                    page_index + 1,
                    cue.bubble_index + 1,
                    format_time_ms(cue.reveal_ms),
                    if bubble.text.trim().is_empty() {
                        "sans texte".to_string()
                    } else {
                        ellipsize(&bubble.text, 60)
                    }
                ),
                bounds: rect,
                role: AccessibleRole::Button,
                selected,
            });
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
                ..music_row
            };
            scene
                .quads
                .push(quad(bar, [0.12, 0.2, 0.38, 1.0], MUSIC_COLOR, 3.0));
            if project.studio().music_ducking {
                for (start, end) in plan.voice_intervals() {
                    scene.quads.push(quad(
                        Rect {
                            x: x_at(start),
                            width: (x_at(end) - x_at(start)).max(1.0),
                            y: bar.y + bar.height * 0.5,
                            height: bar.height * 0.5,
                        },
                        [0.06, 0.08, 0.14, 1.0],
                        [0.0; 4],
                        0.0,
                    ));
                }
            }
            if !compact {
                label(
                    scene,
                    &format!("♪ {}", music.file_name),
                    Rect {
                        height: bar.height + 2.0,
                        y: bar.y - 1.0,
                        ..bar
                    },
                    HAlign::Left,
                    8.0,
                    TEXT,
                );
            }
        }
        if let Some(at_ms) = self.preview_ms {
            let x = x_at(at_ms);
            scene.overlay_quads.push(quad(
                Rect {
                    x: x - 1.0,
                    y: strip.y + 12.0,
                    width: 2.0,
                    height: strip.height - 16.0,
                },
                PLAYHEAD_COLOR,
                [0.0; 4],
                1.0,
            ));
            scene.overlay_quads.push(quad(
                Rect {
                    x: x - 5.0,
                    y: strip.y + 10.0,
                    width: 10.0,
                    height: 8.0,
                },
                PLAYHEAD_COLOR,
                [0.0; 4],
                3.0,
            ));
        }
    }

    fn render_inspector(
        &self,
        project: &ComicDubsProject,
        layout: ComicDubsLayout,
        scene: &mut ComicDubsScene,
    ) {
        let effective = self.effective_tab();
        for (index, tab) in Tab::ALL.iter().enumerate() {
            let rect = layout.inspector_tab(*tab);
            let enabled = !tab.is_bubble() || self.selected_bubble.is_some();
            let active = effective == *tab;
            scene.quads.push(quad(
                rect,
                if active {
                    ACCENT
                } else if enabled {
                    PANEL_ALT
                } else {
                    PANEL_DISABLED
                },
                BORDER,
                5.0,
            ));
            label(
                scene,
                tab.label(),
                rect,
                HAlign::Center,
                10.0,
                if enabled { TEXT } else { MUTED },
            );
            scene.controls.push(SceneControl {
                id: format!("comic.tab.{index}"),
                label: format!("Onglet {}", tab.label()),
                bounds: rect,
                role: AccessibleRole::Tab,
                selected: active,
            });
        }
        let body = layout.inspector_body();
        let (items, content_height) = self.inspector_content(project, layout);
        for item in items.iter().filter(|item| {
            item.rect.y >= body.y - 1.0
                && item.rect.y + item.rect.height <= body.y + body.height + 1.0
        }) {
            render_item(scene, item);
        }
        if content_height > body.height + 1.0 {
            let ratio = body.height / content_height;
            let thumb_h = (body.height * ratio).max(24.0);
            let max_scroll = (content_height - body.height).max(1.0);
            let thumb_y = body.y + (body.height - thumb_h) * (self.inspector_scroll / max_scroll);
            scene.quads.push(quad(
                Rect {
                    x: layout.inspector.x + layout.inspector.width - 6.0,
                    y: thumb_y,
                    width: 3.0,
                    height: thumb_h,
                },
                [0.4, 0.42, 0.5, 0.7],
                [0.0; 4],
                1.5,
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

fn render_item(scene: &mut ComicDubsScene, item: &Item) {
    let rect = item.rect;
    match &item.kind {
        ItemKind::Section(text) => {
            label(scene, text, rect, HAlign::Left, 10.0, MUTED);
            scene.quads.push(quad(
                Rect {
                    y: rect.y + rect.height - 1.0,
                    height: 1.0,
                    ..rect
                },
                BORDER,
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
                    font_size: 10.0,
                    color: MUTED,
                    font_family: None,
                    padding: 2.0,
                    letter_spacing: 0.0,
                    style: None,
                });
            }
        }
        ItemKind::Button {
            text,
            selected,
            danger,
            command,
        } => {
            let enabled = *command != Command::None || *selected;
            scene.quads.push(quad(
                rect,
                if *danger {
                    DANGER
                } else if *selected {
                    ACCENT
                } else if enabled {
                    PANEL_ALT
                } else {
                    PANEL_DISABLED
                },
                BORDER,
                6.0,
            ));
            // Shrink long labels so they fit their button.
            let fitted = (rect.width - 10.0) / (text.chars().count().max(1) as f32 * 0.56);
            label(
                scene,
                text,
                rect,
                HAlign::Center,
                fitted.clamp(8.0, 10.5),
                if enabled { TEXT } else { MUTED },
            );
            scene.controls.push(SceneControl {
                id: item.id.clone(),
                label: text.clone(),
                bounds: rect,
                role: AccessibleRole::Button,
                selected: *selected,
            });
        }
        ItemKind::Toggle { text, on, .. } => {
            let check = Rect {
                x: rect.x,
                y: rect.y + (rect.height - 18.0) * 0.5,
                width: 18.0,
                height: 18.0,
            };
            scene.quads.push(quad(
                check,
                if *on { ACCENT } else { PANEL_ALT },
                BORDER,
                4.0,
            ));
            if *on {
                label(scene, "✓", check, HAlign::Center, 12.0, TEXT);
            }
            label(
                scene,
                text,
                Rect {
                    x: rect.x + 24.0,
                    width: rect.width - 24.0,
                    ..rect
                },
                HAlign::Left,
                10.5,
                TEXT,
            );
            scene.controls.push(SceneControl {
                id: item.id.clone(),
                label: format!("{text}, {}", if *on { "activé" } else { "désactivé" }),
                bounds: rect,
                role: AccessibleRole::Checkbox,
                selected: *on,
            });
        }
        ItemKind::Swatch { name, color, .. } => {
            label(
                scene,
                name,
                Rect {
                    y: rect.y - 16.0,
                    height: 15.0,
                    ..rect
                },
                HAlign::Center,
                9.5,
                MUTED,
            );
            match color {
                Some(color) if color[3] != 0 => {
                    scene.quads.push(quad(rect, gpu_rgba(*color), BORDER, 5.0));
                }
                _ => {
                    scene.quads.push(quad(rect, PANEL_ALT, BORDER, 5.0));
                    label(
                        scene,
                        if color.is_some() {
                            "Transparent"
                        } else {
                            "Aucun"
                        },
                        rect,
                        HAlign::Center,
                        9.5,
                        MUTED,
                    );
                }
            }
            scene.controls.push(SceneControl {
                id: item.id.clone(),
                label: format!(
                    "{name}, {}",
                    color.filter(|color| color[3] != 0).map_or(
                        "aucune couleur".to_string(),
                        |color| format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
                    )
                ),
                bounds: rect,
                role: AccessibleRole::Button,
                selected: false,
            });
        }
        ItemKind::Stepper { name, value, .. } | ItemKind::Choice { name, value, .. } => {
            let choice = matches!(item.kind, ItemKind::Choice { .. });
            let minus = Rect {
                width: 34.0,
                ..rect
            };
            let plus = Rect {
                x: rect.x + rect.width - 34.0,
                width: 34.0,
                ..rect
            };
            if choice {
                scene
                    .quads
                    .push(quad(rect, [0.1, 0.105, 0.13, 1.0], BORDER, 6.0));
            }
            for (button, text) in [
                (minus, if choice { "◀" } else { "−" }),
                (plus, if choice { "▶" } else { "+" }),
            ] {
                scene.quads.push(quad(button, PANEL_ALT, BORDER, 5.0));
                label(
                    scene,
                    text,
                    button,
                    HAlign::Center,
                    if choice { 11.0 } else { 15.0 },
                    TEXT,
                );
            }
            let center = Rect {
                x: rect.x + 38.0,
                width: rect.width - 76.0,
                height: rect.height * 0.5,
                ..rect
            };
            label(scene, name, center, HAlign::Center, 9.5, MUTED);
            label(
                scene,
                value,
                Rect {
                    y: center.y + center.height - 2.0,
                    ..center
                },
                HAlign::Center,
                11.0,
                TEXT,
            );
            let (first, second) = if choice {
                (("previous", "précédent"), ("next", "suivant"))
            } else {
                (("minus", "diminuer"), ("plus", "augmenter"))
            };
            for ((suffix, verb), bounds) in [(first, minus), (second, plus)] {
                scene.controls.push(SceneControl {
                    id: format!("{}.{suffix}", item.id),
                    label: format!("{name} : {value}, {verb}"),
                    bounds,
                    role: AccessibleRole::Button,
                    selected: false,
                });
            }
        }
    }
}

const INFO_LINE_H: f32 = 15.0;

/// Word-wraps inspector help text; UI labels never wrap on their own.
fn info_lines(text: &str, width: f32) -> Vec<String> {
    text_layout::wrap_text(text, ((width - 4.0) / 5.4).floor().max(8.0) as usize)
}

fn draw_tool_icon(scene: &mut ComicDubsScene, tool: Tool, button: Rect) {
    let icon = Rect {
        x: button.x + 7.0,
        y: button.y + 7.0,
        width: button.width - 14.0,
        height: button.height - 14.0,
    };
    let white = [0.92, 0.93, 0.98, 1.0];
    let outline = |scene: &mut ComicDubsScene, points: &[Point]| {
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            scene.quads.push(line_quad(icon, *a, *b, white, 1.4));
        }
    };
    let corner =
        |a: (f32, f32), b: (f32, f32)| (Point { x: a.0, y: a.1 }, Point { x: b.0, y: b.1 });
    match tool {
        Tool::Select => {
            let arrow = [
                Point { x: 0.2, y: 0.05 },
                Point { x: 0.2, y: 0.85 },
                Point { x: 0.42, y: 0.66 },
                Point { x: 0.58, y: 0.98 },
                Point { x: 0.72, y: 0.9 },
                Point { x: 0.56, y: 0.6 },
                Point { x: 0.85, y: 0.58 },
            ];
            scene.quads.extend(polygon_fill_quads(icon, &arrow, white));
        }
        Tool::Polygon => {
            let points = [
                Point { x: 0.1, y: 0.35 },
                Point { x: 0.55, y: 0.05 },
                Point { x: 0.95, y: 0.45 },
                Point { x: 0.6, y: 0.95 },
                Point { x: 0.15, y: 0.8 },
            ];
            outline(scene, &points);
            for point in points {
                let (x, y) = screen_point(icon, point);
                scene.quads.push(quad(
                    Rect {
                        x: x - 2.0,
                        y: y - 2.0,
                        width: 4.0,
                        height: 4.0,
                    },
                    white,
                    [0.0; 4],
                    2.0,
                ));
            }
        }
        Tool::Camera => {
            for (a, b) in [
                corner((0.0, 0.0), (0.35, 0.0)),
                corner((0.0, 0.0), (0.0, 0.35)),
                corner((1.0, 0.0), (0.65, 0.0)),
                corner((1.0, 0.0), (1.0, 0.35)),
                corner((0.0, 1.0), (0.35, 1.0)),
                corner((0.0, 1.0), (0.0, 0.65)),
                corner((1.0, 1.0), (0.65, 1.0)),
                corner((1.0, 1.0), (1.0, 0.65)),
            ] {
                scene.quads.push(line_quad(icon, a, b, CAMERA_COLOR, 1.8));
            }
            let (x, y) = screen_point(icon, Point { x: 0.5, y: 0.5 });
            scene.quads.push(quad(
                Rect {
                    x: x - 3.0,
                    y: y - 3.0,
                    width: 6.0,
                    height: 6.0,
                },
                CAMERA_COLOR,
                [0.0; 4],
                3.0,
            ));
        }
        Tool::Shout => {
            let star = (0..16)
                .map(|index| {
                    let angle = index as f32 / 16.0 * std::f32::consts::TAU;
                    let radius = if index % 2 == 0 { 0.5 } else { 0.24 };
                    Point {
                        x: 0.5 + angle.cos() * radius,
                        y: 0.5 + angle.sin() * radius,
                    }
                })
                .collect::<Vec<_>>();
            outline(scene, &star);
        }
        Tool::Thought => {
            for (x, y, radius) in [(0.36, 0.42, 0.24), (0.62, 0.38, 0.26), (0.5, 0.62, 0.22)] {
                let (cx, cy) = screen_point(icon, Point { x, y });
                let size = radius * icon.width * 2.0;
                scene.quads.push(quad(
                    Rect {
                        x: cx - size * 0.5,
                        y: cy - size * 0.5,
                        width: size,
                        height: size,
                    },
                    [0.0; 4],
                    white,
                    size * 0.5,
                ));
            }
            for (x, y, size) in [(0.16, 0.86, 3.5), (0.06, 0.98, 2.5)] {
                let (cx, cy) = screen_point(icon, Point { x, y });
                scene.quads.push(quad(
                    Rect {
                        x: cx - size * 0.5,
                        y: cy - size * 0.5,
                        width: size,
                        height: size,
                    },
                    white,
                    [0.0; 4],
                    size * 0.5,
                ));
            }
        }
        _ => {
            let (kind, _) = tool.shape().unwrap();
            if let Some(points) = comic_dubs_shapes::shape_points(
                kind,
                Point { x: 0.02, y: 0.1 },
                Point { x: 0.98, y: 0.9 },
            ) {
                if kind == ShapeKind::Narration {
                    scene.quads.extend(polygon_fill_quads(
                        icon,
                        &points,
                        gpu_rgba([255, 238, 170, 255]),
                    ));
                } else {
                    outline(scene, &points);
                }
            }
        }
    }
}

fn draw_region(scene: &mut ComicDubsScene, page: Rect, region: Region, text: &str, clip: Rect) {
    let corners = [
        Point {
            x: region.x,
            y: region.y,
        },
        Point {
            x: region.x + region.width,
            y: region.y,
        },
        Point {
            x: region.x + region.width,
            y: region.y + region.height,
        },
        Point {
            x: region.x,
            y: region.y + region.height,
        },
    ];
    for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)).take(4) {
        let (a, b) = (screen_point(page, *a), screen_point(page, *b));
        // Dashed outline.
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        let dashes = (length / 12.0).ceil().max(1.0) as usize;
        for dash in (0..dashes).step_by(2) {
            let t0 = dash as f32 / dashes as f32;
            let t1 = ((dash + 1) as f32 / dashes as f32).min(1.0);
            let from = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
            let to = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
            if let Some((from, to)) = clip_segment(from, to, clip) {
                scene
                    .overlay_quads
                    .push(screen_line_quad(from, to, CAMERA_COLOR, 2.0));
            }
        }
    }
    let (x, y) = screen_point(page, corners[0]);
    let tag = Rect {
        x,
        y: y - 18.0,
        width: 118.0,
        height: 16.0,
    };
    scene
        .overlay_quads
        .push(quad(tag, [0.3, 0.16, 0.02, 0.9], CAMERA_COLOR, 4.0));
    overlay_label(scene, text, tag, HAlign::Center, 9.0, [255, 214, 170]);
}

fn render_reading_order_badge(
    scene: &mut ComicDubsScene,
    page_rect: Rect,
    points: &[Point],
    order: usize,
    has_audio: bool,
    has_sfx: bool,
    has_fx: bool,
) {
    let bubble = polygon_bounds(page_rect, points);
    let extra = u8::from(has_sfx) + u8::from(has_fx);
    let badge_width = if has_audio { 50.0 } else { 28.0 } + extra as f32 * 9.0;
    let min_x = page_rect.x + 2.0;
    let max_x = (page_rect.x + page_rect.width - badge_width - 2.0).max(min_x);
    let min_y = page_rect.y + 2.0;
    let max_y = (page_rect.y + page_rect.height - 26.0).max(min_y);
    let badge = Rect {
        x: (bubble.x + bubble.width - badge_width).clamp(min_x, max_x),
        y: (bubble.y + 4.0).clamp(min_y, max_y),
        width: badge_width,
        height: 24.0,
    };
    scene.overlay_quads.push(quad(
        badge,
        [0.08, 0.08, 0.11, 0.92],
        [0.9, 0.9, 0.96, 0.9],
        8.0,
    ));
    overlay_label(
        scene,
        &order.to_string(),
        Rect {
            width: 26.0,
            ..badge
        },
        HAlign::Center,
        12.0,
        TEXT,
    );
    let mut x = badge.x + 28.0;
    if has_audio {
        for (dx, dy, width, height) in [
            (0.0, 8.0, 4.0, 8.0),
            (4.0, 6.0, 5.0, 12.0),
            (12.0, 7.0, 2.0, 10.0),
            (17.0, 5.0, 2.0, 14.0),
        ] {
            scene.overlay_quads.push(quad(
                Rect {
                    x: x + dx,
                    y: badge.y + dy,
                    width,
                    height,
                },
                [0.84, 0.88, 1.0, 1.0],
                [0.0; 4],
                1.0,
            ));
        }
        x += 22.0;
    }
    for (enabled, color) in [(has_sfx, SFX_COLOR), (has_fx, [0.72, 0.63, 1.0, 1.0])] {
        if enabled {
            scene.overlay_quads.push(quad(
                Rect {
                    x: x + 1.0,
                    y: badge.y + 9.0,
                    width: 6.0,
                    height: 6.0,
                },
                color,
                [0.0; 4],
                3.0,
            ));
            x += 9.0;
        }
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

/// Next audio in the library order, `None` standing for "no audio".
fn cycle_audio(
    project: &ComicDubsProject,
    current: Option<ComicAudioId>,
    delta: isize,
) -> Option<ComicAudioId> {
    let choices = std::iter::once(None)
        .chain(project.audios().iter().map(|audio| Some(audio.id)))
        .collect::<Vec<_>>();
    crate::comic_dubs::cycle_choice(&choices, current, delta)
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

fn time_at(plan: &Timeline, track: Rect, x: f32) -> u64 {
    (((x - track.x) / track.width.max(1.0)).clamp(0.0, 1.0) * plan.total_ms as f32) as u64
}

fn cue_rect(plan: &Timeline, track: Rect, cue: &timeline::Cue) -> Rect {
    let total = plan.total_ms.max(1) as f32;
    let x = track.x + cue.start_ms as f32 / total * track.width;
    let end = track.x + cue.end_ms as f32 / total * track.width;
    let top = track.y + if track.height < 60.0 { 18.0 } else { 22.0 };
    Rect {
        x: x + 0.5,
        y: top,
        width: (end - x - 1.0).max(2.0),
        height: if track.height < 60.0 { 20.0 } else { 28.0 },
    }
}

/// Largest rectangle of `aspect` centered inside `canvas`.
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

/// Liang–Barsky clipping of a segment against `clip`.
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

/// Edge under the pointer: `(index of its first vertex, projected point)`.
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

fn polygon_fill_quads(rect: Rect, points: &[Point], color: [f32; 4]) -> Vec<QuadInstance> {
    let points = points
        .iter()
        .map(|point| screen_point(rect, *point))
        .collect::<Vec<_>>();
    fill_screen_polygon(
        &points,
        color,
        Rect {
            x: f32::MIN / 4.0,
            y: f32::MIN / 4.0,
            width: f32::MAX / 2.0,
            height: f32::MAX / 2.0,
        },
    )
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

fn media_row(layout: ComicDubsLayout, row: usize) -> Rect {
    Rect {
        x: layout.sidebar.x + 8.0,
        y: layout.sidebar.y + 52.0 + row as f32 * ROW_H,
        width: layout.sidebar.width - 16.0,
        height: ROW_H - 4.0,
    }
}

fn visible_media_rows(layout: ComicDubsLayout) -> usize {
    ((layout.sidebar.height - 104.0) / ROW_H).floor().max(1.0) as usize
}

fn scroll_rows(current: usize, delta: f32, max: usize) -> usize {
    if delta > 0.0 {
        current.saturating_sub(1)
    } else if delta < 0.0 {
        current.saturating_add(1).min(max)
    } else {
        current
    }
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

fn label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
        font_family: None,
        padding: 6.0,
        letter_spacing: 0.0,
        style: None,
    });
}

fn overlay_label(
    scene: &mut ComicDubsScene,
    text: &str,
    bounds: Rect,
    h_align: HAlign,
    font_size: f32,
    color: [u8; 3],
) {
    scene.overlay_labels.push(SceneLabel {
        text: text.into(),
        bounds,
        h_align,
        font_size,
        color,
        font_family: None,
        padding: 6.0,
        letter_spacing: 0.0,
        style: None,
    });
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

#[cfg(test)]
fn opaque_rgba(color: [u8; 4]) -> [f32; 4] {
    let mut color = gpu_rgba(color);
    color[3] = 1.0;
    color
}

fn gpu_rgba(color: [u8; 4]) -> [f32; 4] {
    crate::ui::color_picker::srgb_to_linear(rgba(color))
}

fn luminance(color: [f32; 4]) -> f32 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
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
            width: 1_280.0,
            height: 860.0,
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

    fn item_rect(ui: &ComicDubsWorkspaceUi, project: &ComicDubsProject, id: &str) -> Rect {
        ui.visible_inspector_items(project, layout())
            .into_iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("missing inspector item {id}"))
            .rect
    }

    #[test]
    fn studio_regions_never_overlap() {
        let layout = layout();
        assert_eq!(layout.toolbar.y, layout.header.y + layout.header.height);
        assert_eq!(layout.toolbar.height, 42.0);
        assert!(layout.canvas.y >= layout.toolbar.y + layout.toolbar.height);
        assert!(layout.tools.x + layout.tools.width <= layout.canvas.x);
        assert!(layout.canvas.y + layout.canvas.height <= layout.timeline.y);
        assert!(layout.timeline.x + layout.timeline.width <= layout.inspector.x);
        let last_tool = layout.tool_button(Tool::ALL.len() - 1);
        assert!(last_tool.y + last_tool.height <= layout.tools.y + layout.tools.height);
    }

    #[test]
    fn ctrl_click_then_clicks_close_a_polygon_on_the_first_vertex() {
        let project = project();
        let page_rect = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        assert_eq!(
            ui.handle_event(
                &UiEvent::CtrlClick {
                    x: page_rect.x + 100.0,
                    y: page_rect.y + 100.0
                },
                &project,
                layout()
            ),
            EventResponse::Consumed
        );
        for (x, y) in [
            (page_rect.x + 300.0, page_rect.y + 100.0),
            (page_rect.x + 200.0, page_rect.y + 300.0),
        ] {
            assert_eq!(click(&mut ui, &project, x, y), EventResponse::Consumed);
        }
        click(&mut ui, &project, page_rect.x + 101.0, page_rect.y + 101.0);
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: page_rect.x + 101.0,
                    y: page_rect.y + 101.0
                },
                &project,
                layout()
            ),
            EventResponse::Action(UiAction::ComicDubsAddBubble { .. })
        ));
    }

    #[test]
    fn polygon_tool_starts_a_draft_without_ctrl() {
        let project = project();
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        assert!(ui.control_action("comic.tool.1", &project).is_none());
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
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        for event in [
            UiEvent::CtrlClick {
                x: page.x + page.width * 0.2,
                y: page.y + page.height * 0.2,
            },
            UiEvent::MousePress {
                x: page.x + page.width * 0.6,
                y: page.y + page.height * 0.2,
            },
            UiEvent::MousePress {
                x: page.x + page.width * 0.4,
                y: page.y + page.height * 0.6,
            },
        ] {
            ui.handle_event(&event, &project, layout());
        }
        let second = screen_point(page, ui.draft[1]);
        click(&mut ui, &project, second.0, second.1);
        let moved = (second.0 + page.width * 0.1, second.1 + page.height * 0.1);
        ui.handle_event(
            &UiEvent::MouseMove {
                x: moved.0,
                y: moved.1,
            },
            &project,
            layout(),
        );
        ui.handle_event(
            &UiEvent::MouseRelease {
                x: moved.0,
                y: moved.1,
            },
            &project,
            layout(),
        );
        assert!(ui.draft[1].x > 0.69 && ui.draft[1].y > 0.29);
        assert!(ui.cancel_draft());
        assert!(ui.draft.is_empty());
    }

    #[test]
    fn shape_tools_drag_or_click_to_create_styled_bubbles() {
        let project = project();
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.control_action("comic.tool.4", &project);
        assert_eq!(ui.tool(), Tool::Shout);
        click(
            &mut ui,
            &project,
            page.x + page.width * 0.2,
            page.y + page.height * 0.2,
        );
        ui.handle_event(
            &UiEvent::MouseMove {
                x: page.x + page.width * 0.6,
                y: page.y + page.height * 0.5,
            },
            &project,
            layout(),
        );
        assert!(!ui.scene(&project, layout()).overlay_quads.is_empty());
        let response = ui.handle_event(
            &UiEvent::MouseRelease {
                x: page.x + page.width * 0.6,
                y: page.y + page.height * 0.5,
            },
            &project,
            layout(),
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
        let center = (page.x + page.width * 0.5, page.y + page.height * 0.5);
        click(&mut ui, &project, center.0, center.1);
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: center.0,
                    y: center.1
                },
                &project,
                layout()
            ),
            EventResponse::Action(UiAction::ComicDubsAddStyledBubble {
                preset: BubblePreset::Classic,
                ..
            })
        ));
    }

    #[test]
    fn camera_tool_frames_a_region_for_the_selected_bubble() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.1, 0.1, 0.2)).unwrap();
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi {
            selected_bubble: Some(bubble_id),
            ..Default::default()
        };
        ui.control_action("comic.tool.7", &project);
        assert_eq!(ui.tool(), Tool::Camera);
        assert_eq!(ui.effective_tab(), Tab::Camera);
        click(
            &mut ui,
            &project,
            page.x + page.width * 0.05,
            page.y + page.height * 0.05,
        );
        let end = (page.x + page.width * 0.45, page.y + page.height * 0.35);
        ui.handle_event(
            &UiEvent::MouseMove { x: end.0, y: end.1 },
            &project,
            layout(),
        );
        assert!(matches!(
            ui.handle_event(&UiEvent::MouseRelease { x: end.0, y: end.1 }, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsSetBubbleFx {
                bubble_id: id,
                fx: BubbleFx {
                    camera: CameraFocus::Region,
                    camera_region: Some(region),
                    ..
                },
            }) if id == bubble_id && (region.width - 0.4).abs() < 0.01
        ));
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
        let fill = polygon_fill_quads(rect, &points, opaque_rgba([255, 80, 40, 20]));
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
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let start = screen_point(page, Point { x: 0.3, y: 0.28 });
        let mut ui = ComicDubsWorkspaceUi::default();
        assert_eq!(
            click(&mut ui, &project, start.0, start.1),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), Some(bubble_id));
        assert!(!ui.is_editing_text());
        let end = (start.0 + page.width * 0.1, start.1 + page.height * 0.1);
        ui.handle_event(
            &UiEvent::MouseMove { x: end.0, y: end.1 },
            &project,
            layout(),
        );
        assert!(matches!(
            ui.handle_event(&UiEvent::MouseRelease { x: end.0, y: end.1 }, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints {
                bubble_id: id,
                points
            }) if id == bubble_id && points[0].x > 0.29 && points[0].y > 0.29
        ));
        assert_eq!(
            ui.handle_event(&UiEvent::CursorRight, &project, layout()),
            EventResponse::Ignored
        );
        ui.set_arrow_nudge(true);
        assert_eq!(
            ui.handle_event(&UiEvent::CursorRight, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsNudgeBubble {
                bubble_id,
                dx: 0.004,
                dy: 0.0,
            })
        );
        assert_eq!(
            ui.handle_event(&UiEvent::Delete, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsRemoveBubble(bubble_id))
        );
    }

    #[test]
    fn shift_click_adds_and_removes_vertices_of_the_selected_bubble() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.4)).unwrap();
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let mut ui = ComicDubsWorkspaceUi {
            selected_bubble: Some(bubble_id),
            ..Default::default()
        };
        let edge = screen_point(page, Point { x: 0.4, y: 0.2 });
        assert!(matches!(
            ui.handle_event(&UiEvent::ShiftMousePress { x: edge.0, y: edge.1 }, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsInsertBubbleVertex {
                bubble_id: id,
                after: 0,
                point,
            }) if id == bubble_id && (point.x - 0.4).abs() < 0.01
        ));
        let corner = screen_point(page, Point { x: 0.6, y: 0.6 });
        assert_eq!(
            ui.handle_event(
                &UiEvent::ShiftMousePress {
                    x: corner.0,
                    y: corner.1
                },
                &project,
                layout()
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
        let page = image_rect(layout().canvas, project.active_page().unwrap());
        let center = screen_point(page, Point { x: 0.35, y: 0.3 });
        let vertex = screen_point(page, Point { x: 0.2, y: 0.2 });
        let mut ui = ComicDubsWorkspaceUi::default();
        click(&mut ui, &project, center.0, center.1);
        ui.handle_event(
            &UiEvent::MouseRelease {
                x: center.0,
                y: center.1,
            },
            &project,
            layout(),
        );
        click(&mut ui, &project, vertex.0, vertex.1);
        let moved = (vertex.0 + page.width * 0.05, vertex.1 + page.height * 0.05);
        ui.handle_event(
            &UiEvent::MouseMove {
                x: moved.0,
                y: moved.1,
            },
            &project,
            layout(),
        );
        assert!(matches!(
            ui.handle_event(&UiEvent::MouseRelease { x: moved.0, y: moved.1 }, &project, layout()),
            EventResponse::Action(UiAction::ComicDubsSetBubblePoints { bubble_id: id, points })
                if id == bubble_id && points[0].x > 0.24 && points[1].x == 0.5
        ));

        ui.begin_text_edit(bubble_id, "Texte".into());
        assert_eq!(
            click(
                &mut ui,
                &project,
                page.x + page.width * 0.9,
                page.y + page.height * 0.9
            ),
            EventResponse::Consumed
        );
        assert_eq!(ui.selected_bubble(), None);
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
        assert!(scene
            .labels
            .iter()
            .all(|label| label.text != "Nouveau texte|"));
        let page = scene.page_rect.unwrap();
        let scale = page.height / 1_080.0;
        let label = scene
            .overlay_labels
            .iter()
            .find(|label| label.text.contains("Nouveau"))
            .unwrap();
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
    fn inspector_steppers_and_swatches_emit_bubble_actions() {
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
        let mut ui = ComicDubsWorkspaceUi {
            selected_bubble: Some(bubble_id),
            ..Default::default()
        };
        ui.sync(&project, layout());
        let size = item_rect(&ui, &project, "comic.inspector.text.size");
        assert!(matches!(
            click(&mut ui, &project, size.x + size.width - 4.0, size.y + 4.0),
            EventResponse::Action(UiAction::ComicDubsSetBubbleFontSize {
                bubble_id: id,
                font_size: 26.0,
            }) if id == bubble_id
        ));

        ui.control_action("comic.tab.1", &project);
        assert_eq!(ui.effective_tab(), Tab::Style);
        let fill = item_rect(&ui, &project, "comic.inspector.style.colors.0");
        assert_eq!(
            click(&mut ui, &project, fill.x + 2.0, fill.y + 2.0),
            EventResponse::Consumed
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
        let outline = item_rect(&ui, &project, "comic.inspector.style.colors.1");
        click(&mut ui, &project, outline.x + 2.0, outline.y + 2.0);
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
    fn keyboard_controls_cycle_effects_and_presets() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble_id = project.add_bubble(page_id, square(0.2, 0.2, 0.3)).unwrap();
        let mut ui = ComicDubsWorkspaceUi {
            selected_bubble: Some(bubble_id),
            inspector_tab: Tab::Anim,
            ..Default::default()
        };
        ui.sync(&project, layout());
        let scene = ui.scene(&project, layout());
        assert!(scene
            .controls
            .iter()
            .any(|control| control.id == "comic.inspector.anim.entrance.next"
                && control.label.contains("Apparition")));
        assert!(matches!(
            ui.control_action("comic.inspector.anim.entrance.next", &project),
            Some(UiAction::ComicDubsSetBubbleFx {
                fx: BubbleFx {
                    entrance: BubbleEntrance::Fade,
                    ..
                },
                ..
            })
        ));
        ui.control_action("comic.tab.1", &project);
        assert_eq!(
            ui.control_action("comic.inspector.style.preset0.1", &project),
            Some(UiAction::ComicDubsApplyPreset {
                bubble_id,
                preset: BubblePreset::Shout,
            })
        );
        ui.control_action("comic.tab.6", &project);
        assert!(matches!(
            ui.control_action("comic.inspector.project.music_ducking", &project),
            Some(UiAction::ComicDubsSetStudio(StudioSettings {
                music_ducking: false,
                ..
            }))
        ));
    }

    #[test]
    fn page_tab_is_shown_when_no_bubble_is_selected() {
        let project = project();
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.sync(&project, layout());
        assert_eq!(ui.effective_tab(), Tab::Page);
        assert!(matches!(
            ui.control_action("comic.inspector.page.transition.next", &project),
            Some(UiAction::ComicDubsSetPageFx {
                fx: PageFx {
                    transition: PageTransition::FadeBlack,
                    ..
                },
                ..
            })
        ));
        // Bubble tabs stay disabled without a selection.
        let tab = layout().inspector_tab(Tab::Anim);
        click(&mut ui, &project, tab.x + 2.0, tab.y + 2.0);
        assert_eq!(ui.effective_tab(), Tab::Page);
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
            .any(|control| control.id.starts_with("comic.tab.")));

        let page = scene.page_rect.unwrap();
        let vertex = screen_point(page, project.bubble(bubble_id).unwrap().points[0]);
        click(&mut ui, &project, vertex.0, vertex.1);
        let moved = (vertex.0 + page.width * 0.05, vertex.1 + page.height * 0.05);
        ui.handle_event(
            &UiEvent::MouseMove {
                x: moved.0,
                y: moved.1,
            },
            &project,
            layout(),
        );
        assert!(matches!(
            ui.handle_event(&UiEvent::MouseRelease { x: moved.0, y: moved.1 }, &project, layout()),
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
        assert_eq!(ui.media_tab, MediaTab::Audios);
        assert!(scene
            .labels
            .iter()
            .any(|label| label.text == "Chargement de 2 audio(s)…"));
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
    fn preview_crops_the_page_for_camera_zooms_and_draws_flashes_on_top() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let bubble = project.add_bubble(page_id, square(0.1, 0.1, 0.15)).unwrap();
        project.set_bubble_text(bubble, "Zoom".into());
        project.set_bubble_fx(
            bubble,
            BubbleFx {
                camera: CameraFocus::Bubble,
                camera_ms: 100,
                screen_effect: ScreenEffect::Flash,
                screen_effect_ms: 1_000,
                ..BubbleFx::default()
            },
        );
        let plan = Timeline::build(&project, None, 40);
        let reveal = plan.pages[0].cues[0].reveal_ms;
        let mut ui = ComicDubsWorkspaceUi::default();
        ui.set_preview(Some(reveal + 20), false);
        let scene = ui.scene(&project, layout());
        let layer = scene.page_layers[0];
        assert!(layer.uv[2] - layer.uv[0] < 0.9);
        assert!(scene.top_quads.iter().any(|quad| quad.color[3] > 0.5));
    }

    #[test]
    fn timeline_clicks_seek_and_select_the_bubble_under_the_pointer() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let first = project.add_bubble(page_id, square(0.1, 0.1, 0.2)).unwrap();
        let second = project.add_bubble(page_id, square(0.5, 0.5, 0.2)).unwrap();
        for id in [first, second] {
            project.set_bubble_text(id, "Texte".into());
        }
        let plan = Timeline::build(&project, None, 40);
        let track = layout().timeline_track();
        let cue = &plan.pages[0].cues[1];
        let rect = cue_rect(&plan, track, cue);
        let mut ui = ComicDubsWorkspaceUi::default();
        let response = click(&mut ui, &project, rect.x + rect.width * 0.5, rect.y + 4.0);
        assert!(
            matches!(response, EventResponse::Action(UiAction::ComicDubsSeek(at)) if at >= cue.start_ms && at <= cue.end_ms)
        );
        assert_eq!(ui.selected_bubble(), Some(second));
        assert!(ui.preview_ms().is_some());
        ui.handle_event(
            &UiEvent::MouseRelease {
                x: rect.x,
                y: rect.y,
            },
            &project,
            layout(),
        );
        let canvas = layout().canvas;
        click(&mut ui, &project, canvas.x + 5.0, canvas.y + 5.0);
        assert_eq!(ui.preview_ms(), None);
        assert_eq!(
            ui.control_action("comic.timeline.cue.0.0", &project),
            Some(UiAction::ComicDubsSeek(plan.pages[0].cues[0].reveal_ms))
        );
        assert_eq!(ui.selected_bubble(), Some(first));
    }

    #[test]
    fn editing_badges_show_order_audio_sfx_and_effects_only_while_editing() {
        let mut project = project();
        let page_id = project.active_page_id().unwrap();
        let first = project.add_bubble(page_id, square(0.1, 0.1, 0.3)).unwrap();
        project.add_bubble(page_id, square(0.5, 0.5, 0.3)).unwrap();
        let audio = project.add_audio("line.wav".into(), "line.flac".into(), recorded());
        project.assign_audio(first, Some(audio));
        let mut ui = ComicDubsWorkspaceUi::default();
        let editing = ui.scene(&project, layout());
        assert!(editing.overlay_labels.iter().any(|label| label.text == "1"));
        assert!(editing.overlay_labels.iter().any(|label| label.text == "2"));

        let page = Rect {
            width: 400.0,
            height: 400.0,
            ..Rect::default()
        };
        let triangle = [
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.5, y: 0.9 },
        ];
        let mut plain = ComicDubsScene::default();
        render_reading_order_badge(&mut plain, page, &triangle, 1, false, false, false);
        let mut rich = ComicDubsScene::default();
        render_reading_order_badge(&mut rich, page, &triangle, 1, true, true, true);
        assert!(rich.overlay_quads.len() > plain.overlay_quads.len() + 4);

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
        let page_rect = image_rect(layout().canvas, project.active_page().unwrap());
        let row = media_row(layout(), 0);
        let mut ui = ComicDubsWorkspaceUi {
            media_tab: MediaTab::Audios,
            ..Default::default()
        };
        assert_eq!(
            click(&mut ui, &project, row.x + 10.0, row.y + 10.0),
            EventResponse::Consumed
        );
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: page_rect.x + page_rect.width * 0.5,
                    y: page_rect.y + page_rect.height * 0.5,
                },
                &project,
                layout(),
            ),
            EventResponse::Action(UiAction::ComicDubsAssignAudio {
                bubble_id: id,
                audio_id: Some(audio)
            }) if id == bubble_id && audio == audio_id
        ));
        project.assign_audio(bubble_id, Some(audio_id));
        assert_eq!(
            ui.handle_event(
                &UiEvent::ContextMenu {
                    x: page_rect.x + page_rect.width * 0.5,
                    y: page_rect.y + page_rect.height * 0.5,
                },
                &project,
                layout(),
            ),
            EventResponse::Action(UiAction::ComicDubsPlayAudio(audio_id))
        );

        ui.selected_bubble = Some(bubble_id);
        ui.inspector_tab = Tab::Sound;
        ui.sync(&project, layout());
        let sfx = item_rect(&ui, &project, "comic.inspector.sfx");
        click(&mut ui, &project, row.x + 10.0, row.y + 10.0);
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: sfx.x + sfx.width * 0.5,
                    y: sfx.y + 4.0
                },
                &project,
                layout()
            ),
            EventResponse::Action(UiAction::ComicDubsSetBubbleSound {
                sound: BubbleSound {
                    sfx_audio_id: Some(id),
                    ..
                },
                ..
            }) if id == audio_id
        ));

        ui.inspector_tab = Tab::Project;
        let music = item_rect(&ui, &project, "comic.inspector.music");
        click(&mut ui, &project, row.x + 10.0, row.y + 10.0);
        assert!(matches!(
            ui.handle_event(
                &UiEvent::MouseRelease {
                    x: music.x + music.width * 0.5,
                    y: music.y + 4.0
                },
                &project,
                layout()
            ),
            EventResponse::Action(UiAction::ComicDubsSetStudio(StudioSettings {
                music_audio_id: Some(id),
                ..
            })) if id == audio_id
        ));
    }
}
