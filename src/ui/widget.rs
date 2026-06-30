#[derive(Debug, Clone, Copy)]
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
    ShiftCursorLeft,
    ShiftCursorRight,
    CursorUp,
    CursorDown,
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
pub enum UiAction {
    CloseApp,
    AddVideo,
    ImportProject,
    ImportCappelaProject,
    ImportSrtProject,
    ExportProject,
    OpenExportModal,
    StartExport {
        fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        export_width: u32,
        export_height: u32,
        instrumental_audio_path: Option<std::path::PathBuf>,
    },
    PickExportInstrumentalAudio,
    OpenProxyModal,
    CreateProxy {
        width: u32,
        height: u32,
        crf: u8,
    },
    QuickSave,
    CancelExport,
    TogglePlayPause,
    SetVolume(f32),
    ToggleMute,
    PrevFrame,
    NextFrame,
    SeekRelative(i32),
    SeekAbsolute(i64),
    SeekToNextBoucle {
        direction: i32,
    },
    CreateLine {
        frame: i64,
        y_slot: f32,
    },
    ResizeLine {
        id: u64,
        start_frame: i64,
        duration_frames: i64,
    },
    MoveLine {
        id: u64,
        start_frame: i64,
        y_slot: f32,
    },
    MoveLines {
        moves: Vec<(u64, i64, f32)>,
    },
    UpdateLineText {
        id: u64,
        text: String,
    },
    SetCharacter {
        line_id: u64,
        name: String,
        color: [f32; 4],
    },
    SetCharacterColor {
        line_id: u64,
        color: [f32; 4],
    },
    UpdateCharacterName {
        line_id: u64,
        name: String,
    },
    FinalizeCharacter {
        line_id: u64,
    },
    OpenVoiceActorModal,
    PickVoiceActorIcon,
    CreateVoiceActor {
        name: String,
        icon_path: String,
    },
    AssignVoiceActorLine {
        line_id: u64,
        actor_name: String,
    },
    AssignVoiceActorCharacter {
        line_id: u64,
        actor_name: String,
    },
    UnassignVoiceActorLine {
        line_id: u64,
        actor_name: String,
    },
    UnassignVoiceActorCharacter {
        line_id: u64,
        actor_name: String,
    },
    DeleteSelected,
    MoveMarker {
        index: usize,
        frame: i64,
    },
    AddMarker(crate::rythmo_line::MarkerKind),
    AddQuickLine {
        text: String,
    },
    OpenDropdown(ToolbarDropdown),
    OpenSecondaryDisplay,
    StopEditing,
    ToggleKaraokeForSelection,
    OpenRecentProject {
        video_path: std::path::PathBuf,
        br_path: std::path::PathBuf,
    },
    SetSyllableRatios {
        line_id: u64,
        ratios: Vec<f32>,
    },
    // Network
    OpenServerBrowser,
    OpenConnectModal {
        ip: String,
        port: u16,
        join: bool,
    },
    OpenAddServerModal,
    AddServer {
        ip: String,
        port: u16,
    },
    RemoveServer(usize),
    RefreshServers,
    NetworkConnect {
        ip: String,
        port: u16,
        password: String,
        username: String,
        room_code: Option<String>,
    },
    NetworkDisconnect,
    // Settings
    OpenSettings,
    RestoreBackup,
    SaveSettings {
        lang: String,
        rythmo_font: Option<String>,
        scroll_speed: f32,
    },
    // New project
    NewProject,
    NewProjectSave,
    NewProjectDiscard,
    // Studio mode
    EnterStudioMode,
    ShowStudioWarning,
    // Notes
    AddNote,
    UpdateLineNote {
        line_id: u64,
        note: String,
    },
    SetClipboard(String),
    SetClipboardAndUpdateLineText {
        clipboard: String,
        id: u64,
        text: String,
    },
    SetClipboardAndUpdateCharacterName {
        clipboard: String,
        line_id: u64,
        name: String,
    },
    SetClipboardAndUpdateLineNote {
        clipboard: String,
        line_id: u64,
        note: String,
    },
    CopySelectedLine,
    CutSelectedLine,
    PasteLine,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolbarDropdown {
    Respirations,
    Reactions,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventResponse {
    Ignored,
    Consumed,
    Action(UiAction),
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
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IconInstance {
    pub rect: [f32; 4],    // x, y, w, h in pixels
    pub uv_rect: [f32; 4], // u_min, v_min, u_max, v_max
    pub tint: [f32; 4],    // RGBA tint
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    pub rect: [f32; 4],         // x, y, w, h
    pub color: [f32; 4],        // bg color
    pub color_bottom: [f32; 4], // gradient bottom (if == color, no gradient)
    pub border_color: [f32; 4], // border color
    pub border_width: f32,
    pub border_radius: f32,
    pub shadow_offset: [f32; 2], // shadow dx, dy
    pub shadow_color: [f32; 4],  // shadow color + alpha
    pub shadow_blur: f32,
    pub rotation: f32, // radians, rotation around quad center
    pub _padding: [f32; 2],
}
