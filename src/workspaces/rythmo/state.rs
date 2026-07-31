//! Ephemeral interaction state owned by the rythmo workspace view.

use super::*;

/// What is currently selected in the BR.
#[derive(Clone, PartialEq, Debug)]
pub enum Selection {
    Line(u64),
    Lines(Vec<u64>),
    Marker(usize),
    Detection(crate::detection::DetectionAddress),
    AllLines,
    Strokes(Vec<u64>),
}

/// Ghost line preview shown when holding click on empty BR space.
pub struct GhostPreview {
    pub frame: i64,
    pub y_slot: f32,
    pub duration_frames: i64,
}

#[derive(Clone, Copy)]
pub struct SelectionDrag {
    pub rect: Rect,
    pub additive: bool,
}

pub struct VoiceActorIconDraw {
    pub actor_name: String,
    pub rect: Rect,
}

pub struct LineContextMenu {
    pub line_id: u64,
    pub x: f32,
    pub y: f32,
    pub hover_main: bool,
    pub hover_change_character: bool,
    pub hover_text_emotion: bool,
    pub hover_emotion_index: Option<usize>,
    pub hover_emotion_variant: Option<usize>,
    pub text_range: Option<(usize, usize)>,
    pub hover_actor_index: Option<usize>,
    pub hover_action_index: Option<usize>,
    pub actor_scroll: f32,
}

#[derive(Clone, Copy, Debug)]
struct KaraokeTextWidthCacheEntry {
    text_hash: u64,
    text_len: usize,
    font_size_bits: u32,
    width: f32,
}

#[derive(Clone, Debug)]
struct SyllableVisualRatiosCacheEntry {
    signature: u64,
    ratios: Vec<f32>,
}

struct CachedKaraokeUiIndex {
    signature: u64,
    index: KaraokeUiIndex,
}

struct CachedLintDiagnostics {
    project_revision: u64,
    fps_bits: u64,
    diagnostics: Vec<crate::lint::Diagnostic>,
    severity_by_line: HashMap<u64, crate::lint::Severity>,
    zone_diagnostics: Vec<crate::lint::Diagnostic>,
}

#[derive(Default)]
struct KaraokeWidthPrewarmState {
    signature: u64,
    cursor: usize,
    complete: bool,
}

#[derive(Default)]
struct LeadingVisualSpanCache {
    signature: u64,
    span: f32,
}

pub struct RythmoState {
    pub hovered_line: Option<u64>,
    pub hovered_track: Option<usize>,
    /// Timeline frame under the pointer, used by keyboard paste.
    pub hovered_frame: Option<i64>,
    /// Track used by keyboard-only line creation and playhead cycling.
    pub keyboard_track: usize,
    pub keyboard_cycle_frame: Option<i64>,
    pub selected: Option<Selection>,
    pub editing_line: Option<u64>,
    pub line_input: crate::ui::text_input::TextInputState,
    pub(crate) line_lowercase_override: bool,
    pub editing_character: Option<u64>,
    pub char_input: crate::ui::text_input::TextInputState,
    pub editing_note: Option<u64>,
    pub note_input: crate::ui::text_input::TextInputState,
    pub color_picker: crate::ui::color_picker::ColorPickerState,
    pub autocomplete_index: Option<usize>,
    pub autocomplete_hover: Option<usize>,
    pub autocomplete_scroll: usize,
    pub dragging: Option<DragState>,
    pub ghost_preview: Option<GhostPreview>,
    pub ctrl_held: bool,
    pub panning: bool,
    pub audio_offset_mode: bool,
    pub audio_offset_drag: Option<AudioOffsetDrag>,
    pub pending_cursor_click: Option<(f32, bool)>, // (x_ratio, is_shift_click)
    pub pan_last_x: f32,
    pub pan_accum: f32,
    pub keyboard_pan_direction: i32,
    pub keyboard_pan_last_tick: Option<std::time::Instant>,
    pub keyboard_pan_accum_px: f32,
    pub compact_empty_tracks: bool,
    pub syllable_drag: Option<SyllableDrag>,
    pub context_menu: Option<LineContextMenu>,
    pub detection_hover: Option<DetectionHover>,
    pub detection_menu: Option<DetectionMenu>,
    pub detection_drag: Option<DetectionDrag>,
    pub active_stroke: Option<crate::rythmo_drawing::DrawingStroke>,
    pub drawing_dirty: bool,
    pub selection_drag: Option<SelectionDrag>,
    pub transform_handle: Option<TransformHandle>,
    karaoke_text_width_cache: RefCell<HashMap<u64, KaraokeTextWidthCacheEntry>>,
    karaoke_index_cache: RefCell<Option<CachedKaraokeUiIndex>>,
    lint_diagnostics_cache: RefCell<Option<CachedLintDiagnostics>>,
    karaoke_width_prewarm: RefCell<KaraokeWidthPrewarmState>,
    cached_layout_signature: RefCell<u64>,
    cached_layout_ctx: RefCell<Option<EditorLayoutCtx>>,
    syllable_breaks_cache: RefCell<HashMap<u64, (Vec<usize>, u64)>>, // line_id -> (breaks, text_hash)
    syllable_visual_ratios_cache: RefCell<HashMap<u64, SyllableVisualRatiosCacheEntry>>,
    leading_visual_span_cache: RefCell<LeadingVisualSpanCache>,
    text_emotion_epoch: std::time::Instant,
    has_text_emotions: std::cell::Cell<bool>,
}

impl Default for RythmoState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SyllableDrag {
    pub line_id: u64,
    pub separator_index: usize, // which separator is being dragged (0 = between syl 0 and 1)
    pub ratios: Vec<f32>,       // working copy of ratios
    pub drag_start_x: f32,
    pub line_rect: Rect,
    /// Ctrl+click keeps every boundary before the selected handle fixed.
    pub preserve_prefix: bool,
}

#[derive(Clone, Copy)]
pub struct AudioOffsetDrag {
    pub last_x: f32,
    pub accum_px: f32,
}

#[derive(Clone, Debug)]
pub struct CursorSegmentInfo {
    pub cache_id: u64,
    pub start_char: usize,
    pub end_char: usize,
    pub start_ratio: f32,
    pub width_ratio: f32,
}

pub struct DragState {
    pub target: DragTarget,
    pub drag_start_x: f32,
    pub original_frame: i64,
    // For lines only:
    pub original_duration: i64,
    pub original_y_slot: f32,
    pub drag_start_y: f32,
    pub handle: DragHandle,
    pub group_origins: Vec<DragLineOrigin>,
}

#[derive(Clone, Copy)]
pub struct DragLineOrigin {
    pub line_id: u64,
    pub original_frame: i64,
    pub original_y_slot: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DragTarget {
    Line(u64),
    Marker(usize),
}

#[derive(Clone, Copy, PartialEq)]
pub enum DragHandle {
    Left,
    Right,
    Body,
    VerticalOnly,
    Selection,
}

/// Transform handle for stroke selection bounding box
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformHandleKind {
    /// Top-left corner
    TopLeft,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-right corner
    BottomRight,
    /// Rotate handle (top-center, outside bbox)
    Rotate,
    /// Move handle (bbox body)
    Move,
}

#[derive(Clone, Debug)]
pub struct TransformHandle {
    pub kind: TransformHandleKind,
    pub start_mouse: (f32, f32),
    pub start_bbox: (f64, f32, f64, f32), // min_frame, min_y, max_frame, max_y in frame-space
    pub start_stroke_points: Vec<Vec<(f64, f32)>>, // original points for each stroke
    pub current_stroke_points: Vec<Vec<(f64, f32)>>, // local preview after the last drag event
    pub stroke_ids: Vec<u64>,
}

impl RythmoState {
    pub fn new() -> Self {
        Self {
            hovered_line: None,
            hovered_track: None,
            hovered_frame: None,
            keyboard_track: 0,
            keyboard_cycle_frame: None,
            selected: None,
            editing_line: None,
            line_input: crate::ui::text_input::TextInputState::new(),
            line_lowercase_override: false,
            editing_character: None,
            char_input: crate::ui::text_input::TextInputState::new(),
            editing_note: None,
            note_input: crate::ui::text_input::TextInputState::new(),
            color_picker: crate::ui::color_picker::ColorPickerState::new(),
            autocomplete_index: None,
            autocomplete_hover: None,
            autocomplete_scroll: 0,
            dragging: None,
            ghost_preview: None,
            ctrl_held: false,
            panning: false,
            audio_offset_mode: false,
            audio_offset_drag: None,
            pending_cursor_click: None,
            pan_last_x: 0.0,
            pan_accum: 0.0,
            keyboard_pan_direction: 0,
            keyboard_pan_last_tick: None,
            keyboard_pan_accum_px: 0.0,
            compact_empty_tracks: false,
            syllable_drag: None,
            context_menu: None,
            detection_hover: None,
            detection_menu: None,
            detection_drag: None,
            active_stroke: None,
            drawing_dirty: false,
            selection_drag: None,
            transform_handle: None,
            karaoke_text_width_cache: RefCell::new(HashMap::new()),
            karaoke_index_cache: RefCell::new(None),
            lint_diagnostics_cache: RefCell::new(None),
            karaoke_width_prewarm: RefCell::new(KaraokeWidthPrewarmState::default()),
            cached_layout_signature: RefCell::new(0),
            cached_layout_ctx: RefCell::new(None),
            syllable_breaks_cache: RefCell::new(HashMap::new()),
            syllable_visual_ratios_cache: RefCell::new(HashMap::new()),
            leading_visual_span_cache: RefCell::new(LeadingVisualSpanCache::default()),
            text_emotion_epoch: std::time::Instant::now(),
            has_text_emotions: std::cell::Cell::new(false),
        }
    }

    pub(super) fn cached_karaoke_ui_index(
        &self,
        project: &Project,
        max_gap_frames: i64,
    ) -> Ref<'_, KaraokeUiIndex> {
        let signature = karaoke_ui_index_revision_signature(project, max_gap_frames);
        {
            let cache = self.karaoke_index_cache.borrow();
            if cache
                .as_ref()
                .is_some_and(|cached| cached.signature == signature)
            {
                return Ref::map(cache, |cache| &cache.as_ref().unwrap().index);
            }
        }

        let index = KaraokeUiIndex::new_with_signature(project, max_gap_frames, signature);
        *self.karaoke_index_cache.borrow_mut() = Some(CachedKaraokeUiIndex { signature, index });
        Ref::map(self.karaoke_index_cache.borrow(), |cache| {
            &cache.as_ref().unwrap().index
        })
    }

    /// Project-wide linting is O(lines). It changes after a domain edit or an
    /// FPS change, not merely because the playhead moved.
    pub(crate) fn cached_lint_diagnostics(
        &self,
        project: &Project,
        fps: f64,
    ) -> Ref<'_, Vec<crate::lint::Diagnostic>> {
        let project_revision = project.revision();
        let fps_bits = fps.to_bits();
        let valid = self
            .lint_diagnostics_cache
            .borrow()
            .as_ref()
            .is_some_and(|cached| {
                cached.project_revision == project_revision && cached.fps_bits == fps_bits
            });
        if !valid {
            let diagnostics = crate::lint::analyze(project, fps);
            let mut severity_by_line: HashMap<u64, crate::lint::Severity> = HashMap::new();
            let mut zone_diagnostics = Vec::new();
            for diagnostic in &diagnostics {
                match diagnostic.scope {
                    crate::lint::Scope::Line(line_id) => {
                        severity_by_line
                            .entry(line_id)
                            .and_modify(|severity| *severity = (*severity).max(diagnostic.severity))
                            .or_insert(diagnostic.severity);
                    }
                    crate::lint::Scope::Zone {
                        start_frame,
                        end_frame,
                    } => {
                        zone_diagnostics.push(diagnostic.clone());
                        for line in project.lines().filter(|line| {
                            line.start_frame < end_frame && line.end_frame() > start_frame
                        }) {
                            severity_by_line
                                .entry(line.id)
                                .and_modify(|severity| {
                                    *severity = (*severity).max(diagnostic.severity)
                                })
                                .or_insert(diagnostic.severity);
                        }
                    }
                }
            }
            *self.lint_diagnostics_cache.borrow_mut() = Some(CachedLintDiagnostics {
                project_revision,
                fps_bits,
                diagnostics,
                severity_by_line,
                zone_diagnostics,
            });
        }
        Ref::map(self.lint_diagnostics_cache.borrow(), |cached| {
            &cached.as_ref().unwrap().diagnostics
        })
    }

    pub(crate) fn cached_lint_severities(&self) -> Ref<'_, HashMap<u64, crate::lint::Severity>> {
        Ref::map(self.lint_diagnostics_cache.borrow(), |cached| {
            &cached.as_ref().unwrap().severity_by_line
        })
    }

    pub(crate) fn cached_lint_zones(&self) -> Ref<'_, Vec<crate::lint::Diagnostic>> {
        Ref::map(self.lint_diagnostics_cache.borrow(), |cached| {
            &cached.as_ref().unwrap().zone_diagnostics
        })
    }

    fn layout_signature(project: &Project, zone: &Rect, karaoke_mode_tracks: &[bool]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        // Project revision already covers every change that can affect track
        // usage. Re-scanning all lines here made every pointer move O(n).
        zone.height.to_bits().hash(&mut hasher);
        karaoke_mode_tracks.hash(&mut hasher);
        project.revision().hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn get_or_create_layout_ctx(
        &self,
        project: &Project,
        render_index: &ProjectRenderIndex,
        current_frame: f64,
        fps: f64,
        zone: &Rect,
    ) -> std::cell::Ref<'_, EditorLayoutCtx> {
        let karaoke_mode_tracks =
            render_index.karaoke_mode_tracks(current_frame, karaoke_count_in_frames(fps));
        let signature = Self::layout_signature(project, zone, &karaoke_mode_tracks);
        {
            let cached_sig = self.cached_layout_signature.borrow();
            let cached_ctx = self.cached_layout_ctx.borrow();
            if *cached_sig == signature && cached_ctx.is_some() {
                return Ref::map(cached_ctx, |ctx| ctx.as_ref().unwrap());
            }
        }

        let track_indices = if self.compact_empty_tracks {
            render_index.used_track_indices().to_vec()
        } else {
            crate::rythmo_layout::all_track_indices()
        };
        let layout_ctx = EditorLayoutCtx::new_for_indexed_tracks(
            zone,
            &track_indices,
            &karaoke_mode_tracks,
            render_index.karaoke_tracks(),
            render_index.text_emotion_tracks(),
        );

        *self.cached_layout_signature.borrow_mut() = signature;
        *self.cached_layout_ctx.borrow_mut() = Some(layout_ctx);

        Ref::map(self.cached_layout_ctx.borrow(), |ctx| ctx.as_ref().unwrap())
    }

    pub(crate) fn set_compact_empty_tracks(&mut self, compact: bool) {
        if self.compact_empty_tracks != compact {
            self.compact_empty_tracks = compact;
            *self.cached_layout_signature.borrow_mut() = 0;
            *self.cached_layout_ctx.borrow_mut() = None;
        }
    }

    pub(super) fn get_syllable_breaks(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        lang: &str,
    ) -> Vec<usize> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        line.text.hash(&mut hasher);
        lang.hash(&mut hasher);
        let text_hash = hasher.finish();

        let mut cache = self.syllable_breaks_cache.borrow_mut();
        if let Some((cached_breaks, cached_hash)) = cache.get(&line.id) {
            if *cached_hash == text_hash {
                return cached_breaks.clone();
            }
        }

        let breaks = crate::syllable::syllable_breaks(&line.text, lang);
        cache.insert(line.id, (breaks.clone(), text_hash));
        breaks
    }

    pub(super) fn default_syllable_visual_ratios(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        lang: &str,
        breaks: &[usize],
    ) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let font_size = crate::config::get().ui.font_size * 2.0;
        let font_family = crate::vector_text::rythmo_font_family_name();
        let mut hasher = DefaultHasher::new();
        line.text.hash(&mut hasher);
        lang.hash(&mut hasher);
        breaks.hash(&mut hasher);
        font_family.hash(&mut hasher);
        font_size.to_bits().hash(&mut hasher);
        let signature = hasher.finish();

        if let Some(entry) = self.syllable_visual_ratios_cache.borrow().get(&line.id) {
            if entry.signature == signature {
                return entry.ratios.clone();
            }
        }

        let ratios =
            crate::vector_text::measure_rythmo_text_char_ratios_standalone(&line.text, font_size)
                .and_then(|positions| {
                    crate::syllable::visual_ratios_from_char_positions(
                        &line.text, breaks, &positions,
                    )
                })
                .unwrap_or_else(|| crate::syllable::default_ratios_from_breaks(&line.text, breaks));

        self.syllable_visual_ratios_cache.borrow_mut().insert(
            line.id,
            SyllableVisualRatiosCacheEntry {
                signature,
                ratios: ratios.clone(),
            },
        );
        ratios
    }

    pub(super) fn karaoke_ui_text_width_for_render(
        &self,
        line: &crate::rythmo_line::RythmoLine,
    ) -> f32 {
        let font_size = karaoke_ui_font_size();
        if let Some(width) = self.cached_karaoke_ui_text_width_for_font(line, font_size) {
            return width;
        }

        let width = measure_karaoke_ui_text_width(&line.text, font_size);
        self.store_karaoke_ui_text_width(line, font_size, width);
        width
    }

    fn cached_karaoke_ui_text_width_for_font(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        font_size: f32,
    ) -> Option<f32> {
        let text_hash = hash_karaoke_text(&line.text);
        let text_len = line.text.len();
        let font_size_bits = font_size.to_bits();
        let cache = self.karaoke_text_width_cache.borrow();
        cache.get(&line.id).and_then(|entry| {
            (entry.text_hash == text_hash
                && entry.text_len == text_len
                && entry.font_size_bits == font_size_bits)
                .then_some(entry.width)
        })
    }

    fn store_karaoke_ui_text_width(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        font_size: f32,
        width: f32,
    ) {
        self.karaoke_text_width_cache.borrow_mut().insert(
            line.id,
            KaraokeTextWidthCacheEntry {
                text_hash: hash_karaoke_text(&line.text),
                text_len: line.text.len(),
                font_size_bits: font_size.to_bits(),
                width,
            },
        );
    }

    pub(super) fn prewarm_karaoke_text_widths(
        &self,
        project: &Project,
        index: &KaraokeUiIndex,
        current_frame: i64,
        lookbehind_frames: i64,
        budget: usize,
    ) {
        if budget == 0 || index.karaoke_timeline.is_empty() {
            return;
        }

        let mut prewarm = self.karaoke_width_prewarm.borrow_mut();
        if prewarm.signature != index.signature {
            prewarm.signature = index.signature;
            prewarm.cursor = index.timeline_cursor_at(current_frame - lookbehind_frames);
            prewarm.complete = false;
        }
        if prewarm.complete {
            return;
        }
        if index
            .karaoke_timeline
            .get(prewarm.cursor)
            .is_some_and(|(start_frame, _)| *start_frame + lookbehind_frames < current_frame)
        {
            prewarm.cursor = index.timeline_cursor_at(current_frame - lookbehind_frames);
        }

        let font_size = karaoke_ui_font_size();
        let mut warmed = 0;
        let mut visited = 0;
        while warmed < budget && visited < index.karaoke_timeline.len() {
            let (_, line_id) = index.karaoke_timeline[prewarm.cursor];
            prewarm.cursor = (prewarm.cursor + 1) % index.karaoke_timeline.len();
            visited += 1;

            let Some(line) = project.get_line(line_id) else {
                continue;
            };
            if self
                .cached_karaoke_ui_text_width_for_font(line, font_size)
                .is_some()
            {
                continue;
            }

            let width = measure_karaoke_ui_text_width(&line.text, font_size);
            self.store_karaoke_ui_text_width(line, font_size, width);
            warmed += 1;
        }

        if visited >= index.karaoke_timeline.len() {
            prewarm.complete = true;
        }
    }

    pub(super) fn prune_karaoke_text_width_cache(&self, project: &Project) {
        let max_cache_entries = project.line_count().saturating_mul(2).saturating_add(32);
        if self.karaoke_text_width_cache.borrow().len() <= max_cache_entries {
            return;
        }

        self.karaoke_text_width_cache
            .borrow_mut()
            .retain(|line_id, _| project.get_line(*line_id).is_some());
    }

    pub fn is_editing(&self) -> bool {
        self.editing_line.is_some()
            || self.editing_character.is_some()
            || self.editing_note.is_some()
    }

    pub fn needs_animation_or_interaction(&self) -> bool {
        self.dragging.is_some()
            || self.selection_drag.is_some()
            || self.panning
            || self.keyboard_pan_direction != 0
            || self.ghost_preview.is_some()
            || self.syllable_drag.is_some()
            || self.has_text_emotions.get()
    }

    pub(super) fn text_emotion_seconds(&self) -> f32 {
        self.text_emotion_epoch.elapsed().as_secs_f32()
    }

    pub(super) fn update_text_emotion_presence(&self, render_index: &ProjectRenderIndex) {
        self.has_text_emotions.set(render_index.has_text_emotions());
    }

    pub(super) fn max_leading_visual_span(&self, project: &Project) -> f32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        project.revision().hash(&mut hasher);
        crate::config::get()
            .ui
            .font_size
            .to_bits()
            .hash(&mut hasher);
        project.settings().scroll_speed.to_bits().hash(&mut hasher);
        crate::vector_text::rythmo_font_family_name().hash(&mut hasher);
        let signature = hasher.finish();

        let mut cache = self.leading_visual_span_cache.borrow_mut();
        if cache.signature != signature {
            cache.span = project
                .lines()
                .filter_map(|line| {
                    let (badge_width, actor_count) = if line.kind.is_dialogue() {
                        (
                            badge_width(&line.character_name),
                            line.voice_actor_names.len(),
                        )
                    } else if matches!(line.kind, crate::rythmo_line::RythmoLineKind::AmbianceStart)
                    {
                        (badge_width(&line.character_name), 0)
                    } else {
                        return None;
                    };
                    Some(
                        4.0 * ppf()
                            + badge_width
                            + actor_count as f32 * (ACTOR_ICON_SIZE + ACTOR_ICON_GAP),
                    )
                })
                .fold(0.0_f32, f32::max);
            cache.signature = signature;
        }
        cache.span
    }

    pub fn needs_pointer_motion(&self) -> bool {
        self.dragging.is_some()
            || self.selection_drag.is_some()
            || self.transform_handle.is_some()
            || self.panning
            || self.audio_offset_drag.is_some()
            || self.syllable_drag.is_some()
            || self.detection_drag.is_some()
            || self.active_stroke.is_some()
            || self.ctrl_held
    }

    pub fn next_cursor_blink_deadline(&self) -> Option<std::time::Instant> {
        [
            self.line_input.next_cursor_blink_deadline(),
            self.char_input.next_cursor_blink_deadline(),
            self.note_input.next_cursor_blink_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn stop_line_editing(&mut self) {
        self.editing_line = None;
        self.line_lowercase_override = false;
        self.line_input.deactivate();
    }

    pub fn start_editing_note(&mut self, line_id: u64, text: &str) {
        self.editing_note = Some(line_id);
        self.note_input.activate(text);
        self.selected = Some(Selection::Line(line_id));
    }

    pub fn stop_note_editing(&mut self) {
        self.editing_note = None;
        self.note_input.deactivate();
    }

    pub fn start_editing_line(&mut self, line_id: u64, text: &str) {
        self.editing_line = Some(line_id);
        self.line_lowercase_override = false;
        self.line_input.activate(text);
        self.selected = Some(Selection::Line(line_id));
    }

    pub fn stop_char_editing(&mut self) {
        self.editing_character = None;
        self.char_input.deactivate();
        self.color_picker.close();
        self.autocomplete_index = None;
        self.autocomplete_hover = None;
        self.autocomplete_scroll = 0;
    }

    /// Cancel every transient authoring gesture before changing workspace or
    /// entering a read-only view. Project data is deliberately untouched.
    pub fn cancel_active_interaction(&mut self) {
        self.stop_line_editing();
        self.stop_note_editing();
        self.stop_char_editing();
        self.hovered_line = None;
        self.hovered_track = None;
        self.selected = None;
        self.dragging = None;
        self.ghost_preview = None;
        self.panning = false;
        self.audio_offset_mode = false;
        self.audio_offset_drag = None;
        self.pending_cursor_click = None;
        self.pan_accum = 0.0;
        self.keyboard_pan_direction = 0;
        self.keyboard_pan_last_tick = None;
        self.keyboard_pan_accum_px = 0.0;
        self.syllable_drag = None;
        self.context_menu = None;
        self.detection_hover = None;
        self.detection_menu = None;
        self.detection_drag = None;
        if self.active_stroke.take().is_some() {
            self.drawing_dirty = true;
        }
        self.selection_drag = None;
        self.transform_handle = None;
    }
}
