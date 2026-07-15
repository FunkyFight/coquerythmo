//! Ephemeral interaction state owned by the rythmo workspace view.

use super::*;

/// What is currently selected in the BR.
#[derive(Clone, PartialEq, Debug)]
pub enum Selection {
    Line(u64),
    Marker(usize),
    AllLines,
    Strokes(Vec<u64>),
}

/// Ghost line preview shown when holding click on empty BR space.
pub struct GhostPreview {
    pub frame: i64,
    pub y_slot: f32,
    pub duration_frames: i64,
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

struct CachedKaraokeUiIndex {
    signature: u64,
    index: KaraokeUiIndex,
}

#[derive(Default)]
struct KaraokeWidthPrewarmState {
    signature: u64,
    cursor: usize,
    complete: bool,
}

pub struct RythmoState {
    pub hovered_line: Option<u64>,
    pub hovered_track: Option<usize>,
    pub selected: Option<Selection>,
    pub editing_line: Option<u64>,
    pub line_input: crate::ui::text_input::TextInputState,
    pub editing_character: Option<u64>,
    pub char_input: crate::ui::text_input::TextInputState,
    pub editing_note: Option<u64>,
    pub note_input: crate::ui::text_input::TextInputState,
    pub color_picker: crate::ui::color_picker::ColorPickerState,
    pub autocomplete_index: Option<usize>,
    pub autocomplete_hover: Option<usize>,
    pub dragging: Option<DragState>,
    pub ghost_preview: Option<GhostPreview>,
    pub ctrl_held: bool,
    pub panning: bool,
    pub audio_offset_mode: bool,
    pub audio_offset_drag: Option<AudioOffsetDrag>,
    pub pending_cursor_click: Option<(f32, bool)>, // (x_ratio, is_shift_click)
    pub pan_last_x: f32,
    pub pan_accum: f32,
    pub syllable_drag: Option<SyllableDrag>,
    pub context_menu: Option<LineContextMenu>,
    pub active_stroke: Option<crate::rythmo_drawing::DrawingStroke>,
    pub drawing_dirty: bool,
    pub selection_drag: Option<Rect>,
    pub transform_handle: Option<TransformHandle>,
    karaoke_text_width_cache: RefCell<HashMap<u64, KaraokeTextWidthCacheEntry>>,
    karaoke_index_cache: RefCell<Option<CachedKaraokeUiIndex>>,
    karaoke_width_prewarm: RefCell<KaraokeWidthPrewarmState>,
    cached_layout_signature: RefCell<u64>,
    cached_layout_ctx: RefCell<Option<EditorLayoutCtx>>,
    syllable_breaks_cache: RefCell<HashMap<u64, (Vec<usize>, u64)>>, // line_id -> (breaks, text_hash)
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
            selected: None,
            editing_line: None,
            line_input: crate::ui::text_input::TextInputState::new(),
            editing_character: None,
            char_input: crate::ui::text_input::TextInputState::new(),
            editing_note: None,
            note_input: crate::ui::text_input::TextInputState::new(),
            color_picker: crate::ui::color_picker::ColorPickerState::new(),
            autocomplete_index: None,
            autocomplete_hover: None,
            dragging: None,
            ghost_preview: None,
            ctrl_held: false,
            panning: false,
            audio_offset_mode: false,
            audio_offset_drag: None,
            pending_cursor_click: None,
            pan_last_x: 0.0,
            pan_accum: 0.0,
            syllable_drag: None,
            context_menu: None,
            active_stroke: None,
            drawing_dirty: false,
            selection_drag: None,
            transform_handle: None,
            karaoke_text_width_cache: RefCell::new(HashMap::new()),
            karaoke_index_cache: RefCell::new(None),
            karaoke_width_prewarm: RefCell::new(KaraokeWidthPrewarmState::default()),
            cached_layout_signature: RefCell::new(0),
            cached_layout_ctx: RefCell::new(None),
            syllable_breaks_cache: RefCell::new(HashMap::new()),
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

    fn layout_signature(project: &Project, zone: &Rect) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        // Project revision already covers every change that can affect track
        // usage. Re-scanning all lines here made every pointer move O(n).
        project.revision().hash(&mut hasher);
        zone.height.to_bits().hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn get_or_create_layout_ctx(
        &self,
        project: &Project,
        karaoke_tracks: &[bool],
        zone: &Rect,
    ) -> std::cell::Ref<'_, EditorLayoutCtx> {
        let signature = Self::layout_signature(project, zone);
        {
            let cached_sig = self.cached_layout_signature.borrow();
            let cached_ctx = self.cached_layout_ctx.borrow();
            if *cached_sig == signature && cached_ctx.is_some() {
                return Ref::map(cached_ctx, |ctx| ctx.as_ref().unwrap());
            }
        }

        let karaoke_track_count = karaoke_tracks.iter().filter(|&&k| k).count();
        let normal_body_h = editor_normal_body_height_for_karaoke_tracks(karaoke_track_count, zone);
        let track_layouts = build_track_layouts_from_karaoke_flags(
            &rythmo_layout::all_track_indices(),
            karaoke_tracks,
            normal_body_h,
            slot_header_height(),
            BADGE_GAP,
            1.0,
        );
        let layout_ctx = EditorLayoutCtx::from_track_layouts(normal_body_h, track_layouts);

        *self.cached_layout_signature.borrow_mut() = signature;
        *self.cached_layout_ctx.borrow_mut() = Some(layout_ctx);

        Ref::map(self.cached_layout_ctx.borrow(), |ctx| ctx.as_ref().unwrap())
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
            || self.panning
            || self.ghost_preview.is_some()
            || self.syllable_drag.is_some()
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
        self.line_input.activate(text);
        self.selected = Some(Selection::Line(line_id));
    }

    pub fn stop_char_editing(&mut self) {
        self.editing_character = None;
        self.char_input.deactivate();
        self.color_picker.close();
        self.autocomplete_index = None;
        self.autocomplete_hover = None;
    }
}
