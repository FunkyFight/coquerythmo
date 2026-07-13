use super::renderer::StretchedText;
use super::text_input::{self, TextInputMetrics};
use super::widget::{
    EventResponse, HAlign, IconInstance, LabelInfo, Overflow, QuadInstance, Rect, UiAction,
    UiEvent, VAlign,
};
use crate::constants;
use crate::i18n::t;
use crate::project::Project;
use crate::render_index::ProjectRenderIndex;
use crate::rythmo_layout;
use crate::rythmo_line::MarkerKind;
use std::cell::{Ref, RefCell};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};

const TICK_WIDTH: f32 = 1.0;
const TICK_GAP: f32 = 8.0;
const TICK_COLOR: [f32; 4] = [0.40, 0.40, 0.45, 0.5];

const PLAYHEAD_WIDTH: f32 = 2.0;
const PLAYHEAD_COLOR: [f32; 4] = [0.85, 0.15, 0.15, 1.0];

const HANDLE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
const LINE_BORDER: [f32; 4] = [0.5, 0.5, 0.55, 0.3];
const LINE_BORDER_HOVER: [f32; 4] = [0.6, 0.6, 0.65, 0.5];
const LINE_RADIUS: f32 = 2.0;
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 1.0];
const KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS: f64 = 60.0;
const KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME: usize = 32;
const KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME: usize = 2;

/// What is currently selected in the BR.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Selection {
    Line(u64),
    Marker(usize),
    AllLines,
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
    pub line_input: super::text_input::TextInputState,
    pub editing_character: Option<u64>,
    pub char_input: super::text_input::TextInputState,
    pub editing_note: Option<u64>,
    pub note_input: super::text_input::TextInputState,
    pub color_picker: super::color_picker::ColorPickerState,
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
    karaoke_text_width_cache: RefCell<HashMap<u64, KaraokeTextWidthCacheEntry>>,
    karaoke_index_cache: RefCell<Option<CachedKaraokeUiIndex>>,
    karaoke_width_prewarm: RefCell<KaraokeWidthPrewarmState>,
    cached_layout_signature: RefCell<u64>,
    cached_layout_ctx: RefCell<Option<EditorLayoutCtx>>,
    syllable_breaks_cache: RefCell<HashMap<u64, (Vec<usize>, u64)>>, // line_id -> (breaks, text_hash)
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
    Selection,
}

impl RythmoState {
    pub fn new() -> Self {
        Self {
            hovered_line: None,
            hovered_track: None,
            selected: None,
            editing_line: None,
            line_input: super::text_input::TextInputState::new(),
            editing_character: None,
            char_input: super::text_input::TextInputState::new(),
            editing_note: None,
            note_input: super::text_input::TextInputState::new(),
            color_picker: super::color_picker::ColorPickerState::new(),
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
            karaoke_text_width_cache: RefCell::new(HashMap::new()),
            karaoke_index_cache: RefCell::new(None),
            karaoke_width_prewarm: RefCell::new(KaraokeWidthPrewarmState::default()),
            cached_layout_signature: RefCell::new(0),
            cached_layout_ctx: RefCell::new(None),
            syllable_breaks_cache: RefCell::new(HashMap::new()),
        }
    }

    fn cached_karaoke_ui_index(
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
        // Track usage (which tracks have lines)
        let track_count = rythmo_layout::track_count();
        let mut used_tracks = vec![false; track_count];
        let mut karaoke_tracks = vec![false; track_count];
        for line in project.lines() {
            let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
            used_tracks[track_index] = true;
            if line.karaoke {
                karaoke_tracks[track_index] = true;
            }
        }
        for (i, (used, karaoke)) in used_tracks.iter().zip(karaoke_tracks.iter()).enumerate() {
            if *used {
                i.hash(&mut hasher);
                karaoke.hash(&mut hasher);
            }
        }
        zone.height.to_bits().hash(&mut hasher);
        hasher.finish()
    }

    fn get_or_create_layout_ctx(
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

    fn get_syllable_breaks(&self, line: &crate::rythmo_line::RythmoLine, lang: &str) -> Vec<usize> {
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

    fn karaoke_ui_text_width_for_render(&self, line: &crate::rythmo_line::RythmoLine) -> f32 {
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

    fn prewarm_karaoke_text_widths(
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

    fn prune_karaoke_text_width_cache(&self, project: &Project) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rect_approx_eq(left: Rect, right: Rect) {
        assert!((left.x - right.x).abs() < 0.01, "x: {left:?} != {right:?}");
        assert!((left.y - right.y).abs() < 0.01, "y: {left:?} != {right:?}");
        assert!(
            (left.width - right.width).abs() < 0.01,
            "width: {left:?} != {right:?}"
        );
        assert!(
            (left.height - right.height).abs() < 0.01,
            "height: {left:?} != {right:?}"
        );
    }

    #[test]
    fn redistribute_group_preserves_proportions_above_minimum() {
        let mut ratios = vec![0.2, 0.3, 0.5];
        redistribute_group_to_total(&mut ratios, 0.5, 0.05);

        let sum: f32 = ratios.iter().sum();
        assert!((sum - 0.5).abs() < 0.0001);
        assert!(ratios.iter().all(|ratio| *ratio >= 0.05));
        assert!(ratios[2] > ratios[1]);
        assert!(ratios[1] > ratios[0]);
    }

    #[test]
    fn reducing_left_group_expands_right_group_proportionally() {
        let mut state = RythmoState::new();
        state.syllable_drag = Some(SyllableDrag {
            line_id: 1,
            separator_index: 1,
            ratios: vec![0.2, 0.3, 0.2, 0.3],
            drag_start_x: 100.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
        });

        let _ = syllable_mouse_move(&mut state, 90.0);
        let ratios = &state.syllable_drag.as_ref().unwrap().ratios;

        assert!((ratios[..2].iter().sum::<f32>() - 0.4).abs() < 0.0001);
        assert!((ratios[2..].iter().sum::<f32>() - 0.6).abs() < 0.0001);
        assert!(ratios[2] > 0.2);
        assert!(ratios[3] > 0.3);
        assert!(ratios[3] > ratios[2]);
    }

    #[test]
    fn syllable_drag_does_not_block_at_old_five_percent_minimum() {
        let mut state = RythmoState::new();
        state.syllable_drag = Some(SyllableDrag {
            line_id: 1,
            separator_index: 1,
            ratios: vec![0.48, 0.48, 0.04],
            drag_start_x: 100.0,
            line_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
        });

        let _ = syllable_mouse_move(&mut state, 102.0);
        let ratios = &state.syllable_drag.as_ref().unwrap().ratios;

        assert!(ratios[2] < 0.04, "right group should still compress");
        assert!(ratios[..2].iter().sum::<f32>() > 0.96);
    }

    #[test]
    fn adjacent_karaoke_preview_ignores_distant_lines() {
        let mut project = Project::new();
        let active_id = project.add_line(0, 24, 0.25);
        let near_id = project.add_line(24 * 20, 24, 0.25);
        let far_id = project.add_line(24 * 40, 24, 0.25);
        for id in [active_id, near_id, far_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let active = project.get_line(active_id).unwrap();
        assert_eq!(
            next_karaoke_line_after(&project, active, karaoke_adjacent_max_gap_frames(24.0))
                .map(|line| line.id),
            Some(near_id)
        );

        project.remove_line(near_id);
        let active = project.get_line(active_id).unwrap();
        assert!(
            next_karaoke_line_after(&project, active, karaoke_adjacent_max_gap_frames(24.0))
                .is_none()
        );
    }

    #[test]
    fn first_karaoke_line_scrolls_before_island_starts() {
        let mut project = Project::new();
        let first_id = project.add_line(24 * 10, 24, 0.25);
        let second_id = project.add_line(24 * 12, 24, 0.25);
        for id in [first_id, second_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();

        assert!(!karaoke_prestart_scroll_visible(
            &project,
            first,
            0.0,
            max_gap_frames,
            count_in_frames
        ));
        assert!(karaoke_prestart_scroll_visible(
            &project,
            first,
            (first.start_frame - count_in_frames) as f64,
            max_gap_frames,
            count_in_frames
        ));
        assert!(!karaoke_prestart_scroll_visible(
            &project,
            second,
            0.0,
            max_gap_frames,
            count_in_frames
        ));
        assert!(!karaoke_prestart_scroll_visible(
            &project,
            first,
            first.start_frame as f64,
            max_gap_frames,
            count_in_frames
        ));
    }

    #[test]
    fn normal_line_splits_karaoke_islands() {
        let mut project = Project::new();
        let previous_karaoke_id = project.add_line(0, 24, 0.25);
        let normal_id = project.add_line(24 * 2, 24, 0.25);
        let next_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(previous_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(next_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let previous_karaoke = project.get_line(previous_karaoke_id).unwrap();
        let next_karaoke = project.get_line(next_karaoke_id).unwrap();

        assert!(next_karaoke_line_after(&project, previous_karaoke, max_gap_frames).is_none());
        assert!(previous_karaoke_line_before(&project, next_karaoke, max_gap_frames).is_none());
        assert!(karaoke_prestart_scroll_visible(
            &project,
            next_karaoke,
            (next_karaoke.start_frame - count_in_frames) as f64,
            max_gap_frames,
            count_in_frames
        ));
    }

    #[test]
    fn karaoke_island_after_normal_line_continues_alternating_rows() {
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.25);
        let first_karaoke_id = project.add_line(24 * 2, 24, 0.25);
        let second_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(first_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(second_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first_karaoke = project.get_line(first_karaoke_id).unwrap();
        let second_karaoke = project.get_line(second_karaoke_id).unwrap();
        let index = KaraokeUiIndex::new(&project, max_gap_frames);

        assert_eq!(
            karaoke_stack_row(&project, first_karaoke, max_gap_frames),
            1
        );
        assert_eq!(
            karaoke_stack_row(&project, second_karaoke, max_gap_frames),
            0
        );
        assert_eq!(index.stack_row(first_karaoke), 1);
        assert_eq!(index.stack_row(second_karaoke), 0);
    }

    #[test]
    fn karaoke_island_lines_alternate_stack_rows() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        let normal_id = project.add_line(24 * 6, 24, 0.25);
        let next_island_id = project.add_line(24 * 8, 24, 0.25);
        for id in [first_id, second_id, third_id, next_island_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }
        project.get_line_mut(normal_id).unwrap().karaoke = false;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();
        let next_island = project.get_line(next_island_id).unwrap();

        assert_eq!(karaoke_stack_row(&project, first, max_gap_frames), 0);
        assert_eq!(karaoke_stack_row(&project, second, max_gap_frames), 1);
        assert_eq!(karaoke_stack_row(&project, third, max_gap_frames), 0);
        assert_eq!(karaoke_stack_row(&project, next_island, max_gap_frames), 1);
    }

    #[test]
    fn next_karaoke_line_stays_visible_inside_started_island() {
        let mut project = Project::new();
        let first_id = project.add_line(24 * 10, 24, 0.25);
        let second_id = project.add_line(24 * 14, 24, 0.25);
        for id in [first_id, second_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();

        assert!(!karaoke_upcoming_stack_visible(
            &project,
            second,
            (first.start_frame - 1) as f64,
            max_gap_frames
        ));
        assert!(karaoke_upcoming_stack_visible(
            &project,
            second,
            first.start_frame as f64,
            max_gap_frames
        ));
        assert!(karaoke_upcoming_stack_visible(
            &project,
            second,
            (first.end_frame() + 1) as f64,
            max_gap_frames
        ));
        assert!(!karaoke_upcoming_stack_visible(
            &project,
            second,
            second.start_frame as f64,
            max_gap_frames
        ));
    }

    #[test]
    fn karaoke_stack_rows_stay_inside_track_body() {
        let row_height = 40.0;
        let base = Rect {
            x: 0.0,
            y: 10.0,
            width: 200.0,
            height: rythmo_layout::karaoke_track_body_height(row_height, 1.0),
        };
        let top = karaoke_stack_rect(base, 0, 1.0);
        let bottom = karaoke_stack_rect(base, 1, 1.0);

        assert!(top.y >= base.y);
        assert!(bottom.y > top.y);
        assert!(top.y + top.height <= bottom.y);
        assert!(bottom.y + bottom.height <= base.y + base.height);
        assert!((top.height - row_height).abs() < f32::EPSILON);
        assert!((bottom.height - row_height).abs() < f32::EPSILON);
    }

    #[test]
    fn editor_only_karaoke_tracks_get_double_body_height() {
        crate::config::init();
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 300.0,
        };

        let normal_body_h = editor_normal_body_height(&project, &zone);
        let normal_rect = line_rect(&project, project.get_line(normal_id).unwrap(), 0.0, &zone);
        let karaoke_body = editor_track_body_rect(&project, 0.5, &zone);
        let karaoke_rect = karaoke_preview_line_rect(
            &project,
            project.get_line(karaoke_id).unwrap(),
            24.0,
            &zone,
            karaoke_adjacent_max_gap_frames(24.0),
        );

        assert!((normal_rect.height - normal_body_h).abs() < f32::EPSILON);
        assert!(karaoke_body.height > normal_body_h * 1.9);
        assert!((karaoke_rect.height - normal_body_h).abs() < 0.01);
    }

    #[test]
    fn y_to_slot_uses_variable_track_offsets() {
        let mut project = Project::new();
        let karaoke_id = project.add_line(0, 24, 0.25);
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 0.0,
            y: 10.0,
            width: 800.0,
            height: 300.0,
        };
        let layouts = editor_track_layouts(&project, &zone);
        let karaoke_track = rythmo_layout::track_for_index(&layouts, 1).unwrap();
        let next_track = rythmo_layout::track_for_index(&layouts, 2).unwrap();

        let karaoke_y =
            zone.y + constants::RULER_HEIGHT + karaoke_track.top + karaoke_track.total_h - 1.0;
        let next_y = zone.y + constants::RULER_HEIGHT + next_track.top + 1.0;

        assert_eq!(y_to_slot(&project, karaoke_y, &zone), 0.25);
        assert_eq!(y_to_slot(&project, next_y, &zone), 0.5);
    }

    #[test]
    fn karaoke_character_label_only_on_first_or_character_change() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        for id in [first_id, second_id, third_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        project.get_line_mut(third_id).unwrap().character_name = "Bob".to_string();

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();

        assert!(karaoke_character_label_visible(
            &project,
            first,
            max_gap_frames
        ));
        assert!(!karaoke_character_label_visible(
            &project,
            second,
            max_gap_frames
        ));
        assert!(karaoke_character_label_visible(
            &project,
            third,
            max_gap_frames
        ));
    }

    #[test]
    fn karaoke_ui_index_uses_chronological_order_not_insertion_order() {
        let mut project = Project::new();
        let second_id = project.add_line(24 * 2, 24, 0.25);
        let first_id = project.add_line(0, 24, 0.25);
        let third_id = project.add_line(24 * 4, 24, 0.25);
        for id in [first_id, second_id, third_id] {
            let line = project.get_line_mut(id).unwrap();
            line.karaoke = true;
            line.character_name = "Alice".to_string();
        }
        project.get_line_mut(third_id).unwrap().character_name = "Bob".to_string();

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let first = project.get_line(first_id).unwrap();
        let second = project.get_line(second_id).unwrap();
        let third = project.get_line(third_id).unwrap();

        assert_eq!(index.stack_row(first), 0);
        assert_eq!(index.stack_row(second), 1);
        assert_eq!(index.stack_row(third), 0);
        assert_eq!(index.previous_adjacent_karaoke_id(second), Some(first_id));
        assert!(index.character_label_visible(first));
        assert!(!index.character_label_visible(second));
        assert!(index.character_label_visible(third));
    }

    #[test]
    fn karaoke_ui_index_normal_line_cuts_island() {
        let mut project = Project::new();
        let previous_karaoke_id = project.add_line(0, 24, 0.25);
        let normal_id = project.add_line(24 * 2, 24, 0.25);
        let next_karaoke_id = project.add_line(24 * 4, 24, 0.25);
        project.get_line_mut(previous_karaoke_id).unwrap().karaoke = true;
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(next_karaoke_id).unwrap().karaoke = true;

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let count_in_frames = karaoke_count_in_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let next_karaoke = project.get_line(next_karaoke_id).unwrap();

        assert_eq!(index.previous_adjacent_karaoke_id(next_karaoke), None);
        assert_eq!(index.stack_row(next_karaoke), 1);
        assert!(!index.prestart_scroll_visible(next_karaoke, 0.0, count_in_frames));
        assert!(index.prestart_scroll_visible(
            next_karaoke,
            (next_karaoke.start_frame - count_in_frames) as f64,
            count_in_frames
        ));
    }

    #[test]
    fn karaoke_ui_index_uses_quantized_track_for_drifted_slots() {
        let mut project = Project::new();
        let first_id = project.add_line(0, 24, 0.25);
        let drifted_id = project.add_line(24 * 2, 24, 0.26);
        for id in [first_id, drifted_id] {
            project.get_line_mut(id).unwrap().karaoke = true;
        }

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        let first = project.get_line(first_id).unwrap();
        let drifted = project.get_line(drifted_id).unwrap();

        assert_eq!(index.stack_row(first), 0);
        assert_eq!(index.stack_row(drifted), 1);
        assert_eq!(index.previous_adjacent_karaoke_id(drifted), Some(first_id));
    }

    #[test]
    fn editor_layout_ctx_matches_wrapper_rects() {
        crate::config::init();
        let mut project = Project::new();
        let normal_id = project.add_line(0, 24, 0.0);
        let karaoke_id = project.add_line(24, 24, 0.5);
        project.get_line_mut(normal_id).unwrap().karaoke = false;
        project.get_line_mut(karaoke_id).unwrap().karaoke = true;
        let zone = Rect {
            x: 12.0,
            y: 8.0,
            width: 800.0,
            height: 300.0,
        };
        let ctx = EditorLayoutCtx::new(&project, &zone);

        assert!((ctx.normal_body_h - editor_normal_body_height(&project, &zone)).abs() < 0.01);
        assert_rect_approx_eq(
            ctx.track_body_rect(0.5, &zone),
            editor_track_body_rect(&project, 0.5, &zone),
        );
        assert_rect_approx_eq(
            ctx.line_rect_with_karaoke_width(
                project.get_line(normal_id).unwrap(),
                0.0,
                &zone,
                false,
                None,
            ),
            line_rect(&project, project.get_line(normal_id).unwrap(), 0.0, &zone),
        );

        let max_gap_frames = karaoke_adjacent_max_gap_frames(24.0);
        let karaoke = project.get_line(karaoke_id).unwrap();
        let index = KaraokeUiIndex::new(&project, max_gap_frames);
        assert_rect_approx_eq(
            karaoke_preview_line_rect_with_state(
                &ctx,
                karaoke,
                24.0,
                &zone,
                false,
                index.upcoming_stack_visible(karaoke, 24.0),
                index.stack_row(karaoke),
                None,
            ),
            karaoke_preview_line_rect(&project, karaoke, 24.0, &zone, max_gap_frames),
        );
    }

    #[test]
    fn new_karaoke_line_render_width_uses_measured_width() {
        crate::config::init();
        let mut project = Project::new();
        let line_id = project.add_line(0, 24, 0.25);
        let line = project.get_line_mut(line_id).unwrap();
        line.karaoke = true;
        line.text = "Karaoke width check".to_string();

        let state = RythmoState::new();
        let line = project.get_line(line_id).unwrap();
        assert_eq!(
            state.karaoke_ui_text_width_for_render(line),
            karaoke_ui_text_width(&line.text)
        );
    }

    #[test]
    fn karaoke_count_in_dot_moves_from_left_onto_text() {
        let line_rect = Rect {
            x: 300.0,
            y: 80.0,
            width: 120.0,
            height: 32.0,
        };
        let start = karaoke_count_in_dot_rect(&line_rect, 0.0, 1.0);
        let mid = karaoke_count_in_dot_rect(&line_rect, 0.5, 1.0);
        let end = karaoke_count_in_dot_rect(&line_rect, 1.0, 1.0);

        assert!(start.x + start.width <= line_rect.x);
        assert!(mid.x > start.x);
        assert!(mid.x < line_rect.x);
        assert!((end.x - line_rect.x).abs() < 0.01);
    }

    #[test]
    fn fractional_frame_geometry_shifts_by_subframe_amounts() {
        crate::config::init();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };

        let whole_frame_x = frame_to_x(100, 100.0, &zone);
        let half_frame_x = frame_to_x(100, 100.5, &zone);

        assert!((half_frame_x - (whole_frame_x - ppf() * 0.5)).abs() < 0.01);
        assert_eq!(x_to_frame(half_frame_x, 100.5, &zone), 100);
        assert_eq!(x_to_frame(frame_to_x(101, 100.5, &zone), 100.5, &zone), 101);
    }

    #[test]
    fn waveform_offset_keeps_visible_audio_peaks_rendered() {
        crate::config::init();
        let project = Project::new();
        let mut render_index = ProjectRenderIndex::new();
        render_index.refresh(&project);
        let state = RythmoState::new();
        let zone = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 240.0,
        };
        let current_frame = 10_000;
        let waveform_offset_frames = 9_000;
        let visible_audio_frame = current_frame - waveform_offset_frames;
        let mut waveform = vec![0.0; (visible_audio_frame as usize + 1) * 4];
        waveform[visible_audio_frame as usize * 4] = 1.0;

        let quads = render_rythmo_base(
            &zone,
            &project,
            &render_index,
            current_frame as f64,
            &waveform,
            waveform_offset_frames,
            true,
            false,
            24.0,
            &state,
        );

        assert!(quads.iter().any(|quad| {
            quad.color == [0.30, 0.90, 0.45, 0.85]
                && quad.rect[0] >= zone.x
                && quad.rect[0] <= zone.x + zone.width
                && (quad.rect[3] - constants::RULER_HEIGHT).abs() < 0.01
        }));
    }
}

fn ppf() -> f32 {
    constants::PIXELS_PER_FRAME * crate::config::scroll_speed()
}

fn f64_floor_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.floor() as i64
    }
}

fn f64_ceil_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.ceil() as i64
    }
}

fn f64_round_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        0
    } else if value <= i64::MIN as f64 {
        i64::MIN
    } else if value >= i64::MAX as f64 {
        i64::MAX
    } else {
        value.round() as i64
    }
}

fn visual_frame_to_i64(current_frame: f64) -> i64 {
    f64_floor_to_i64(current_frame)
}

fn render_window(zone: &Rect, current_frame: f64, margin_frames: i64) -> (i64, i64) {
    let half_visible_frames = zone.width as f64 / ppf().max(0.001) as f64 / 2.0;
    let margin_frames = margin_frames.max(0);
    let first_frame =
        f64_floor_to_i64(current_frame - half_visible_frames).saturating_sub(margin_frames);
    let last_frame =
        f64_ceil_to_i64(current_frame + half_visible_frames).saturating_add(margin_frames);
    (first_frame, last_frame.max(first_frame))
}

fn interactive_render_margin_frames(fps: f64, render_index: &ProjectRenderIndex) -> i64 {
    let fps = fps.max(1.0);
    karaoke_adjacent_max_gap_frames(fps)
        .max(karaoke_count_in_frames(fps))
        .max((fps * 10.0).round() as i64)
        .saturating_add(render_index.max_duration_frames())
}

fn frame_to_x(frame: i64, current_frame: f64, zone: &Rect) -> f32 {
    let center_x = zone.x + zone.width / 2.0;
    center_x + (frame as f64 - current_frame) as f32 * ppf()
}

fn x_to_frame(x: f32, current_frame: f64, zone: &Rect) -> i64 {
    let center_x = zone.x + zone.width / 2.0;
    f64_round_to_i64(current_frame + (x - center_x) as f64 / ppf().max(0.001) as f64)
}

fn clamped_new_line_duration(project: &Project, frame: i64, y_slot: f32, fps: f64) -> i64 {
    let default_dur = (fps * constants::DEFAULT_LINE_DURATION_SEC) as i64;
    project
        .lines()
        .filter(|line| (line.y_slot - y_slot).abs() < 0.01 && line.start_frame > frame)
        .map(|line| line.start_frame)
        .min()
        .map(|start| (start - frame - constants::TICK_GAP_FRAMES).clamp(1, default_dur))
        .unwrap_or(default_dur)
}

fn y_to_slot(project: &Project, y: f32, zone: &Rect) -> f32 {
    let relative_y = y - zone.y - constants::RULER_HEIGHT;
    let layouts = editor_track_layouts(project, zone);
    let layout = layouts
        .iter()
        .find(|layout| relative_y < layout.top + layout.total_h)
        .or_else(|| layouts.last())
        .unwrap_or_else(|| {
            layouts
                .first()
                .expect("editor track layout should not be empty")
        });
    rythmo_layout::y_slot_for_track_index(layout.track_index)
}

fn karaoke_ui_font_size() -> f32 {
    crate::config::get().ui.font_size * 2.0 * constants::KARAOKE_TEXT_FONT_SCALE
}

fn hash_karaoke_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn measure_karaoke_ui_text_width(text: &str, font_size: f32) -> f32 {
    crate::vector_text::measure_rythmo_text_width_standalone(text, font_size)
        .map(|width| width.ceil() + 1.0)
        .unwrap_or_else(|| estimate_karaoke_ui_text_width(text, font_size))
        .max(2.0)
}

fn estimate_karaoke_ui_text_width(text: &str, font_size: f32) -> f32 {
    let char_count = text.chars().count().max(1) as f32;
    (char_count * font_size * 0.62 + font_size * 0.7).max(2.0)
}

fn karaoke_ui_text_width(text: &str) -> f32 {
    measure_karaoke_ui_text_width(text, karaoke_ui_font_size())
}

fn line_visual_x_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> (f32, f32) {
    line_visual_x_width_with_karaoke_width(line, current_frame, zone, karaoke_preview, None)
}

fn line_visual_x_width_with_karaoke_width(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    active_karaoke_width: Option<f32>,
) -> (f32, f32) {
    let center_x = zone.x + zone.width / 2.0;
    if karaoke_preview && line.karaoke_active(current_frame) {
        let width = active_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        return (center_x - width / 2.0, width);
    } else if karaoke_preview {
        return line.visual_x_width(current_frame, center_x, ppf(), zone.width, 1.0);
    }

    let x1 = frame_to_x(line.start_frame, current_frame, zone);
    let x2 = frame_to_x(line.end_frame(), current_frame, zone);
    (x1, (x2 - x1).max(2.0))
}

fn badge_rect_for_line(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    badge_rect_for_line_with_karaoke_preview(project, line, current_frame, zone, false)
}

fn badge_rect_for_line_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(
        project,
        line,
        &line.character_name,
        current_frame,
        zone,
        karaoke_preview,
    )
}

fn badge_rect_for_name(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    badge_rect_for_name_with_karaoke_preview(project, line, name, current_frame, zone, false)
}

fn badge_rect_for_name_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    name: &str,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> Rect {
    let (x1, _) = line_visual_x_width(line, current_frame, zone, karaoke_preview);
    let body_rect = editor_track_body_rect(project, line.y_slot, zone);
    let w = badge_width(name);
    // Right edge a few px to the left of the line's top-left corner, top-aligned.
    let right = x1 - BADGE_GAP;
    Rect {
        x: right - w,
        y: body_rect.y,
        width: w,
        height: body_rect.height * BADGE_OVERLAP_HEIGHT_RATIO,
    }
}

fn color_picker_origin_for_badge(badge: &Rect, zone: &Rect) -> (f32, f32) {
    let (picker_w, picker_h) = super::color_picker::ColorPickerState::panel_size();
    let gap = 10.0;
    let zone_right = zone.x + zone.width;
    let zone_bottom = zone.y + zone.height;

    let right_x = badge.x + badge.width + gap;
    let x = if right_x + picker_w <= zone_right {
        right_x
    } else {
        (badge.x - picker_w - gap).max(zone.x)
    };

    let y = (badge.y - 8.0).clamp(zone.y, (zone_bottom - picker_h).max(zone.y));
    (x, y)
}

fn collect_track_usage(project: &Project) -> (Vec<bool>, Vec<bool>) {
    let track_count = rythmo_layout::track_count();
    let mut used_tracks = vec![false; track_count];
    let mut karaoke_tracks = vec![false; track_count];
    for line in project.lines() {
        let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
        used_tracks[track_index] = true;
        if line.karaoke {
            karaoke_tracks[track_index] = true;
        }
    }

    if !used_tracks.iter().any(|used| *used) && !used_tracks.is_empty() {
        used_tracks[0] = true;
    }

    (used_tracks, karaoke_tracks)
}

fn track_indices_from_usage(used_tracks: &[bool]) -> Vec<usize> {
    let mut tracks: Vec<usize> = used_tracks
        .iter()
        .enumerate()
        .filter_map(|(track_index, used)| used.then_some(track_index))
        .collect();
    if tracks.is_empty() {
        tracks.push(0);
    }
    tracks
}

fn build_track_layouts_from_karaoke_flags(
    track_indices: &[usize],
    karaoke_tracks: &[bool],
    normal_body_h: f32,
    slot_header_h: f32,
    badge_gap: f32,
    scale: f32,
) -> Vec<rythmo_layout::TrackLayout> {
    let mut top = 0.0;
    track_indices
        .iter()
        .map(|&track_index| {
            let has_karaoke = karaoke_tracks.get(track_index).copied().unwrap_or(false);
            let body_h = if has_karaoke {
                rythmo_layout::karaoke_track_body_height(normal_body_h, scale)
            } else {
                normal_body_h
            };
            let total_h = slot_header_h + badge_gap + body_h;
            let layout = rythmo_layout::TrackLayout {
                track_index,
                top,
                total_h,
                body_h,
                has_karaoke,
            };
            top += total_h;
            layout
        })
        .collect()
}

fn editor_normal_body_height_for_karaoke_tracks(karaoke_track_count: usize, zone: &Rect) -> f32 {
    let track_count = rythmo_layout::track_count();
    let usable_h = (zone.height - constants::RULER_HEIGHT).max(1.0);
    let header_total = track_count as f32 * (slot_header_height() + BADGE_GAP);
    let weighted_rows = (track_count + karaoke_track_count).max(1) as f32;
    let mut body_h = ((usable_h - header_total) / weighted_rows).max(8.0);
    for _ in 0..4 {
        let stack_gaps =
            karaoke_track_count as f32 * rythmo_layout::karaoke_stack_gap(body_h * 2.0, 1.0);
        body_h = ((usable_h - header_total - stack_gaps) / weighted_rows).max(8.0);
    }
    body_h
}

fn editor_normal_body_height(project: &Project, zone: &Rect) -> f32 {
    let (_, karaoke_tracks) = collect_track_usage(project);
    let karaoke_track_count = karaoke_tracks
        .iter()
        .filter(|has_karaoke| **has_karaoke)
        .count();
    editor_normal_body_height_for_karaoke_tracks(karaoke_track_count, zone)
}

struct EditorLayoutCtx {
    normal_body_h: f32,
    track_layouts: Vec<rythmo_layout::TrackLayout>,
    track_by_index: Vec<Option<rythmo_layout::TrackLayout>>,
}

impl EditorLayoutCtx {
    fn new(project: &Project, zone: &Rect) -> Self {
        let (_, karaoke_tracks) = collect_track_usage(project);
        Self::from_karaoke_tracks(&karaoke_tracks, zone)
    }

    fn from_karaoke_tracks(karaoke_tracks: &[bool], zone: &Rect) -> Self {
        let karaoke_track_count = karaoke_tracks
            .iter()
            .filter(|has_karaoke| **has_karaoke)
            .count();
        let normal_body_h = editor_normal_body_height_for_karaoke_tracks(karaoke_track_count, zone);
        let track_layouts = build_track_layouts_from_karaoke_flags(
            &rythmo_layout::all_track_indices(),
            &karaoke_tracks,
            normal_body_h,
            slot_header_height(),
            BADGE_GAP,
            1.0,
        );
        Self::from_track_layouts(normal_body_h, track_layouts)
    }

    fn from_track_layouts(
        normal_body_h: f32,
        track_layouts: Vec<rythmo_layout::TrackLayout>,
    ) -> Self {
        let mut track_by_index = vec![None; rythmo_layout::track_count()];
        for layout in &track_layouts {
            if let Some(slot) = track_by_index.get_mut(layout.track_index) {
                *slot = Some(*layout);
            }
        }

        Self {
            normal_body_h,
            track_layouts,
            track_by_index,
        }
    }

    fn track_for_index(&self, track_index: usize) -> Option<&rythmo_layout::TrackLayout> {
        self.track_by_index
            .get(track_index)
            .and_then(|layout| layout.as_ref())
    }

    fn track_for_y_slot(&self, y_slot: f32) -> &rythmo_layout::TrackLayout {
        let track_index = rythmo_layout::track_index_for_y_slot(y_slot);
        self.track_for_index(track_index).unwrap_or_else(|| {
            self.track_layouts
                .first()
                .expect("editor track layout should not be empty")
        })
    }

    fn track_y_base(&self, y_slot: f32, zone: &Rect) -> f32 {
        zone.y + constants::RULER_HEIGHT + self.track_for_y_slot(y_slot).top
    }

    fn track_body_rect(&self, y_slot: f32, zone: &Rect) -> Rect {
        let layout = self.track_for_y_slot(y_slot);
        Rect {
            x: zone.x,
            y: zone.y + constants::RULER_HEIGHT + layout.top + slot_header_height() + BADGE_GAP,
            width: zone.width,
            height: layout.body_h,
        }
    }

    fn line_rect_with_karaoke_width(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
        zone: &Rect,
        karaoke_preview: bool,
        active_karaoke_width: Option<f32>,
    ) -> Rect {
        let (x1, width) = line_visual_x_width_with_karaoke_width(
            line,
            current_frame,
            zone,
            karaoke_preview,
            active_karaoke_width,
        );
        let body_rect = self.track_body_rect(line.y_slot, zone);
        Rect {
            x: x1,
            y: body_rect.y,
            width,
            height: self.normal_body_h,
        }
    }

    fn badge_rect_for_name(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        name: &str,
        x: f32,
        zone: &Rect,
    ) -> Rect {
        let body_rect = self.track_body_rect(line.y_slot, zone);
        let badge_h = body_rect.height * BADGE_OVERLAP_HEIGHT_RATIO;
        let w = badge_width(name);
        // Right edge a few px to the left of the line's top-left corner;
        // badge extends leftward from there, top-aligned to the line.
        let right = x - BADGE_GAP;
        Rect {
            x: right - w,
            y: body_rect.y,
            width: w,
            height: badge_h,
        }
    }
}

fn editor_track_layouts(project: &Project, zone: &Rect) -> Vec<rythmo_layout::TrackLayout> {
    EditorLayoutCtx::new(project, zone).track_layouts
}

fn editor_track_body_rect(project: &Project, y_slot: f32, zone: &Rect) -> Rect {
    EditorLayoutCtx::new(project, zone).track_body_rect(y_slot, zone)
}

fn line_rect(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
) -> Rect {
    line_rect_with_karaoke_preview(project, line, current_frame, zone, false)
}

fn line_rect_with_karaoke_preview(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
) -> Rect {
    EditorLayoutCtx::new(project, zone).line_rect_with_karaoke_width(
        line,
        current_frame,
        zone,
        karaoke_preview,
        None,
    )
}

fn badge_width(name: &str) -> f32 {
    (text_input::text_width(name, BADGE_FONT_SIZE) + BADGE_PADDING_H * 2.0).max(BADGE_MIN_W)
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn badge_text_metrics() -> TextInputMetrics {
    TextInputMetrics::center(BADGE_FONT_SIZE, BADGE_PADDING_H)
}

fn note_text_metrics() -> TextInputMetrics {
    TextInputMetrics::left(9.0, 0.0)
}

fn push_playhead_segments(
    quads: &mut Vec<QuadInstance>,
    x: f32,
    width: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    shadow_color: [f32; 4],
    shadow_blur: f32,
    skip_ranges: &[(f32, f32)],
) {
    let mut ranges: Vec<(f32, f32)> = skip_ranges
        .iter()
        .map(|(start, end)| (start.max(y), end.min(y + height)))
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut cursor_y = y;
    for (skip_start, skip_end) in ranges {
        if skip_start > cursor_y {
            quads.push(QuadInstance {
                rect: [x, cursor_y, width, skip_start - cursor_y],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0, 0.0],
                shadow_color,
                shadow_blur,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        cursor_y = cursor_y.max(skip_end);
    }

    let end_y = y + height;
    if cursor_y < end_y {
        quads.push(QuadInstance {
            rect: [x, cursor_y, width, end_y - cursor_y],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0, 0.0],
            shadow_color,
            shadow_blur,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
}

fn active_karaoke_skip_ranges(
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    zone: &Rect,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
) -> Vec<(f32, f32)> {
    if !karaoke_preview {
        return Vec::new();
    }

    let max_gap_frames = karaoke_adjacent_max_gap_frames(fps);
    let karaoke_index = state.cached_karaoke_ui_index(project, max_gap_frames);
    let layout_ctx = EditorLayoutCtx::from_karaoke_tracks(karaoke_index.karaoke_tracks(), zone);
    let first_frame = f64_floor_to_i64(current_frame);
    let last_frame = f64_ceil_to_i64(current_frame);
    render_index
        .visible_line_ids(project, first_frame, last_frame)
        .into_iter()
        .filter_map(|line_id| project.get_line(line_id))
        .filter(|line| line.karaoke && line.karaoke_active(current_frame))
        .map(|line| {
            let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
            let rect = karaoke_stack_rect(
                Rect {
                    x: body_rect.x,
                    y: body_rect.y,
                    width: body_rect.width,
                    height: body_rect.height,
                },
                karaoke_index.stack_row(line),
                1.0,
            );
            (rect.y, rect.y + rect.height)
        })
        .collect()
}

pub fn render_rythmo_base(
    zone: &Rect,
    project: &Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    waveform: &[f32],
    waveform_offset_frames: i64,
    waveform_is_instrumental: bool,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
) -> Vec<QuadInstance> {
    let mut quads = Vec::new();

    // Waveform (rendered first, behind playhead)
    // waveform has WAVEFORM_SUBDIVISIONS (4) entries per video frame
    let (wave_top, wave_bottom) = if waveform_is_instrumental {
        ([0.30, 0.90, 0.45, 0.85], [0.10, 0.62, 0.25, 0.4])
    } else {
        ([0.4, 0.65, 1.0, 0.85], [0.2, 0.45, 0.85, 0.4])
    };
    if !waveform.is_empty() {
        let subs = 4usize; // must match WAVEFORM_SUBDIVISIONS in video.rs
        let ruler_h = constants::RULER_HEIGHT;
        let sub_ppf = ppf() / subs as f32; // pixels per sub-frame
        let bar_w = sub_ppf.max(1.0);
        let visible_frames = (zone.width / ppf()) as i64 + 4;
        let half_visible_frames = visible_frames as f64 / 2.0;
        let first_frame = f64_floor_to_i64(current_frame - half_visible_frames);
        let last_frame = f64_ceil_to_i64(current_frame + half_visible_frames);
        let first_wave_frame = first_frame.saturating_sub(waveform_offset_frames);
        let last_wave_frame = last_frame.saturating_sub(waveform_offset_frames);
        let first_sub = first_wave_frame
            .saturating_mul(subs as i64)
            .clamp(0, waveform.len() as i64);
        let last_sub = last_wave_frame
            .saturating_add(1)
            .saturating_mul(subs as i64)
            .clamp(0, waveform.len() as i64);

        for si in first_sub..last_sub {
            let amp = waveform[si as usize].min(1.0);
            let bar_h = amp * ruler_h;
            if bar_h < 0.3 {
                continue;
            }

            // Position: which video frame + sub offset
            let frame = (si / subs as i64).saturating_add(waveform_offset_frames);
            let sub_offset = (si % subs as i64) as f32;
            let x = frame_to_x(frame, current_frame, zone) + sub_offset * sub_ppf;
            if x < zone.x || x > zone.x + zone.width {
                continue;
            }

            quads.push(QuadInstance {
                rect: [x, zone.y + ruler_h - bar_h, bar_w, bar_h],
                color: wave_top,
                color_bottom: wave_bottom,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    } else if waveform_is_instrumental {
        let ruler_h = constants::RULER_HEIGHT;
        let bar_w = 3.0;
        let step = 8.0;
        let mut x = zone.x;
        let mut i = 0.0_f32;
        while x < zone.x + zone.width {
            let amp = (0.25 + (i * 0.55).sin().abs() * 0.55).clamp(0.0, 1.0);
            let bar_h = amp * ruler_h;
            quads.push(QuadInstance {
                rect: [x, zone.y + ruler_h - bar_h, bar_w, bar_h],
                color: wave_top,
                color_bottom: wave_bottom,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            x += step;
            i += 1.0;
        }
    }

    if state.audio_offset_mode {
        quads.push(QuadInstance {
            rect: [zone.x, zone.y, zone.width, constants::RULER_HEIGHT],
            color: [1.0, 0.55, 0.10, 0.10],
            color_bottom: [1.0, 0.55, 0.10, 0.10],
            border_color: [1.0, 0.62, 0.18, 0.9],
            border_width: 1.5,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    // Ticks removed from UI (kept in CPU/GPU export renderers)

    let playhead_x = zone.x + (zone.width - PLAYHEAD_WIDTH) / 2.0;
    let skip_ranges = active_karaoke_skip_ranges(
        project,
        render_index,
        current_frame,
        zone,
        karaoke_preview,
        fps,
        state,
    );
    push_playhead_segments(
        &mut quads,
        playhead_x,
        PLAYHEAD_WIDTH,
        zone.y,
        zone.height,
        PLAYHEAD_COLOR,
        [0.85, 0.15, 0.15, 0.3],
        4.0,
        &skip_ranges,
    );

    quads
}

/// Returns optional (line_id, cursor_pos, text_x, text_w, rect_y, rect_h) for cursor rendering.
const BADGE_HEIGHT: f32 = 13.0;
const BADGE_PADDING_H: f32 = 8.0;
const BADGE_GAP: f32 = 2.0;
const BADGE_MIN_W: f32 = 24.0;
const BADGE_FONT_SIZE: f32 = 11.0;

// Badge height ~1/3 of line height, positioned at line left edge
const BADGE_OVERLAP_HEIGHT_RATIO: f32 = constants::BADGE_OVERLAP_HEIGHT_RATIO;
const ACTOR_ICON_SIZE: f32 = constants::VOICE_ACTOR_DISPLAY_ICON_SIZE;
const ACTOR_ICON_GAP: f32 = 3.0;

fn slot_header_height() -> f32 {
    BADGE_HEIGHT.max(ACTOR_ICON_SIZE)
}

fn line_color_tint(line: &crate::rythmo_line::RythmoLine) -> [f32; 4] {
    [
        line.character_color[0].clamp(0.0, 1.0),
        line.character_color[1].clamp(0.0, 1.0),
        line.character_color[2].clamp(0.0, 1.0),
        1.0,
    ]
}

fn line_color_label(line: &crate::rythmo_line::RythmoLine) -> [u8; 3] {
    [
        (line.character_color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (line.character_color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (line.character_color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

#[derive(Clone, Copy)]
struct KaraokeProgressRenderInfo {
    visual_progress: f32,
    local_progress: f32,
}

fn karaoke_progress_render_info(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    lang: &str,
) -> Option<KaraokeProgressRenderInfo> {
    let progress = line.karaoke_progress(current_frame)?;
    let ratios = crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang);
    let local_progress = crate::syllable::active_syllable_local_progress(&ratios, progress)
        .unwrap_or(progress)
        .clamp(0.0, 1.0);
    let visual_progress = crate::syllable::visual_progress_from_timing(
        &line.text,
        &line.syllable_ratios,
        lang,
        progress,
    );

    Some(KaraokeProgressRenderInfo {
        visual_progress,
        local_progress,
    })
}

fn push_plain_rythmo_text(
    stretched: &mut Vec<StretchedText>,
    line_id: u64,
    text: String,
    dest_rect: Rect,
) {
    stretched.push(StretchedText::new(line_id, text, dest_rect));
}

fn push_natural_karaoke_text(
    stretched: &mut Vec<StretchedText>,
    line_id: u64,
    text: String,
    dest_rect: Rect,
    tint: [f32; 4],
) {
    stretched.push(StretchedText::natural(
        line_id,
        text,
        dest_rect,
        constants::KARAOKE_TEXT_FONT_SCALE,
        tint,
    ));
}

fn syllable_segment_cache_id(line_id: u64, segment_index: usize) -> u64 {
    (1_u64 << 63) ^ line_id.wrapping_mul(1_000_003) ^ (segment_index as u64).wrapping_add(1)
}

fn same_karaoke_track(
    a: &crate::rythmo_line::RythmoLine,
    b: &crate::rythmo_line::RythmoLine,
) -> bool {
    rythmo_layout::track_index_for_y_slot(a.y_slot)
        == rythmo_layout::track_index_for_y_slot(b.y_slot)
}

fn karaoke_adjacent_max_gap_frames(fps: f64) -> i64 {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    };
    (constants::KARAOKE_ADJACENT_MAX_GAP_SECONDS * fps).round() as i64
}

fn karaoke_count_in_frames(fps: f64) -> i64 {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    };
    (constants::KARAOKE_COUNT_IN_SECONDS * fps).round().max(1.0) as i64
}

fn karaoke_count_in_progress(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    count_in_frames: i64,
) -> Option<f32> {
    if !line.karaoke || current_frame >= line.start_frame as f64 || count_in_frames <= 0 {
        return None;
    }

    let count_in_start = (line.start_frame - count_in_frames) as f64;
    if current_frame < count_in_start {
        return None;
    }

    Some(((current_frame - count_in_start) as f32 / count_in_frames as f32).clamp(0.0, 1.0))
}

fn karaoke_count_in_visible(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    count_in_frames: i64,
) -> bool {
    karaoke_count_in_progress(line, current_frame, count_in_frames).is_some()
}

fn karaoke_previous_gap_frames(
    previous: &crate::rythmo_line::RythmoLine,
    line: &crate::rythmo_line::RythmoLine,
) -> i64 {
    (line.start_frame - previous.end_frame()).max(0)
}

#[cfg(test)]
fn karaoke_next_gap_frames(
    line: &crate::rythmo_line::RythmoLine,
    next: &crate::rythmo_line::RythmoLine,
) -> i64 {
    (next.start_frame - line.end_frame()).max(0)
}

const KARAOKE_UI_SIGNATURE_OFFSET: u64 = 0xcbf29ce484222325;
const KARAOKE_UI_SIGNATURE_PRIME: u64 = 0x100000001b3;

fn karaoke_signature_mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(KARAOKE_UI_SIGNATURE_PRIME);
}

#[cfg(test)]
fn karaoke_signature_mix_str(hash: &mut u64, value: &str) {
    karaoke_signature_mix(hash, value.len() as u64);
    for &byte in value.as_bytes() {
        karaoke_signature_mix(hash, byte as u64);
    }
}

fn karaoke_ui_index_revision_signature(project: &Project, max_gap_frames: i64) -> u64 {
    let mut hash = KARAOKE_UI_SIGNATURE_OFFSET;
    karaoke_signature_mix(&mut hash, project.revision());
    karaoke_signature_mix(&mut hash, max_gap_frames as u64);
    hash
}

#[cfg(test)]
fn karaoke_ui_index_signature(project: &Project, max_gap_frames: i64) -> u64 {
    let mut hash = KARAOKE_UI_SIGNATURE_OFFSET;
    karaoke_signature_mix(&mut hash, project.line_count() as u64);
    karaoke_signature_mix(&mut hash, max_gap_frames as u64);
    for line in project.lines() {
        karaoke_signature_mix(&mut hash, line.id);
        karaoke_signature_mix(&mut hash, line.start_frame as u64);
        karaoke_signature_mix(&mut hash, line.duration_frames as u64);
        karaoke_signature_mix(
            &mut hash,
            rythmo_layout::track_index_for_y_slot(line.y_slot) as u64,
        );
        karaoke_signature_mix(&mut hash, line.karaoke as u64);
        if line.karaoke {
            karaoke_signature_mix_str(&mut hash, &line.character_name);
        }
    }
    hash
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct KaraokeLineUiState {
    previous_adjacent_karaoke_id: Option<u64>,
    previous_adjacent_start_frame: Option<i64>,
    stack_row: usize,
    label_visible: bool,
    island_index: usize,
}

struct KaraokeUiIndex {
    signature: u64,
    by_line_id: HashMap<u64, KaraokeLineUiState>,
    used_tracks: Vec<bool>,
    karaoke_tracks: Vec<bool>,
    karaoke_timeline: Vec<(i64, u64)>,
}

impl KaraokeUiIndex {
    #[cfg(test)]
    fn new(project: &Project, max_gap_frames: i64) -> Self {
        Self::new_with_signature(
            project,
            max_gap_frames,
            karaoke_ui_index_signature(project, max_gap_frames),
        )
    }

    fn new_with_signature(project: &Project, max_gap_frames: i64, signature: u64) -> Self {
        let track_count = rythmo_layout::track_count();
        let mut used_tracks = vec![false; track_count];
        let mut karaoke_tracks = vec![false; track_count];
        let mut karaoke_timeline = Vec::new();
        let mut lines_by_track: Vec<Vec<&crate::rythmo_line::RythmoLine>> =
            (0..track_count).map(|_| Vec::new()).collect();
        for line in project.lines() {
            let track_index = rythmo_layout::track_index_for_y_slot(line.y_slot);
            used_tracks[track_index] = true;
            if line.karaoke {
                karaoke_tracks[track_index] = true;
                karaoke_timeline.push((line.start_frame, line.id));
            }
            if let Some(track_lines) = lines_by_track.get_mut(track_index) {
                track_lines.push(line);
            }
        }
        if !used_tracks.iter().any(|used| *used) && !used_tracks.is_empty() {
            used_tracks[0] = true;
        }
        karaoke_timeline.sort_unstable_by_key(|&(start_frame, line_id)| (start_frame, line_id));

        let mut by_line_id = HashMap::with_capacity(project.line_count());
        for track_lines in &mut lines_by_track {
            track_lines.sort_by_key(|line| (line.start_frame, line.id));
            let mut previous_line: Option<&crate::rythmo_line::RythmoLine> = None;
            for line in track_lines.iter().copied() {
                if line.karaoke {
                    let previous_adjacent = previous_line.and_then(|previous| {
                        if previous.karaoke
                            && karaoke_previous_gap_frames(previous, line) <= max_gap_frames
                        {
                            Some(previous)
                        } else {
                            None
                        }
                    });
                    let island_index = previous_adjacent
                        .and_then(|previous| by_line_id.get(&previous.id))
                        .map(|previous_state: &KaraokeLineUiState| previous_state.island_index + 1)
                        .unwrap_or_else(|| {
                            if previous_line.is_some_and(|previous| !previous.karaoke) {
                                1
                            } else {
                                0
                            }
                        });
                    let label_visible = !line.character_name.is_empty()
                        && previous_adjacent
                            .map(|previous| previous.character_name != line.character_name)
                            .unwrap_or(true);
                    by_line_id.insert(
                        line.id,
                        KaraokeLineUiState {
                            previous_adjacent_karaoke_id: previous_adjacent.map(|line| line.id),
                            previous_adjacent_start_frame: previous_adjacent
                                .map(|line| line.start_frame),
                            stack_row: island_index % 2,
                            label_visible,
                            island_index,
                        },
                    );
                }
                previous_line = Some(line);
            }
        }

        Self {
            signature,
            by_line_id,
            used_tracks,
            karaoke_tracks,
            karaoke_timeline,
        }
    }

    fn timeline_cursor_at(&self, frame: i64) -> usize {
        self.karaoke_timeline
            .partition_point(|(start_frame, _)| *start_frame < frame)
            .min(self.karaoke_timeline.len().saturating_sub(1))
    }

    fn used_tracks(&self) -> &[bool] {
        &self.used_tracks
    }

    fn karaoke_tracks(&self) -> &[bool] {
        &self.karaoke_tracks
    }

    fn line_state(&self, line: &crate::rythmo_line::RythmoLine) -> KaraokeLineUiState {
        self.by_line_id.get(&line.id).copied().unwrap_or_default()
    }

    #[cfg(test)]
    fn previous_adjacent_karaoke_id(&self, line: &crate::rythmo_line::RythmoLine) -> Option<u64> {
        self.line_state(line).previous_adjacent_karaoke_id
    }

    fn stack_row(&self, line: &crate::rythmo_line::RythmoLine) -> usize {
        self.line_state(line).stack_row
    }

    fn prestart_scroll_visible(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
        count_in_frames: i64,
    ) -> bool {
        line.karaoke
            && karaoke_count_in_visible(line, current_frame, count_in_frames)
            && self.line_state(line).previous_adjacent_karaoke_id.is_none()
    }

    fn upcoming_stack_visible(
        &self,
        line: &crate::rythmo_line::RythmoLine,
        current_frame: f64,
    ) -> bool {
        if !line.karaoke || current_frame >= line.start_frame as f64 {
            return false;
        }

        self.line_state(line)
            .previous_adjacent_start_frame
            .is_some_and(|start_frame| current_frame >= start_frame as f64)
    }

    fn character_label_visible(&self, line: &crate::rythmo_line::RythmoLine) -> bool {
        self.line_state(line).label_visible
    }
}

fn previous_line_on_same_track_before<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && same_karaoke_track(candidate, line)
                && (candidate.start_frame < line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id < line.id))
        })
        .max_by_key(|candidate| (candidate.start_frame, candidate.id))
}

#[cfg(test)]
fn next_line_on_same_track_after<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    project
        .lines()
        .filter(|candidate| {
            candidate.id != line.id
                && same_karaoke_track(candidate, line)
                && (candidate.start_frame > line.start_frame
                    || (candidate.start_frame == line.start_frame && candidate.id > line.id))
        })
        .min_by_key(|candidate| (candidate.start_frame, candidate.id))
}

fn previous_karaoke_line_before<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    let previous = previous_line_on_same_track_before(project, line)?;
    if previous.karaoke && karaoke_previous_gap_frames(previous, line) <= max_gap_frames {
        Some(previous)
    } else {
        None
    }
}

#[cfg(test)]
fn next_karaoke_line_after<'a>(
    project: &'a Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> Option<&'a crate::rythmo_line::RythmoLine> {
    let next = next_line_on_same_track_after(project, line)?;
    if next.karaoke && karaoke_next_gap_frames(line, next) <= max_gap_frames {
        Some(next)
    } else {
        None
    }
}

fn karaoke_prestart_scroll_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
    count_in_frames: i64,
) -> bool {
    line.karaoke
        && karaoke_count_in_visible(line, current_frame, count_in_frames)
        && previous_karaoke_line_before(project, line, max_gap_frames).is_none()
}

fn karaoke_upcoming_stack_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || current_frame >= line.start_frame as f64 {
        return false;
    }

    previous_karaoke_line_before(project, line, max_gap_frames)
        .is_some_and(|previous| current_frame >= previous.start_frame as f64)
}

fn karaoke_island_index(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> usize {
    let mut index = 0;
    let mut current = line;
    while let Some(previous) = previous_karaoke_line_before(project, current, max_gap_frames) {
        index += 1;
        current = previous;
    }
    if previous_line_on_same_track_before(project, current)
        .is_some_and(|previous| !previous.karaoke)
    {
        index += 1;
    }
    index
}

fn karaoke_stack_row(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> usize {
    karaoke_island_index(project, line, max_gap_frames) % 2
}

fn karaoke_stack_rect(mut rect: Rect, row: usize, scale: f32) -> Rect {
    let gap = rythmo_layout::karaoke_stack_gap(rect.height, scale);
    let row_h = ((rect.height - gap).max(1.0) / 2.0).max(1.0);
    rect.y += row.min(1) as f32 * (row_h + gap);
    rect.height = row_h;
    rect
}

fn karaoke_character_label_visible(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    max_gap_frames: i64,
) -> bool {
    if !line.karaoke || line.character_name.is_empty() {
        return false;
    }

    previous_karaoke_line_before(project, line, max_gap_frames)
        .map(|previous| previous.character_name != line.character_name)
        .unwrap_or(true)
}

fn karaoke_centered_x_width_with_width(zone: &Rect, width: f32) -> (f32, f32) {
    let center_x = zone.x + zone.width / 2.0;
    (center_x - width / 2.0, width)
}

fn karaoke_preview_line_rect_with_state(
    layout_ctx: &EditorLayoutCtx,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    prestart_count_in: bool,
    upcoming_stack: bool,
    stack_row: usize,
    centered_karaoke_width: Option<f32>,
) -> Rect {
    let (x1, width) = if line.karaoke_active(current_frame) || prestart_count_in || upcoming_stack {
        let width = centered_karaoke_width.unwrap_or_else(|| karaoke_ui_text_width(&line.text));
        karaoke_centered_x_width_with_width(zone, width)
    } else {
        line_visual_x_width(line, current_frame, zone, true)
    };
    let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
    let rect = Rect {
        x: x1,
        y: body_rect.y,
        width,
        height: body_rect.height,
    };
    karaoke_stack_rect(rect, stack_row, 1.0)
}

fn karaoke_preview_line_rect(
    project: &Project,
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    zone: &Rect,
    max_gap_frames: i64,
) -> Rect {
    let upcoming_stack =
        karaoke_upcoming_stack_visible(project, line, current_frame, max_gap_frames);
    let layout_ctx = EditorLayoutCtx::new(project, zone);
    karaoke_preview_line_rect_with_state(
        &layout_ctx,
        line,
        current_frame,
        zone,
        false,
        upcoming_stack,
        karaoke_stack_row(project, line, max_gap_frames),
        None,
    )
}

fn badge_rect_for_karaoke_rect(line: &crate::rythmo_line::RythmoLine, line_rect: &Rect) -> Rect {
    let width = badge_width(&line.character_name);
    let badge_h = line_rect.height * BADGE_OVERLAP_HEIGHT_RATIO;
    // Right edge a few px to the left of the line's top-left corner, top-aligned.
    Rect {
        x: line_rect.x - width - BADGE_GAP,
        y: line_rect.y,
        width,
        height: badge_h,
    }
}

fn visible_syllable_segments(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
) -> Option<(Vec<usize>, Vec<f32>)> {
    if line.text.is_empty() || line.text == "↑" || line.text == "↓" {
        return None;
    }

    let breaks = state.get_syllable_breaks(line, lang);
    if breaks.is_empty() {
        return None;
    }

    let drag = drag.filter(|drag| drag.line_id == line.id);
    let has_drag = drag.is_some();
    let has_saved = line.syllable_ratios.len() == breaks.len() + 1;
    let ratios = if let Some(drag) = drag {
        drag.ratios.clone()
    } else if has_saved {
        line.syllable_ratios.clone()
    } else if line.karaoke && !karaoke_preview {
        crate::syllable::timing_ratios(&line.text, &line.syllable_ratios, lang)
    } else {
        Vec::new()
    };

    if has_drag || has_saved || !ratios.is_empty() {
        Some((breaks, ratios))
    } else {
        None
    }
}

fn cursor_ratios_from_segments(text: &str, breaks: &[usize], ratios: &[f32]) -> Vec<f32> {
    let char_count = text.chars().count();
    let mut cursor_ratios = vec![0.0; char_count + 1];
    let mut seg_start = 0usize;
    let mut x = 0.0;

    for (i, ratio) in ratios.iter().enumerate() {
        let seg_end = if i < breaks.len() {
            breaks[i].min(char_count)
        } else {
            char_count
        };
        let seg_len = seg_end.saturating_sub(seg_start);
        if seg_len > 0 {
            for local_idx in 0..=seg_len {
                let char_idx = seg_start + local_idx;
                if char_idx <= char_count {
                    let local_ratio = local_idx as f32 / seg_len as f32;
                    cursor_ratios[char_idx] = (x + local_ratio * ratio).clamp(0.0, 1.0);
                }
            }
        }
        x += ratio;
        seg_start = seg_end;
    }

    if let Some(last) = cursor_ratios.last_mut() {
        *last = 1.0;
    }
    cursor_ratios
}

fn closest_cursor_index_from_ratios(ratios: &[f32], x_ratio: f32) -> Option<usize> {
    ratios
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - x_ratio)
                .abs()
                .partial_cmp(&(*b - x_ratio).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
}

fn segmented_cursor_ratios_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
) -> Option<Vec<f32>> {
    let (breaks, ratios) = visible_syllable_segments(line, drag, lang, karaoke_preview, state)?;
    Some(cursor_ratios_from_segments(&line.text, &breaks, &ratios))
}

pub(super) fn cursor_segments_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
) -> Option<Vec<CursorSegmentInfo>> {
    let (breaks, ratios) = visible_syllable_segments(line, drag, lang, karaoke_preview, state)?;
    let mut start_char = 0usize;
    let mut start_ratio = 0.0;
    let mut segments = Vec::new();

    for (i, ratio) in ratios.iter().enumerate() {
        let end_char = if i < breaks.len() {
            breaks[i]
        } else {
            line.text.chars().count()
        };
        if end_char > start_char && *ratio > 0.0 {
            segments.push(CursorSegmentInfo {
                cache_id: syllable_segment_cache_id(line.id, i),
                start_char,
                end_char,
                start_ratio,
                width_ratio: *ratio,
            });
        }
        start_char = end_char;
        start_ratio = (start_ratio + ratio).clamp(0.0, 1.0);
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

pub(super) fn segmented_cursor_index_for_line_at_ratio(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
    x_ratio: f32,
) -> Option<usize> {
    let ratios = segmented_cursor_ratios_for_line(line, drag, lang, karaoke_preview, state)?;
    closest_cursor_index_from_ratios(&ratios, x_ratio)
}

fn cursor_index_for_line_at_ratio(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
    karaoke_preview: bool,
    state: &RythmoState,
    x_ratio: f32,
) -> usize {
    if let Some(idx) =
        segmented_cursor_index_for_line_at_ratio(line, drag, lang, karaoke_preview, state, x_ratio)
    {
        idx
    } else {
        (x_ratio * line.text.chars().count() as f32).round() as usize
    }
}

fn push_karaoke_rythmo_text(
    stretched: &mut Vec<StretchedText>,
    line: &crate::rythmo_line::RythmoLine,
    dest_rect: Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
) {
    push_natural_karaoke_text(
        stretched,
        line.id,
        line.text.clone(),
        dest_rect,
        [1.0, 1.0, 1.0, 1.0],
    );

    let Some(progress_info) = progress_info else {
        return;
    };
    if let Some(colored) = StretchedText::natural_clipped(
        line.id,
        line.text.clone(),
        dest_rect,
        progress_info.visual_progress,
        constants::KARAOKE_TEXT_FONT_SCALE,
        line_color_tint(line),
    ) {
        stretched.push(colored);
    }
}

fn push_editor_karaoke_texture_prewarm_texts(
    stretched: &mut Vec<StretchedText>,
    state: &RythmoState,
    project: &Project,
    index: &KaraokeUiIndex,
    layout_ctx: &EditorLayoutCtx,
    current_frame: i64,
    fps: f64,
    zone: &Rect,
) {
    let lookahead_frames =
        (fps.max(1.0) * KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS).round() as i64;
    let start = index.timeline_cursor_at(current_frame - lookahead_frames / 10);
    let end_frame = current_frame + lookahead_frames;
    let mut pushed = 0;

    for &(start_frame, line_id) in index
        .karaoke_timeline
        .iter()
        .skip(start)
        .take(KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME)
    {
        if start_frame > end_frame {
            break;
        }
        let Some(line) = project.get_line(line_id) else {
            continue;
        };
        if line.text.is_empty() || line.text == "↑" || line.text == "↓" {
            continue;
        }

        let body_rect = layout_ctx.track_body_rect(line.y_slot, zone);
        let row_rect = karaoke_stack_rect(
            Rect {
                x: zone.x,
                y: body_rect.y,
                width: state.karaoke_ui_text_width_for_render(line),
                height: body_rect.height,
            },
            index.stack_row(line),
            1.0,
        );
        stretched.push(StretchedText::natural_prewarm(
            line.id,
            line.text.clone(),
            row_rect,
            constants::KARAOKE_TEXT_FONT_SCALE,
        ));
        pushed += 1;
        if pushed >= KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME {
            break;
        }
    }
}

fn push_studio_karaoke_texture_prewarm_texts(
    stretched: &mut Vec<StretchedText>,
    state: &RythmoState,
    project: &Project,
    index: &KaraokeUiIndex,
    track_layouts: &[rythmo_layout::TrackLayout],
    current_frame: i64,
    fps: f64,
    zone: &Rect,
    ruler_h: f32,
    slot_header_h: f32,
    badge_gap: f32,
    scale: f32,
) {
    let lookahead_frames =
        (fps.max(1.0) * KARAOKE_TEXTURE_PREWARM_LOOKAHEAD_SECONDS).round() as i64;
    let start = index.timeline_cursor_at(current_frame - lookahead_frames / 10);
    let end_frame = current_frame + lookahead_frames;
    let mut pushed = 0;

    for &(start_frame, line_id) in index
        .karaoke_timeline
        .iter()
        .skip(start)
        .take(KARAOKE_TEXTURE_PREWARM_CANDIDATES_PER_FRAME)
    {
        if start_frame > end_frame {
            break;
        }
        let Some(line) = project.get_line(line_id) else {
            continue;
        };
        if line.text.is_empty() || line.text == "↑" || line.text == "↓" {
            continue;
        }
        let Some(track) = rythmo_layout::track_for_y_slot(track_layouts, line.y_slot) else {
            continue;
        };

        let body_y = zone.y + ruler_h + track.top + slot_header_h + badge_gap;
        let row_rect = karaoke_stack_rect(
            Rect {
                x: zone.x,
                y: body_y,
                width: state.karaoke_ui_text_width_for_render(line),
                height: track.body_h,
            },
            index.stack_row(line),
            scale,
        );
        stretched.push(StretchedText::natural_prewarm(
            line.id,
            line.text.clone(),
            row_rect,
            constants::KARAOKE_TEXT_FONT_SCALE,
        ));
        pushed += 1;
        if pushed >= KARAOKE_TEXTURE_PREWARM_PUSHES_PER_FRAME {
            break;
        }
    }
}

fn syllable_ratios_for_line(
    line: &crate::rythmo_line::RythmoLine,
    drag: Option<&SyllableDrag>,
    lang: &str,
) -> Option<Vec<f32>> {
    let breaks = crate::syllable::syllable_breaks(&line.text, lang);
    if breaks.is_empty() {
        return None;
    }

    if let Some(drag) = drag.filter(|drag| drag.line_id == line.id) {
        return Some(drag.ratios.clone());
    }

    if line.syllable_ratios.len() == breaks.len() + 1 {
        Some(line.syllable_ratios.clone())
    } else {
        Some(crate::syllable::default_ratios_from_breaks(
            &line.text, &breaks,
        ))
    }
}

fn render_syllable_handles(
    rect: &Rect,
    ratios: &[f32],
    active: bool,
    quads: &mut Vec<QuadInstance>,
) {
    if ratios.len() <= 1 || rect.width <= 2.0 {
        return;
    }

    let alpha = if active { 1.0 } else { 0.78 };
    let color = [0.95, 0.08, 0.03, alpha];
    let stroke = if active { 3.0 } else { 2.5 };
    let tick_h = if active { 9.0 } else { 7.0 };
    let top_y = rect.y + 1.0;
    let cap_gap = 2.0;

    let mut x = rect.x;
    let mut boundaries = vec![rect.x];
    for ratio in ratios.iter().take(ratios.len() - 1) {
        x += ratio * rect.width;
        boundaries.push(x.clamp(rect.x, rect.x + rect.width));
    }
    boundaries.push(rect.x + rect.width);

    for pair in boundaries.windows(2) {
        let start = pair[0] + cap_gap;
        let end = pair[1] - cap_gap;
        if end > start {
            quads.push(QuadInstance {
                rect: [start, top_y, end - start, stroke],
                color,
                color_bottom: color,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: stroke / 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0, 0.0, 0.0, 0.22],
                shadow_blur: 2.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }

    for boundary in boundaries {
        quads.push(QuadInstance {
            rect: [boundary - stroke / 2.0, top_y, stroke, tick_h],
            color,
            color_bottom: color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: stroke / 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0, 0.0, 0.0, 0.22],
            shadow_blur: 2.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
}

pub fn render_lines<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    syllable_quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    note_icons: &mut Vec<IconInstance>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
    note_uv: [f32; 4],
) -> Option<(
    u64,
    usize,
    Option<(usize, usize)>,
    f32,
    f32,
    f32,
    f32,
    Option<Vec<CursorSegmentInfo>>,
)> {
    state.prune_karaoke_text_width_cache(project);
    let karaoke_max_gap_frames = karaoke_adjacent_max_gap_frames(fps);
    let karaoke_index = state.cached_karaoke_ui_index(project, karaoke_max_gap_frames);
    let current_frame_i64 = visual_frame_to_i64(current_frame);
    state.prewarm_karaoke_text_widths(
        project,
        &karaoke_index,
        current_frame_i64,
        (fps.max(1.0) * 10.0).round() as i64,
        if karaoke_preview { 2 } else { 8 },
    );
    let karaoke_count_in_frame_count = karaoke_count_in_frames(fps);
    let layout_ctx = state.get_or_create_layout_ctx(project, karaoke_index.karaoke_tracks(), zone);

    // Rend le highlight de la track survolée (s'il y en a une et qu'elle est valide)
    if let Some(track_idx) = state.hovered_track {
        if let Some(track) = layout_ctx.track_for_index(track_idx) {
            let y_base = zone.y + constants::RULER_HEIGHT + track.top;
            quads.push(QuadInstance {
                rect: [zone.x, y_base, zone.width, track.total_h],
                color: [1.0, 1.0, 1.0, 0.03],
                color_bottom: [1.0, 1.0, 1.0, 0.03],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
    }

    let mut cursor_info = None;
    let karaoke_lang = crate::config::get().lang.clone();
    let margin_frames = interactive_render_margin_frames(fps, render_index);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames);
    let mut visible_line_ids = render_index.visible_line_ids(project, first_frame, last_frame);
    visible_line_ids.sort_by_key(|id| project.line_index(*id).unwrap_or(usize::MAX));

    // Precompute line data ONCE - rect, karaoke flags, badge rect, character name
    #[derive(Clone, Copy)]
    struct LineRenderData {
        rect: Rect,
        badge_rect: Rect,
        karaoke_count_in: bool,
        karaoke_progress_info: Option<KaraokeProgressRenderInfo>,
    }

    let mut line_data: Vec<(u64, LineRenderData)> = Vec::with_capacity(visible_line_ids.len());
    for &lid in &visible_line_ids {
        let Some(line) = project.get_line(lid) else {
            continue;
        };
        let karaoke_active = line.karaoke_active(current_frame);
        let karaoke_count_in = karaoke_preview
            && karaoke_count_in_visible(line, current_frame, karaoke_count_in_frame_count);
        let karaoke_prestart_count_in = karaoke_preview
            && karaoke_index.prestart_scroll_visible(line, current_frame, karaoke_count_in_frame_count);
        let karaoke_upcoming_stack = karaoke_preview && karaoke_index.upcoming_stack_visible(line, current_frame);
        
        if karaoke_preview && line.karaoke && !karaoke_active && !karaoke_count_in && !karaoke_prestart_count_in && !karaoke_upcoming_stack {
            continue;
        }

        let centered_karaoke_width = if karaoke_preview && line.karaoke && (karaoke_active || karaoke_prestart_count_in || karaoke_upcoming_stack) {
            Some(state.karaoke_ui_text_width_for_render(line))
        } else {
            None
        };
        let r = if karaoke_preview && line.karaoke {
            karaoke_preview_line_rect_with_state(
                &layout_ctx,
                line,
                current_frame,
                zone,
                karaoke_prestart_count_in,
                karaoke_upcoming_stack,
                karaoke_index.stack_row(line),
                centered_karaoke_width,
            )
        } else {
            layout_ctx.line_rect_with_karaoke_width(line, current_frame, zone, karaoke_preview, None)
        };

        if r.x + r.width < zone.x || r.x > zone.x + zone.width {
            continue;
        }

        let karaoke_progress_info = if karaoke_preview && line.karaoke {
            karaoke_progress_render_info(line, current_frame, &karaoke_lang)
        } else {
            None
        };

        let badge_rect = if karaoke_preview && line.karaoke {
            badge_rect_for_karaoke_rect(line, &r)
        } else {
            layout_ctx.badge_rect_for_name(line, &line.character_name, r.x, zone)
        };

        line_data.push((
            lid,
            LineRenderData {
                rect: r,
                badge_rect,
                karaoke_count_in,
                karaoke_progress_info,
            },
        ));
    }

    // Optimize badge overlap: sort by y, then sweep line O(n log n) instead of O(n²)
    line_data.sort_by(|a, b| a.1.badge_rect.y.partial_cmp(&b.1.badge_rect.y).unwrap_or(std::cmp::Ordering::Equal));
    let mut badge_hidden: HashMap<u64, bool> = HashMap::new();
    let mut badge_overlap_alpha: HashMap<u64, f32> = HashMap::new();
    
    for i in 0..line_data.len() {
        let (id_i, data_i) = &line_data[i];
        let mut hidden = false;
        let mut alpha = 1.0;
        
        // Only check nearby badges (spatially close in Y)
        for j in (i + 1)..line_data.len() {
            let (id_j, data_j) = &line_data[j];
            if data_j.badge_rect.y > data_i.badge_rect.y + data_i.badge_rect.height + 2.0 {
                break; // Too far down, no more overlaps possible
            }
            if rects_overlap(&data_i.badge_rect, &data_j.badge_rect) {
                // Need to compare actual character names
                let name_i = project.get_line(*id_i).map(|l| &l.character_name);
                let name_j = project.get_line(*id_j).map(|l| &l.character_name);
                if let (Some(ni), Some(nj)) = (name_i, name_j) {
                    if ni == nj {
                        hidden = true;
                        break;
                    } else {
                        alpha = 0.5;
                    }
                }
            }
        }
        badge_hidden.insert(*id_i, hidden);
        badge_overlap_alpha.insert(*id_i, alpha);
    }

// Now render all lines using precomputed data
    for (line_id, data) in line_data {
        let Some(line) = project.get_line(line_id) else {
            continue;
        };

        let is_hovered = state.hovered_line == Some(line.id);
        let is_selected = matches!(state.selected, Some(Selection::Line(id)) if id == line.id)
            || matches!(state.selected, Some(Selection::AllLines));
        let is_editing = state.editing_line == Some(line.id);
        let karaoke_playback_line = karaoke_preview && line.karaoke;

        if !karaoke_playback_line {
            // Subtle dark background + border
            let bg = if is_editing {
                [0.12, 0.12, 0.15, 0.6]
            } else if is_hovered {
                [0.10, 0.10, 0.13, 0.4]
            } else {
                [0.08, 0.08, 0.10, 0.3]
            };
            let border = if is_selected {
                [0.90, 0.78, 0.30, 0.75]
            } else if line.karaoke {
                [0.35, 0.72, 1.0, 0.75]
            } else if is_hovered || is_editing {
                LINE_BORDER_HOVER
            } else {
                LINE_BORDER
            };
            quads.push(QuadInstance {
                rect: [data.rect.x, data.rect.y, data.rect.width, data.rect.height],
                color: bg,
                color_bottom: bg,
                border_color: border,
                border_width: if is_selected { 1.5 } else { 1.0 },
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Stretched text or special rendering for breath arrows
        let mut cursor_segments = None;
        if !line.text.is_empty() {
            if line.text == "↑" || line.text == "↓" {
                render_breath_arrow(&data.rect, line.text == "↑", quads);
            } else if line.karaoke && karaoke_preview {
                push_karaoke_rythmo_text(stretched, line, data.rect, data.karaoke_progress_info);
            } else {
                let drag_ratios = state
                    .syllable_drag
                    .as_ref()
                    .filter(|d| d.line_id == line.id);
                if let Some((breaks, ratios)) =
                    visible_syllable_segments(line, drag_ratios, &karaoke_lang, karaoke_preview, state)
                {
                    let chars: Vec<char> = line.text.chars().collect();
                    let mut seg_x = data.rect.x;
                    let mut prev_break = 0usize;
                    let mut editing_segments = if is_editing { Some(Vec::new()) } else { None };
                    for (i, &ratio) in ratios.iter().enumerate() {
                        let seg_w = ratio * data.rect.width;
                        let end_break = if i < breaks.len() {
                            breaks[i]
                        } else {
                            chars.len()
                        };
                        let segment: String = chars[prev_break..end_break].iter().collect();
                        if !segment.is_empty() && seg_w > 1.0 {
                            let cache_id = syllable_segment_cache_id(line.id, i);
                            if let Some(segments) = &mut editing_segments {
                                segments.push(CursorSegmentInfo {
                                    cache_id,
                                    start_char: prev_break,
                                    end_char: end_break,
                                    start_ratio: ((seg_x - data.rect.x) / data.rect.width).clamp(0.0, 1.0),
                                    width_ratio: (seg_w / data.rect.width).clamp(0.0, 1.0),
                                });
                            }
                            push_plain_rythmo_text(
                                stretched,
                                cache_id,
                                segment,
                                Rect {
                                    x: seg_x,
                                    y: data.rect.y,
                                    width: seg_w,
                                    height: data.rect.height,
                                },
                            );
                        }
                        seg_x += seg_w;
                        prev_break = end_break;
                    }
                    cursor_segments = editing_segments.filter(|segments| !segments.is_empty());
                } else {
                    push_plain_rythmo_text(
                        stretched,
                        line.id,
                        line.text.clone(),
                        Rect {
                            x: data.rect.x,
                            y: data.rect.y,
                            width: data.rect.width,
                            height: data.rect.height,
                        },
                    );
                }
            }
        }

        // Cursor info for mod.rs to resolve with renderer
        if is_editing {
            if state.line_input.cursor_visible() || state.line_input.has_selection() {
                cursor_info = Some((
                    line.id,
                    state.line_input.cursor_pos,
                    state.line_input.selection_range(),
                    data.rect.x,
                    data.rect.width,
                    data.rect.y,
                    data.rect.height,
                    cursor_segments.clone(),
                ));
            }
        }

        if data.karaoke_count_in {
            render_karaoke_count_in_dot_scaled(
                line,
                current_frame,
                &data.rect,
                karaoke_count_in_frame_count,
                1.0,
                quads,
            );
        } else if karaoke_preview && line.karaoke {
            render_karaoke_dot(line, &data.rect, data.karaoke_progress_info, quads);
        }

        let is_syllable_drag_line =
            state.syllable_drag.as_ref().map(|d| d.line_id) == Some(line.id);
        if !(karaoke_preview && line.karaoke) && (is_hovered || is_syllable_drag_line) {
            if let Some(ratios) =
                syllable_ratios_for_line(line, state.syllable_drag.as_ref(), &karaoke_lang)
            {
                render_syllable_handles(&data.rect, &ratios, true, syllable_quads);
            }
        }

        // Handles (only on hover/editing)
        if (is_hovered || is_editing) && !karaoke_playback_line {
            quads.push(QuadInstance {
                rect: [data.rect.x, data.rect.y, constants::HANDLE_WIDTH, data.rect.height],
                color: HANDLE_COLOR,
                color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
            quads.push(QuadInstance {
                rect: [
                    data.rect.x + data.rect.width - constants::HANDLE_WIDTH,
                    data.rect.y,
                    constants::HANDLE_WIDTH,
                    data.rect.height,
                ],
                color: HANDLE_COLOR,
                color_bottom: HANDLE_COLOR,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: LINE_RADIUS,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Character badge — use precomputed badge_rect
        let br = data.badge_rect;

        // Overlap detection vs OTHER lines: use precomputed HashMaps
        let badge_hidden = *badge_hidden.get(&line_id).unwrap_or(&false);
        let badge_overlap_alpha = *badge_overlap_alpha.get(&line_id).unwrap_or(&1.0);

        if karaoke_playback_line {
            if karaoke_index.character_label_visible(line) {
                labels.push(LabelInfo {
                    text: &line.character_name,
                    bounds: br,
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Visible,
                    padding: 0.0,
                    font_size_override: Some(BADGE_FONT_SIZE),
                    color_override: Some(line_color_label(line)),
                    font_family_override: None,
                });
            }
            continue;
        }

if !badge_hidden {
        let mut badge_color = line.character_color;
        badge_color[3] *= badge_overlap_alpha;
        let is_editing_char = state.editing_character == Some(line.id);
        let badge_border = if is_editing_char {
            [0.8, 0.8, 0.85, 0.8]
        } else {
            [0.0_f32; 4]
        };
        quads.push(QuadInstance {
            rect: [br.x, br.y, br.width, br.height],
            color: badge_color,
            color_bottom: badge_color,
            border_color: badge_border,
            border_width: if is_editing_char { 1.0 } else { 0.0 },
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Character name text — black on bright backgrounds for contrast
        if !line.character_name.is_empty() {
            let [r, g, b, _] = line.character_color;
            let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
            let text_color = if luminance > 0.55 {
                Some([0, 0, 0])
            } else {
                None
            };

            labels.push(LabelInfo {
                text: &line.character_name,
                bounds: br,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: BADGE_PADDING_H,
                font_size_override: Some(BADGE_FONT_SIZE),
                color_override: text_color,
                font_family_override: None,
            });
        }

        render_voice_actor_icons_for_line(
            line,
            project,
            zone,
            br,
            ACTOR_ICON_SIZE,
            quads,
            labels,
            actor_icons,
        );

        text_input::render_selection_and_cursor(
            quads,
            br,
            &line.character_name,
            &state.char_input,
            is_editing_char,
            badge_text_metrics(),
            3.0,
            3.0,
            [0.25, 0.45, 0.95, 0.45],
            CURSOR_COLOR,
        );

        // Note indicator: small icon at the end of the badge if line has a note
        if !line.note.is_empty() {
            let icon_size = 10.0;
            note_icons.push(IconInstance {
                rect: [
                    br.x + br.width - icon_size - 2.0,
                    br.y + (br.height - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                ],
                uv_rect: note_uv,
                tint: [0.7, 0.7, 0.75, 0.9],
            });
        }
        }

        // Note text: small italic label at the bottom of the line
        let note_label_h = 12.0;
        let note_y = data.rect.y + data.rect.height - note_label_h - 1.0;
        let note_rect = Rect {
            x: data.rect.x + 4.0,
            y: note_y,
            width: data.rect.width - 8.0,
            height: note_label_h,
        };
        if !line.note.is_empty() {
            labels.push(LabelInfo {
                text: &line.note,
                bounds: note_rect,
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(9.0),
                color_override: Some([160, 160, 170]),
                font_family_override: None,
            });
        }

        let is_editing_note = state.editing_note == Some(line.id);
        text_input::render_selection_and_cursor(
            quads,
            note_rect,
            &line.note,
            &state.note_input,
            is_editing_note,
            note_text_metrics(),
            1.0,
            1.0,
            [0.25, 0.45, 0.95, 0.45],
            CURSOR_COLOR,
        );
    }

    push_editor_karaoke_texture_prewarm_texts(
        stretched,
        state,
        project,
        &karaoke_index,
        &layout_ctx,
        current_frame_i64,
        fps,
        zone,
    );

        // Ghost preview line when holding click on empty space
        if let Some(ghost) = &state.ghost_preview {
            let body_rect = layout_ctx.track_body_rect(ghost.y_slot, zone);
        let ghost_rect_x = frame_to_x(ghost.frame, current_frame, zone);
        let ghost_w = (ghost.duration_frames as f32 * ppf()).max(2.0);

        let ghost_bg = [0.25, 0.25, 0.35, 0.2];
        let ghost_border = [0.5, 0.5, 0.6, 0.3];
        quads.push(QuadInstance {
            rect: [ghost_rect_x, body_rect.y, ghost_w, layout_ctx.normal_body_h],
            color: ghost_bg,
            color_bottom: ghost_bg,
            border_color: ghost_border,
            border_width: 1.0,
            border_radius: LINE_RADIUS,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        // Ghost badge — rectangular, top-aligned, right edge a few px left of line
        let ghost_badge_w = BADGE_MIN_W;
        let ghost_badge_h = body_rect.height * BADGE_OVERLAP_HEIGHT_RATIO;
        let ghost_badge_x = ghost_rect_x - BADGE_GAP - ghost_badge_w;
        quads.push(QuadInstance {
            rect: [
                ghost_badge_x,
                body_rect.y,
                ghost_badge_w,
                ghost_badge_h,
            ],
            color: [0.4, 0.4, 0.5, 0.2],
            color_bottom: [0.4, 0.4, 0.5, 0.2],
            border_color: ghost_border,
            border_width: 1.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }

    cursor_info
}

fn render_voice_actor_icons_for_line<'a>(
    line: &'a crate::rythmo_line::RythmoLine,
    project: &'a Project,
    zone: &Rect,
    badge: Rect,
    icon_size: f32,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
) {
    if line.voice_actor_names.is_empty() {
        return;
    }

    let size = icon_size;
    let gap = ACTOR_ICON_GAP;
    let mut x = badge.x + badge.width + gap;
    let y = badge.y + (badge.height - size) * 0.5;
    for actor_name in &line.voice_actor_names {
        if x > zone.x + zone.width {
            break;
        }
        let rect = Rect {
            x,
            y,
            width: size,
            height: size,
        };
        quads.push(QuadInstance {
            rect: [rect.x, rect.y, rect.width, rect.height],
            color: [0.05, 0.05, 0.07, 0.92],
            color_bottom: [0.02, 0.02, 0.03, 0.92],
            border_color: [0.75, 0.75, 0.85, 0.45],
            border_width: 1.0,
            border_radius: 3.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });

        if let Some(actor) = project.find_voice_actor(actor_name) {
            if actor.icon_png_base64.is_some() {
                actor_icons.push(VoiceActorIconDraw {
                    actor_name: actor.name.clone(),
                    rect,
                });
            } else {
                labels.push(LabelInfo {
                    text: &actor.name,
                    bounds: rect,
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 1.0,
                    font_size_override: Some((size * 0.55).max(8.0)),
                    color_override: Some([230, 230, 238]),
                    font_family_override: None,
                });
            }
        } else {
            labels.push(LabelInfo {
                text: actor_name,
                bounds: rect,
                h_align: HAlign::Center,
                v_align: VAlign::Center,
                overflow: Overflow::Clip,
                padding: 1.0,
                font_size_override: Some((size * 0.55).max(8.0)),
                color_override: Some([230, 230, 238]),
                font_family_override: None,
            });
        }
        x += size + gap;
    }
}

/// Render a diagonal arrow for breath markers using rotated quads.
/// `up` = bottom-left → top-right (inspiration), `!up` = top-left → bottom-right (expiration).
fn render_breath_arrow(r: &Rect, up: bool, quads: &mut Vec<QuadInstance>) {
    let margin = 4.0;
    let cx = r.x + r.width / 2.0;
    let cy = r.y + r.height / 2.0;
    let dx = r.width - margin * 2.0;
    let dy = r.height - margin * 2.0;
    let length = (dx * dx + dy * dy).sqrt();
    let angle = if up {
        -(dy).atan2(dx) // bottom-left to top-right
    } else {
        (dy).atan2(dx) // top-left to bottom-right
    };
    let thickness = 2.0;
    let color = [0.85, 0.85, 0.90, 0.9];

    // Main diagonal line — a thin rectangle rotated
    quads.push(QuadInstance {
        rect: [cx - length / 2.0, cy - thickness / 2.0, length, thickness],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: angle,
        _padding: [0.0; 2],
    });

    // Arrowhead at the end (top-right for up, bottom-right for down)
    let tip_x = r.x + r.width - margin;
    let tip_y = if up {
        r.y + margin
    } else {
        r.y + r.height - margin
    };
    let arrow_len = 8.0;
    let arrow_thickness = 2.0;
    let spread = 0.5; // ~30 degrees from the main line

    // Two short lines forming the arrowhead
    let base_angle = if up {
        std::f32::consts::PI + angle
    } else {
        std::f32::consts::PI + angle
    };
    quads.push(QuadInstance {
        rect: [
            tip_x - arrow_len / 2.0,
            tip_y - arrow_thickness / 2.0,
            arrow_len,
            arrow_thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: base_angle + spread,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [
            tip_x - arrow_len / 2.0,
            tip_y - arrow_thickness / 2.0,
            arrow_len,
            arrow_thickness,
        ],
        color,
        color_bottom: color,
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: base_angle - spread,
        _padding: [0.0; 2],
    });
}

fn render_karaoke_dot(
    line: &crate::rythmo_line::RythmoLine,
    line_rect: &Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
    quads: &mut Vec<QuadInstance>,
) {
    render_karaoke_dot_scaled(line, line_rect, progress_info, 1.0, quads);
}

fn karaoke_count_in_dot_rect(line_rect: &Rect, count_in_progress: f32, scale: f32) -> Rect {
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let progress = count_in_progress.clamp(0.0, 1.0);
    let bounce_progress = (progress * constants::KARAOKE_COUNT_IN_BOUNCES).fract();
    let bounce = (bounce_progress * std::f32::consts::PI).sin().max(0.0);
    let travel = constants::KARAOKE_NEXT_PREVIEW_GAP * 4.0 * scale + size * 2.0;
    let start_x = line_rect.x - travel;
    let end_x = line_rect.x;
    Rect {
        x: start_x + (end_x - start_x) * progress,
        y: line_rect.y + 3.0 * scale.max(0.5)
            - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE,
        width: size,
        height: size,
    }
}

fn render_karaoke_count_in_dot_scaled(
    line: &crate::rythmo_line::RythmoLine,
    current_frame: f64,
    line_rect: &Rect,
    count_in_frames: i64,
    scale: f32,
    quads: &mut Vec<QuadInstance>,
) {
    let Some(count_in_progress) = karaoke_count_in_progress(line, current_frame, count_in_frames)
    else {
        return;
    };

    let dot = karaoke_count_in_dot_rect(line_rect, count_in_progress, scale);
    let tint = line_color_tint(line);
    quads.push(QuadInstance {
        rect: [dot.x - 1.5, dot.y - 1.5, dot.width + 3.0, dot.height + 3.0],
        color: [0.0, 0.0, 0.0, 0.35],
        color_bottom: [0.0, 0.0, 0.0, 0.35],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: (dot.width + 3.0) / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [dot.x, dot.y, dot.width, dot.height],
        color: tint,
        color_bottom: tint,
        border_color: [1.0, 1.0, 1.0, 0.85],
        border_width: 1.0,
        border_radius: dot.width / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn render_karaoke_dot_scaled(
    line: &crate::rythmo_line::RythmoLine,
    line_rect: &Rect,
    progress_info: Option<KaraokeProgressRenderInfo>,
    scale: f32,
    quads: &mut Vec<QuadInstance>,
) {
    let Some(progress_info) = progress_info else {
        return;
    };

    let bounce = (progress_info.local_progress * std::f32::consts::PI)
        .sin()
        .max(0.0);
    let size = constants::KARAOKE_DOT_SIZE * scale.max(0.5);
    let x = if line_rect.width > size {
        line_rect.x + progress_info.visual_progress.clamp(0.0, 1.0) * (line_rect.width - size)
    } else {
        line_rect.x + (line_rect.width - size) * 0.5
    };
    let y = line_rect.y + 3.0 * scale.max(0.5)
        - bounce * size * constants::KARAOKE_DOT_BOUNCE_AMPLITUDE;
    let tint = line_color_tint(line);

    quads.push(QuadInstance {
        rect: [x - 1.5, y - 1.5, size + 3.0, size + 3.0],
        color: [0.0, 0.0, 0.0, 0.35],
        color_bottom: [0.0, 0.0, 0.0, 0.35],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: (size + 3.0) / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
    quads.push(QuadInstance {
        rect: [x, y, size, size],
        color: tint,
        color_bottom: tint,
        border_color: [1.0, 1.0, 1.0, 0.85],
        border_width: 1.0,
        border_radius: size / 2.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

/// Render autocomplete dropdown AFTER all lines (so it's on top).
pub fn render_autocomplete<'a>(
    zone: &Rect,
    project: &'a Project,
    current_frame: f64,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let line_id = match state.editing_character {
        Some(id) => id,
        None => return,
    };
    let line = match project.get_line(line_id) {
        Some(l) => l,
        None => return,
    };
    if line.character_name.is_empty() {
        return;
    }

    let suggestions = project.autocomplete(&line.character_name);
    if suggestions.is_empty() {
        return;
    }

    let r = line_rect(project, line, current_frame, zone);
    let br = badge_rect_for_line(project, line, current_frame, zone);
    let dropdown_x = br.x;
    let mut dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;
    let dropdown_h = suggestions.len() as f32 * item_h;

    // Background
    quads.push(QuadInstance {
        rect: [dropdown_x, dropdown_y, dropdown_w, dropdown_h],
        color: [0.15, 0.15, 0.17, 0.95],
        color_bottom: [0.12, 0.12, 0.14, 0.95],
        border_color: [0.3, 0.3, 0.36, 0.6],
        border_width: 1.0,
        border_radius: 3.0,
        shadow_offset: [0.0, 2.0],
        shadow_color: [0.0, 0.0, 0.0, 0.4],
        shadow_blur: 6.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });

    for (i, suggestion) in suggestions.iter().enumerate() {
        let is_selected = state.autocomplete_index == Some(i);
        let is_hovered = state.autocomplete_hover == Some(i);

        // Highlight
        if is_selected || is_hovered {
            let alpha = if is_selected { 0.15 } else { 0.08 };
            quads.push(QuadInstance {
                rect: [
                    dropdown_x + 2.0,
                    dropdown_y + 1.0,
                    dropdown_w - 4.0,
                    item_h - 2.0,
                ],
                color: [1.0, 1.0, 1.0, alpha],
                color_bottom: [1.0, 1.0, 1.0, alpha],
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 2.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }

        // Color swatch
        quads.push(QuadInstance {
            rect: [dropdown_x + 4.0, dropdown_y + 4.0, 12.0, item_h - 8.0],
            color: suggestion.color,
            color_bottom: suggestion.color,
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 2.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
        // Name label
        labels.push(LabelInfo {
            text: &suggestion.name,
            bounds: Rect {
                x: dropdown_x + 20.0,
                y: dropdown_y,
                width: dropdown_w - 24.0,
                height: item_h,
            },
            h_align: HAlign::Left,
            v_align: VAlign::Center,
            overflow: Overflow::Ellipsis,
            padding: 2.0,
            font_size_override: Some(11.0),
            color_override: None,
            font_family_override: None,
        });
        dropdown_y += item_h;
    }
}

/// Returns the autocomplete suggestion rect for hit testing
/// Render markers (boucle, out, scene change, liaisons) on the bande rythmo.
pub fn render_markers<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    liaison_icons: &mut Vec<IconInstance>,
    liaison_left_uv: [f32; 4],
    liaison_right_uv: [f32; 4],
) {
    let margin_frames = f64_ceil_to_i64(20.0 / ppf().max(0.001) as f64).saturating_add(1);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames);
    for marker_index in render_index.visible_marker_indices(first_frame, last_frame) {
        let Some(marker) = project.markers.get(marker_index) else {
            continue;
        };
        let x = frame_to_x(marker.frame, current_frame, zone);
        if x < zone.x - 20.0 || x > zone.x + zone.width + 20.0 {
            continue;
        }

        match &marker.kind {
            MarkerKind::Boucle => {
                let red = [0.85, 0.15, 0.15, 0.9];
                // Red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Big "X" — two smooth rotated bars
                let cy = zone.y + zone.height / 2.0;
                let arm_len = 20.0;
                let thickness = 2.5;
                let pi4 = std::f32::consts::FRAC_PI_4;
                // "\" bar
                quads.push(QuadInstance {
                    rect: [x - arm_len / 2.0, cy - thickness / 2.0, arm_len, thickness],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: pi4,
                    _padding: [0.0; 2],
                });
                // "/" bar
                quads.push(QuadInstance {
                    rect: [x - arm_len / 2.0, cy - thickness / 2.0, arm_len, thickness],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: -pi4,
                    _padding: [0.0; 2],
                });
            }
            MarkerKind::Out => {
                let col = [0.85, 0.45, 0.45, 0.7];
                // Light red vertical bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: col,
                    color_bottom: col,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Two parallel oblique bars crossing the vertical bar
                let cy = zone.y + zone.height / 2.0;
                let bar_len = zone.height * 0.25;
                let thickness = 2.0;
                let angle = 0.5; // ~30 degrees
                for offset in &[-5.0_f32, 5.0] {
                    quads.push(QuadInstance {
                        rect: [
                            x + offset - bar_len / 2.0,
                            cy - thickness / 2.0,
                            bar_len,
                            thickness,
                        ],
                        color: col,
                        color_bottom: col,
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 0.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: angle,
                        _padding: [0.0; 2],
                    });
                }
                // "out" text
                labels.push(LabelInfo {
                    text: "out",
                    bounds: Rect {
                        x: x + 12.0,
                        y: cy - 8.0,
                        width: 30.0,
                        height: 16.0,
                    },
                    h_align: HAlign::Left,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(10.0),
                    color_override: Some([220, 120, 120]),
                    font_family_override: None,
                });
            }
            MarkerKind::SceneChange => {
                // White bar
                quads.push(QuadInstance {
                    rect: [x - 1.0, zone.y, 2.0, zone.height],
                    color: [0.9, 0.9, 0.95, 0.8],
                    color_bottom: [0.9, 0.9, 0.95, 0.8],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
            MarkerKind::LiaisonLeft => {
                let uv = liaison_left_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
            MarkerKind::LiaisonRight => {
                let uv = liaison_right_uv;
                liaison_icons.push(IconInstance {
                    rect: [x - 8.0, zone.y, 16.0, constants::RULER_HEIGHT],
                    uv_rect: uv,
                    tint: [0.7, 0.7, 0.75, 0.9],
                });
            }
        }
    }
}

pub fn autocomplete_hit(
    zone: &Rect,
    project: &Project,
    current_frame: f64,
    state: &RythmoState,
    click_x: f32,
    click_y: f32,
) -> Option<(String, [f32; 4])> {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = project.lines().find(|l| l.id == line_id) {
            let br = badge_rect_for_line(project, line, current_frame, zone);
            let lr = line_rect(project, line, current_frame, zone);
            let suggestions = project.autocomplete(&line.character_name);
            if !suggestions.is_empty() {
                let dropdown_x = br.x;
                let mut dropdown_y = lr.y + lr.height + 2.0;
                let item_h = 20.0;
                let dropdown_w = 140.0;

                for suggestion in &suggestions {
                    let item_rect = Rect {
                        x: dropdown_x,
                        y: dropdown_y,
                        width: dropdown_w,
                        height: item_h,
                    };
                    if item_rect.contains(click_x, click_y) {
                        return Some((suggestion.name.clone(), suggestion.color));
                    }
                    dropdown_y += item_h;
                }
            }
        }
    }
    None
}

/// Context passed to all rythmo sub-handlers.
struct RythmoCtx<'a> {
    zone: &'a Rect,
    project: &'a Project,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
}

pub fn handle_rythmo_event(
    event: &UiEvent,
    zone: &Rect,
    project: &Project,
    current_frame: f64,
    karaoke_preview: bool,
    fps: f64,
    state: &mut RythmoState,
) -> EventResponse {
    let ctx = RythmoCtx {
        zone,
        project,
        current_frame,
        karaoke_preview,
        fps,
    };

    match event {
        UiEvent::DoubleClick { x, y }
            if *x >= ctx.zone.x
                && *x <= ctx.zone.x + ctx.zone.width
                && *y >= ctx.zone.y
                && *y <= ctx.zone.y + constants::RULER_HEIGHT =>
        {
            state.audio_offset_mode = !state.audio_offset_mode;
            state.audio_offset_drag = None;
            return EventResponse::Consumed;
        }
        UiEvent::MousePress { x, y }
            if state.audio_offset_mode
                && *x >= ctx.zone.x
                && *x <= ctx.zone.x + ctx.zone.width
                && *y >= ctx.zone.y
                && *y <= ctx.zone.y + constants::RULER_HEIGHT =>
        {
            state.audio_offset_drag = Some(AudioOffsetDrag {
                last_x: *x,
                accum_px: 0.0,
            });
            return EventResponse::Consumed;
        }
        UiEvent::MousePress { .. } if state.audio_offset_mode => {
            state.audio_offset_mode = false;
            state.audio_offset_drag = None;
            return EventResponse::Consumed;
        }
        UiEvent::MouseMove { x, .. } => {
            if let Some(drag) = &mut state.audio_offset_drag {
                let dx = *x - drag.last_x;
                drag.last_x = *x;
                drag.accum_px += dx;
                let frames = (drag.accum_px / ppf()).round() as i64;
                if frames != 0 {
                    drag.accum_px -= frames as f32 * ppf();
                    return EventResponse::Action(UiAction::OffsetActiveAudioBy(frames));
                }
                return EventResponse::Consumed;
            }
        }
        UiEvent::MouseRelease { .. } => {
            if state.audio_offset_drag.is_some() {
                state.audio_offset_drag = None;
                return EventResponse::Consumed;
            }
        }
        _ => {}
    }

    // Autocomplete click has highest priority (before color picker eats it)
    if let UiEvent::MousePress { x, y } = event {
        if let Some((name, color)) =
            autocomplete_hit(ctx.zone, ctx.project, ctx.current_frame, state, *x, *y)
        {
            if let Some(line_id) = state.editing_character {
                state.stop_char_editing();
                return EventResponse::Action(UiAction::SetCharacter {
                    line_id,
                    name,
                    color,
                });
            }
        }
    }

    // Color picker overlay
    if state.color_picker.handle_event(event) {
        if let Some(line_id) = state.editing_character {
            return EventResponse::Action(UiAction::SetCharacterColor {
                line_id,
                color: state.color_picker.current_color(),
            });
        }
        return EventResponse::Consumed;
    }

    // Middle mouse pan
    if let UiEvent::MiddlePress { x, y } = event {
        if ctx.zone.contains(*x, *y) {
            state.panning = true;
            state.pan_last_x = *x;
            state.pan_accum = 0.0;
            return EventResponse::Consumed;
        }
    }
    if let UiEvent::MiddleRelease { .. } = event {
        if state.panning {
            state.panning = false;
            return EventResponse::Consumed;
        }
    }
    if let UiEvent::MouseMove { x, .. } = event {
        if state.panning {
            let dx = *x - state.pan_last_x;
            state.pan_last_x = *x;
            state.pan_accum -= dx;
            let frames = (state.pan_accum / ppf()).round() as i32;
            if frames != 0 {
                state.pan_accum -= frames as f32 * ppf();
                return EventResponse::Action(UiAction::SeekRelative(frames));
            }
            return EventResponse::Consumed;
        }
    }

    match event {
        UiEvent::MousePress { x, y } => {
            if let Some(resp) = syllable_mouse_press(&ctx, state, *x, *y) {
                return resp;
            }
        }
        UiEvent::MouseMove { x, .. } => {
            if let Some(resp) = syllable_mouse_move(state, *x) {
                return resp;
            }
        }
        UiEvent::MouseRelease { .. } => {
            if let Some(resp) = syllable_mouse_release(state) {
                return resp;
            }
        }
        _ => {}
    }

    match event {
        UiEvent::MouseMove { x, y } => handle_mouse_move(&ctx, state, *x, *y),
        UiEvent::MousePress { x, y } => handle_mouse_press(&ctx, state, *x, *y),
        UiEvent::MouseRelease { .. } => handle_mouse_release(state),
        UiEvent::CtrlClick { x, y } => handle_ctrl_click(&ctx, state, *x, *y),
        UiEvent::ShiftMousePress { x, y } => handle_shift_mouse_press(&ctx, state, *x, *y),
        UiEvent::DoubleClick { x, y } => handle_double_click(&ctx, state, *x, *y),
        UiEvent::KeyInput { text } => handle_key_input(&ctx, state, text),
        UiEvent::CursorLeft => handle_cursor_move(&ctx, state, -1, false),
        UiEvent::CursorRight => handle_cursor_move(&ctx, state, 1, false),
        UiEvent::ShiftCursorLeft => handle_cursor_move(&ctx, state, -1, true),
        UiEvent::ShiftCursorRight => handle_cursor_move(&ctx, state, 1, true),
        UiEvent::CursorUp => handle_autocomplete_nav(&ctx, state, -1),
        UiEvent::CursorDown => handle_autocomplete_nav(&ctx, state, 1),
        UiEvent::SelectAll => handle_select_all(&ctx, state),
        UiEvent::Copy => handle_copy(&ctx, state),
        UiEvent::Cut => handle_cut(&ctx, state),
        UiEvent::UndoTextEdit => handle_text_undo(&ctx, state),
        UiEvent::Delete => {
            if state.selected.is_some() {
                EventResponse::Action(UiAction::DeleteSelected)
            } else {
                EventResponse::Ignored
            }
        }
        _ => EventResponse::Ignored,
    }
}

const MENU_ITEM_H: f32 = 26.0;
const MENU_ROOT_W: f32 = 230.0;
const MENU_ACTOR_W: f32 = 240.0;
const MENU_ACTION_W: f32 = 285.0;
const MENU_GAP: f32 = 0.0;
const MENU_MARGIN: f32 = 8.0;
const MENU_MAX_ACTOR_H: f32 = 260.0;

pub fn handle_context_menu_event(
    event: &UiEvent,
    project: &Project,
    current_frame: f64,
    zone: &Rect,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
) -> EventResponse {
    match event {
        UiEvent::ContextMenu { x, y } => {
            let line_id = project
                .lines()
                .find(|line| {
                    line_rect(project, line, current_frame, zone).contains(*x, *y)
                        || badge_rect_for_line(project, line, current_frame, zone).contains(*x, *y)
                })
                .map(|line| line.id);
            if let Some(line_id) = line_id {
                state.context_menu = Some(LineContextMenu {
                    line_id,
                    x: *x,
                    y: *y,
                    hover_main: true,
                    hover_actor_index: None,
                    hover_action_index: None,
                    actor_scroll: 0.0,
                });
                state.selected = Some(Selection::Line(line_id));
                state.dragging = None;
                return EventResponse::Consumed;
            }
            state.context_menu = None;
            EventResponse::Ignored
        }
        UiEvent::MouseMove { x, y } => {
            if state.context_menu.is_none() {
                return EventResponse::Ignored;
            }
            update_context_menu_hover(project, screen_w, screen_h, state, *x, *y);
            EventResponse::Consumed
        }
        UiEvent::Scroll { x, y, delta, .. } => {
            let Some(menu) = state.context_menu.as_mut() else {
                return EventResponse::Ignored;
            };
            let (_, actor_rect, _, _, max_scroll) =
                context_menu_layout(project, screen_w, screen_h, menu);
            if actor_rect.contains(*x, *y) {
                menu.actor_scroll =
                    (menu.actor_scroll - delta * MENU_ITEM_H * 2.0).clamp(0.0, max_scroll);
                return EventResponse::Consumed;
            }
            EventResponse::Consumed
        }
        UiEvent::MousePress { x, y } | UiEvent::DoubleClick { x, y } => {
            if state.context_menu.is_none() {
                return EventResponse::Ignored;
            }
            update_context_menu_hover(project, screen_w, screen_h, state, *x, *y);
            let Some(menu) = state.context_menu.as_ref() else {
                return EventResponse::Ignored;
            };
            let (root_rect, actor_rect, action_rect, actor_scroll, _) =
                context_menu_layout(project, screen_w, screen_h, menu);

            if let (Some(actor_index), Some(action_index)) =
                (menu.hover_actor_index, menu.hover_action_index)
            {
                if action_rect.contains(*x, *y) {
                    if let Some(actor) = project.voice_actors.get(actor_index) {
                        let line_id = menu.line_id;
                        let actor_name = actor.name.clone();
                        state.context_menu = None;
                        return match action_index {
                            0 => EventResponse::Action(UiAction::AssignVoiceActorLine {
                                line_id,
                                actor_name,
                            }),
                            1 => EventResponse::Action(UiAction::AssignVoiceActorCharacter {
                                line_id,
                                actor_name,
                            }),
                            2 => EventResponse::Action(UiAction::UnassignVoiceActorLine {
                                line_id,
                                actor_name,
                            }),
                            3 => EventResponse::Action(UiAction::UnassignVoiceActorCharacter {
                                line_id,
                                actor_name,
                            }),
                            _ => EventResponse::Consumed,
                        };
                    }
                }
            }

            if actor_rect.contains(*x, *y) {
                let item_index =
                    ((*y - actor_rect.y + actor_scroll) / MENU_ITEM_H).floor() as usize;
                if item_index == project.voice_actors.len() {
                    state.context_menu = None;
                    return EventResponse::Action(UiAction::OpenVoiceActorModal);
                }
                return EventResponse::Consumed;
            }

            if root_rect.contains(*x, *y)
                || action_rect.contains(*x, *y)
                || context_menu_bridge_contains(root_rect, actor_rect, action_rect, *x, *y)
            {
                return EventResponse::Consumed;
            }

            state.context_menu = None;
            EventResponse::Consumed
        }
        UiEvent::KeyInput { text } if text == "\x1b" => {
            state.context_menu = None;
            EventResponse::Consumed
        }
        _ => {
            if state.context_menu.is_some() {
                EventResponse::Consumed
            } else {
                EventResponse::Ignored
            }
        }
    }
}

pub fn render_context_menu<'a>(
    project: &'a Project,
    screen_w: f32,
    screen_h: f32,
    state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
) {
    let Some(menu) = &state.context_menu else {
        return;
    };
    let (root_rect, actor_rect, action_rect, actor_scroll, max_scroll) =
        context_menu_layout(project, screen_w, screen_h, menu);

    render_menu_panel(quads, root_rect);
    render_menu_item(
        quads,
        labels,
        root_rect,
        t("context.voice_actor.assign_to_actor"),
        menu.hover_main,
        true,
    );

    if !context_actor_menu_visible(menu) {
        return;
    }

    render_menu_panel(quads, actor_rect);
    let assigned_names = project
        .get_line(menu.line_id)
        .map(|line| line.voice_actor_names.as_slice())
        .unwrap_or(&[]);
    for (index, actor) in project.voice_actors.iter().enumerate() {
        let y = actor_rect.y + index as f32 * MENU_ITEM_H - actor_scroll;
        if y + MENU_ITEM_H < actor_rect.y || y > actor_rect.y + actor_rect.height {
            continue;
        }
        let item_rect = Rect {
            x: actor_rect.x,
            y,
            width: actor_rect.width,
            height: MENU_ITEM_H,
        };
        let assigned = assigned_names.iter().any(|name| name == &actor.name);
        render_menu_item(
            quads,
            labels,
            item_rect,
            &actor.name,
            menu.hover_actor_index == Some(index) || assigned,
            true,
        );
    }

    let create_index = project.voice_actors.len();
    let create_y = actor_rect.y + create_index as f32 * MENU_ITEM_H - actor_scroll;
    if create_y + MENU_ITEM_H >= actor_rect.y && create_y <= actor_rect.y + actor_rect.height {
        render_menu_separator(quads, actor_rect.x, create_y, actor_rect.width);
        render_menu_item(
            quads,
            labels,
            Rect {
                x: actor_rect.x,
                y: create_y,
                width: actor_rect.width,
                height: MENU_ITEM_H,
            },
            t("context.voice_actor.create"),
            menu.hover_actor_index == Some(create_index),
            false,
        );
    }

    if max_scroll > 0.0 {
        render_menu_scrollbar(quads, actor_rect, actor_scroll, max_scroll);
    }

    if let Some(actor_index) = menu.hover_actor_index {
        if actor_index < project.voice_actors.len() {
            render_menu_panel(quads, action_rect);
            let actions = [
                t("context.voice_actor.assign_line"),
                t("context.voice_actor.assign_character"),
                t("context.voice_actor.unassign_line"),
                t("context.voice_actor.unassign_character"),
            ];
            for (index, label) in actions.iter().enumerate() {
                render_menu_item(
                    quads,
                    labels,
                    Rect {
                        x: action_rect.x,
                        y: action_rect.y + index as f32 * MENU_ITEM_H,
                        width: action_rect.width,
                        height: MENU_ITEM_H,
                    },
                    label,
                    menu.hover_action_index == Some(index),
                    false,
                );
            }
        }
    }
}

fn context_menu_layout(
    project: &Project,
    screen_w: f32,
    screen_h: f32,
    menu: &LineContextMenu,
) -> (Rect, Rect, Rect, f32, f32) {
    let root_h = MENU_ITEM_H;
    let (root_x, root_y) =
        clamped_menu_origin(menu.x, menu.y, MENU_ROOT_W, root_h, screen_w, screen_h);
    let root_rect = Rect {
        x: root_x,
        y: root_y,
        width: MENU_ROOT_W,
        height: root_h,
    };

    let actor_items = project.voice_actors.len() + 1;
    let total_actor_h = actor_items as f32 * MENU_ITEM_H;
    let actor_h = total_actor_h
        .min(MENU_MAX_ACTOR_H)
        .min((screen_h - MENU_MARGIN * 2.0).max(MENU_ITEM_H));
    let actor_x_right = root_rect.x + root_rect.width + MENU_GAP;
    let actor_x = if actor_x_right + MENU_ACTOR_W <= screen_w - MENU_MARGIN {
        actor_x_right
    } else {
        (root_rect.x - MENU_ACTOR_W - MENU_GAP).max(MENU_MARGIN)
    };
    let actor_y = root_rect.y.clamp(
        MENU_MARGIN,
        (screen_h - actor_h - MENU_MARGIN).max(MENU_MARGIN),
    );
    let actor_rect = Rect {
        x: actor_x,
        y: actor_y,
        width: MENU_ACTOR_W,
        height: actor_h,
    };
    let max_scroll = (total_actor_h - actor_h).max(0.0);
    let actor_scroll = menu.actor_scroll.clamp(0.0, max_scroll);

    let hovered_actor_y = menu
        .hover_actor_index
        .map(|index| actor_rect.y + index as f32 * MENU_ITEM_H - actor_scroll)
        .unwrap_or(actor_rect.y)
        .clamp(
            MENU_MARGIN,
            (screen_h - MENU_ITEM_H * 4.0 - MENU_MARGIN).max(MENU_MARGIN),
        );
    let action_x_right = actor_rect.x + actor_rect.width + MENU_GAP;
    let action_x = if action_x_right + MENU_ACTION_W <= screen_w - MENU_MARGIN {
        action_x_right
    } else {
        (actor_rect.x - MENU_ACTION_W - MENU_GAP).max(MENU_MARGIN)
    };
    let action_rect = Rect {
        x: action_x,
        y: hovered_actor_y,
        width: MENU_ACTION_W,
        height: MENU_ITEM_H * 4.0,
    };

    (root_rect, actor_rect, action_rect, actor_scroll, max_scroll)
}

fn bridge_rect(a: Rect, b: Rect) -> Rect {
    let a_right = a.x + a.width;
    let b_right = b.x + b.width;
    let (x, width) = if a_right <= b.x {
        (a_right, b.x - a_right)
    } else if b_right <= a.x {
        (b_right, a.x - b_right)
    } else {
        (a.x.max(b.x), 0.0)
    };
    let y = a.y.min(b.y);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect {
        x,
        y,
        width,
        height: bottom - y,
    }
}

fn context_menu_bridge_contains(
    root_rect: Rect,
    actor_rect: Rect,
    action_rect: Rect,
    x: f32,
    y: f32,
) -> bool {
    bridge_rect(root_rect, actor_rect).contains(x, y)
        || bridge_rect(actor_rect, action_rect).contains(x, y)
}

fn update_context_menu_hover(
    project: &Project,
    screen_w: f32,
    screen_h: f32,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) {
    let Some(menu) = state.context_menu.as_mut() else {
        return;
    };
    let (root_rect, actor_rect, action_rect, actor_scroll, _) =
        context_menu_layout(project, screen_w, screen_h, menu);

    let root_hover = root_rect.contains(x, y);
    let mut actor_hover = None;
    let mut action_hover = None;
    let root_actor_bridge = bridge_rect(root_rect, actor_rect).contains(x, y);
    let actor_action_bridge =
        menu.hover_actor_index.is_some() && bridge_rect(actor_rect, action_rect).contains(x, y);

    if actor_rect.contains(x, y) {
        let index = ((y - actor_rect.y + actor_scroll) / MENU_ITEM_H).floor() as usize;
        if index <= project.voice_actors.len() {
            actor_hover = Some(index);
        }
    }

    if action_rect.contains(x, y) {
        let index = ((y - action_rect.y) / MENU_ITEM_H).floor() as usize;
        if index < 4 {
            action_hover = Some(index);
            actor_hover = menu.hover_actor_index;
        }
    }

    if actor_action_bridge {
        actor_hover = menu.hover_actor_index;
    }

    menu.hover_main = root_hover || root_actor_bridge;
    menu.hover_actor_index = actor_hover;
    menu.hover_action_index = action_hover;
}

fn context_actor_menu_visible(menu: &LineContextMenu) -> bool {
    menu.hover_main || menu.hover_actor_index.is_some() || menu.hover_action_index.is_some()
}

fn clamped_menu_origin(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    screen_w: f32,
    screen_h: f32,
) -> (f32, f32) {
    (
        x.clamp(
            MENU_MARGIN,
            (screen_w - width - MENU_MARGIN).max(MENU_MARGIN),
        ),
        y.clamp(
            MENU_MARGIN,
            (screen_h - height - MENU_MARGIN).max(MENU_MARGIN),
        ),
    )
}

fn render_menu_panel(quads: &mut Vec<QuadInstance>, rect: Rect) {
    quads.push(QuadInstance {
        rect: [rect.x, rect.y, rect.width, rect.height],
        color: [0.16, 0.16, 0.19, 0.98],
        color_bottom: [0.11, 0.11, 0.14, 0.98],
        border_color: [0.42, 0.42, 0.50, 0.85],
        border_width: 1.0,
        border_radius: 0.0,
        shadow_offset: [0.0, 4.0],
        shadow_color: [0.0, 0.0, 0.0, 0.45],
        shadow_blur: 10.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn render_menu_item<'a>(
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    rect: Rect,
    text: &'a str,
    hovered: bool,
    arrow: bool,
) {
    if hovered {
        quads.push(QuadInstance {
            rect: [
                rect.x + 3.0,
                rect.y + 2.0,
                rect.width - 6.0,
                rect.height - 4.0,
            ],
            color: [0.31, 0.40, 0.72, 0.85],
            color_bottom: [0.24, 0.32, 0.62, 0.85],
            border_color: [0.0; 4],
            border_width: 0.0,
            border_radius: 0.0,
            shadow_offset: [0.0; 2],
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            rotation: 0.0,
            _padding: [0.0; 2],
        });
    }
    labels.push(LabelInfo {
        text,
        bounds: Rect {
            x: rect.x + 10.0,
            y: rect.y,
            width: rect.width - if arrow { 28.0 } else { 20.0 },
            height: rect.height,
        },
        h_align: HAlign::Left,
        v_align: VAlign::Center,
        overflow: Overflow::Ellipsis,
        padding: 0.0,
        font_size_override: Some(12.0),
        color_override: Some([230, 230, 238]),
        font_family_override: None,
    });
    if arrow {
        labels.push(LabelInfo {
            text: ">",
            bounds: Rect {
                x: rect.x + rect.width - 24.0,
                y: rect.y,
                width: 16.0,
                height: rect.height,
            },
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            overflow: Overflow::Clip,
            padding: 0.0,
            font_size_override: Some(12.0),
            color_override: Some([190, 190, 205]),
            font_family_override: None,
        });
    }
}

fn render_menu_separator(quads: &mut Vec<QuadInstance>, x: f32, y: f32, width: f32) {
    quads.push(QuadInstance {
        rect: [x + 8.0, y, width - 16.0, 1.0],
        color: [0.42, 0.42, 0.50, 0.55],
        color_bottom: [0.42, 0.42, 0.50, 0.55],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn render_menu_scrollbar(quads: &mut Vec<QuadInstance>, rect: Rect, scroll: f32, max_scroll: f32) {
    let track_h = rect.height - 10.0;
    let thumb_h = (track_h * (rect.height / (rect.height + max_scroll))).clamp(24.0, track_h);
    let thumb_y = rect.y + 5.0 + (track_h - thumb_h) * (scroll / max_scroll.max(1.0));
    quads.push(QuadInstance {
        rect: [rect.x + rect.width - 6.0, thumb_y, 3.0, thumb_h],
        color: [0.70, 0.70, 0.78, 0.45],
        color_bottom: [0.70, 0.70, 0.78, 0.45],
        border_color: [0.0; 4],
        border_width: 0.0,
        border_radius: 0.0,
        shadow_offset: [0.0; 2],
        shadow_color: [0.0; 4],
        shadow_blur: 0.0,
        rotation: 0.0,
        _padding: [0.0; 2],
    });
}

fn handle_text_undo(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(note) = state.note_input.undo(&line.note) {
                return EventResponse::Action(UiAction::UpdateLineNote { line_id, note });
            }
        }
        return EventResponse::Consumed;
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(name) = state.char_input.undo(&line.character_name) {
                state.autocomplete_index = Some(0);
                return EventResponse::Action(UiAction::UpdateCharacterName { line_id, name });
            }
        }
        return EventResponse::Consumed;
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.undo(&line.text) {
                return EventResponse::Action(UiAction::UpdateLineText { id: line_id, text });
            }
        }
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

fn autocomplete_hover_index(ctx: &RythmoCtx, state: &RythmoState, x: f32, y: f32) -> Option<usize> {
    let line_id = state.editing_character?;
    let line = ctx.project.get_line(line_id)?;
    let suggestions = ctx.project.autocomplete(&line.character_name);
    if suggestions.is_empty() {
        return None;
    }

    let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
    let br = badge_rect_for_line(ctx.project, line, ctx.current_frame, ctx.zone);
    let dropdown_x = br.x;
    let dropdown_y = r.y + r.height + 2.0;
    let item_h = 20.0;
    let dropdown_w = 140.0;

    for (i, _) in suggestions.iter().enumerate() {
        let iy = dropdown_y + i as f32 * item_h;
        let item_rect = Rect {
            x: dropdown_x,
            y: iy,
            width: dropdown_w,
            height: item_h,
        };
        if item_rect.contains(x, y) {
            return Some(i);
        }
    }
    None
}

fn handle_mouse_move(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // Autocomplete hover tracking
    if state.editing_character.is_some() {
        let new_hover = autocomplete_hover_index(ctx, state, x, y);
        if new_hover != state.autocomplete_hover {
            state.autocomplete_hover = new_hover;
            // Also set keyboard index to match mouse for Enter to work
            if new_hover.is_some() {
                state.autocomplete_index = new_hover;
            }
            return EventResponse::Consumed;
        }
    }

    if let Some(drag) = &state.dragging {
        let dx_frames = ((x - drag.drag_start_x) / ppf()) as i64;
        return match &drag.target {
            DragTarget::Marker(idx) => {
                let new_frame = drag.original_frame + dx_frames;
                EventResponse::Action(UiAction::MoveMarker {
                    index: *idx,
                    frame: new_frame,
                })
            }
            DragTarget::Line(line_id) => {
                let line_id = *line_id;
                match drag.handle {
                    DragHandle::Left => {
                        let end = drag.original_frame + drag.original_duration;
                        let ns = (drag.original_frame + dx_frames).min(end - 1);
                        EventResponse::Action(UiAction::ResizeLine {
                            id: line_id,
                            start_frame: ns,
                            duration_frames: end - ns,
                        })
                    }
                    DragHandle::Right => EventResponse::Action(UiAction::ResizeLine {
                        id: line_id,
                        start_frame: drag.original_frame,
                        duration_frames: (drag.original_duration + dx_frames).max(1),
                    }),
                    DragHandle::Selection => {
                        if let Some(line) = ctx.project.get_line(line_id) {
                            let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
                            let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                            state.pending_cursor_click = Some((ratio, true));

                            let lang = crate::config::get().lang.clone();
                            let char_pos = cursor_index_for_line_at_ratio(
                                line,
                                state.syllable_drag.as_ref(),
                                &lang,
                                ctx.karaoke_preview,
                                state,
                                ratio,
                            );
                            state.line_input.update_selection(char_pos);
                        }
                        EventResponse::Consumed
                    }
                    DragHandle::Body => {
                        let candidate = y_to_slot(ctx.project, y, ctx.zone);
                        let new_y_slot = if candidate != drag.original_y_slot {
                            let layouts = editor_track_layouts(ctx.project, ctx.zone);
                            let orig_track_idx =
                                rythmo_layout::track_index_for_y_slot(drag.original_y_slot);
                            let orig_track =
                                rythmo_layout::track_for_index(&layouts, orig_track_idx)
                                    .unwrap_or_else(|| {
                                        layouts
                                            .first()
                                            .expect("editor track layout should not be empty")
                                    });
                            let orig_center = ctx.zone.y
                                + constants::RULER_HEIGHT
                                + orig_track.top
                                + orig_track.total_h / 2.0;
                            if (y - orig_center).abs() > orig_track.total_h * 0.6 {
                                candidate
                            } else {
                                drag.original_y_slot
                            }
                        } else {
                            drag.original_y_slot
                        };
                        if !drag.group_origins.is_empty()
                            && matches!(state.selected, Some(Selection::AllLines))
                        {
                            let y_delta = new_y_slot - drag.original_y_slot;
                            let moves = drag
                                .group_origins
                                .iter()
                                .map(|origin| {
                                    (
                                        origin.line_id,
                                        origin.original_frame + dx_frames,
                                        (origin.original_y_slot + y_delta).clamp(0.0, 0.75),
                                    )
                                })
                                .collect();
                            return EventResponse::Action(UiAction::MoveLines { moves });
                        }
                        EventResponse::Action(UiAction::MoveLine {
                            id: line_id,
                            start_frame: drag.original_frame + dx_frames,
                            y_slot: new_y_slot,
                        })
                    }
                }
            }
        };
    }

    // Ghost preview when CTRL held and hovering empty BR space
    if state.ctrl_held && ctx.zone.contains(x, y) {
        let on_line = ctx
            .project
            .lines()
            .any(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y));
        if !on_line {
            let frame = x_to_frame(x, ctx.current_frame, ctx.zone);
            let y_slot = y_to_slot(ctx.project, y, ctx.zone);
            state.ghost_preview = Some(GhostPreview {
                frame,
                y_slot,
                duration_frames: clamped_new_line_duration(ctx.project, frame, y_slot, ctx.fps),
            });
            return EventResponse::Consumed;
        }
    }
    // Clear ghost when not applicable
    if state.ghost_preview.is_some() {
        state.ghost_preview = None;
    }

    if !ctx.zone.contains(x, y) {
        let mut consumed = false;
        if state.hovered_line.take().is_some() {
            consumed = true;
        }
        if state.hovered_track.take().is_some() {
            consumed = true;
        }
        return if consumed {
            EventResponse::Consumed
        } else {
            EventResponse::Ignored
        };
    }

    let found = ctx
        .project
        .lines()
        .find(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y))
        .map(|l| l.id);

    let hovered_track = {
        let relative_y = y - ctx.zone.y - constants::RULER_HEIGHT;
        editor_track_layouts(ctx.project, ctx.zone)
            .iter()
            .find(|layout| relative_y >= layout.top && relative_y < layout.top + layout.total_h)
            .map(|layout| layout.track_index)
    };

    let mut changed = false;
    if found != state.hovered_line {
        state.hovered_line = found;
        changed = true;
    }
    if hovered_track != state.hovered_track {
        state.hovered_track = hovered_track;
        changed = true;
    }

    if changed {
        EventResponse::Consumed
    } else {
        EventResponse::Ignored
    }
}

fn handle_mouse_press(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // (autocomplete click already handled before color picker in handle_rythmo_event)

    // Click outside zone while editing → finalize
    if !ctx.zone.contains(x, y) {
        let char_id = state.editing_character;
        let was_editing_line = state.editing_line.is_some();
        let was_editing_note = state.editing_note.is_some();
        if char_id.is_some() {
            state.stop_char_editing();
        }
        if was_editing_line {
            state.stop_line_editing();
        }
        if was_editing_note {
            state.stop_note_editing();
        }
        if let Some(line_id) = char_id {
            return EventResponse::Action(UiAction::FinalizeCharacter { line_id });
        }
        return if was_editing_line {
            EventResponse::Action(UiAction::StopEditing)
        } else {
            EventResponse::Ignored
        };
    }

    // Check markers first (smaller hit targets, on top visually)
    let marker_hit_w = 12.0;
    for (i, marker) in ctx.project.markers.iter().enumerate() {
        let mx = frame_to_x(marker.frame, ctx.current_frame, ctx.zone);
        if (x - mx).abs() < marker_hit_w {
            state.selected = Some(Selection::Marker(i));
            state.dragging = Some(DragState {
                target: DragTarget::Marker(i),
                drag_start_x: x,
                original_frame: marker.frame,
                original_duration: 0,
                original_y_slot: 0.0,
                drag_start_y: y,
                handle: DragHandle::Body,
                group_origins: Vec::new(),
            });
            return EventResponse::Consumed;
        }
    }

    // Check lines
    for line in ctx.project.lines() {
        let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        if !r.contains(x, y) {
            continue;
        }

        // If editing this line, single click positions cursor instead of starting a generic drag
        // Only exceptions are the resize handles which should still resize the line
        let is_left_handle = x < r.x + constants::HANDLE_WIDTH;
        let is_right_handle = x > r.x + r.width - constants::HANDLE_WIDTH;
        let is_editing = state.editing_line == Some(line.id);

        if is_editing && !is_left_handle && !is_right_handle {
            if !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                state.pending_cursor_click = Some((ratio, false));

                let lang = crate::config::get().lang.clone();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    &lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.start_selection(char_pos);
            }
            // Add a special drag handle for mouse selection to allow mouse drag selection
            state.dragging = Some(DragState {
                target: DragTarget::Line(line.id),
                handle: DragHandle::Selection,
                drag_start_x: x,
                original_frame: line.start_frame,
                original_duration: line.duration_frames,
                original_y_slot: line.y_slot,
                drag_start_y: y,
                group_origins: Vec::new(),
            });
            return EventResponse::Consumed;
        }

        let handle = if is_left_handle {
            DragHandle::Left
        } else if is_right_handle {
            DragHandle::Right
        } else {
            DragHandle::Body
        };
        let group_origins =
            if handle == DragHandle::Body && matches!(state.selected, Some(Selection::AllLines)) {
                all_line_origins(ctx.project)
            } else {
                state.selected = Some(Selection::Line(line.id));
                Vec::new()
            };

        state.dragging = Some(DragState {
            target: DragTarget::Line(line.id),
            handle,
            drag_start_x: x,
            original_frame: line.start_frame,
            original_duration: line.duration_frames,
            original_y_slot: line.y_slot,
            drag_start_y: y,
            group_origins,
        });
        return EventResponse::Consumed;
    }

    // Click on empty space → deselect & stop editing
    state.selected = None;
    let char_id = state.editing_character;
    let was_editing_line = state.editing_line.is_some();
    let was_editing_note = state.editing_note.is_some();
    if char_id.is_some() {
        state.stop_char_editing();
    }
    if was_editing_line {
        state.stop_line_editing();
    }
    if was_editing_note {
        state.stop_note_editing();
    }
    if let Some(line_id) = char_id {
        return EventResponse::Action(UiAction::FinalizeCharacter { line_id });
    }
    if was_editing_line || was_editing_note {
        return EventResponse::Action(UiAction::StopEditing);
    }
    EventResponse::Ignored
}

fn all_line_origins(project: &Project) -> Vec<DragLineOrigin> {
    project
        .lines()
        .map(|line| DragLineOrigin {
            line_id: line.id,
            original_frame: line.start_frame,
            original_y_slot: line.y_slot,
        })
        .collect()
}

fn handle_mouse_release(state: &mut RythmoState) -> EventResponse {
    if state.dragging.take().is_some() {
        EventResponse::Consumed
    } else {
        EventResponse::Ignored
    }
}

fn handle_ctrl_click(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    if !ctx.zone.contains(x, y) {
        return EventResponse::Ignored;
    }
    state.stop_line_editing();
    state.stop_char_editing();
    state.stop_note_editing();
    EventResponse::Action(UiAction::CreateLine {
        frame: x_to_frame(x, ctx.current_frame, ctx.zone),
        y_slot: y_to_slot(ctx.project, y, ctx.zone),
    })
}

fn handle_shift_mouse_press(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> EventResponse {
    if !ctx.zone.contains(x, y) {
        return EventResponse::Ignored;
    }

    // Line text editing selection
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
            if r.contains(x, y) && !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                state.pending_cursor_click = Some((ratio, true));

                // If there's no selection, start one from current cursor
                if !state.line_input.has_selection() {
                    let current = state.line_input.cursor_pos;
                    state.line_input.selection = Some((current, current));
                }

                let lang = crate::config::get().lang.clone();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    &lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.update_selection(char_pos);

                return EventResponse::Consumed;
            }
        }
    }

    EventResponse::Ignored
}

fn handle_double_click(ctx: &RythmoCtx, state: &mut RythmoState, x: f32, y: f32) -> EventResponse {
    // Save current character edit before switching
    let finalize_line_id = state.editing_character;

    // Badge → character editing
    for line in ctx.project.lines() {
        let br = badge_rect_for_line(ctx.project, line, ctx.current_frame, ctx.zone);
        if br.contains(x, y) {
            if let Some(old_id) = finalize_line_id {
                if old_id != line.id {
                    state.stop_char_editing();
                    // Can't dispatch two actions, so finalize happens via FinalizeCharacter below
                }
            }
            state.editing_character = Some(line.id);
            state.char_input.activate(&line.character_name);
            state.char_input.select_all(&line.character_name);
            let (picker_x, picker_y) = color_picker_origin_for_badge(&br, ctx.zone);
            state
                .color_picker
                .open(picker_x, picker_y, line.character_color);
            state.stop_line_editing();
            state.stop_note_editing();
            return if let Some(old_id) = finalize_line_id.filter(|&id| id != line.id) {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Line body → note editing (if has note and click is in note area) or text editing
    for line in ctx.project.lines() {
        let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);
        if r.contains(x, y) {
            // If the line has a note and click is in the bottom part, edit note
            if !line.note.is_empty() {
                let note_label_h = 12.0;
                let note_y = r.y + r.height - note_label_h - 1.0;
                if y >= note_y {
                    state.stop_line_editing();
                    state.stop_char_editing();
                    return EventResponse::Action(UiAction::AddNote);
                }
            }
            // If already editing this line, select the clicked word.
            if state.editing_line == Some(line.id) && !line.text.is_empty() {
                let ratio = ((x - r.x) / r.width).clamp(0.0, 1.0);
                let lang = crate::config::get().lang.clone();
                let char_pos = cursor_index_for_line_at_ratio(
                    line,
                    state.syllable_drag.as_ref(),
                    &lang,
                    ctx.karaoke_preview,
                    state,
                    ratio,
                );
                state.line_input.select_word_at(&line.text, char_pos);
                return EventResponse::Consumed;
            }
            state.editing_line = Some(line.id);
            state.line_input.activate(&line.text);
            state.stop_char_editing();
            state.stop_note_editing();
            return if let Some(old_id) = finalize_line_id {
                EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id })
            } else {
                EventResponse::Consumed
            };
        }
    }
    // Click empty → stop editing
    if let Some(old_id) = finalize_line_id {
        state.stop_char_editing();
        return EventResponse::Action(UiAction::FinalizeCharacter { line_id: old_id });
    }
    if state.editing_line.is_some() {
        state.stop_line_editing();
        return EventResponse::Action(UiAction::StopEditing);
    }
    EventResponse::Ignored
}

fn handle_key_input(ctx: &RythmoCtx, state: &mut RythmoState, text: &str) -> EventResponse {
    use super::text_input::TextInputAction;

    // Note editing takes priority
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            match state.note_input.handle_key(text, &line.note) {
                Some(TextInputAction::Changed(new_note)) => {
                    return EventResponse::Action(UiAction::UpdateLineNote {
                        line_id,
                        note: new_note,
                    })
                }
                Some(TextInputAction::Finished) => {
                    state.stop_note_editing();
                    return EventResponse::Action(UiAction::StopEditing);
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }

    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            // Enter with autocomplete → confirm suggestion (default to first)
            if text == "\r" || text == "\n" {
                let suggestions = ctx.project.autocomplete(&line.character_name);
                if !suggestions.is_empty() {
                    let idx = state.autocomplete_index.unwrap_or(0);
                    if let Some(suggestion) = suggestions.get(idx) {
                        let name = suggestion.name.clone();
                        let color = suggestion.color;
                        state.stop_char_editing();
                        return EventResponse::Action(UiAction::SetCharacter {
                            line_id,
                            name,
                            color,
                        });
                    }
                }
            }

            match state.char_input.handle_key(text, &line.character_name) {
                Some(TextInputAction::Changed(name)) => {
                    state.autocomplete_index = Some(0); // default to first suggestion
                    let br =
                        badge_rect_for_name(ctx.project, line, &name, ctx.current_frame, ctx.zone);
                    let (picker_x, picker_y) = color_picker_origin_for_badge(&br, ctx.zone);
                    state.color_picker.move_to(picker_x, picker_y);
                    return EventResponse::Action(UiAction::UpdateCharacterName { line_id, name });
                }
                Some(TextInputAction::Finished) => {
                    let name = line.character_name.clone();
                    let color = state.color_picker.current_color();
                    state.stop_char_editing();
                    return if !name.is_empty() {
                        EventResponse::Action(UiAction::SetCharacter {
                            line_id,
                            name,
                            color,
                        })
                    } else {
                        EventResponse::Action(UiAction::StopEditing)
                    };
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }

    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            match state.line_input.handle_key(text, &line.text) {
                Some(TextInputAction::Changed(new_text)) => {
                    return EventResponse::Action(UiAction::UpdateLineText {
                        id: line_id,
                        text: new_text,
                    })
                }
                Some(TextInputAction::Finished) => {
                    state.stop_line_editing();
                    return EventResponse::Action(UiAction::StopEditing);
                }
                None => {}
            }
        }
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

fn handle_autocomplete_nav(ctx: &RythmoCtx, state: &mut RythmoState, dir: i32) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            let suggestions = ctx.project.autocomplete(&line.character_name);
            if suggestions.is_empty() {
                return EventResponse::Ignored;
            }

            let count = suggestions.len();
            let new_idx = match state.autocomplete_index {
                Some(idx) => {
                    let next = idx as i32 + dir;
                    if next < 0 {
                        None
                    } else {
                        Some((next as usize).min(count - 1))
                    }
                }
                None => {
                    if dir > 0 {
                        Some(0)
                    } else {
                        None
                    }
                }
            };
            state.autocomplete_index = new_idx;
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}

fn handle_select_all(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.char_input.select_all(&line.character_name);
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.line_input.select_all(&line.text);
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            state.note_input.select_all(&line.note);
            return EventResponse::Consumed;
        }
    }
    if ctx.project.lines().next().is_some() {
        state.selected = Some(Selection::AllLines);
        state.dragging = None;
        state.stop_line_editing();
        state.stop_char_editing();
        state.stop_note_editing();
        return EventResponse::Consumed;
    }
    EventResponse::Ignored
}

fn handle_copy(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.note_input.selected_text(&line.note) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.char_input.selected_text(&line.character_name) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.selected_text(&line.text) {
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    EventResponse::Consumed
}

fn handle_cut(ctx: &RythmoCtx, state: &mut RythmoState) -> EventResponse {
    let delete = "\x08";
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.note_input.selected_text(&line.note) {
                if let Some(super::text_input::TextInputAction::Changed(note)) =
                    state.note_input.handle_key(delete, &line.note)
                {
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateLineNote {
                        clipboard: text,
                        line_id,
                        note,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.char_input.selected_text(&line.character_name) {
                if let Some(super::text_input::TextInputAction::Changed(name)) =
                    state.char_input.handle_key(delete, &line.character_name)
                {
                    state.autocomplete_index = Some(0);
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateCharacterName {
                        clipboard: text,
                        line_id,
                        name,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if let Some(text) = state.line_input.selected_text(&line.text) {
                if let Some(super::text_input::TextInputAction::Changed(new_text)) =
                    state.line_input.handle_key(delete, &line.text)
                {
                    return EventResponse::Action(UiAction::SetClipboardAndUpdateLineText {
                        clipboard: text,
                        id: line_id,
                        text: new_text,
                    });
                }
                return EventResponse::Action(UiAction::SetClipboard(text));
            }
        }
    }
    EventResponse::Consumed
}

fn handle_cursor_move(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    dir: i32,
    shift: bool,
) -> EventResponse {
    if let Some(line_id) = state.editing_character {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.char_input.move_left_shift();
                } else {
                    state.char_input.move_left();
                }
            } else {
                if shift {
                    state.char_input.move_right_shift(&line.character_name);
                } else {
                    state.char_input.move_right(&line.character_name);
                }
            }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_line {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.line_input.move_left_shift();
                } else {
                    state.line_input.move_left();
                }
            } else {
                if shift {
                    state.line_input.move_right_shift(&line.text);
                } else {
                    state.line_input.move_right(&line.text);
                }
            }
            return EventResponse::Consumed;
        }
    }
    if let Some(line_id) = state.editing_note {
        if let Some(line) = ctx.project.get_line(line_id) {
            if dir < 0 {
                if shift {
                    state.note_input.move_left_shift();
                } else {
                    state.note_input.move_left();
                }
            } else {
                if shift {
                    state.note_input.move_right_shift(&line.note);
                } else {
                    state.note_input.move_right(&line.note);
                }
            }
            return EventResponse::Consumed;
        }
    }
    EventResponse::Ignored
}

// ── Syllable mode helpers ──────────────────────────────────────────────────

fn syllable_mouse_press(
    ctx: &RythmoCtx,
    state: &mut RythmoState,
    x: f32,
    y: f32,
) -> Option<EventResponse> {
    if !ctx.zone.contains(x, y) {
        return None;
    }

    // Find which line was clicked
    let line = ctx
        .project
        .lines()
        .find(|l| line_rect(ctx.project, l, ctx.current_frame, ctx.zone).contains(x, y))?;
    if ctx.karaoke_preview && line.karaoke {
        return None;
    }
    if state.hovered_line != Some(line.id) {
        return None;
    }

    let r = line_rect(ctx.project, line, ctx.current_frame, ctx.zone);

    let lang = crate::config::get().lang.clone();
    let ratios = syllable_ratios_for_line(line, state.syllable_drag.as_ref(), &lang)?;
    if ratios.len() <= 1 {
        return None;
    }

    // Find which separator is closest to click
    let mut sep_x = r.x;
    let hit_w = 7.0;
    let top_y = r.y + 1.0;
    if y < top_y - 6.0 || y > top_y + 14.0 {
        return None;
    }
    for (i, ratio) in ratios.iter().enumerate() {
        sep_x += ratio * r.width;
        if i < ratios.len() - 1 && (x - sep_x).abs() < hit_w {
            state.syllable_drag = Some(SyllableDrag {
                line_id: line.id,
                separator_index: i,
                ratios: ratios.clone(),
                drag_start_x: x,
                line_rect: r,
            });
            return Some(EventResponse::Consumed);
        }
    }
    None
}

fn syllable_mouse_move(state: &mut RythmoState, x: f32) -> Option<EventResponse> {
    let drag = state.syllable_drag.as_mut()?;

    let dx = x - drag.drag_start_x;
    let delta_ratio = dx / drag.line_rect.width;
    drag.drag_start_x = x;

    let i = drag.separator_index;
    let min_ratio = syllable_drag_min_ratio(drag.ratios.len(), drag.line_rect.width);
    if delta_ratio.abs() <= 0.0001 || i + 1 >= drag.ratios.len() {
        return Some(EventResponse::Consumed);
    }

    let left_end = i + 1;
    let right_start = i + 1;
    let left_total: f32 = drag.ratios[..left_end].iter().sum();
    let right_total: f32 = drag.ratios[right_start..].iter().sum();
    let left_min_total = min_ratio * left_end as f32;
    let right_min_total = min_ratio * (drag.ratios.len() - right_start) as f32;

    if delta_ratio > 0.0 {
        let applied = delta_ratio.min((right_total - right_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[..left_end],
                left_total + applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..],
                right_total - applied,
                min_ratio,
            );
        }
    } else {
        let applied = (-delta_ratio).min((left_total - left_min_total).max(0.0));
        if applied > 0.0 {
            redistribute_group_to_total(
                &mut drag.ratios[..left_end],
                left_total - applied,
                min_ratio,
            );
            redistribute_group_to_total(
                &mut drag.ratios[right_start..],
                right_total + applied,
                min_ratio,
            );
        }
    }

    normalize_ratios_in_place(&mut drag.ratios);

    Some(EventResponse::Consumed)
}

fn syllable_drag_min_ratio(segment_count: usize, line_width: f32) -> f32 {
    if segment_count == 0 || line_width <= 1.0 {
        return 0.001;
    }

    // Keep handles usable without reserving a large percentage of the line.
    // A fixed 5% minimum made separators feel blocked on lines with many syllables.
    let pixel_min = 3.0 / line_width.max(1.0);
    let total_budget_min = 0.35 / segment_count as f32;
    pixel_min
        .clamp(0.001, 0.02)
        .min(total_budget_min.max(0.001))
}

fn redistribute_group_to_total(ratios: &mut [f32], target_total: f32, min_ratio: f32) {
    if ratios.is_empty() {
        return;
    }

    let count = ratios.len() as f32;
    let min_total = min_ratio * count;
    let target_total = target_total.max(min_total);
    let target_free = (target_total - min_total).max(0.0);
    let free_sum: f32 = ratios
        .iter()
        .map(|ratio| (*ratio - min_ratio).max(0.0))
        .sum();

    if free_sum <= f32::EPSILON {
        let each = target_total / count;
        for ratio in ratios.iter_mut() {
            *ratio = each;
        }
        return;
    }

    for ratio in ratios.iter_mut() {
        let free = (*ratio - min_ratio).max(0.0);
        *ratio = min_ratio + free / free_sum * target_free;
    }
}

fn normalize_ratios_in_place(ratios: &mut [f32]) {
    let sum: f32 = ratios.iter().sum();
    if sum <= f32::EPSILON {
        return;
    }
    for ratio in ratios.iter_mut() {
        *ratio /= sum;
    }
}

fn syllable_mouse_release(state: &mut RythmoState) -> Option<EventResponse> {
    let drag = state.syllable_drag.take()?;
    Some(EventResponse::Action(UiAction::SetSyllableRatios {
        line_id: drag.line_id,
        ratios: drag.ratios,
    }))
}

// -- Studio Mode (export-style rythmo rendering) --

fn studio_reference_height_from_track_flags(used_tracks: &[bool], karaoke_tracks: &[bool]) -> f32 {
    let slot_header_h = 20.0_f32.max(ACTOR_ICON_SIZE);
    let track_indices = track_indices_from_usage(used_tracks);
    let layouts = build_track_layouts_from_karaoke_flags(
        &track_indices,
        karaoke_tracks,
        32.0,
        slot_header_h,
        4.0,
        1.0,
    );
    let content_h = 20.0 + rythmo_layout::total_tracks_height(&layouts);
    content_h.max(300.0)
}

/// Compute the fixed rythmo band height for studio preview.
pub fn studio_br_height(_project: &Project, _width: f32) -> f32 {
    // Studio preview lives inside the UI, so keep the panel stable and scale the BR inside it.
    300.0
}

/// Export-style rythmo: ticks, playhead, lines with badges, markers. No waveform, no handles.
pub fn render_studio_rythmo<'a>(
    zone: &Rect,
    project: &'a Project,
    render_index: &ProjectRenderIndex,
    current_frame: f64,
    fps: f64,
    rythmo_state: &RythmoState,
    quads: &mut Vec<QuadInstance>,
    labels: &mut Vec<LabelInfo<'a>>,
    stretched: &mut Vec<StretchedText>,
    actor_icons: &mut Vec<VoiceActorIconDraw>,
) {
    rythmo_state.prune_karaoke_text_width_cache(project);
    let karaoke_max_gap_frames = karaoke_adjacent_max_gap_frames(fps);
    let karaoke_index = rythmo_state.cached_karaoke_ui_index(project, karaoke_max_gap_frames);
    // Studio mode: render with proportions scaled to the same height chosen above.
    let scale = zone.height
        / studio_reference_height_from_track_flags(
            karaoke_index.used_tracks(),
            karaoke_index.karaoke_tracks(),
        );

    // Readable sizes (increase text)
    let ruler_h = 20.0 * scale;
    let normal_slot_h = 32.0 * scale;
    let badge_h = 20.0 * scale;
    let badge_gap = 4.0 * scale;
    let actor_icon_size = ACTOR_ICON_SIZE * scale;
    let slot_header_h = badge_h.max(actor_icon_size);
    let badge_char_w = 8.0 * scale;
    let badge_font_size = 16.0 * scale; // increased from 13.0
    let badge_padding = 4.0 * scale;
    let badge_min_w = 14.0 * scale;

    // PPF: same as editor mode (not dependent on zone width)
    let ppf = constants::PIXELS_PER_FRAME * crate::config::scroll_speed();
    let track_indices = track_indices_from_usage(karaoke_index.used_tracks());
    let track_layouts = build_track_layouts_from_karaoke_flags(
        &track_indices,
        karaoke_index.karaoke_tracks(),
        normal_slot_h,
        slot_header_h,
        badge_gap,
        scale,
    );
    let tick_long = 10.0 * scale;
    let tick_short = 5.0 * scale;
    let tick_w = 1.0 * scale;
    let playhead_w = 2.0 * scale;
    let center_x = zone.x + zone.width / 2.0;
    let karaoke_count_in_frame_count = karaoke_count_in_frames(fps);

    // Ruler ticks (alternating long/short — export style)
    let visible_frames = (zone.width / ppf) as i64 + 4;
    let half_visible_frames = visible_frames as f64 / 2.0;
    let first_tick_frame = f64_floor_to_i64(current_frame - half_visible_frames);
    let first_tick = (first_tick_frame / constants::TICK_GAP_FRAMES) * constants::TICK_GAP_FRAMES;
    let mut tf = first_tick;
    loop {
        let x = center_x + (tf as f64 - current_frame) as f32 * ppf;
        if x > zone.x + zone.width {
            break;
        }
        if x >= zone.x {
            let tick_idx = tf / constants::TICK_GAP_FRAMES;
            let th = if tick_idx % 2 == 0 {
                tick_long
            } else {
                tick_short
            };
            let c = [100.0 / 255.0, 100.0 / 255.0, 115.0 / 255.0, 128.0 / 255.0];
            quads.push(QuadInstance {
                rect: [x, zone.y, tick_w, th],
                color: c,
                color_bottom: c,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });
        }
        tf += constants::TICK_GAP_FRAMES;
    }

    let first_active_frame = f64_floor_to_i64(current_frame);
    let last_active_frame = f64_ceil_to_i64(current_frame);
    let studio_skip_ranges: Vec<(f32, f32)> = render_index
        .visible_line_ids(project, first_active_frame, last_active_frame)
        .into_iter()
        .filter_map(|line_id| project.get_line(line_id))
        .filter(|line| line.karaoke && line.karaoke_active(current_frame))
        .filter_map(|line| {
            let track = rythmo_layout::track_for_y_slot(&track_layouts, line.y_slot)?;
            let body_y = zone.y + ruler_h + track.top + slot_header_h + badge_gap;
            let body_rect = karaoke_stack_rect(
                Rect {
                    x: 0.0,
                    y: body_y,
                    width: 0.0,
                    height: track.body_h,
                },
                karaoke_index.stack_row(line),
                scale,
            );
            Some((body_rect.y, body_rect.y + body_rect.height))
        })
        .collect();

    // Playhead, split around active karaoke lines.
    let ph_c = [217.0 / 255.0, 38.0 / 255.0, 38.0 / 255.0, 1.0];
    push_playhead_segments(
        quads,
        center_x - playhead_w / 2.0,
        playhead_w,
        zone.y,
        zone.height,
        ph_c,
        [0.0; 4],
        0.0,
        &studio_skip_ranges,
    );

    // Lines (export style: no handles, no borders, no hover effects)
    let karaoke_lang = crate::config::get().lang.clone();
    let margin_frames = interactive_render_margin_frames(fps, render_index);
    let (first_frame, last_frame) = render_window(zone, current_frame, margin_frames);
    let mut visible_line_ids = render_index.visible_line_ids(project, first_frame, last_frame);
    visible_line_ids.sort_by_key(|id| project.line_index(*id).unwrap_or(usize::MAX));

    for line_id in visible_line_ids {
        let Some(line) = project.get_line(line_id) else {
            continue;
        };
        let karaoke_active = line.karaoke_active(current_frame);
        let karaoke_count_in =
            karaoke_count_in_visible(line, current_frame, karaoke_count_in_frame_count);
        let karaoke_prestart_count_in = karaoke_index.prestart_scroll_visible(
            line,
            current_frame,
            karaoke_count_in_frame_count,
        );
        let karaoke_upcoming_stack = karaoke_index.upcoming_stack_visible(line, current_frame);
        if line.karaoke && !karaoke_active && !karaoke_prestart_count_in && !karaoke_upcoming_stack
        {
            continue;
        }

        let (x1, lw) = if line.karaoke
            && (karaoke_active || karaoke_prestart_count_in || karaoke_upcoming_stack)
        {
            let width = rythmo_state.karaoke_ui_text_width_for_render(line);
            (center_x - width / 2.0, width)
        } else {
            line.visual_x_width(current_frame, center_x, ppf, zone.width, scale)
        };
        if x1 + lw < zone.x || x1 > zone.x + zone.width {
            continue;
        }

        let Some(track) = rythmo_layout::track_for_y_slot(&track_layouts, line.y_slot) else {
            continue;
        };
        let y_base = zone.y + ruler_h + track.top;
        let body_y = y_base + slot_header_h + badge_gap;
        let mut line_y = body_y;
        let mut body_h = normal_slot_h;
        if line.karaoke {
            let stacked_rect = karaoke_stack_rect(
                Rect {
                    x: x1,
                    y: body_y,
                    width: lw,
                    height: track.body_h,
                },
                karaoke_index.stack_row(line),
                scale,
            );
            line_y = stacked_rect.y;
            body_h = stacked_rect.height;
        }
        let [cr, cg, cb, _] = line.character_color;
        let badge_w = (line.character_name.chars().count().max(1) as f32 * badge_char_w
            + badge_padding * 2.0)
            .max(badge_min_w);
        let badge_x = if line.karaoke {
            x1 - badge_w - constants::KARAOKE_NEXT_PREVIEW_GAP * 0.5 * scale
        } else {
            x1
        };
        let badge_y = if line.karaoke {
            line_y + (body_h - badge_h) * 0.5
        } else {
            y_base + ((slot_header_h - badge_h) * 0.5).max(0.0)
        };
        let show_badge = !line.karaoke || karaoke_index.character_label_visible(line);
        if show_badge {
            let bc = [cr, cg, cb, 1.0];
            quads.push(QuadInstance {
                rect: [badge_x, badge_y, badge_w, badge_h],
                color: bc,
                color_bottom: bc,
                border_color: [0.0; 4],
                border_width: 0.0,
                border_radius: 0.0,
                shadow_offset: [0.0; 2],
                shadow_color: [0.0; 4],
                shadow_blur: 0.0,
                rotation: 0.0,
                _padding: [0.0; 2],
            });

            // Badge text
            if !line.character_name.is_empty() {
                let luminance = 0.299 * cr + 0.587 * cg + 0.114 * cb;
                let text_color = if luminance > 0.55 {
                    Some([0, 0, 0])
                } else {
                    Some([224, 224, 230])
                };
                labels.push(LabelInfo {
                    text: &line.character_name,
                    bounds: Rect {
                        x: badge_x,
                        y: badge_y,
                        width: badge_w,
                        height: badge_h,
                    },
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: badge_padding,
                    font_size_override: Some(badge_font_size),
                    color_override: text_color,
                    font_family_override: None,
                });
            }

            render_voice_actor_icons_for_line(
                line,
                project,
                zone,
                Rect {
                    x: badge_x,
                    y: badge_y,
                    width: badge_w,
                    height: badge_h,
                },
                actor_icon_size,
                quads,
                labels,
                actor_icons,
            );
        }

        let body_rect = Rect {
            x: x1,
            y: line_y,
            width: lw,
            height: body_h,
        };
        let karaoke_progress_info = if line.karaoke {
            karaoke_progress_render_info(line, current_frame, &karaoke_lang)
        } else {
            None
        };

        // Stretched text or breath arrows
        if !line.text.is_empty() && line.text != "\u{2191}" && line.text != "\u{2193}" {
            if line.karaoke {
                push_karaoke_rythmo_text(stretched, line, body_rect, karaoke_progress_info);
            } else {
                let breaks = crate::syllable::syllable_breaks(&line.text, &karaoke_lang);
                let ratios = if line.syllable_ratios.len() == breaks.len() + 1 {
                    line.syllable_ratios.clone()
                } else {
                    Vec::new()
                };
                if !ratios.is_empty() {
                    let chars: Vec<char> = line.text.chars().collect();
                    let mut seg_x = x1;
                    let mut prev_break = 0usize;
                    for (i, &ratio) in ratios.iter().enumerate() {
                        let seg_w = ratio * lw;
                        let end_break = if i < breaks.len() {
                            breaks[i]
                        } else {
                            chars.len()
                        };
                        let segment: String = chars[prev_break..end_break].iter().collect();
                        if !segment.is_empty() && seg_w > 1.0 {
                            push_plain_rythmo_text(
                                stretched,
                                syllable_segment_cache_id(line.id, i),
                                segment,
                                Rect {
                                    x: seg_x,
                                    y: line_y,
                                    width: seg_w,
                                    height: body_h,
                                },
                            );
                        }
                        seg_x += seg_w;
                        prev_break = end_break;
                    }
                } else {
                    push_plain_rythmo_text(
                        stretched,
                        line.id,
                        line.text.clone(),
                        Rect {
                            x: x1,
                            y: line_y,
                            width: lw,
                            height: body_h,
                        },
                    );
                }
            }
        }

        // Breath arrows
        if line.text == "\u{2191}" || line.text == "\u{2193}" {
            let up = line.text == "\u{2191}";
            let r = Rect {
                x: x1,
                y: line_y,
                width: lw,
                height: body_h,
            };
            render_breath_arrow(&r, up, quads);
        }

        if karaoke_count_in {
            render_karaoke_count_in_dot_scaled(
                line,
                current_frame,
                &body_rect,
                karaoke_count_in_frame_count,
                scale,
                quads,
            );
        } else {
            render_karaoke_dot_scaled(line, &body_rect, karaoke_progress_info, scale, quads);
        }

        // Note text in studio mode
        if !line.note.is_empty() {
            let note_label_h = 10.0 * scale;
            let note_y = line_y + body_h - note_label_h - 1.0;
            labels.push(LabelInfo {
                text: &line.note,
                bounds: Rect {
                    x: x1 + 4.0 * scale,
                    y: note_y,
                    width: lw - 8.0 * scale,
                    height: note_label_h,
                },
                h_align: HAlign::Left,
                v_align: VAlign::Center,
                overflow: Overflow::Ellipsis,
                padding: 0.0,
                font_size_override: Some(8.0 * scale),
                color_override: Some([160, 160, 170]),
                font_family_override: None,
            });
        }
    }

    push_studio_karaoke_texture_prewarm_texts(
        stretched,
        rythmo_state,
        project,
        &karaoke_index,
        &track_layouts,
        visual_frame_to_i64(current_frame),
        fps,
        zone,
        ruler_h,
        slot_header_h,
        badge_gap,
        scale,
    );

    // Markers (export-style: use center_x + frame offset with studio ppf)
    let marker_margin_frames = f64_ceil_to_i64(20.0 / ppf.max(0.001) as f64).saturating_add(1);
    let (first_marker_frame, last_marker_frame) =
        render_window(zone, current_frame, marker_margin_frames);
    for marker_index in render_index.visible_marker_indices(first_marker_frame, last_marker_frame) {
        let Some(marker) = project.markers.get(marker_index) else {
            continue;
        };
        let marker_x = center_x + (marker.frame as f64 - current_frame) as f32 * ppf;
        if marker_x < zone.x - 20.0 || marker_x > zone.x + zone.width + 20.0 {
            continue;
        }

        match &marker.kind {
            MarkerKind::Boucle => {
                let red = [0.85, 0.15, 0.15, 0.9];
                // Red vertical bar
                quads.push(QuadInstance {
                    rect: [marker_x - 1.0, zone.y, 2.0, zone.height],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Big "X" — two smooth rotated bars
                let cy = zone.y + zone.height / 2.0;
                let arm_len = 20.0;
                let thickness = 2.5;
                let pi4 = std::f32::consts::FRAC_PI_4;
                // "\" bar
                quads.push(QuadInstance {
                    rect: [
                        marker_x - arm_len / 2.0,
                        cy - thickness / 2.0,
                        arm_len,
                        thickness,
                    ],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: pi4,
                    _padding: [0.0; 2],
                });
                // "/" bar
                quads.push(QuadInstance {
                    rect: [
                        marker_x - arm_len / 2.0,
                        cy - thickness / 2.0,
                        arm_len,
                        thickness,
                    ],
                    color: red,
                    color_bottom: red,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: -pi4,
                    _padding: [0.0; 2],
                });
            }
            MarkerKind::Out => {
                let col = [0.85, 0.45, 0.45, 0.7];
                // Light red vertical bar
                quads.push(QuadInstance {
                    rect: [marker_x - 1.0, zone.y, 2.0, zone.height],
                    color: col,
                    color_bottom: col,
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
                // Two parallel oblique bars crossing the vertical bar
                let cy = zone.y + zone.height / 2.0;
                let bar_len = zone.height * 0.25;
                let thickness = 2.0;
                let angle = 0.5;
                for offset in &[-5.0_f32, 5.0] {
                    quads.push(QuadInstance {
                        rect: [
                            marker_x + offset - bar_len / 2.0,
                            cy - thickness / 2.0,
                            bar_len,
                            thickness,
                        ],
                        color: col,
                        color_bottom: col,
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        border_radius: 0.0,
                        shadow_offset: [0.0; 2],
                        shadow_color: [0.0; 4],
                        shadow_blur: 0.0,
                        rotation: angle,
                        _padding: [0.0; 2],
                    });
                }
                // "out" text
                labels.push(LabelInfo {
                    text: "out",
                    bounds: Rect {
                        x: marker_x + 12.0,
                        y: cy - 8.0,
                        width: 30.0,
                        height: 16.0,
                    },
                    h_align: HAlign::Center,
                    v_align: VAlign::Center,
                    overflow: Overflow::Clip,
                    padding: 0.0,
                    font_size_override: Some(10.0),
                    color_override: Some([220, 120, 120]),
                    font_family_override: None,
                });
            }
            MarkerKind::SceneChange => {
                // White bar
                quads.push(QuadInstance {
                    rect: [marker_x - 1.0, zone.y, 2.0, zone.height],
                    color: [0.9, 0.9, 0.95, 0.8],
                    color_bottom: [0.9, 0.9, 0.95, 0.8],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    border_radius: 0.0,
                    shadow_offset: [0.0; 2],
                    shadow_color: [0.0; 4],
                    shadow_blur: 0.0,
                    rotation: 0.0,
                    _padding: [0.0; 2],
                });
            }
            _ => {} // LiaisonLeft/Right not rendered in studio mode
        }
    }
}
