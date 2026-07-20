//! Semantic application commands emitted by UI and input adapters.

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    Accessibility(crate::accessibility::AccessibilityEvent),
    ActivateWorkspace(crate::application::workspace_service::WorkspaceId),
    RecordingChooseSolo,
    RecordingChooseOnline,
    RecordingSetTool(crate::recording::RecordingTool),
    RecordingToggleTrackMute(crate::recording::AudioTrackId),
    RecordingToggleTrackSolo(crate::recording::AudioTrackId),
    RecordingArmTrack(crate::recording::AudioTrackId),
    RecordingSelectClip {
        clip_id: crate::recording::AudioClipId,
        additive: bool,
    },
    RecordingSelectAsset(crate::recording::AudioAssetId),
    RecordingStartCapture,
    RecordingStopCapture,
    CloseApp,
    CloseSecondaryDisplay,
    Undo,
    Redo,
    AddVideo,
    ImportProject,
    ImportCappelaProject,
    ImportSrtProject,
    /// Open the single subtitle/project import picker and detect the format.
    ImportSubtitles,
    ExportProject,
    OpenExportModal,
    OpenLanguages,
    CreateLanguage {
        name: String,
    },
    RenameLanguage {
        id: u64,
        name: String,
    },
    DeleteLanguage {
        id: u64,
    },
    SelectLanguage {
        id: u64,
    },
    SetLanguageSyllableLanguage {
        id: u64,
        language: crate::project::SyllableLanguage,
    },
    PickLanguageInstrumentalAudio {
        id: u64,
    },
    ClearLanguageInstrumentalAudio {
        id: u64,
    },
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
    StartConfiguredExport {
        configuration: crate::project::ExportConfiguration,
    },
    StartConfiguredExportToPath {
        output_path: std::path::PathBuf,
        configuration: crate::project::ExportConfiguration,
    },
    SaveExportConfiguration {
        configuration: crate::project::ExportConfiguration,
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
    OpenAutomation,
    CloseAutomation,
    OpenLinesPanel,
    OpenRolesPanel,
    CloseSidePanel,
    DeleteSidePanelLines {
        line_ids: Vec<u64>,
    },
    CopySidePanelLines {
        line_ids: Vec<u64>,
        cut: bool,
    },
    SetLinesRole {
        line_ids: Vec<u64>,
        name: String,
        color: [f32; 4],
    },
    SetRoleColor {
        role: String,
        color: [f32; 4],
    },
    AutomationAddNode {
        kind: crate::automation::AutomationNodeKind,
        x: f32,
        y: f32,
    },
    AutomationAddConnectedNode {
        kind: crate::automation::AutomationNodeKind,
        x: f32,
        y: f32,
        from_node: u64,
        edge_kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
    },
    AutomationMoveNode {
        node_id: u64,
        x: f32,
        y: f32,
    },
    AutomationDeleteNode {
        node_id: u64,
    },
    AutomationConnect {
        from_node: u64,
        kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
        to_node: u64,
    },
    AutomationDisconnect {
        from_node: u64,
        kind: crate::automation::AutomationEdgeKind,
        branch: crate::automation::AutomationBranch,
    },
    AutomationAddRole {
        node_id: u64,
        role: String,
    },
    AutomationRemoveRole {
        node_id: u64,
        role: String,
    },
    AutomationSetTrack {
        node_id: u64,
        track: u8,
    },
    AutomationSetNodeEnabled {
        node_id: u64,
        enabled: bool,
    },
    RenameCharacter {
        old_name: String,
        new_name: String,
    },
    QuickSave,
    CancelExport,
    TogglePlayPause,
    SetVolume(f32),
    AdjustVolume(f32),
    ToggleMute,
    PrevFrame,
    NextFrame,
    SeekRelative(i32),
    SeekAbsolute(i64),
    FinishSeek,
    SeekToNextBoucle {
        direction: i32,
    },
    CreateLine {
        frame: i64,
        y_slot: f32,
    },
    SelectLineAtPlayhead,
    NavigateLines {
        direction: i32,
    },
    ClearLineSelection,
    SetSelectedLineStartAtPlayhead,
    SetSelectedLineEndAtPlayhead,
    StartEditingSelectedLine,
    StartEditingSelectedCharacter,
    BeginKeyboardPan {
        direction: i32,
    },
    EndKeyboardPan,
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
    /// Move the currently selected rythmo line to the adjacent track.
    ///
    /// A semantic action is used here instead of mutating the line directly
    /// from the keyboard router so mouse and keyboard edits continue to share
    /// the same command/history path.
    MoveSelectedLineTrack {
        direction: i32,
    },
    /// Shift the selected line group horizontally without changing duration.
    NudgeSelectedLines {
        delta_frames: i64,
    },
    MoveLines {
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
    OpenRecentProjects,
    RemoveRecentProject {
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
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
    },
    ToggleActiveAudio,
    OffsetActiveAudioBy(i64),
    // New project
    NewProject,
    NewProjectSave,
    NewProjectDiscard,
    CloseProject,
    CloseProjectSave,
    CloseProjectDiscard,
    ExitApplication,
    ExitApplicationSave,
    ExitApplicationDiscard,
    ToggleScreenReader,
    CreateLineAtTrack {
        track: usize,
    },
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

impl UiAction {
    /// Document mutations that belong to bande-rythmo authoring.
    ///
    /// Recording keeps receiving remote/sync changes, but local UI commands
    /// matching this list are rejected by the application dispatcher as a
    /// second line of defence behind the read-only input controller.
    pub fn mutates_rythmo_project(&self) -> bool {
        if matches!(self, Self::CopySidePanelLines { cut: true, .. }) {
            return true;
        }
        matches!(
            self,
            Self::Undo
                | Self::Redo
                | Self::CreateLanguage { .. }
                | Self::RenameLanguage { .. }
                | Self::DeleteLanguage { .. }
                | Self::SelectLanguage { .. }
                | Self::SetLanguageSyllableLanguage { .. }
                | Self::PickLanguageInstrumentalAudio { .. }
                | Self::ClearLanguageInstrumentalAudio { .. }
                | Self::PickProjectInstrumentalAudio
                | Self::SaveProjectSettings { .. }
                | Self::DeleteSidePanelLines { .. }
                | Self::SetLinesRole { .. }
                | Self::SetRoleColor { .. }
                | Self::AutomationAddNode { .. }
                | Self::AutomationAddConnectedNode { .. }
                | Self::AutomationMoveNode { .. }
                | Self::AutomationDeleteNode { .. }
                | Self::AutomationConnect { .. }
                | Self::AutomationDisconnect { .. }
                | Self::AutomationAddRole { .. }
                | Self::AutomationRemoveRole { .. }
                | Self::AutomationSetTrack { .. }
                | Self::AutomationSetNodeEnabled { .. }
                | Self::RenameCharacter { .. }
                | Self::OffsetActiveAudioBy(_)
                | Self::CreateLine { .. }
                | Self::SetSelectedLineStartAtPlayhead
                | Self::SetSelectedLineEndAtPlayhead
                | Self::ResizeLine { .. }
                | Self::MoveLine { .. }
                | Self::MoveSelectedLineTrack { .. }
                | Self::NudgeSelectedLines { .. }
                | Self::MoveLines { .. }
                | Self::AddDetection { .. }
                | Self::MoveDetection { .. }
                | Self::DeleteDetection { .. }
                | Self::NudgeSelectedDetection { .. }
                | Self::UpdateLineText { .. }
                | Self::SetCharacter { .. }
                | Self::SetCharacterColor { .. }
                | Self::UpdateCharacterName { .. }
                | Self::FinalizeCharacter { .. }
                | Self::CreateVoiceActor { .. }
                | Self::AssignVoiceActorLine { .. }
                | Self::AssignVoiceActorCharacter { .. }
                | Self::UnassignVoiceActorLine { .. }
                | Self::UnassignVoiceActorCharacter { .. }
                | Self::DeleteSelected
                | Self::MoveMarker { .. }
                | Self::AddMarker(_)
                | Self::AddQuickLine { .. }
                | Self::ToggleKaraokeForSelection
                | Self::SetSyllableRatios { .. }
                | Self::SplitDialogue
                | Self::CreateLineAtTrack { .. }
                | Self::AddNote
                | Self::UpdateLineNote { .. }
                | Self::SetClipboardAndUpdateLineText { .. }
                | Self::SetClipboardAndUpdateCharacterName { .. }
                | Self::SetClipboardAndUpdateLineNote { .. }
                | Self::CutSelectedLine
                | Self::PasteLine
                | Self::AddDrawingStroke(_)
                | Self::EraseDrawingStrokes(_)
                | Self::TransformStrokes { .. }
                | Self::Text(
                    TextCommand::Cut | TextCommand::Paste | TextCommand::Undo | TextCommand::Delete
                )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::workspace_service::WorkspaceId;

    #[test]
    fn classifies_authoring_actions_for_recording_guard() {
        assert!(UiAction::MoveLine {
            id: 1,
            start_frame: 25,
            y_slot: 2.0,
        }
        .mutates_rythmo_project());
        assert!(UiAction::Text(TextCommand::Delete).mutates_rythmo_project());
        assert!(UiAction::CopySidePanelLines {
            line_ids: vec![1],
            cut: true,
        }
        .mutates_rythmo_project());

        assert!(!UiAction::SeekRelative(1).mutates_rythmo_project());
        assert!(!UiAction::CopySelectedLine.mutates_rythmo_project());
        assert!(!UiAction::ActivateWorkspace(WorkspaceId::Recording).mutates_rythmo_project());
    }
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
    CloseProjectSave,
    ExitApplicationSave,
    VoiceActorIcon,
    ProjectInstrumentalAudio,
    LanguageInstrumentalAudio {
        language_id: u64,
    },
    ExportMp4 {
        fps: f64,
        br_scale: f32,
        karaoke_text_scale: f32,
        export_width: u32,
        export_height: u32,
        export_original_audio: bool,
        export_instrumental_audio: bool,
    },
    ConfiguredExport {
        configuration: crate::project::ExportConfiguration,
    },
}
