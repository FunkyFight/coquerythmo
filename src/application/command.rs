//! Semantic application commands emitted by UI and input adapters.

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    CloseApp,
    CloseSecondaryDisplay,
    Undo,
    Redo,
    ExitStudioMode,
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
        export_original_audio: bool,
        export_instrumental_audio: bool,
    },
    StartExportToPath {
        output_path: std::path::PathBuf,
        fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        export_width: u32,
        export_height: u32,
        export_original_audio: bool,
        export_instrumental_audio: bool,
    },
    FilePickerSelected {
        intent: FilePickerIntent,
        path: std::path::PathBuf,
    },
    PickProjectInstrumentalAudio,
    OpenProxyModal,
    CreateProxy {
        width: u32,
        height: u32,
        crf: u8,
    },
    OpenRenameCharacterModal,
    RenameCharacter {
        old_name: String,
        new_name: String,
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
    SelectAll,
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
    SplitDialogue,
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
    OpenProjectSettings,
    RestoreBackup,
    SaveSettings {
        lang: String,
        rythmo_font: Option<String>,
        scroll_speed: f32,
    },
    SaveProjectSettings {
        instrumental_audio_path: Option<String>,
    },
    ToggleActiveAudio,
    OffsetActiveAudioBy(i64),
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
    // Drawing
    AddDrawingStroke(crate::rythmo_drawing::DrawingStroke),
    EraseDrawingStrokes(Vec<u64>),
    TransformStrokes {
        stroke_ids: Vec<u64>,
        old_points: Vec<Vec<(f64, f32)>>,
        new_points: Vec<Vec<(f64, f32)>>,
    },
    // Tool mode
    SetToolMode(ToolMode),
    CycleBrushSize,
    ToggleEraser,
    OpenBrushColorPicker,
    CycleBrushColor {
        index: usize,
        color: [f32; 4],
    },
    // Pricing / support page
    OpenPricingPage,
    OpenDiscord,
    SubscribePlan {
        plan: String,
    },
    ActivateLicense {
        key: String,
    },
    Text(TextCommand),
}

/// Editing-only commands routed by the input layer before they become
/// low-level `UiEvent`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCommand {
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    CursorLeft,
    CursorRight,
    SelectLeft,
    SelectRight,
    CursorUp,
    CursorDown,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolbarDropdown {
    Respirations,
    Reactions,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Select,
    Draw,
}
#[derive(Debug, Clone, PartialEq)]
pub enum FilePickerIntent {
    AddVideo,
    ImportProject,
    ImportCappelaProject,
    ImportSrtProject,
    ExportProject,
    QuickSave,
    NewProjectSave,
    VoiceActorIcon,
    ProjectInstrumentalAudio,
    ExportMp4 {
        fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        export_width: u32,
        export_height: u32,
        export_original_audio: bool,
        export_instrumental_audio: bool,
    },
}
