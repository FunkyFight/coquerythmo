//! Semantic application commands emitted by UI and input adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePickerMode {
    Open,
    Save,
    Folder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileFilterSpec {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilterSpec {
    pub fn new(name: impl Into<String>, extensions: &[&str]) -> Self {
        Self {
            name: name.into(),
            extensions: extensions
                .iter()
                .map(|extension| extension.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilePickerRequest {
    pub title: String,
    pub mode: FilePickerMode,
    pub intent: FilePickerIntent,
    pub filters: Vec<FileFilterSpec>,
    pub initial_dir: Option<std::path::PathBuf>,
    pub default_extension: Option<String>,
    pub initial_filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    Accessibility(crate::accessibility::AccessibilityEvent),
    ActivateWorkspace(crate::application::workspace_service::WorkspaceId),
    ComicDubsImportImages,
    ComicDubsImportAudios,
    ComicDubsSelectPage(crate::comic_dubs::PageId),
    ComicDubsRemovePage(crate::comic_dubs::PageId),
    ComicDubsMovePage {
        page_id: crate::comic_dubs::PageId,
        delta: isize,
    },
    ComicDubsRemoveAudio(crate::comic_dubs::ComicAudioId),
    ComicDubsAddBubble {
        page_id: crate::comic_dubs::PageId,
        points: Vec<crate::comic_dubs::Point>,
    },
    ComicDubsSetBubbleText {
        bubble_id: crate::comic_dubs::BubbleId,
        text: String,
    },
    ComicDubsSetBubbleColor {
        bubble_id: crate::comic_dubs::BubbleId,
        color: [u8; 4],
    },
    ComicDubsSetBubbleFontSize {
        bubble_id: crate::comic_dubs::BubbleId,
        font_size: f32,
    },
    ComicDubsSetBubbleLetterSpacing {
        bubble_id: crate::comic_dubs::BubbleId,
        spacing: f32,
    },
    ComicDubsSetBubbleLineSpacing {
        bubble_id: crate::comic_dubs::BubbleId,
        spacing: f32,
    },
    ComicDubsSetBubbleTextColor {
        bubble_id: crate::comic_dubs::BubbleId,
        color: [u8; 4],
    },
    ComicDubsSetBubbleTextAlignment {
        bubble_id: crate::comic_dubs::BubbleId,
        alignment: crate::comic_dubs::TextAlignment,
    },
    ComicDubsSetBubbleTextStyle {
        bubble_id: crate::comic_dubs::BubbleId,
        bold: bool,
        strikethrough: bool,
        underline: bool,
    },
    ComicDubsSetBubblePoints {
        bubble_id: crate::comic_dubs::BubbleId,
        points: Vec<crate::comic_dubs::Point>,
    },
    ComicDubsOpenVertexEditor(crate::comic_dubs::BubbleId),
    ComicDubsCloseVertexEditor,
    ComicDubsSetVertexEditorPlayhead(u64),
    ComicDubsToggleVertexEditorPreview,
    ComicDubsSetBubbleVertexKeyframe {
        bubble_id: crate::comic_dubs::BubbleId,
        at_ms: u64,
        points: Vec<crate::comic_dubs::Point>,
    },
    ComicDubsRemoveBubbleVertexKeyframe {
        bubble_id: crate::comic_dubs::BubbleId,
        at_ms: u64,
    },
    ComicDubsAssignAudio {
        bubble_id: crate::comic_dubs::BubbleId,
        audio_id: Option<crate::comic_dubs::ComicAudioId>,
    },
    ComicDubsRemoveBubble(crate::comic_dubs::BubbleId),
    ComicDubsMoveBubble {
        bubble_id: crate::comic_dubs::BubbleId,
        delta: isize,
    },
    VoicelinesImportAudio,
    VoicelinesSelectAudio(crate::voicelines::AudioId),
    VoicelinesRemoveAudio(crate::voicelines::AudioId),
    VoicelinesAddRegion {
        start_ms: u64,
        end_ms: u64,
    },
    VoicelinesMoveRegion {
        region_id: crate::voicelines::RegionId,
        start_ms: u64,
        end_ms: u64,
    },
    VoicelinesSelectRegion(Option<crate::voicelines::RegionId>),
    VoicelinesRenameRegion {
        region_id: crate::voicelines::RegionId,
        name: String,
    },
    VoicelinesJoinRegions(Vec<crate::voicelines::RegionId>),
    VoicelinesDeleteRegion(crate::voicelines::RegionId),
    VoicelinesSetNamingPattern(String),
    VoicelinesAutoDetect,
    VoicelinesPlayRegion(crate::voicelines::RegionId),
    VoicelinesExportRegion(crate::voicelines::RegionId),
    VoicelinesExportAll,
    VoicelinesSendAudio {
        audio_id: crate::voicelines::AudioId,
        workspace: crate::application::workspace_service::WorkspaceId,
    },
    VoicelinesUpdateAudio {
        audio_id: crate::voicelines::AudioId,
        workspace: crate::application::workspace_service::WorkspaceId,
    },
    VoicelinesSaveSession,
    VoicelinesLoadSession,
    RecordingChooseSolo,
    RecordingChooseOnline,
    RecordingImportAudio,
    RecordingConfirmAudioImport {
        path: std::path::PathBuf,
        username: String,
        placement: Option<(crate::recording::AudioTrackId, i64)>,
    },
    RecordingSetTool(crate::recording::RecordingTool),
    RecordingAddTrack,
    RecordingRemoveTrack(crate::recording::AudioTrackId),
    RecordingBeginRenameTrack(crate::recording::AudioTrackId),
    RecordingRenameTrack {
        track_id: crate::recording::AudioTrackId,
        name: String,
    },
    RecordingToggleTrackMute(crate::recording::AudioTrackId),
    RecordingToggleTrackSolo(crate::recording::AudioTrackId),
    RecordingArmTrack(crate::recording::AudioTrackId),
    RecordingSetTrackVolume {
        track_id: crate::recording::AudioTrackId,
        volume: f32,
    },
    RecordingAdjustTrackVolume {
        track_id: crate::recording::AudioTrackId,
        delta: f32,
    },
    RecordingExportTrack(crate::recording::AudioTrackId),
    RecordingCutClip {
        clip_id: crate::recording::AudioClipId,
        at_frame: i64,
    },
    RecordingSelectClip {
        clip_id: crate::recording::AudioClipId,
        additive: bool,
    },
    RecordingSelectAsset(crate::recording::AudioAssetId),
    RecordingSendAssetToVoicelines(crate::recording::AudioAssetId),
    RecordingDeleteSelectedAsset,
    RecordingPlaceAsset {
        asset_id: crate::recording::AudioAssetId,
        track_id: crate::recording::AudioTrackId,
        start_frame: i64,
    },
    RecordingMoveSelectedClips {
        track_id: crate::recording::AudioTrackId,
        delta_frames: i64,
    },
    RecordingDeleteSelectedClips,
    RecordingStartCapture,
    RecordingStopCapture,
    OpenRecordingActorMenu,
    OpenRecordingInputDeviceModal,
    RequestActorsOpenMicrophone,
    RequestActorsTransferProject,
    RequestActorsTransferDisplaySettings,
    RequestActorsCloseProjectTransferWaiting,
    ProjectTransferAccept,
    ProjectTransferSaveAndAccept,
    ProjectTransferReplace,
    ProjectTransferRefuse,
    SetRecordingInputDevice(Option<String>),
    RecordingToggleSharedAudio,
    RecordingCycleLanguage,
    /// Copy a `coquerythmo://` host link for the current project and session
    /// to the clipboard (menu "Mise en place rapide").
    CopyQuickHostLink,
    /// Copy a `coquerythmo://` join link for the current session and room
    /// code to the clipboard. The recipient supplies their own username.
    CopyQuickJoinLink,
    /// Open the current online room invitation (room code and join URL).
    OpenRoomInvitation,
    /// Copy the current online room code to the clipboard.
    CopyRoomCode,
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
    OpenMediaExplorer,
    ToggleFileTree,
    CloseFileTree,
    MediaVideoUse {
        id: crate::project::MediaId,
    },
    MediaVideoSetDefault {
        id: crate::project::MediaId,
    },
    MediaVideoRemove {
        id: crate::project::MediaId,
    },
    MediaVideoRename {
        id: crate::project::MediaId,
        name: String,
    },
    MediaVideoBeginRename {
        id: crate::project::MediaId,
    },
    MediaVideoCreateProxy {
        id: crate::project::MediaId,
    },
    MediaVideoAssociateProxy {
        proxy_id: crate::project::MediaId,
        source_id: crate::project::MediaId,
    },
    MediaVideoDissociateProxy {
        id: crate::project::MediaId,
    },
    MediaAudioAdd {
        path: String,
    },
    MediaAudioRemove {
        id: crate::project::MediaId,
    },
    MediaAudioRename {
        id: crate::project::MediaId,
        name: String,
    },
    MediaAudioBeginRename {
        id: crate::project::MediaId,
    },
    MediaReorderVideo {
        id: crate::project::MediaId,
        to_index: usize,
    },
    MediaReorderAudio {
        id: crate::project::MediaId,
        to_index: usize,
    },
    LanguageReorder {
        id: u64,
        to_index: usize,
    },
    LanguageBeginRename {
        id: u64,
    },
    SetLanguageInstrumentalAudioPath {
        id: u64,
        path: String,
    },
    SetLanguageInstrumentalAudioByMediaId {
        band_id: u64,
        media_id: crate::project::MediaId,
    },
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
    SwitchMediaVideo {
        use_proxy: bool,
    },
    SetDefaultMediaVideo {
        use_proxy: bool,
    },
    DeleteMediaVideo {
        use_proxy: bool,
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
    PickProjectInstrumentalAudio,
    OpenProxyModal,
    CreateProxy {
        width: u32,
        height: u32,
        crf: u8,
        encoder: crate::video_proxy::ProxyEncoder,
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
    OpenTextEmotionMenu,
    SetTextEmotion {
        line_id: u64,
        range: Option<(usize, usize)>,
        emotion: Option<crate::rythmo_line::TextEmotion>,
    },
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
    GenerateDetectionSigns {
        line_id: u64,
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
    SetLinePresence {
        line_id: u64,
        presence: crate::rythmo_line::LinePresence,
    },
    ResizeDetection {
        address: crate::detection::DetectionAddress,
        media_tick: crate::detection::MediaTick,
        duration: crate::detection::MediaTick,
    },
    NudgeSelectedSyncAnchor {
        delta_graphemes: i32,
    },
    ToggleSelectedSyncAffinity,
    MoveSyncAnchor {
        address: crate::detection::DetectionAddress,
        grapheme_boundary: u32,
    },
    AddSyncPointAtPlayhead,
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
    PickTemporaryDirectory,
    SaveSettings {
        lang: String,
        temporary_directory: std::path::PathBuf,
        show_controls_hint: bool,
    },
    SaveProjectSettings {
        rythmo_font: Option<String>,
        scroll_speed: f32,
        reading_bar_offset_percent: f32,
        instrumental_audio_path: Option<String>,
        highlight_read_word: bool,
        scrolling_text_uses_character_color: bool,
        show_text_emotion_lanes: bool,
    },
    SaveComicDubsSettings {
        font_family: Option<String>,
        bubble_duration_ms: u64,
        page_duration_ms: u64,
        default_font_size: f32,
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
                | Self::ResizeDetection { .. }
                | Self::DeleteDetection { .. }
                | Self::NudgeSelectedDetection { .. }
                | Self::NudgeSelectedSyncAnchor { .. }
                | Self::MoveSyncAnchor { .. }
                | Self::AddSyncPointAtPlayhead
                | Self::UpdateLineText { .. }
                | Self::SetTextEmotion { .. }
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

    /// Actions refused while a background task (export, proxy or project
    /// import) runs: they would replace the document, touch the media files
    /// the worker is reading, or start a concurrent worker on the shared
    /// progress slot. Everything else stays available so the UI never
    /// freezes behind a progress indicator.
    pub fn blocked_during_background_task(&self) -> bool {
        matches!(
            self,
            Self::AddVideo
                | Self::NewProject
                | Self::OpenRecentProject { .. }
                | Self::ImportProject
                | Self::ImportCappelaProject
                | Self::ImportSrtProject
                | Self::ImportSubtitles
                | Self::SwitchMediaVideo { .. }
                | Self::SetDefaultMediaVideo { .. }
                | Self::DeleteMediaVideo { .. }
                | Self::StartExport { .. }
                | Self::StartExportToPath { .. }
                | Self::StartConfiguredExport { .. }
                | Self::StartConfiguredExportToPath { .. }
                | Self::OpenProxyModal
                | Self::CreateProxy { .. }
                | Self::ExitApplication
                | Self::CloseApp
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
    ComicDubsImage,
    ComicDubsAudio,
    ComicDubsExport {
        configuration: crate::project::ExportConfiguration,
    },
    RecordingAudio,
    VoicelinesAudio,
    VoicelinesExportRegion {
        audio_id: crate::voicelines::AudioId,
        region_id: crate::voicelines::RegionId,
    },
    VoicelinesExportAll {
        audio_id: crate::voicelines::AudioId,
    },
    VoicelinesSaveSession,
    VoicelinesLoadSession,
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
    ExportRecordingTrack {
        track_id: crate::recording::AudioTrackId,
    },
}
