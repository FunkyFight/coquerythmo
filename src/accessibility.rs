//! Semantic announcements and the internal, interruptible speech worker.

use std::collections::{HashMap, VecDeque};
#[cfg(target_os = "macos")]
use std::process::Child;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
#[cfg(target_os = "windows")]
use windows::core::{HSTRING, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::Media::Speech::{
    ISpVoice, SpVoice, SPF_ASYNC, SPF_PURGEBEFORESPEAK, SPRS_IS_SPEAKING, SPVOICESTATUS,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

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

enum WorkerCommand {
    Speak(Announcement),
    Stop,
    Shutdown,
}

pub struct NarrationService {
    enabled: bool,
    available: bool,
    sender: Sender<WorkerCommand>,
    worker: Option<JoinHandle<()>>,
}

impl NarrationService {
    pub fn new(enabled: bool) -> Self {
        let available = cfg!(any(target_os = "windows", target_os = "macos"));
        let (sender, receiver) = mpsc::channel();
        let worker = available.then(|| {
            thread::Builder::new()
                .name("coquerythmo-narration".into())
                .spawn(move || speech_worker(receiver))
                .expect("spawn narration worker")
        });
        Self {
            enabled: enabled && available,
            available,
            sender,
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
        if !self.enabled {
            let _ = self.sender.send(WorkerCommand::Stop);
        }
        self.enabled
    }

    pub fn announce(&self, announcement: Announcement) {
        let mut announcement = announcement;
        announcement.text = words_only(&announcement.text);
        if self.enabled && !announcement.text.trim().is_empty() {
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
        self.announce(format_event(event));
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

enum ActiveSpeech {
    #[cfg(target_os = "windows")]
    Windows(WindowsSpeech),
    #[cfg(target_os = "macos")]
    Mac(Child),
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Unavailable,
}

fn speech_worker(receiver: Receiver<WorkerCommand>) {
    let mut active: Option<ActiveSpeech> = None;
    let mut queue = VecDeque::<Announcement>::new();
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
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(WorkerCommand::Speak(next)) => {
                // Speech is latest-wins.  UI input can arrive much faster than
                // a sentence can be spoken; retaining older announcements
                // makes the screen reader describe stale state long after the
                // user moved on.  Stop the platform voice synchronously and
                // discard every pending item before accepting the new one.
                stop_active(&mut active);
                queue.clear();
                queue.push_back(next);
            }
            Ok(WorkerCommand::Stop) => {
                queue.clear();
                stop_active(&mut active);
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

fn pop_highest_priority(queue: &mut VecDeque<Announcement>) -> Option<Announcement> {
    let index = queue
        .iter()
        .enumerate()
        .max_by_key(|(_, item)| item.priority)
        .map(|(index, _)| index)?;
    queue.remove(index)
}

fn active_finished(active: &mut ActiveSpeech) -> bool {
    match active {
        #[cfg(target_os = "windows")]
        ActiveSpeech::Windows(speech) => speech.finished(),
        #[cfg(target_os = "macos")]
        ActiveSpeech::Mac(child) => child.try_wait().ok().flatten().is_some(),
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        ActiveSpeech::Unavailable => true,
    }
}

fn stop_active(active: &mut Option<ActiveSpeech>) {
    let Some(active) = active.take() else {
        return;
    };
    match active {
        #[cfg(target_os = "windows")]
        ActiveSpeech::Windows(speech) => speech.stop(),
        #[cfg(target_os = "macos")]
        ActiveSpeech::Mac(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        ActiveSpeech::Unavailable => {}
    }
}

#[cfg(target_os = "windows")]
fn spawn_platform_speech(text: &str) -> std::io::Result<ActiveSpeech> {
    let mut speech = WindowsSpeech::new()?;
    speech.speak(text)?;
    Ok(ActiveSpeech::Windows(speech))
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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn spawn_platform_speech(_text: &str) -> std::io::Result<ActiveSpeech> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "speech is unavailable on this platform",
    ))
}

#[cfg(target_os = "windows")]
struct WindowsSpeech {
    voice: ISpVoice,
    started: Instant,
}

#[cfg(target_os = "windows")]
impl WindowsSpeech {
    fn new() -> std::io::Result<Self> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if !initialized.is_ok() {
            return Err(std::io::Error::other(format!(
                "COM initialization failed: {initialized:?}"
            )));
        }
        let voice = unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| std::io::Error::other(format!("SAPI unavailable: {error:?}")));
        match voice {
            Ok(voice) => Ok(Self {
                voice,
                started: Instant::now(),
            }),
            Err(error) => {
                unsafe { CoUninitialize() };
                Err(error)
            }
        }
    }

    fn speak(&mut self, text: &str) -> std::io::Result<()> {
        let text = HSTRING::from(text);
        unsafe {
            self.voice
                .Speak(&text, (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32, None)
                .map_err(|error| std::io::Error::other(format!("SAPI speak failed: {error:?}")))?;
        }
        self.started = Instant::now();
        Ok(())
    }

    fn finished(&self) -> bool {
        // SAPI reports NOT_STARTED briefly while an asynchronous utterance is
        // being queued.  Do not drop the COM voice during that window or
        // short action/line announcements can disappear completely.
        if self.started.elapsed() < Duration::from_millis(100) {
            return false;
        }
        let mut status = SPVOICESTATUS::default();
        unsafe {
            self.voice
                .GetStatus(&mut status, std::ptr::null_mut())
                .is_ok()
                && status.dwRunningState != SPRS_IS_SPEAKING.0 as u32
        }
    }

    fn stop(&self) {
        let _ = unsafe {
            self.voice
                .Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK.0 as u32, None)
        };
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsSpeech {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
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
