//! Semantic announcements and the internal, interruptible speech worker.

use std::collections::{HashMap, VecDeque};
#[cfg(target_os = "macos")]
use std::process::Child;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::thread::{self, JoinHandle};
#[cfg(target_os = "macos")]
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum AccessibilityEvent {
    Focus {
        label: String,
        role: String,
    },
    Selection {
        label: String,
    },
    ValueChanged {
        label: String,
        value: String,
    },
    Activation {
        label: String,
    },
    Success {
        message: String,
    },
    Error {
        message: String,
    },
    Opened {
        label: String,
    },
    Closed {
        label: String,
    },
    Collapsed {
        label: String,
    },
    CharacterTyped {
        character: Option<char>,
        secret: bool,
    },
    CharacterDeleted {
        secret: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnnouncementPriority {
    Navigation,
    Action,
    Confirmation,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub text: String,
    pub priority: AnnouncementPriority,
    pub interruptible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDescriptor {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpeechRequest {
    pub text: String,
    pub language: String,
    pub voice_id: Option<String>,
    pub target_samples: Option<usize>,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SpeechClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpeechCacheKey {
    pub line_id: u64,
    pub content_revision: u64,
    pub voice_id: String,
    pub language: String,
    pub duration_samples: usize,
}

/// Versioned in-memory clip cache with deterministic byte accounting.
pub struct SpeechClipCache {
    max_bytes: usize,
    bytes: usize,
    entries: HashMap<SpeechCacheKey, SpeechClip>,
    order: VecDeque<SpeechCacheKey>,
}

impl SpeechClipCache {
    pub const DEFAULT_MAX_BYTES: usize = 512 * 1024 * 1024;

    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &SpeechCacheKey) -> Option<&SpeechClip> {
        if self.entries.contains_key(key) {
            self.order.retain(|candidate| candidate != key);
            self.order.push_back(key.clone());
        }
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: SpeechCacheKey, clip: SpeechClip) {
        let clip_bytes = clip.samples.len() * std::mem::size_of::<f32>();
        if clip_bytes > self.max_bytes {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.samples.len() * 4);
            self.order.retain(|candidate| candidate != &key);
        }
        while self.bytes + clip_bytes > self.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(old.samples.len() * 4);
            }
        }
        self.bytes += clip_bytes;
        self.order.push_back(key.clone());
        self.entries.insert(key, clip);
    }

    pub fn invalidate_line(&mut self, line_id: u64) {
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|key| key.line_id == line_id)
            .cloned()
            .collect();
        for key in keys {
            if let Some(clip) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(clip.samples.len() * 4);
            }
            self.order.retain(|candidate| candidate != &key);
        }
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

pub trait SpeechBackend: Send + 'static {
    fn is_available(&self) -> bool;
    fn voices(&mut self) -> Result<Vec<VoiceDescriptor>, String>;
    fn synthesize(&mut self, request: &SpeechRequest) -> Result<SpeechClip, String>;
    fn stop(&mut self);
}

pub fn event_for_action(
    action: &crate::application::command::UiAction,
) -> Option<AccessibilityEvent> {
    use crate::application::command::{ToolMode, ToolbarDropdown, UiAction};
    use crate::rythmo_line::MarkerKind;
    let key = match action {
        UiAction::ImportSubtitles => "menu.project.import",
        UiAction::RestoreBackup => "menu.project.restore_backup",
        UiAction::OpenRecentProjects => "menu.project.recent",
        UiAction::CloseProject => "menu.project.close",
        UiAction::ExportProject | UiAction::OpenExportModal => "menu.export.mp4",
        UiAction::ShowStudioWarning => "menu.export.studio_mode",
        UiAction::OpenRenameCharacterModal => "menu.tools.rename_character",
        UiAction::OpenProjectSettings => "project_settings.title",
        UiAction::OpenSettings => "settings.title",
        UiAction::PickProjectInstrumentalAudio => "project_settings.browse",
        UiAction::SaveProjectSettings { .. } => "settings.save",
        UiAction::PrevFrame => "toolbar.prev_frame",
        UiAction::NextFrame => "toolbar.next_frame",
        UiAction::ToggleMute => "toolbar.mute",
        UiAction::SetSelectedLineStartAtPlayhead => "accessibility.line_start_set",
        UiAction::SetSelectedLineEndAtPlayhead => "accessibility.line_end_set",
        UiAction::StartEditingSelectedLine => "accessibility.edit_line",
        UiAction::StartEditingSelectedCharacter => "accessibility.edit_character",
        UiAction::AddMarker(MarkerKind::Boucle) => "toolbar.boucle",
        UiAction::AddMarker(MarkerKind::Out) => "toolbar.out",
        UiAction::AddMarker(MarkerKind::SceneChange) => "toolbar.scene",
        UiAction::AddMarker(MarkerKind::LiaisonLeft) => "toolbar.liaison_left",
        UiAction::AddMarker(MarkerKind::LiaisonRight) => "toolbar.liaison_right",
        UiAction::OpenDropdown(ToolbarDropdown::Respirations) => "toolbar.respirations",
        UiAction::OpenDropdown(ToolbarDropdown::Reactions) => "toolbar.reactions",
        UiAction::AddNote => "toolbar.note",
        UiAction::ToggleKaraokeForSelection => "toolbar.karaoke",
        UiAction::SetToolMode(ToolMode::Select) => "toolbar.select_mode",
        UiAction::SetToolMode(ToolMode::Draw) => "toolbar.draw_mode",
        _ => return None,
    };
    Some(AccessibilityEvent::Activation {
        label: crate::i18n::t(key).to_string(),
    })
}

/// Return an announcement for every routed keyboard command. Commands with a
/// semantic action name keep that name; every other command falls back to the
/// localized key chord so adding a new shortcut can never make it silent.
pub fn event_for_keyboard_shortcut(
    action: &crate::application::command::UiAction,
    shortcut_label: &str,
) -> AccessibilityEvent {
    event_for_action(action).unwrap_or_else(|| AccessibilityEvent::Activation {
        label: shortcut_label.to_string(),
    })
}

#[cfg(target_os = "macos")]
enum WorkerCommand {
    Speak(Announcement),
    Stop { resumable: bool },
    Resume,
    Shutdown,
}

pub struct NarrationService {
    enabled: bool,
    available: bool,
    accessibility_sender: Option<Sender<AccessibilityEvent>>,
    paused: AtomicBool,
    last_event: Mutex<Option<AccessibilityEvent>>,
    #[cfg(target_os = "macos")]
    sender: Sender<WorkerCommand>,
    #[cfg(target_os = "macos")]
    worker: Option<JoinHandle<()>>,
}

impl NarrationService {
    pub fn new(enabled: bool, accessibility_sender: Option<Sender<AccessibilityEvent>>) -> Self {
        let available = cfg!(target_os = "macos")
            || (cfg!(target_os = "windows") && accessibility_sender.is_some());
        #[cfg(target_os = "macos")]
        let (sender, receiver) = mpsc::channel();
        #[cfg(target_os = "macos")]
        let worker = Some(
            thread::Builder::new()
                .name("coquerythmo-narration".into())
                .spawn(move || speech_worker(receiver))
                .expect("spawn narration worker"),
        );
        Self {
            enabled: enabled && available,
            available,
            accessibility_sender,
            paused: AtomicBool::new(false),
            last_event: Mutex::new(None),
            #[cfg(target_os = "macos")]
            sender,
            #[cfg(target_os = "macos")]
            worker,
        }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        self.enabled = enabled && self.available;
        self.paused.store(false, Ordering::Release);
        if !self.enabled {
            #[cfg(target_os = "macos")]
            let _ = self.sender.send(WorkerCommand::Stop { resumable: false });
        }
        self.enabled
    }

    pub fn announce(&self, announcement: Announcement) {
        let mut announcement = announcement;
        announcement.text = words_only(&announcement.text);
        if self.enabled && !announcement.text.trim().is_empty() {
            #[cfg(target_os = "macos")]
            let _ = self.sender.send(WorkerCommand::Speak(announcement));
        }
    }

    pub fn announce_event(&self, event: AccessibilityEvent) {
        // Character echo makes normal text entry unusable with speech. Text
        // fields are announced at the field/action level instead of once per
        // keystroke; keep these variants for semantic integrations but never
        // send them to the voice backend.
        if matches!(
            event,
            AccessibilityEvent::CharacterTyped { .. } | AccessibilityEvent::CharacterDeleted { .. }
        ) {
            return;
        }
        if !self.enabled {
            return;
        }
        if let Ok(mut last_event) = self.last_event.lock() {
            *last_event = Some(event.clone());
        }
        if self.paused.load(Ordering::Acquire) {
            return;
        }
        #[cfg(target_os = "windows")]
        if let Some(sender) = &self.accessibility_sender {
            let _ = sender.send(event);
            return;
        }
        self.announce(format_event(event));
    }

    pub fn stop(&self) {
        self.paused.store(true, Ordering::Release);
        #[cfg(target_os = "macos")]
        let _ = self.sender.send(WorkerCommand::Stop { resumable: true });
    }

    pub fn resume(&self) {
        if !self.enabled || !self.paused.swap(false, Ordering::AcqRel) {
            return;
        }
        #[cfg(target_os = "windows")]
        if let (Some(sender), Ok(last_event)) = (&self.accessibility_sender, self.last_event.lock())
        {
            if let Some(event) = last_event.clone() {
                let _ = sender.send(event);
            }
            return;
        }
        #[cfg(target_os = "macos")]
        let _ = self.sender.send(WorkerCommand::Resume);
    }
}

/// Keep spoken output semantic: decorative arrows, chevrons, percent signs,
/// slashes and punctuation are separators, never words read aloud by SAPI or
/// VoiceOver.
pub fn words_only(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        // Keep letters and numbers only. In particular, symbols such as ®,
        // ™, arrows and icon glyphs must never reach the platform voice.
        if character.is_alphanumeric() && !matches!(character, '\u{00a9}' | '\u{00ae}' | '\u{2122}')
        {
            if pending_space && !output.is_empty() {
                output.push(' ');
            }
            pending_space = false;
            output.push(character);
        } else {
            pending_space = true;
        }
    }
    output
}

#[cfg(target_os = "macos")]
impl Drop for NarrationService {
    fn drop(&mut self) {
        let _ = self.sender.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn format_event(event: AccessibilityEvent) -> Announcement {
    use AccessibilityEvent as Event;
    let (text, priority, interruptible) = match event {
        Event::Focus { label, role } => (
            format!("{label}, {role}"),
            AnnouncementPriority::Navigation,
            true,
        ),
        Event::Selection { label } => (
            format!("{} : {label}", crate::i18n::t("accessibility.selected")),
            AnnouncementPriority::Navigation,
            true,
        ),
        Event::ValueChanged { label, value } => (
            format!("{label} : {value}"),
            AnnouncementPriority::Action,
            true,
        ),
        Event::Activation { label } => (label, AnnouncementPriority::Action, false),
        Event::Success { message } => (message, AnnouncementPriority::Confirmation, false),
        Event::Error { message } => (message, AnnouncementPriority::Error, false),
        Event::Opened { label } => (
            format!("{label}, {}", crate::i18n::t("accessibility.opened")),
            AnnouncementPriority::Action,
            false,
        ),
        Event::Closed { label } => (
            format!("{label}, {}", crate::i18n::t("accessibility.closed")),
            AnnouncementPriority::Action,
            false,
        ),
        Event::Collapsed { label } => (
            format!("{label}, {}", crate::i18n::t("accessibility.collapsed")),
            AnnouncementPriority::Action,
            false,
        ),
        Event::CharacterTyped { character, secret } => {
            let text = if secret {
                crate::i18n::t("accessibility.character_typed").to_string()
            } else if character == Some(' ') {
                crate::i18n::t("accessibility.space").to_string()
            } else {
                character
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| crate::i18n::t("accessibility.character_typed").to_string())
            };
            (text, AnnouncementPriority::Action, true)
        }
        Event::CharacterDeleted { .. } => (
            crate::i18n::t("accessibility.character_deleted").to_string(),
            AnnouncementPriority::Action,
            true,
        ),
    };
    Announcement {
        text,
        priority,
        interruptible,
    }
}

#[cfg(target_os = "windows")]
const ACCESSKIT_ROOT_ID: accesskit::NodeId = accesskit::NodeId(0);
#[cfg(target_os = "windows")]
const ACCESSKIT_INITIAL_FOCUS_ID: accesskit::NodeId = accesskit::NodeId(1);

/// Publishes the application's existing semantic accessibility events through
/// UI Automation. Windows screen readers remain responsible for speech,
/// voices, punctuation and interruption policy.
#[cfg(target_os = "windows")]
pub struct WindowsAccessibilityAdapter {
    adapter: accesskit_winit::Adapter,
    state: std::sync::Arc<Mutex<AccessKitTreeState>>,
}

#[cfg(target_os = "windows")]
impl WindowsAccessibilityAdapter {
    pub fn new<T>(
        window: &winit::window::Window,
        proxy: winit::event_loop::EventLoopProxy<T>,
    ) -> Self
    where
        T: From<accesskit_winit::ActionRequestEvent> + Send + 'static,
    {
        let state = std::sync::Arc::new(Mutex::new(AccessKitTreeState::new()));
        let initial_state = std::sync::Arc::clone(&state);
        let adapter = accesskit_winit::Adapter::new(
            window,
            move || {
                initial_state
                    .lock()
                    .expect("AccessKit state poisoned")
                    .full_tree()
            },
            proxy,
        );
        Self { adapter, state }
    }

    pub fn process_event(&self, window: &winit::window::Window, event: &winit::event::WindowEvent) {
        self.adapter.process_event(window, event);
    }

    pub fn announce(&self, event: AccessibilityEvent) {
        let update = self
            .state
            .lock()
            .expect("AccessKit state poisoned")
            .apply_event(event);
        self.adapter.update_if_active(move || update);
    }
}

#[cfg(target_os = "windows")]
struct AccessKitTreeState {
    node_classes: accesskit::NodeClassSet,
    next_node_id: u64,
    focus_id: accesskit::NodeId,
    focus_node: accesskit::Node,
    live_node: Option<(accesskit::NodeId, accesskit::Node)>,
}

#[cfg(target_os = "windows")]
impl AccessKitTreeState {
    fn new() -> Self {
        let mut node_classes = accesskit::NodeClassSet::new();
        let mut focus = accesskit::NodeBuilder::new(accesskit::Role::Application);
        focus.set_name("Coquerythmo");
        focus.add_action(accesskit::Action::Focus);
        let focus_node = focus.build(&mut node_classes);
        Self {
            node_classes,
            next_node_id: 2,
            focus_id: ACCESSKIT_INITIAL_FOCUS_ID,
            focus_node,
            live_node: None,
        }
    }

    fn next_id(&mut self) -> accesskit::NodeId {
        let id = accesskit::NodeId(self.next_node_id);
        self.next_node_id = self.next_node_id.wrapping_add(1).max(2);
        id
    }

    fn root_node(&mut self) -> accesskit::Node {
        let mut root = accesskit::NodeBuilder::new(accesskit::Role::Window);
        root.set_name("Coquerythmo");
        root.push_child(self.focus_id);
        if let Some((id, _)) = &self.live_node {
            root.push_child(*id);
        }
        root.build(&mut self.node_classes)
    }

    fn full_tree(&mut self) -> accesskit::TreeUpdate {
        let mut nodes = vec![
            (ACCESSKIT_ROOT_ID, self.root_node()),
            (self.focus_id, self.focus_node.clone()),
        ];
        if let Some(live_node) = self.live_node.clone() {
            nodes.push(live_node);
        }
        let mut tree = accesskit::Tree::new(ACCESSKIT_ROOT_ID);
        tree.app_name = Some("Coquerythmo".to_string());
        tree.toolkit_name = Some("Coquerythmo custom UI".to_string());
        accesskit::TreeUpdate {
            nodes,
            tree: Some(tree),
            focus: self.focus_id,
        }
    }

    fn apply_event(&mut self, event: AccessibilityEvent) -> accesskit::TreeUpdate {
        match event {
            AccessibilityEvent::Focus { label, role } => {
                self.update_focus(label, accesskit_role(&role), None)
            }
            AccessibilityEvent::Selection { label } => {
                self.update_focus(label, accesskit::Role::ListBoxOption, None)
            }
            AccessibilityEvent::ValueChanged { label, value } => {
                self.update_focus(label, accesskit::Role::StaticText, Some(value))
            }
            event @ AccessibilityEvent::Error { .. } => {
                self.update_live(format_event(event).text, true)
            }
            event @ (AccessibilityEvent::Activation { .. }
            | AccessibilityEvent::Success { .. }
            | AccessibilityEvent::Opened { .. }
            | AccessibilityEvent::Closed { .. }
            | AccessibilityEvent::Collapsed { .. }) => {
                self.update_live(format_event(event).text, false)
            }
            AccessibilityEvent::CharacterTyped { .. }
            | AccessibilityEvent::CharacterDeleted { .. } => accesskit::TreeUpdate {
                nodes: Vec::new(),
                tree: None,
                focus: self.focus_id,
            },
        }
    }

    fn update_focus(
        &mut self,
        label: String,
        role: accesskit::Role,
        value: Option<String>,
    ) -> accesskit::TreeUpdate {
        let id = self.next_id();
        let mut node = accesskit::NodeBuilder::new(role);
        node.set_name(words_only(&label));
        if let Some(value) = value {
            node.set_value(words_only(&value));
        }
        node.add_action(accesskit::Action::Focus);
        self.focus_id = id;
        self.focus_node = node.build(&mut self.node_classes);
        self.live_node = None;
        accesskit::TreeUpdate {
            nodes: vec![
                (ACCESSKIT_ROOT_ID, self.root_node()),
                (self.focus_id, self.focus_node.clone()),
            ],
            tree: None,
            focus: self.focus_id,
        }
    }

    fn update_live(&mut self, text: String, assertive: bool) -> accesskit::TreeUpdate {
        let id = self.next_id();
        let mut node = accesskit::NodeBuilder::new(if assertive {
            accesskit::Role::Alert
        } else {
            accesskit::Role::StaticText
        });
        node.set_name(words_only(&text));
        node.set_live(if assertive {
            accesskit::Live::Assertive
        } else {
            accesskit::Live::Polite
        });
        let node = node.build(&mut self.node_classes);
        self.live_node = Some((id, node.clone()));
        accesskit::TreeUpdate {
            nodes: vec![(ACCESSKIT_ROOT_ID, self.root_node()), (id, node)],
            tree: None,
            focus: self.focus_id,
        }
    }
}

#[cfg(target_os = "windows")]
fn accesskit_role(role: &str) -> accesskit::Role {
    let role = role.to_ascii_lowercase();
    if role.contains("checkbox") {
        accesskit::Role::CheckBox
    } else if role.contains("button") {
        accesskit::Role::Button
    } else if role.contains("text") || role.contains("field") {
        accesskit::Role::TextInput
    } else if role.contains("list") {
        accesskit::Role::List
    } else {
        accesskit::Role::Group
    }
}

#[cfg(target_os = "macos")]
enum ActiveSpeech {
    Mac(Child),
}

#[cfg(target_os = "macos")]
fn speech_worker(receiver: Receiver<WorkerCommand>) {
    let mut active: Option<ActiveSpeech> = None;
    let mut queue = VecDeque::<Announcement>::new();
    let mut last_announcement = None;
    let mut resumable = false;
    loop {
        if active.as_mut().is_some_and(active_finished) {
            active = None;
        }
        if active.is_none() {
            if let Some(next) = pop_highest_priority(&mut queue) {
                match spawn_platform_speech(&next.text) {
                    Ok(child) => active = Some(child),
                    Err(error) => log::warn!("Narration backend error: {error}"),
                }
            }
        }
        match receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(WorkerCommand::Speak(next)) => {
                // Speech is latest-wins.  UI input can arrive much faster than
                // a sentence can be spoken; retaining older announcements
                // makes the screen reader describe stale state long after the
                // user moved on.  Stop the platform voice synchronously and
                // discard every pending item before accepting the new one.
                stop_active(&mut active);
                queue.clear();
                last_announcement = Some(next.clone());
                resumable = true;
                queue.push_back(next);
            }
            Ok(WorkerCommand::Stop {
                resumable: can_resume,
            }) => {
                if let Some(pending) = queue.back().cloned() {
                    last_announcement = Some(pending);
                }
                queue.clear();
                stop_active(&mut active);
                resumable = can_resume && last_announcement.is_some();
            }
            Ok(WorkerCommand::Resume) => {
                if resumable {
                    stop_active(&mut active);
                    queue.clear();
                    if let Some(last) = last_announcement.clone() {
                        queue.push_back(last);
                    }
                    resumable = false;
                }
            }
            Ok(WorkerCommand::Shutdown) => {
                stop_active(&mut active);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                stop_active(&mut active);
                return;
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn pop_highest_priority(queue: &mut VecDeque<Announcement>) -> Option<Announcement> {
    let index = queue
        .iter()
        .enumerate()
        .max_by_key(|(_, item)| item.priority)
        .map(|(index, _)| index)?;
    queue.remove(index)
}

#[cfg(target_os = "macos")]
fn active_finished(active: &mut ActiveSpeech) -> bool {
    match active {
        ActiveSpeech::Mac(child) => child.try_wait().ok().flatten().is_some(),
    }
}

#[cfg(target_os = "macos")]
fn stop_active(active: &mut Option<ActiveSpeech>) {
    let Some(active) = active.take() else {
        return;
    };
    match active {
        ActiveSpeech::Mac(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(target_os = "macos")]
fn spawn_platform_speech(text: &str) -> std::io::Result<ActiveSpeech> {
    Command::new("say")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(ActiveSpeech::Mac)
}

/// WSOLA-style overlap/add time stretching followed by an exact sample trim.
/// Frames are copied at their original rate, preserving pitch; only their
/// spacing changes.  The output is always exactly `target_samples` long.
pub fn fit_clip_exact(samples: Vec<f32>, target_samples: usize) -> Vec<f32> {
    if target_samples == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0.0; target_samples];
    }
    if samples.len() == target_samples {
        return samples;
    }
    const WINDOW: usize = 1024;
    const HOP: usize = WINDOW / 2;
    if samples.len() < WINDOW * 2 || target_samples < WINDOW {
        let mut output = samples;
        output.resize(target_samples, 0.0);
        output.truncate(target_samples);
        return output;
    }
    let mut output = vec![0.0; target_samples + WINDOW];
    let mut weights = vec![0.0; output.len()];
    let scale = samples.len() as f64 / target_samples as f64;
    let mut output_pos = 0usize;
    while output_pos < target_samples {
        let source_pos = ((output_pos as f64 * scale).round() as usize)
            .min(samples.len().saturating_sub(WINDOW));
        for index in 0..WINDOW {
            let phase = index as f32 / (WINDOW - 1) as f32;
            let window = 0.5 - 0.5 * (std::f32::consts::TAU * phase).cos();
            output[output_pos + index] += samples[source_pos + index] * window;
            weights[output_pos + index] += window;
        }
        output_pos += HOP;
    }
    for (sample, weight) in output.iter_mut().zip(weights) {
        if weight > 1.0e-6 {
            *sample /= weight;
        }
    }
    output.truncate(target_samples);
    output.resize(target_samples, 0.0);
    output
}

pub fn target_sample_count(duration_frames: i64, fps: f64, sample_rate: u32) -> usize {
    if duration_frames <= 0 || !fps.is_finite() || fps <= 0.0 {
        return 0;
    }
    (duration_frames as f64 / fps * sample_rate as f64)
        .round()
        .max(0.0) as usize
}

/// Mix media, UI announcements and simultaneous line voices without allowing
/// clipping. Media is ducked while any line is active; UI speech ducks lines.
pub fn mix_accessibility_audio(
    media: &[f32],
    ui: &[f32],
    lines: &[&[f32]],
    media_ducking: f32,
    line_ducking: f32,
) -> Vec<f32> {
    let len = std::iter::once(media.len())
        .chain(std::iter::once(ui.len()))
        .chain(lines.iter().map(|line| line.len()))
        .max()
        .unwrap_or(0);
    let has_lines = lines
        .iter()
        .any(|line| line.iter().any(|sample| sample.abs() > 1.0e-6));
    let has_ui = ui.iter().any(|sample| sample.abs() > 1.0e-6);
    (0..len)
        .map(|index| {
            let mut mixed = media.get(index).copied().unwrap_or(0.0)
                * if has_lines {
                    media_ducking.clamp(0.0, 1.0)
                } else {
                    1.0
                };
            let line_gain = if has_ui {
                line_ducking.clamp(0.0, 1.0)
            } else {
                1.0
            };
            mixed += lines
                .iter()
                .map(|line| line.get(index).copied().unwrap_or(0.0))
                .sum::<f32>()
                * line_gain;
            mixed += ui.get(index).copied().unwrap_or(0.0);
            // Smooth, deterministic limiter. It is monotonic and never exceeds 1.
            mixed / (1.0 + mixed.abs())
        })
        .collect()
}

pub fn voice_for_character<'a>(
    character_name: &str,
    language: &str,
    voices: &'a [VoiceDescriptor],
) -> Option<&'a VoiceDescriptor> {
    let language_base = language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase();
    let compatible: Vec<_> = voices
        .iter()
        .filter(|voice| {
            voice.language.as_ref().is_none_or(|voice_language| {
                voice_language
                    .to_ascii_lowercase()
                    .starts_with(&language_base)
            })
        })
        .collect();
    let pool = if compatible.is_empty() {
        voices.iter().collect()
    } else {
        compatible
    };
    if pool.is_empty() {
        return None;
    }
    let hash = character_name
        .bytes()
        .fold(1_469_598_103_934_665_603u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
        });
    Some(pool[hash as usize % pool.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_characters_are_never_exposed() {
        let spoken = format_event(AccessibilityEvent::CharacterTyped {
            character: Some('x'),
            secret: true,
        });
        assert!(!spoken.text.contains('x'));
    }

    #[test]
    fn spoken_selection_drops_decorative_symbols() {
        assert_eq!(words_only("Réactions ▸ 35 %"), "Réactions 35");
    }

    #[test]
    fn spoken_selection_drops_registered_trademark_and_icons() {
        assert_eq!(words_only("S® ™ © →"), "S");
    }

    #[test]
    fn exact_clip_length_for_two_and_a_half_seconds() {
        for sample_rate in [44_100usize, 48_000] {
            let target = (2.5 * sample_rate as f64).round() as usize;
            assert_eq!(fit_clip_exact(vec![0.25; 10_000], target).len(), target);
            assert_eq!(target_sample_count(60, 24.0, sample_rate as u32), target);
        }
    }

    #[test]
    fn mixer_ducks_and_limits_overlapping_voices() {
        let media = vec![1.0; 32];
        let ui = vec![0.5; 32];
        let line_a = vec![1.0; 32];
        let line_b = vec![1.0; 32];
        let mixed = mix_accessibility_audio(&media, &ui, &[&line_a, &line_b], 0.35, 0.60);
        assert_eq!(mixed.len(), 32);
        assert!(mixed.iter().all(|sample| sample.abs() < 1.0));
    }

    #[test]
    fn voice_assignment_is_deterministic_and_language_aware() {
        let voices = vec![
            VoiceDescriptor {
                id: "fr-1".into(),
                name: "A".into(),
                language: Some("fr-FR".into()),
            },
            VoiceDescriptor {
                id: "fr-2".into(),
                name: "B".into(),
                language: Some("fr-FR".into()),
            },
            VoiceDescriptor {
                id: "en".into(),
                name: "C".into(),
                language: Some("en-US".into()),
            },
        ];
        let first = voice_for_character("Alice", "fr-FR", &voices).unwrap();
        let again = voice_for_character("Alice", "fr-FR", &voices).unwrap();
        assert_eq!(first.id, again.id);
        assert!(first.id.starts_with("fr-"));
    }

    #[test]
    fn cache_is_bounded_and_invalidates_only_one_line() {
        let mut cache = SpeechClipCache::new(32);
        let key = |line_id| SpeechCacheKey {
            line_id,
            content_revision: 1,
            voice_id: "v".into(),
            language: "fr".into(),
            duration_samples: 4,
        };
        cache.insert(
            key(1),
            SpeechClip {
                samples: vec![0.0; 4],
                sample_rate: 48_000,
            },
        );
        cache.insert(
            key(2),
            SpeechClip {
                samples: vec![0.0; 4],
                sample_rate: 48_000,
            },
        );
        assert_eq!(cache.bytes(), 32);
        cache.invalidate_line(1);
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_some());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn accesskit_events_pause_and_resume_with_the_latest_event() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let narration = NarrationService::new(true, Some(sender));
        let first = AccessibilityEvent::Focus {
            label: "first".into(),
            role: "button".into(),
        };
        narration.announce_event(first.clone());
        assert_eq!(receiver.try_recv().unwrap(), first);

        narration.stop();
        let latest = AccessibilityEvent::Focus {
            label: "latest".into(),
            role: "button".into(),
        };
        narration.announce_event(latest.clone());
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));

        narration.resume();
        assert_eq!(receiver.try_recv().unwrap(), latest);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn priority_queue_prefers_errors() {
        let mut queue = VecDeque::from([
            Announcement {
                text: "focus".into(),
                priority: AnnouncementPriority::Navigation,
                interruptible: true,
            },
            Announcement {
                text: "error".into(),
                priority: AnnouncementPriority::Error,
                interruptible: false,
            },
        ]);
        assert_eq!(pop_highest_priority(&mut queue).unwrap().text, "error");
    }
}
