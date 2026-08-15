//! Backend-neutral UI interaction and drawing primitives.
//!
//! These types describe pointer/keyboard input and the small render payloads
//! emitted by widgets. They deliberately do not know about application state
//! or project mutations.

pub use crate::application::command::{ToolbarDropdown, UiAction};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum HAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Overflow {
    #[default]
    Clip,
    ClipWithLetterSpacing(f32),
    Ellipsis,
    Visible,
}

pub struct LabelInfo<'a> {
    pub text: &'a str,
    pub bounds: Rect,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub overflow: Overflow,
    pub padding: f32,
    pub font_size_override: Option<f32>,
    pub color_override: Option<[u8; 3]>,
    pub font_family_override: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    MouseMove {
        x: f32,
        y: f32,
    },
    MousePress {
        x: f32,
        y: f32,
    },
    MouseRelease {
        x: f32,
        y: f32,
    },
    Scroll {
        x: f32,
        y: f32,
        delta: f32,
        fast: bool,
        ctrl: bool,
    },
    KeyInput {
        text: String,
    },
    CursorLeft,
    CursorRight,
    MoveWordLeft,
    MoveWordRight,
    ShiftCursorLeft,
    ShiftCursorRight,
    CursorUp,
    CursorDown,
    SelectWordLeft,
    SelectWordRight,
    FocusNext,
    FocusPrevious,
    Activate,
    Home,
    End,
    PageUp,
    PageDown,
    AltCursorLeft,
    AltCursorRight,
    OpenContextMenu,
    Delete,
    SelectAll,
    Copy,
    Cut,
    UndoTextEdit,
    CtrlClick {
        x: f32,
        y: f32,
    },
    ShiftMousePress {
        x: f32,
        y: f32,
    },
    DoubleClick {
        x: f32,
        y: f32,
    },
    MiddlePress {
        x: f32,
        y: f32,
    },
    MiddleRelease {
        x: f32,
        y: f32,
    },
    ContextMenu {
        x: f32,
        y: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventResponse {
    Ignored,
    Consumed,
    Action(UiAction),
    /// Multiple semantic actions are needed when an interaction both
    /// performs a command and changes an accessibility container state.
    Actions(Vec<UiAction>),
}

pub trait Widget {
    fn bounds(&self) -> Rect;
    fn handle_event(&mut self, event: &UiEvent) -> EventResponse;
    fn render_quads(&self) -> Vec<QuadInstance>;
    fn render_icons(&self) -> Vec<IconInstance> {
        vec![]
    }
    fn labels(&self) -> Vec<LabelInfo<'_>>;
    /// When true, this widget receives events before all others (e.g. open dropdown).
    fn captures_all(&self) -> bool {
        false
    }
    /// Tooltip text shown on hover. Return None for no tooltip.
    fn tooltip(&self) -> Option<&str> {
        None
    }
    fn accessible_label(&self) -> Option<&str> {
        self.tooltip()
    }
    fn accessible_role(&self) -> super::focus::AccessibleRole {
        super::focus::AccessibleRole::Button
    }
    fn accessible_selected(&self) -> Option<bool> {
        None
    }
    /// Open a specific submenu from a semantic shortcut.
    fn open_submenu(&mut self, _trigger_index: usize) -> bool {
        false
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IconInstance {
    pub rect: [f32; 4],
    pub uv_rect: [f32; 4],
    pub tint: [f32; 4],
    /// Rotation, horizontal skew and normalized pivot x/y.
    pub transform: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub color_bottom: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub border_radius: f32,
    pub shadow_offset: [f32; 2],
    pub shadow_color: [f32; 4],
    pub shadow_blur: f32,
    pub rotation: f32,
    pub _padding: [f32; 2],
}
