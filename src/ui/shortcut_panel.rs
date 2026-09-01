//! Compact contextual shortcut list anchored to the bottom-left corner.
//!
//! The panel lists only the bindings that can actually fire in the current
//! situation: a line-dependent shortcut shows up only when a line is
//! selected, playback keys only when a media is loaded, detection chords only
//! when a detection is selected, and so on. When the list exceeds the
//! viewport, a scrollbar appears and the wheel scrolls the panel while the
//! cursor hovers it. Every label is clipped to the panel bounds so text never
//! spills outside the box.

use std::sync::OnceLock;

use winit::keyboard::KeyLocation;

use super::primitives::{HAlign, LabelInfo, Overflow, QuadInstance, Rect, UiEvent, VAlign};
use crate::application::command::UiAction;
use crate::application::workspace_service::WorkspaceId;
use crate::input::binding::{Binding, KeyPattern};
use crate::input::context::InputContext;
use crate::input::key::{InputWindow, KeyCode, KeyStroke};
use crate::input::router::ShortcutRouter;

const PANEL_MARGIN: f32 = 8.0;
const PANEL_PADDING_H: f32 = 10.0;
const PANEL_PADDING_V: f32 = 6.0;
const PANEL_LINE_GAP: f32 = 2.0;
const PANEL_RADIUS: f32 = 6.0;
const PANEL_MAX_WIDTH: f32 = 420.0;
/// Lines visible at once; extra lines are reachable through the scrollbar.
const PANEL_VISIBLE_LINES: usize = 7;
/// The panel is a passive hint, so it renders smaller than body text.
const PANEL_FONT_SCALE: f32 = 0.85;
/// Approximate width of one glyph, as a fraction of the font size.
const GLYPH_WIDTH_RATIO: f32 = 0.56;
const SCROLLBAR_WIDTH: f32 = 3.0;
const SCROLLBAR_INSET: f32 = 5.0;
const SCROLLBAR_MIN_THUMB: f32 = 24.0;

/// The shortcut table is static for the whole process lifetime; building it
/// once avoids re-allocating the ~70 bindings on every rendered frame.
pub fn shortcut_router() -> &'static ShortcutRouter<UiAction> {
    static ROUTER: OnceLock<ShortcutRouter<UiAction>> = OnceLock::new();
    ROUTER.get_or_init(crate::input::router::existing_shortcuts)
}

/// Fine-grained facts about the current UI situation. This is the cache key
/// of the panel: any fact change rebuilds the listed shortcuts.
#[derive(Clone, PartialEq)]
pub struct PanelSituation {
    /// Router context stack (modal, text editing, workspace…).
    pub contexts: Vec<InputContext>,
    pub workspace: WorkspaceId,
    /// A media (video or audio-only) is loaded: playback keys are usable.
    pub has_video: bool,
    /// An instrumental track exists: Ctrl+Tab can toggle audio tracks.
    pub has_instrumental: bool,
    /// A line selection exists (`Selection::Line`/`Lines`/`AllLines`).
    pub line_selected: bool,
    /// Any rythmo selection exists (line, marker, detection, strokes).
    pub any_selection: bool,
    /// A detection is selected: detection chords are usable.
    pub detection_selected: bool,
    /// The pointer hovers a line (karaoke toggle accepts hover fallback).
    pub hovered_line: bool,
    /// A line text editor is open.
    pub editing_line: bool,
    /// Any rythmo editor is open (text, character, note).
    pub editing_any: bool,
    /// At least one line exists in the project: line navigation is usable.
    pub has_lines: bool,
    /// A line covers the playhead: Enter can cycle selections.
    pub line_at_playhead: bool,
    /// The internal line clipboard holds at least one entry: paste is usable.
    pub line_clipboard_available: bool,
}

/// Whether a binding can actually fire in this situation. Bindings that
/// would resolve but do nothing (no selection, no media, no clipboard…) are
/// hidden so the panel stays precise and compact.
fn is_applicable(action: &UiAction, s: &PanelSituation) -> bool {
    match action {
        UiAction::SetSelectedLineStartAtPlayhead
        | UiAction::SetSelectedLineEndAtPlayhead
        | UiAction::StartEditingSelectedLine
        | UiAction::StartEditingSelectedCharacter
        | UiAction::MoveSelectedLineTrack { .. }
        | UiAction::NudgeSelectedLines { .. } => s.line_selected,
        UiAction::ClearLineSelection => s.any_selection,
        UiAction::CopySelectedLine | UiAction::CutSelectedLine => s.line_selected,
        UiAction::PasteLine => s.line_clipboard_available,
        UiAction::ToggleKaraokeForSelection => s.line_selected || s.hovered_line,
        UiAction::SplitDialogue => s.editing_line || s.line_selected,
        UiAction::OpenTextEmotionMenu => s.editing_any || s.line_selected,
        UiAction::NudgeSelectedDetection { .. } => s.detection_selected,
        UiAction::AddSyncPointAtPlayhead => {
            s.editing_line || s.line_selected || s.detection_selected
        }
        UiAction::SelectLineAtPlayhead => s.line_at_playhead,
        UiAction::NavigateLines { .. } => s.has_lines,
        UiAction::TogglePlayPause
        | UiAction::PrevFrame
        | UiAction::NextFrame
        | UiAction::BeginKeyboardPan { .. }
        | UiAction::AdjustVolume(_)
        | UiAction::ToggleMute => s.has_video,
        UiAction::ToggleActiveAudio => s.has_instrumental,
        _ => true,
    }
}

/// Localized `(keystroke, action)` lines for a situation. Bindings are
/// listed from the most specific context to the most general, in the same
/// priority order as shortcut resolution. Actions sharing a name (arrow
/// moves, copy at text and line level…) are merged into one line whose
/// chords are joined with `/`.
pub fn build_lines(
    situation: &PanelSituation,
    bindings: &[Binding<UiAction>],
) -> Vec<(String, String)> {
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut push = |chord: String, label: String| {
        if let Some(existing) = lines.iter_mut().find(|(_, name)| *name == label) {
            if !existing.0.split('/').any(|part| part == chord) {
                existing.0.push('/');
                existing.0.push_str(&chord);
            }
            return;
        }
        lines.push((chord, label));
    };

    for context in &situation.contexts {
        for binding in bindings
            .iter()
            .filter(|binding| binding.context == *context && binding.pattern.pressed)
        {
            if !is_applicable(&binding.command, situation) {
                continue;
            }
            let Some(label) = action_label(&binding.command) else {
                continue;
            };
            push(compact_keystroke(&binding.pattern), label);
        }
    }

    // Handled outside the router: Delete removes the current rythmo
    // selection (controller_base.rs → UiAction::DeleteSelected).
    if situation.any_selection
        && situation
            .contexts
            .iter()
            .any(|context| *context == InputContext::Workspace)
    {
        push(
            crate::i18n::t("shortcut.delete").to_string(),
            crate::i18n::t("panel.delete_selection").to_string(),
        );
    }

    // No router binding exists for modals: list the navigation controls that
    // every modal supports.
    if situation
        .contexts
        .iter()
        .any(|context| *context == InputContext::Modal)
    {
        let tab = crate::i18n::t("shortcut.tab");
        push(
            format!("{tab}/Maj+{tab}"),
            crate::i18n::t("panel.modal_next_control").to_string(),
        );
        push(
            "↑/↓/←/→".to_string(),
            crate::i18n::t("panel.modal_adjust").to_string(),
        );
        push(
            format!(
                "{}/{}",
                crate::i18n::t("shortcut.enter"),
                crate::i18n::t("shortcut.space")
            ),
            crate::i18n::t("panel.modal_activate").to_string(),
        );
        push(
            crate::i18n::t("shortcut.escape").to_string(),
            crate::i18n::t("panel.modal_close").to_string(),
        );
    }

    lines
}

/// Semantic action name. Narration labels are reused first; panel-only
/// actions (navigation helpers, continuous pan, caret moves) fall back to
/// their own names so everything doable in the context shows up.
fn action_label(action: &UiAction) -> Option<String> {
    if let Some(crate::accessibility::AccessibilityEvent::Activation { label }) =
        crate::accessibility::event_for_action(action)
    {
        return Some(label);
    }
    let key = crate::accessibility::panel_label_key_for_action(action)?;
    Some(crate::i18n::t(key).to_string())
}

/// Compact, localized chord text such as `Contrôle+S` (no announcement
/// prefix, no spaces around `+`, arrow keys rendered as glyphs).
fn compact_keystroke(pattern: &KeyPattern) -> String {
    let arrow = match pattern.key {
        KeyCode::ArrowLeft => Some(("←", crate::i18n::t("shortcut.arrow_left"))),
        KeyCode::ArrowRight => Some(("→", crate::i18n::t("shortcut.arrow_right"))),
        KeyCode::ArrowUp => Some(("↑", crate::i18n::t("shortcut.arrow_up"))),
        KeyCode::ArrowDown => Some(("↓", crate::i18n::t("shortcut.arrow_down"))),
        _ => None,
    };
    let stroke = KeyStroke {
        key: pattern.key,
        physical_key: None,
        location: KeyLocation::Standard,
        modifiers: pattern.modifiers,
        pressed: true,
        repeat: false,
        window: InputWindow::Main,
    };
    let full = stroke.accessibility_label();
    let prefix = crate::i18n::t("shortcut.prefix");
    let body = full
        .strip_prefix(prefix)
        .map(str::trim_start)
        .unwrap_or(full.as_str());
    let compact = body.replace(" + ", "+");
    match arrow {
        Some((glyph, long)) => compact.replace(long, glyph),
        None => compact,
    }
}

/// Owned, cached panel state. Rebuilt only when the situation changes.
pub struct ShortcutPanelState {
    situation_key: Option<PanelSituation>,
    lines: Vec<(String, String)>,
    /// Pixel offset of the first visible line, clamped to the overflow.
    scroll_offset: f32,
}

impl ShortcutPanelState {
    pub fn new() -> Self {
        Self {
            situation_key: None,
            lines: Vec::new(),
            scroll_offset: 0.0,
        }
    }

    /// Refresh the cached lines when the situation changed.
    pub fn sync(&mut self, situation: &PanelSituation, bindings: &[Binding<UiAction>]) {
        if self.situation_key.as_ref() == Some(situation) {
            return;
        }
        self.situation_key = Some(situation.clone());
        self.lines = build_lines(situation, bindings);
        self.scroll_offset = 0.0;
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn font_size() -> f32 {
        crate::config::get().ui.font_size * PANEL_FONT_SCALE
    }

    fn line_height() -> f32 {
        Self::font_size() + PANEL_LINE_GAP
    }

    fn content_height(&self) -> f32 {
        self.lines.len() as f32 * Self::line_height() - PANEL_LINE_GAP
    }

    fn viewport_height(&self) -> f32 {
        self.lines.len().min(PANEL_VISIBLE_LINES) as f32 * Self::line_height() - PANEL_LINE_GAP
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport_height()).max(0.0)
    }

    pub fn rect(&self, screen_h: f32) -> Rect {
        let font_size = Self::font_size();
        let longest = self
            .lines
            .iter()
            .map(|(chord, action)| chord.chars().count() + 3 + action.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let scrollbar_allowance = if self.max_scroll() > 0.0 {
            SCROLLBAR_INSET + SCROLLBAR_WIDTH
        } else {
            0.0
        };
        let width = (longest * font_size * GLYPH_WIDTH_RATIO
            + PANEL_PADDING_H * 2.0
            + scrollbar_allowance)
            .clamp(0.0, PANEL_MAX_WIDTH);
        let height = self.viewport_height() + PANEL_PADDING_V * 2.0;
        Rect {
            x: PANEL_MARGIN,
            y: screen_h - PANEL_MARGIN - height,
            width,
            height,
        }
    }

    /// Scroll the list when the wheel is used over the panel rect. Returns
    /// true when the event was consumed.
    pub fn handle_scroll(&mut self, event: &UiEvent, screen_h: f32) -> bool {
        let UiEvent::Scroll { x, y, delta, .. } = event else {
            return false;
        };
        if self.lines.is_empty() || !self.rect(screen_h).contains(*x, *y) {
            return false;
        }
        let step = Self::line_height();
        self.scroll_offset = (self.scroll_offset - delta * step).clamp(0.0, self.max_scroll());
        true
    }

    pub fn render_quads(&self, screen_h: f32) -> Vec<QuadInstance> {
        if self.lines.is_empty() {
            return Vec::new();
        }
        let rect = self.rect(screen_h);
        let mut quads = vec![QuadInstance {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: [0.13, 0.13, 0.15, 0.72],
            color_bottom: [0.10, 0.10, 0.12, 0.72],
            border_color: [0.30, 0.30, 0.36, 0.5],
            border_width: 1.0,
            border_radius: PANEL_RADIUS,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        }];
        let max_scroll = self.max_scroll();
        if max_scroll > 0.0 {
            let track_h = rect.height - SCROLLBAR_INSET * 2.0;
            let thumb_h = (track_h * (self.viewport_height() / self.content_height()))
                .clamp(SCROLLBAR_MIN_THUMB.min(track_h), track_h);
            let thumb_y = rect.y
                + SCROLLBAR_INSET
                + (track_h - thumb_h) * (self.scroll_offset / max_scroll);
            quads.push(QuadInstance {
                rect: [
                    rect.x + rect.width - SCROLLBAR_INSET - SCROLLBAR_WIDTH,
                    thumb_y,
                    SCROLLBAR_WIDTH,
                    thumb_h,
                ],
                color: [0.70, 0.70, 0.78, 0.45],
                color_bottom: [0.70, 0.70, 0.78, 0.45],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: SCROLLBAR_WIDTH / 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        quads
    }

    /// Intersect a label box with the panel interior so partially visible
    /// lines are clipped instead of spilling outside the box.
    fn clip_to_panel(&self, bounds: Rect, panel: Rect) -> Option<Rect> {
        let x0 = bounds.x.max(panel.x);
        let y0 = bounds.y.max(panel.y);
        let x1 = (bounds.x + bounds.width).min(panel.x + panel.width);
        let y1 = (bounds.y + bounds.height).min(panel.y + panel.height);
        (x1 > x0 && y1 > y0).then(|| Rect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    pub fn render_labels(&self, screen_h: f32) -> Vec<LabelInfo<'_>> {
        if self.lines.is_empty() {
            return Vec::new();
        }
        let font_size = Self::font_size();
        let line_height = Self::line_height();
        let rect = self.rect(screen_h);
        let clip = Rect {
            x: rect.x,
            y: rect.y + 1.0,
            width: rect.width,
            height: rect.height - 2.0,
        };
        let viewport_top = rect.y + PANEL_PADDING_V;
        let viewport_bottom = viewport_top + self.viewport_height();
        let first_line = (self.scroll_offset / line_height).floor() as usize;
        let mut labels = Vec::new();
        for (index, (chord, action)) in self.lines.iter().enumerate().skip(first_line) {
            let y = viewport_top + index as f32 * line_height - self.scroll_offset;
            if y >= viewport_bottom {
                break;
            }
            let chord_width = (chord.chars().count() + 1) as f32 * font_size * GLYPH_WIDTH_RATIO;
            let action_x = rect.x + PANEL_PADDING_H + chord_width + 8.0;
            if let Some(bounds) = self.clip_to_panel(
                Rect {
                    x: rect.x + PANEL_PADDING_H,
                    y,
                    width: chord_width,
                    height: font_size,
                },
                clip,
            ) {
                labels.push(LabelInfo {
                    text: chord,
                    bounds,
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(font_size),
                    color_override: Some([140, 180, 255]),
                    font_family_override: None,
                });
            }
            let action_width = rect.x + rect.width
                - PANEL_PADDING_H
                - action_x
                - if self.max_scroll() > 0.0 {
                    SCROLLBAR_INSET + SCROLLBAR_WIDTH
                } else {
                    0.0
                };
            if action_width > 4.0 {
                if let Some(bounds) = self.clip_to_panel(
                    Rect {
                        x: action_x,
                        y,
                        width: action_width,
                        height: font_size,
                    },
                    clip,
                ) {
                    labels.push(LabelInfo {
                        text: action,
                        bounds,
                        h_align: HAlign::Left,
                        v_align: VAlign::Center,
                        overflow: Overflow::Ellipsis,
                        padding: 0.0,
                        font_size_override: Some(font_size),
                        color_override: Some([205, 205, 215]),
                        font_family_override: None,
                    });
                }
            }
        }
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn situation(contexts: &[InputContext]) -> PanelSituation {
        PanelSituation {
            contexts: contexts.to_vec(),
            workspace: WorkspaceId::Rythmo,
            has_video: true,
            has_instrumental: true,
            line_selected: false,
            any_selection: false,
            detection_selected: false,
            hovered_line: false,
            editing_line: false,
            editing_any: false,
            has_lines: true,
            line_at_playhead: true,
            line_clipboard_available: false,
        }
    }

    fn labels_of(lines: &[(String, String)]) -> Vec<&str> {
        lines.iter().map(|(_, action)| action.as_str()).collect()
    }

    #[test]
    fn line_shortcuts_only_show_with_a_line_selection() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::Workspace, InputContext::Global]);
        let without = build_lines(&s, router.bindings());
        let names = labels_of(&without);
        let edit = crate::i18n::t("accessibility.edit_line");
        assert!(!names.contains(&edit));

        s.line_selected = true;
        s.any_selection = true;
        let with = build_lines(&s, router.bindings());
        let names = labels_of(&with);
        assert!(names.contains(&edit));
        // Selection-dependent actions free space when nothing is selected.
        assert!(with.len() > without.len());
    }

    #[test]
    fn paste_only_shows_with_a_filled_clipboard() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::Workspace, InputContext::Global]);
        assert!(!labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("accessibility.paste")));
        s.line_clipboard_available = true;
        assert!(labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("accessibility.paste")));
    }

    #[test]
    fn playback_keys_require_a_loaded_media() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::Workspace, InputContext::Global]);
        assert!(labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.play_pause")));
        s.has_video = false;
        assert!(!labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.play_pause")));
    }

    #[test]
    fn detection_chords_only_show_with_a_detection_selected() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::Workspace, InputContext::Global]);
        assert!(!labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.audition_detection")));
        s.detection_selected = true;
        s.any_selection = true;
        assert!(labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.audition_detection")));
    }

    #[test]
    fn delete_selection_entry_shows_with_any_selection() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::Workspace, InputContext::Global]);
        assert!(!labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.delete_selection")));
        s.any_selection = true;
        assert!(labels_of(&build_lines(&s, router.bindings()))
            .contains(&crate::i18n::t("panel.delete_selection")));
    }

    #[test]
    fn modal_context_lists_modal_navigation_controls() {
        let router = shortcut_router();
        let lines = build_lines(&situation(&[InputContext::Modal]), router.bindings());
        let names = labels_of(&lines);
        assert!(names.contains(&crate::i18n::t("panel.modal_next_control")));
        assert!(names.contains(&crate::i18n::t("panel.modal_activate")));
        assert!(names.contains(&crate::i18n::t("panel.modal_close")));
    }

    #[test]
    fn empty_context_stack_hides_the_panel() {
        let router = shortcut_router();
        assert!(build_lines(&situation(&[]), router.bindings()).is_empty());
    }

    #[test]
    fn caret_moves_merge_into_one_line_with_glyph_chords() {
        let router = shortcut_router();
        let mut s = situation(&[InputContext::TextEditing]);
        s.editing_any = true;
        let lines = build_lines(&s, router.bindings());
        let (chord, _) = lines
            .iter()
            .find(|(_, action)| action == crate::i18n::t("panel.move_cursor"))
            .expect("caret moves are listed");
        assert!(chord.contains('←') && chord.contains('→'));
        assert!(chord.contains('/'));
    }

    #[test]
    fn compact_chord_has_no_announcement_prefix() {
        let router = shortcut_router();
        let lines = build_lines(&situation(&[InputContext::Global]), router.bindings());
        for (chord, _) in lines {
            assert!(!chord.starts_with(crate::i18n::t("shortcut.prefix")));
            assert!(!chord.contains(" + "));
        }
    }

    #[test]
    fn scroll_is_clamped_to_the_overflow() {
        crate::config::init();
        let mut panel = ShortcutPanelState::new();
        panel.sync(
            &situation(&[InputContext::Workspace, InputContext::Global]),
            shortcut_router().bindings(),
        );
        assert!(panel.max_scroll() > 0.0);

        let rect = panel.rect(720.0);
        let scroll = |delta| UiEvent::Scroll {
            x: rect.x + 4.0,
            y: rect.y + 4.0,
            delta,
            fast: false,
            ctrl: false,
        };
        assert!(panel.handle_scroll(&scroll(-100.0), 720.0));
        assert_eq!(panel.scroll_offset, panel.max_scroll());
        assert!(panel.handle_scroll(&scroll(100.0), 720.0));
        assert_eq!(panel.scroll_offset, 0.0);
        assert!(!panel.handle_scroll(
            &UiEvent::Scroll {
                x: rect.x + rect.width + 40.0,
                y: rect.y,
                delta: -1.0,
                fast: false,
                ctrl: false,
            },
            720.0
        ));
    }

    #[test]
    fn labels_stay_inside_the_panel_when_scrolled() {
        crate::config::init();
        let mut panel = ShortcutPanelState::new();
        panel.sync(
            &situation(&[InputContext::Workspace, InputContext::Global]),
            shortcut_router().bindings(),
        );
        let rect = panel.rect(720.0);
        panel.scroll_offset = panel.max_scroll() / 2.0;
        for label in panel.render_labels(720.0) {
            assert!(label.bounds.y >= rect.y - 0.01);
            assert!(label.bounds.y + label.bounds.height <= rect.y + rect.height + 0.01);
            assert!(label.bounds.x >= rect.x - 0.01);
            assert!(label.bounds.x + label.bounds.width <= rect.x + rect.width + 0.01);
        }
    }
}
